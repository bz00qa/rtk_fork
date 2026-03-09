use crate::config;
use crate::discover::registry;
use anyhow::{Context, Result};
use std::io::Read;

/// Native Claude Code PreToolUse hook for command rewriting.
///
/// Reads the hook JSON from stdin, extracts `.tool_input.command`,
/// rewrites it via the registry, and outputs the hook response JSON.
///
/// Replaces the bash+jq hook (`hooks/rtk-rewrite.sh`) for cross-platform support.
///
/// Protocol: https://docs.anthropic.com/en/docs/claude-code/hooks
pub fn run() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("Failed to read hook input from stdin")?;

    let parsed: serde_json::Value =
        serde_json::from_str(&input).context("Failed to parse hook JSON")?;

    let cmd = match parsed
        .get("tool_input")
        .and_then(|ti| ti.get("command"))
        .and_then(|c| c.as_str())
    {
        Some(c) => c,
        None => {
            // No command field — pass through silently
            return Ok(());
        }
    };

    let excluded = config::Config::load()
        .map(|c| c.hooks.exclude_commands)
        .unwrap_or_default();

    let rewritten = match registry::rewrite_command(cmd, &excluded) {
        Some(r) if r != cmd => r,
        _ => {
            // No rewrite needed — exit silently (empty output = pass through)
            return Ok(());
        }
    };

    // Build the updated tool_input with the rewritten command
    let mut updated_input = parsed
        .get("tool_input")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    updated_input["command"] = serde_json::Value::String(rewritten);

    let response = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": "RTK auto-rewrite",
            "updatedInput": updated_input
        }
    });

    println!(
        "{}",
        serde_json::to_string(&response).context("Failed to serialize hook response")?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_hook_json_structure() {
        // Verify the JSON response structure matches Claude Code hook protocol
        let response = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "RTK auto-rewrite",
                "updatedInput": {
                    "command": "rtk git status",
                    "description": "test"
                }
            }
        });

        let output = response.get("hookSpecificOutput").unwrap();
        assert_eq!(output["hookEventName"], "PreToolUse");
        assert_eq!(output["permissionDecision"], "allow");
        assert_eq!(output["updatedInput"]["command"], "rtk git status");
    }

    #[test]
    fn test_rewrite_preserves_other_fields() {
        let input: serde_json::Value = serde_json::json!({
            "tool_input": {
                "command": "git status",
                "description": "Show working tree status",
                "timeout": 30000
            }
        });

        let cmd = input["tool_input"]["command"].as_str().unwrap();
        let rewritten = registry::rewrite_command(cmd, &[]).unwrap();

        let mut updated = input["tool_input"].clone();
        updated["command"] = serde_json::Value::String(rewritten);

        // All original fields preserved
        assert_eq!(updated["command"], "rtk git status");
        assert_eq!(updated["description"], "Show working tree status");
        assert_eq!(updated["timeout"], 30000);
    }

    #[test]
    fn test_unsupported_gets_proxy_filter() {
        let cmd = "terraform plan";
        assert_eq!(
            registry::rewrite_command(cmd, &[]),
            Some("rtk proxy -f terraform plan".into())
        );
    }

    #[test]
    fn test_no_rewrite_for_already_rtk() {
        let cmd = "rtk git status";
        let result = registry::rewrite_command(cmd, &[]).unwrap();
        // Same as input — hook should skip
        assert_eq!(result, cmd);
    }
}
