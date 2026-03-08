use crate::git;
use crate::tracking;
use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

/// Cache file for dedup: stores hash of last context output
fn cache_path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("rtk").join("context_hash"))
}

fn hash_output(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Compound context command: git status + diff + recent log in one shot.
///
/// Gives the LLM complete project context in a single call instead of
/// three separate commands, reducing round-trip overhead.
///
/// Session dedup: if output is identical to last call, prints a one-liner
/// instead of repeating the full context.
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

    // Session dedup: check if output matches last call
    let current_hash = hash_output(&output);
    let is_duplicate = if let Some(path) = cache_path() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|prev| prev == current_hash)
            .unwrap_or(false)
    } else {
        false
    };

    if is_duplicate {
        println!("(no changes since last rtk context)");
        timer.track(
            "git status && git diff && git log",
            "rtk context (dedup)",
            &output,
            "(no changes since last rtk context)",
        );
    } else {
        // Save hash for next dedup check
        if let Some(path) = cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, current_hash.to_string());
        }

        if verbose > 0 {
            eprintln!("Context: 4 sections combined");
        }

        println!("{}", output);

        let raw_estimate = format!("{}\n{}\n{}", status_out, diff_stat, log_out);
        timer.track(
            "git status && git diff && git log",
            "rtk context",
            &raw_estimate,
            &output,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_output_format() {
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

    #[test]
    fn test_hash_deterministic() {
        let a = hash_output("hello world");
        let b = hash_output("hello world");
        let c = hash_output("hello world!");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_cache_path_exists() {
        // cache_path() should return Some on all platforms
        let path = cache_path();
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("context_hash"));
    }
}
