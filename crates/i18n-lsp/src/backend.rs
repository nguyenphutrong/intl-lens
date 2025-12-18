use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::I18nConfig;
use crate::document::DocumentStore;
use crate::i18n::{KeyFinder, TranslationStore};

pub struct I18nBackend {
    client: Client,
    config: Arc<RwLock<I18nConfig>>,
    documents: Arc<RwLock<DocumentStore>>,
    translation_store: Arc<RwLock<Option<TranslationStore>>>,
    key_finder: Arc<RwLock<KeyFinder>>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
}

impl I18nBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: Arc::new(RwLock::new(I18nConfig::default())),
            documents: Arc::new(RwLock::new(DocumentStore::new())),
            translation_store: Arc::new(RwLock::new(None)),
            key_finder: Arc::new(RwLock::new(KeyFinder::default())),
            workspace_root: Arc::new(RwLock::new(None)),
        }
    }

    async fn initialize_workspace(&self, root: PathBuf) {
        let config = I18nConfig::load_from_workspace(&root);
        
        let key_finder = KeyFinder::new(&config.function_patterns);
        *self.key_finder.write().await = key_finder;

        let store = TranslationStore::new(root.clone());
        store.scan_and_load(&config.locale_paths);

        let locales = store.get_locales();
        let keys = store.get_all_keys();
        
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "i18n-lsp initialized: {} locales, {} keys",
                    locales.len(),
                    keys.len()
                ),
            )
            .await;

        *self.translation_store.write().await = Some(store);
        *self.config.write().await = config;
        *self.workspace_root.write().await = Some(root);
    }

    async fn diagnose_document(&self, uri: &Url, content: &str) {
        let diagnostics = self.compute_diagnostics(content).await;
        
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    async fn compute_diagnostics(&self, content: &str) -> Vec<Diagnostic> {
        let key_finder = self.key_finder.read().await;
        let found_keys = key_finder.find_keys(content);

        let translation_store = self.translation_store.read().await;

        let Some(store) = translation_store.as_ref() else {
            return vec![];
        };

        let mut diagnostics = Vec::new();

        for found_key in found_keys {
            if !store.key_exists(&found_key.key) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: found_key.line as u32,
                            character: found_key.start_char as u32,
                        },
                        end: Position {
                            line: found_key.line as u32,
                            character: found_key.end_char as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("missing-translation".to_string())),
                    source: Some("i18n".to_string()),
                    message: format!("Translation key '{}' not found", found_key.key),
                    ..Default::default()
                });
            } else {
                let missing_locales = store.get_missing_locales(&found_key.key);
                if !missing_locales.is_empty() {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: found_key.line as u32,
                                character: found_key.start_char as u32,
                            },
                            end: Position {
                                line: found_key.line as u32,
                                character: found_key.end_char as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::HINT),
                        code: Some(NumberOrString::String("incomplete-translation".to_string())),
                        source: Some("i18n".to_string()),
                        message: format!(
                            "Translation '{}' missing in: {}",
                            found_key.key,
                            missing_locales.join(", ")
                        ),
                        ..Default::default()
                    });
                }
            }
        }

        diagnostics
    }

    async fn get_hover_content(&self, key: &str) -> Option<String> {
        let translation_store = self.translation_store.read().await;
        let config = self.config.read().await;
        let store = translation_store.as_ref()?;

        let translations = store.get_all_translations(key);
        if translations.is_empty() {
            return None;
        }

        let mut content = format!("### 🌍 `{}`\n\n", key);
        
        let source_locale = &config.source_locale;
        if let Some(entry) = translations.get(source_locale) {
            content.push_str(&format!("**{}**: {}\n\n", source_locale, entry.value));
        }

        content.push_str("---\n\n");
        
        for (locale, entry) in &translations {
            if locale != source_locale {
                content.push_str(&format!("**{}**: {}\n\n", locale, entry.value));
            }
        }

        Some(content)
    }

    async fn get_completions(&self, prefix: &str) -> Vec<CompletionItem> {
        let translation_store = self.translation_store.read().await;
        let config = self.config.read().await;
        
        let Some(store) = translation_store.as_ref() else {
            return vec![];
        };

        let all_keys = store.get_all_keys();
        let source_locale = &config.source_locale;

        all_keys
            .into_iter()
            .filter(|key| key.starts_with(prefix) || prefix.is_empty())
            .take(100)
            .map(|key| {
                let translation = store.get_translation(&key, source_locale);
                CompletionItem {
                    label: key.clone(),
                    kind: Some(CompletionItemKind::TEXT),
                    detail: translation.clone(),
                    documentation: translation.map(|t| {
                        Documentation::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**{}**: {}", source_locale, t),
                        })
                    }),
                    insert_text: Some(key.clone()),
                    ..Default::default()
                }
            })
            .collect()
    }

    async fn get_definition_location(&self, key: &str) -> Option<Location> {
        let translation_store = self.translation_store.read().await;
        let config = self.config.read().await;
        let store = translation_store.as_ref()?;

        let location = store.get_translation_location(key, &config.source_locale)?;
        
        let uri = Url::from_file_path(&location.file_path).ok()?;
        
        Some(Location {
            uri,
            range: Range::default(),
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for I18nBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(root_uri) = params.root_uri {
            if let Ok(root_path) = root_uri.to_file_path() {
                self.initialize_workspace(root_path).await;
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "\"".to_string(),
                        "'".to_string(),
                        ".".to_string(),
                    ]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "i18n-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "i18n-lsp server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        let version = params.text_document.version;

        {
            let mut docs = self.documents.write().await;
            docs.open(uri.to_string(), content.clone(), version);
        }

        self.diagnose_document(&uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        
        if let Some(change) = params.content_changes.into_iter().last() {
            let content = change.text;
            let version = params.text_document.version;

            {
                let mut docs = self.documents.write().await;
                docs.update(&uri.to_string(), content.clone(), version);
            }

            self.diagnose_document(&uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.close(&params.text_document.uri.to_string());
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };

        let content = doc.content.to_string();
        let key_finder = self.key_finder.read().await;

        let Some(found_key) = key_finder.find_key_at_position(
            &content,
            position.line as usize,
            position.character as usize,
        ) else {
            return Ok(None);
        };

        let Some(hover_content) = self.get_hover_content(&found_key.key).await else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_content,
            }),
            range: Some(Range {
                start: Position {
                    line: found_key.line as u32,
                    character: found_key.start_char as u32,
                },
                end: Position {
                    line: found_key.line as u32,
                    character: found_key.end_char as u32,
                },
            }),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };

        let content = doc.content.to_string();
        let line_content: String = content
            .lines()
            .nth(position.line as usize)
            .unwrap_or("")
            .to_string();

        let prefix = Self::extract_completion_prefix(&line_content, position.character as usize);
        let completions = self.get_completions(&prefix).await;

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri.to_string()) else {
            return Ok(None);
        };

        let content = doc.content.to_string();
        let key_finder = self.key_finder.read().await;

        let Some(found_key) = key_finder.find_key_at_position(
            &content,
            position.line as usize,
            position.character as usize,
        ) else {
            return Ok(None);
        };

        let Some(location) = self.get_definition_location(&found_key.key).await else {
            return Ok(None);
        };

        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }
}

impl I18nBackend {
    fn extract_completion_prefix(line: &str, character: usize) -> String {
        let before_cursor = &line[..character.min(line.len())];
        
        let quote_patterns = ["t(\"", "t('", "$t(\"", "$t('", "i18n.t(\"", "i18n.t('"];
        
        for pattern in quote_patterns {
            if let Some(pos) = before_cursor.rfind(pattern) {
                let start = pos + pattern.len();
                return before_cursor[start..].to_string();
            }
        }

        String::new()
    }
}
