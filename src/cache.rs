use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::config::Config;

/// List of volatile commands whose output changes on every invocation.
/// These should never be cached.
#[allow(dead_code)]
const VOLATILE_COMMANDS: &[&str] = &[
    "git status",
    "git diff",
    "git log",
    "git show",
    "git stash",
    "ls",
    "find",
    "grep",
    "cat",
    "head",
    "tail",
    "ps",
    "top",
    "env",
    "date",
    "time",
];

/// Maximum number of "Resolved" lines to show in diff output.
const MAX_REMOVED_LINES: usize = 20;

/// Maximum number of "New" lines to show in diff output.
const MAX_ADDED_LINES: usize = 30;

/// Returns the cache directory path: `~/.local/share/rtk/cache/`
pub fn cache_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir().context("Could not determine local data directory")?;
    Ok(data_dir.join("rtk").join("cache"))
}

/// Computes a deterministic cache key from command string and working directory.
pub fn cache_key(cmd: &str, cwd: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    cmd.hash(&mut hasher);
    cwd.hash(&mut hasher);
    hasher.finish()
}

/// Returns `false` for volatile commands whose output changes every invocation.
#[allow(dead_code)]
pub fn should_cache(cmd: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();
    for volatile in VOLATILE_COMMANDS {
        if cmd_lower == *volatile || cmd_lower.starts_with(&format!("{} ", volatile)) {
            return false;
        }
    }
    true
}

/// Loads cached output if it exists and is within the TTL.
/// Returns `Some((content, age_secs))` if valid, `None` otherwise.
/// Deletes expired entries.
pub fn load(cmd: &str, cwd: &str, ttl_minutes: u64) -> Option<(String, u64)> {
    let dir = cache_dir().ok()?;
    let key = cache_key(cmd, cwd);
    let path = dir.join(format!("{}.txt", key));

    if !path.exists() {
        return None;
    }

    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = modified.elapsed().ok()?;
    let age_secs = age.as_secs();

    if age_secs > ttl_minutes * 60 {
        // Expired — delete and return None
        let _ = std::fs::remove_file(&path);
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    Some((content, age_secs))
}

/// Stores command output in the cache.
pub fn store(cmd: &str, cwd: &str, output: &str) -> Result<()> {
    let dir = cache_dir().context("Could not determine cache directory")?;
    std::fs::create_dir_all(&dir).context("Could not create cache directory")?;

    let key = cache_key(cmd, cwd);
    let path = dir.join(format!("{}.txt", key));

    std::fs::write(&path, output).context("Could not write cache file")?;
    Ok(())
}

/// Computes a line-level diff between cached and current output.
///
/// Shows "Resolved" for removed lines, "New" for added lines,
/// or "(no changes)" if identical. Truncates at limits with "... (N more)".
pub fn diff_output(cached: &str, current: &str) -> String {
    if cached == current {
        return "(no changes)".to_string();
    }

    let cached_lines: std::collections::HashSet<&str> = cached.lines().collect();
    let current_lines: std::collections::HashSet<&str> = current.lines().collect();

    let removed: Vec<&str> = cached
        .lines()
        .filter(|line| !current_lines.contains(line))
        .collect();
    let added: Vec<&str> = current
        .lines()
        .filter(|line| !cached_lines.contains(line))
        .collect();

    if removed.is_empty() && added.is_empty() {
        return "(no changes)".to_string();
    }

    let mut parts: Vec<String> = Vec::new();

    if !removed.is_empty() {
        let shown = removed.len().min(MAX_REMOVED_LINES);
        for line in &removed[..shown] {
            parts.push(format!("Resolved: {}", line));
        }
        if removed.len() > MAX_REMOVED_LINES {
            parts.push(format!("... ({} more)", removed.len() - MAX_REMOVED_LINES));
        }
    }

    if !added.is_empty() {
        let shown = added.len().min(MAX_ADDED_LINES);
        for line in &added[..shown] {
            parts.push(format!("New: {}", line));
        }
        if added.len() > MAX_ADDED_LINES {
            parts.push(format!("... ({} more)", added.len() - MAX_ADDED_LINES));
        }
    }

    parts.join("\n")
}

/// Returns the cache TTL in minutes.
/// Priority: `RTK_CACHE_TTL` env var > config file > default (5).
pub fn get_ttl_minutes() -> u64 {
    if let Ok(val) = std::env::var("RTK_CACHE_TTL") {
        if let Ok(minutes) = val.parse::<u64>() {
            return minutes;
        }
    }

    if let Ok(config) = Config::load() {
        return config.cache.ttl_minutes;
    }

    5
}

/// Returns whether caching is enabled.
/// Priority: `RTK_CACHE` env var > config file > default (true).
pub fn is_enabled() -> bool {
    if let Ok(val) = std::env::var("RTK_CACHE") {
        return val != "0" && val.to_lowercase() != "false";
    }

    if let Ok(config) = Config::load() {
        return config.cache.enabled;
    }

    true
}

/// Removes the entire cache directory.
#[allow(dead_code)]
pub fn clear() -> Result<()> {
    let dir = cache_dir().context("Could not determine cache directory")?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).context("Could not remove cache directory")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let k1 = cache_key("cargo build", "/home/user/project");
        let k2 = cache_key("cargo build", "/home/user/project");
        assert_eq!(k1, k2, "Same inputs must produce the same key");
    }

    #[test]
    fn test_cache_key_varies_by_cwd() {
        let k1 = cache_key("cargo build", "/home/user/project-a");
        let k2 = cache_key("cargo build", "/home/user/project-b");
        assert_ne!(k1, k2, "Different cwd must produce different keys");
    }

    #[test]
    fn test_cache_key_varies_by_cmd() {
        let k1 = cache_key("cargo build", "/home/user/project");
        let k2 = cache_key("cargo test", "/home/user/project");
        assert_ne!(k1, k2, "Different commands must produce different keys");
    }

    #[test]
    fn test_should_cache_positive() {
        assert!(should_cache("cargo build"));
        assert!(should_cache("cargo test"));
        assert!(should_cache("npm install"));
        assert!(should_cache("rustc --version"));
    }

    #[test]
    fn test_should_cache_negative() {
        assert!(!should_cache("git status"));
        assert!(!should_cache("git diff"));
        assert!(!should_cache("git log"));
        assert!(!should_cache("git log --oneline -10"));
        assert!(!should_cache("git show abc123"));
        assert!(!should_cache("git stash"));
        assert!(!should_cache("ls"));
        assert!(!should_cache("ls -la"));
        assert!(!should_cache("find . -name foo"));
        assert!(!should_cache("grep pattern file"));
        assert!(!should_cache("ps"));
        assert!(!should_cache("top"));
        assert!(!should_cache("env"));
        assert!(!should_cache("date"));
        assert!(!should_cache("time"));
    }

    #[test]
    fn test_diff_identical() {
        let output = diff_output("line1\nline2\nline3", "line1\nline2\nline3");
        assert_eq!(output, "(no changes)");
    }

    #[test]
    fn test_diff_added_lines() {
        let cached = "line1\nline2";
        let current = "line1\nline2\nline3\nline4";
        let diff = diff_output(cached, current);
        assert!(diff.contains("New: line3"));
        assert!(diff.contains("New: line4"));
        assert!(!diff.contains("Resolved"));
    }

    #[test]
    fn test_diff_removed_lines() {
        let cached = "line1\nline2\nline3";
        let current = "line1";
        let diff = diff_output(cached, current);
        assert!(diff.contains("Resolved: line2"));
        assert!(diff.contains("Resolved: line3"));
        assert!(!diff.contains("New"));
    }

    #[test]
    fn test_diff_mixed_changes() {
        let cached = "error1\nerror2\nwarning1";
        let current = "error2\nwarning1\nnew_error";
        let diff = diff_output(cached, current);
        assert!(diff.contains("Resolved: error1"));
        assert!(diff.contains("New: new_error"));
    }

    #[test]
    fn test_diff_truncates_removed() {
        let cached_lines: Vec<String> = (0..25).map(|i| format!("removed_{}", i)).collect();
        let cached = cached_lines.join("\n");
        let current = "only_this";
        let diff = diff_output(&cached, current);
        assert!(
            diff.contains("... (5 more)"),
            "Should truncate removed lines at 20"
        );
    }

    #[test]
    fn test_diff_truncates_added() {
        let cached = "only_this";
        let added_lines: Vec<String> = (0..35).map(|i| format!("added_{}", i)).collect();
        let current = added_lines.join("\n");
        let diff = diff_output(cached, &current);
        assert!(
            diff.contains("... (5 more)"),
            "Should truncate added lines at 30"
        );
    }
}
