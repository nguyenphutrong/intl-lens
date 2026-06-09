use regex::Regex;

use super::namespace;

#[derive(Debug, Clone)]
pub struct FoundKey {
    pub key: String,
    pub start_offset: usize,
    pub line: usize,
    pub start_char: usize,
    pub end_char: usize,
}

pub struct KeyFinder {
    patterns: Vec<Regex>,
}

impl KeyFinder {
    pub fn new(patterns: &[String]) -> Self {
        let compiled_patterns: Vec<Regex> =
            patterns.iter().filter_map(|p| Regex::new(p).ok()).collect();

        Self {
            patterns: compiled_patterns,
        }
    }

    pub fn find_keys(&self, content: &str) -> Vec<FoundKey> {
        let mut found_keys = Vec::new();
        let namespace_context = namespace::infer_namespace_context(content);

        for pattern in &self.patterns {
            for cap in pattern.captures_iter(content) {
                if let Some(key_match) = cap.get(1) {
                    let raw_key = key_match.as_str();
                    self.push_found_key(
                        content,
                        &namespace_context,
                        raw_key,
                        key_match,
                        &mut found_keys,
                    );
                }
            }
        }

        self.find_dynamic_template_prefixes(content, &namespace_context, &mut found_keys);

        found_keys.sort_by_key(|k| k.start_offset);
        found_keys.dedup_by(|a, b| a.start_offset == b.start_offset);
        found_keys
    }

    fn push_found_key(
        &self,
        content: &str,
        namespace_context: &namespace::NamespaceContext,
        raw_key: &str,
        key_match: regex::Match<'_>,
        found_keys: &mut Vec<FoundKey>,
    ) {
        self.push_found_key_with_range(
            content,
            namespace_context,
            raw_key,
            key_match.start(),
            key_match.end(),
            found_keys,
        );
    }

    fn push_found_key_with_range(
        &self,
        content: &str,
        namespace_context: &namespace::NamespaceContext,
        raw_key: &str,
        start_offset: usize,
        end_offset: usize,
        found_keys: &mut Vec<FoundKey>,
    ) {
        if raw_key.is_empty() {
            return;
        }

        let key = namespace::apply_namespace_context(raw_key, namespace_context);

        let (line, start_char, end_char) =
            Self::offset_to_position(content, start_offset, end_offset);

        found_keys.push(FoundKey {
            key,
            start_offset,
            line,
            start_char,
            end_char,
        });
    }

    fn find_dynamic_template_prefixes(
        &self,
        content: &str,
        namespace_context: &namespace::NamespaceContext,
        found_keys: &mut Vec<FoundKey>,
    ) {
        let Ok(pattern) =
            Regex::new(r#"(?:^|[^\w.])(?:t|\$t|\$tc|\$te|i18n\.t)\s*\(\s*`([^`$]+?)\s*\$\{"#)
        else {
            return;
        };

        for cap in pattern.captures_iter(content) {
            let Some(key_match) = cap.get(1) else {
                continue;
            };

            let raw_key = key_match.as_str().trim_end_matches('.');
            let end_offset = content[key_match.start()..]
                .find('`')
                .map(|relative| key_match.start() + relative)
                .unwrap_or_else(|| key_match.end());

            self.push_found_key_with_range(
                content,
                namespace_context,
                raw_key,
                key_match.start(),
                end_offset,
                found_keys,
            );
        }
    }

    pub fn find_key_at_position(
        &self,
        content: &str,
        line: usize,
        character: usize,
    ) -> Option<FoundKey> {
        let keys = self.find_keys(content);

        keys.into_iter()
            .find(|k| k.line == line && character >= k.start_char && character <= k.end_char)
    }

    fn offset_to_position(
        content: &str,
        start_offset: usize,
        end_offset: usize,
    ) -> (usize, usize, usize) {
        let mut line = 0;
        let mut line_start = 0;

        for (i, ch) in content.char_indices() {
            if i >= start_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                line_start = i + 1;
            }
        }

        let start_char = start_offset - line_start;
        let end_char = end_offset - line_start;

        (line, start_char, end_char)
    }
}

impl Default for KeyFinder {
    fn default() -> Self {
        Self::new(&default_patterns())
    }
}

fn default_patterns() -> Vec<String> {
    vec![
        // JavaScript/TypeScript patterns
        // Match t() but not .post(), .get(), .put(), .delete(), etc.
        r#"(?:^|[^\w.])t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"i18n\.t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$tc\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$te\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"useI18n\s*\(\s*\)\s*.*?\.t\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"formatMessage\s*\(\s*\{\s*id:\s*["']([^"']+)["']"#.to_string(),
        r#"<Trans\s+i18nKey\s*=\s*["']([^"']+)["']"#.to_string(),
        // Svelte patterns (svelte-i18n)
        r#"\$_\s*\(\s*["']([^"']+)["']"#.to_string(),
        r#"\$format\s*\(\s*["']([^"']+)["']"#.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_t_function() {
        let finder = KeyFinder::default();
        let content = r#"const msg = t("hello.world");"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello.world");
    }

    #[test]
    fn test_find_dollar_t() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $t("common.button");"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.button");
    }

    #[test]
    fn test_find_multiple_keys() {
        let finder = KeyFinder::default();
        let content = r#"
            const a = t("first.key");
            const b = t("second.key");
        "#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key, "first.key");
        assert_eq!(keys[1].key, "second.key");
    }

    #[test]
    fn test_find_trans_component() {
        let finder = KeyFinder::default();
        let content = r#"<Trans i18nKey="my.key">Default</Trans>"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "my.key");
    }

    #[test]
    fn test_find_key_at_position() {
        let finder = KeyFinder::default();
        let content = r#"const msg = t("hello.world");"#;

        let found = finder.find_key_at_position(content, 0, 16);
        assert!(found.is_some());
        assert_eq!(found.unwrap().key, "hello.world");

        let not_found = finder.find_key_at_position(content, 0, 0);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_applies_single_use_translation_namespace() {
        let finder = KeyFinder::default();
        let content = r#"
            const { t } = useTranslation('common');
            const label = t('buttons.save');
        "#;

        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.buttons.save");
    }

    #[test]
    fn test_applies_first_use_translation_array_namespace() {
        let finder = KeyFinder::default();
        let content = r#"
            const { t } = useTranslation(['member', 'common']);
            const label = t('memberLabel.color');
        "#;

        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "member.memberLabel.color");
    }

    #[test]
    fn test_keeps_explicit_colon_namespace() {
        let finder = KeyFinder::default();
        let content = r#"
            const { t } = useTranslation('common');
            const label = t('common:buttons.save');
        "#;

        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common:buttons.save");
    }

    #[test]
    fn test_find_dynamic_template_prefix() {
        let finder = KeyFinder::default();
        let content = r#"
            const options = Object.keys(t('status', { returnObjects: true })).map(key => {
                return t(`status.${key}`);
            });
        "#;

        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key, "status");
        assert_eq!(keys[1].key, "status");

        let dynamic_key = &keys[1];
        assert!(dynamic_key.end_char > dynamic_key.start_char + "status".len());
        assert!(finder
            .find_key_at_position(content, dynamic_key.line, dynamic_key.start_char)
            .is_some());
        assert!(finder
            .find_key_at_position(
                content,
                dynamic_key.line,
                dynamic_key.end_char.saturating_sub(1)
            )
            .is_some());
    }

    #[test]
    fn test_does_not_double_prefix_dot_namespace() {
        let finder = KeyFinder::default();
        let content = r#"
            const { t } = useTranslation('common');
            const label = t('common.buttons.save');
        "#;

        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.buttons.save");
    }

    #[test]
    fn test_find_flutter_easy_localization_tr() {
        let finder = KeyFinder::default();
        let content = r#"Text('hello.world'.tr())"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello.world");
    }

    #[test]
    fn test_find_flutter_easy_localization_tr_function() {
        let finder = KeyFinder::default();
        let content = r#"tr('common.greeting')"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.greeting");
    }

    #[test]
    fn test_find_flutter_i18n_translate() {
        let finder = KeyFinder::default();
        let content = r#"FlutterI18n.translate(context, 'messages.welcome')"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "messages.welcome");
    }

    #[test]
    fn test_find_flutter_i18n_text_widget() {
        let finder = KeyFinder::default();
        let content = r#"I18nText('button.submit')"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "button.submit");
    }

    #[test]
    fn test_find_flutter_getx_tr() {
        let finder = KeyFinder::default();
        let content = r#"Text('hello'.tr)"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello");
    }

    #[test]
    fn test_find_flutter_getx_tr_params() {
        let finder = KeyFinder::default();
        let content = r#"'greeting'.trParams({'name': 'John'})"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "greeting");
    }

    #[test]
    fn test_should_not_match_api_methods() {
        let finder = KeyFinder::default();
        // Should NOT match .post(), .get(), .put(), .delete(), .patch(), .request()
        let test_cases = vec![
            r#"apiClient.post('/api/products')"#,
            r#"client.get('/api/users')"#,
            r#"http.put('/api/update')"#,
            r#"axios.delete('/api/remove')"#,
            r#"fetch.request('/api/data')"#,
            r#"this.httpClient.get('/users')"#,
            r#"await api.post('/endpoint')"#,
            // More realistic cases
            r#"const response = await apiClient.post('/api/products', data);"#,
            r#"return this.http.get('/api/users');"#,
            r#"apiClient.put('/api/update', { id: 1 });"#,
            // Edge cases that should NOT match
            r#"transport('/some/path')"#,
            r#"contrast('/api/test')"#,
        ];

        for content in test_cases {
            let keys = finder.find_keys(content);
            assert_eq!(
                keys.len(),
                0,
                "Should not match: {} but got {:?}",
                content,
                keys.iter().map(|k| &k.key).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_should_match_t_but_not_method_ending_with_t() {
        let finder = KeyFinder::default();
        // Should match t() but not .post(), .request(), etc.
        let content = r#"
            const msg = t("hello.world");
            apiClient.post('/api/products');
        "#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello.world");
    }

    #[test]
    fn test_find_svelte_dollar_underscore() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $_("hello.world");"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello.world");
    }

    #[test]
    fn test_find_svelte_dollar_underscore_single_quotes() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $_('common.greeting');"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.greeting");
    }

    #[test]
    fn test_find_svelte_dollar_format() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $format("hello.world");"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "hello.world");
    }

    #[test]
    fn test_find_svelte_dollar_underscore_in_template() {
        let finder = KeyFinder::default();
        let content = r#"<p>{$_("page.title")}</p>"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "page.title");
    }

    #[test]
    fn test_find_svelte_dollar_t_in_template() {
        let finder = KeyFinder::default();
        let content = r#"<h1>{$t("welcome.heading")}</h1>"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "welcome.heading");
    }

    #[test]
    fn test_find_svelte_multiple_keys() {
        let finder = KeyFinder::default();
        let content = r#"
            <h1>{$_("page.title")}</h1>
            <p>{$_("page.description")}</p>
            <button>{$t("common.submit")}</button>
        "#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].key, "page.title");
        assert_eq!(keys[1].key, "page.description");
        assert_eq!(keys[2].key, "common.submit");
    }

    #[test]
    fn test_find_vue_dollar_t() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $t('common.greeting');"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "common.greeting");
    }

    #[test]
    fn test_find_vue_dollar_tc() {
        let finder = KeyFinder::default();
        let content = r#"const msg = $tc('messages.item', count);"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "messages.item");
    }

    #[test]
    fn test_find_vue_dollar_te() {
        let finder = KeyFinder::default();
        let content = r#"if ($te('key.exists')) { }"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "key.exists");
    }

    #[test]
    fn test_find_vue_composition_api() {
        let finder = KeyFinder::default();
        let content = r#"const { t } = useI18n(); const msg = t('welcome.message');"#;
        let keys = finder.find_keys(content);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "welcome.message");
    }
}
