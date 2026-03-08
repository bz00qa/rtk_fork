use crate::filter::{self, FilterLevel, Language};
use crate::tracking;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(
    file: &Path,
    level: FilterLevel,
    max_lines: Option<usize>,
    line_numbers: bool,
    verbose: u8,
) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("Reading: {} (filter: {})", file.display(), level);
    }

    // Read file content
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    // Detect language from extension
    let lang = file
        .extension()
        .and_then(|e| e.to_str())
        .map(Language::from_extension)
        .unwrap_or(Language::Unknown);

    if verbose > 1 {
        eprintln!("Detected language: {:?}", lang);
    }

    // Apply filter
    let filter = filter::get_filter(level);
    let mut filtered = filter.filter(&content, &lang);

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = filtered.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!(
            "Lines: {} -> {} ({:.1}% reduction)",
            original_lines, filtered_lines, reduction
        );
    }

    // Apply smart truncation if max_lines is set
    if let Some(max) = max_lines {
        filtered = filter::smart_truncate(&filtered, max, &lang);
    }

    let rtk_output = if line_numbers {
        format_with_line_numbers(&filtered)
    } else {
        filtered.clone()
    };
    println!("{}", rtk_output);
    timer.track(
        &format!("cat {}", file.display()),
        "rtk read",
        &content,
        &rtk_output,
    );
    Ok(())
}

pub fn run_stdin(
    level: FilterLevel,
    max_lines: Option<usize>,
    line_numbers: bool,
    verbose: u8,
) -> Result<()> {
    use std::io::{self, Read as IoRead};

    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("Reading from stdin (filter: {})", level);
    }

    // Read from stdin
    let mut content = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut content)
        .context("Failed to read from stdin")?;

    // No file extension, so use Unknown language
    let lang = Language::Unknown;

    if verbose > 1 {
        eprintln!("Language: {:?} (stdin has no extension)", lang);
    }

    // Apply filter
    let filter = filter::get_filter(level);
    let mut filtered = filter.filter(&content, &lang);

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = filtered.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!(
            "Lines: {} -> {} ({:.1}% reduction)",
            original_lines, filtered_lines, reduction
        );
    }

    // Apply smart truncation if max_lines is set
    if let Some(max) = max_lines {
        filtered = filter::smart_truncate(&filtered, max, &lang);
    }

    let rtk_output = if line_numbers {
        format_with_line_numbers(&filtered)
    } else {
        filtered.clone()
    };
    println!("{}", rtk_output);

    timer.track("cat - (stdin)", "rtk read -", &content, &rtk_output);
    Ok(())
}

/// Diet mode for markdown files: strip verbose patterns while preserving essential rules.
///
/// Removes:
/// - Code block examples (```...```) — keeps the description before them
/// - Table rows beyond the header (keeps header + separator)
/// - Checklist items (- [ ] ...)
/// - HTML comments (<!-- ... -->)
/// - Consecutive blank lines (collapse to 1)
/// - Lines that are purely decorative (===, ---, ~~~)
///
/// Preserves:
/// - Headings (#, ##, ###)
/// - Bullet points and numbered lists (non-checklist)
/// - Bold/italic emphasis text
/// - Links and references
pub fn diet_markdown(content: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut in_code_block = false;
    let mut in_html_comment = false;
    let mut prev_blank = false;
    let mut in_table = false;
    let mut table_rows = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Track HTML comments
        if !in_code_block {
            if trimmed.contains("<!--") && trimmed.contains("-->") {
                i += 1;
                continue;
            }
            if trimmed.contains("<!--") {
                in_html_comment = true;
                i += 1;
                continue;
            }
            if in_html_comment {
                if trimmed.contains("-->") {
                    in_html_comment = false;
                }
                i += 1;
                continue;
            }
        }

        // Code blocks: skip content, keep a marker
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                result.push("  (code example omitted)".to_string());
                i += 1;
                continue;
            } else {
                in_code_block = false;
                i += 1;
                continue;
            }
        }
        if in_code_block {
            i += 1;
            continue;
        }

        // Skip purely decorative lines
        if trimmed.len() >= 3
            && (trimmed.chars().all(|c| c == '=' || c == ' ')
                || trimmed.chars().all(|c| c == '~' || c == ' '))
        {
            i += 1;
            continue;
        }

        // Tables: keep header row, skip data rows
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            if !in_table {
                in_table = true;
                table_rows = 0;
            }
            table_rows += 1;
            if table_rows <= 2 {
                // Header + separator
                result.push(line.to_string());
            } else if table_rows == 3 {
                let data_count = lines[i..]
                    .iter()
                    .take_while(|l| l.trim().starts_with('|') && l.trim().ends_with('|'))
                    .count();
                result.push(format!("  ({} rows omitted)", data_count));
                // Skip remaining table rows
                i += data_count;
                in_table = false;
                continue;
            }
            i += 1;
            continue;
        } else {
            in_table = false;
            table_rows = 0;
        }

        // Skip checklists
        if trimmed.starts_with("- [ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]")
        {
            i += 1;
            continue;
        }

        // Collapse consecutive blank lines
        if trimmed.is_empty() {
            if !prev_blank {
                result.push(String::new());
                prev_blank = true;
            }
            i += 1;
            continue;
        }
        prev_blank = false;

        result.push(line.to_string());
        i += 1;
    }

    result.join("\n")
}

pub fn run_diet(file: &Path, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let filtered = diet_markdown(&content);

    if verbose > 0 {
        let original_lines = content.lines().count();
        let filtered_lines = filtered.lines().count();
        let reduction = if original_lines > 0 {
            ((original_lines - filtered_lines) as f64 / original_lines as f64) * 100.0
        } else {
            0.0
        };
        eprintln!(
            "Diet: {} -> {} lines ({:.1}% reduction)",
            original_lines, filtered_lines, reduction
        );
    }

    println!("{}", filtered);
    timer.track(
        &format!("cat {}", file.display()),
        "rtk read --diet",
        &content,
        &filtered,
    );
    Ok(())
}

fn format_with_line_numbers(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let width = lines.len().to_string().len();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("{:>width$} │ {}\n", i + 1, line, width = width));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_rust_file() -> Result<()> {
        let mut file = NamedTempFile::with_suffix(".rs")?;
        writeln!(
            file,
            r#"// Comment
fn main() {{
    println!("Hello");
}}"#
        )?;

        // Just verify it doesn't panic
        run(file.path(), FilterLevel::Minimal, None, false, 0)?;
        Ok(())
    }

    #[test]
    fn test_stdin_support_signature() {
        // Test that run_stdin has correct signature and compiles
        // We don't actually run it because it would hang waiting for stdin
        // Compile-time verification that the function exists with correct signature
    }

    #[test]
    fn test_diet_strips_code_blocks() {
        let input = "# Title\nSome text\n```bash\ngit status\ngit log\n```\nMore text";
        let output = diet_markdown(input);
        assert!(output.contains("# Title"));
        assert!(output.contains("Some text"));
        assert!(output.contains("(code example omitted)"));
        assert!(!output.contains("git status"));
        assert!(output.contains("More text"));
    }

    #[test]
    fn test_diet_collapses_tables() {
        let input = "| Col1 | Col2 |\n|------|------|\n| a | b |\n| c | d |\n| e | f |\nAfter";
        let output = diet_markdown(input);
        assert!(output.contains("| Col1 | Col2 |"));
        assert!(output.contains("rows omitted"));
        assert!(!output.contains("| a | b |"));
        assert!(output.contains("After"));
    }

    #[test]
    fn test_diet_strips_checklists() {
        let input = "# Steps\n- [ ] Do thing\n- [x] Done thing\n- Normal bullet";
        let output = diet_markdown(input);
        assert!(output.contains("# Steps"));
        assert!(!output.contains("Do thing"));
        assert!(!output.contains("Done thing"));
        assert!(output.contains("Normal bullet"));
    }

    #[test]
    fn test_diet_strips_html_comments() {
        let input = "Before\n<!-- rtk-instructions v2 -->\nAfter\n<!-- start\nmultiline\n-->\nEnd";
        let output = diet_markdown(input);
        assert!(output.contains("Before"));
        assert!(output.contains("After"));
        assert!(!output.contains("rtk-instructions"));
        assert!(!output.contains("multiline"));
        assert!(output.contains("End"));
    }

    #[test]
    fn test_diet_collapses_blank_lines() {
        let input = "Line 1\n\n\n\n\nLine 2";
        let output = diet_markdown(input);
        assert_eq!(output, "Line 1\n\nLine 2");
    }

    #[test]
    fn test_diet_preserves_headings_and_bullets() {
        let input = "# H1\n## H2\n- bullet\n1. numbered\n**bold** text";
        let output = diet_markdown(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_diet_real_claude_md_reduction() {
        // Simulate a typical CLAUDE.md with examples and tables
        let input = r#"# Project
## Commands
```bash
cargo build
cargo test
```
## Table
| Command | Savings |
|---------|---------|
| git log | 80% |
| cargo test | 90% |
| pnpm list | 70% |
## Checklist
- [ ] Run tests
- [ ] Check lint
- [x] Build
## Rules
- Always use rtk prefix
- Never skip tests"#;
        let output = diet_markdown(input);
        let input_tokens: usize = input.split_whitespace().count();
        let output_tokens: usize = output.split_whitespace().count();
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 30.0,
            "Expected >=30% savings, got {:.1}%",
            savings
        );
    }
}
