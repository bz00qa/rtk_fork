pub mod provider;
pub mod registry;
mod report;
pub mod rules;

use anyhow::Result;
use std::collections::HashMap;

use provider::{ClaudeProvider, SessionProvider};
use registry::{
    category_avg_tokens, classify_command, has_rtk_disabled_prefix, split_command_chain,
    strip_disabled_prefix, Classification,
};
use report::{DiscoverReport, PatternOpportunity, SupportedEntry, TokenConsumer, UnsupportedEntry};

/// Aggregation bucket for supported commands.
struct SupportedBucket {
    rtk_equivalent: &'static str,
    category: &'static str,
    count: usize,
    total_output_tokens: usize,
    savings_pct: f64,
    // For display: the most common raw command
    command_counts: HashMap<String, usize>,
}

/// Aggregation bucket for unsupported commands.
struct UnsupportedBucket {
    count: usize,
    example: String,
}

pub fn run(
    project: Option<&str>,
    all: bool,
    since_days: u64,
    limit: usize,
    format: &str,
    verbose: u8,
) -> Result<()> {
    let provider = ClaudeProvider;

    // Determine project filter
    let project_filter = if all {
        None
    } else if let Some(p) = project {
        Some(p.to_string())
    } else {
        // Default: current working directory
        let cwd = std::env::current_dir()?;
        let cwd_str = cwd.to_string_lossy().to_string();
        let encoded = ClaudeProvider::encode_project_path(&cwd_str);
        Some(encoded)
    };

    let sessions = provider.discover_sessions(project_filter.as_deref(), Some(since_days))?;

    if verbose > 0 {
        eprintln!("Scanning {} session files...", sessions.len());
        for s in &sessions {
            eprintln!("  {}", s.display());
        }
    }

    let mut total_commands: usize = 0;
    let mut already_rtk: usize = 0;
    let mut parse_errors: usize = 0;
    let mut rtk_disabled_count: usize = 0;
    let mut rtk_disabled_cmds: HashMap<String, usize> = HashMap::new();
    let mut supported_map: HashMap<&'static str, SupportedBucket> = HashMap::new();
    let mut unsupported_map: HashMap<String, UnsupportedBucket> = HashMap::new();
    // Track all commands by base (first 2 words) for top token consumers
    let mut consumer_map: HashMap<String, (usize, usize)> = HashMap::new(); // base -> (count, total_output_bytes)

    for session_path in &sessions {
        let extracted = match provider.extract_commands(session_path) {
            Ok(cmds) => cmds,
            Err(e) => {
                if verbose > 0 {
                    eprintln!("Warning: skipping {}: {}", session_path.display(), e);
                }
                parse_errors += 1;
                continue;
            }
        };

        for ext_cmd in &extracted {
            let parts = split_command_chain(&ext_cmd.command);

            // Find the last non-ignored part — that's the command that actually
            // produced the output. In chains like `PATH=... && cd dir && npm build`,
            // only `npm build` should get output_len attributed.
            let effective_idx = parts
                .iter()
                .rposition(|p| !matches!(classify_command(p), Classification::Ignored));

            for (idx, part) in parts.iter().enumerate() {
                total_commands += 1;

                // Accumulate for top token consumers
                {
                    let base = consumer_base(part);
                    if !base.is_empty() {
                        let entry = consumer_map.entry(base).or_insert((0, 0));
                        entry.0 += 1;
                        // Only attribute output to the effective command
                        if Some(idx) == effective_idx {
                            entry.1 += ext_cmd.output_len.unwrap_or(0);
                        }
                    }
                }

                // Detect RTK_DISABLED= bypass before classification
                if has_rtk_disabled_prefix(part) {
                    let actual_cmd = strip_disabled_prefix(part);
                    // Only count if the underlying command is one RTK supports
                    match classify_command(actual_cmd) {
                        Classification::Supported { .. } => {
                            rtk_disabled_count += 1;
                            let display = truncate_command(actual_cmd);
                            *rtk_disabled_cmds.entry(display).or_insert(0) += 1;
                        }
                        _ => {
                            // RTK_DISABLED on unsupported/ignored command — not interesting
                        }
                    }
                    continue;
                }

                match classify_command(part) {
                    Classification::Supported {
                        rtk_equivalent,
                        category,
                        estimated_savings_pct,
                        status,
                    } => {
                        let bucket = supported_map.entry(rtk_equivalent).or_insert_with(|| {
                            SupportedBucket {
                                rtk_equivalent,
                                category,
                                count: 0,
                                total_output_tokens: 0,
                                savings_pct: estimated_savings_pct,
                                command_counts: HashMap::new(),
                            }
                        });

                        bucket.count += 1;

                        // Estimate tokens for this command
                        let output_tokens = if Some(idx) == effective_idx {
                            if let Some(len) = ext_cmd.output_len {
                                // Real: from tool_result content length
                                len / 4
                            } else {
                                // Fallback: category average
                                let subcmd = extract_subcmd(part);
                                category_avg_tokens(category, subcmd)
                            }
                        } else {
                            // Not the effective command — use category average
                            let subcmd = extract_subcmd(part);
                            category_avg_tokens(category, subcmd)
                        };

                        let savings =
                            (output_tokens as f64 * estimated_savings_pct / 100.0) as usize;
                        bucket.total_output_tokens += savings;

                        // Track the display name with status
                        let display_name = truncate_command(part);
                        let entry = bucket
                            .command_counts
                            .entry(format!("{}:{:?}", display_name, status))
                            .or_insert(0);
                        *entry += 1;
                    }
                    Classification::Unsupported { base_command } => {
                        let bucket = unsupported_map.entry(base_command).or_insert_with(|| {
                            UnsupportedBucket {
                                count: 0,
                                example: part.to_string(),
                            }
                        });
                        bucket.count += 1;
                    }
                    Classification::Ignored => {
                        // Check if it starts with "rtk "
                        if part.trim().starts_with("rtk ") {
                            already_rtk += 1;
                        }
                        // Otherwise just skip
                    }
                }
            }
        }
    }

    // Detect patterns across sessions
    let patterns = detect_patterns(&sessions, &provider, verbose);

    // Build report
    let mut supported: Vec<SupportedEntry> = supported_map
        .into_values()
        .map(|bucket| {
            // Pick the most common command as the display name
            let (command_with_status, status) = bucket
                .command_counts
                .into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(name, _)| {
                    // Extract status from "command:Status" format
                    if let Some(colon_pos) = name.rfind(':') {
                        let cmd = name[..colon_pos].to_string();
                        let status_str = &name[colon_pos + 1..];
                        let status = match status_str {
                            "Passthrough" => report::RtkStatus::Passthrough,
                            "NotSupported" => report::RtkStatus::NotSupported,
                            _ => report::RtkStatus::Existing,
                        };
                        (cmd, status)
                    } else {
                        (name, report::RtkStatus::Existing)
                    }
                })
                .unwrap_or_else(|| (String::new(), report::RtkStatus::Existing));

            SupportedEntry {
                command: command_with_status,
                count: bucket.count,
                rtk_equivalent: bucket.rtk_equivalent,
                category: bucket.category,
                estimated_savings_tokens: bucket.total_output_tokens,
                estimated_savings_pct: bucket.savings_pct,
                rtk_status: status,
            }
        })
        .collect();

    // Sort by estimated savings descending
    supported.sort_by(|a, b| b.estimated_savings_tokens.cmp(&a.estimated_savings_tokens));

    let mut unsupported: Vec<UnsupportedEntry> = unsupported_map
        .into_iter()
        .map(|(base, bucket)| UnsupportedEntry {
            base_command: base,
            count: bucket.count,
            example: bucket.example,
        })
        .collect();

    // Sort by count descending
    unsupported.sort_by(|a, b| b.count.cmp(&a.count));

    // Build top token consumers
    let mut consumers: Vec<TokenConsumer> = consumer_map
        .into_iter()
        .map(|(base, (count, total_bytes))| {
            let total_tokens = total_bytes / 4;
            let avg_tokens = if count > 0 { total_tokens / count } else { 0 };
            // classify_command regexes require args (e.g. "find\s+"), so bare
            // command names like "find" or "cat" won't match. Try with a dummy arg.
            let has_rtk_filter =
                matches!(classify_command(&base), Classification::Supported { .. })
                    || matches!(
                        classify_command(&format!("{} .", &base)),
                        Classification::Supported { .. }
                    );
            TokenConsumer {
                command: base,
                count,
                total_tokens,
                avg_tokens,
                has_rtk_filter,
            }
        })
        .collect();
    consumers.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
    consumers.truncate(15);

    // Build RTK_DISABLED examples sorted by frequency (top 5)
    let rtk_disabled_examples: Vec<String> = {
        let mut sorted: Vec<_> = rtk_disabled_cmds.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        sorted
            .into_iter()
            .take(5)
            .map(|(cmd, count)| format!("{} ({}x)", cmd, count))
            .collect()
    };

    let report = DiscoverReport {
        sessions_scanned: sessions.len(),
        total_commands,
        already_rtk,
        since_days,
        supported,
        unsupported,
        patterns,
        consumers,
        parse_errors,
        rtk_disabled_count,
        rtk_disabled_examples,
    };

    match format {
        "json" => println!("{}", report::format_json(&report)),
        _ => print!("{}", report::format_text(&report, limit, verbose > 0)),
    }

    Ok(())
}

/// Detect usage patterns that could benefit from RTK meta-commands.
///
/// Patterns detected:
/// 1. Sequential git status+diff+log → rtk context
/// 2. Same command repeated 3+ times → rtk watch
/// 3. Commands with large output (>2K chars) → rtk dedup
fn detect_patterns(
    sessions: &[std::path::PathBuf],
    provider: &ClaudeProvider,
    verbose: u8,
) -> Vec<PatternOpportunity> {
    let mut context_count = 0usize;
    let mut watch_candidates: HashMap<String, usize> = HashMap::new();
    let mut dedup_candidates: HashMap<String, (usize, usize)> = HashMap::new(); // cmd -> (count, total_output)

    for session_path in sessions {
        let extracted = match provider.extract_commands(session_path) {
            Ok(cmds) => cmds,
            Err(_) => continue,
        };

        // Sort by sequence index
        let mut cmds = extracted;
        cmds.sort_by_key(|c| c.sequence_index);

        // Detect context pattern: git status near git diff near git log
        let git_cmds: Vec<&str> = cmds
            .iter()
            .filter_map(|c| {
                let t = c.command.trim();
                if t.starts_with("git status")
                    || t.starts_with("rtk git status")
                    || t.starts_with("git diff")
                    || t.starts_with("rtk git diff")
                    || t.starts_with("git log")
                    || t.starts_with("rtk git log")
                {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();

        // Count windows of 3 where we see status+diff+log
        for window in git_cmds.windows(3) {
            let has_status = window.iter().any(|c| c.contains("status"));
            let has_diff = window.iter().any(|c| c.contains("diff"));
            let has_log = window.iter().any(|c| c.contains("log"));
            if has_status && has_diff && has_log {
                context_count += 1;
            }
        }

        // Detect watch pattern: same command base repeated
        let mut cmd_runs: HashMap<String, usize> = HashMap::new();
        for cmd in &cmds {
            if let Some(base) = normalize_cmd_base(&cmd.command) {
                *cmd_runs.entry(base).or_insert(0) += 1;
            }
        }
        for (base, count) in cmd_runs {
            if count >= 3 {
                *watch_candidates.entry(base).or_insert(0) += count;
            }
        }

        // Detect dedup pattern: commands with large output
        for cmd in &cmds {
            if let Some(len) = cmd.output_len {
                if len > 2000 {
                    if let Some(base) = normalize_cmd_base(&cmd.command) {
                        let entry = dedup_candidates.entry(base).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += len;
                    }
                }
            }
        }
    }

    let mut patterns = Vec::new();

    if context_count > 0 {
        // Estimate: each context pattern saves ~3 round-trips × ~200 tokens overhead
        patterns.push(PatternOpportunity {
            pattern: "git status + diff + log sequence".to_string(),
            suggestion: "rtk context".to_string(),
            occurrences: context_count,
            estimated_savings_tokens: context_count * 600,
        });
    }

    // Top watch candidates
    let mut watch_vec: Vec<_> = watch_candidates.into_iter().collect();
    watch_vec.sort_by(|a, b| b.1.cmp(&a.1));
    for (cmd, count) in watch_vec.into_iter().take(5) {
        // Estimate: repeated runs with identical output → 90% savings on 2nd+ runs
        let est_savings = (count - count / 3) * 150; // ~150 tokens per avoided repeat
        patterns.push(PatternOpportunity {
            pattern: format!("{} repeated", cmd),
            suggestion: format!("rtk watch {}", cmd),
            occurrences: count,
            estimated_savings_tokens: est_savings,
        });
    }

    // Top dedup candidates
    let mut dedup_vec: Vec<_> = dedup_candidates.into_iter().collect();
    dedup_vec.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
    for (cmd, (count, total_bytes)) in dedup_vec.into_iter().take(5) {
        if count >= 2 {
            // Estimate: dedup saves ~30% of large outputs
            let est_savings = total_bytes / 4 * 30 / 100; // bytes→tokens × 30%
            patterns.push(PatternOpportunity {
                pattern: format!("{} (large output)", cmd),
                suggestion: format!("rtk dedup {}", cmd),
                occurrences: count,
                estimated_savings_tokens: est_savings,
            });
        }
    }

    if verbose > 0 && !patterns.is_empty() {
        eprintln!("Detected {} usage patterns", patterns.len());
    }

    patterns
}

/// Commands not meaningful for watch/dedup pattern detection
const PATTERN_SKIP_PREFIXES: &[&str] = &[
    "cd ", "cd\t", "ls", "echo ", "cat ", "pwd", "mkdir ", "rm ", "cp ", "mv ", "touch ", "chmod ",
    "export ", "source ", ".", "PATH=", "SKIP_ENV", "set ", "unset ", "head ", "tail ", "wc ",
    "which ", "where ", "type ",
];

/// Normalize a command to its base form for comparison.
/// "cargo test -- --nocapture" → "cargo test"
/// "rtk cargo test" → "cargo test"
/// Returns None for commands not meaningful for pattern detection.
fn normalize_cmd_base(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();

    // Skip non-meaningful commands
    for prefix in PATTERN_SKIP_PREFIXES {
        if trimmed.starts_with(prefix) {
            return None;
        }
    }

    // Skip pure env assignments
    if trimmed.contains('=') && !trimmed.contains(' ') {
        return None;
    }

    let stripped = trimmed.strip_prefix("rtk ").unwrap_or(trimmed);
    let parts: Vec<&str> = stripped.splitn(3, char::is_whitespace).collect();
    match parts.len() {
        0 => None,
        1 => Some(parts[0].to_string()),
        _ => Some(format!("{} {}", parts[0], parts[1])),
    }
}

/// Extract a smart base key for top-token-consumer grouping.
///
/// Handles special patterns:
/// - `python -m pytest ...` → `python -m pytest` (3 words, not 2)
/// - `python3 -m mypy ...`  → `python3 -m mypy`
/// - `pnpm --filter X cmd`  → `pnpm cmd` (skip --filter + its arg)
/// - `pnpm -r build`        → `pnpm build` (skip -r flag)
/// - `npm run build`        → `npm run` (standard 2-word)
/// - `go test ./...`        → `go test` (standard 2-word)
fn consumer_base(cmd: &str) -> String {
    let words: Vec<&str> = cmd.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    // python/python3 -m <module> → keep 3 words
    if (words[0] == "python" || words[0] == "python3") && words.len() >= 3 && words[1] == "-m" {
        return format!("{} -m {}", words[0], words[2]);
    }

    // pnpm with flags before the subcommand: skip --filter/F + arg, -r/-w/etc
    if words[0] == "pnpm" && words.len() >= 2 {
        let mut i = 1;
        while i < words.len() {
            if (words[i] == "--filter" || words[i] == "-F") && i + 1 < words.len() {
                i += 2; // skip flag + its argument
            } else if words[i].starts_with("--filter=") || words[i].starts_with("-F") {
                i += 1; // skip combined flag=value
            } else if words[i] == "-r"
                || words[i] == "--recursive"
                || words[i] == "-w"
                || words[i] == "--workspace-root"
            {
                i += 1; // skip standalone flags
            } else {
                break;
            }
        }
        if i < words.len() {
            return format!("pnpm {}", words[i]);
        }
        return "pnpm".to_string();
    }

    // Commands where the second word is a path/file argument, not a subcommand.
    // Group by command name only (optionally with flags).
    const ARG_COMMANDS: &[&str] = &[
        "cat", "head", "tail", "wc", "less", "more", "touch", "rm", "cp", "mv", "mkdir", "chmod",
        "chown", "file", "stat", "du", "df", "sort", "uniq", "cut", "tr", "tee", "xargs",
    ];
    if ARG_COMMANDS.contains(&words[0]) {
        return words[0].to_string();
    }

    // grep/find/ls: keep first flag if present (e.g. "grep -n", "find .", "ls -la")
    const FLAG_COMMANDS: &[&str] = &["grep", "find", "ls"];
    if FLAG_COMMANDS.contains(&words[0]) && words.len() >= 2 {
        // Keep first flag (-n, -la, etc.) but not path arguments
        if words[1].starts_with('-') {
            return format!("{} {}", words[0], words[1]);
        }
        return words[0].to_string();
    }

    // Default: first 2 words
    if words.len() >= 2 {
        format!("{} {}", words[0], words[1])
    } else {
        words[0].to_string()
    }
}

/// Extract the subcommand from a command string (second word).
fn extract_subcmd(cmd: &str) -> &str {
    let parts: Vec<&str> = cmd.trim().splitn(3, char::is_whitespace).collect();
    if parts.len() >= 2 {
        parts[1]
    } else {
        ""
    }
}

/// Truncate a command for display (keep first meaningful portion).
fn truncate_command(cmd: &str) -> String {
    let trimmed = cmd.trim();
    // Keep first two words for display
    let parts: Vec<&str> = trimmed.splitn(3, char::is_whitespace).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        _ => format!("{} {}", parts[0], parts[1]),
    }
}
