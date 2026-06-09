use std::path::{Path, PathBuf};

use regex::Regex;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectedNamespaceProject {
    pub locale_paths: Vec<String>,
    pub source_locale: Option<String>,
    pub default_namespace: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamespaceContext {
    pub namespaces: Vec<String>,
}

/// Detect common namespace-based i18n project configuration.
///
/// This intentionally returns a generic namespace project shape instead of leaking
/// framework details to callers. More detectors can be added here without changing
/// the LSP, store, scanner, CLI, or MCP layers.
pub fn detect_namespace_project(root: &Path) -> Option<DetectedNamespaceProject> {
    detect_next_i18next_project(root).or_else(|| detect_i18next_locale_tree(root))
}

/// Return the namespace prefix for a translation file.
///
/// File-based namespaces are common across i18next-style projects:
/// `locales/en/common.json` -> `common.*`.
/// PHP keeps its historical namespace behavior for Laravel language files.
pub fn file_namespace_prefix<'a>(
    extension: &str,
    file_stem: &'a str,
    namespace_enabled: bool,
    is_locale_code: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    if file_stem.is_empty() || is_locale_code(file_stem) {
        return None;
    }

    if extension == "php" {
        return Some(file_stem);
    }

    if namespace_enabled && matches!(extension, "json" | "yaml" | "yml") {
        return Some(file_stem);
    }

    None
}

pub fn apply_file_namespace(key: String, namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) if key.starts_with(&format!("{}.", namespace)) => key,
        Some(namespace) => format!("{}.{}", namespace, key),
        None => key,
    }
}

pub fn key_lookup_variants(key: &str) -> Vec<String> {
    let mut variants = vec![key.to_string()];

    if let Some((namespace, rest)) = key.split_once(':') {
        if !namespace.is_empty() && !rest.is_empty() {
            variants.push(format!("{}.{}", namespace, rest));
        }
    }

    variants
}

pub fn infer_namespace_context(content: &str) -> NamespaceContext {
    let mut namespaces = Vec::new();

    if let Ok(single_namespace) = Regex::new(r#"useTranslation\s*\(\s*[\"']([A-Za-z0-9_-]+)[\"']"#)
    {
        for cap in single_namespace.captures_iter(content) {
            if let Some(namespace) = cap.get(1) {
                push_unique_namespace(&mut namespaces, namespace.as_str());
            }
        }
    }

    if let Ok(array_namespace) = Regex::new(r#"useTranslation\s*\(\s*\[([^\]]+)\]"#) {
        for cap in array_namespace.captures_iter(content) {
            let Some(items) = cap.get(1) else {
                continue;
            };

            for namespace in parse_namespace_list(items.as_str()) {
                push_unique_namespace(&mut namespaces, &namespace);
            }
        }
    }

    NamespaceContext { namespaces }
}

pub fn apply_namespace_context(raw_key: &str, context: &NamespaceContext) -> String {
    if context.namespaces.is_empty() || raw_key.contains(':') {
        return raw_key.to_string();
    }

    if context
        .namespaces
        .iter()
        .any(|namespace| raw_key.starts_with(&format!("{}.", namespace)))
    {
        return raw_key.to_string();
    }

    format!("{}.{}", context.namespaces[0], raw_key)
}

fn detect_next_i18next_project(root: &Path) -> Option<DetectedNamespaceProject> {
    let config_path = [
        "next-i18next.config.js",
        "next-i18next.config.cjs",
        "next-i18next.config.mjs",
        "next-i18next.config.ts",
    ]
    .iter()
    .map(|file| root.join(file))
    .find(|path| path.exists())?;

    let content = std::fs::read_to_string(config_path).ok()?;
    let mut detected = DetectedNamespaceProject::default();

    if let Some(locale_path) = extract_js_string_property(&content, "localePath") {
        detected
            .locale_paths
            .push(normalize_locale_path(root, &locale_path));
    }

    if detected.locale_paths.is_empty() {
        detected.locale_paths.extend(existing_locale_roots(root));
    }

    detected.source_locale = extract_js_string_property(&content, "defaultLocale");
    detected.default_namespace = extract_js_string_property(&content, "defaultNS")
        .or_else(|| extract_js_string_property(&content, "defaultNamespace"));

    Some(detected)
}

fn detect_i18next_locale_tree(root: &Path) -> Option<DetectedNamespaceProject> {
    let locale_paths = existing_locale_roots(root);
    if locale_paths
        .iter()
        .any(|path| looks_like_namespace_locale_root(root, path))
    {
        Some(DetectedNamespaceProject {
            locale_paths,
            source_locale: None,
            default_namespace: None,
        })
    } else {
        None
    }
}

fn looks_like_namespace_locale_root(root: &Path, relative_path: &str) -> bool {
    let base = root.join(relative_path);
    let Ok(locale_entries) = std::fs::read_dir(base) else {
        return false;
    };

    for locale_entry in locale_entries.filter_map(|entry| entry.ok()) {
        let locale_path = locale_entry.path();
        if !locale_path.is_dir() {
            continue;
        }

        let Ok(files) = std::fs::read_dir(locale_path) else {
            continue;
        };

        let namespace_file_count = files
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file()
                    && entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| matches!(extension, "json" | "yaml" | "yml"))
            })
            .take(2)
            .count();

        if namespace_file_count >= 2 {
            return true;
        }
    }

    false
}

fn existing_locale_roots(root: &Path) -> Vec<String> {
    ["public/locales", "public/static/locales", "locales"]
        .into_iter()
        .filter(|candidate| root.join(candidate).is_dir())
        .map(ToString::to_string)
        .collect()
}

fn extract_js_string_property(content: &str, property: &str) -> Option<String> {
    let pattern = format!(
        r#"(?s)\b{}\s*:\s*(?:path\.resolve\(\s*)?['\"]([^'\"]+)['\"]"#,
        regex::escape(property)
    );
    let regex = Regex::new(&pattern).ok()?;
    regex
        .captures(content)
        .and_then(|captures| captures.get(1))
        .map(|matched| matched.as_str().trim().to_string())
}

fn normalize_locale_path(root: &Path, locale_path: &str) -> String {
    let trimmed = locale_path
        .trim()
        .trim_start_matches("./")
        .trim_end_matches(['/', '\\']);

    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path.strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        trimmed.replace('\\', "/")
    }
}

fn parse_namespace_list(input: &str) -> Vec<String> {
    let Ok(namespace_item) = Regex::new(r#"[\"']([A-Za-z0-9_-]+)[\"']"#) else {
        return Vec::new();
    };

    namespace_item
        .captures_iter(input)
        .filter_map(|captures| captures.get(1))
        .map(|matched| matched.as_str().to_string())
        .collect()
}

fn push_unique_namespace(namespaces: &mut Vec<String>, namespace: &str) {
    if !namespace.is_empty() && !namespaces.iter().any(|existing| existing == namespace) {
        namespaces.push(namespace.to_string());
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn key_lookup_variants_supports_i18next_colon_separator() {
        assert_eq!(
            key_lookup_variants("common:buttons.save"),
            vec![
                "common:buttons.save".to_string(),
                "common.buttons.save".to_string()
            ]
        );
    }

    #[test]
    fn apply_file_namespace_does_not_duplicate_existing_root() {
        assert_eq!(
            apply_file_namespace("signin.description".to_string(), Some("signin")),
            "signin.description"
        );
        assert_eq!(
            apply_file_namespace("buttons.save".to_string(), Some("common")),
            "common.buttons.save"
        );
    }

    #[test]
    fn infers_use_translation_namespaces() {
        let context = infer_namespace_context(
            r#"
            const { t } = useTranslation('common')
            const { t: memberT } = useTranslation(['member', 'common'])
            "#,
        );

        assert_eq!(context.namespaces, vec!["common", "member"]);
        assert_eq!(
            apply_namespace_context("buttons.save", &context),
            "common.buttons.save"
        );
        assert_eq!(
            apply_namespace_context("common:buttons.save", &context),
            "common:buttons.save"
        );
    }

    #[test]
    fn detects_next_i18next_project_config() {
        let root = test_workspace("next-i18next-project");
        fs::create_dir_all(root.join("public/static/locales/en")).expect("create locales");
        fs::write(
            root.join("next-i18next.config.js"),
            r#"
            const path = require('path')
            module.exports = {
              i18n: { defaultLocale: 'en', locales: ['en', 'ja'] },
              localePath: path.resolve('./public/static/locales'),
              defaultNS: 'common',
            }
            "#,
        )
        .expect("write config");

        let detected = detect_namespace_project(&root).expect("detect namespace project");
        assert_eq!(detected.locale_paths, vec!["public/static/locales"]);
        assert_eq!(detected.source_locale.as_deref(), Some("en"));
        assert_eq!(detected.default_namespace.as_deref(), Some("common"));

        fs::remove_dir_all(root).ok();
    }

    fn test_workspace(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("intl-lens-namespace-{name}-{nonce}"))
    }
}
