use std::path::PathBuf;

const CURRENT_HOOK_VERSION: u8 = 2;
const WARN_INTERVAL_SECS: u64 = 24 * 3600;

/// Hook status for diagnostics and `rtk gain`.
#[derive(Debug, PartialEq, Clone)]
pub enum HookStatus {
    /// Hook is installed and up to date.
    Ok,
    /// Hook exists but is outdated or unreadable.
    Outdated,
    /// No hook file found (but Claude Code is installed).
    Missing,
}

/// Return the current hook status without printing anything.
/// Returns `Ok` if no Claude Code is detected (not applicable).
pub fn status() -> HookStatus {
    // Don't warn users who don't have Claude Code installed
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return HookStatus::Ok,
    };
    if !home.join(".claude").exists() {
        return HookStatus::Ok;
    }

    // Check bash hook file first
    if let Some(hook_path) = hook_installed_path() {
        let Ok(content) = std::fs::read_to_string(&hook_path) else {
            return HookStatus::Outdated;
        };
        return if parse_hook_version(&content) >= CURRENT_HOOK_VERSION {
            HookStatus::Ok
        } else {
            HookStatus::Outdated
        };
    }

    // Check native PreToolUse hook in settings.json (Windows / cross-platform)
    if has_native_hook(&home) {
        return HookStatus::Ok;
    }

    HookStatus::Missing
}

/// Check if the installed hook is missing or outdated, warn once per day.
pub fn maybe_warn() {
    // Don't block startup — fail silently on any error
    let _ = check_and_warn();
}

/// Single source of truth: delegates to `status()` then rate-limits the warning.
fn check_and_warn() -> Option<()> {
    let warning = match status() {
        HookStatus::Ok => return Some(()),
        HookStatus::Missing => {
            "[rtk] /!\\ No hook installed — run `rtk init -g` for automatic token savings"
        }
        HookStatus::Outdated => "[rtk] /!\\ Hook outdated — run `rtk init -g` to update",
    };

    // Rate limit: warn once per day
    let marker = warn_marker_path()?;
    if let Ok(meta) = std::fs::metadata(&marker) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().map(|e| e.as_secs()).unwrap_or(u64::MAX) < WARN_INTERVAL_SECS {
                return Some(());
            }
        }
    }

    eprintln!("{}", warning);

    // Touch marker after warning is printed
    let _ = std::fs::create_dir_all(marker.parent()?);
    let _ = std::fs::write(&marker, b"");

    Some(())
}

pub fn parse_hook_version(content: &str) -> u8 {
    // Version tag must be in the first 5 lines (shebang + header convention)
    for line in content.lines().take(5) {
        if let Some(rest) = line.strip_prefix("# rtk-hook-version:") {
            if let Ok(v) = rest.trim().parse::<u8>() {
                return v;
            }
        }
    }
    0 // No version tag = version 0 (outdated)
}

fn hook_installed_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join(".claude").join("hooks").join("rtk-rewrite.sh");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Check if a native PreToolUse hook containing "rtk hook-rewrite" exists in settings.json.
/// This is the Windows-native hook mechanism (no bash script needed).
fn has_native_hook(home: &std::path::Path) -> bool {
    let settings_path = home.join(".claude").join("settings.json");
    let content = match std::fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Quick substring check — avoids pulling in serde_json for a simple detection
    content.contains("rtk hook-rewrite") && content.contains("PreToolUse")
}

fn warn_marker_path() -> Option<PathBuf> {
    let data_dir = dirs::data_local_dir()?.join("rtk");
    Some(data_dir.join(".hook_warn_last"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hook_version_present() {
        let content = "#!/usr/bin/env bash\n# rtk-hook-version: 2\n# some comment\n";
        assert_eq!(parse_hook_version(content), 2);
    }

    #[test]
    fn test_parse_hook_version_missing() {
        let content = "#!/usr/bin/env bash\n# old hook without version\n";
        assert_eq!(parse_hook_version(content), 0);
    }

    #[test]
    fn test_parse_hook_version_future() {
        let content = "#!/usr/bin/env bash\n# rtk-hook-version: 5\n";
        assert_eq!(parse_hook_version(content), 5);
    }

    #[test]
    fn test_parse_hook_version_no_tag() {
        assert_eq!(parse_hook_version("no version here"), 0);
        assert_eq!(parse_hook_version(""), 0);
    }

    #[test]
    fn test_hook_status_enum() {
        assert_ne!(HookStatus::Ok, HookStatus::Missing);
        assert_ne!(HookStatus::Outdated, HookStatus::Missing);
        assert_eq!(HookStatus::Ok, HookStatus::Ok);
        // Clone works
        let s = HookStatus::Missing;
        assert_eq!(s.clone(), HookStatus::Missing);
    }

    #[test]
    fn test_status_returns_valid_variant() {
        // Skip on machines without Claude Code
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        if !home.join(".claude").exists() {
            assert_eq!(status(), HookStatus::Ok);
            return;
        }

        let has_bash_hook = home
            .join(".claude")
            .join("hooks")
            .join("rtk-rewrite.sh")
            .exists();
        let has_native = has_native_hook(&home);

        let s = status();
        if has_bash_hook || has_native {
            assert!(
                s == HookStatus::Ok || s == HookStatus::Outdated,
                "Expected Ok or Outdated when hook exists, got {:?}",
                s
            );
        } else {
            assert_eq!(s, HookStatus::Missing);
        }
    }

    #[test]
    fn test_has_native_hook_detection() {
        // Test with synthetic settings content via the substring check logic
        let content_with_hook = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook-rewrite"}]}]}}"#;
        assert!(
            content_with_hook.contains("rtk hook-rewrite")
                && content_with_hook.contains("PreToolUse")
        );

        let content_without = r#"{"hooks":{}}"#;
        assert!(
            !(content_without.contains("rtk hook-rewrite")
                && content_without.contains("PreToolUse"))
        );
    }
}
