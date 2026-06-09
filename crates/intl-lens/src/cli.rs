use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use intl_lens::audit::AuditResult;
use intl_lens::config::I18nConfig;
use intl_lens::i18n::store::TranslationStore;
use intl_lens::scanner::CodeScanner;

#[derive(Parser)]
#[command(name = "intl-lens-cli")]
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
    Audit {
        /// Filter by missing locales (comma-separated)
        #[arg(long)]
        missing_in: Option<String>,

        /// Include AI-ready fix suggestions
        #[arg(long)]
        suggest_fixes: bool,
    },
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
    },
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Audit {
            missing_in,
            suggest_fixes,
        } => {
            run_audit(
                &cli.workspace,
                cli.format,
                cli.output,
                missing_in,
                suggest_fixes,
            )
            .await?;
        }
        Commands::Check { files } => {
            run_check(&cli.workspace, files, cli.format, cli.output).await?;
        }
        Commands::Fix { dry_run } => {
            run_fix(&cli.workspace, dry_run).await?;
        }
    }

    Ok(())
}

async fn run_audit(
    workspace: &Path,
    format: OutputFormat,
    output: Option<PathBuf>,
    missing_in: Option<String>,
    suggest_fixes: bool,
) -> anyhow::Result<()> {
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
    store.scan_and_load_config(&config);

    pb.set_message("Scanning codebase...");
    let mut result = AuditResult::new(workspace.to_path_buf(), config, store);
    result.scan_codebase();

    pb.set_message("Generating report...");
    let mut report = result.generate_report();

    // Filter by missing_in if specified
    if let Some(locales_str) = missing_in {
        let locales: HashSet<&str> = locales_str.split(',').map(str::trim).collect();
        report.missing.retain(|m| {
            m.missing_in
                .iter()
                .any(|loc| locales.contains(loc.as_str()))
        });
        // Recalculate summary
        report.summary.missing_translations = report.missing.len();
    }

    pb.finish_and_clear();

    let output_str = match format {
        OutputFormat::Terminal => format_terminal(&report, suggest_fixes),
        OutputFormat::Json => serde_json::to_string_pretty(&report)?,
        OutputFormat::Markdown => format_markdown(&report, suggest_fixes),
    };

    if let Some(output_path) = output {
        std::fs::write(&output_path, output_str)?;
        println!("✓ Report written to {}", output_path.display());
    } else {
        println!("{}", output_str);
    }

    // Exit with error code if issues found
    if report.summary.missing_translations > 0 || report.summary.unused_keys > 0 {
        std::process::exit(1);
    }

    Ok(())
}

async fn run_check(
    workspace: &Path,
    files: Vec<PathBuf>,
    format: OutputFormat,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
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

    // Load translations to check existence
    let store = TranslationStore::new(workspace.to_path_buf());
    store.scan_and_load_config(&config);

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

            if !missing.is_empty() {
                std::process::exit(1);
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
            if !missing.is_empty() {
                std::process::exit(1);
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

            if !missing.is_empty() {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn run_fix(_workspace: &Path, _dry_run: bool) -> anyhow::Result<()> {
    println!("Fix command is coming soon!");
    println!("This will suggest or automatically apply fixes for missing translations.");
    Ok(())
}

fn format_terminal(report: &intl_lens::audit::AuditReport, suggest_fixes: bool) -> String {
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

    // Summary
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

    // Missing translations
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

    // Unused keys
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

    // Placeholder issues
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

fn format_markdown(report: &intl_lens::audit::AuditReport, suggest_fixes: bool) -> String {
    let mut md = String::new();

    md.push_str("# i18n Audit Report\n\n");

    // Summary
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

    // Missing translations
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

    // Unused keys
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

    // Placeholder issues
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
