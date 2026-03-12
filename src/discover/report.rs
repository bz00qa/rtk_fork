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

/// A detected usage pattern that could benefit from RTK meta-commands.
#[derive(Debug, Serialize)]
pub struct PatternOpportunity {
    pub pattern: String,
    pub suggestion: String,
    pub occurrences: usize,
    pub estimated_savings_tokens: usize,
}

/// A command ranked by total output tokens consumed.
#[derive(Debug, Serialize)]
pub struct TokenConsumer {
    pub command: String,
    pub count: usize,
    pub total_tokens: usize,
    pub avg_tokens: usize,
    pub has_rtk_filter: bool,
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
    pub patterns: Vec<PatternOpportunity>,
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

    pub fn total_pattern_tokens(&self) -> usize {
        self.patterns
            .iter()
            .map(|p| p.estimated_savings_tokens)
            .sum()
    }

    pub fn total_pattern_occurrences(&self) -> usize {
        self.patterns.iter().map(|p| p.occurrences).sum()
    }

    pub fn grand_total_tokens(&self) -> usize {
        self.total_saveable_tokens() + self.total_pattern_tokens()
    }
}

/// Bold green styled text (TTY-aware)
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

fn pad_right(text: &str, width: usize, styler: fn(&str) -> String) -> String {
    let padded = format!("{:<width$}", text, width = width);
    let visible_len = text.len().min(width);
    let trailing = &padded[visible_len..];
    format!("{}{}", styler(&padded[..visible_len]), trailing)
}

fn pad_left(text: &str, width: usize, styler: fn(&str) -> String) -> String {
    if text.len() >= width {
        return styler(text);
    }
    let padding = width - text.len();
    format!("{}{}", " ".repeat(padding), styler(text))
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
        out.push_str("\nAll commands are already using RTK. Nothing to report.\n");
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

    // Pattern opportunities
    if !report.patterns.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            styled("PATTERN OPPORTUNITIES -- use RTK meta-commands")
        ));
        out.push_str(&format!("{}\n", styled(&"-".repeat(72))));

        for p in &report.patterns {
            out.push_str(&format!(
                "  {} ({}x) \u{2192} {}\n    Est. savings: ~{}\n",
                style_cmd(&p.pattern),
                p.occurrences,
                style_cmd(&p.suggestion),
                style_tokens(&format_tokens(p.estimated_savings_tokens)),
            ));
        }

        out.push_str(&format!("{}\n", styled(&"-".repeat(72))));
        out.push_str(&format!(
            "Total: {} patterns, {} occurrences -> ~{} saveable\n",
            style_tokens(&report.patterns.len().to_string()),
            style_tokens(&report.total_pattern_occurrences().to_string()),
            style_tokens(&format_tokens(report.total_pattern_tokens())),
        ));
    }

    // Grand total
    let grand = report.grand_total_tokens();
    if grand > 0 {
        out.push_str(&format!(
            "\n{}: ~{} (missed + patterns)\n",
            styled("TOTAL SAVEABLE"),
            style_tokens(&format_tokens(grand)),
        ));
    }

    // Top token consumers
    if !report.consumers.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            styled("TOP TOKEN CONSUMERS (by output size)")
        ));
        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(72))));
        out.push_str(&format!(
            "  {:<3} {:<22} {:>5}  {:>9} {:>9}   {}\n",
            "#", "Command", "Count", "Total", "Avg", "RTK?"
        ));
        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(72))));

        for (i, c) in report.consumers.iter().take(limit).enumerate() {
            let rtk_label = if c.has_rtk_filter {
                styled("Yes")
            } else if std::io::stdout().is_terminal() {
                "No".red().bold().to_string()
            } else {
                "No".to_string()
            };
            out.push_str(&format!(
                " {:>2}.  {} {:>5}  {} {}   {}\n",
                i + 1,
                pad_right(&truncate_str(&c.command, 22), 22, style_cmd),
                c.count,
                pad_left(&format_tokens(c.total_tokens), 9, style_tokens),
                pad_left(&format_tokens(c.avg_tokens), 9, style_tokens),
                rtk_label,
            ));
        }

        out.push_str(&format!("{}\n", styled(&"\u{2500}".repeat(72))));

        let has_unfiltered = report
            .consumers
            .iter()
            .take(limit)
            .any(|c| !c.has_rtk_filter);
        if has_unfiltered {
            out.push_str("  RTK?=No: not yet filtered. Request support:\n  -> github.com/rtk-ai/rtk/issues\n");
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

    // RTK_DISABLED summary
    if report.rtk_disabled_count > 0 {
        out.push_str(&format!(
            "\nRTK_DISABLED=1 bypasses: {} commands\n",
            report.rtk_disabled_count
        ));
        if !report.rtk_disabled_examples.is_empty() {
            out.push_str("  Examples: ");
            out.push_str(&report.rtk_disabled_examples.join(", "));
            out.push('\n');
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        fn fake_ansi(s: &str) -> String {
            format!("\x1b[1;96m{}\x1b[0m", s)
        }
        let result = pad_right("git", 10, fake_ansi);
        assert!(result.starts_with("\x1b[1;96mgit\x1b[0m"));
        assert!(result.ends_with("       "));
    }

    #[test]
    fn test_pad_left_with_ansi_preserves_visual_width() {
        fn fake_ansi(s: &str) -> String {
            format!("\x1b[97m{}\x1b[0m", s)
        }
        let result = pad_left("42", 8, fake_ansi);
        assert!(result.starts_with("      \x1b[97m42\x1b[0m"));
    }

    #[test]
    fn test_format_text_table_alignment() {
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
            patterns: vec![],
            total_commands: 100,
            sessions_scanned: 5,
            already_rtk: 0,
            since_days: 90,
            parse_errors: 0,
            rtk_disabled_count: 0,
            rtk_disabled_examples: vec![],
        };

        let output = format_text(&report, 20, false);
        let lines: Vec<&str> = output.lines().collect();

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

        let header_rtk_pos = header_line.find("RTK Equivalent").unwrap();
        let data1_rtk_pos = data_line_1.find("rtk git").unwrap();
        let data2_rtk_pos = data_line_2.find("rtk cargo").unwrap();

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
}
