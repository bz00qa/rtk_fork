use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Patterns that indicate noisy/repeated lines to collapse
    static ref NOISE_PATTERNS: Vec<Regex> = vec![
        // Go
        Regex::new(r"^#\s+\S+").unwrap(),                           // "# package/path" build lines
        // Rust/Cargo
        Regex::new(r"^\s*Compiling\s+\S+\s+v").unwrap(),            // "Compiling foo v0.1.0"
        Regex::new(r"^\s*Downloading\s+\S+\s+v").unwrap(),          // "Downloading foo v0.1.0"
        Regex::new(r"^\s*Downloaded\s+\d+\s+crate").unwrap(),       // "Downloaded 47 crates"
        // npm/pnpm
        Regex::new(r"^npm warn\s").unwrap(),                        // "npm warn deprecated"
        Regex::new(r"^npm WARN\s").unwrap(),                        // "npm WARN deprecated"
        Regex::new(r"^WARN\s").unwrap(),                            // pnpm warnings
        // pip/Python
        Regex::new(r"^\s*Requirement already satisfied").unwrap(),   // pip install noise
        Regex::new(r"^\s*Collecting\s+\S+").unwrap(),                // "Collecting requests"
        Regex::new(r"^\s*Using cached\s+").unwrap(),                 // "Using cached foo-1.0.tar.gz"
        // General
        Regex::new(r"^\s*warning\[").unwrap(),                       // Rust warnings
        // Progress bars and spinners
        Regex::new(r"^\s*[\[({]?[#=\->.·]+[\])}]?\s*\d+%").unwrap(),
        Regex::new(r"^[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏✓✗●○◉◎]").unwrap(),
        // Vite/Webpack/Turbo build lines
        Regex::new(r"^\s*✓\s+\d+\s+modules?\s+transformed").unwrap(),
        Regex::new(r"^\s*\[vite\]\s+optimized\s+deps").unwrap(),
        Regex::new(r"^\s*hmr\s+update\s+").unwrap(),
        // Turbo/pnpm workspace
        Regex::new(r"^\s*cache\s+(hit|miss|bypass)").unwrap(),
        // Generic build noise
        Regex::new(r"^\s*Bundling\s+").unwrap(),
        Regex::new(r"^\s*Transforming\s+").unwrap(),
        Regex::new(r"^\s*Processing\s+").unwrap(),
        Regex::new(r"^\s*Generating\s+").unwrap(),
    ];
}

/// Collapse repeated/noisy lines in command output.
///
/// Lines matching noise patterns are grouped and replaced with a count.
/// Non-noise lines are preserved as-is.
///
/// Example: 47 "Compiling X v0.1.0" lines → "  (47 compile lines omitted)"
pub fn dedup_output(output: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check if this line matches a noise pattern
        if let Some(pattern_idx) = match_noise(line) {
            // Count consecutive lines matching the same pattern
            let start = i;
            while i < lines.len() && match_noise(lines[i]) == Some(pattern_idx) {
                i += 1;
            }
            let count = i - start;

            if count >= 3 {
                // Show first line, collapse rest
                result.push(lines[start].to_string());
                result.push(format!("  ... ({} similar lines omitted)", count - 1));
            } else {
                // Few lines — keep them
                for line_ref in &lines[start..i] {
                    result.push(line_ref.to_string());
                }
            }
        } else {
            result.push(line.to_string());
            i += 1;
        }
    }

    result.join("\n")
}

/// Deduplicate identical consecutive lines (e.g., repeated log entries).
/// Lines that appear N times consecutively are collapsed to "line (xN)".
pub fn dedup_identical(output: &str) -> String {
    let mut result = Vec::new();
    let mut lines = output.lines().peekable();

    while let Some(line) = lines.next() {
        let mut count = 1;
        while lines.peek() == Some(&line) {
            lines.next();
            count += 1;
        }
        if count > 1 {
            result.push(format!("{} (x{})", line, count));
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

/// Count noise lines that would be deduplicated.
/// Returns (total_lines, noise_lines, estimated_savings_pct).
#[allow(dead_code)]
pub fn estimate_savings(output: &str) -> (usize, usize, f64) {
    let total = output.lines().count();
    let noise = output.lines().filter(|l| match_noise(l).is_some()).count();
    let pct = if total > 0 {
        (noise as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    (total, noise, pct)
}

fn match_noise(line: &str) -> Option<usize> {
    NOISE_PATTERNS.iter().position(|p| p.is_match(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_cargo_compiling() {
        let input = "   Compiling serde v1.0.0
   Compiling serde_json v1.0.0
   Compiling clap v4.0.0
   Compiling anyhow v1.0.0
   Compiling regex v1.0.0
    Finished dev [unoptimized + debuginfo] target(s) in 5.0s";
        let output = dedup_output(input);
        assert!(output.contains("Compiling serde v1.0.0"));
        assert!(output.contains("4 similar lines omitted"));
        assert!(output.contains("Finished"));
    }

    #[test]
    fn test_dedup_npm_warnings() {
        let input = "npm warn deprecated foo@1.0.0
npm warn deprecated bar@2.0.0
npm warn deprecated baz@3.0.0
npm warn deprecated qux@4.0.0
added 200 packages in 3s";
        let output = dedup_output(input);
        assert!(output.contains("npm warn deprecated foo"));
        assert!(output.contains("3 similar lines omitted"));
        assert!(output.contains("added 200 packages"));
    }

    #[test]
    fn test_dedup_pip_satisfied() {
        let input = "Requirement already satisfied: requests in /usr/lib
Requirement already satisfied: urllib3 in /usr/lib
Requirement already satisfied: certifi in /usr/lib
Requirement already satisfied: charset-normalizer in /usr/lib
Successfully installed new-package-1.0";
        let output = dedup_output(input);
        assert!(output.contains("Requirement already satisfied: requests"));
        assert!(output.contains("3 similar lines omitted"));
        assert!(output.contains("Successfully installed"));
    }

    #[test]
    fn test_dedup_few_lines_preserved() {
        let input = "   Compiling serde v1.0.0
   Compiling clap v4.0.0
    Finished dev target(s) in 2.0s";
        let output = dedup_output(input);
        // Only 2 compile lines — below threshold, keep them
        assert!(output.contains("serde"));
        assert!(output.contains("clap"));
        assert!(!output.contains("omitted"));
    }

    #[test]
    fn test_dedup_identical() {
        let input = "Processing...
Processing...
Processing...
Done!";
        let output = dedup_identical(input);
        assert!(output.contains("Processing... (x3)"));
        assert!(output.contains("Done!"));
        assert_eq!(output.lines().count(), 2);
    }

    #[test]
    fn test_dedup_mixed_content() {
        let input = "Building project...
   Compiling a v1.0
   Compiling b v1.0
   Compiling c v1.0
   Compiling d v1.0
error[E0308]: mismatched types
  --> src/main.rs:10:5";
        let output = dedup_output(input);
        assert!(output.contains("Building project"));
        assert!(output.contains("3 similar lines omitted"));
        assert!(output.contains("error[E0308]"));
    }

    #[test]
    fn test_estimate_savings() {
        let input = "   Compiling a v1.0
   Compiling b v1.0
   Compiling c v1.0
Real output here
Another real line";
        let (total, noise, pct) = estimate_savings(input);
        assert_eq!(total, 5);
        assert_eq!(noise, 3);
        assert!((pct - 60.0).abs() < 0.1);
    }
}
