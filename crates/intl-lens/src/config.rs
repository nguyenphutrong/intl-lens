use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct I18nConfig {
    #[serde(default = "default_locale_paths")]
    pub locale_paths: Vec<String>,

    #[serde(default = "default_source_locale")]
    pub source_locale: String,

    #[serde(default = "default_key_style")]
    pub key_style: KeyStyle,

    #[serde(default)]
    pub namespace_enabled: bool,

    #[serde(default = "default_function_patterns")]
    pub function_patterns: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct I18nConfigOverrides {
    pub locale_paths: Option<Vec<String>>,
    pub source_locale: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyStyle {
    #[default]
    Nested,
    Flat,
    Auto,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            locale_paths: default_locale_paths(),
            source_locale: default_source_locale(),
            key_style: default_key_style(),
            namespace_enabled: false,
            function_patterns: default_function_patterns(),
        }
    }
}

impl I18nConfig {
    pub fn effective_with_overrides(root: &Path, overrides: &I18nConfigOverrides) -> Self {
        let config_paths = [
            root.join(".i18n-ally.json"),
            root.join("i18n-ally.config.json"),
        ];

        for config_path in config_paths {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                let raw_config = serde_json::from_str::<Value>(&content).ok();

                if let Ok(mut config) = serde_json::from_str::<I18nConfig>(&content) {
                    let has_locale_paths = raw_config
                        .as_ref()
                        .and_then(|value| value.as_object())
                        .is_some_and(|object| {
                            object.contains_key("localePaths")
                                || object.contains_key("locale_paths")
                        });

                    if !has_locale_paths {
                        config.add_detected_locale_paths(root);
                    }

                    config.apply_overrides(overrides);
                    tracing::info!("Loaded config from {:?}", config_path);
                    return config;
                }
            }
        }

        tracing::info!("Using default config");
        let mut config = Self::default();
        config.add_detected_locale_paths(root);
        config.apply_overrides(overrides);
        config
    }

    pub fn load_from_workspace_with_overrides(
        root: &Path,
        overrides: &I18nConfigOverrides,
    ) -> Self {
        Self::effective_with_overrides(root, overrides)
    }

    pub fn apply_overrides(&mut self, overrides: &I18nConfigOverrides) {
        if let Some(locale_paths) = overrides.sanitized_locale_paths() {
            self.locale_paths = locale_paths;
        }

        if let Some(source_locale) = overrides.sanitized_source_locale() {
            self.source_locale = source_locale;
        }
    }

    fn add_detected_locale_paths(&mut self, root: &Path) {
        let detected_paths = detect_framework_locale_paths(root);
        if detected_paths.is_empty() {
            return;
        }

        let mut existing: HashSet<String> = self.locale_paths.iter().cloned().collect();
        for path in detected_paths {
            if existing.insert(path.clone()) {
                self.locale_paths.push(path);
            }
        }
    }
}

impl I18nConfigOverrides {
    pub fn sanitized_locale_paths(&self) -> Option<Vec<String>> {
        self.locale_paths.as_ref().and_then(|paths| {
            let sanitized: Vec<String> = paths
                .iter()
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .map(ToOwned::to_owned)
                .collect();

            (!sanitized.is_empty()).then_some(sanitized)
        })
    }

    pub fn sanitized_source_locale(&self) -> Option<String> {
        self.source_locale
            .as_deref()
            .map(str::trim)
            .filter(|locale| !locale.is_empty())
            .map(ToOwned::to_owned)
    }
}

fn default_locale_paths() -> Vec<String> {
    vec![
        "locales".to_string(),
        "i18n".to_string(),
        "translations".to_string(),
        "public/locales".to_string(),
        "src/locales".to_string(),
        "src/i18n".to_string(),
    ]
}

fn default_source_locale() -> String {
    "en".to_string()
}

fn default_key_style() -> KeyStyle {
    KeyStyle::Auto
}

fn default_function_patterns() -> Vec<String> {
    vec![
        // JavaScript/TypeScript patterns
        // Match t() but not .post(), .get(), .put(), .delete(), etc.
        r#"(?:^|[^\w.])t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"i18n\.t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"useTranslation\s*\(\s*\)\s*.*?t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$tc\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$te\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"useI18n\s*\(\s*\)\s*.*?\.t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"formatMessage\s*\(\s*\{\s*id:\s*["']([^"']+)["']"#.to_string(),
        // Angular patterns
        r#"translateService\.(?:instant|get|stream)\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"translocoService\.(?:translate|selectTranslate)\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"["']([^"']+)["']\s*\|\s*(?:translate|transloco)\b"#.to_string(),
        // PHP/Laravel patterns
        r#"__\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"trans(?:_choice)?\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"Lang::(?:get|choice)\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"@lang\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"@choice\s*\(\s*["']([^"']+)["']"#.to_string(),
        // Flutter/Dart patterns - easy_localization
        r#"['"]([^'"]+)['"]\s*\.tr\("#.to_string(),
        r#"['"]([^'"]+)['"]\s*\.tr\(\)"#.to_string(),
        r#"(?:^|[^\w.])tr\(\s*['"]([^'"]+)['"]"#.to_string(),
        r#"context\.tr\(\s*['"]([^'"]+)['"]"#.to_string(),
        r#"['"]([^'"]+)['"]\s*\.plural\("#.to_string(),
        // Flutter/Dart patterns - flutter_i18n
        r#"FlutterI18n\.translate\([^,]+,\s*['"]([^'"]+)['"]"#.to_string(),
        r#"FlutterI18n\.plural\([^,]+,\s*['"]([^'"]+)['"]"#.to_string(),
        r#"I18nText\(\s*['"]([^'"]+)['"]"#.to_string(),
        r#"I18nPlural\(\s*['"]([^'"]+)['"]"#.to_string(),
        // Flutter/Dart patterns - GetX
        r#"['"]([^'"]+)['"]\s*\.tr(?:\s|$|\)|,)"#.to_string(),
        r#"['"]([^'"]+)['"]\s*\.trParams\("#.to_string(),
        r#"['"]([^'"]+)['"]\s*\.trPlural\("#.to_string(),
    ]
}

fn detect_framework_locale_paths(root: &Path) -> Vec<String> {
    let mut paths = Vec::new();

    if is_angular_project(root) {
        paths.push("src/assets/i18n".to_string());
    }

    if is_laravel_project(root) {
        paths.push("resources/lang".to_string());
        paths.push("lang".to_string());
    }

    if is_flutter_project(root) {
        // Check l10n.yaml for custom arb-dir
        if let Some(arb_dir) = parse_l10n_yaml(root) {
            paths.push(arb_dir);
        }
        // Default Flutter locale paths
        paths.push("lib/l10n".to_string());
        paths.push("assets/translations".to_string());
        paths.push("assets/flutter_i18n".to_string());
        paths.push("assets/i18n".to_string());
    }

    if is_vue_project(root) {
        paths.push("src/locales".to_string());
        paths.push("src/i18n".to_string());
        paths.push("locales".to_string());
        paths.push("i18n".to_string());
        paths.push("public/locales".to_string());
    }

    paths
        .into_iter()
        .filter(|path| root.join(path).exists())
        .collect()
}

fn is_angular_project(root: &Path) -> bool {
    let package_json = root.join("package.json");
    let Some(value) = read_json(&package_json) else {
        return false;
    };

    json_has_dependency(
        &value,
        "@angular/core",
        &["dependencies", "devDependencies"],
    ) || json_has_dependency(&value, "@angular/cli", &["dependencies", "devDependencies"])
}

fn is_laravel_project(root: &Path) -> bool {
    let composer_json = root.join("composer.json");
    let Some(value) = read_json(&composer_json) else {
        return false;
    };

    json_has_dependency(&value, "laravel/framework", &["require", "require-dev"])
        || json_has_name(&value, "laravel/laravel")
}

fn read_json(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&content).ok()
}

fn json_has_dependency(value: &Value, dependency: &str, sections: &[&str]) -> bool {
    sections.iter().any(|section| {
        value
            .get(*section)
            .and_then(|deps| deps.as_object())
            .is_some_and(|deps| deps.contains_key(dependency))
    })
}

fn json_has_name(value: &Value, name: &str) -> bool {
    value.get("name").and_then(|v| v.as_str()) == Some(name)
}

fn is_flutter_project(root: &Path) -> bool {
    let pubspec = root.join("pubspec.yaml");
    let Ok(content) = std::fs::read_to_string(&pubspec) else {
        return false;
    };

    // Check for flutter sdk dependency in pubspec.yaml
    content.contains("flutter:") && content.contains("sdk: flutter")
}

fn parse_l10n_yaml(root: &Path) -> Option<String> {
    let l10n_yaml = root.join("l10n.yaml");
    let content = std::fs::read_to_string(&l10n_yaml).ok()?;

    // Parse l10n.yaml to find arb-dir
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    yaml.get("arb-dir")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn is_vue_project(root: &Path) -> bool {
    let package_json = root.join("package.json");
    let Some(value) = read_json(&package_json) else {
        return false;
    };

    json_has_dependency(&value, "vue", &["dependencies", "devDependencies"])
        || json_has_dependency(&value, "vue-i18n", &["dependencies", "devDependencies"])
        || json_has_dependency(
            &value,
            "@intlify/vue-i18n",
            &["dependencies", "devDependencies"],
        )
        || json_has_dependency(&value, "@nuxtjs/i18n", &["dependencies", "devDependencies"])
        || root.join("vue.config.js").exists()
        || root.join("vite.config.js").exists()
        || root.join("vite.config.ts").exists()
        || root.join("nuxt.config.js").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_effective_config_applies_overrides_over_defaults() {
        let dir = tempdir().unwrap();
        let overrides = I18nConfigOverrides {
            locale_paths: Some(vec!["apps/admin/src/assets/locales".into()]),
            source_locale: Some("vi".into()),
        };

        let config = I18nConfig::effective_with_overrides(dir.path(), &overrides);

        assert_eq!(
            config.locale_paths,
            vec!["apps/admin/src/assets/locales".to_string()]
        );
        assert_eq!(config.source_locale, "vi".to_string());
    }

    #[test]
    fn test_load_from_workspace_with_overrides_matches_effective_config() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join(".i18n-ally.json"),
            r#"{
                "localePaths": ["src/locales"],
                "sourceLocale": "en"
            }"#,
        )
        .unwrap();

        let overrides = I18nConfigOverrides {
            locale_paths: Some(vec!["apps/admin/src/assets/locales".into()]),
            source_locale: Some("vi".into()),
        };

        let config = I18nConfig::load_from_workspace_with_overrides(dir.path(), &overrides);
        let effective = I18nConfig::effective_with_overrides(dir.path(), &overrides);

        assert_eq!(config.locale_paths, effective.locale_paths);
        assert_eq!(config.source_locale, effective.source_locale);
    }
}
