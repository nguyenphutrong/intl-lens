use std::path::Path;

use serde::{Deserialize, Serialize};

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
    pub fn load_from_workspace(root: &Path) -> Self {
        let config_paths = [
            root.join(".i18n-ally.json"),
            root.join("i18n-ally.config.json"),
            root.join(".zed/i18n.json"),
        ];

        for config_path in config_paths {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<I18nConfig>(&content) {
                    tracing::info!("Loaded config from {:?}", config_path);
                    return config;
                }
            }
        }

        tracing::info!("Using default config");
        Self::default()
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
        r#"t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"i18n\.t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"useTranslation\s*\(\s*\)\s*.*?t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"formatMessage\s*\(\s*\{\s*id:\s*["']([^"']+)["']"#.to_string(),
        r#"translate(?:Service)?\.(?:instant|get|stream)\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"transloco(?:Service)?\.(?:translate|selectTranslate)\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"["']([^"']+)["']\s*\|\s*(?:translate|transloco)\b"#.to_string(),
    ]
}
