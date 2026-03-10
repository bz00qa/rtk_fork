use crate::display_helpers::{format_duration, print_period_table};
use crate::tracking::{DayStats, MonthStats, Tracker, WeekStats};
use crate::utils::format_tokens;
use anyhow::{Context, Result};
use colored::Colorize; // added: terminal colors
use serde::Serialize;
use std::io::IsTerminal; // added: TTY detection for graceful degradation
use std::path::PathBuf; // added: for project path resolution

/// Command-to-category registry for coverage reporting.
/// Categories match `--help` display_order grouping.
/// Excludes meta/internal commands (gain, discover, init, verify, etc.)
const COMMAND_REGISTRY: &[(&str, &str)] = &[
    // Git & VCS (10-12)
    ("git", "Git & VCS"),
    ("gh", "Git & VCS"),
    ("gt", "Git & VCS"),
    // Build & Compile (20-25)
    ("cargo", "Build & Compile"),
    ("tsc", "Build & Compile"),
    ("next", "Build & Compile"),
    ("lint", "Build & Compile"),
    ("prettier", "Build & Compile"),
    ("format", "Build & Compile"),
    // Test (30-33)
    ("test", "Test"),
    ("vitest", "Test"),
    ("playwright", "Test"),
    ("pytest", "Test"),
    // Languages (40-44)
    ("go", "Languages"),
    ("golangci-lint", "Languages"),
    ("ruff", "Languages"),
    ("mypy", "Languages"),
    ("pip", "Languages"),
    ("deno", "Languages"),
    // Package Managers (50-53)
    ("pnpm", "Package Managers"),
    ("npm", "Package Managers"),
    ("npx", "Package Managers"),
    ("bun", "Package Managers"),
    ("bunx", "Package Managers"),
    ("prisma", "Package Managers"),
    // Files & Search (60-66)
    ("ls", "Files & Search"),
    ("tree", "Files & Search"),
    ("read", "Files & Search"),
    ("find", "Files & Search"),
    ("grep", "Files & Search"),
    ("diff", "Files & Search"),
    ("wc", "Files & Search"),
    // Analysis & Debug (70-76)
    ("err", "Analysis & Debug"),
    ("json", "Analysis & Debug"),
    ("deps", "Analysis & Debug"),
    ("env", "Analysis & Debug"),
    ("log", "Analysis & Debug"),
    ("summary", "Analysis & Debug"),
    ("smart", "Analysis & Debug"),
    // Infrastructure (80-85)
    ("docker", "Infrastructure"),
    ("kubectl", "Infrastructure"),
    ("aws", "Infrastructure"),
    ("psql", "Infrastructure"),
    ("curl", "Infrastructure"),
    ("wget", "Infrastructure"),
    // Meta Commands (90-94)
    ("context", "Meta Commands"),
    ("dedup", "Meta Commands"),
    ("watch", "Meta Commands"),
    ("proxy", "Meta Commands"),
];

#[allow(clippy::too_many_arguments)]
pub fn run(
    project: bool, // added: per-project scope flag
    graph: bool,
    history: bool,
    quota: bool,
    tier: &str,
    daily: bool,
    weekly: bool,
    monthly: bool,
    all: bool,
    format: &str,
    failures: bool,
    _verbose: u8,
) -> Result<()> {
    let tracker = Tracker::new().context("Failed to initialize tracking database")?;
    let project_scope = resolve_project_scope(project)?; // added: resolve project path

    if failures {
        return show_failures(&tracker);
    }

    // Handle export formats
    match format {
        "json" => {
            return export_json(
                &tracker,
                daily,
                weekly,
                monthly,
                all,
                project_scope.as_deref(), // added: pass project scope
            );
        }
        "csv" => {
            return export_csv(
                &tracker,
                daily,
                weekly,
                monthly,
                all,
                project_scope.as_deref(), // added: pass project scope
            );
        }
        _ => {} // Continue with text format
    }

    let summary = tracker
        .get_summary_filtered(project_scope.as_deref()) // changed: use filtered variant
        .context("Failed to load token savings summary from database")?;

    if summary.total_commands == 0 {
        println!("No tracking data yet.");
        println!("Run some rtk commands to start tracking savings.");
        return Ok(());
    }

    // Default view (summary)
    if !daily && !weekly && !monthly && !all {
        // added: scope-aware styled header // changed: merged upstream styled + project scope
        let title = if project_scope.is_some() {
            "RTK Token Savings (Project Scope)"
        } else {
            "RTK Token Savings (Global Scope)"
        };
        println!("{}", styled(title, true));
        println!("{}", "═".repeat(60));
        // added: show project path when scoped
        if let Some(ref scope) = project_scope {
            println!("Scope: {}", shorten_path(scope));
        }
        println!();

        // added: KPI-style aligned output
        print_kpi("Total commands", summary.total_commands.to_string());
        print_kpi("Input tokens", format_tokens(summary.total_input));
        print_kpi("Output tokens", format_tokens(summary.total_output));
        print_kpi(
            "Tokens saved",
            format!(
                "{} ({:.1}%)",
                format_tokens(summary.total_saved),
                summary.avg_savings_pct
            ),
        );
        print_kpi(
            "Total exec time",
            format!(
                "{} (avg {})",
                format_duration(summary.total_time_ms),
                format_duration(summary.avg_time_ms)
            ),
        );
        print_efficiency_meter(summary.avg_savings_pct); // added: visual meter
        println!();

        if !summary.by_command.is_empty() {
            // added: styled section header
            println!("{}", styled("By Command (top 10)", true));

            // added: dynamic column widths for clean alignment
            let cmd_width = 24usize;
            let impact_width = 10usize;
            let count_width = summary
                .by_command
                .iter()
                .map(|(_, count, _, _, _)| count.to_string().len())
                .max()
                .unwrap_or(5)
                .max(5);
            let saved_width = summary
                .by_command
                .iter()
                .map(|(_, _, saved, _, _)| format_tokens(*saved).len())
                .max()
                .unwrap_or(5)
                .max(5);
            let time_width = summary
                .by_command
                .iter()
                .map(|(_, _, _, _, avg_time)| format_duration(*avg_time).len())
                .max()
                .unwrap_or(6)
                .max(6);

            let cat_width = 16usize;
            let table_width = 3
                + 2
                + cmd_width
                + 2
                + cat_width
                + 2
                + count_width
                + 2
                + saved_width
                + 2
                + 6
                + 2
                + time_width
                + 2
                + impact_width;
            println!("{}", "─".repeat(table_width));
            println!(
                "{:>3}  {:<cmd_width$}  {:<cat_width$}  {:>count_width$}  {:>saved_width$}  {:>6}  {:>time_width$}  {:<impact_width$}",
                "#", "Command", "Category", "Count", "Saved", "Avg%", "Time", "Impact",
                cmd_width = cmd_width, cat_width = cat_width, count_width = count_width,
                saved_width = saved_width, time_width = time_width,
                impact_width = impact_width
            );
            println!("{}", "─".repeat(table_width));

            let max_saved = summary
                .by_command
                .iter()
                .map(|(_, _, saved, _, _)| *saved)
                .max()
                .unwrap_or(1);

            for (idx, (cmd, count, saved, pct, avg_time)) in summary.by_command.iter().enumerate() {
                let row_idx = format!("{:>2}.", idx + 1);
                let cmd_cell = style_command_cell(&truncate_for_column(cmd, cmd_width));
                let cat = lookup_category(cmd);
                let cat_cell = truncate_for_column(cat, cat_width);
                let count_cell = format!("{:>count_width$}", count, count_width = count_width);
                let saved_cell = format!(
                    "{:>saved_width$}",
                    format_tokens(*saved),
                    saved_width = saved_width
                );
                let pct_plain = format!("{:>6}", format!("{pct:.1}%"));
                let pct_cell = colorize_pct_cell(*pct, &pct_plain);
                let time_cell = format!(
                    "{:>time_width$}",
                    format_duration(*avg_time),
                    time_width = time_width
                );
                let impact = mini_bar(*saved, max_saved, impact_width);
                println!(
                    "{}  {}  {}  {}  {}  {}  {}  {}",
                    row_idx,
                    cmd_cell,
                    cat_cell,
                    count_cell,
                    saved_cell,
                    pct_cell,
                    time_cell,
                    impact
                );
            }
            println!("{}", "─".repeat(table_width));
            println!();
        }

        // Command Coverage section
        let all_commands = tracker
            .get_by_command_all(project_scope.as_deref())
            .context("Failed to load command coverage data")?;
        print_command_coverage(&all_commands);

        // Cache performance section
        let (cache_hits, tokens_avoided) = tracker
            .get_cache_stats(project_scope.as_deref())
            .unwrap_or((0, 0));
        if cache_hits > 0 {
            println!("{}", styled("Cache Performance", true));
            let table_width = 56;
            println!("{}", "\u{2500}".repeat(table_width));
            print_kpi("Cache hits", cache_hits.to_string());
            print_kpi("Tokens avoided", format_tokens(tokens_avoided));
            let hit_rate = if summary.total_commands > 0 {
                (cache_hits as f64 / (summary.total_commands + cache_hits) as f64) * 100.0
            } else {
                0.0
            };
            print_kpi("Hit rate", format!("{:.1}%", hit_rate));
            println!("{}", "\u{2500}".repeat(table_width));
            println!();
        }

        print_routing_breakdown(&all_commands, cache_hits);

        if graph && !summary.by_day.is_empty() {
            println!("{}", styled("Daily Savings (last 30 days)", true)); // added: styled header
            println!("──────────────────────────────────────────────────────────");
            print_ascii_graph(&summary.by_day);
            println!();
        }

        if history {
            let recent = tracker.get_recent_filtered(10, project_scope.as_deref())?; // changed: filtered
            if !recent.is_empty() {
                println!("{}", styled("Recent Commands", true)); // added: styled header
                println!("──────────────────────────────────────────────────────────");
                for rec in recent {
                    let time = rec.timestamp.format("%m-%d %H:%M");
                    let cmd_short = if rec.rtk_cmd.len() > 25 {
                        format!("{}...", &rec.rtk_cmd[..22])
                    } else {
                        rec.rtk_cmd.clone()
                    };
                    // added: tier indicators by savings level
                    let sign = if rec.savings_pct >= 70.0 {
                        "▲"
                    } else if rec.savings_pct >= 30.0 {
                        "■"
                    } else {
                        "•"
                    };
                    println!(
                        "{} {} {:<25} -{:.0}% ({})",
                        time,
                        sign,
                        cmd_short,
                        rec.savings_pct,
                        format_tokens(rec.saved_tokens)
                    );
                }
                println!();
            }
        }

        if quota {
            const ESTIMATED_PRO_MONTHLY: usize = 6_000_000;

            let (quota_tokens, tier_name) = match tier {
                "pro" => (ESTIMATED_PRO_MONTHLY, "Pro ($20/mo)"),
                "5x" => (ESTIMATED_PRO_MONTHLY * 5, "Max 5x ($100/mo)"),
                "20x" => (ESTIMATED_PRO_MONTHLY * 20, "Max 20x ($200/mo)"),
                _ => (ESTIMATED_PRO_MONTHLY, "Pro ($20/mo)"),
            };

            let quota_pct = (summary.total_saved as f64 / quota_tokens as f64) * 100.0;

            println!("{}", styled("Monthly Quota Analysis", true)); // added: styled header
            println!("──────────────────────────────────────────────────────────");
            print_kpi("Subscription tier", tier_name.to_string()); // added: KPI style
            print_kpi("Estimated monthly quota", format_tokens(quota_tokens));
            print_kpi(
                "Tokens saved (lifetime)",
                format_tokens(summary.total_saved),
            );
            print_kpi("Quota preserved", format!("{:.1}%", quota_pct));
            println!();
            println!("Note: Heuristic estimate based on ~44K tokens/5h (Pro baseline)");
            println!("      Actual limits use rolling 5-hour windows, not monthly caps.");
        }

        return Ok(());
    }

    // Time breakdown views
    if all || daily {
        print_daily_full(&tracker, project_scope.as_deref())?; // changed: pass project scope
    }

    if all || weekly {
        print_weekly(&tracker, project_scope.as_deref())?; // changed: pass project scope
    }

    if all || monthly {
        print_monthly(&tracker, project_scope.as_deref())?; // changed: pass project scope
    }

    Ok(())
}

// ── Display helpers (TTY-aware) ── // added: entire section

/// Format text with bold styling (TTY-aware). // added
fn styled(text: &str, strong: bool) -> String {
    if !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    if strong {
        text.bold().green().to_string()
    } else {
        text.to_string()
    }
}

/// Print a key-value pair in KPI layout. // added
fn print_kpi(label: &str, value: String) {
    println!("{:<18} {}", format!("{label}:"), value);
}

/// Colorize percentage based on savings tier (TTY-aware). // added
fn colorize_pct_cell(pct: f64, padded: &str) -> String {
    if !std::io::stdout().is_terminal() {
        return padded.to_string();
    }
    if pct >= 70.0 {
        padded.green().bold().to_string()
    } else if pct >= 40.0 {
        padded.yellow().bold().to_string()
    } else {
        padded.red().bold().to_string()
    }
}

/// Truncate text to fit column width with ellipsis. // added
fn truncate_for_column(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let char_count = text.chars().count();
    if char_count <= width {
        return format!("{:<width$}", text, width = width);
    }
    if width <= 3 {
        return text.chars().take(width).collect();
    }
    let mut out: String = text.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

/// Style command names with cyan+bold (TTY-aware). // added
fn style_command_cell(cmd: &str) -> String {
    if !std::io::stdout().is_terminal() {
        return cmd.to_string();
    }
    cmd.bright_cyan().bold().to_string()
}

/// Render a proportional bar chart segment (TTY-aware). // added
fn mini_bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 || width == 0 {
        return String::new();
    }
    let filled = ((value as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut bar = "█".repeat(filled);
    bar.push_str(&"░".repeat(width - filled));
    if std::io::stdout().is_terminal() {
        bar.cyan().to_string()
    } else {
        bar
    }
}

/// Print an efficiency meter with colored progress bar (TTY-aware). // added
fn print_efficiency_meter(pct: f64) {
    let width = 24usize;
    let filled = (((pct / 100.0) * width as f64).round() as usize).min(width);
    let meter = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    if std::io::stdout().is_terminal() {
        let pct_str = format!("{pct:.1}%");
        let colored_pct = if pct >= 70.0 {
            pct_str.green().bold().to_string()
        } else if pct >= 40.0 {
            pct_str.yellow().bold().to_string()
        } else {
            pct_str.red().bold().to_string()
        };
        println!("Efficiency meter: {} {}", meter.green(), colored_pct);
    } else {
        println!("Efficiency meter: {} {:.1}%", meter, pct);
    }
}

/// Look up the category for a base_cmd (e.g., "rtk git status" → "Git & VCS").
fn lookup_category(base_cmd: &str) -> &'static str {
    let parts: Vec<&str> = base_cmd.split_whitespace().collect();
    if parts.len() >= 2 {
        let cmd_name = parts[1];
        if let Some(&(_, category)) = COMMAND_REGISTRY.iter().find(|&&(c, _)| c == cmd_name) {
            return category;
        }
    }
    "Other"
}

/// Resolve project scope from --project flag. // added
fn resolve_project_scope(project: bool) -> Result<Option<String>> {
    if !project {
        return Ok(None);
    }
    let cwd = std::env::current_dir().context("Failed to resolve current working directory")?;
    let canonical = cwd.canonicalize().unwrap_or(cwd);
    Ok(Some(canonical.to_string_lossy().to_string()))
}

/// Shorten long absolute paths for display. // added
fn shorten_path(path: &str) -> String {
    let path_buf = PathBuf::from(path);
    let comps: Vec<String> = path_buf
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if comps.len() <= 4 {
        return path.to_string();
    }
    let root = comps[0].as_str();
    if root == "/" || root.is_empty() {
        format!("/.../{}/{}", comps[comps.len() - 2], comps[comps.len() - 1])
    } else {
        format!(
            "{}/.../{}/{}",
            root,
            comps[comps.len() - 2],
            comps[comps.len() - 1]
        )
    }
}

/// Print command coverage table grouped by category.
/// Shows used/available, count, saved tokens, avg%, and unused command names.
fn print_command_coverage(by_command: &[(String, usize, usize, f64, u64)]) {
    const CATEGORY_ORDER: &[&str] = &[
        "Git & VCS",
        "Build & Compile",
        "Test",
        "Languages",
        "Package Managers",
        "Files & Search",
        "Analysis & Debug",
        "Infrastructure",
        "Meta Commands",
    ];

    let mut used_cmds: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (base_cmd, _, _, _, _) in by_command {
        let parts: Vec<&str> = base_cmd.split_whitespace().collect();
        if parts.len() >= 2 {
            used_cmds.insert(parts[1].to_string());
        }
    }

    struct CatStats {
        available: Vec<&'static str>,
        used: Vec<&'static str>,
        count: usize,
        saved: usize,
        total_pct: f64,
        pct_entries: usize,
    }

    let mut cats: std::collections::HashMap<&str, CatStats> = std::collections::HashMap::new();

    for &(cmd, category) in COMMAND_REGISTRY {
        let entry = cats.entry(category).or_insert_with(|| CatStats {
            available: Vec::new(),
            used: Vec::new(),
            count: 0,
            saved: 0,
            total_pct: 0.0,
            pct_entries: 0,
        });
        if !entry.available.contains(&cmd) {
            entry.available.push(cmd);
        }
        if used_cmds.contains(cmd) && !entry.used.contains(&cmd) {
            entry.used.push(cmd);
        }
    }

    for (base_cmd, count, saved, pct, _) in by_command {
        let parts: Vec<&str> = base_cmd.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let cmd_name = parts[1];
        if let Some(&(_, category)) = COMMAND_REGISTRY.iter().find(|&&(c, _)| c == cmd_name) {
            if let Some(cat) = cats.get_mut(category) {
                cat.count += count;
                cat.saved += saved;
                cat.total_pct += pct * (*count as f64);
                cat.pct_entries += count;
            }
        }
    }

    let total_used: usize = cats.values().map(|c| c.used.len()).sum();
    let total_avail: usize = cats.values().map(|c| c.available.len()).sum();
    let grand_count: usize = cats.values().map(|c| c.count).sum();
    let grand_saved: usize = cats.values().map(|c| c.saved).sum();
    let grand_pct_entries: usize = cats.values().map(|c| c.pct_entries).sum();
    let grand_avg_pct = if grand_pct_entries > 0 {
        cats.values().map(|c| c.total_pct).sum::<f64>() / grand_pct_entries as f64
    } else {
        0.0
    };

    println!(
        "{}",
        styled(
            &format!(
                "Command Coverage ({} / {} commands used)",
                total_used, total_avail
            ),
            true,
        )
    );
    let table_width = 72;
    println!("{}", "\u{2500}".repeat(table_width));
    println!(
        "  {:<20} {:>10}  {:>5}  {:>8}  {:>5}   Unused",
        "Category", "Used/Avail", "Count", "Saved", "Avg%"
    );
    println!("{}", "\u{2500}".repeat(table_width));

    // Sort categories by saved tokens descending (active categories first)
    let mut sorted_cats: Vec<&&str> = CATEGORY_ORDER.iter().collect();
    sorted_cats.sort_by(|a, b| {
        let a_saved = cats.get(**a).map(|c| c.saved).unwrap_or(0);
        let b_saved = cats.get(**b).map(|c| c.saved).unwrap_or(0);
        b_saved.cmp(&a_saved)
    });

    for cat_name in &sorted_cats {
        if let Some(cat) = cats.get(**cat_name) {
            let used_avail = format!("{} / {}", cat.used.len(), cat.available.len());
            let avg_pct = if cat.pct_entries > 0 {
                cat.total_pct / cat.pct_entries as f64
            } else {
                0.0
            };

            let unused: Vec<&str> = cat
                .available
                .iter()
                .filter(|cmd| !cat.used.contains(cmd))
                .copied()
                .collect();
            let unused_str = if unused.is_empty() {
                "\u{2014}".to_string()
            } else if unused.len() <= 3 {
                unused.join(", ")
            } else {
                format!("{}, {}...", unused[..2].join(", "), unused.len() - 2)
            };

            let pct_str = if cat.count > 0 {
                let plain = format!("{:.1}%", avg_pct);
                colorize_pct_cell(avg_pct, &plain)
            } else {
                "    \u{2014}".to_string()
            };

            let saved_str = if cat.saved > 0 {
                format_tokens(cat.saved)
            } else {
                "0".to_string()
            };

            println!(
                "  {:<20} {:>10}  {:>5}  {:>8}  {}   {}",
                **cat_name, used_avail, cat.count, saved_str, pct_str, unused_str
            );
        }
    }

    println!("{}", "\u{2500}".repeat(table_width));
    let total_used_avail = format!("{} / {} used", total_used, total_avail);
    let grand_pct_plain = format!("{:.1}%", grand_avg_pct);
    let grand_pct_cell = colorize_pct_cell(grand_avg_pct, &grand_pct_plain);
    let impact = mini_bar(grand_saved, grand_saved, 10);
    println!(
        "  {:<20} {:>10}  {:>5}  {:>8}  {}  {}",
        "Total",
        total_used_avail,
        grand_count,
        format_tokens(grand_saved),
        grand_pct_cell,
        impact
    );

    println!();
    println!("  Tip: Run 'rtk discover --all' to find missed savings opportunities");
    println!("{}", "\u{2500}".repeat(table_width));
    println!();
}

fn print_routing_breakdown(by_command: &[(String, usize, usize, f64, u64)], cache_hits: usize) {
    let mut dedicated = 0usize;
    let mut proxy = 0usize;
    let mut other = 0usize;

    for (base_cmd, count, _, _, _) in by_command {
        let parts: Vec<&str> = base_cmd.split_whitespace().collect();
        if parts.len() < 2 {
            other += count;
            continue;
        }
        let cmd_name = parts[1];
        if cmd_name == "proxy" {
            proxy += count;
        } else if COMMAND_REGISTRY.iter().any(|&(c, _)| c == cmd_name) {
            dedicated += count;
        } else {
            other += count;
        }
    }

    println!("{}", styled("Routing Breakdown", true));
    let table_width = 56;
    println!("{}", "\u{2500}".repeat(table_width));
    println!(
        "  Dedicated filters:  {:>6} commands  (specialized output)",
        dedicated
    );
    println!(
        "  Proxy auto-filter:  {:>6} commands  (ANSI/dedup/truncate)",
        proxy
    );
    if cache_hits > 0 {
        println!(
            "  Cache hits:         {:>6} commands  (repeated output skipped)",
            cache_hits
        );
    }
    if other > 0 {
        println!("  Other:              {:>6} commands", other);
    }
    println!("{}", "\u{2500}".repeat(table_width));
    println!();
}

fn print_ascii_graph(data: &[(String, usize)]) {
    if data.is_empty() {
        return;
    }

    let max_val = data.iter().map(|(_, v)| *v).max().unwrap_or(1);
    let width = 40;

    for (date, value) in data {
        let date_short = if date.len() >= 10 { &date[5..10] } else { date };

        let bar_len = if max_val > 0 {
            ((*value as f64 / max_val as f64) * width as f64) as usize
        } else {
            0
        };

        let bar: String = "█".repeat(bar_len);
        let spaces: String = " ".repeat(width - bar_len);

        println!(
            "{} │{}{} {}",
            date_short,
            bar,
            spaces,
            format_tokens(*value)
        );
    }
}

fn print_daily_full(tracker: &Tracker, project_scope: Option<&str>) -> Result<()> {
    // changed: add project scope
    let days = tracker.get_all_days_filtered(project_scope)?; // changed: use filtered variant
    print_period_table(&days);
    Ok(())
}

fn print_weekly(tracker: &Tracker, project_scope: Option<&str>) -> Result<()> {
    // changed: add project scope
    let weeks = tracker.get_by_week_filtered(project_scope)?; // changed: use filtered variant
    print_period_table(&weeks);
    Ok(())
}

fn print_monthly(tracker: &Tracker, project_scope: Option<&str>) -> Result<()> {
    // changed: add project scope
    let months = tracker.get_by_month_filtered(project_scope)?; // changed: use filtered variant
    print_period_table(&months);
    Ok(())
}

#[derive(Serialize)]
struct ExportData {
    summary: ExportSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    daily: Option<Vec<DayStats>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weekly: Option<Vec<WeekStats>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    monthly: Option<Vec<MonthStats>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage: Option<ExportCoverage>,
}

#[derive(Serialize)]
struct ExportSummary {
    total_commands: usize,
    total_input: usize,
    total_output: usize,
    total_saved: usize,
    avg_savings_pct: f64,
    total_time_ms: u64,
    avg_time_ms: u64,
}

#[derive(Serialize)]
struct ExportCoverage {
    total_used: usize,
    total_available: usize,
    categories: Vec<ExportCategoryStats>,
}

#[derive(Serialize)]
struct ExportCategoryStats {
    category: String,
    used: usize,
    available: usize,
    count: usize,
    saved_tokens: usize,
    avg_savings_pct: f64,
    unused_commands: Vec<String>,
}

fn export_json(
    tracker: &Tracker,
    daily: bool,
    weekly: bool,
    monthly: bool,
    all: bool,
    project_scope: Option<&str>, // added: project scope
) -> Result<()> {
    let summary = tracker
        .get_summary_filtered(project_scope) // changed: use filtered variant
        .context("Failed to load token savings summary from database")?;

    // Build coverage data
    let all_commands = tracker
        .get_by_command_all(project_scope)
        .context("Failed to load command coverage data for JSON export")?;

    let mut used_cmds: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (base_cmd, _, _, _, _) in &all_commands {
        let parts: Vec<&str> = base_cmd.split_whitespace().collect();
        if parts.len() >= 2 {
            used_cmds.insert(parts[1].to_string());
        }
    }

    #[allow(clippy::type_complexity)]
    let mut cat_map: std::collections::HashMap<
        &str,
        (Vec<&str>, Vec<&str>, usize, usize, f64, usize),
    > = std::collections::HashMap::new();

    for &(cmd, category) in COMMAND_REGISTRY {
        let entry = cat_map
            .entry(category)
            .or_insert_with(|| (Vec::new(), Vec::new(), 0, 0, 0.0, 0));
        if !entry.0.contains(&cmd) {
            entry.0.push(cmd); // available
        }
        if used_cmds.contains(cmd) && !entry.1.contains(&cmd) {
            entry.1.push(cmd); // used
        }
    }

    for (base_cmd, count, saved, pct, _) in &all_commands {
        let parts: Vec<&str> = base_cmd.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let cmd_name = parts[1];
        if let Some(&(_, category)) = COMMAND_REGISTRY.iter().find(|&&(c, _)| c == cmd_name) {
            if let Some(cat) = cat_map.get_mut(category) {
                cat.2 += count; // count
                cat.3 += saved; // saved
                cat.4 += pct * (*count as f64); // weighted pct
                cat.5 += count; // pct_entries
            }
        }
    }

    let total_used: usize = cat_map.values().map(|c| c.1.len()).sum();
    let total_available: usize = cat_map.values().map(|c| c.0.len()).sum();

    let category_order = [
        "Git & VCS",
        "Build & Compile",
        "Test",
        "Languages",
        "Package Managers",
        "Files & Search",
        "Analysis & Debug",
        "Infrastructure",
        "Meta Commands",
    ];

    let categories: Vec<ExportCategoryStats> = category_order
        .iter()
        .filter_map(|&cat_name| {
            cat_map
                .get(cat_name)
                .map(|(available, used, count, saved, total_pct, pct_entries)| {
                    let avg_pct = if *pct_entries > 0 {
                        *total_pct / *pct_entries as f64
                    } else {
                        0.0
                    };
                    let unused_commands: Vec<String> = available
                        .iter()
                        .filter(|cmd| !used.contains(cmd))
                        .map(|cmd| cmd.to_string())
                        .collect();
                    ExportCategoryStats {
                        category: cat_name.to_string(),
                        used: used.len(),
                        available: available.len(),
                        count: *count,
                        saved_tokens: *saved,
                        avg_savings_pct: avg_pct,
                        unused_commands,
                    }
                })
        })
        .collect();

    let coverage_data = ExportCoverage {
        total_used,
        total_available,
        categories,
    };

    let export = ExportData {
        summary: ExportSummary {
            total_commands: summary.total_commands,
            total_input: summary.total_input,
            total_output: summary.total_output,
            total_saved: summary.total_saved,
            avg_savings_pct: summary.avg_savings_pct,
            total_time_ms: summary.total_time_ms,
            avg_time_ms: summary.avg_time_ms,
        },
        daily: if all || daily {
            Some(tracker.get_all_days_filtered(project_scope)?) // changed: use filtered
        } else {
            None
        },
        weekly: if all || weekly {
            Some(tracker.get_by_week_filtered(project_scope)?) // changed: use filtered
        } else {
            None
        },
        monthly: if all || monthly {
            Some(tracker.get_by_month_filtered(project_scope)?) // changed: use filtered
        } else {
            None
        },
        coverage: Some(coverage_data),
    };

    let json = serde_json::to_string_pretty(&export)?;
    println!("{}", json);

    Ok(())
}

fn export_csv(
    tracker: &Tracker,
    daily: bool,
    weekly: bool,
    monthly: bool,
    all: bool,
    project_scope: Option<&str>, // added: project scope
) -> Result<()> {
    if all || daily {
        let days = tracker.get_all_days_filtered(project_scope)?; // changed: use filtered
        println!("# Daily Data");
        println!("date,commands,input_tokens,output_tokens,saved_tokens,savings_pct,total_time_ms,avg_time_ms");
        for day in days {
            println!(
                "{},{},{},{},{},{:.2},{},{}",
                day.date,
                day.commands,
                day.input_tokens,
                day.output_tokens,
                day.saved_tokens,
                day.savings_pct,
                day.total_time_ms,
                day.avg_time_ms
            );
        }
        println!();
    }

    if all || weekly {
        let weeks = tracker.get_by_week_filtered(project_scope)?; // changed: use filtered
        println!("# Weekly Data");
        println!(
            "week_start,week_end,commands,input_tokens,output_tokens,saved_tokens,savings_pct,total_time_ms,avg_time_ms"
        );
        for week in weeks {
            println!(
                "{},{},{},{},{},{},{:.2},{},{}",
                week.week_start,
                week.week_end,
                week.commands,
                week.input_tokens,
                week.output_tokens,
                week.saved_tokens,
                week.savings_pct,
                week.total_time_ms,
                week.avg_time_ms
            );
        }
        println!();
    }

    if all || monthly {
        let months = tracker.get_by_month_filtered(project_scope)?; // changed: use filtered
        println!("# Monthly Data");
        println!("month,commands,input_tokens,output_tokens,saved_tokens,savings_pct,total_time_ms,avg_time_ms");
        for month in months {
            println!(
                "{},{},{},{},{},{:.2},{},{}",
                month.month,
                month.commands,
                month.input_tokens,
                month.output_tokens,
                month.saved_tokens,
                month.savings_pct,
                month.total_time_ms,
                month.avg_time_ms
            );
        }
    }

    Ok(())
}

fn show_failures(tracker: &Tracker) -> Result<()> {
    let summary = tracker
        .get_parse_failure_summary()
        .context("Failed to load parse failure data")?;

    if summary.total == 0 {
        println!("No parse failures recorded.");
        println!("This means all commands parsed successfully (or fallback hasn't triggered yet).");
        return Ok(());
    }

    println!("{}", styled("RTK Parse Failures", true));
    println!("{}", "═".repeat(60));
    println!();

    print_kpi("Total failures", summary.total.to_string());
    print_kpi("Recovery rate", format!("{:.1}%", summary.recovery_rate));
    println!();

    if !summary.top_commands.is_empty() {
        println!("{}", styled("Top Commands (by frequency)", true));
        println!("{}", "─".repeat(60));
        for (cmd, count) in &summary.top_commands {
            let cmd_display = if cmd.len() > 50 {
                format!("{}...", &cmd[..47])
            } else {
                cmd.clone()
            };
            println!("  {:>4}x  {}", count, cmd_display);
        }
        println!();
    }

    if !summary.recent.is_empty() {
        println!("{}", styled("Recent Failures (last 10)", true));
        println!("{}", "─".repeat(60));
        for rec in &summary.recent {
            let ts_short = if rec.timestamp.len() >= 16 {
                &rec.timestamp[..16]
            } else {
                &rec.timestamp
            };
            let status = if rec.fallback_succeeded { "ok" } else { "FAIL" };
            let cmd_display = if rec.raw_command.len() > 40 {
                format!("{}...", &rec.raw_command[..37])
            } else {
                rec.raw_command.clone()
            };
            println!("  {} [{}] {}", ts_short, status, cmd_display);
        }
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_registry_categories_valid() {
        let valid_cats = [
            "Git & VCS",
            "Build & Compile",
            "Test",
            "Languages",
            "Package Managers",
            "Files & Search",
            "Analysis & Debug",
            "Infrastructure",
            "Meta Commands",
        ];
        for &(cmd, cat) in COMMAND_REGISTRY {
            assert!(
                valid_cats.contains(&cat),
                "Command '{}' has unknown category '{}'",
                cmd,
                cat
            );
        }
    }

    #[test]
    fn test_command_registry_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for &(cmd, _) in COMMAND_REGISTRY {
            assert!(seen.insert(cmd), "Duplicate command in registry: '{}'", cmd);
        }
    }

    #[test]
    fn test_print_command_coverage_empty() {
        // Should not panic with empty data
        print_command_coverage(&[]);
    }

    #[test]
    fn test_print_command_coverage_with_data() {
        // Should not panic with sample data
        let data = vec![
            (
                "rtk git status".to_string(),
                10usize,
                5000usize,
                70.0f64,
                100u64,
            ),
            (
                "rtk cargo test".to_string(),
                5usize,
                10000usize,
                90.0f64,
                200u64,
            ),
            ("rtk find".to_string(), 20usize, 3000usize, 80.0f64, 10u64),
        ];
        print_command_coverage(&data);
    }
}
