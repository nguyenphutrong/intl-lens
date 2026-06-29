use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

fn intl_lens() -> Command {
    Command::cargo_bin("intl-lens").expect("intl-lens binary")
}

fn intl_lens_cli() -> Command {
    Command::cargo_bin("intl-lens-cli").expect("intl-lens-cli binary")
}

fn write_workspace(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("temp workspace");
    fs::create_dir_all(dir.path().join("locales")).expect("locales dir");
    fs::create_dir_all(dir.path().join("src/generated")).expect("src dir");
    fs::create_dir_all(dir.path().join(".zed")).expect("zed dir");
    fs::write(
        dir.path().join(".zed/i18n.json"),
        r#"{"localePaths":["locales"],"sourceLocale":"en"}"#,
    )
    .expect("config");

    for (path, content) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(full_path, content).expect("fixture file");
    }

    dir
}

fn run_json(workspace: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace)
        .arg("--format")
        .arg("json")
        .args(args);
    command.assert()
}

#[test]
fn intl_lens_help_shows_cli_commands() {
    intl_lens()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("audit"))
        .stdout(contains("ci"))
        .stdout(contains("check"));
}

#[test]
fn audit_fails_on_missing_and_unused_by_default() {
    let workspace = write_workspace(&[
        (
            "locales/en.json",
            r#"{"checkout":{"submit":"Submit"},"legacy":{"title":"Legacy"}}"#,
        ),
        ("locales/vi.json", r#"{}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);

    run_json(workspace.path(), &["audit"])
        .failure()
        .stdout(contains("\"missing_translations\": 2"))
        .stdout(contains("\"unused_keys\": 1"));
}

#[test]
fn ci_does_not_fail_on_unused_by_default() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"legacy":{"title":"Legacy"}}"#),
        ("locales/vi.json", r#"{"legacy":{"title":"Cu"}}"#),
        ("src/App.tsx", "export const App = () => null;"),
    ]);

    run_json(workspace.path(), &["ci"])
        .success()
        .stdout(contains("\"unused_keys\": 1"));
}

#[test]
fn audit_can_fail_on_placeholder_only() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"greeting":"Hello {name}"}"#),
        ("locales/vi.json", r#"{"greeting":"Xin chao"}"#),
        ("src/App.tsx", r#"export const App = () => t("greeting");"#),
    ]);

    run_json(workspace.path(), &["audit", "--fail-on", "placeholder"])
        .failure()
        .stdout(contains("\"placeholder_mismatches\": 1"));
}

#[test]
fn max_unused_controls_unused_failures() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"one":"One","two":"Two"}"#),
        ("locales/vi.json", r#"{"one":"Mot","two":"Hai"}"#),
        ("src/App.tsx", "export const App = () => null;"),
    ]);

    run_json(
        workspace.path(),
        &["ci", "--fail-on", "unused", "--max-unused", "1"],
    )
    .failure()
    .stdout(contains("\"unused_keys\": 2"));

    run_json(
        workspace.path(),
        &["ci", "--fail-on", "unused", "--max-unused", "2"],
    )
    .success()
    .stdout(contains("\"unused_keys\": 2"));
}

#[test]
fn ignore_key_pattern_suppresses_matching_issues() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"legacy":{"title":"Legacy"}}"#),
        ("locales/vi.json", r#"{}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("legacy.title");"#,
        ),
    ]);

    run_json(
        workspace.path(),
        &["ci", "--ignore-key-pattern", r"^legacy\."],
    )
    .success()
    .stdout(contains("\"missing_translations\": 0"));
}

#[test]
fn ignore_file_suppresses_issues_used_only_by_ignored_files() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"generated":{"title":"Generated"}}"#),
        ("locales/vi.json", r#"{}"#),
        (
            "src/generated/App.tsx",
            r#"export const App = () => t("generated.title");"#,
        ),
    ]);

    run_json(
        workspace.path(),
        &["ci", "--ignore-file", "src/generated/**"],
    )
    .success()
    .stdout(contains("\"missing_translations\": 0"));
}

#[test]
fn baseline_write_and_read_suppresses_existing_issues() {
    let workspace = write_workspace(&[
        (
            "locales/en.json",
            r#"{"checkout":{"submit":"Submit","cancel":"Cancel"}}"#,
        ),
        ("locales/vi.json", r#"{}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);
    let baseline = workspace.path().join(".intl-lens-baseline.json");

    let mut write_command = intl_lens();
    write_command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("audit")
        .arg("--write-baseline")
        .arg(&baseline);
    write_command.assert().success();

    let content = fs::read_to_string(&baseline).expect("baseline content");
    let json: Value = serde_json::from_str(&content).expect("baseline json");
    assert_eq!(json["version"], 1);
    assert!(json["issues"]
        .as_array()
        .is_some_and(|issues| !issues.is_empty()));

    run_json(
        workspace.path(),
        &["ci", "--baseline", baseline.to_str().unwrap()],
    )
    .success()
    .stdout(contains("\"missing_translations\": 0"));

    fs::write(
        workspace.path().join("locales/en.json"),
        r#"{"checkout":{"submit":"Submit","cancel":"Cancel","pay":"Pay"}}"#,
    )
    .expect("add new issue");

    run_json(
        workspace.path(),
        &["ci", "--baseline", baseline.to_str().unwrap()],
    )
    .failure()
    .stdout(contains("checkout.pay"));
}

#[test]
fn compatibility_intl_lens_cli_alias_still_runs() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"save":"Save"}"#),
        ("locales/vi.json", r#"{"save":"Luu"}"#),
        ("src/App.tsx", r#"export const App = () => t("save");"#),
    ]);

    let mut command = intl_lens_cli();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("--format")
        .arg("json")
        .arg("audit");
    command
        .assert()
        .success()
        .stdout(contains("\"missing_translations\": 0"));
}

#[test]
fn fix_dry_run_outputs_reviewable_suggestions() {
    let workspace = write_workspace(&[
        (
            "locales/en.json",
            r#"{"checkout":{"submit":"Submit"},"greeting":"Hello {name}","legacy":"Legacy"}"#,
        ),
        ("locales/vi.json", r#"{"greeting":"Xin chao"}"#),
        (
            "src/App.tsx",
            r#"export const App = () => <>{t("checkout.submit")}{t("greeting")}</>;"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--dry-run");
    command
        .assert()
        .success()
        .stdout(contains("i18n Fix Dry Run"))
        .stdout(contains("checkout.submit"))
        .stdout(contains("action: add_translation"))
        .stdout(contains("locales/vi.json"))
        .stdout(contains("legacy"))
        .stdout(contains("action: remove_or_review"))
        .stdout(contains("greeting"))
        .stdout(contains("action: review_placeholder_mismatch"));
}

#[test]
fn fix_add_missing_writes_json_placeholder_values() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"checkout":{"submit":"Submit"}}"#),
        ("locales/vi.json", r#"{}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--add-missing")
        .arg("--placeholder")
        .arg("_TODO_");
    command
        .assert()
        .success()
        .stdout(contains("Added 1 missing translations."));

    let content = fs::read_to_string(workspace.path().join("locales/vi.json")).expect("vi json");
    let json: Value = serde_json::from_str(&content).expect("updated json");
    assert_eq!(json["checkout"]["submit"], "_TODO_");
}

#[test]
fn fix_add_missing_defaults_to_source_text() {
    let workspace = write_workspace(&[
        ("locales/en.json", r#"{"checkout":{"submit":"Submit"}}"#),
        ("locales/vi.json", r#"{}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--add-missing");
    command.assert().success();

    let content = fs::read_to_string(workspace.path().join("locales/vi.json")).expect("vi json");
    let json: Value = serde_json::from_str(&content).expect("updated json");
    assert_eq!(json["checkout"]["submit"], "Submit");
}

#[test]
fn fix_add_missing_writes_yaml_locale_files() {
    let workspace = write_workspace(&[
        ("locales/en.yaml", "checkout:\n  submit: Submit\n"),
        ("locales/vi.yaml", "{}\n"),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--add-missing")
        .arg("--placeholder")
        .arg("_TODO_");
    command.assert().success();

    let content = fs::read_to_string(workspace.path().join("locales/vi.yaml")).expect("vi yaml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).expect("updated yaml");
    assert_eq!(yaml["checkout"]["submit"], "_TODO_");
}

#[test]
fn fix_add_missing_writes_arb_locale_files() {
    let workspace = write_workspace(&[
        (
            "locales/app_en.arb",
            r#"{"@@locale":"en","checkoutSubmit":"Submit"}"#,
        ),
        ("locales/app_vi.arb", r#"{"@@locale":"vi"}"#),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkoutSubmit");"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--add-missing")
        .arg("--placeholder")
        .arg("_TODO_");
    command.assert().success();

    let content = fs::read_to_string(workspace.path().join("locales/app_vi.arb")).expect("vi arb");
    let json: Value = serde_json::from_str(&content).expect("updated arb");
    assert_eq!(json["checkoutSubmit"], "_TODO_");
}

#[test]
fn fix_add_missing_writes_php_locale_files() {
    let workspace = write_workspace(&[
        (
            "locales/en.php",
            r#"<?php

return [
    'checkout.submit' => 'Submit',
];
"#,
        ),
        (
            "locales/vi.php",
            r#"<?php

return [
];
"#,
        ),
        (
            "src/App.tsx",
            r#"export const App = () => t("checkout.submit");"#,
        ),
    ]);

    let mut command = intl_lens();
    command
        .arg("--workspace")
        .arg(workspace.path())
        .arg("fix")
        .arg("--add-missing")
        .arg("--placeholder")
        .arg("_TODO_");
    command
        .assert()
        .success()
        .stdout(contains("Added 1 missing translations."));

    let content = fs::read_to_string(workspace.path().join("locales/vi.php")).expect("vi php");
    assert!(content.contains("'checkout.submit' => '_TODO_',"));

    run_json(workspace.path(), &["ci"])
        .success()
        .stdout(contains("\"missing_translations\": 0"));
}
