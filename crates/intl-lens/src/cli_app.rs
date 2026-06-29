use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use globset::{Glob, GlobSet, GlobSetBuilder};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::audit::{AuditReport, AuditResult, AuditSummary};
use crate::config::I18nConfig;
use crate::i18n::store::TranslationStore;
use crate::scanner::CodeScanner;

#[derive(Parser)]
#[command(name = "intl-lens")]
#[command(about = "CLI tool for i18n auditing and analysis")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to the project root
    #[arg(short, long, default_value = ".", global = true)]
    workspace: PathBuf,

    /// Output format
    #[arg(short, long, value_enum, default_value = "terminal", global = true)]
    format: OutputFormat,

    /// Output file (if not specified, prints to stdout)
    #[arg(short, long, global = true)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Audit the entire project for i18n issues
    Audit(AuditArgs),
    /// Audit with CI-oriented defaults
    Ci(CiArgs),
    /// Check specific files for i18n key usage
    Check {
        /// Files to check
        files: Vec<PathBuf>,
    },
    /// Fix issues automatically (with approval)
    Fix {
        /// Dry run - show what would be changed without making changes
        #[arg(long)]
        dry_run: bool,
        /// Add missing translations to target locale files
        #[arg(long)]
        add_missing: bool,
        /// Value to use when adding missing translations. Defaults to source text.
        #[arg(long)]
        placeholder: Option<String>,
    },
}

#[derive(clap::Args)]
struct AuditArgs {
    /// Filter by missing locales (comma-separated)
    #[arg(long)]
    missing_in: Option<String>,

    /// Include AI-ready fix suggestions
    #[arg(long)]
    suggest_fixes: bool,

    /// Issue kinds that should fail the command: missing,unused,placeholder
    #[arg(long, value_parser = parse_fail_on, default_value = "missing,unused")]
    fail_on: FailOn,

    /// Ignore translation keys matching this regex
    #[arg(long)]
    ignore_key_pattern: Vec<String>,

    /// Ignore issues from files matching this glob, relative to workspace
    #[arg(long)]
    ignore_file: Vec<String>,

    /// Baseline file with accepted existing issues
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Write a baseline file from the current active issues and exit successfully
    #[arg(long)]
    write_baseline: Option<PathBuf>,

    /// Allow up to this many active unused keys before failing on unused
    #[arg(long)]
    max_unused: Option<usize>,
}

#[derive(clap::Args)]
struct CiArgs {
    /// Filter by missing locales (comma-separated)
    #[arg(long)]
    missing_in: Option<String>,

    /// Include AI-ready fix suggestions
    #[arg(long)]
    suggest_fixes: bool,

    /// Issue kinds that should fail the command: missing,unused,placeholder
    #[arg(long, value_parser = parse_fail_on, default_value = "missing,placeholder")]
    fail_on: FailOn,

    /// Ignore translation keys matching this regex
    #[arg(long)]
    ignore_key_pattern: Vec<String>,

    /// Ignore issues from files matching this glob, relative to workspace
    #[arg(long)]
    ignore_file: Vec<String>,

    /// Baseline file with accepted existing issues. Defaults to .intl-lens-baseline.json when present.
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Write a baseline file from the current active issues and exit successfully
    #[arg(long)]
    write_baseline: Option<PathBuf>,

    /// Allow up to this many active unused keys before failing on unused
    #[arg(long)]
    max_unused: Option<usize>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    /// Human-readable terminal output with colors
    Terminal,
    /// JSON format for programmatic consumption
    Json,
    /// Markdown report
    Markdown,
}

#[derive(Debug, Clone)]
struct FailOn {
    missing: bool,
    unused: bool,
    placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum IssueKind {
    Missing,
    Unused,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct IssueIdentity {
    kind: IssueKind,
    key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Baseline {
    version: u8,
    issues: Vec<IssueIdentity>,
}

struct AuditOptions {
    missing_in: Option<String>,
    suggest_fixes: bool,
    fail_on: FailOn,
    ignore_key_pattern: Vec<String>,
    ignore_file: Vec<String>,
    baseline: Option<PathBuf>,
    write_baseline: Option<PathBuf>,
    max_unused: Option<usize>,
}

pub async fn run_from_env() -> anyhow::Result<i32> {
    run(std::env::args_os()).await
}

pub async fn run<I, T>(args: I) -> anyhow::Result<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Audit(args) => {
            run_audit(
                &cli.workspace,
                cli.format,
                cli.output,
                AuditOptions::from(args),
            )
            .await
        }
        Commands::Ci(args) => {
            run_audit(
                &cli.workspace,
                cli.format,
                cli.output,
                AuditOptions::from_ci(args, &cli.workspace),
            )
            .await
        }
        Commands::Check { files } => run_check(&cli.workspace, files, cli.format, cli.output).await,
        Commands::Fix {
            dry_run,
            add_missing,
            placeholder,
        } => run_fix(&cli.workspace, dry_run, add_missing, placeholder).await,
    }
}

impl From<AuditArgs> for AuditOptions {
    fn from(args: AuditArgs) -> Self {
        Self {
            missing_in: args.missing_in,
            suggest_fixes: args.suggest_fixes,
            fail_on: args.fail_on,
            ignore_key_pattern: args.ignore_key_pattern,
            ignore_file: args.ignore_file,
            baseline: args.baseline,
            write_baseline: args.write_baseline,
            max_unused: args.max_unused,
        }
    }
}

impl AuditOptions {
    fn from_ci(args: CiArgs, workspace: &Path) -> Self {
        let baseline = args.baseline.or_else(|| {
            let default = workspace.join(".intl-lens-baseline.json");
            default.exists().then_some(default)
        });

        Self {
            missing_in: args.missing_in,
            suggest_fixes: args.suggest_fixes,
            fail_on: args.fail_on,
            ignore_key_pattern: args.ignore_key_pattern,
            ignore_file: args.ignore_file,
            baseline,
            write_baseline: args.write_baseline,
            max_unused: args.max_unused,
        }
    }
}

fn parse_fail_on(value: &str) -> Result<FailOn, String> {
    let mut fail_on = FailOn {
        missing: false,
        unused: false,
        placeholder: false,
    };

    for part in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part {
            "missing" => fail_on.missing = true,
            "unused" => fail_on.unused = true,
            "placeholder" => fail_on.placeholder = true,
            "none" => {}
            other => {
                return Err(format!(
                    "unknown issue kind '{other}', expected missing, unused, or placeholder"
                ));
            }
        }
    }

    Ok(fail_on)
}

async fn run_audit(
    workspace: &Path,
    format: OutputFormat,
    output: Option<PathBuf>,
    options: AuditOptions,
) -> anyhow::Result<i32> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")?
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );

    pb.set_message("Loading configuration...");
    let config = I18nConfig::load_from_workspace(workspace);

    pb.set_message("Scanning translation files...");
    let store = TranslationStore::new(workspace.to_path_buf());
    store.scan_and_load(&config.locale_paths);

    pb.set_message("Scanning codebase...");
    let mut result = AuditResult::new(workspace.to_path_buf(), config, store);
    result.scan_codebase();

    pb.set_message("Generating report...");
    let mut report = result.generate_report();
    pb.finish_and_clear();

    apply_audit_filters(&mut report, workspace, &options)?;

    if let Some(path) = options.write_baseline.as_ref() {
        write_baseline(path, workspace, &report)?;
        println!("✓ Baseline written to {}", path.display());
        return Ok(0);
    }

    let output_str = match format {
        OutputFormat::Terminal => format_terminal(&report, options.suggest_fixes),
        OutputFormat::Json => serde_json::to_string_pretty(&report)?,
        OutputFormat::Markdown => format_markdown(&report, options.suggest_fixes),
    };

    if let Some(output_path) = output {
        std::fs::write(&output_path, output_str)?;
        println!("✓ Report written to {}", output_path.display());
    } else {
        println!("{}", output_str);
    }

    Ok(evaluate_exit_code(
        &report,
        &options.fail_on,
        options.max_unused,
    ))
}

fn apply_audit_filters(
    report: &mut AuditReport,
    workspace: &Path,
    options: &AuditOptions,
) -> anyhow::Result<()> {
    if let Some(locales_str) = options.missing_in.as_ref() {
        let locales: HashSet<&str> = locales_str.split(',').map(str::trim).collect();
        report.missing.retain_mut(|item| {
            item.missing_in
                .retain(|locale| locales.contains(locale.as_str()));
            !item.missing_in.is_empty()
        });
    }

    let key_patterns = compile_key_patterns(&options.ignore_key_pattern)?;
    if !key_patterns.is_empty() {
        report
            .missing
            .retain(|item| !key_patterns.iter().any(|regex| regex.is_match(&item.key)));
        report
            .unused
            .retain(|item| !key_patterns.iter().any(|regex| regex.is_match(&item.key)));
        report
            .placeholder_issues
            .retain(|item| !key_patterns.iter().any(|regex| regex.is_match(&item.key)));
    }

    let file_globs = compile_file_globs(&options.ignore_file)?;
    if let Some(file_globs) = file_globs.as_ref() {
        report.missing.retain_mut(|item| {
            let had_usages = !item.used_in.is_empty();
            item.used_in
                .retain(|usage| !matches_glob(file_globs, workspace, &usage.file));
            !had_usages || !item.used_in.is_empty()
        });
        report
            .unused
            .retain(|item| !matches_glob(file_globs, workspace, &item.defined_in.file_path));
    }

    if let Some(path) = options.baseline.as_ref() {
        let baseline = read_baseline(path)?;
        apply_baseline(report, workspace, &baseline);
    }

    recalculate_summary(report);
    Ok(())
}

fn compile_key_patterns(patterns: &[String]) -> anyhow::Result<Vec<Regex>> {
    patterns
        .iter()
        .map(|pattern| Regex::new(pattern).with_context(|| format!("Invalid regex: {pattern}")))
        .collect()
}

fn compile_file_globs(patterns: &[String]) -> anyhow::Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("Invalid glob: {pattern}"))?);
    }

    Ok(Some(builder.build()?))
}

fn matches_glob(globs: &GlobSet, workspace: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(workspace).unwrap_or(path);
    globs.is_match(relative)
}

fn read_baseline(path: &Path) -> anyhow::Result<Baseline> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read baseline {}", path.display()))?;
    let baseline = serde_json::from_str::<Baseline>(&content)
        .with_context(|| format!("Failed to parse baseline {}", path.display()))?;
    if baseline.version != 1 {
        return Err(anyhow!(
            "Unsupported baseline version {}, expected 1",
            baseline.version
        ));
    }
    Ok(baseline)
}

fn write_baseline(path: &Path, workspace: &Path, report: &AuditReport) -> anyhow::Result<()> {
    let mut issues: Vec<IssueIdentity> = issue_identities(report, workspace).into_iter().collect();
    issues.sort();

    let baseline = Baseline { version: 1, issues };

    let content = serde_json::to_string_pretty(&baseline)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)?;
    Ok(())
}

fn apply_baseline(report: &mut AuditReport, workspace: &Path, baseline: &Baseline) {
    let issues: HashSet<IssueIdentity> = baseline.issues.iter().cloned().collect();

    report.missing.retain_mut(|item| {
        item.missing_in.retain(|locale| {
            !issues.contains(&IssueIdentity {
                kind: IssueKind::Missing,
                key: item.key.clone(),
                locale: Some(locale.clone()),
                file: None,
            })
        });
        !item.missing_in.is_empty()
    });

    report.unused.retain(|item| {
        !issues.contains(&IssueIdentity {
            kind: IssueKind::Unused,
            key: item.key.clone(),
            locale: None,
            file: Some(relative_path(workspace, &item.defined_in.file_path)),
        })
    });

    report.placeholder_issues.retain_mut(|item| {
        item.locale_values.retain(|locale, _| {
            !issues.contains(&IssueIdentity {
                kind: IssueKind::Placeholder,
                key: item.key.clone(),
                locale: Some(locale.clone()),
                file: None,
            })
        });
        !item.locale_values.is_empty()
    });
}

fn issue_identities(report: &AuditReport, workspace: &Path) -> HashSet<IssueIdentity> {
    let mut issues = HashSet::new();

    for item in &report.missing {
        for locale in &item.missing_in {
            issues.insert(IssueIdentity {
                kind: IssueKind::Missing,
                key: item.key.clone(),
                locale: Some(locale.clone()),
                file: None,
            });
        }
    }

    for item in &report.unused {
        issues.insert(IssueIdentity {
            kind: IssueKind::Unused,
            key: item.key.clone(),
            locale: None,
            file: Some(relative_path(workspace, &item.defined_in.file_path)),
        });
    }

    for item in &report.placeholder_issues {
        for locale in item.locale_values.keys() {
            issues.insert(IssueIdentity {
                kind: IssueKind::Placeholder,
                key: item.key.clone(),
                locale: Some(locale.clone()),
                file: None,
            });
        }
    }

    issues
}

fn relative_path(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn recalculate_summary(report: &mut AuditReport) {
    report.summary = AuditSummary {
        total_keys: report.summary.total_keys,
        total_locales: report.summary.total_locales,
        missing_translations: report.missing.len(),
        unused_keys: report.unused.len(),
        placeholder_mismatches: report.placeholder_issues.len(),
    };
}

fn evaluate_exit_code(report: &AuditReport, fail_on: &FailOn, max_unused: Option<usize>) -> i32 {
    if fail_on.missing && report.summary.missing_translations > 0 {
        return 1;
    }

    if fail_on.unused && report.summary.unused_keys > max_unused.unwrap_or(0) {
        return 1;
    }

    if fail_on.placeholder && report.summary.placeholder_mismatches > 0 {
        return 1;
    }

    0
}

async fn run_check(
    workspace: &Path,
    files: Vec<PathBuf>,
    format: OutputFormat,
    output: Option<PathBuf>,
) -> anyhow::Result<i32> {
    let config = I18nConfig::load_from_workspace(workspace);
    let scanner = CodeScanner::new(&config.function_patterns);

    let mut all_keys = Vec::new();

    for file in files {
        let content = std::fs::read_to_string(&file)?;
        let occurrences = scanner.scan_content(&content);
        for occ in occurrences {
            all_keys.push((file.clone(), occ));
        }
    }

    let store = TranslationStore::new(workspace.to_path_buf());
    store.scan_and_load(&config.locale_paths);

    let mut missing = Vec::new();
    let mut found = Vec::new();

    for (file, occ) in all_keys {
        if store.key_exists(&occ.key) {
            found.push((file, occ));
        } else {
            missing.push((file, occ));
        }
    }

    match format {
        OutputFormat::Terminal => {
            println!("{}", "i18n Key Check Results".bold().underline());
            println!();

            if !missing.is_empty() {
                println!(
                    "{}",
                    format!("❌ Missing Keys ({}):", missing.len()).red().bold()
                );
                for (file, occ) in &missing {
                    println!(
                        "  {}:{} {}",
                        file.display().to_string().cyan(),
                        occ.line + 1,
                        occ.key.yellow()
                    );
                }
                println!();
            }

            if !found.is_empty() {
                println!(
                    "{}",
                    format!("✓ Found Keys ({})", found.len()).green().bold()
                );
                for (file, occ) in &found {
                    println!(
                        "  {}:{} {}",
                        file.display().to_string().dimmed(),
                        occ.line + 1,
                        occ.key.dimmed()
                    );
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "missing": missing.iter().map(|(f, o)| serde_json::json!({
                    "file": f,
                    "line": o.line + 1,
                    "key": o.key,
                })).collect::<Vec<_>>(),
                "found": found.iter().map(|(f, o)| serde_json::json!({
                    "file": f,
                    "line": o.line + 1,
                    "key": o.key,
                })).collect::<Vec<_>>(),
            });
            let output_str = serde_json::to_string_pretty(&json)?;
            if let Some(output_path) = output {
                std::fs::write(&output_path, output_str)?;
            } else {
                println!("{}", output_str);
            }
        }
        OutputFormat::Markdown => {
            let mut md = String::new();
            md.push_str("# i18n Key Check Results\n\n");

            if !missing.is_empty() {
                md.push_str(&format!("## ❌ Missing Keys ({}):\n\n", missing.len()));
                for (file, occ) in &missing {
                    md.push_str(&format!(
                        "- `{}:{}` - `{}`\n",
                        file.display(),
                        occ.line + 1,
                        occ.key
                    ));
                }
                md.push('\n');
            }

            if !found.is_empty() {
                md.push_str(&format!("## ✓ Found Keys ({}):\n\n", found.len()));
                for (file, occ) in &found {
                    md.push_str(&format!(
                        "- `{}:{}` - `{}`\n",
                        file.display(),
                        occ.line + 1,
                        occ.key
                    ));
                }
            }

            if let Some(output_path) = output {
                std::fs::write(&output_path, md)?;
            } else {
                println!("{}", md);
            }
        }
    }

    Ok(if missing.is_empty() { 0 } else { 1 })
}

async fn run_fix(
    workspace: &Path,
    dry_run: bool,
    add_missing: bool,
    placeholder: Option<String>,
) -> anyhow::Result<i32> {
    let config = I18nConfig::load_from_workspace(workspace);
    let store = TranslationStore::new(workspace.to_path_buf());
    store.scan_and_load(&config.locale_paths);

    let mut result = AuditResult::new(workspace.to_path_buf(), config.clone(), store);
    result.scan_codebase();
    let report = result.generate_report();

    if dry_run {
        println!("{}", format_fix_dry_run(workspace, &config, &report));
        return Ok(0);
    }

    if add_missing {
        let summary =
            apply_missing_translations(workspace, &config, &report, placeholder.as_deref())?;
        println!("Added {} missing translations.", summary.added);
        if summary.skipped > 0 {
            println!(
                "Skipped {} missing translations without a supported JSON target file.",
                summary.skipped
            );
        }
        return Ok(0);
    }

    println!("Write mode requires an explicit fix option. Run `intl-lens fix --dry-run` to preview fixes.");
    println!("Supported write option: `intl-lens fix --add-missing --placeholder _TODO_`.");
    Ok(1)
}

struct MissingWriteSummary {
    added: usize,
    skipped: usize,
}

fn apply_missing_translations(
    workspace: &Path,
    config: &I18nConfig,
    report: &AuditReport,
    placeholder: Option<&str>,
) -> anyhow::Result<MissingWriteSummary> {
    let mut added = 0;
    let mut skipped = 0;

    for item in &report.missing {
        for locale in &item.missing_in {
            let Some(file) = find_locale_file(workspace, config, locale) else {
                skipped += 1;
                continue;
            };

            if file.extension().and_then(|extension| extension.to_str()) != Some("json") {
                skipped += 1;
                continue;
            }

            let value = placeholder.unwrap_or(&item.source_value);
            add_json_translation(&file, &item.key, value)?;
            added += 1;
        }
    }

    Ok(MissingWriteSummary { added, skipped })
}

fn add_json_translation(path: &Path, key: &str, value: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read locale file {}", path.display()))?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON locale file {}", path.display()))?;

    insert_json_key(&mut json, key, value);

    let mut output = serde_json::to_string_pretty(&json)?;
    output.push('\n');
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write locale file {}", path.display()))?;
    Ok(())
}

fn insert_json_key(json: &mut serde_json::Value, key: &str, value: &str) {
    if !json.is_object() {
        *json = serde_json::json!({});
    }

    let parts: Vec<&str> = key.split('.').collect();
    let mut current = json;

    for part in &parts[..parts.len().saturating_sub(1)] {
        if !current.get(*part).is_some_and(serde_json::Value::is_object) {
            current[*part] = serde_json::json!({});
        }
        current = &mut current[*part];
    }

    if let Some(last) = parts.last() {
        current[*last] = serde_json::Value::String(value.to_string());
    }
}

fn format_fix_dry_run(workspace: &Path, config: &I18nConfig, report: &AuditReport) -> String {
    let mut output = String::new();
    output.push_str("i18n Fix Dry Run\n\n");
    output.push_str(&format!(
        "Missing translations: {}\n",
        report.summary.missing_translations
    ));
    output.push_str(&format!("Unused keys: {}\n", report.summary.unused_keys));
    output.push_str(&format!(
        "Placeholder issues: {}\n\n",
        report.summary.placeholder_mismatches
    ));

    if report.missing.is_empty() && report.unused.is_empty() && report.placeholder_issues.is_empty()
    {
        output.push_str("No fixes to suggest.\n");
        return output;
    }

    if !report.missing.is_empty() {
        output.push_str("Missing translations\n");
        for item in &report.missing {
            output.push_str(&format!("- {}\n", item.key));
            output.push_str(&format!(
                "  action: {}\n",
                fix_action(item.suggestion.as_ref())
            ));
            output.push_str(&format!(
                "  source: {} = {}\n",
                item.source_locale, item.source_value
            ));
            output.push_str(&format!("  missing in: {}\n", item.missing_in.join(", ")));
            output.push_str("  files:\n");
            for file in fix_files(item.suggestion.as_ref()) {
                output.push_str(&format!("    - {}\n", relative_path(workspace, &file)));
            }
        }
        output.push('\n');
    }

    if !report.unused.is_empty() {
        output.push_str("Unused keys\n");
        for item in &report.unused {
            output.push_str(&format!("- {}\n", item.key));
            output.push_str(&format!(
                "  action: {}\n",
                fix_action(item.suggestion.as_ref())
            ));
            output.push_str(&format!(
                "  defined in: {}\n",
                relative_path(workspace, &item.defined_in.file_path)
            ));
        }
        output.push('\n');
    }

    if !report.placeholder_issues.is_empty() {
        output.push_str("Placeholder issues\n");
        for item in &report.placeholder_issues {
            let mut locales: Vec<&String> = item.locale_values.keys().collect();
            locales.sort();
            output.push_str(&format!("- {}\n", item.key));
            output.push_str("  action: review_placeholder_mismatch\n");
            output.push_str(&format!(
                "  expected placeholders: {}\n",
                item.expected_placeholders.join(", ")
            ));
            output.push_str(&format!(
                "  mismatched locales: {}\n",
                locales
                    .iter()
                    .map(|locale| locale.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            output.push_str("  files:\n");
            for locale in locales {
                if let Some(file) = find_locale_file(workspace, config, locale) {
                    output.push_str(&format!("    - {}\n", relative_path(workspace, &file)));
                }
            }
        }
    }

    output
}

fn fix_action(suggestion: Option<&crate::audit::FixSuggestion>) -> &str {
    suggestion
        .map(|suggestion| suggestion.action.as_str())
        .unwrap_or("review")
}

fn fix_files(suggestion: Option<&crate::audit::FixSuggestion>) -> Vec<PathBuf> {
    suggestion
        .map(|suggestion| suggestion.files_to_edit.clone())
        .unwrap_or_default()
}

fn find_locale_file(workspace: &Path, config: &I18nConfig, locale: &str) -> Option<PathBuf> {
    for locale_path in &config.locale_paths {
        let base = workspace.join(locale_path);
        for extension in ["json", "yaml", "yml", "arb", "php"] {
            let candidate = base.join(format!("{locale}.{extension}"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn format_terminal(report: &AuditReport, suggest_fixes: bool) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "{}\n\n",
        "╔══════════════════════════════════════╗".blue().bold()
    ));
    output.push_str(&format!(
        "{}\n",
        "║      i18n Audit Report               ║".blue().bold()
    ));
    output.push_str(&format!(
        "{}\n\n",
        "╚══════════════════════════════════════╝".blue().bold()
    ));

    output.push_str(&"Summary\n".bold().underline().to_string());
    output.push_str(&format!(
        "  Total Keys:        {}\n",
        report.summary.total_keys.to_string().cyan()
    ));
    output.push_str(&format!(
        "  Total Locales:     {}\n",
        report.summary.total_locales.to_string().cyan()
    ));

    let missing_str = if report.summary.missing_translations > 0 {
        report.summary.missing_translations.to_string().red().bold()
    } else {
        report.summary.missing_translations.to_string().green()
    };
    output.push_str(&format!("  Missing Translations: {}\n", missing_str));

    let unused_str = if report.summary.unused_keys > 0 {
        report.summary.unused_keys.to_string().yellow().bold()
    } else {
        report.summary.unused_keys.to_string().green()
    };
    output.push_str(&format!("  Unused Keys:       {}\n", unused_str));

    if report.summary.placeholder_mismatches > 0 {
        output.push_str(&format!(
            "  Placeholder Issues: {}\n",
            report
                .summary
                .placeholder_mismatches
                .to_string()
                .red()
                .bold()
        ));
    }
    output.push('\n');

    if !report.missing.is_empty() {
        output.push_str(&format!(
            "{}",
            "Missing Translations\n".red().bold().underline()
        ));
        for item in &report.missing {
            output.push_str(&format!("  {} {}\n", "•".red(), item.key.yellow()));
            output.push_str(&format!(
                "    Source ({}): {}\n",
                item.source_locale,
                item.source_value.dimmed()
            ));
            output.push_str(&format!(
                "    Missing in: {}\n",
                item.missing_in.join(", ").red()
            ));

            if !item.used_in.is_empty() {
                output.push_str("    Used in:\n");
                for usage in &item.used_in {
                    output.push_str(&format!(
                        "      - {}:{}\n",
                        usage.file.display().to_string().dimmed(),
                        usage.line + 1
                    ));
                }
            }

            if suggest_fixes {
                if let Some(sugg) = item.suggestion.as_ref() {
                    output.push_str(&format!(
                        "    {} {}\n",
                        "→".green(),
                        "Suggestion:".green().bold()
                    ));
                    output.push_str(&format!("      Action: {}\n", sugg.action.green()));
                    if !sugg.files_to_edit.is_empty() {
                        output.push_str("      Files to edit:\n");
                        for f in &sugg.files_to_edit {
                            output.push_str(&format!(
                                "        - {}\n",
                                f.display().to_string().green()
                            ));
                        }
                    }
                }
            }
            output.push('\n');
        }
    }

    if !report.unused.is_empty() {
        output.push_str(&"Unused Keys\n".yellow().bold().underline().to_string());
        for item in &report.unused {
            output.push_str(&format!("  {} {}\n", "•".yellow(), item.key.dimmed()));
            output.push_str(&format!(
                "    Defined in: {}:{}\n",
                item.defined_in.file_path.display().to_string().dimmed(),
                item.defined_in.line
            ));
            output.push('\n');
        }
    }

    if !report.placeholder_issues.is_empty() {
        output.push_str(&format!(
            "{}",
            "Placeholder Issues\n".red().bold().underline()
        ));
        for item in &report.placeholder_issues {
            output.push_str(&format!("  {} {}\n", "•".red(), item.key.yellow()));
            output.push_str(&format!(
                "    Expected placeholders: {}\n",
                item.expected_placeholders.join(", ").cyan()
            ));
            output.push_str("    Mismatched locales:\n");
            for (locale, value) in &item.locale_values {
                output.push_str(&format!("      {}: {}\n", locale.red(), value));
            }
            output.push('\n');
        }
    }

    if report.missing.is_empty() && report.unused.is_empty() && report.placeholder_issues.is_empty()
    {
        output.push_str(&format!("{}\n", "✓ All i18n checks passed!".green().bold()));
    }

    output
}

fn format_markdown(report: &AuditReport, suggest_fixes: bool) -> String {
    let mut md = String::new();

    md.push_str("# i18n Audit Report\n\n");

    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Count |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!("| Total Keys | {} |\n", report.summary.total_keys));
    md.push_str(&format!(
        "| Total Locales | {} |\n",
        report.summary.total_locales
    ));

    let missing_badge = if report.summary.missing_translations > 0 {
        format!("**{}** ⚠️", report.summary.missing_translations)
    } else {
        format!("{} ✓", report.summary.missing_translations)
    };
    md.push_str(&format!("| Missing Translations | {} |\n", missing_badge));

    let unused_badge = if report.summary.unused_keys > 0 {
        format!("**{}** ⚠️", report.summary.unused_keys)
    } else {
        format!("{} ✓", report.summary.unused_keys)
    };
    md.push_str(&format!("| Unused Keys | {} |\n", unused_badge));

    if report.summary.placeholder_mismatches > 0 {
        md.push_str(&format!(
            "| Placeholder Issues | **{}** ⚠️ |\n",
            report.summary.placeholder_mismatches
        ));
    }
    md.push('\n');

    if !report.missing.is_empty() {
        md.push_str("## Missing Translations\n\n");
        for item in &report.missing {
            md.push_str(&format!("### `{}`\n\n", item.key));
            md.push_str(&format!(
                "- **Source ({}):** {}\n",
                item.source_locale, item.source_value
            ));
            md.push_str(&format!(
                "- **Missing in:** `{}`\n",
                item.missing_in.join("`, `")
            ));

            if !item.used_in.is_empty() {
                md.push_str("- **Used in:**\n");
                for usage in &item.used_in {
                    md.push_str(&format!(
                        "  - `{}:{}`\n",
                        usage.file.display(),
                        usage.line + 1
                    ));
                }
            }

            if suggest_fixes {
                if let Some(sugg) = item.suggestion.as_ref() {
                    md.push_str("\n**Suggestion:**\n");
                    md.push_str(&format!("- Action: `{}`\n", sugg.action));
                    if !sugg.files_to_edit.is_empty() {
                        md.push_str("- Files to edit:\n");
                        for f in &sugg.files_to_edit {
                            md.push_str(&format!("  - `{}`\n", f.display()));
                        }
                    }
                }
            }
            md.push('\n');
        }
    }

    if !report.unused.is_empty() {
        md.push_str("## Unused Keys\n\n");
        for item in &report.unused {
            md.push_str(&format!(
                "- `{}` - defined in `{}:{}`\n",
                item.key,
                item.defined_in.file_path.display(),
                item.defined_in.line
            ));
        }
        md.push('\n');
    }

    if !report.placeholder_issues.is_empty() {
        md.push_str("## Placeholder Issues\n\n");
        for item in &report.placeholder_issues {
            md.push_str(&format!("### `{}`\n\n", item.key));
            md.push_str(&format!(
                "- **Expected placeholders:** `{}`\n",
                item.expected_placeholders.join("`, `")
            ));
            md.push_str("- **Mismatched locales:**\n");
            for (locale, value) in &item.locale_values {
                md.push_str(&format!("  - `{}`: `{}`\n", locale, value));
            }
            md.push('\n');
        }
    }

    if report.missing.is_empty() && report.unused.is_empty() && report.placeholder_issues.is_empty()
    {
        md.push_str("## ✓ All i18n checks passed!\n");
    }

    md
}
