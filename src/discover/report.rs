use serde::Serialize;

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

    /// Total saveable across all sections (missed + patterns)
    pub fn grand_total_tokens(&self) -> usize {
        self.total_saveable_tokens() + self.total_pattern_tokens()
    }
}

/// Format report as text.
pub fn format_text(report: &DiscoverReport, limit: usize, verbose: bool) -> String {
    let mut out = String::with_capacity(2048);

    out.push_str("RTK Discover -- Savings Opportunities\n");
    out.push_str(&"=".repeat(52));
    out.push('\n');
    out.push_str(&format!(
        "Scanned: {} sessions (last {} days), {} Bash commands\n",
        report.sessions_scanned, report.since_days, report.total_commands
    ));
    out.push_str(&format!(
        "Already using RTK: {} commands ({}%)\n",
        report.already_rtk,
        if report.total_commands > 0 {
            report.already_rtk * 100 / report.total_commands
        } else {
            0
        }
    ));

    if report.supported.is_empty() && report.unsupported.is_empty() {
        out.push_str("\nNo missed savings found. RTK usage looks good!\n");
        return out;
    }

    // Missed savings
    if !report.supported.is_empty() {
        out.push_str("\nMISSED SAVINGS -- Commands RTK already handles\n");
        out.push_str(&"-".repeat(72));
        out.push('\n');
        out.push_str(&format!(
            "{:<24} {:>5}    {:<18} {:<13} {:>12}\n",
            "Command", "Count", "RTK Equivalent", "Status", "Est. Savings"
        ));

        for entry in report.supported.iter().take(limit) {
            out.push_str(&format!(
                "{:<24} {:>5}    {:<18} {:<13} ~{}\n",
                truncate_str(&entry.command, 23),
                entry.count,
                entry.rtk_equivalent,
                entry.rtk_status.as_str(),
                format_tokens(entry.estimated_savings_tokens),
            ));
        }

        out.push_str(&"-".repeat(72));
        out.push('\n');
        out.push_str(&format!(
            "Total: {} commands -> ~{} saveable\n",
            report.total_supported_count(),
            format_tokens(report.total_saveable_tokens()),
        ));
    }

    // Pattern opportunities
    if !report.patterns.is_empty() {
        out.push_str("\nPATTERN OPPORTUNITIES -- use RTK meta-commands\n");
        out.push_str(&"-".repeat(72));
        out.push('\n');

        for p in &report.patterns {
            out.push_str(&format!(
                "  {} ({}x) → {}\n    Est. savings: ~{}\n",
                p.pattern,
                p.occurrences,
                p.suggestion,
                format_tokens(p.estimated_savings_tokens),
            ));
        }

        out.push_str(&"-".repeat(72));
        out.push('\n');
        out.push_str(&format!(
            "Total: {} patterns, {} occurrences -> ~{} saveable\n",
            report.patterns.len(),
            report.total_pattern_occurrences(),
            format_tokens(report.total_pattern_tokens()),
        ));
    }

    // Grand total
    let grand = report.grand_total_tokens();
    if grand > 0 {
        out.push_str(&format!(
            "\nTOTAL SAVEABLE: ~{} (missed + patterns)\n",
            format_tokens(grand),
        ));
    }

    // Top token consumers
    if !report.consumers.is_empty() {
        out.push_str("\nTOP TOKEN CONSUMERS (by output size)\n");
        out.push_str(&"\u{2500}".repeat(72));
        out.push('\n');
        out.push_str(&format!(
            "  {:<3} {:<22} {:>5}  {:>9} {:>9}   {}\n",
            "#", "Command", "Count", "Total", "Avg", "RTK?"
        ));
        out.push_str(&"\u{2500}".repeat(72));
        out.push('\n');

        for (i, c) in report.consumers.iter().take(limit).enumerate() {
            let rtk_label = if c.has_rtk_filter { "Yes" } else { "No" };
            out.push_str(&format!(
                " {:>2}.  {:<22} {:>5}  {:>9} {:>9}   {}\n",
                i + 1,
                truncate_str(&c.command, 22),
                c.count,
                format_tokens(c.total_tokens),
                format_tokens(c.avg_tokens),
                rtk_label,
            ));
        }

        out.push_str(&"\u{2500}".repeat(72));
        out.push('\n');
    }

    // Unhandled
    if !report.unsupported.is_empty() {
        out.push_str("\nTOP UNHANDLED COMMANDS -- open an issue?\n");
        out.push_str(&"-".repeat(52));
        out.push('\n');
        out.push_str(&format!(
            "{:<24} {:>5}    {}\n",
            "Command", "Count", "Example"
        ));

        for entry in report.unsupported.iter().take(limit) {
            out.push_str(&format!(
                "{:<24} {:>5}    {}\n",
                truncate_str(&entry.base_command, 23),
                entry.count,
                truncate_str(&entry.example, 40),
            ));
        }

        out.push_str(&"-".repeat(52));
        out.push('\n');
        let total_unhandled: usize = report.unsupported.iter().map(|u| u.count).sum();
        out.push_str(&format!(
            "Total: {} unique commands, {} occurrences\n",
            report.unsupported.len(),
            total_unhandled
        ));
        out.push_str("-> github.com/rtk-ai/rtk/issues\n");
    }

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
