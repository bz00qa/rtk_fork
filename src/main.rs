mod auto_filter;
mod aws_cmd;
mod binlog;
mod bun_cmd;
mod cache;
mod cargo_cmd;
mod cc_economics;
mod ccusage;
mod config;
mod container;
mod context_cmd;
mod curl_cmd;
mod dedup;
mod deno_cmd;
mod deps;
mod diff_cmd;
mod discover;
mod display_helpers;
mod dotnet_cmd;
mod dotnet_format_report;
mod dotnet_trx;
mod env_cmd;
mod filter;
mod find_cmd;
mod format_cmd;
mod gain;
mod gh_cmd;
mod git;
mod go_cmd;
mod golangci_cmd;
mod grep_cmd;
mod gt_cmd;
mod hook_audit_cmd;
mod hook_check;
mod hook_rewrite_cmd;
mod init;
mod integrity;
mod json_cmd;
mod learn;
mod lint_cmd;
mod local_llm;
mod log_cmd;
mod ls;
mod mypy_cmd;
mod next_cmd;
mod npm_cmd;
mod parser;
mod pip_cmd;
mod playwright_cmd;
mod pnpm_cmd;
mod prettier_cmd;
mod prisma_cmd;
mod psql_cmd;
mod pytest_cmd;
mod read;
mod rewrite_cmd;
mod ruff_cmd;
mod runner;
mod summary;
mod tee;
mod telemetry;
mod toml_filter;
mod tracking;
mod tree;
mod tsc_cmd;
mod utils;
mod verify_cmd;
mod vitest_cmd;
mod watch_cmd;
mod wc_cmd;
mod wget_cmd;

use anyhow::{Context, Result};
use clap::error::ErrorKind;
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "rtk",
    version,
    about = "Rust Token Killer - Minimize LLM token consumption",
    long_about = "A high-performance CLI proxy designed to filter and summarize system outputs before they reach your LLM context."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Ultra-compact mode: ASCII icons, inline format (Level 2 optimizations)
    #[arg(short = 'u', long, global = true)]
    ultra_compact: bool,

    /// Set SKIP_ENV_VALIDATION=1 for child processes (Next.js, tsc, lint, prisma)
    #[arg(long = "skip-env", global = true)]
    skip_env: bool,
}

#[derive(Subcommand)]
enum Commands {
    // ── Git & VCS ───────────────────────────────────────────
    /// Git commands with compact output
    #[command(display_order = 10)]
    Git {
        /// Change to directory before executing (like git -C <path>, can be repeated)
        #[arg(short = 'C', action = clap::ArgAction::Append)]
        directory: Vec<String>,

        /// Git configuration override (like git -c key=value, can be repeated)
        #[arg(short = 'c', action = clap::ArgAction::Append)]
        config_override: Vec<String>,

        /// Set the path to the .git directory
        #[arg(long = "git-dir")]
        git_dir: Option<String>,

        /// Set the path to the working tree
        #[arg(long = "work-tree")]
        work_tree: Option<String>,

        /// Disable pager (like git --no-pager)
        #[arg(long = "no-pager")]
        no_pager: bool,

        /// Skip optional locks (like git --no-optional-locks)
        #[arg(long = "no-optional-locks")]
        no_optional_locks: bool,

        /// Treat repository as bare (like git --bare)
        #[arg(long)]
        bare: bool,

        /// Treat pathspecs literally (like git --literal-pathspecs)
        #[arg(long = "literal-pathspecs")]
        literal_pathspecs: bool,

        #[command(subcommand)]
        command: GitCommands,
    },

    /// GitHub CLI (gh) commands with token-optimized output
    #[command(display_order = 11)]
    Gh {
        /// Subcommand: pr, issue, run, repo
        subcommand: String,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Graphite (gt) stacked PR commands with compact output
    #[command(display_order = 12)]
    Gt {
        #[command(subcommand)]
        command: GtCommands,
    },

    // ── Build & Compile ─────────────────────────────────────
    /// Cargo commands with compact output
    #[command(display_order = 20)]
    Cargo {
        #[command(subcommand)]
        command: CargoCommands,
    },

    /// TypeScript compiler with grouped error output
    #[command(display_order = 21)]
    Tsc {
        /// TypeScript compiler arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Next.js build with compact output
    #[command(display_order = 22)]
    Next {
        /// Next.js build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// ESLint with grouped rule violations
    #[command(display_order = 23)]
    Lint {
        /// Linter arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Prettier format checker with compact output
    #[command(display_order = 24)]
    Prettier {
        /// Prettier arguments (e.g., --check, --write)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Universal format checker (prettier, black, ruff format)
    #[command(display_order = 25)]
    Format {
        /// Formatter arguments (auto-detects formatter from project files)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Test ─────────────────────────────────────────────────
    /// Run tests and show only failures (generic)
    #[command(display_order = 30)]
    Test {
        /// Test command (e.g. cargo test)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Vitest commands with compact output
    #[command(display_order = 31)]
    Vitest {
        #[command(subcommand)]
        command: VitestCommands,
    },

    /// Playwright E2E tests with compact output
    #[command(display_order = 32)]
    Playwright {
        /// Playwright arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Pytest test runner with compact output
    #[command(display_order = 33)]
    Pytest {
        /// Pytest arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Languages ────────────────────────────────────────────
    /// Go commands with compact output
    #[command(display_order = 40)]
    Go {
        #[command(subcommand)]
        command: GoCommands,
    },

    /// golangci-lint with compact output
    #[command(name = "golangci-lint", display_order = 41)]
    GolangciLint {
        /// golangci-lint arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Ruff linter/formatter with compact output
    #[command(display_order = 42)]
    Ruff {
        /// Ruff arguments (e.g., check, format --check)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Mypy type checker with grouped error output
    #[command(display_order = 43)]
    Mypy {
        /// Mypy arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Deno runtime commands with compact output
    #[command(display_order = 43)]
    Deno {
        #[command(subcommand)]
        command: DenoCommands,
    },

    /// Pip package manager with compact output (auto-detects uv)
    #[command(display_order = 44)]
    Pip {
        /// Pip arguments (e.g., list, outdated, install)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Package Managers ─────────────────────────────────────
    /// pnpm commands with ultra-compact output
    #[command(display_order = 50)]
    Pnpm {
        #[command(subcommand)]
        command: PnpmCommands,
    },

    /// npm run with filtered output (strip boilerplate)
    #[command(display_order = 51)]
    Npm {
        /// npm run arguments (script name + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// npx with intelligent routing (tsc, eslint, prisma -> specialized filters)
    #[command(display_order = 52)]
    Npx {
        /// npx arguments (command + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Prisma commands with compact output (no ASCII art)
    #[command(display_order = 53)]
    Prisma {
        #[command(subcommand)]
        command: PrismaCommands,
    },

    /// Bun runtime commands with compact output
    #[command(display_order = 54)]
    Bun {
        #[command(subcommand)]
        command: BunCommands,
    },

    /// bunx with passthrough + auto-filter
    #[command(display_order = 55)]
    Bunx {
        /// bunx arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Files & Search ───────────────────────────────────────
    /// List directory contents with token-optimized output
    #[command(display_order = 60)]
    Ls {
        /// Arguments passed to ls (supports all native ls flags like -l, -a, -h, -R)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Directory tree with token-optimized output
    #[command(display_order = 61)]
    Tree {
        /// Arguments passed to tree (supports all native tree flags like -L, -d, -a)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Read file with intelligent filtering
    #[command(display_order = 62)]
    Read {
        /// File to read
        file: PathBuf,
        /// Filter: none, minimal, aggressive
        #[arg(short, long, default_value = "minimal")]
        level: filter::FilterLevel,
        /// Max lines
        #[arg(short, long, conflicts_with = "tail_lines")]
        max_lines: Option<usize>,
        /// Keep only last N lines
        #[arg(long, conflicts_with = "max_lines")]
        tail_lines: Option<usize>,
        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,
        /// Markdown diet mode: strip examples, collapse tables, remove verbose sections
        #[arg(short, long)]
        diet: bool,
    },

    /// Find files with compact tree output (accepts native find flags)
    #[command(display_order = 63)]
    Find {
        /// All find arguments (supports both RTK and native find syntax)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Compact grep - strips whitespace, truncates, groups by file
    #[command(display_order = 64)]
    Grep {
        /// Pattern to search
        pattern: String,
        /// Path to search in
        #[arg(default_value = ".")]
        path: String,
        /// Max line length
        #[arg(short = 'l', long, default_value = "80")]
        max_len: usize,
        /// Max results to show
        #[arg(short, long, default_value = "50")]
        max: usize,
        /// Show only match context (not full line)
        #[arg(short, long)]
        context_only: bool,
        /// Filter by file type (e.g., ts, py, rust)
        #[arg(short = 't', long)]
        file_type: Option<String>,
        /// Show line numbers (always on, accepted for grep/rg compatibility)
        #[arg(short = 'n', long)]
        line_numbers: bool,
        /// Extra ripgrep arguments (e.g., -i, -A 3, -w, --glob)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },

    /// Ultra-condensed diff (only changed lines)
    #[command(display_order = 65)]
    Diff {
        /// First file or - for stdin (unified diff)
        file1: PathBuf,
        /// Second file (optional if stdin)
        file2: Option<PathBuf>,
    },

    /// Word/line/byte count with compact output (strips paths and padding)
    #[command(display_order = 66)]
    Wc {
        /// Arguments passed to wc (files, flags like -l, -w, -c)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Analysis & Debug ─────────────────────────────────────
    /// Run command and show only errors/warnings
    #[command(display_order = 70)]
    Err {
        /// Command to run
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Show JSON structure without values
    #[command(display_order = 71)]
    Json {
        /// JSON file
        file: PathBuf,
        /// Max depth
        #[arg(short, long, default_value = "5")]
        depth: usize,
    },

    /// Summarize project dependencies
    #[command(display_order = 72)]
    Deps {
        /// Project path
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Show environment variables (filtered, sensitive masked)
    #[command(display_order = 73)]
    Env {
        /// Filter by name (e.g. PATH, AWS)
        #[arg(short, long)]
        filter: Option<String>,
        /// Show all (include sensitive)
        #[arg(long)]
        show_all: bool,
    },

    /// Filter and deduplicate log output
    #[command(display_order = 74)]
    Log {
        /// Log file (omit for stdin)
        file: Option<PathBuf>,
    },

    /// Run command and show heuristic summary
    #[command(display_order = 75)]
    Summary {
        /// Command to run and summarize
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Generate 2-line technical summary (heuristic-based)
    #[command(display_order = 76)]
    Smart {
        /// File to analyze
        file: PathBuf,
        /// Model: heuristic
        #[arg(short, long, default_value = "heuristic")]
        model: String,
        /// Force model download
        #[arg(long)]
        force_download: bool,
    },

    // ── Infrastructure ───────────────────────────────────────
    /// .NET commands with compact output (build/test/restore/format)
    #[command(display_order = 79)]
    Dotnet {
        #[command(subcommand)]
        command: DotnetCommands,
    },

    /// Docker commands with compact output
    #[command(display_order = 80)]
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },

    /// Kubectl commands with compact output
    #[command(display_order = 81)]
    Kubectl {
        #[command(subcommand)]
        command: KubectlCommands,
    },

    /// AWS CLI with compact output (force JSON, compress)
    #[command(display_order = 82)]
    Aws {
        /// AWS service subcommand (e.g., sts, s3, ec2, ecs, rds, cloudformation)
        subcommand: String,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// PostgreSQL client with compact output (strip borders, compress tables)
    #[command(display_order = 83)]
    Psql {
        /// psql arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Curl with auto-JSON detection and schema output
    #[command(display_order = 84)]
    Curl {
        /// Curl arguments (URL + options)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Download with compact output (strips progress bars)
    #[command(display_order = 85)]
    Wget {
        /// URL to download
        url: String,
        /// Output to stdout instead of file
        #[arg(short = 'O', long)]
        stdout: bool,
        /// Additional wget arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    // ── Meta Commands ────────────────────────────────────────
    /// Combined git context: status + diff + log in one call
    #[command(display_order = 90)]
    Context {
        /// Max recent commits to show
        #[arg(short = 'n', long, default_value = "5")]
        log_count: usize,
    },

    /// Collapse repeated/noisy lines in command output (pipe or run)
    #[command(display_order = 91)]
    Dedup {
        /// Command to run and deduplicate (omit to read from stdin)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Run command and show only changes since last run (iterative dev)
    #[command(display_order = 92)]
    Watch {
        /// Command to run and diff against previous output
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },

    /// Clear watch cache (reset diff history)
    #[command(name = "watch-clear", display_order = 93)]
    WatchClear,

    /// Execute command with auto noise reduction (ANSI strip, dedup, truncate)
    #[command(display_order = 94)]
    Proxy {
        /// Apply auto noise reduction (ANSI strip, dedup, truncate)
        #[arg(short = 'f', long)]
        filter: bool,
        /// Command and arguments to execute
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },

    // ── Stats & Config ───────────────────────────────────────
    /// Show token savings summary and history
    #[command(display_order = 100)]
    Gain {
        /// Filter statistics to current project (current working directory)
        #[arg(short, long)]
        project: bool,
        /// Show ASCII graph of daily savings
        #[arg(short, long)]
        graph: bool,
        /// Show recent command history
        #[arg(short = 'H', long)]
        history: bool,
        /// Show monthly quota savings estimate
        #[arg(short, long)]
        quota: bool,
        /// Subscription tier for quota calculation: pro, 5x, 20x
        #[arg(short, long, default_value = "20x", requires = "quota")]
        tier: String,
        /// Show detailed daily breakdown (all days)
        #[arg(short, long)]
        daily: bool,
        /// Show weekly breakdown
        #[arg(short, long)]
        weekly: bool,
        /// Show monthly breakdown
        #[arg(short, long)]
        monthly: bool,
        /// Show all time breakdowns (daily + weekly + monthly)
        #[arg(short, long)]
        all: bool,
        /// Output format: text, json, csv
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Show parse failure log (commands that fell back to raw execution)
        #[arg(short = 'F', long)]
        failures: bool,
    },

    /// Discover missed RTK savings from Claude Code history
    #[command(display_order = 101)]
    Discover {
        /// Filter by project path (substring match)
        #[arg(short, long)]
        project: Option<String>,
        /// Max commands per section
        #[arg(short, long, default_value = "15")]
        limit: usize,
        /// Scan all projects (default: current project only)
        #[arg(short, long)]
        all: bool,
        /// Limit to sessions from last N days
        #[arg(short, long, default_value = "30")]
        since: u64,
        /// Output format: text, json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Claude Code economics: spending (ccusage) vs savings (rtk) analysis
    #[command(display_order = 102)]
    CcEconomics {
        /// Show detailed daily breakdown
        #[arg(short, long)]
        daily: bool,
        /// Show weekly breakdown
        #[arg(short, long)]
        weekly: bool,
        /// Show monthly breakdown
        #[arg(short, long)]
        monthly: bool,
        /// Show all time breakdowns (daily + weekly + monthly)
        #[arg(short, long)]
        all: bool,
        /// Output format: text, json, csv
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Initialize rtk instructions in CLAUDE.md
    #[command(display_order = 103)]
    Init {
        /// Add to global ~/.claude/CLAUDE.md instead of local
        #[arg(short, long)]
        global: bool,

        /// Install OpenCode plugin (in addition to Claude Code)
        #[arg(long)]
        opencode: bool,

        /// Show current configuration
        #[arg(long)]
        show: bool,

        /// Inject full instructions into CLAUDE.md (legacy mode)
        #[arg(long = "claude-md", group = "mode")]
        claude_md: bool,

        /// Hook only, no RTK.md
        #[arg(long = "hook-only", group = "mode")]
        hook_only: bool,

        /// Auto-patch settings.json without prompting
        #[arg(long = "auto-patch", group = "patch")]
        auto_patch: bool,

        /// Skip settings.json patching (print manual instructions)
        #[arg(long = "no-patch", group = "patch")]
        no_patch: bool,

        /// Remove all RTK artifacts (hook, RTK.md, CLAUDE.md reference, settings.json entry)
        #[arg(long)]
        uninstall: bool,
    },

    /// Show or create configuration file
    #[command(display_order = 104)]
    Config {
        /// Create default config file
        #[arg(long)]
        create: bool,
    },

    /// Learn CLI corrections from Claude Code error history
    #[command(display_order = 105)]
    Learn {
        /// Filter by project path (substring match)
        #[arg(short, long)]
        project: Option<String>,
        /// Scan all projects (default: current project only)
        #[arg(short, long)]
        all: bool,
        /// Limit to sessions from last N days
        #[arg(short, long, default_value = "30")]
        since: u64,
        /// Output format: text, json
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Generate .claude/rules/cli-corrections.md file
        #[arg(short, long)]
        write_rules: bool,
        /// Minimum confidence threshold (0.0-1.0)
        #[arg(long, default_value = "0.6")]
        min_confidence: f64,
        /// Minimum occurrences to include in report
        #[arg(long, default_value = "1")]
        min_occurrences: usize,
    },

    // ── Internals ────────────────────────────────────────────
    /// Verify hook integrity and run TOML filter inline tests
    #[command(display_order = 200)]
    Verify {
        /// Run tests only for this filter name
        #[arg(long)]
        filter: Option<String>,
        /// Fail if any filter has no inline tests (CI mode)
        #[arg(long)]
        require_all: bool,
    },

    /// Show hook rewrite audit metrics (requires RTK_HOOK_AUDIT=1)
    #[command(name = "hook-audit", display_order = 201)]
    HookAudit {
        /// Show entries from last N days (0 = all time)
        #[arg(short, long, default_value = "7")]
        since: u64,
    },

    /// Rewrite a raw command to its RTK equivalent (single source of truth for hooks)
    #[command(display_order = 202)]
    Rewrite {
        /// Raw command to rewrite (e.g. "git status", "cargo test && git push")
        /// Accepts multiple args: `rtk rewrite ls -al` is equivalent to `rtk rewrite "ls -al"`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Native Claude Code PreToolUse hook (reads JSON from stdin, no jq needed)
    #[command(name = "hook-rewrite", display_order = 203)]
    HookRewrite,
}

#[derive(Subcommand)]
enum GitCommands {
    /// Condensed diff output
    Diff {
        /// Git arguments (supports all git diff flags like --stat, --cached, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// One-line commit history
    Log {
        /// Git arguments (supports all git log flags like --oneline, --graph, --all)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact status (supports all git status flags)
    Status {
        /// Git arguments (supports all git status flags like --porcelain, --short, -s)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact show (commit summary + stat + compacted diff)
    Show {
        /// Git arguments (supports all git show flags)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add files → "ok ✓"
    Add {
        /// Files and flags to add (supports all git add flags like -A, -p, --all, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Commit → "ok ✓ \<hash\>"
    Commit {
        /// Git commit arguments (supports -a, -m, --amend, --allow-empty, etc)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Push → "ok ✓ \<branch\>"
    Push {
        /// Git push arguments (supports -u, remote, branch, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Pull → "ok ✓ \<stats\>"
    Pull {
        /// Git pull arguments (supports --rebase, remote, branch, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact branch listing (current/local/remote)
    Branch {
        /// Git branch arguments (supports -d, -D, -m, etc.)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Fetch → "ok fetched (N new refs)"
    Fetch {
        /// Git fetch arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Stash management (list, show, pop, apply, drop)
    Stash {
        /// Subcommand: list, show, pop, apply, drop, push
        subcommand: Option<String>,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact worktree listing
    Worktree {
        /// Git worktree arguments (add, remove, prune, or empty for list)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported git subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum PnpmCommands {
    /// List installed packages (ultra-dense)
    List {
        /// Depth level (default: 0)
        #[arg(short, long, default_value = "0")]
        depth: usize,
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Show outdated packages (condensed: "pkg: old → new")
    Outdated {
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install packages (filter progress bars)
    Install {
        /// Packages to install
        packages: Vec<String>,
        /// Additional pnpm arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Build (generic passthrough, no framework-specific filter)
    Build {
        /// Additional build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Typecheck (delegates to tsc filter)
    Typecheck {
        /// Additional typecheck arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported pnpm subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum DockerCommands {
    /// List running containers
    Ps,
    /// List images
    Images,
    /// Show container logs (deduplicated)
    Logs { container: String },
    /// Docker Compose commands with compact output
    Compose {
        #[command(subcommand)]
        command: ComposeCommands,
    },
    /// Passthrough: runs any unsupported docker subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// List compose services (compact)
    Ps,
    /// Show compose logs (deduplicated)
    Logs {
        /// Optional service name
        service: Option<String>,
    },
    /// Build compose services (summary)
    Build {
        /// Optional service name
        service: Option<String>,
    },
    /// Passthrough: runs any unsupported compose subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum KubectlCommands {
    /// List pods
    Pods {
        #[arg(short, long)]
        namespace: Option<String>,
        /// All namespaces
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// List services
    Services {
        #[arg(short, long)]
        namespace: Option<String>,
        /// All namespaces
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// Show pod logs (deduplicated)
    Logs {
        pod: String,
        #[arg(short, long)]
        container: Option<String>,
    },
    /// Passthrough: runs any unsupported kubectl subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum VitestCommands {
    /// Run tests with filtered output (90% token reduction)
    Run {
        /// Additional vitest arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PrismaCommands {
    /// Generate Prisma Client (strip ASCII art)
    Generate {
        /// Additional prisma arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage migrations
    Migrate {
        #[command(subcommand)]
        command: PrismaMigrateCommands,
    },
    /// Push schema to database
    DbPush {
        /// Additional prisma arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum PrismaMigrateCommands {
    /// Create and apply migration
    Dev {
        /// Migration name
        #[arg(short, long)]
        name: Option<String>,
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Check migration status
    Status {
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Deploy migrations to production
    Deploy {
        /// Additional arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CargoCommands {
    /// Build with compact output (strip Compiling lines, keep errors)
    Build {
        /// Additional cargo build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Test with failures-only output
    Test {
        /// Additional cargo test arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Clippy with warnings grouped by lint rule
    Clippy {
        /// Additional cargo clippy arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Check with compact output (strip Checking lines, keep errors)
    Check {
        /// Additional cargo check arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install with compact output (strip dep compilation, keep installed/errors)
    Install {
        /// Additional cargo install arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Nextest with failures-only output
    Nextest {
        /// Additional cargo nextest arguments (e.g., run, list, --lib)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported cargo subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum DotnetCommands {
    /// Build with compact output
    Build {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Test with compact output
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Restore with compact output
    Restore {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Format with compact output
    Format {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported dotnet subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum GoCommands {
    /// Run tests with compact output (90% token reduction via JSON streaming)
    Test {
        /// Additional go test arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Build with compact output (errors only)
    Build {
        /// Additional go build arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Vet with compact output
    Vet {
        /// Additional go vet arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported go subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum BunCommands {
    /// Install packages (filter progress bars)
    Install {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run scripts
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Build project (errors only)
    Build {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Test with compact output (failures only)
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add packages
    Add {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Remove packages
    Remove {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Package manager commands (pm ls, etc.)
    Pm {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: runs any unsupported bun subcommand directly
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

#[derive(Subcommand)]
enum DenoCommands {
    /// Run a script
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Type-check without running
    Check {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Lint source files
    Lint {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run tests (failures only)
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run a task from deno.json
    Task {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compile to standalone executable (errors only)
    Compile {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install dependencies
    Install {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

/// RTK-only subcommands that should never fall back to raw execution.
/// If Clap fails to parse these, show the Clap error directly.
const RTK_META_COMMANDS: &[&str] = &[
    "gain",
    "discover",
    "learn",
    "init",
    "config",
    "proxy",
    "hook-audit",
    "cc-economics",
    "verify",
];

fn run_fallback(parse_error: clap::Error) -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // No args → show Clap's error (user ran just "rtk" with bad syntax)
    if args.is_empty() {
        parse_error.exit();
    }

    // RTK meta-commands should never fall back to raw execution.
    // e.g. `rtk gain --badtypo` should show Clap's error, not try to run `gain` from $PATH.
    if RTK_META_COMMANDS.contains(&args[0].as_str()) {
        parse_error.exit();
    }

    let raw_command = args.join(" ");
    let error_message = utils::strip_ansi(&parse_error.to_string());

    // Start timer before execution to capture actual command runtime
    let timer = tracking::TimedExecution::start();

    // TOML filter lookup — bypass with RTK_NO_TOML=1
    // Use basename of args[0] so absolute paths (/usr/bin/make) still match "^make\b".
    let lookup_cmd = {
        let base = std::path::Path::new(&args[0])
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| args[0].clone());
        std::iter::once(base.as_str())
            .chain(args[1..].iter().map(|s| s.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let toml_match = if std::env::var("RTK_NO_TOML").ok().as_deref() == Some("1") {
        None
    } else {
        toml_filter::find_matching_filter(&lookup_cmd)
    };

    if let Some(filter) = toml_match {
        // TOML match: capture stdout for filtering
        let result = std::process::Command::new(&args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped()) // capture
            .stderr(std::process::Stdio::inherit()) // stderr always direct
            .output();

        match result {
            Ok(output) => {
                let stdout_raw = String::from_utf8_lossy(&output.stdout);

                // Tee raw output BEFORE filtering on failure — lets LLM re-read if needed
                let tee_hint = if !output.status.success() {
                    tee::tee_and_hint(&stdout_raw, &raw_command, output.status.code().unwrap_or(1))
                } else {
                    None
                };

                let filtered = toml_filter::apply_filter(filter, &stdout_raw);
                println!("{}", filtered);
                if let Some(hint) = tee_hint {
                    println!("{}", hint);
                }

                timer.track(
                    &raw_command,
                    &format!("rtk:toml {}", raw_command),
                    &stdout_raw,
                    &filtered,
                );
                tracking::record_parse_failure_silent(&raw_command, &error_message, true);

                if !output.status.success() {
                    std::process::exit(output.status.code().unwrap_or(1));
                }
            }
            Err(e) => {
                // Command not found — same behaviour as no-TOML path
                tracking::record_parse_failure_silent(&raw_command, &error_message, false);
                eprintln!("[rtk: {}]", e);
                std::process::exit(127);
            }
        }
    } else {
        // No TOML match: original passthrough behaviour (Stdio::inherit, streaming)
        let status = std::process::Command::new(&args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();

        match status {
            Ok(s) => {
                timer.track_passthrough(&raw_command, &format!("rtk fallback: {}", raw_command));

                tracking::record_parse_failure_silent(&raw_command, &error_message, true);

                if !s.success() {
                    std::process::exit(s.code().unwrap_or(1));
                }
            }
            Err(e) => {
                tracking::record_parse_failure_silent(&raw_command, &error_message, false);
                // Command not found or other OS error — single message, no duplicate Clap error
                eprintln!("[rtk: {}]", e);
                std::process::exit(127);
            }
        }
    }

    Ok(())
}

#[derive(Subcommand)]
enum GtCommands {
    /// Compact stack log output
    Log {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact submit output
    Submit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact sync output
    Sync {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact restack output
    Restack {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Compact create output
    Create {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Branch info and management
    Branch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Passthrough: git-passthrough detection or direct gt execution
    #[command(external_subcommand)]
    Other(Vec<OsString>),
}

/// Split a string into shell-like tokens, respecting single and double quotes.
/// e.g. `git log --format="%H %s"` → ["git", "log", "--format=%H %s"]
fn shell_split(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for c in input.chars() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn main() -> Result<()> {
    // Fire-and-forget telemetry ping (1/day, non-blocking)
    telemetry::maybe_ping();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                e.exit();
            }
            return run_fallback(e);
        }
    };

    // Warn if installed hook is outdated/missing (1/day, non-blocking).
    // Skip for Gain — it shows its own inline hook warning.
    if !matches!(cli.command, Commands::Gain { .. }) {
        hook_check::maybe_warn();
    }

    // Runtime integrity check for operational commands.
    // Meta commands (init, gain, verify, config, etc.) skip the check
    // because they don't go through the hook pipeline.
    if is_operational_command(&cli.command) {
        integrity::runtime_check()?;
    }

    match cli.command {
        Commands::Ls { args } => {
            ls::run(&args, cli.verbose)?;
        }

        Commands::Tree { args } => {
            tree::run(&args, cli.verbose)?;
        }

        Commands::Read {
            file,
            level,
            max_lines,
            tail_lines,
            line_numbers,
            diet,
        } => {
            if file == Path::new("-") {
                read::run_stdin(level, max_lines, tail_lines, line_numbers, cli.verbose)?;
            } else if diet {
                read::run_diet(&file, cli.verbose)?;
            } else {
                read::run(
                    &file,
                    level,
                    max_lines,
                    tail_lines,
                    line_numbers,
                    cli.verbose,
                )?;
            }
        }

        Commands::Smart {
            file,
            model,
            force_download,
        } => {
            local_llm::run(&file, &model, force_download, cli.verbose)?;
        }

        Commands::Git {
            directory,
            config_override,
            git_dir,
            work_tree,
            no_pager,
            no_optional_locks,
            bare,
            literal_pathspecs,
            command,
        } => {
            // Build global git args (inserted between "git" and subcommand)
            let mut global_args: Vec<String> = Vec::new();
            for dir in &directory {
                global_args.push("-C".to_string());
                global_args.push(dir.clone());
            }
            for cfg in &config_override {
                global_args.push("-c".to_string());
                global_args.push(cfg.clone());
            }
            if let Some(ref dir) = git_dir {
                global_args.push("--git-dir".to_string());
                global_args.push(dir.clone());
            }
            if let Some(ref tree) = work_tree {
                global_args.push("--work-tree".to_string());
                global_args.push(tree.clone());
            }
            if no_pager {
                global_args.push("--no-pager".to_string());
            }
            if no_optional_locks {
                global_args.push("--no-optional-locks".to_string());
            }
            if bare {
                global_args.push("--bare".to_string());
            }
            if literal_pathspecs {
                global_args.push("--literal-pathspecs".to_string());
            }

            match command {
                GitCommands::Diff { args } => {
                    git::run(
                        git::GitCommand::Diff,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Log { args } => {
                    git::run(git::GitCommand::Log, &args, None, cli.verbose, &global_args)?;
                }
                GitCommands::Status { args } => {
                    git::run(
                        git::GitCommand::Status,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Show { args } => {
                    git::run(
                        git::GitCommand::Show,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Add { args } => {
                    git::run(git::GitCommand::Add, &args, None, cli.verbose, &global_args)?;
                }
                GitCommands::Commit { args } => {
                    git::run(
                        git::GitCommand::Commit,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Push { args } => {
                    git::run(
                        git::GitCommand::Push,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Pull { args } => {
                    git::run(
                        git::GitCommand::Pull,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Branch { args } => {
                    git::run(
                        git::GitCommand::Branch,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Fetch { args } => {
                    git::run(
                        git::GitCommand::Fetch,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Stash { subcommand, args } => {
                    git::run(
                        git::GitCommand::Stash { subcommand },
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Worktree { args } => {
                    git::run(
                        git::GitCommand::Worktree,
                        &args,
                        None,
                        cli.verbose,
                        &global_args,
                    )?;
                }
                GitCommands::Other(args) => {
                    git::run_passthrough(&args, &global_args, cli.verbose)?;
                }
            }
        }

        Commands::Gh { subcommand, args } => {
            gh_cmd::run(&subcommand, &args, cli.verbose, cli.ultra_compact)?;
        }

        Commands::Aws { subcommand, args } => {
            aws_cmd::run(&subcommand, &args, cli.verbose)?;
        }

        Commands::Psql { args } => {
            psql_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pnpm { command } => match command {
            PnpmCommands::List { depth, args } => {
                pnpm_cmd::run(pnpm_cmd::PnpmCommand::List { depth }, &args, cli.verbose)?;
            }
            PnpmCommands::Outdated { args } => {
                pnpm_cmd::run(pnpm_cmd::PnpmCommand::Outdated, &args, cli.verbose)?;
            }
            PnpmCommands::Install { packages, args } => {
                pnpm_cmd::run(
                    pnpm_cmd::PnpmCommand::Install { packages },
                    &args,
                    cli.verbose,
                )?;
            }
            PnpmCommands::Build { args } => {
                let mut build_args: Vec<String> = vec!["build".into()];
                build_args.extend(args);
                let os_args: Vec<OsString> = build_args.into_iter().map(OsString::from).collect();
                pnpm_cmd::run_passthrough(&os_args, cli.verbose)?;
            }
            PnpmCommands::Typecheck { args } => {
                tsc_cmd::run(&args, cli.verbose)?;
            }
            PnpmCommands::Other(args) => {
                pnpm_cmd::run_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Err { command } => {
            let cmd = command.join(" ");
            runner::run_err(&cmd, cli.verbose)?;
        }

        Commands::Test { command } => {
            let cmd = command.join(" ");
            runner::run_test(&cmd, cli.verbose)?;
        }

        Commands::Json { file, depth } => {
            if file == Path::new("-") {
                json_cmd::run_stdin(depth, cli.verbose)?;
            } else {
                json_cmd::run(&file, depth, cli.verbose)?;
            }
        }

        Commands::Deps { path } => {
            deps::run(&path, cli.verbose)?;
        }

        Commands::Env { filter, show_all } => {
            env_cmd::run(filter.as_deref(), show_all, cli.verbose)?;
        }

        Commands::Find { args } => {
            find_cmd::run_from_args(&args, cli.verbose)?;
        }

        Commands::Diff { file1, file2 } => {
            if let Some(f2) = file2 {
                diff_cmd::run(&file1, &f2, cli.verbose)?;
            } else {
                diff_cmd::run_stdin(cli.verbose)?;
            }
        }

        Commands::Log { file } => {
            if let Some(f) = file {
                log_cmd::run_file(&f, cli.verbose)?;
            } else {
                log_cmd::run_stdin(cli.verbose)?;
            }
        }

        Commands::Dotnet { command } => match command {
            DotnetCommands::Build { args } => {
                dotnet_cmd::run_build(&args, cli.verbose)?;
            }
            DotnetCommands::Test { args } => {
                dotnet_cmd::run_test(&args, cli.verbose)?;
            }
            DotnetCommands::Restore { args } => {
                dotnet_cmd::run_restore(&args, cli.verbose)?;
            }
            DotnetCommands::Format { args } => {
                dotnet_cmd::run_format(&args, cli.verbose)?;
            }
            DotnetCommands::Other(args) => {
                dotnet_cmd::run_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Docker { command } => match command {
            DockerCommands::Ps => {
                container::run(container::ContainerCmd::DockerPs, &[], cli.verbose)?;
            }
            DockerCommands::Images => {
                container::run(container::ContainerCmd::DockerImages, &[], cli.verbose)?;
            }
            DockerCommands::Logs { container: c } => {
                container::run(container::ContainerCmd::DockerLogs, &[c], cli.verbose)?;
            }
            DockerCommands::Compose { command: compose } => match compose {
                ComposeCommands::Ps => {
                    container::run_compose_ps(cli.verbose)?;
                }
                ComposeCommands::Logs { service } => {
                    container::run_compose_logs(service.as_deref(), cli.verbose)?;
                }
                ComposeCommands::Build { service } => {
                    container::run_compose_build(service.as_deref(), cli.verbose)?;
                }
                ComposeCommands::Other(args) => {
                    container::run_compose_passthrough(&args, cli.verbose)?;
                }
            },
            DockerCommands::Other(args) => {
                container::run_docker_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Kubectl { command } => match command {
            KubectlCommands::Pods { namespace, all } => {
                let mut args: Vec<String> = Vec::new();
                if all {
                    args.push("-A".to_string());
                } else if let Some(n) = namespace {
                    args.push("-n".to_string());
                    args.push(n);
                }
                container::run(container::ContainerCmd::KubectlPods, &args, cli.verbose)?;
            }
            KubectlCommands::Services { namespace, all } => {
                let mut args: Vec<String> = Vec::new();
                if all {
                    args.push("-A".to_string());
                } else if let Some(n) = namespace {
                    args.push("-n".to_string());
                    args.push(n);
                }
                container::run(container::ContainerCmd::KubectlServices, &args, cli.verbose)?;
            }
            KubectlCommands::Logs { pod, container: c } => {
                let mut args = vec![pod];
                if let Some(cont) = c {
                    args.push("-c".to_string());
                    args.push(cont);
                }
                container::run(container::ContainerCmd::KubectlLogs, &args, cli.verbose)?;
            }
            KubectlCommands::Other(args) => {
                container::run_kubectl_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Summary { command } => {
            let cmd = command.join(" ");
            summary::run(&cmd, cli.verbose)?;
        }

        Commands::Context { log_count } => {
            context_cmd::run(log_count, cli.verbose)?;
        }

        Commands::Dedup { command } => {
            let timer = tracking::TimedExecution::start();
            let (raw_cmd, output) = if command.is_empty() {
                // Read from stdin
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .lock()
                    .read_to_string(&mut buf)
                    .context("Failed to read stdin")?;
                ("(stdin)".to_string(), buf)
            } else {
                // Run command and capture output
                let cmd_str = command.join(" ");
                let result = if cfg!(target_os = "windows") {
                    std::process::Command::new("cmd")
                        .args(["/C", &cmd_str])
                        .output()
                } else {
                    std::process::Command::new("sh")
                        .args(["-c", &cmd_str])
                        .output()
                }
                .context("Failed to execute command")?;
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                (cmd_str, format!("{}{}", stdout, stderr))
            };
            let deduped = dedup::dedup_output(&output);
            let deduped = dedup::dedup_identical(&deduped);
            println!("{}", deduped);
            timer.track(&raw_cmd, "rtk dedup", &output, &deduped);
        }

        Commands::Watch { command } => {
            let cmd_str = command.join(" ");
            watch_cmd::run(&cmd_str, cli.verbose)?;
        }

        Commands::WatchClear => {
            watch_cmd::clear()?;
        }

        Commands::Grep {
            pattern,
            path,
            max_len,
            max,
            context_only,
            file_type,
            line_numbers: _, // no-op: line numbers always enabled in grep_cmd::run
            extra_args,
        } => {
            grep_cmd::run(
                &pattern,
                &path,
                max_len,
                max,
                context_only,
                file_type.as_deref(),
                &extra_args,
                cli.verbose,
            )?;
        }

        Commands::Init {
            global,
            opencode,
            show,
            claude_md,
            hook_only,
            auto_patch,
            no_patch,
            uninstall,
        } => {
            if show {
                init::show_config()?;
            } else if uninstall {
                init::uninstall(global, cli.verbose)?;
            } else {
                let install_opencode = opencode;
                let install_claude = !opencode;

                let patch_mode = if auto_patch {
                    init::PatchMode::Auto
                } else if no_patch {
                    init::PatchMode::Skip
                } else {
                    init::PatchMode::Ask
                };
                init::run(
                    global,
                    install_claude,
                    install_opencode,
                    claude_md,
                    hook_only,
                    patch_mode,
                    cli.verbose,
                )?;
            }
        }

        Commands::Wget { url, stdout, args } => {
            if stdout {
                wget_cmd::run_stdout(&url, &args, cli.verbose)?;
            } else {
                wget_cmd::run(&url, &args, cli.verbose)?;
            }
        }

        Commands::Wc { args } => {
            wc_cmd::run(&args, cli.verbose)?;
        }

        Commands::Gain {
            project, // added
            graph,
            history,
            quota,
            tier,
            daily,
            weekly,
            monthly,
            all,
            format,
            failures,
        } => {
            gain::run(
                project, // added: pass project flag
                graph,
                history,
                quota,
                &tier,
                daily,
                weekly,
                monthly,
                all,
                &format,
                failures,
                cli.verbose,
            )?;
        }

        Commands::CcEconomics {
            daily,
            weekly,
            monthly,
            all,
            format,
        } => {
            cc_economics::run(daily, weekly, monthly, all, &format, cli.verbose)?;
        }

        Commands::Config { create } => {
            if create {
                let path = config::Config::create_default()?;
                println!("Created: {}", path.display());
            } else {
                config::show_config()?;
            }
        }

        Commands::Vitest { command } => match command {
            VitestCommands::Run { args } => {
                vitest_cmd::run(vitest_cmd::VitestCommand::Run, &args, cli.verbose)?;
            }
        },

        Commands::Prisma { command } => match command {
            PrismaCommands::Generate { args } => {
                prisma_cmd::run(prisma_cmd::PrismaCommand::Generate, &args, cli.verbose)?;
            }
            PrismaCommands::Migrate { command } => match command {
                PrismaMigrateCommands::Dev { name, args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Dev { name },
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
                PrismaMigrateCommands::Status { args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Status,
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
                PrismaMigrateCommands::Deploy { args } => {
                    prisma_cmd::run(
                        prisma_cmd::PrismaCommand::Migrate {
                            subcommand: prisma_cmd::MigrateSubcommand::Deploy,
                        },
                        &args,
                        cli.verbose,
                    )?;
                }
            },
            PrismaCommands::DbPush { args } => {
                prisma_cmd::run(prisma_cmd::PrismaCommand::DbPush, &args, cli.verbose)?;
            }
        },

        Commands::Tsc { args } => {
            tsc_cmd::run(&args, cli.verbose)?;
        }

        Commands::Next { args } => {
            next_cmd::run(&args, cli.verbose)?;
        }

        Commands::Lint { args } => {
            lint_cmd::run(&args, cli.verbose)?;
        }

        Commands::Prettier { args } => {
            prettier_cmd::run(&args, cli.verbose)?;
        }

        Commands::Format { args } => {
            format_cmd::run(&args, cli.verbose)?;
        }

        Commands::Playwright { args } => {
            playwright_cmd::run(&args, cli.verbose)?;
        }

        Commands::Cargo { command } => match command {
            CargoCommands::Build { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Build, &args, cli.verbose)?;
            }
            CargoCommands::Test { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Test, &args, cli.verbose)?;
            }
            CargoCommands::Clippy { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Clippy, &args, cli.verbose)?;
            }
            CargoCommands::Check { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Check, &args, cli.verbose)?;
            }
            CargoCommands::Install { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Install, &args, cli.verbose)?;
            }
            CargoCommands::Nextest { args } => {
                cargo_cmd::run(cargo_cmd::CargoCommand::Nextest, &args, cli.verbose)?;
            }
            CargoCommands::Other(args) => {
                cargo_cmd::run_passthrough(&args, cli.verbose)?;
            }
        },

        Commands::Npm { args } => {
            npm_cmd::run(&args, cli.verbose, cli.skip_env)?;
        }

        Commands::Curl { args } => {
            curl_cmd::run(&args, cli.verbose)?;
        }

        Commands::Discover {
            project,
            limit,
            all,
            since,
            format,
        } => {
            discover::run(project.as_deref(), all, since, limit, &format, cli.verbose)?;
        }

        Commands::Learn {
            project,
            all,
            since,
            format,
            write_rules,
            min_confidence,
            min_occurrences,
        } => {
            learn::run(
                project,
                all,
                since,
                format,
                write_rules,
                min_confidence,
                min_occurrences,
            )?;
        }

        Commands::Npx { args } => {
            if args.is_empty() {
                anyhow::bail!("npx requires a command argument");
            }

            // Intelligent routing: delegate to specialized filters
            match args[0].as_str() {
                "tsc" | "typescript" => {
                    tsc_cmd::run(&args[1..], cli.verbose)?;
                }
                "eslint" => {
                    lint_cmd::run(&args[1..], cli.verbose)?;
                }
                "prisma" => {
                    // Route to prisma_cmd based on subcommand
                    if args.len() > 1 {
                        let prisma_args: Vec<String> = args[2..].to_vec();
                        match args[1].as_str() {
                            "generate" => {
                                prisma_cmd::run(
                                    prisma_cmd::PrismaCommand::Generate,
                                    &prisma_args,
                                    cli.verbose,
                                )?;
                            }
                            "db" if args.len() > 2 && args[2] == "push" => {
                                prisma_cmd::run(
                                    prisma_cmd::PrismaCommand::DbPush,
                                    &args[3..],
                                    cli.verbose,
                                )?;
                            }
                            _ => {
                                // Passthrough other prisma subcommands
                                let timer = tracking::TimedExecution::start();
                                let mut cmd = std::process::Command::new("npx");
                                for arg in &args {
                                    cmd.arg(arg);
                                }
                                let status = cmd.status().context("Failed to run npx prisma")?;
                                let args_str = args.join(" ");
                                timer.track_passthrough(
                                    &format!("npx {}", args_str),
                                    &format!("rtk npx {} (passthrough)", args_str),
                                );
                                if !status.success() {
                                    std::process::exit(status.code().unwrap_or(1));
                                }
                            }
                        }
                    } else {
                        let timer = tracking::TimedExecution::start();
                        let status = std::process::Command::new("npx")
                            .arg("prisma")
                            .status()
                            .context("Failed to run npx prisma")?;
                        timer.track_passthrough("npx prisma", "rtk npx prisma (passthrough)");
                        if !status.success() {
                            std::process::exit(status.code().unwrap_or(1));
                        }
                    }
                }
                "next" => {
                    next_cmd::run(&args[1..], cli.verbose)?;
                }
                "prettier" => {
                    prettier_cmd::run(&args[1..], cli.verbose)?;
                }
                "playwright" => {
                    playwright_cmd::run(&args[1..], cli.verbose)?;
                }
                _ => {
                    // Generic passthrough with npm boilerplate filter
                    npm_cmd::run(&args, cli.verbose, cli.skip_env)?;
                }
            }
        }

        Commands::Ruff { args } => {
            ruff_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pytest { args } => {
            pytest_cmd::run(&args, cli.verbose)?;
        }

        Commands::Mypy { args } => {
            mypy_cmd::run(&args, cli.verbose)?;
        }

        Commands::Pip { args } => {
            pip_cmd::run(&args, cli.verbose)?;
        }

        Commands::Go { command } => match command {
            GoCommands::Test { args } => {
                go_cmd::run_test(&args, cli.verbose)?;
            }
            GoCommands::Build { args } => {
                go_cmd::run_build(&args, cli.verbose)?;
            }
            GoCommands::Vet { args } => {
                go_cmd::run_vet(&args, cli.verbose)?;
            }
            GoCommands::Other(args) => {
                go_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::Gt { command } => match command {
            GtCommands::Log { args } => {
                gt_cmd::run_log(&args, cli.verbose)?;
            }
            GtCommands::Submit { args } => {
                gt_cmd::run_submit(&args, cli.verbose)?;
            }
            GtCommands::Sync { args } => {
                gt_cmd::run_sync(&args, cli.verbose)?;
            }
            GtCommands::Restack { args } => {
                gt_cmd::run_restack(&args, cli.verbose)?;
            }
            GtCommands::Create { args } => {
                gt_cmd::run_create(&args, cli.verbose)?;
            }
            GtCommands::Branch { args } => {
                gt_cmd::run_branch(&args, cli.verbose)?;
            }
            GtCommands::Other(args) => {
                gt_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::GolangciLint { args } => {
            golangci_cmd::run(&args, cli.verbose)?;
        }

        Commands::Bun { command } => match command {
            BunCommands::Install { args }
            | BunCommands::Add { args }
            | BunCommands::Remove { args } => {
                bun_cmd::run_install(&args, cli.verbose)?;
            }
            BunCommands::Run { args } => {
                bun_cmd::run_run(&args, cli.verbose)?;
            }
            BunCommands::Build { args } => {
                let cmd = format!("bun build {}", args.join(" "));
                runner::run_err(&cmd, cli.verbose)?;
            }
            BunCommands::Test { args } => {
                let cmd = format!("bun test {}", args.join(" "));
                runner::run_test(&cmd, cli.verbose)?;
            }
            BunCommands::Pm { args } => {
                if args.first().map(|s| s.as_str()) == Some("ls") {
                    bun_cmd::run_pm_ls(&args[1..], cli.verbose)?;
                } else {
                    bun_cmd::run_other(
                        &args.iter().map(OsString::from).collect::<Vec<_>>(),
                        cli.verbose,
                    )?;
                }
            }
            BunCommands::Other(args) => {
                bun_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::Bunx { args } => {
            if args.is_empty() {
                anyhow::bail!("bunx requires a command argument");
            }
            match args[0].as_str() {
                "tsc" | "typescript" => {
                    tsc_cmd::run(&args[1..], cli.verbose)?;
                }
                "eslint" => {
                    lint_cmd::run(&args[1..], cli.verbose)?;
                }
                _ => {
                    let cmd = format!("bunx {}", args.join(" "));
                    runner::run_err(&cmd, cli.verbose)?;
                }
            }
        }

        Commands::Deno { command } => match command {
            DenoCommands::Test { args } => {
                let cmd = format!("deno test {}", args.join(" "));
                runner::run_test(&cmd, cli.verbose)?;
            }
            DenoCommands::Check { args } => {
                deno_cmd::run_check(&args, cli.verbose)?;
            }
            DenoCommands::Lint { args } => {
                deno_cmd::run_lint(&args, cli.verbose)?;
            }
            DenoCommands::Run { args } | DenoCommands::Task { args } => {
                deno_cmd::run_task(&args, cli.verbose)?;
            }
            DenoCommands::Compile { args } => {
                let cmd = format!("deno compile {}", args.join(" "));
                runner::run_err(&cmd, cli.verbose)?;
            }
            DenoCommands::Install { args } => {
                deno_cmd::run_other(
                    &args.iter().map(OsString::from).collect::<Vec<_>>(),
                    cli.verbose,
                )?;
            }
            DenoCommands::Other(args) => {
                deno_cmd::run_other(&args, cli.verbose)?;
            }
        },

        Commands::HookAudit { since } => {
            hook_audit_cmd::run(since, cli.verbose)?;
        }

        Commands::Rewrite { args } => {
            let cmd = args.join(" ");
            rewrite_cmd::run(&cmd)?;
        }

        Commands::HookRewrite => {
            hook_rewrite_cmd::run()?;
        }

        Commands::Proxy { filter, args } => {
            use std::io::Read;
            use std::process::{Command, Stdio};

            if args.is_empty() {
                anyhow::bail!(
                    "proxy requires a command to execute\nUsage: rtk proxy <command> [args...]"
                );
            }

            let timer = tracking::TimedExecution::start();

            // If a single quoted arg contains spaces, split it respecting quotes (#388).
            // e.g. rtk proxy 'head -50 file.php' → cmd=head, args=["-50", "file.php"]
            // e.g. rtk proxy 'git log --format="%H %s"' → cmd=git, args=["log", "--format=%H %s"]
            let (cmd_name, cmd_args): (String, Vec<String>) = if args.len() == 1 {
                let full = args[0].to_string_lossy();
                let parts = shell_split(&full);
                if parts.len() > 1 {
                    (parts[0].clone(), parts[1..].to_vec())
                } else {
                    (full.into_owned(), vec![])
                }
            } else {
                (
                    args[0].to_string_lossy().into_owned(),
                    args[1..]
                        .iter()
                        .map(|s| s.to_string_lossy().into_owned())
                        .collect(),
                )
            };

            if cli.verbose > 0 {
                eprintln!(
                    "Proxy mode{}: {} {}",
                    if filter { " (filtered)" } else { "" },
                    cmd_name,
                    cmd_args.join(" ")
                );
            }

            if filter {
                // Filtered mode: capture all output, apply auto_filter, then print
                let child_output = Command::new(cmd_name.as_str())
                    .args(&cmd_args)
                    .output()
                    .context(format!("Failed to execute command: {}", cmd_name))?;

                let stdout = String::from_utf8_lossy(&child_output.stdout);
                let stderr = String::from_utf8_lossy(&child_output.stderr);
                let full_output = format!("{}{}", stdout, stderr);
                let success = child_output.status.success();

                let (filtered, _) = auto_filter::filter_with_status(&full_output, success);

                // Auto-cache: diff against recent cached output
                let cmd_str = format!("{} {}", cmd_name, cmd_args.join(" "));
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();

                let (final_output, was_cache_hit) = if cache::is_enabled() {
                    let ttl = cache::get_ttl_minutes();
                    if let Some((cached, age_secs)) = cache::load(&cmd_str, &cwd, ttl) {
                        let diff = cache::diff_output(&cached, &filtered);
                        let _ = cache::store(&cmd_str, &cwd, &filtered);
                        if diff.contains("no changes") {
                            (format!("[no changes since {}s ago]", age_secs), true)
                        } else {
                            (format!("[changes since {}s ago]\n{}", age_secs, diff), true)
                        }
                    } else {
                        let _ = cache::store(&cmd_str, &cwd, &filtered);
                        (filtered.clone(), false)
                    }
                } else {
                    (filtered.clone(), false)
                };

                print!("{}", final_output);

                let orig_cmd = format!("{} {}", cmd_name, cmd_args.join(" "));
                let rtk_cmd_str = format!("rtk proxy -f {} {}", cmd_name, cmd_args.join(" "));
                if was_cache_hit {
                    timer.track_cache_hit(&orig_cmd, &rtk_cmd_str, &full_output, &final_output);
                } else {
                    timer.track(&orig_cmd, &rtk_cmd_str, &full_output, &final_output);
                }
                maybe_hint_watch(&rtk_cmd_str);

                if !success {
                    std::process::exit(child_output.status.code().unwrap_or(1));
                }
            } else {
                // Raw mode: stream output in real-time, track but don't filter
                let mut child = Command::new(cmd_name.as_str())
                    .args(&cmd_args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .context(format!("Failed to execute command: {}", cmd_name))?;

                let stdout_pipe = child
                    .stdout
                    .take()
                    .context("Failed to capture child stdout")?;
                let stderr_pipe = child
                    .stderr
                    .take()
                    .context("Failed to capture child stderr")?;

                let stdout_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
                    let mut reader = stdout_pipe;
                    let mut captured = Vec::new();
                    let mut buf = [0u8; 8192];
                    loop {
                        let count = reader.read(&mut buf)?;
                        if count == 0 {
                            break;
                        }
                        captured.extend_from_slice(&buf[..count]);
                        use std::io::Write;
                        let mut out = std::io::stdout().lock();
                        out.write_all(&buf[..count])?;
                        out.flush()?;
                    }
                    Ok(captured)
                });

                let stderr_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
                    let mut reader = stderr_pipe;
                    let mut captured = Vec::new();
                    let mut buf = [0u8; 8192];
                    loop {
                        let count = reader.read(&mut buf)?;
                        if count == 0 {
                            break;
                        }
                        captured.extend_from_slice(&buf[..count]);
                        use std::io::Write;
                        let mut err = std::io::stderr().lock();
                        err.write_all(&buf[..count])?;
                        err.flush()?;
                    }
                    Ok(captured)
                });

                let status = child
                    .wait()
                    .context(format!("Failed waiting for command: {}", cmd_name))?;

                let stdout_bytes = stdout_handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("stdout streaming thread panicked"))??;
                let stderr_bytes = stderr_handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("stderr streaming thread panicked"))??;

                let stdout = String::from_utf8_lossy(&stdout_bytes);
                let stderr = String::from_utf8_lossy(&stderr_bytes);
                let full_output = format!("{}{}", stdout, stderr);

                timer.track(
                    &format!("{} {}", cmd_name, cmd_args.join(" ")),
                    &format!("rtk proxy {} {}", cmd_name, cmd_args.join(" ")),
                    &full_output,
                    &full_output,
                );
                maybe_hint_watch(&format!("rtk proxy {} {}", cmd_name, cmd_args.join(" ")));

                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
            }
        }

        Commands::Verify {
            filter,
            require_all,
        } => {
            if filter.is_some() {
                // Filter-specific mode: run only that filter's tests
                verify_cmd::run(filter, require_all)?;
            } else {
                // Default or --require-all: always run integrity check first
                integrity::run_verify(cli.verbose)?;
                verify_cmd::run(None, require_all)?;
            }
        }
    }

    Ok(())
}

/// Returns true for commands that are invoked via the hook pipeline
/// (i.e., commands that process rewritten shell commands).
/// Meta commands (init, gain, verify, etc.) are excluded because
/// they are run directly by the user, not through the hook.
/// Returns true for commands that go through the hook pipeline
/// and therefore require integrity verification.
///
/// SECURITY: whitelist pattern — new commands are NOT integrity-checked
/// until explicitly added here. A forgotten command fails open (no check)
/// rather than creating false confidence about what's protected.
fn is_operational_command(cmd: &Commands) -> bool {
    matches!(
        cmd,
        Commands::Ls { .. }
            | Commands::Tree { .. }
            | Commands::Read { .. }
            | Commands::Smart { .. }
            | Commands::Git { .. }
            | Commands::Gh { .. }
            | Commands::Pnpm { .. }
            | Commands::Err { .. }
            | Commands::Test { .. }
            | Commands::Json { .. }
            | Commands::Deps { .. }
            | Commands::Env { .. }
            | Commands::Find { .. }
            | Commands::Diff { .. }
            | Commands::Log { .. }
            | Commands::Dotnet { .. }
            | Commands::Docker { .. }
            | Commands::Kubectl { .. }
            | Commands::Summary { .. }
            | Commands::Context { .. }
            | Commands::Dedup { .. }
            | Commands::Watch { .. }
            | Commands::WatchClear
            | Commands::Grep { .. }
            | Commands::Wget { .. }
            | Commands::Vitest { .. }
            | Commands::Prisma { .. }
            | Commands::Tsc { .. }
            | Commands::Next { .. }
            | Commands::Lint { .. }
            | Commands::Prettier { .. }
            | Commands::Playwright { .. }
            | Commands::Cargo { .. }
            | Commands::Npm { .. }
            | Commands::Npx { .. }
            | Commands::Curl { .. }
            | Commands::Ruff { .. }
            | Commands::Pytest { .. }
            | Commands::Pip { .. }
            | Commands::Go { .. }
            | Commands::GolangciLint { .. }
            | Commands::Bun { .. }
            | Commands::Bunx { .. }
            | Commands::Deno { .. }
            | Commands::Gt { .. }
            | Commands::Proxy { .. }
    )
}

/// Emit a hint to stderr if the same command has been repeated 3+ times recently.
/// Only hints once per command per session (tracks via a static HashSet).
fn maybe_hint_watch(rtk_cmd: &str) {
    use std::sync::Mutex;
    lazy_static::lazy_static! {
        static ref HINTED: Mutex<std::collections::HashSet<String>> =
            Mutex::new(std::collections::HashSet::new());
    }

    // Skip if quiet mode
    if std::env::var("RTK_QUIET").is_ok_and(|v| v == "1" || v.to_lowercase() == "true") {
        return;
    }

    // Extract base_cmd (first 3 words of rtk_cmd)
    let base_cmd: String = rtk_cmd
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");

    // Skip volatile commands that are naturally repeated
    const SKIP_HINT: &[&str] = &[
        "rtk git status",
        "rtk git diff",
        "rtk git log",
        "rtk git show",
        "rtk git add",
        "rtk git commit",
        "rtk git push",
        "rtk git pull",
        "rtk ls",
        "rtk find",
        "rtk grep",
        "rtk read",
    ];
    if SKIP_HINT.iter().any(|p| base_cmd.starts_with(p)) {
        return;
    }

    // Only hint once per base_cmd per session
    {
        let mut hinted = match HINTED.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        if hinted.contains(&base_cmd) {
            return;
        }

        if let Ok(tracker) = crate::tracking::Tracker::new() {
            if let Ok(count) = tracker.count_recent_runs(&base_cmd, 60) {
                if count >= 3 {
                    let short_cmd = base_cmd.strip_prefix("rtk ").unwrap_or(&base_cmd);
                    eprintln!("[repeated {}x — consider: rtk watch {}]", count, short_cmd);
                    hinted.insert(base_cmd);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_git_commit_single_message() {
        let cli = Cli::try_parse_from(["rtk", "git", "commit", "-m", "fix: typo"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["-m", "fix: typo"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_commit_multiple_messages() {
        let cli = Cli::try_parse_from([
            "rtk",
            "git",
            "commit",
            "-m",
            "feat: add support",
            "-m",
            "Body paragraph here.",
        ])
        .unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(
                    args,
                    vec!["-m", "feat: add support", "-m", "Body paragraph here."]
                );
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    // #327: git commit -am "msg" was rejected by Clap
    #[test]
    fn test_git_commit_am_flag() {
        let cli = Cli::try_parse_from(["rtk", "git", "commit", "-am", "quick fix"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["-am", "quick fix"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_commit_amend() {
        let cli =
            Cli::try_parse_from(["rtk", "git", "commit", "--amend", "-m", "new msg"]).unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(args, vec!["--amend", "-m", "new msg"]);
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_git_global_options_parsing() {
        let cli =
            Cli::try_parse_from(["rtk", "git", "--no-pager", "--no-optional-locks", "status"])
                .unwrap();
        match cli.command {
            Commands::Git {
                no_pager,
                no_optional_locks,
                bare,
                literal_pathspecs,
                ..
            } => {
                assert!(no_pager);
                assert!(no_optional_locks);
                assert!(!bare);
                assert!(!literal_pathspecs);
            }
            _ => panic!("Expected Git command"),
        }
    }

    #[test]
    fn test_git_commit_long_flag_multiple() {
        let cli = Cli::try_parse_from([
            "rtk",
            "git",
            "commit",
            "--message",
            "title",
            "--message",
            "body",
            "--message",
            "footer",
        ])
        .unwrap();
        match cli.command {
            Commands::Git {
                command: GitCommands::Commit { args },
                ..
            } => {
                assert_eq!(
                    args,
                    vec![
                        "--message",
                        "title",
                        "--message",
                        "body",
                        "--message",
                        "footer"
                    ]
                );
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_try_parse_valid_git_status() {
        let result = Cli::try_parse_from(["rtk", "git", "status"]);
        assert!(result.is_ok(), "git status should parse successfully");
    }

    #[test]
    fn test_try_parse_help_is_display_help() {
        match Cli::try_parse_from(["rtk", "--help"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayHelp),
            Ok(_) => panic!("Expected DisplayHelp error"),
        }
    }

    #[test]
    fn test_try_parse_version_is_display_version() {
        match Cli::try_parse_from(["rtk", "--version"]) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayVersion),
            Ok(_) => panic!("Expected DisplayVersion error"),
        }
    }

    #[test]
    fn test_try_parse_unknown_subcommand_is_error() {
        match Cli::try_parse_from(["rtk", "nonexistent-command"]) {
            Err(e) => assert!(!matches!(
                e.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            )),
            Ok(_) => panic!("Expected parse error for unknown subcommand"),
        }
    }

    #[test]
    fn test_try_parse_git_with_dash_c_succeeds() {
        let result = Cli::try_parse_from(["rtk", "git", "-C", "/path", "status"]);
        assert!(
            result.is_ok(),
            "git -C /path status should parse successfully"
        );
        if let Ok(cli) = result {
            match cli.command {
                Commands::Git { directory, .. } => {
                    assert_eq!(directory, vec!["/path"]);
                }
                _ => panic!("Expected Git command"),
            }
        }
    }

    #[test]
    fn test_gain_failures_flag_parses() {
        let result = Cli::try_parse_from(["rtk", "gain", "--failures"]);
        assert!(result.is_ok());
        if let Ok(cli) = result {
            match cli.command {
                Commands::Gain { failures, .. } => assert!(failures),
                _ => panic!("Expected Gain command"),
            }
        }
    }

    #[test]
    fn test_gain_failures_short_flag_parses() {
        let result = Cli::try_parse_from(["rtk", "gain", "-F"]);
        assert!(result.is_ok());
        if let Ok(cli) = result {
            match cli.command {
                Commands::Gain { failures, .. } => assert!(failures),
                _ => panic!("Expected Gain command"),
            }
        }
    }

    #[test]
    fn test_meta_commands_reject_bad_flags() {
        // RTK meta-commands should produce parse errors (not fall through to raw execution).
        // Skip "proxy" because it uses trailing_var_arg (accepts any args by design).
        for cmd in RTK_META_COMMANDS {
            if *cmd == "proxy" {
                continue;
            }
            let result = Cli::try_parse_from(["rtk", cmd, "--nonexistent-flag-xyz"]);
            assert!(
                result.is_err(),
                "Meta-command '{}' with bad flag should fail to parse",
                cmd
            );
        }
    }

    #[test]
    fn test_meta_command_list_is_complete() {
        // Verify all meta-commands are in the guard list by checking they parse with valid syntax
        let meta_cmds_that_parse = [
            vec!["rtk", "gain"],
            vec!["rtk", "discover"],
            vec!["rtk", "learn"],
            vec!["rtk", "init"],
            vec!["rtk", "config"],
            vec!["rtk", "proxy", "echo", "hi"],
            vec!["rtk", "hook-audit"],
            vec!["rtk", "cc-economics"],
        ];
        for args in &meta_cmds_that_parse {
            let result = Cli::try_parse_from(args.iter());
            assert!(
                result.is_ok(),
                "Meta-command {:?} should parse successfully",
                args
            );
        }
    }

    #[test]
    fn test_shell_split_simple() {
        assert_eq!(
            shell_split("head -50 file.php"),
            vec!["head", "-50", "file.php"]
        );
    }

    #[test]
    fn test_shell_split_double_quotes() {
        assert_eq!(
            shell_split(r#"git log --format="%H %s""#),
            vec!["git", "log", "--format=%H %s"]
        );
    }

    #[test]
    fn test_shell_split_single_quotes() {
        assert_eq!(
            shell_split("grep -r 'hello world' ."),
            vec!["grep", "-r", "hello world", "."]
        );
    }

    #[test]
    fn test_shell_split_single_word() {
        assert_eq!(shell_split("ls"), vec!["ls"]);
    }

    #[test]
    fn test_shell_split_empty() {
        let result: Vec<String> = shell_split("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_rewrite_clap_multi_args() {
        // This is the bug KuSh reported: `rtk rewrite ls -al` failed because
        // Clap rejected `-al` as an unknown flag. With trailing_var_arg + allow_hyphen_values,
        // multiple args are accepted and joined into a single command string.
        let cases = vec![
            vec!["rtk", "rewrite", "ls", "-al"],
            vec!["rtk", "rewrite", "git", "status"],
            vec!["rtk", "rewrite", "npm", "exec"],
            vec!["rtk", "rewrite", "cargo", "test"],
            vec!["rtk", "rewrite", "du", "-sh", "."],
            vec!["rtk", "rewrite", "head", "-50", "file.txt"],
        ];
        for args in &cases {
            let result = Cli::try_parse_from(args.iter());
            assert!(
                result.is_ok(),
                "rtk rewrite {:?} should parse (was failing before trailing_var_arg fix)",
                &args[2..]
            );
            if let Ok(cli) = result {
                match cli.command {
                    Commands::Rewrite { ref args } => {
                        assert!(args.len() >= 2, "rewrite args should capture all tokens");
                    }
                    _ => panic!("expected Rewrite command"),
                }
            }
        }
    }

    #[test]
    fn test_rewrite_clap_quoted_single_arg() {
        // Quoted form: `rtk rewrite "git status"` — single arg containing spaces
        let result = Cli::try_parse_from(["rtk", "rewrite", "git status"]);
        assert!(result.is_ok());
        if let Ok(cli) = result {
            match cli.command {
                Commands::Rewrite { ref args } => {
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0], "git status");
                }
                _ => panic!("expected Rewrite command"),
            }
        }
    }
}
