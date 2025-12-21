use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{Client, LanguageServer};

use crate::config::I18nConfig;
use crate::document::DocumentStore;
use crate::i18n::{KeyFinder, TranslationStore};

fn truncate_string(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }

    let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{}...", truncated)
}

pub struct I18nBackend {
    client: Client,
    config: Arc<RwLock<I18nConfig>>,
    documents: Arc<RwLock<DocumentStore>>,
    translation_store: Arc<RwLock<Option<TranslationStore>>>,
    key_finder: Arc<RwLock<KeyFinder>>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
    inlay_hint_dynamic_registration_supported: Arc<RwLock<bool>>,
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
            inlay_hint_dynamic_registration_supported: Arc::new(RwLock::new(false)),
        }
    }

    async fn initialize_workspace(&self, root: PathBuf) {
        tracing::info!("Initializing workspace at {:?}", root);

        let config = I18nConfig::load_from_workspace(&root);
        tracing::info!("Config loaded, locale_paths: {:?}", config.locale_paths);

        let key_finder = KeyFinder::new(&config.function_patterns);
        *self.key_finder.write().await = key_finder;

        let store = TranslationStore::new(root.clone());
        store.scan_and_load(&config.locale_paths);

        let locales = store.get_locales();
        let keys = store.get_all_keys();

        tracing::info!("Found {} locales: {:?}", locales.len(), locales);
        tracing::info!("Found {} translation keys", keys.len());

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "i18n-lsp initialized: {} locales, {} keys in {:?}",
                    locales.len(),
                    keys.len(),
                    root
                ),
            )
            .await;

        *self.translation_store.write().await = Some(store);
        *self.config.write().await = config;
        *self.workspace_root.write().await = Some(root);
    }

    async fn register_inlay_hint_capability(&self) {
        let supports_dynamic = *self
            .inlay_hint_dynamic_registration_supported
            .read()
            .await;

        if !supports_dynamic {
            tracing::debug!(
                "Skipping inlay hint dynamic registration (dynamicRegistration=false)"
            );
            return;
        }

        let document_selector = Some(vec![
            DocumentFilter {
                language: Some("typescript".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("typescriptreact".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("javascript".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("javascriptreact".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("html".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("angular".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("php".to_string()),
                scheme: None,
                pattern: None,
            },
            DocumentFilter {
                language: Some("blade".to_string()),
                scheme: None,
                pattern: None,
            },
        ]);

        let register_options = InlayHintRegistrationOptions {
            inlay_hint_options: InlayHintOptions {
                resolve_provider: Some(false),
                work_done_progress_options: Default::default(),
            },
            text_document_registration_options: TextDocumentRegistrationOptions {
                document_selector,
            },
            static_registration_options: StaticRegistrationOptions {
                id: Some("intl-lens-inlay-hint".to_string()),
            },
        };

        let register_options = match serde_json::to_value(register_options) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(
                    "Failed to serialize inlay hint registration options: {:?}",
                    err
                );
                return;
            }
        };

        let registration = Registration {
            id: "intl-lens-inlay-hint".to_string(),
            method: "textDocument/inlayHint".to_string(),
            register_options: Some(register_options),
        };

        match self.client.register_capability(vec![registration]).await {
            Ok(_) => tracing::info!("Registered inlay hint capability dynamically"),
            Err(err) => tracing::warn!(
                "Dynamic inlay hint registration failed: {:?}",
                err
            ),
        }
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
            range: Range {
                start: Position {
                    line: location.line as u32,
                    character: 0,
                },
                end: Position {
                    line: location.line as u32,
                    character: 0,
                },
            },
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for I18nBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("i18n-lsp initialize called");
        tracing::debug!("Client capabilities: {:?}", params.capabilities);

        let inlay_hint_dynamic_registration_support = params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.inlay_hint.as_ref())
            .and_then(|inlay| inlay.dynamic_registration)
            .unwrap_or(false);

        *self.inlay_hint_dynamic_registration_supported.write().await =
            inlay_hint_dynamic_registration_support;

        tracing::info!(
            "Client inlay hint dynamicRegistration: {}",
            inlay_hint_dynamic_registration_support
        );

        let root_path = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|folder| folder.uri.to_file_path().ok())
            .or_else(|| {
                params
                    .root_uri
                    .as_ref()
                    .and_then(|uri| uri.to_file_path().ok())
            });

        if let Some(root) = root_path {
            self.initialize_workspace(root).await;
        } else {
            tracing::warn!("No workspace root found in initialize params");
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
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                ))),
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
        self.register_inlay_hint_capability().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let dominated_changes = params
            .changes
            .iter()
            .any(|change| change.uri.path().ends_with(".json"));

        if dominated_changes {
            tracing::info!("Translation files changed, reloading...");

            let workspace_root = self.workspace_root.read().await;
            let config = self.config.read().await;

            if let Some(root) = workspace_root.as_ref() {
                let store = TranslationStore::new(root.clone());
                store.scan_and_load(&config.locale_paths);

                let locales = store.get_locales();
                let keys = store.get_all_keys();

                self.client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "Reloaded translations: {} locales, {} keys",
                            locales.len(),
                            keys.len()
                        ),
                    )
                    .await;

                *self.translation_store.write().await = Some(store);
            }
        }
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

        if let Some(change) = params.content_changes.into_iter().next_back() {
            let content = change.text;
            let version = params.text_document.version;

            {
                let mut docs = self.documents.write().await;
                docs.update(uri.as_str(), content.clone(), version);
            }

            self.diagnose_document(&uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.close(params.text_document.uri.as_str());
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri.as_str()) else {
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
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri.as_str()) else {
            return Ok(None);
        };

        let content = doc.content.to_string();
        let line_content: String = content
            .lines()
            .nth(position.line as usize)
            .unwrap_or("")
            .to_string();

        let Some(prefix) =
            Self::extract_completion_prefix(&line_content, position.character as usize)
        else {
            return Ok(None);
        };

        let completions = self.get_completions(&prefix).await;

        if completions.is_empty() {
            return Ok(None);
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri.as_str()) else {
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

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        tracing::debug!(">>> inlay_hint: uri={}, range={:?}", uri, params.range);

        let source_locale = self.config.read().await.source_locale.clone();

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri.as_str()) else {
            tracing::warn!("<<< inlay_hint: document NOT in store: {}", uri);
            return Ok(None);
        };

        let content = doc.content.as_str();
        let key_finder = self.key_finder.read().await;
        let found_keys = key_finder.find_keys(content);

        let translation_store = self.translation_store.read().await;
        let Some(store) = translation_store.as_ref() else {
            return Ok(None);
        };

        let mut hints = Vec::new();
        let request_range = params.range;
        let request_is_empty = request_range.start == request_range.end;

        let position_leq = |a: Position, b: Position| -> bool {
            a.line < b.line || (a.line == b.line && a.character <= b.character)
        };

        let ranges_overlap = |start: Position, end: Position, range: &Range| -> bool {
            position_leq(range.start, end) && position_leq(start, range.end)
        };

        for found_key in found_keys {
            let key_start = Position {
                line: found_key.line as u32,
                character: found_key.start_char as u32,
            };
            let key_end = Position {
                line: found_key.line as u32,
                character: found_key.end_char as u32,
            };

            if !request_is_empty && !ranges_overlap(key_start, key_end, &request_range) {
                continue;
            }

            if let Some(translation) = store.get_translation(&found_key.key, &source_locale) {
                let display_text = truncate_string(&translation, 30);

                let mut hint_char = found_key.end_char;
                if let Some(line) = content.lines().nth(found_key.line) {
                    let line_bytes = line.as_bytes();
                    if matches!(line_bytes.get(hint_char), Some(b'\'') | Some(b'"')) {
                        hint_char += 1;
                    }
                }

                hints.push(InlayHint {
                    position: Position {
                        line: found_key.line as u32,
                        character: hint_char as u32,
                    },
                    label: InlayHintLabel::String(format!("= {}", display_text)),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                });
            }
        }

        tracing::debug!("<<< inlay_hint: returning {} hints", hints.len());
        Ok(Some(hints))
    }
}

impl I18nBackend {
    fn extract_completion_prefix(line: &str, character: usize) -> Option<String> {
        let before_cursor = &line[..character.min(line.len())];

        let quote_patterns = ["t(\"", "t('", "$t(\"", "$t('", "i18n.t(\"", "i18n.t('"];

        for pattern in quote_patterns {
            if let Some(pos) = before_cursor.rfind(pattern) {
                let after_quote = pos + pattern.len();
                let prefix = &before_cursor[after_quote..];

                if !prefix.contains('"') && !prefix.contains('\'') {
                    return Some(prefix.to_string());
                }
            }
        }

        None
    }
}
