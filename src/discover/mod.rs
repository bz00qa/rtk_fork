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
use report::{DiscoverReport, SupportedEntry, TokenConsumer, UnsupportedEntry};

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
            let classification = match classify_command(&base) {
                c @ Classification::Supported { .. } => c,
                _ => classify_command(&format!("{} .", &base)),
            };
            let rtk_handling = match classification {
                Classification::Supported {
                    status: report::RtkStatus::Existing,
                    ..
                } => report::RtkHandling::Filtered,
                Classification::Supported {
                    status: report::RtkStatus::Passthrough,
                    ..
                } => report::RtkHandling::Passthrough,
                _ => report::RtkHandling::None,
            };
            TokenConsumer {
                command: base,
                count,
                total_tokens,
                avg_tokens,
                rtk_handling,
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- consumer_base() tests ---

    #[test]
    fn test_consumer_base_empty() {
        assert_eq!(consumer_base(""), "");
    }

    #[test]
    fn test_consumer_base_single_word() {
        assert_eq!(consumer_base("htop"), "htop");
    }

    #[test]
    fn test_consumer_base_two_words() {
        assert_eq!(consumer_base("git status"), "git status");
    }

    #[test]
    fn test_consumer_base_three_words_truncated() {
        assert_eq!(consumer_base("go test ./..."), "go test");
    }

    #[test]
    fn test_consumer_base_python_m_module() {
        assert_eq!(consumer_base("python -m pytest foo"), "python -m pytest");
        assert_eq!(consumer_base("python3 -m mypy src/"), "python3 -m mypy");
    }

    #[test]
    fn test_consumer_base_python_m_short() {
        // Only 2 words: "python -m" — not enough for 3-word form, falls to default
        assert_eq!(consumer_base("python -m"), "python -m");
    }

    #[test]
    fn test_consumer_base_pnpm_filter_skip() {
        assert_eq!(consumer_base("pnpm --filter @app/web build"), "pnpm build");
        assert_eq!(consumer_base("pnpm -F @app/api test"), "pnpm test");
    }

    #[test]
    fn test_consumer_base_pnpm_recursive_skip() {
        assert_eq!(consumer_base("pnpm -r build"), "pnpm build");
        assert_eq!(consumer_base("pnpm --recursive build"), "pnpm build");
        assert_eq!(consumer_base("pnpm -w install"), "pnpm install");
        assert_eq!(
            consumer_base("pnpm --workspace-root install"),
            "pnpm install"
        );
    }

    #[test]
    fn test_consumer_base_pnpm_filter_combined() {
        // --filter=value form
        assert_eq!(consumer_base("pnpm --filter=@app/web build"), "pnpm build");
    }

    #[test]
    fn test_consumer_base_pnpm_only_flags() {
        // All flags, no subcommand — falls back to "pnpm"
        assert_eq!(consumer_base("pnpm -r"), "pnpm");
    }

    #[test]
    fn test_consumer_base_arg_commands() {
        // cat/head/tail etc. group by command name only
        assert_eq!(consumer_base("cat foo.txt"), "cat");
        assert_eq!(consumer_base("head -20 file.rs"), "head");
        assert_eq!(consumer_base("wc -l src/main.rs"), "wc");
        assert_eq!(consumer_base("sort output.txt"), "sort");
    }

    #[test]
    fn test_consumer_base_flag_commands() {
        // grep/find/ls keep first flag if present
        assert_eq!(consumer_base("grep -n pattern"), "grep -n");
        assert_eq!(consumer_base("find . -name"), "find");
        assert_eq!(consumer_base("ls -la"), "ls -la");
        // No flag — just command name
        assert_eq!(consumer_base("grep pattern"), "grep");
        assert_eq!(consumer_base("ls src/"), "ls");
    }

    // --- effective_idx tests ---

    #[test]
    fn test_effective_idx_single_command() {
        let parts = ["git status"];
        let idx = parts
            .iter()
            .rposition(|p| !matches!(classify_command(p), Classification::Ignored));
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn test_effective_idx_chain_with_cd_prefix() {
        let parts = ["cd /some/dir", "npm run build"];
        let idx = parts
            .iter()
            .rposition(|p| !matches!(classify_command(p), Classification::Ignored));
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_effective_idx_all_ignored() {
        let parts = ["cd /tmp", "echo hello", "pwd"];
        let idx = parts
            .iter()
            .rposition(|p| !matches!(classify_command(p), Classification::Ignored));
        assert_eq!(idx, None);
    }

    #[test]
    fn test_effective_idx_env_chain() {
        // PATH=... is a pure assignment (Ignored), npm run build is Supported
        let parts = [r#"PATH="/c/Program Files/nodejs:$PATH""#, "npm run build"];
        let idx = parts
            .iter()
            .rposition(|p| !matches!(classify_command(p), Classification::Ignored));
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_effective_idx_last_non_ignored() {
        // Multiple supported commands — picks LAST one (rposition)
        let parts = ["git status", "cargo test"];
        let idx = parts
            .iter()
            .rposition(|p| !matches!(classify_command(p), Classification::Ignored));
        assert_eq!(idx, Some(1));
    }
}
