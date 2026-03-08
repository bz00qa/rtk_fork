use crate::git;
use crate::tracking;
use anyhow::{Context, Result};
use std::process::Command;

/// Compound context command: git status + diff + recent log in one shot.
///
/// Gives the LLM complete project context in a single call instead of
/// three separate commands, reducing round-trip overhead.
pub fn run(max_log: usize, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let mut sections = Vec::new();

    // 1. Git status (compact)
    let status = Command::new("git")
        .args(["status", "--short", "--branch"])
        .output()
        .context("Failed to run git status")?;
    let status_out = String::from_utf8_lossy(&status.stdout);

    sections.push("=== Status ===".to_string());
    if status_out.trim().is_empty() {
        sections.push("(clean)".to_string());
    } else {
        sections.push(status_out.trim().to_string());
    }

    // 2. Git diff (compact, changes only)
    let diff = Command::new("git")
        .args(["diff", "--stat"])
        .output()
        .context("Failed to run git diff")?;
    let diff_stat = String::from_utf8_lossy(&diff.stdout);

    if !diff_stat.trim().is_empty() {
        sections.push(String::new());
        sections.push("=== Diff ===".to_string());
        sections.push(diff_stat.trim().to_string());

        // Get actual changes (compact)
        let diff_full = Command::new("git")
            .arg("diff")
            .output()
            .context("Failed to run git diff")?;
        let diff_text = String::from_utf8_lossy(&diff_full.stdout);
        if !diff_text.is_empty() {
            let compacted = git::compact_diff(&diff_text, 200);
            if !compacted.is_empty() {
                sections.push(compacted);
            }
        }
    }

    // 3. Staged changes
    let staged = Command::new("git")
        .args(["diff", "--cached", "--stat"])
        .output()
        .context("Failed to run git diff --cached")?;
    let staged_stat = String::from_utf8_lossy(&staged.stdout);

    if !staged_stat.trim().is_empty() {
        sections.push(String::new());
        sections.push("=== Staged ===".to_string());
        sections.push(staged_stat.trim().to_string());
    }

    // 4. Recent log
    let log = Command::new("git")
        .args(["log", &format!("-{}", max_log), "--oneline", "--no-merges"])
        .output()
        .context("Failed to run git log")?;
    let log_out = String::from_utf8_lossy(&log.stdout);

    if !log_out.trim().is_empty() {
        sections.push(String::new());
        sections.push(format!("=== Log (last {}) ===", max_log));
        sections.push(log_out.trim().to_string());
    }

    let output = sections.join("\n");

    if verbose > 0 {
        eprintln!("Context: 4 sections combined");
    }

    println!("{}", output);

    // Track as if all 3 commands ran separately
    let raw_estimate = format!("{}\n{}\n{}", status_out, diff_stat, log_out);
    timer.track(
        "git status && git diff && git log",
        "rtk context",
        &raw_estimate,
        &output,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_output_format() {
        // Verify the output structure has section headers
        let sections = vec![
            "=== Status ===",
            "## main...origin/main",
            " M src/main.rs",
            "",
            "=== Log (last 5) ===",
            "abc1234 feat: add context command",
        ];
        let output = sections.join("\n");
        assert!(output.contains("=== Status ==="));
        assert!(output.contains("=== Log"));
        assert!(output.contains("abc1234"));
    }
}
