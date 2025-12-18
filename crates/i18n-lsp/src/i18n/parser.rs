use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

pub struct TranslationParser;

impl TranslationParser {
    pub fn parse_file(path: &Path) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(path)?;
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        match extension {
            "yaml" | "yml" => Self::parse_yaml(&content),
            "json" | _ => Self::parse_json(&content),
        }
    }

    pub fn parse_json(content: &str) -> Result<HashMap<String, String>> {
        let value: JsonValue = serde_json::from_str(content)?;
        let mut result = HashMap::new();
        Self::flatten_json(&value, String::new(), &mut result);
        Ok(result)
    }

    pub fn parse_yaml(content: &str) -> Result<HashMap<String, String>> {
        let value: YamlValue = serde_yaml::from_str(content)?;
        let mut result = HashMap::new();
        Self::flatten_yaml(&value, String::new(), &mut result);
        Ok(result)
    }

    fn flatten_json(value: &JsonValue, prefix: String, result: &mut HashMap<String, String>) {
        match value {
            JsonValue::Object(map) => {
                for (key, val) in map {
                    let new_key = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::flatten_json(val, new_key, result);
                }
            }
            JsonValue::String(s) => {
                result.insert(prefix, s.clone());
            }
            JsonValue::Number(n) => {
                result.insert(prefix, n.to_string());
            }
            JsonValue::Bool(b) => {
                result.insert(prefix, b.to_string());
            }
            JsonValue::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let new_key = format!("{}.{}", prefix, i);
                    Self::flatten_json(val, new_key, result);
                }
            }
            JsonValue::Null => {}
        }
    }

    fn flatten_yaml(value: &YamlValue, prefix: String, result: &mut HashMap<String, String>) {
        match value {
            YamlValue::Mapping(map) => {
                for (key, val) in map {
                    let key_str = match key {
                        YamlValue::String(s) => s.clone(),
                        _ => key.as_str().unwrap_or("").to_string(),
                    };
                    let new_key = if prefix.is_empty() {
                        key_str
                    } else {
                        format!("{}.{}", prefix, key_str)
                    };
                    Self::flatten_yaml(val, new_key, result);
                }
            }
            YamlValue::String(s) => {
                result.insert(prefix, s.clone());
            }
            YamlValue::Number(n) => {
                result.insert(prefix, n.to_string());
            }
            YamlValue::Bool(b) => {
                result.insert(prefix, b.to_string());
            }
            YamlValue::Sequence(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let new_key = format!("{}.{}", prefix, i);
                    Self::flatten_yaml(val, new_key, result);
                }
            }
            YamlValue::Null | YamlValue::Tagged(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flat_json() {
        let json = r#"{"hello": "Hello", "world": "World"}"#;
        let result = TranslationParser::parse_json(json).unwrap();
        assert_eq!(result.get("hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("world"), Some(&"World".to_string()));
    }

    #[test]
    fn test_parse_nested_json() {
        let json = r#"{"common": {"hello": "Hello", "bye": "Goodbye"}}"#;
        let result = TranslationParser::parse_json(json).unwrap();
        assert_eq!(result.get("common.hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("common.bye"), Some(&"Goodbye".to_string()));
    }

    #[test]
    fn test_parse_deeply_nested() {
        let json = r#"{"a": {"b": {"c": "deep"}}}"#;
        let result = TranslationParser::parse_json(json).unwrap();
        assert_eq!(result.get("a.b.c"), Some(&"deep".to_string()));
    }

    #[test]
    fn test_parse_flat_yaml() {
        let yaml = "hello: Hello\nworld: World";
        let result = TranslationParser::parse_yaml(yaml).unwrap();
        assert_eq!(result.get("hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("world"), Some(&"World".to_string()));
    }

    #[test]
    fn test_parse_nested_yaml() {
        let yaml = "common:\n  hello: Hello\n  bye: Goodbye";
        let result = TranslationParser::parse_yaml(yaml).unwrap();
        assert_eq!(result.get("common.hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("common.bye"), Some(&"Goodbye".to_string()));
    }
}
