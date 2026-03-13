use colored::Colorize;
use serde::Serialize;
use std::io::IsTerminal;

/// RTK support status for a command.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum RtkStatus {
    /// Dedicated handler with filtering (e.g., git status → git.rs:run_status())
    Existing,
    /// Works via external_subcommand passthrough, no filtering (e.g., cargo fmt → Other)
    Passthrough,
    /// RTK doesn't handle this command at all
    NotSupported,
}

impl RtkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RtkStatus::Existing => "existing",
            RtkStatus::Passthrough => "passthrough",
            RtkStatus::NotSupported => "not-supported",
        }
    }
}

/// A supported command that RTK already handles.
#[derive(Debug, Serialize)]
pub struct SupportedEntry {
    pub command: String,
    pub count: usize,
    pub rtk_equivalent: &'static str,
    pub category: &'static str,
    pub estimated_savings_tokens: usize,
    pub estimated_savings_pct: f64,
    pub rtk_status: RtkStatus,
}

/// An unsupported command not yet handled by RTK.
#[derive(Debug, Serialize)]
pub struct UnsupportedEntry {
    pub base_command: String,
    pub count: usize,
    pub example: String,
}

/// RTK handling level for a token consumer.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum RtkHandling {
    /// Dedicated filter with token savings
    Filtered,
    /// Routed through RTK but no filtering (0% savings)
    Passthrough,
    /// Not handled by RTK at all
    None,
}

/// A command ranked by total output tokens consumed.
#[derive(Debug, Serialize)]
pub struct TokenConsumer {
    pub command: String,
    pub count: usize,
    pub total_tokens: usize,
    pub avg_tokens: usize,
    pub rtk_handling: RtkHandling,
}

/// Full discover report.
#[derive(Debug, Serialize)]
pub struct DiscoverReport {
    pub sessions_scanned: usize,
    pub total_commands: usize,
    pub already_rtk: usize,
    pub since_days: u64,
    pub supported: Vec<SupportedEntry>,
    pub unsupported: Vec<UnsupportedEntry>,
    pub consumers: Vec<TokenConsumer>,
    pub parse_errors: usize,
    pub rtk_disabled_count: usize,
    pub rtk_disabled_examples: Vec<String>,
}

impl DiscoverReport {
    pub fn total_saveable_tokens(&self) -> usize {
        self.supported
            .iter()
            .map(|s| s.estimated_savings_tokens)
            .sum()
    }

    pub fn total_supported_count(&self) -> usize {
        self.supported.iter().map(|s| s.count).sum()
    }

    /// Total saveable tokens across all supported commands.
    pub fn grand_total_tokens(&self) -> usize {
        self.total_saveable_tokens()
    }
}

/// Bold green styled text (TTY-aware, matches gain.rs style)
fn styled(text: &str) -> String {
    if !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    text.bold().green().to_string()
}

/// Colorize percentage based on savings tier (TTY-aware)
fn colorize_pct(pct: f64) -> String {
    let text = format!("{:.0}%", pct);
    if !std::io::stdout().is_terminal() {
        return text;
    }
    if pct >= 70.0 {
        text.green().bold().to_string()
    } else if pct >= 40.0 {
        text.yellow().bold().to_string()
    } else {
        text.red().bold().to_string()
    }
}

/// Style command names with cyan+bold (TTY-aware)
fn style_cmd(cmd: &str) -> String {
    if !std::io::stdout().is_terminal() {
        return cmd.to_string();
    }
    cmd.bright_cyan().bold().to_string()
}

/// Style token counts (TTY-aware)
fn style_tokens(text: &str) -> String {
    if !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    text.bright_white().to_string()
}

/// Format report as text.
pub fn format_text(report: &DiscoverReport, limit: usize, verbose: bool) -> String {
    let mut out = String::with_capacity(2048);

    out.push_str(&format!(
        "{}\n",
        styled("RTK Discover -- Savings Opportunities")
    ));
    out.push_str(&format!("{}\n", styled(&"=".repeat(52))));
    out.push_str(&format!(
        "Scanned: {} sessions (last {} days), {} Bash commands\n",
        style_tokens(&report.sessions_scanned.to_string()),
        report.since_days,
        style_tokens(&report.total_commands.to_string()),
    ));
    let rtk_pct = if report.total_commands > 0 {
        report.already_rtk * 100 / report.total_commands
    } else {
        0
    };
    out.push_str(&format!(
        "Already using RTK: {} commands ({})\n",
        style_tokens(&report.already_rtk.to_string()),
        colorize_pct(rtk_pct as f64),
    ));

    if report.supported.is_empty() && report.unsupported.is_empty() {
        out.push_str("\nNo missed savings found. RTK usage looks good!\n");
        return out;
    }

    // Missed savings
    if !report.supported.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            styled("MISSED SAVINGS -- Commands RTK already handles")
        ));
        out.push_str(&format!("{}\n", styled(&"-".repeat(72))));
        out.push_str(&format!(
            "{:<24} {:>5}    {:<18} {:<13} {:>12}\n",
            "Command", "Count", "RTK Equivalent", "Status", "Est. Savings"
        ));

        for entry in report.supported.iter().take(limit) {
            out.push_str(&format!(
                "{} {:>5}    {} {:<13} ~{}\n",
                pad_right(&truncate_str(&entry.command, 23), 24, style_cmd),
                entry.count,
                pad_right(entry.rtk_equivalent, 18, style_cmd),
                entry.rtk_status.as_str(),
                pad_left(
                    &format_tokens(entry.estimated_savings_tokens),
                    12,
                    style_tokens
                ),
            ));
        }

        out.push_str(&format!("{}\n", styled(&"-".repeat(72))));
        out.push_str(&format!(
            "Total: {} commands -> ~{} saveable\n",
            style_tokens(&report.total_supported_count().to_string()),
            style_tokens(&format_tokens(report.total_saveable_tokens())),
        ));
    }

    // Grand total
    let grand = report.grand_total_tokens();
    if grand > 0 {
        out.push_str(&format!(
            "\n{}: ~{}\n",
            styled("TOTAL SAVEABLE"),
            style_tokens(&format_tokens(grand)),
        ));
    }

    // Top token consumers
    if !report.consumers.is_empty() {
        // Compute dynamic column widths for Total and Avg
        let total_w = report
            .consumers
            .iter()
            .take(limit)
            .map(|c| format_tokens(c.total_tokens).len())
            .max()
            .unwrap_or(5)
            .max(5);
        let avg_w = report
            .consumers
            .iter()
            .take(limit)
            .map(|c| format_tokens(c.avg_tokens).len())
            .max()
            .unwrap_or(3)
            .max(3);
        // Compute count column width dynamically too
        let count_w = report
            .consumers
            .iter()
            .take(limit)
            .map(|c| c.count.to_string().len())
            .max()
            .unwrap_or(5)
            .max(5);
        let table_w = 4 + 22 + 2 + count_w + 2 + total_w + 2 + avg_w + 3 + 4;

        out.push_str(&format!(
            "\n{}\n",
            styled("TOP TOKEN CONSUMERS (by output size)")
        ));
        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(table_w))));
        out.push_str(&format!(
            "  {:<3} {:<22} {:>count_w$}  {:>total_w$}  {:>avg_w$}   {}\n",
            "#",
            "Command",
            "Count",
            "Total",
            "Avg",
            "RTK?",
            count_w = count_w,
            total_w = total_w,
            avg_w = avg_w,
        ));
        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(table_w))));

        for (i, c) in report.consumers.iter().take(limit).enumerate() {
            let rtk_label = match c.rtk_handling {
                RtkHandling::Filtered => styled("Yes"),
                RtkHandling::Passthrough => {
                    if std::io::stdout().is_terminal() {
                        "Pass".yellow().to_string()
                    } else {
                        "Pass".to_string()
                    }
                }
                RtkHandling::None => {
                    if std::io::stdout().is_terminal() {
                        "No".red().bold().to_string()
                    } else {
                        "No".to_string()
                    }
                }
            };
            out.push_str(&format!(
                " {:>2}.  {} {:>count_w$}  {}  {}   {}\n",
                i + 1,
                pad_right(&truncate_str(&c.command, 22), 22, style_cmd),
                c.count,
                pad_left(&format_tokens(c.total_tokens), total_w, style_tokens),
                pad_left(&format_tokens(c.avg_tokens), avg_w, style_tokens),
                rtk_label,
                count_w = count_w,
            ));
        }

        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(table_w))));

        let has_unfiltered = report
            .consumers
            .iter()
            .take(limit)
            .any(|c| c.rtk_handling == RtkHandling::None);
        if has_unfiltered {
            out.push_str(
                "  RTK?=No: not yet filtered. Request support:\n  \
                 -> github.com/rtk-ai/rtk/issues\n",
            );
        }
    }

    // Unhandled
    if !report.unsupported.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            styled("TOP UNHANDLED COMMANDS -- open an issue?")
        ));
        out.push_str(&format!("{}\n", styled(&"-".repeat(52))));
        out.push_str(&format!(
            "{:<24} {:>5}    {}\n",
            "Command", "Count", "Example"
        ));

        for entry in report.unsupported.iter().take(limit) {
            out.push_str(&format!(
                "{} {:>5}    {}\n",
                pad_right(&truncate_str(&entry.base_command, 23), 24, style_cmd),
                entry.count,
                truncate_str(&entry.example, 40),
            ));
        }

        out.push_str(&format!("{}\n", styled(&"-".repeat(52))));
        let total_unhandled: usize = report.unsupported.iter().map(|u| u.count).sum();
        out.push_str(&format!(
            "Total: {} unique commands, {} occurrences\n",
            style_tokens(&report.unsupported.len().to_string()),
            style_tokens(&total_unhandled.to_string()),
        ));
        out.push_str("-> github.com/rtk-ai/rtk/issues\n");
    }

    // RTK_DISABLED bypass warning
    if report.rtk_disabled_count > 0 {
        out.push_str(&format!(
            "\nRTK_DISABLED BYPASS -- {} commands ran without filtering\n",
            report.rtk_disabled_count
        ));
        out.push_str(&"-".repeat(72));
        out.push('\n');
        out.push_str("These commands used RTK_DISABLED=1 unnecessarily:\n");
        if !report.rtk_disabled_examples.is_empty() {
            out.push_str(&format!("  {}\n", report.rtk_disabled_examples.join(", ")));
        }
        out.push_str("-> Remove RTK_DISABLED=1 to recover token savings\n");
    }

    out.push_str("\n~estimated from tool_result output sizes\n");

    if verbose && report.parse_errors > 0 {
        out.push_str(&format!(
            "\nParse errors skipped: {}\n",
            report.parse_errors
        ));
    }

    out
}

/// Format report as JSON.
pub fn format_json(report: &DiscoverReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{} tokens", tokens)
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // UTF-8 safe truncation: collect chars up to max-2, then add ".."
        let truncated: String = s
            .char_indices()
            .take_while(|(i, _)| *i < max.saturating_sub(2))
            .map(|(_, c)| c)
            .collect();
        format!("{}..", truncated)
    }
}

/// Pad text to `width` (left-aligned) BEFORE applying ANSI styling.
/// This ensures format alignment is correct regardless of escape sequences.
fn pad_right(text: &str, width: usize, styler: fn(&str) -> String) -> String {
    let padded = format!("{:<width$}", text, width = width);
    // Split into visible content and trailing spaces
    let visible_len = text.len().min(width);
    let trailing = &padded[visible_len..];
    format!("{}{}", styler(&padded[..visible_len]), trailing)
}

/// Pad text to `width` (right-aligned) BEFORE applying ANSI styling.
fn pad_left(text: &str, width: usize, styler: fn(&str) -> String) -> String {
    if text.len() >= width {
        return styler(text);
    }
    let padding = width - text.len();
    format!("{}{}", " ".repeat(padding), styler(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Identity styler for testing (no ANSI codes)
    fn no_style(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn test_pad_right_shorter_text() {
        let result = pad_right("hello", 10, no_style);
        assert_eq!(result, "hello     ");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_pad_right_exact_width() {
        let result = pad_right("hello", 5, no_style);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_pad_left_shorter_text() {
        let result = pad_left("42", 5, no_style);
        assert_eq!(result, "   42");
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_pad_left_exact_width() {
        let result = pad_left("hello", 5, no_style);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_pad_right_with_ansi_preserves_visual_width() {
        // Simulate ANSI styling that adds escape codes
        fn fake_ansi(s: &str) -> String {
            format!("\x1b[1;96m{}\x1b[0m", s)
        }
        let result = pad_right("git", 10, fake_ansi);
        // Should have ANSI around "git" then 7 trailing spaces
        assert!(result.starts_with("\x1b[1;96mgit\x1b[0m"));
        assert!(result.ends_with("       "));
        // Visual width: 3 (git) + 7 (spaces) = 10
    }

    #[test]
    fn test_pad_left_with_ansi_preserves_visual_width() {
        fn fake_ansi(s: &str) -> String {
            format!("\x1b[97m{}\x1b[0m", s)
        }
        let result = pad_left("42", 8, fake_ansi);
        // Should have 6 spaces then ANSI "42"
        assert!(result.starts_with("      \x1b[97m42\x1b[0m"));
    }

    #[test]
    fn test_format_text_table_alignment() {
        // Build a minimal report with varied-length data
        let report = DiscoverReport {
            supported: vec![
                SupportedEntry {
                    command: "git status".into(),
                    count: 42,
                    rtk_equivalent: "rtk git",
                    category: "Git",
                    rtk_status: RtkStatus::Existing,
                    estimated_savings_pct: 70.0,
                    estimated_savings_tokens: 1500,
                },
                SupportedEntry {
                    command: "cargo test filter::".into(),
                    count: 3,
                    rtk_equivalent: "rtk cargo",
                    category: "Cargo",
                    rtk_status: RtkStatus::Existing,
                    estimated_savings_pct: 90.0,
                    estimated_savings_tokens: 250000,
                },
            ],
            unsupported: vec![],
            consumers: vec![],

            total_commands: 100,
            sessions_scanned: 5,
            already_rtk: 0,
            since_days: 90,
            parse_errors: 0,
            rtk_disabled_count: 0,
            rtk_disabled_examples: vec![],
        };

        // Non-TTY output has no ANSI codes — verify columns align
        let output = format_text(&report, 20, false);
        let lines: Vec<&str> = output.lines().collect();

        // Find header and data rows in MISSED SAVINGS table
        let header_line = lines
            .iter()
            .find(|l| l.contains("Command") && l.contains("Count"))
            .expect("header not found");
        let data_line_1 = lines
            .iter()
            .find(|l| l.contains("git status"))
            .expect("git status row not found");
        let data_line_2 = lines
            .iter()
            .find(|l| l.contains("cargo test"))
            .expect("cargo test row not found");

        // Verify column alignment by checking "RTK Equivalent" column position
        let header_rtk_pos = header_line.find("RTK Equivalent").unwrap();
        let data1_rtk_pos = data_line_1.find("rtk git").unwrap();
        let data2_rtk_pos = data_line_2.find("rtk cargo").unwrap();

        // RTK Equivalent column should start at same position in all rows
        assert_eq!(
            data1_rtk_pos, data2_rtk_pos,
            "RTK Equivalent column misaligned:\n  '{}'\n  '{}'",
            data_line_1, data_line_2
        );
        assert_eq!(
            header_rtk_pos, data1_rtk_pos,
            "Header vs data RTK column misaligned:\n  '{}'\n  '{}'",
            header_line, data_line_1
        );
    }

    #[test]
    fn test_consumers_table_alignment() {
        let report = DiscoverReport {
            supported: vec![],
            // Need at least one unsupported entry to avoid early return
            unsupported: vec![UnsupportedEntry {
                base_command: "dummy".into(),
                count: 1,
                example: "dummy".into(),
            }],

            consumers: vec![
                TokenConsumer {
                    command: "git diff".into(),
                    count: 99,
                    total_tokens: 121600,
                    avg_tokens: 1228,
                    rtk_handling: RtkHandling::Filtered,
                },
                TokenConsumer {
                    command: "cargo test".into(),
                    count: 91,
                    total_tokens: 59100,
                    avg_tokens: 649,
                    rtk_handling: RtkHandling::Filtered,
                },
                TokenConsumer {
                    command: "grep -n".into(),
                    count: 73,
                    total_tokens: 5300,
                    avg_tokens: 72,
                    rtk_handling: RtkHandling::Passthrough,
                },
            ],
            total_commands: 263,
            sessions_scanned: 10,
            already_rtk: 0,
            since_days: 30,
            parse_errors: 0,
            rtk_disabled_count: 0,
            rtk_disabled_examples: vec![],
        };

        let output = format_text(&report, 20, false);
        let lines: Vec<&str> = output.lines().collect();

        // Find header and data lines in TOP TOKEN CONSUMERS
        let header = lines
            .iter()
            .find(|l| l.contains("Command") && l.contains("Total") && l.contains("Avg"))
            .expect("consumers header not found");
        let row1 = lines
            .iter()
            .find(|l| l.contains("git diff"))
            .expect("git diff row not found");
        let row3 = lines
            .iter()
            .find(|l| l.contains("grep -n"))
            .expect("grep row not found");

        // RTK? column should align — Yes/Pass/No all start at same column
        let header_rtk = header.find("RTK?").unwrap();
        let row1_rtk = row1.find("Yes").unwrap();
        let row3_rtk = row3.find("Pass").unwrap();
        assert_eq!(
            row1_rtk, row3_rtk,
            "RTK? column misaligned between rows:\n  '{}'\n  '{}'",
            row1, row3
        );
        assert_eq!(
            header_rtk, row1_rtk,
            "RTK? header vs data misaligned:\n  '{}'\n  '{}'",
            header, row1
        );
    }
}
