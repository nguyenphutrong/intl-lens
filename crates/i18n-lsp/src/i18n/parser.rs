use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

pub struct TranslationParser;

impl TranslationParser {
    pub fn parse_file(path: &Path) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_json(&content)
    }

    pub fn parse_json(content: &str) -> Result<HashMap<String, String>> {
        let value: Value = serde_json::from_str(content)?;
        let mut result = HashMap::new();
        Self::flatten_json(&value, String::new(), &mut result);
        Ok(result)
    }

    fn flatten_json(value: &Value, prefix: String, result: &mut HashMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_key = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::flatten_json(val, new_key, result);
                }
            }
            Value::String(s) => {
                result.insert(prefix, s.clone());
            }
            Value::Number(n) => {
                result.insert(prefix, n.to_string());
            }
            Value::Bool(b) => {
                result.insert(prefix, b.to_string());
            }
            Value::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let new_key = format!("{}.{}", prefix, i);
                    Self::flatten_json(val, new_key, result);
                }
            }
            Value::Null => {}
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
}
