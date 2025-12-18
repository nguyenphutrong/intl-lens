use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dashmap::DashMap;
use globset::Glob;
use walkdir::WalkDir;

use super::parser::TranslationParser;

#[derive(Debug, Clone)]
pub struct TranslationEntry {
    pub key: String,
    pub value: String,
    pub file_path: PathBuf,
    pub locale: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct TranslationLocation {
    pub file_path: PathBuf,
    pub locale: String,
    pub line: usize,
}

pub struct TranslationStore {
    translations: DashMap<String, HashMap<String, TranslationEntry>>,
    locale_files: DashMap<String, Vec<PathBuf>>,
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
        for locale_path in locale_paths {
            let full_path = self.workspace_root.join(locale_path);
            if full_path.exists() {
                self.scan_directory(&full_path);
            }
        }
    }

    fn scan_directory(&self, dir: &Path) {
        let json_glob = Glob::new("*.json").unwrap().compile_matcher();
        let yaml_glob = Glob::new("*.{yaml,yml}").unwrap().compile_matcher();

        for entry in WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();
            
            if path.is_file() && (json_glob.is_match(file_name) || yaml_glob.is_match(file_name)) {
                if let Some(locale) = self.extract_locale_from_path(path) {
                    self.load_translation_file(path, &locale);
                }
            }
        }
    }

    fn extract_locale_from_path(&self, path: &Path) -> Option<String> {
        let file_stem = path.file_stem()?.to_str()?;
        
        if is_locale_code(file_stem) {
            return Some(file_stem.to_string());
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

    fn load_translation_file(&self, path: &Path, locale: &str) {
        match TranslationParser::parse_file(path) {
            Ok(translations) => {
                let mut locale_map = self.translations.entry(locale.to_string()).or_default();
                
                for (key, value) in translations {
                    locale_map.insert(
                        key.clone(),
                        TranslationEntry {
                            key,
                            value,
                            file_path: path.to_path_buf(),
                            locale: locale.to_string(),
                            line: 0,
                        },
                    );
                }

                self.locale_files
                    .entry(locale.to_string())
                    .or_default()
                    .push(path.to_path_buf());

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
        self.translations
            .get(locale)
            .and_then(|map| map.get(key).map(|e| e.value.clone()))
    }

    pub fn get_all_translations(&self, key: &str) -> HashMap<String, TranslationEntry> {
        let mut result = HashMap::new();
        for entry in self.translations.iter() {
            let locale = entry.key();
            if let Some(translation) = entry.value().get(key) {
                result.insert(locale.clone(), translation.clone());
            }
        }
        result
    }

    pub fn get_translation_location(&self, key: &str, locale: &str) -> Option<TranslationLocation> {
        self.translations.get(locale).and_then(|map| {
            map.get(key).map(|e| {
                let line = Self::find_key_line_in_file(&e.file_path, key).unwrap_or(0);
                TranslationLocation {
                    file_path: e.file_path.clone(),
                    locale: e.locale.clone(),
                    line,
                }
            })
        })
    }

    fn find_key_line_in_file(file_path: &Path, key: &str) -> Option<usize> {
        let content = std::fs::read_to_string(file_path).ok()?;
        
        let last_part = key.split('.').last().unwrap_or(key);
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
        self.translations
            .iter()
            .any(|entry| entry.value().contains_key(key))
    }

    pub fn get_missing_locales(&self, key: &str) -> Vec<String> {
        let all_locales: Vec<String> = self.get_locales();
        all_locales
            .into_iter()
            .filter(|locale| {
                self.translations
                    .get(locale)
                    .map(|m| !m.contains_key(key))
                    .unwrap_or(true)
            })
            .collect()
    }

    pub fn reload(&self, locale_paths: &[String]) {
        self.translations.clear();
        self.locale_files.clear();
        self.scan_and_load(locale_paths);
    }
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
        "en", "en-US", "en-GB", "es", "es-ES", "fr", "fr-FR", "de", "de-DE",
        "it", "it-IT", "pt", "pt-BR", "ja", "ja-JP", "ko", "ko-KR", "zh",
        "zh-CN", "zh-TW", "ru", "ru-RU", "ar", "ar-SA", "vi", "vi-VN",
    ];

    common_locales.contains(&s)
}
