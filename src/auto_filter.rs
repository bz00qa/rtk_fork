use crate::dedup;
use crate::utils;
use lazy_static::lazy_static;
use regex::Regex;

/// Maximum output lines before auto-truncation
const MAX_LINES: usize = 500;

/// Lines to preserve from the tail when truncating.
/// Errors, summaries, and final status typically appear at the end.
const TAIL_CONTEXT: usize = 30;

/// Apply all auto-mode noise reduction filters to command output.
///
/// Pipeline:
/// 1. Strip ANSI escape codes (LLMs can't see colors)
/// 2. Collapse repeated/noisy lines (compile, warn, satisfied)
/// 3. Collapse identical consecutive lines
/// 4. Smart truncate: keep head + tail, never lose error context
///
/// `success`: if false (command failed), skip truncation entirely
///            so error messages are never lost.
///
/// Returns (filtered_output, was_truncated)
#[allow(dead_code)]
pub fn filter(output: &str) -> (String, bool) {
    filter_with_status(output, true)
}

/// Strip lines overwritten by carriage return (progress bars, spinners).
/// Only keeps the final version of each line group.
fn strip_cr_overwrites(output: &str) -> String {
    output
        .lines()
        .map(|line| {
            if let Some(pos) = line.rfind('\r') {
                &line[pos + 1..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip decorative separators (═══, ───, ***, etc.) and collapse
/// runs of 3+ blank lines into a single blank line.
fn strip_decorative(output: &str) -> String {
    lazy_static! {
        static ref DECORATOR: Regex = Regex::new(r"^[\s=═─━\-\*~_╌╍┄┅┈┉]{4,}\s*$").unwrap();
    }
    let mut result = Vec::new();
    let mut blank_count = 0u32;

    for line in output.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push(line);
            }
        } else if DECORATOR.is_match(line) {
            // Skip decorative lines entirely
        } else {
            blank_count = 0;
            result.push(line);
        }
    }

    result.join("\n")
}

/// Filter with awareness of command exit status.
/// When `success` is false, truncation is skipped to preserve error context.
pub fn filter_with_status(output: &str, success: bool) -> (String, bool) {
    // 1. Strip ANSI
    let clean = utils::strip_ansi(output);

    // 1.5. Strip carriage-return overwritten lines
    let clean = strip_cr_overwrites(&clean);

    // 1.6. Strip decorative separators and excessive blank lines
    let clean = strip_decorative(&clean);

    // 2. Noise pattern dedup
    let deduped = dedup::dedup_output(&clean);

    // 3. Identical line dedup
    let deduped = dedup::dedup_identical(&deduped);

    // 4. Smart truncate (skip if command failed — preserve all error context)
    if !success {
        return (deduped, false);
    }

    let lines: Vec<&str> = deduped.lines().collect();
    if lines.len() > MAX_LINES {
        // Keep head + tail so errors/summaries at the end are preserved
        let head_count = MAX_LINES - TAIL_CONTEXT;
        let head: String = lines[..head_count].join("\n");
        let tail: String = lines[lines.len() - TAIL_CONTEXT..].join("\n");
        let omitted = lines.len() - MAX_LINES;
        let result = format!(
            "{}\n\n... ({} lines omitted, {} total)\n\n{}",
            head,
            omitted,
            lines.len(),
            tail
        );
        (result, true)
    } else {
        (deduped, false)
    }
}

/// Lighter filter: only ANSI strip + identical dedup (no noise patterns).
/// Use for commands where noise patterns might match real content.
#[allow(dead_code)]
pub fn filter_light(output: &str) -> String {
    let clean = utils::strip_ansi(output);
    dedup::dedup_identical(&clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_strips_ansi() {
        let input = "\x1b[32mSuccess\x1b[0m\n\x1b[31mError\x1b[0m";
        let (output, _) = filter(input);
        assert!(output.contains("Success"));
        assert!(output.contains("Error"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_filter_dedup_compiling() {
        let mut input = String::new();
        for i in 1..=10 {
            input.push_str(&format!("   Compiling pkg{} v1.0.0\n", i));
        }
        input.push_str("    Finished dev target\n");
        let (output, _) = filter(&input);
        assert!(output.contains("Compiling pkg1"));
        assert!(output.contains("9 similar lines omitted"));
        assert!(output.contains("Finished"));
    }

    #[test]
    fn test_filter_truncates_large_output_keeps_tail() {
        let mut input = String::new();
        for i in 1..=600 {
            input.push_str(&format!("line {}\n", i));
        }
        let (output, truncated) = filter(&input);
        assert!(truncated);
        assert!(output.contains("omitted"));
        assert!(output.contains("line 1")); // head preserved
        assert!(output.contains("line 600")); // tail preserved
        assert!(output.contains("line 580")); // within tail context
    }

    #[test]
    fn test_filter_no_truncate_on_failure() {
        let mut input = String::new();
        for i in 1..=600 {
            input.push_str(&format!("error line {}\n", i));
        }
        let (output, truncated) = filter_with_status(&input, false);
        assert!(!truncated);
        assert!(output.contains("error line 1"));
        assert!(output.contains("error line 600"));
    }

    #[test]
    fn test_filter_small_output_unchanged() {
        let input = "hello\nworld\n";
        let (output, truncated) = filter(input);
        assert!(!truncated);
        assert_eq!(output.trim(), "hello\nworld");
    }

    #[test]
    fn test_filter_light_basic() {
        let input = "ok\nok\nok\ndone";
        let output = filter_light(input);
        assert!(output.contains("ok (x3)"));
        assert!(output.contains("done"));
    }

    #[test]
    fn test_strip_cr_overwrites() {
        let input = "Building...\rBuilding... 50%\rBuilding... 100%\rDone!";
        let result = strip_cr_overwrites(input);
        assert_eq!(result, "Done!");
    }

    #[test]
    fn test_strip_decorative_separators() {
        let input = "Header\n════════════════\nContent\n────────────────\nFooter";
        let result = strip_decorative(input);
        assert_eq!(result, "Header\nContent\nFooter");
    }

    #[test]
    fn test_strip_decorative_preserves_short_dashes() {
        let input = "file-name.rs\n--flag value";
        let result = strip_decorative(input);
        assert!(result.contains("file-name.rs"));
        assert!(result.contains("--flag value"));
    }

    #[test]
    fn test_collapse_blank_lines() {
        let input = "line1\n\n\n\n\nline2\n\nline3";
        let result = strip_decorative(input);
        let blank_count = result.lines().filter(|l| l.trim().is_empty()).count();
        // 4 consecutive blanks between line1/line2 collapse to 2,
        // 1 blank between line2/line3 stays → 3 total blanks
        assert!(
            blank_count <= 3,
            "Expected at most 3 blank lines, got {}",
            blank_count
        );
        // Verify the original 4-blank run was actually reduced
        assert!(blank_count < 5, "Blank lines were not collapsed");
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));
    }

    #[test]
    fn test_filter_build_output_savings() {
        let mut input = String::new();
        for i in 1..=20 {
            input.push_str(&format!("   Compiling pkg{} v1.0.0\n", i));
        }
        input.push_str("\n════════════════════════════════\n");
        input.push_str("✓ 42 modules transformed\n");
        input.push_str("✓ 38 modules transformed\n");
        input.push_str("✓ 15 modules transformed\n");
        input.push_str("\n\n\n\n\n");
        input.push_str("Build complete in 2.3s\n");
        input.push_str("  bundle size: 145 kB\n");

        let (output, _) = filter(&input);

        let input_tokens = input.split_whitespace().count();
        let output_tokens = output.split_whitespace().count();
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);

        assert!(
            savings >= 50.0,
            "Expected ≥50% savings on build output, got {:.1}%\nOutput:\n{}",
            savings,
            output
        );
    }
}
