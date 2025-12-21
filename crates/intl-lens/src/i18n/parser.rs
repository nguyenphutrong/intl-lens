use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

pub struct TranslationParser;

impl TranslationParser {
    pub fn parse_file(path: &Path) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(path)?;
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension {
            "yaml" | "yml" => Self::parse_yaml(&content),
            "php" => Self::parse_php(&content),
            _ => Self::parse_json(&content),
        }
    }

    pub fn parse_php(content: &str) -> Result<HashMap<String, String>> {
        let mut parser = PhpParser::new(content);
        let value = parser.parse_root_array()?;
        let mut result = HashMap::new();
        flatten_php(&value, String::new(), &mut result);
        Ok(result)
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

#[derive(Debug, Clone)]
enum PhpValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
    Array(Vec<(Option<String>, PhpValue)>),
}

#[derive(Debug, Clone)]
enum PhpToken {
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Arrow,
    String(String),
    Ident(String),
    Number(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhpTokenKind {
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Arrow,
}

struct PhpLexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> PhpLexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next_token(&mut self) -> Option<PhpToken> {
        self.skip_whitespace_and_comments();

        if self.pos >= self.input.len() {
            return None;
        }

        if self.starts_with("=>") {
            self.pos += 2;
            return Some(PhpToken::Arrow);
        }

        let ch = self.next_char()?;
        let token = match ch {
            '[' => PhpToken::LBracket,
            ']' => PhpToken::RBracket,
            '(' => PhpToken::LParen,
            ')' => PhpToken::RParen,
            ',' => PhpToken::Comma,
            '\'' | '"' => PhpToken::String(self.read_string(ch)),
            ';' => return self.next_token(),
            _ if ch.is_ascii_digit() || ch == '-' => {
                let mut number = String::new();
                number.push(ch);
                number.push_str(&self.read_number());
                PhpToken::Number(number)
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let mut ident = String::new();
                ident.push(ch);
                ident.push_str(&self.read_ident());
                PhpToken::Ident(ident)
            }
            _ => return self.next_token(),
        };

        Some(token)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while self.peek_char().is_some_and(|ch| ch.is_whitespace()) {
                self.next_char();
            }

            if self.starts_with("//") {
                self.consume_until("\n");
                continue;
            }

            if self.starts_with("#") {
                self.consume_until("\n");
                continue;
            }

            if self.starts_with("/*") {
                self.pos += 2;
                self.consume_until("*/");
                continue;
            }

            break;
        }
    }

    fn consume_until(&mut self, delimiter: &str) {
        while self.pos < self.input.len() {
            if self.starts_with(delimiter) {
                self.pos += delimiter.len();
                break;
            }
            self.next_char();
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn read_string(&mut self, quote: char) -> String {
        let mut result = String::new();

        while let Some(ch) = self.next_char() {
            if ch == quote {
                break;
            }

            if ch == '\\' {
                if let Some(escaped) = self.next_char() {
                    let unescaped = match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '\'' => '\'',
                        '"' => '"',
                        other => other,
                    };
                    result.push(unescaped);
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn read_ident(&mut self) -> String {
        let mut result = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                result.push(ch);
                self.next_char();
            } else {
                break;
            }
        }
        result
    }

    fn read_number(&mut self) -> String {
        let mut result = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() || ch == '.' {
                result.push(ch);
                self.next_char();
            } else {
                break;
            }
        }
        result
    }
}

struct PhpParser<'a> {
    lexer: PhpLexer<'a>,
    lookahead: Option<PhpToken>,
}

impl<'a> PhpParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lexer: PhpLexer::new(input),
            lookahead: None,
        }
    }

    fn parse_root_array(&mut self) -> Result<PhpValue> {
        while let Some(token) = self.peek_token() {
            match token {
                PhpToken::LBracket => return self.parse_array(),
                PhpToken::Ident(ref ident) if ident == "array" => return self.parse_array(),
                _ => {
                    self.next_token();
                }
            }
        }

        bail!("No PHP array found")
    }

    fn parse_array(&mut self) -> Result<PhpValue> {
        let end_kind = match self.next_token() {
            Some(PhpToken::LBracket) => PhpTokenKind::RBracket,
            Some(PhpToken::Ident(ident)) if ident == "array" => {
                self.expect_kind(PhpTokenKind::LParen)?;
                PhpTokenKind::RParen
            }
            _ => bail!("Expected array start"),
        };

        let mut items = Vec::new();

        loop {
            if self.peek_kind() == Some(end_kind) {
                self.next_token();
                break;
            }

            if self.peek_token().is_none() {
                break;
            }

            let key_or_value = self.parse_value()?;

            if self.consume_kind(PhpTokenKind::Arrow) {
                let key = value_to_key(&key_or_value);
                let value = self.parse_value()?;
                if !key.is_empty() {
                    items.push((Some(key), value));
                }
            } else {
                items.push((None, key_or_value));
            }

            self.consume_kind(PhpTokenKind::Comma);
        }

        Ok(PhpValue::Array(items))
    }

    fn parse_value(&mut self) -> Result<PhpValue> {
        match self.peek_token() {
            Some(PhpToken::LBracket) => self.parse_array(),
            Some(PhpToken::Ident(ref ident)) if ident == "array" => self.parse_array(),
            Some(PhpToken::String(_)) => match self.next_token() {
                Some(PhpToken::String(value)) => Ok(PhpValue::String(value)),
                _ => bail!("Expected string"),
            },
            Some(PhpToken::Number(_)) => match self.next_token() {
                Some(PhpToken::Number(value)) => Ok(PhpValue::Number(value)),
                _ => bail!("Expected number"),
            },
            Some(PhpToken::Ident(_)) => match self.next_token() {
                Some(PhpToken::Ident(ident)) => match ident.as_str() {
                    "true" => Ok(PhpValue::Bool(true)),
                    "false" => Ok(PhpValue::Bool(false)),
                    "null" => Ok(PhpValue::Null),
                    _ => Ok(PhpValue::String(ident)),
                },
                _ => bail!("Expected identifier"),
            },
            Some(token) => {
                self.next_token();
                bail!("Unexpected token: {:?}", token)
            }
            None => bail!("Unexpected end of input"),
        }
    }

    fn peek_token(&mut self) -> Option<PhpToken> {
        if self.lookahead.is_none() {
            self.lookahead = self.lexer.next_token();
        }
        self.lookahead.clone()
    }

    fn next_token(&mut self) -> Option<PhpToken> {
        if self.lookahead.is_some() {
            return self.lookahead.take();
        }
        self.lexer.next_token()
    }

    fn expect_kind(&mut self, kind: PhpTokenKind) -> Result<()> {
        if self.peek_kind() == Some(kind) {
            self.next_token();
            Ok(())
        } else {
            bail!("Expected token kind: {:?}", kind)
        }
    }

    fn consume_kind(&mut self, kind: PhpTokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn peek_kind(&mut self) -> Option<PhpTokenKind> {
        self.peek_token().and_then(token_kind)
    }
}

fn token_kind(token: PhpToken) -> Option<PhpTokenKind> {
    match token {
        PhpToken::LBracket => Some(PhpTokenKind::LBracket),
        PhpToken::RBracket => Some(PhpTokenKind::RBracket),
        PhpToken::LParen => Some(PhpTokenKind::LParen),
        PhpToken::RParen => Some(PhpTokenKind::RParen),
        PhpToken::Comma => Some(PhpTokenKind::Comma),
        PhpToken::Arrow => Some(PhpTokenKind::Arrow),
        _ => None,
    }
}

fn value_to_key(value: &PhpValue) -> String {
    match value {
        PhpValue::String(value) => value.clone(),
        PhpValue::Number(value) => value.clone(),
        PhpValue::Bool(value) => value.to_string(),
        PhpValue::Null => String::new(),
        PhpValue::Array(_) => String::new(),
    }
}

fn flatten_php(value: &PhpValue, prefix: String, result: &mut HashMap<String, String>) {
    match value {
        PhpValue::String(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.clone());
            }
        }
        PhpValue::Number(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.clone());
            }
        }
        PhpValue::Bool(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.to_string());
            }
        }
        PhpValue::Null => {}
        PhpValue::Array(items) => {
            let mut list_index = 0;
            for (key_opt, entry) in items {
                let key = match key_opt {
                    Some(key) => key.clone(),
                    None => {
                        let index = list_index.to_string();
                        list_index += 1;
                        index
                    }
                };

                if key.is_empty() {
                    continue;
                }

                let new_prefix = if prefix.is_empty() {
                    key
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_php(entry, new_prefix, result);
            }
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

    #[test]
    fn test_parse_flat_php() {
        let php = r#"<?php return ['hello' => 'Hello', "world" => "World"];"#;
        let result = TranslationParser::parse_php(php).unwrap();
        assert_eq!(result.get("hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("world"), Some(&"World".to_string()));
    }

    #[test]
    fn test_parse_nested_php() {
        let php = r#"<?php
        return [
            'common' => [
                'hello' => 'Hello',
                'bye' => "Goodbye",
            ],
        ];"#;
        let result = TranslationParser::parse_php(php).unwrap();
        assert_eq!(result.get("common.hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("common.bye"), Some(&"Goodbye".to_string()));
    }
}
