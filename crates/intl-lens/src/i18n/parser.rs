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
            "arb" => Self::parse_arb(&content),
            "js" => Self::parse_js(&content),
            _ => Self::parse_json(&content),
        }
    }

    /// Parse ARB (Application Resource Bundle) files used by Flutter.
    /// ARB is JSON-based but contains metadata keys (starting with @ or @@) that should be filtered.
    pub fn parse_arb(content: &str) -> Result<HashMap<String, String>> {
        let value: JsonValue = serde_json::from_str(content)?;
        let mut result = HashMap::new();

        if let JsonValue::Object(map) = value {
            for (key, val) in map {
                // Skip metadata keys: @@locale, @keyName (descriptions), etc.
                if key.starts_with('@') {
                    continue;
                }

                // Only include string values (translations)
                if let JsonValue::String(s) = val {
                    result.insert(key, s);
                }
            }
        }

        Ok(result)
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

    pub fn parse_js(content: &str) -> Result<HashMap<String, String>> {
        let mut parser = JsParser::new(content);
        let value = parser.parse_root_object()?;
        let mut result = HashMap::new();
        flatten_js(&value, String::new(), &mut result);
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

#[derive(Debug, Clone)]
enum JsValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
    Object(Vec<(String, JsValue)>),
    Array(Vec<JsValue>),
}

#[derive(Debug, Clone)]
enum JsToken {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    String(String),
    Ident(String),
    Number(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsTokenKind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
}

struct JsLexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> JsLexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next_token(&mut self) -> Option<JsToken> {
        self.skip_whitespace_and_comments();

        if self.pos >= self.input.len() {
            return None;
        }

        let ch = self.next_char()?;
        let token = match ch {
            '{' => JsToken::LBrace,
            '}' => JsToken::RBrace,
            '[' => JsToken::LBracket,
            ']' => JsToken::RBracket,
            ':' => JsToken::Colon,
            ',' => JsToken::Comma,
            '\'' | '"' | '`' => JsToken::String(self.read_string(ch)),
            _ if ch.is_ascii_digit() || ch == '-' => {
                let mut number = String::new();
                number.push(ch);
                number.push_str(&self.read_number());
                JsToken::Number(number)
            }
            _ if ch.is_alphabetic() || ch == '_' || ch == '$' => {
                let mut ident = String::new();
                ident.push(ch);
                ident.push_str(&self.read_ident());
                JsToken::Ident(ident)
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
                        '`' => '`',
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
            if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '-' {
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

struct JsParser<'a> {
    lexer: JsLexer<'a>,
    lookahead: Option<JsToken>,
}

impl<'a> JsParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lexer: JsLexer::new(input),
            lookahead: None,
        }
    }

    fn parse_root_object(&mut self) -> Result<JsValue> {
        while let Some(token) = self.peek_token() {
            match token {
                JsToken::LBrace => return self.parse_object(),
                _ => {
                    self.next_token();
                }
            }
        }

        bail!("No JavaScript object found")
    }

    fn parse_object(&mut self) -> Result<JsValue> {
        self.expect_kind(JsTokenKind::LBrace)?;
        let mut items = Vec::new();

        loop {
            if self.peek_kind() == Some(JsTokenKind::RBrace) {
                self.next_token();
                break;
            }

            if self.peek_token().is_none() {
                bail!("Unexpected end of input while parsing object");
            }

            let key = self.parse_object_key()?;
            self.expect_kind(JsTokenKind::Colon)?;
            let value = self.parse_value()?;
            items.push((key, value));

            if !self.consume_kind(JsTokenKind::Comma)
                && self.peek_kind() != Some(JsTokenKind::RBrace)
            {
                bail!("Expected ',' or '}}' in object");
            }
        }

        Ok(JsValue::Object(items))
    }

    fn parse_array(&mut self) -> Result<JsValue> {
        self.expect_kind(JsTokenKind::LBracket)?;
        let mut items = Vec::new();

        loop {
            if self.peek_kind() == Some(JsTokenKind::RBracket) {
                self.next_token();
                break;
            }

            if self.peek_token().is_none() {
                bail!("Unexpected end of input while parsing array");
            }

            items.push(self.parse_value()?);

            if !self.consume_kind(JsTokenKind::Comma)
                && self.peek_kind() != Some(JsTokenKind::RBracket)
            {
                bail!("Expected ',' or ']' in array");
            }
        }

        Ok(JsValue::Array(items))
    }

    fn parse_object_key(&mut self) -> Result<String> {
        match self.next_token() {
            Some(JsToken::String(value)) => Ok(value),
            Some(JsToken::Ident(value)) => Ok(value),
            Some(JsToken::Number(value)) => Ok(value),
            other => bail!("Unexpected object key token: {:?}", other),
        }
    }

    fn parse_value(&mut self) -> Result<JsValue> {
        match self.peek_token() {
            Some(JsToken::LBrace) => self.parse_object(),
            Some(JsToken::LBracket) => self.parse_array(),
            Some(JsToken::String(_)) => match self.next_token() {
                Some(JsToken::String(value)) => Ok(JsValue::String(value)),
                _ => bail!("Expected string"),
            },
            Some(JsToken::Number(_)) => match self.next_token() {
                Some(JsToken::Number(value)) => Ok(JsValue::Number(value)),
                _ => bail!("Expected number"),
            },
            Some(JsToken::Ident(_)) => match self.next_token() {
                Some(JsToken::Ident(ident)) => match ident.as_str() {
                    "true" => Ok(JsValue::Bool(true)),
                    "false" => Ok(JsValue::Bool(false)),
                    "null" | "undefined" => Ok(JsValue::Null),
                    _ => Ok(JsValue::String(ident)),
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

    fn peek_token(&mut self) -> Option<JsToken> {
        if self.lookahead.is_none() {
            self.lookahead = self.lexer.next_token();
        }
        self.lookahead.clone()
    }

    fn next_token(&mut self) -> Option<JsToken> {
        if self.lookahead.is_some() {
            return self.lookahead.take();
        }
        self.lexer.next_token()
    }

    fn expect_kind(&mut self, kind: JsTokenKind) -> Result<()> {
        if self.peek_kind() == Some(kind) {
            self.next_token();
            Ok(())
        } else {
            bail!("Expected token kind: {:?}", kind)
        }
    }

    fn consume_kind(&mut self, kind: JsTokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn peek_kind(&mut self) -> Option<JsTokenKind> {
        self.peek_token().and_then(js_token_kind)
    }
}

fn js_token_kind(token: JsToken) -> Option<JsTokenKind> {
    match token {
        JsToken::LBrace => Some(JsTokenKind::LBrace),
        JsToken::RBrace => Some(JsTokenKind::RBrace),
        JsToken::LBracket => Some(JsTokenKind::LBracket),
        JsToken::RBracket => Some(JsTokenKind::RBracket),
        JsToken::Colon => Some(JsTokenKind::Colon),
        JsToken::Comma => Some(JsTokenKind::Comma),
        _ => None,
    }
}

fn flatten_js(value: &JsValue, prefix: String, result: &mut HashMap<String, String>) {
    match value {
        JsValue::String(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.clone());
            }
        }
        JsValue::Number(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.clone());
            }
        }
        JsValue::Bool(value) => {
            if !prefix.is_empty() {
                result.insert(prefix, value.to_string());
            }
        }
        JsValue::Null => {}
        JsValue::Object(items) => {
            for (key, entry) in items {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_js(entry, new_prefix, result);
            }
        }
        JsValue::Array(items) => {
            for (index, entry) in items.iter().enumerate() {
                let new_prefix = if prefix.is_empty() {
                    index.to_string()
                } else {
                    format!("{}.{}", prefix, index)
                };
                flatten_js(entry, new_prefix, result);
            }
        }
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

    #[test]
    fn test_parse_arb_basic() {
        let arb = r#"{
            "@@locale": "en",
            "helloWorld": "Hello World!",
            "@helloWorld": {
                "description": "The conventional greeting"
            },
            "greeting": "Hello {name}",
            "@greeting": {
                "description": "A greeting with name",
                "placeholders": {
                    "name": {
                        "type": "String"
                    }
                }
            }
        }"#;
        let result = TranslationParser::parse_arb(arb).unwrap();
        assert_eq!(result.get("helloWorld"), Some(&"Hello World!".to_string()));
        assert_eq!(result.get("greeting"), Some(&"Hello {name}".to_string()));
        // Metadata keys should be filtered out
        assert!(!result.contains_key("@@locale"));
        assert!(!result.contains_key("@helloWorld"));
        assert!(!result.contains_key("@greeting"));
    }

    #[test]
    fn test_parse_arb_with_plurals() {
        let arb = r#"{
            "@@locale": "en",
            "itemCount": "{count, plural, =0{no items} =1{1 item} other{{count} items}}",
            "@itemCount": {
                "placeholders": {
                    "count": {
                        "type": "num"
                    }
                }
            }
        }"#;
        let result = TranslationParser::parse_arb(arb).unwrap();
        assert_eq!(
            result.get("itemCount"),
            Some(&"{count, plural, =0{no items} =1{1 item} other{{count} items}}".to_string())
        );
        assert!(!result.contains_key("@itemCount"));
    }

    #[test]
    fn test_parse_js_export_default_object() {
        let js = r#"
            export default {
                common: {
                    hello: "Hello",
                    bye: 'Goodbye',
                },
                enabled: true,
                items: ["One", "Two"],
            };
        "#;
        let result = TranslationParser::parse_js(js).unwrap();
        assert_eq!(result.get("common.hello"), Some(&"Hello".to_string()));
        assert_eq!(result.get("common.bye"), Some(&"Goodbye".to_string()));
        assert_eq!(result.get("enabled"), Some(&"true".to_string()));
        assert_eq!(result.get("items.0"), Some(&"One".to_string()));
        assert_eq!(result.get("items.1"), Some(&"Two".to_string()));
    }

    #[test]
    fn test_parse_js_module_exports_with_comments() {
        let js = r#"
            // locale definitions
            module.exports = {
                greeting: `Hello`,
                /* keep nested structure */
                nested: {
                    count: 3,
                },
            };
        "#;
        let result = TranslationParser::parse_js(js).unwrap();
        assert_eq!(result.get("greeting"), Some(&"Hello".to_string()));
        assert_eq!(result.get("nested.count"), Some(&"3".to_string()));
    }
}
