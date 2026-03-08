pub mod provider;
pub mod registry;
mod report;
pub mod rules;

use anyhow::Result;
use std::collections::HashMap;

use provider::{ClaudeProvider, SessionProvider};
use registry::{category_avg_tokens, classify_command, split_command_chain, Classification};
use report::{DiscoverReport, PatternOpportunity, SupportedEntry, UnsupportedEntry};

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
    let mut supported_map: HashMap<&'static str, SupportedBucket> = HashMap::new();
    let mut unsupported_map: HashMap<String, UnsupportedBucket> = HashMap::new();

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
            for part in parts {
                total_commands += 1;

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
                        let output_tokens = if let Some(len) = ext_cmd.output_len {
                            // Real: from tool_result content length
                            len / 4
                        } else {
                            // Fallback: category average
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

    let report = DiscoverReport {
        sessions_scanned: sessions.len(),
        total_commands,
        already_rtk,
        since_days,
        supported,
        unsupported,
        patterns,
        parse_errors,
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
