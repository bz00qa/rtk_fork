use crate::tracking;
use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Cache directory for watch state
fn watch_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("rtk").join("watch"))
}

/// Generate a stable key for the command (filesystem-safe hash)
fn cmd_key(cmd: &str) -> String {
    let mut hasher = DefaultHasher::new();
    cmd.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Run a command and show only what changed since the last run.
///
/// First run: shows full output.
/// Subsequent runs: shows only new/changed lines (diff from previous).
///
/// Great for iterative workflows:
///   rtk watch cargo test
///   rtk watch go test ./...
///   rtk watch pytest
///   rtk watch npx vitest run
pub fn run(command: &str, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("Watch: {}", command);
    }

    // Run the command
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    } else {
        Command::new("sh")
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    }
    .context("Failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let current = format!("{}{}", stdout, stderr);

    let exit_code = output.status.code().unwrap_or(1);
    let key = cmd_key(command);

    // Load previous output
    let prev = load_previous(&key);

    // Save current for next comparison
    save_current(&key, &current);

    let rtk_output = match prev {
        None => {
            // First run — show full output
            if verbose > 0 {
                eprintln!("(first run, showing full output)");
            }
            current.clone()
        }
        Some(ref prev_output) if prev_output == &current => {
            // Identical — minimal output
            format!("(no changes from last run, exit code {})", exit_code)
        }
        Some(ref prev_output) => {
            // Diff from previous
            compute_diff(prev_output, &current)
        }
    };

    println!("{}", rtk_output.trim());

    timer.track(command, "rtk watch", &current, &rtk_output);

    // Propagate exit code
    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

/// Compute a simple line-level diff between previous and current output.
/// Shows only added (+) and removed (-) lines with section markers.
fn compute_diff(prev: &str, current: &str) -> String {
    let prev_lines: Vec<&str> = prev.lines().collect();
    let curr_lines: Vec<&str> = current.lines().collect();

    // Build sets for quick lookup
    let prev_set: std::collections::HashSet<&str> = prev_lines.iter().copied().collect();
    let curr_set: std::collections::HashSet<&str> = curr_lines.iter().copied().collect();

    let mut result = Vec::new();

    // Lines removed (were in prev, not in current)
    let removed: Vec<&&str> = prev_lines
        .iter()
        .filter(|l| !l.trim().is_empty() && !curr_set.contains(**l))
        .collect();

    // Lines added (in current, not in prev)
    let added: Vec<&&str> = curr_lines
        .iter()
        .filter(|l| !l.trim().is_empty() && !prev_set.contains(**l))
        .collect();

    if removed.is_empty() && added.is_empty() {
        return "(no changes from last run)".to_string();
    }

    if !removed.is_empty() {
        result.push(format!("--- Resolved ({}) ---", removed.len()));
        for line in removed.iter().take(20) {
            result.push(format!("  {}", line));
        }
        if removed.len() > 20 {
            result.push(format!("  ... ({} more)", removed.len() - 20));
        }
    }

    if !added.is_empty() {
        result.push(format!("--- New ({}) ---", added.len()));
        for line in added.iter().take(30) {
            result.push(format!("  {}", line));
        }
        if added.len() > 30 {
            result.push(format!("  ... ({} more)", added.len() - 30));
        }
    }

    result.join("\n")
}

fn load_previous(key: &str) -> Option<String> {
    watch_dir()
        .map(|d| d.join(key))
        .and_then(|p| std::fs::read_to_string(p).ok())
}

fn save_current(key: &str, content: &str) {
    if let Some(dir) = watch_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(key), content);
    }
}

/// Clear all watch state (for testing or reset)
pub fn clear() -> Result<()> {
    if let Some(dir) = watch_dir() {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).context("Failed to clear watch cache")?;
            println!("Watch cache cleared");
        } else {
            println!("No watch cache found");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff_all_new() {
        let prev = "test_a ... ok\ntest_b ... ok\n";
        let current = "test_a ... ok\ntest_b ... ok\ntest_c ... ok\n";
        let diff = compute_diff(prev, current);
        assert!(diff.contains("New (1)"));
        assert!(diff.contains("test_c"));
        assert!(!diff.contains("Resolved"));
    }

    #[test]
    fn test_compute_diff_resolved() {
        let prev = "test_a ... FAILED\ntest_b ... ok\n";
        let current = "test_a ... ok\ntest_b ... ok\n";
        let diff = compute_diff(prev, current);
        assert!(diff.contains("Resolved (1)"));
        assert!(diff.contains("FAILED"));
        assert!(diff.contains("New (1)"));
    }

    #[test]
    fn test_compute_diff_identical() {
        let prev = "test_a ... ok\ntest_b ... ok\n";
        let current = "test_a ... ok\ntest_b ... ok\n";
        let diff = compute_diff(prev, current);
        assert!(diff.contains("no changes"), "got: {}", diff);
    }

    #[test]
    fn test_cmd_key_deterministic() {
        let a = cmd_key("cargo test");
        let b = cmd_key("cargo test");
        let c = cmd_key("go test ./...");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_cmd_key_filesystem_safe() {
        let key = cmd_key("cargo test -- --nocapture");
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(key.len(), 16);
    }

    #[test]
    fn test_compute_diff_many_changes() {
        let prev = (1..=50)
            .map(|i| format!("old_line_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let current = (1..=50)
            .map(|i| format!("new_line_{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = compute_diff(&prev, &current);
        // Should truncate at 20 removed, 30 added
        assert!(diff.contains("... (30 more)"));
        assert!(diff.contains("... (20 more)"));
    }
}
