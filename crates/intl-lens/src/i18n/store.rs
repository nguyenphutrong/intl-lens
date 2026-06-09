use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use dashmap::DashMap;
use globset::Glob;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::config::I18nConfig;
use crate::i18n::namespace;

use super::parser::TranslationParser;

#[derive(Debug, Clone)]
pub struct TranslationEntry {
    pub value: String,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationLocation {
    pub file_path: PathBuf,
    pub line: usize,
}

pub struct TranslationStore {
    translations: DashMap<String, HashMap<String, TranslationEntry>>,
    locale_files: DashMap<String, HashSet<PathBuf>>,
    workspace_root: PathBuf,
}

impl TranslationStore {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            translations: DashMap::new(),
            locale_files: DashMap::new(),
            workspace_root,
        }
    }

    pub fn scan_and_load(&self, locale_paths: &[String]) {
        self.scan_and_load_with_options(locale_paths, false);
    }

    pub fn scan_and_load_config(&self, config: &I18nConfig) {
        self.scan_and_load_with_options(&config.locale_paths, config.namespace_enabled);
    }

    fn scan_and_load_with_options(&self, locale_paths: &[String], namespace_enabled: bool) {
        for locale_path in locale_paths {
            let trimmed = locale_path.trim_end_matches(['/', '\\']);
            if trimmed.is_empty() {
                continue;
            }

            if has_glob_meta(trimmed) {
                self.scan_glob_path(trimmed, namespace_enabled);
                continue;
            }

            let full_path = self.workspace_root.join(trimmed);
            if full_path.is_file() {
                self.scan_file(&full_path, namespace_enabled);
            } else if full_path.exists() {
                self.scan_directory(&full_path, namespace_enabled);
            }
        }
    }

    fn scan_glob_path(&self, locale_path: &str, namespace_enabled: bool) {
        let Ok(glob) = Glob::new(locale_path) else {
            tracing::warn!("Invalid locale path glob: {}", locale_path);
            return;
        };
        let matcher = glob.compile_matcher();

        for entry in WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_entry(|entry| !is_ignored_workspace_dir(entry.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let Ok(relative_path) = path.strip_prefix(&self.workspace_root) else {
                continue;
            };

            if !matcher.is_match(relative_path) {
                continue;
            }

            if path.is_dir() {
                self.scan_directory(path, namespace_enabled);
            } else if path.is_file() {
                self.scan_file(path, namespace_enabled);
            }
        }
    }

    fn scan_directory(&self, dir: &Path, namespace_enabled: bool) {
        let json_glob = Glob::new("*.json").unwrap().compile_matcher();
        let yaml_glob = Glob::new("*.{yaml,yml}").unwrap().compile_matcher();
        let php_glob = Glob::new("*.php").unwrap().compile_matcher();
        let arb_glob = Glob::new("*.arb").unwrap().compile_matcher();

        for entry in WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();

            if path.is_file()
                && (json_glob.is_match(file_name)
                    || yaml_glob.is_match(file_name)
                    || php_glob.is_match(file_name)
                    || arb_glob.is_match(file_name))
            {
                self.scan_file(path, namespace_enabled);
            }
        }
    }

    fn scan_file(&self, path: &Path, namespace_enabled: bool) {
        if let Some(locale) = self.extract_locale_from_path(path) {
            self.locale_files
                .entry(locale.clone())
                .or_default()
                .insert(path.to_path_buf());
            self.load_translation_file(path, &locale, namespace_enabled);
        }
    }

    fn extract_locale_from_path(&self, path: &Path) -> Option<String> {
        let file_stem = path.file_stem()?.to_str()?;

        if is_locale_code(file_stem) {
            return Some(file_stem.to_string());
        }

        // Handle ARB naming convention: app_en.arb, app_es.arb, etc.
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "arb" {
                // Try to extract locale from patterns like "app_en" or "messages_en_US"
                if let Some(locale) = extract_locale_from_arb_filename(file_stem) {
                    return Some(locale);
                }
            }
        }

        if let Some(parent) = path.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                if is_locale_code(parent_name) {
                    return Some(parent_name.to_string());
                }
            }
        }

        None
    }

    fn load_translation_file(&self, path: &Path, locale: &str, namespace_enabled: bool) {
        match TranslationParser::parse_file(path) {
            Ok(translations) => {
                let mut locale_map = self.translations.entry(locale.to_string()).or_default();
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let prefix = namespace::file_namespace_prefix(
                    extension,
                    file_stem,
                    namespace_enabled,
                    is_locale_code,
                );

                for (key, value) in translations {
                    let full_key = namespace::apply_file_namespace(key, prefix);

                    locale_map.insert(
                        full_key,
                        TranslationEntry {
                            value,
                            file_path: path.to_path_buf(),
                        },
                    );
                }

                tracing::debug!(
                    "Loaded {} translations from {:?} for locale {}",
                    locale_map.len(),
                    path,
                    locale
                );
            }
            Err(e) => {
                tracing::warn!("Failed to parse {:?}: {}", path, e);
            }
        }
    }

    pub fn get_translation(&self, key: &str, locale: &str) -> Option<String> {
        self.translations.get(locale).and_then(|map| {
            namespace::key_lookup_variants(key)
                .iter()
                .find_map(|candidate| map.get(candidate).map(|e| e.value.clone()))
        })
    }

    pub fn get_all_translations(&self, key: &str) -> HashMap<String, TranslationEntry> {
        let mut result = HashMap::new();
        let candidates = namespace::key_lookup_variants(key);
        for entry in self.translations.iter() {
            let locale = entry.key();
            if let Some(translation) = candidates
                .iter()
                .find_map(|candidate| entry.value().get(candidate))
            {
                result.insert(locale.clone(), translation.clone());
            }
        }
        result
    }

    pub fn get_prefixed_translations(
        &self,
        key: &str,
    ) -> HashMap<String, Vec<(String, TranslationEntry)>> {
        let mut result = HashMap::new();
        let prefixes: Vec<String> = namespace::key_lookup_variants(key)
            .into_iter()
            .map(|candidate| format!("{}.", candidate))
            .collect();

        for entry in self.translations.iter() {
            let mut matches: Vec<(String, TranslationEntry)> = entry
                .value()
                .iter()
                .filter(|(existing_key, _)| {
                    prefixes
                        .iter()
                        .any(|prefix| existing_key.starts_with(prefix))
                })
                .map(|(key, translation)| (key.clone(), translation.clone()))
                .collect();

            matches.sort_by(|a, b| a.0.cmp(&b.0));

            if !matches.is_empty() {
                result.insert(entry.key().clone(), matches);
            }
        }

        result
    }

    pub fn get_translation_location(&self, key: &str, locale: &str) -> Option<TranslationLocation> {
        self.translations.get(locale).and_then(|map| {
            namespace::key_lookup_variants(key)
                .iter()
                .find_map(|candidate| {
                    map.get(candidate).map(|e| {
                        let line =
                            Self::find_key_line_in_file(&e.file_path, candidate).unwrap_or(0);
                        TranslationLocation {
                            file_path: e.file_path.clone(),
                            line,
                        }
                    })
                })
        })
    }

    fn find_key_line_in_file(file_path: &Path, key: &str) -> Option<usize> {
        let content = std::fs::read_to_string(file_path).ok()?;

        let last_part = key.split('.').next_back().unwrap_or(key);
        let search_patterns = [
            format!("\"{}\"", last_part),
            format!("'{}'", last_part),
            format!("{}: ", last_part),
            format!("{}:", last_part),
        ];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &search_patterns {
                if line.contains(pattern) {
                    return Some(line_num);
                }
            }
        }

        None
    }

    pub fn get_all_keys(&self) -> Vec<String> {
        let mut keys = std::collections::HashSet::new();
        for entry in self.translations.iter() {
            for key in entry.value().keys() {
                keys.insert(key.clone());
            }
        }
        keys.into_iter().collect()
    }

    pub fn get_locales(&self) -> Vec<String> {
        self.translations.iter().map(|e| e.key().clone()).collect()
    }

    pub fn key_exists(&self, key: &str) -> bool {
        let candidates = namespace::key_lookup_variants(key);
        self.translations.iter().any(|entry| {
            candidates
                .iter()
                .any(|candidate| key_or_prefix_exists(entry.value(), candidate))
        })
    }

    pub fn get_locale_file_paths(&self, locale: &str) -> Vec<PathBuf> {
        let mut result: Vec<PathBuf> = self
            .locale_files
            .get(locale)
            .map(|set| set.value().iter().cloned().collect())
            .unwrap_or_default();
        result.sort();
        result
    }

    pub fn get_missing_locales(&self, key: &str) -> Vec<String> {
        let all_locales: Vec<String> = self.get_locales();
        all_locales
            .into_iter()
            .filter(|locale| {
                self.translations
                    .get(locale)
                    .map(|m| {
                        let candidates = namespace::key_lookup_variants(key);
                        !candidates
                            .iter()
                            .any(|candidate| key_or_prefix_exists(&m, candidate))
                    })
                    .unwrap_or(true)
            })
            .collect()
    }
}

fn key_or_prefix_exists(map: &HashMap<String, TranslationEntry>, key: &str) -> bool {
    if map.contains_key(key) {
        return true;
    }

    let prefix = format!("{}.", key);
    map.keys().any(|existing| existing.starts_with(&prefix))
}

fn is_locale_code(s: &str) -> bool {
    let locale_patterns = [
        r"^[a-z]{2}$",
        r"^[a-z]{2}[-_][A-Z]{2}$",
        r"^[a-z]{2}[-_][a-z]{2}$",
    ];

    for pattern in &locale_patterns {
        if regex::Regex::new(pattern).unwrap().is_match(s) {
            return true;
        }
    }

    let common_locales = [
        "en", "en-US", "en-GB", "es", "es-ES", "fr", "fr-FR", "de", "de-DE", "it", "it-IT", "pt",
        "pt-BR", "ja", "ja-JP", "ko", "ko-KR", "zh", "zh-CN", "zh-TW", "ru", "ru-RU", "ar",
        "ar-SA", "vi", "vi-VN",
    ];

    common_locales.contains(&s)
}

fn has_glob_meta(path: &str) -> bool {
    path.contains('*') || path.contains('?') || path.contains('[')
}

fn is_ignored_workspace_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "node_modules" | ".git" | "target" | "dist" | "build" | ".nuxt" | ".output"
            )
        })
}

/// Extract locale from ARB filename patterns like "app_en", "messages_en_US", "intl_vi"
fn extract_locale_from_arb_filename(file_stem: &str) -> Option<String> {
    // Common ARB file prefixes
    let prefixes = ["app_", "intl_", "messages_", "l10n_", "strings_"];

    for prefix in prefixes {
        if let Some(locale_part) = file_stem.strip_prefix(prefix) {
            if is_locale_code(locale_part) {
                return Some(locale_part.to_string());
            }
        }
    }

    // Try splitting by underscore and check if last part(s) form a locale
    let parts: Vec<&str> = file_stem.split('_').collect();
    if parts.len() >= 2 {
        // Try last part as locale (e.g., "app_en" -> "en")
        let last = parts[parts.len() - 1];
        if is_locale_code(last) {
            return Some(last.to_string());
        }

        // Try last two parts as locale (e.g., "app_en_US" -> "en_US")
        if parts.len() >= 3 {
            let locale = format!("{}_{}", parts[parts.len() - 2], parts[parts.len() - 1]);
            if is_locale_code(&locale) {
                return Some(locale);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn scan_and_load_expands_glob_locale_directories() {
        let root = test_workspace("glob-locale-directories");
        let locale_dir = root.join("layers/foo/i18n/locales");
        fs::create_dir_all(&locale_dir).expect("create locale dir");
        fs::write(
            locale_dir.join("en.json"),
            r#"{"foo":{"greeting":"Hello from layer"}}"#,
        )
        .expect("write en locale");

        let store = TranslationStore::new(root.clone());
        store.scan_and_load(&["**/*/i18n/locales".to_string()]);

        assert_eq!(
            store.get_translation("foo.greeting", "en").as_deref(),
            Some("Hello from layer")
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scan_and_load_expands_glob_locale_files() {
        let root = test_workspace("glob-locale-files");
        let locale_dir = root.join("layers/foo/i18n/locales");
        fs::create_dir_all(&locale_dir).expect("create locale dir");
        fs::write(locale_dir.join("fr.json"), r#"{"hello":"Bonjour"}"#).expect("write fr locale");

        let store = TranslationStore::new(root.clone());
        store.scan_and_load(&["layers/*/i18n/locales/*.json".to_string()]);

        assert_eq!(
            store.get_translation("hello", "fr").as_deref(),
            Some("Bonjour")
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scan_and_load_config_prefixes_json_namespace_files() {
        let root = test_workspace("namespace-json-files");
        let locale_dir = root.join("public/static/locales/en");
        fs::create_dir_all(&locale_dir).expect("create locale dir");
        fs::write(
            locale_dir.join("common.json"),
            r#"{"buttons":{"save":"Save"}}"#,
        )
        .expect("write namespace locale");

        let config = I18nConfig {
            locale_paths: vec!["public/static/locales".to_string()],
            namespace_enabled: true,
            ..Default::default()
        };
        let store = TranslationStore::new(root.clone());
        store.scan_and_load_config(&config);

        assert_eq!(
            store
                .get_translation("common.buttons.save", "en")
                .as_deref(),
            Some("Save")
        );
        assert_eq!(
            store
                .get_translation("common:buttons.save", "en")
                .as_deref(),
            Some("Save")
        );
        assert!(store.key_exists("common.buttons"));
        assert!(store.get_missing_locales("common.buttons").is_empty());

        let prefixed = store.get_prefixed_translations("common.buttons");
        assert_eq!(prefixed["en"].len(), 1);
        assert_eq!(prefixed["en"][0].0, "common.buttons.save");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scan_and_load_config_does_not_duplicate_existing_namespace_root() {
        let root = test_workspace("namespace-existing-root");
        let locale_dir = root.join("public/static/locales/en");
        fs::create_dir_all(&locale_dir).expect("create locale dir");
        fs::write(
            locale_dir.join("signin.json"),
            r#"{"signin":{"description":"Welcome"}}"#,
        )
        .expect("write namespace locale");

        let config = I18nConfig {
            locale_paths: vec!["public/static/locales".to_string()],
            namespace_enabled: true,
            ..Default::default()
        };
        let store = TranslationStore::new(root.clone());
        store.scan_and_load_config(&config);

        assert_eq!(
            store.get_translation("signin.description", "en").as_deref(),
            Some("Welcome")
        );
        assert!(store
            .get_translation("signin.signin.description", "en")
            .is_none());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scan_and_load_keeps_json_namespace_disabled_behavior() {
        let root = test_workspace("namespace-disabled-json-files");
        let locale_dir = root.join("locales/en");
        fs::create_dir_all(&locale_dir).expect("create locale dir");
        fs::write(locale_dir.join("common.json"), r#"{"hello":"Hello"}"#).expect("write locale");

        let store = TranslationStore::new(root.clone());
        store.scan_and_load(&["locales".to_string()]);

        assert_eq!(
            store.get_translation("hello", "en").as_deref(),
            Some("Hello")
        );
        assert!(store.get_translation("common.hello", "en").is_none());

        fs::remove_dir_all(root).ok();
    }

    fn test_workspace(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("intl-lens-{name}-{nonce}"))
    }
}
