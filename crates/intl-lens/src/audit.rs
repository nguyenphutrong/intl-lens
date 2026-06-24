use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::I18nConfig;
use crate::i18n::store::{TranslationLocation, TranslationStore};
use crate::scanner::{CodeScanner, ScannedFile};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub summary: AuditSummary,
    pub missing: Vec<MissingTranslation>,
    pub unused: Vec<UnusedKey>,
    pub placeholder_issues: Vec<PlaceholderIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub total_keys: usize,
    pub total_locales: usize,
    pub missing_translations: usize,
    pub unused_keys: usize,
    pub placeholder_mismatches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingTranslation {
    pub key: String,
    pub source_value: String,
    pub source_locale: String,
    pub missing_in: Vec<String>,
    pub used_in: Vec<KeyUsage>,
    pub suggestion: Option<FixSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnusedKey {
    pub key: String,
    pub defined_in: TranslationLocation,
    pub suggestion: Option<FixSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceholderIssue {
    pub key: String,
    pub issue_type: PlaceholderIssueType,
    pub locale_values: HashMap<String, String>,
    pub expected_placeholders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceholderIssueType {
    Mismatch,
    Missing,
    Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUsage {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    pub action: String,
    pub files_to_edit: Vec<PathBuf>,
    pub context: Option<String>,
}

pub struct AuditResult {
    pub workspace_root: PathBuf,
    pub config: I18nConfig,
    pub store: TranslationStore,
    pub scanned_files: Vec<ScannedFile>,
    pub used_keys: HashMap<String, Vec<KeyUsage>>,
}

impl AuditResult {
    pub fn new(workspace_root: PathBuf, config: I18nConfig, store: TranslationStore) -> Self {
        Self {
            workspace_root,
            config,
            store,
            scanned_files: Vec::new(),
            used_keys: HashMap::new(),
        }
    }

    pub fn scan_codebase(&mut self) {
        let scanner = CodeScanner::new(&self.config.function_patterns);
        self.scanned_files = scanner.scan_directory(&self.workspace_root);

        // Aggregate key usages
        for file in &self.scanned_files {
            for found in &file.found_keys {
                let usage = KeyUsage {
                    file: file.path.clone(),
                    line: found.line,
                    column: found.start_char,
                    code: found.code_snippet.clone(),
                };
                self.used_keys
                    .entry(found.key.clone())
                    .or_default()
                    .push(usage);
            }
        }
    }

    pub fn generate_report(&self) -> AuditReport {
        let all_keys = self.store.get_all_keys();
        let all_locales = self.store.get_locales();
        let source_locale = &self.config.source_locale;

        // Find missing translations
        let mut missing = Vec::new();
        for key in &all_keys {
            let missing_locales = self.store.get_missing_locales(key);
            if missing_locales.is_empty() {
                continue;
            }

            // Only report if key is used in code or configured to check all
            let usages = self.used_keys.get(key).cloned().unwrap_or_default();

            let source_value = self
                .store
                .get_translation(key, source_locale)
                .unwrap_or_default();

            let suggestion = if !usages.is_empty() {
                Some(FixSuggestion {
                    action: "add_translation".to_string(),
                    files_to_edit: missing_locales
                        .iter()
                        .filter_map(|locale| self.get_locale_file_path(key, locale))
                        .collect(),
                    context: Some(format!("Translation for '{}'", key)),
                })
            } else {
                None
            };

            missing.push(MissingTranslation {
                key: key.clone(),
                source_value,
                source_locale: source_locale.clone(),
                missing_in: missing_locales,
                used_in: usages,
                suggestion,
            });
        }

        // Find unused keys
        let mut unused = Vec::new();
        for key in &all_keys {
            if !self.used_keys.contains_key(key) {
                if let Some(location) = self.store.get_translation_location(key, source_locale) {
                    unused.push(UnusedKey {
                        key: key.clone(),
                        defined_in: location,
                        suggestion: Some(FixSuggestion {
                            action: "remove_or_review".to_string(),
                            files_to_edit: vec![],
                            context: Some("Key not found in any source code".to_string()),
                        }),
                    });
                }
            }
        }

        // Find placeholder issues
        let placeholder_issues = self.validate_placeholders(&all_keys, &all_locales);

        AuditReport {
            summary: AuditSummary {
                total_keys: all_keys.len(),
                total_locales: all_locales.len(),
                missing_translations: missing.len(),
                unused_keys: unused.len(),
                placeholder_mismatches: placeholder_issues.len(),
            },
            missing,
            unused,
            placeholder_issues,
        }
    }

    fn validate_placeholders(&self, keys: &[String], locales: &[String]) -> Vec<PlaceholderIssue> {
        let mut issues = Vec::new();

        for key in keys {
            let mut locale_values = HashMap::new();
            let mut all_placeholders: HashSet<String> = HashSet::new();

            for locale in locales {
                if let Some(value) = self.store.get_translation(key, locale) {
                    let placeholders = extract_placeholders(&value);
                    for p in &placeholders {
                        all_placeholders.insert(p.clone());
                    }
                    locale_values.insert(locale.clone(), value);
                }
            }

            if all_placeholders.is_empty() {
                continue;
            }

            // Check for mismatches
            let expected: Vec<String> = all_placeholders.iter().cloned().collect();
            let mut mismatched_locales = HashMap::new();

            for (locale, value) in &locale_values {
                let placeholders = extract_placeholders(value);
                if placeholders != expected {
                    mismatched_locales.insert(locale.clone(), value.clone());
                }
            }

            if !mismatched_locales.is_empty() {
                issues.push(PlaceholderIssue {
                    key: key.clone(),
                    issue_type: PlaceholderIssueType::Mismatch,
                    locale_values: mismatched_locales,
                    expected_placeholders: expected,
                });
            }
        }

        issues
    }

    fn get_locale_file_path(&self, _key: &str, locale: &str) -> Option<PathBuf> {
        // Find the appropriate translation file for the locale
        for path in &self.config.locale_paths {
            let full_path = self.workspace_root.join(path);
            if full_path.exists() {
                // Try to find the locale file
                for ext in ["json", "yaml", "yml"] {
                    let file_path = full_path.join(format!("{}.{}", locale, ext));
                    if file_path.exists() {
                        return Some(file_path);
                    }
                }
            }
        }
        None
    }
}

fn extract_placeholders(value: &str) -> Vec<String> {
    let mut placeholders = Vec::new();

    // Match {{name}} pattern (Handlebars, Vue, etc.)
    let double_brace_regex = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();
    for cap in double_brace_regex.captures_iter(value) {
        if let Some(m) = cap.get(1) {
            placeholders.push(m.as_str().to_string());
        }
    }

    // Match {name} pattern (ICU, Flutter, etc.)
    let single_brace_regex = regex::Regex::new(r"\{(\w+)\}").unwrap();
    for cap in single_brace_regex.captures_iter(value) {
        if let Some(m) = cap.get(1) {
            let p = m.as_str().to_string();
            if !placeholders.contains(&p) {
                placeholders.push(p);
            }
        }
    }

    // Match %s, %d patterns (printf-style)
    let printf_regex = regex::Regex::new(r"%(\w)").unwrap();
    for cap in printf_regex.captures_iter(value) {
        if let Some(m) = cap.get(0) {
            placeholders.push(m.as_str().to_string());
        }
    }

    placeholders.sort();
    placeholders
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("intl-lens-{name}-{unique}"))
    }

    #[test]
    fn extracts_distinct_placeholders_from_multiple_styles() {
        let placeholders =
            extract_placeholders("Hello {{name}}, you have {count} items and %s left");

        assert_eq!(placeholders, vec!["%s", "count", "name"]);
    }

    #[test]
    fn reports_single_locale_placeholder_mismatch() {
        let workspace = temp_workspace("audit-placeholders");
        let locales_dir = workspace.join("locales");
        fs::create_dir_all(&locales_dir).expect("create locales dir");
        fs::write(
            locales_dir.join("en.json"),
            r#"{"greeting": "Hello {{name}}"}"#,
        )
        .expect("write en translations");
        fs::write(locales_dir.join("vi.json"), r#"{"greeting": "Xin chào"}"#)
            .expect("write vi translations");

        let config = I18nConfig::default();
        let store = TranslationStore::new(workspace.clone(), config.separator().to_string());
        store.scan_and_load(&config.locale_paths);

        let audit = AuditResult::new(workspace.clone(), config, store);
        let report = audit.generate_report();

        assert_eq!(report.placeholder_issues.len(), 1);
        assert_eq!(report.placeholder_issues[0].key, "greeting");

        fs::remove_dir_all(workspace).expect("cleanup temp workspace");
    }
}
