use crate::tracking;
use crate::utils::truncate;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Command;

/// JSON structure for `bun pm ls --json` output.
/// Maps package name to an object with an optional version field.
#[derive(Debug, Deserialize)]
struct BunPmPackage {
    version: Option<String>,
}

/// Filter bun install/add output — strip progress lines, version headers, empty lines.
/// Returns "ok" if nothing meaningful remains.
pub fn filter_bun_install(output: &str) -> String {
    let mut result = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip progress lines like "[1/5] ..." or "[12/34] ..."
        if trimmed.starts_with('[') && trimmed.contains(']') {
            let after_bracket = &trimmed[trimmed.find(']').unwrap() + 1..];
            if after_bracket.trim_end().ends_with("...") {
                // Verify the bracket content is N/N
                let bracket_content = &trimmed[1..trimmed.find(']').unwrap()];
                if bracket_content.contains('/') {
                    let parts: Vec<&str> = bracket_content.split('/').collect();
                    if parts.len() == 2
                        && parts[0].trim().parse::<u32>().is_ok()
                        && parts[1].trim().parse::<u32>().is_ok()
                    {
                        continue;
                    }
                }
            }
        }

        // Skip version headers like "bun install v1.1.0" or "bun add v1.2.3"
        if (trimmed.starts_with("bun install v") || trimmed.starts_with("bun add v"))
            && trimmed.split_whitespace().count() <= 4
        {
            continue;
        }

        result.push(trimmed.to_string());
    }

    if result.is_empty() {
        "ok".to_string()
    } else {
        result.join("\n")
    }
}

/// Parse JSON output from `bun pm ls --json`.
/// Returns `Some("N deps\npkg@ver\n...")` sorted alphabetically, or None if empty/parse fails.
pub fn filter_bun_pm_ls_json(raw: &str) -> Option<String> {
    let packages: HashMap<String, BunPmPackage> = serde_json::from_str(raw).ok()?;

    if packages.is_empty() {
        return None;
    }

    let mut entries: Vec<String> = packages
        .iter()
        .map(|(name, pkg)| {
            if let Some(ver) = &pkg.version {
                format!("{}@{}", name, ver)
            } else {
                name.clone()
            }
        })
        .collect();

    entries.sort();

    let count = entries.len();
    let mut result = format!("{} deps\n", count);
    result.push_str(&entries.join("\n"));

    Some(result)
}

/// Text fallback for `bun pm ls` — strip empty lines, truncate to 500 chars.
/// Returns "ok" if empty.
pub fn filter_bun_pm_ls_text(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        return "ok".to_string();
    }

    let joined = lines.join("\n");
    truncate(&joined, 500)
}

pub fn run_install(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = Command::new("bun");
    cmd.arg("install");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("Running: bun install {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("Failed to run bun install. Is bun installed?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{}\n{}", stdout, stderr);

    let filtered = filter_bun_install(&raw);
    println!("{}", filtered);

    timer.track(
        &format!("bun install {}", args.join(" ")),
        &format!("rtk bun install {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

pub fn run_pm_ls(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // Try JSON first
    let mut cmd = Command::new("bun");
    cmd.arg("pm").arg("ls").arg("--json");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("Running: bun pm ls --json {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("Failed to run bun pm ls. Is bun installed?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{}\n{}", stdout, stderr);

    let filtered = if let Some(json_result) = filter_bun_pm_ls_json(&stdout) {
        json_result
    } else {
        // Fallback to text parsing
        filter_bun_pm_ls_text(&raw)
    };

    println!("{}", filtered);

    timer.track(
        &format!("bun pm ls {}", args.join(" ")),
        &format!("rtk bun pm ls {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

pub fn run_run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = Command::new("bun");
    cmd.arg("run");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("Running: bun run {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("Failed to run bun run. Is bun installed?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{}\n{}", stdout, stderr);

    let filtered = filter_bun_install(&raw);
    println!("{}", filtered);

    timer.track(
        &format!("bun run {}", args.join(" ")),
        &format!("rtk bun run {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

pub fn run_other(args: &[OsString], verbose: u8) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("bun: no subcommand specified");
    }

    let timer = tracking::TimedExecution::start();

    let subcommand = args[0].to_string_lossy();
    let mut cmd = Command::new("bun");
    cmd.arg(&*subcommand);

    for arg in &args[1..] {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("Running: bun {} ...", subcommand);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run bun {}", subcommand))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{}\n{}", stdout, stderr);

    print!("{}", stdout);
    eprint!("{}", stderr);

    timer.track(
        &format!("bun {}", subcommand),
        &format!("rtk bun {}", subcommand),
        &raw,
        &raw, // No filtering for unsupported commands
    );

    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_bun_install_strips_progress() {
        let output = r#"bun install v1.1.0
[1/5] Resolving packages...
[2/5] Fetching packages...
[3/5] Linking packages...
[4/5] Building fresh packages...
[5/5] Cleaning up...

+ installed express@4.18.2
+ installed lodash@4.17.21
3 packages installed in 1.2s
"#;
        let result = filter_bun_install(output);
        assert!(!result.contains("[1/5]"));
        assert!(!result.contains("[2/5]"));
        assert!(!result.contains("bun install v1.1.0"));
        assert!(result.contains("express"));
        assert!(result.contains("3 packages installed"));
    }

    #[test]
    fn test_filter_bun_install_empty_output() {
        let output = "\n\n\n";
        let result = filter_bun_install(output);
        assert_eq!(result, "ok");
    }

    #[test]
    fn test_filter_bun_pm_ls_json() {
        let json = r#"{
            "express": {"version": "4.18.2"},
            "lodash": {"version": "4.17.21"},
            "axios": {"version": "1.6.0"}
        }"#;
        let result = filter_bun_pm_ls_json(json).unwrap();
        assert!(result.starts_with("3 deps"));
        // Should be sorted alphabetically
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[1], "axios@1.6.0");
        assert_eq!(lines[2], "express@4.18.2");
        assert_eq!(lines[3], "lodash@4.17.21");
    }

    #[test]
    fn test_filter_bun_pm_ls_json_empty() {
        let json = "{}";
        let result = filter_bun_pm_ls_json(json);
        assert!(result.is_none());
    }
}
