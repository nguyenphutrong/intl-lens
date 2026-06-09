use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use intl_lens::audit::{
    AuditReport, AuditResult, FixSuggestion, MissingTranslation, PlaceholderIssue,
};
use intl_lens::config::I18nConfig;
use intl_lens::i18n::store::TranslationStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct AuditParams {
    workspace: Option<PathBuf>,
    scope: Option<String>,
    include_suggestions: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct MissingTranslationsParams {
    workspace: Option<PathBuf>,
    locales: Vec<String>,
    include_context: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct SuggestTranslationFixesParams {
    workspace: Option<PathBuf>,
    key: String,
    target_locales: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ValidatePlaceholdersParams {
    workspace: Option<PathBuf>,
    key: String,
}

#[derive(Debug, Serialize)]
struct ServerInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct ToolDefinition {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct ResourceDefinition {
    uri: String,
    name: String,
    description: String,
    mime_type: String,
}

#[derive(Debug, Serialize)]
struct ResourceContents {
    uri: String,
    mime_type: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct FixSuggestionResponse {
    key: String,
    source_locale: String,
    source_value: String,
    target_locales: Vec<String>,
    files_to_edit: Vec<PathBuf>,
    suggestion: FixSuggestion,
}

struct McpServer {
    workspace_root: PathBuf,
}

impl McpServer {
    fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params),
            "notifications/initialized" => Ok(Value::Null),
            "ping" => Ok(json!({ "pong": true })),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.handle_tool_call(request.params),
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resource_read(request.params),
            method => Err(anyhow!("Unsupported method: {method}")),
        };

        match response {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: error.to_string(),
                    data: None,
                }),
            },
        }
    }

    fn handle_initialize(&self, _params: Option<Value>) -> Result<Value> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": ServerInfo {
                name: "intl-lens-mcp",
                version: env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false }
            }
        }))
    }

    fn handle_tool_call(&self, params: Option<Value>) -> Result<Value> {
        let params = params.unwrap_or(Value::Null);
        let tool_name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Missing tool name"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let result = match tool_name {
            "audit_i18n" => {
                let arguments: AuditParams = serde_json::from_value(arguments)?;
                self.audit_i18n(arguments)?
            }
            "get_missing_translations" => {
                let arguments: MissingTranslationsParams = serde_json::from_value(arguments)?;
                self.get_missing_translations(arguments)?
            }
            "suggest_translation_fixes" => {
                let arguments: SuggestTranslationFixesParams = serde_json::from_value(arguments)?;
                self.suggest_translation_fixes(arguments)?
            }
            "validate_placeholders" => {
                let arguments: ValidatePlaceholdersParams = serde_json::from_value(arguments)?;
                self.validate_placeholders(arguments)?
            }
            name => return Err(anyhow!("Unknown tool: {name}")),
        };

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result)?
            }],
            "structuredContent": result,
            "isError": false
        }))
    }

    fn handle_resources_list(&self) -> Result<Value> {
        let resources = vec![
            ResourceDefinition {
                uri: "intl-lens://config".to_string(),
                name: "Intl Lens Config".to_string(),
                description: "Resolved i18n configuration for the current workspace".to_string(),
                mime_type: "application/json".to_string(),
            },
            ResourceDefinition {
                uri: "intl-lens://audit/latest".to_string(),
                name: "Latest Audit Report".to_string(),
                description: "Fresh audit report generated from the current workspace".to_string(),
                mime_type: "application/json".to_string(),
            },
            ResourceDefinition {
                uri: "intl-lens://translations/index".to_string(),
                name: "Translation Inventory".to_string(),
                description: "Loaded locales and translation key count".to_string(),
                mime_type: "application/json".to_string(),
            },
        ];

        Ok(json!({ "resources": resources }))
    }

    fn handle_resource_read(&self, params: Option<Value>) -> Result<Value> {
        let uri = params
            .as_ref()
            .and_then(|value| value.get("uri"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Missing resource uri"))?;

        let contents = match uri {
            "intl-lens://config" => {
                let config = I18nConfig::load_from_workspace(&self.workspace_root);
                vec![ResourceContents {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: serde_json::to_string_pretty(&config)?,
                }]
            }
            "intl-lens://audit/latest" => {
                let report = self.build_report(&self.workspace_root)?;
                vec![ResourceContents {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: serde_json::to_string_pretty(&report)?,
                }]
            }
            "intl-lens://translations/index" => {
                let (_, store) = self.load_store(&self.workspace_root);
                let payload = json!({
                    "workspace": self.workspace_root,
                    "locales": store.get_locales(),
                    "total_keys": store.get_all_keys().len()
                });
                vec![ResourceContents {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: serde_json::to_string_pretty(&payload)?,
                }]
            }
            _ => return Err(anyhow!("Unknown resource uri: {uri}")),
        };

        Ok(json!({ "contents": contents }))
    }

    fn audit_i18n(&self, params: AuditParams) -> Result<Value> {
        let workspace = self.resolve_workspace(params.workspace);
        let report = self.build_report(&workspace)?;

        let include_suggestions = params.include_suggestions;
        let scope = params.scope.unwrap_or_else(|| "workspace".to_string());

        let missing = if include_suggestions {
            serde_json::to_value(&report.missing)?
        } else {
            serde_json::to_value(strip_missing_suggestions(&report.missing))?
        };

        Ok(json!({
            "workspace": workspace,
            "scope": scope,
            "summary": report.summary,
            "missing": missing,
            "unused": report.unused,
            "placeholder_issues": report.placeholder_issues
        }))
    }

    fn get_missing_translations(&self, params: MissingTranslationsParams) -> Result<Value> {
        let workspace = self.resolve_workspace(params.workspace);
        let report = self.build_report(&workspace)?;

        let filtered: Vec<Value> = report
            .missing
            .into_iter()
            .filter_map(|item| {
                let missing_in = if params.locales.is_empty() {
                    item.missing_in.clone()
                } else {
                    item.missing_in
                        .iter()
                        .filter(|locale| params.locales.contains(*locale))
                        .cloned()
                        .collect()
                };

                if missing_in.is_empty() {
                    return None;
                }

                let mut value = json!({
                    "key": item.key,
                    "source_locale": item.source_locale,
                    "source_value": item.source_value,
                    "missing_in": missing_in,
                });

                if params.include_context {
                    value["used_in"] = serde_json::to_value(item.used_in).ok()?;
                    value["suggestion"] = serde_json::to_value(item.suggestion).ok()?;
                }

                Some(value)
            })
            .collect();

        Ok(json!({
            "workspace": workspace,
            "requested_locales": params.locales,
            "count": filtered.len(),
            "missing": filtered
        }))
    }

    fn suggest_translation_fixes(&self, params: SuggestTranslationFixesParams) -> Result<Value> {
        if params.key.trim().is_empty() {
            return Err(anyhow!("'key' is required"));
        }

        let workspace = self.resolve_workspace(params.workspace);
        let report = self.build_report(&workspace)?;
        let missing = report
            .missing
            .into_iter()
            .find(|item| item.key == params.key)
            .ok_or_else(|| anyhow!("No missing translation found for key '{}'", params.key))?;

        let target_locales = if params.target_locales.is_empty() {
            missing.missing_in.clone()
        } else {
            let filtered: Vec<String> = missing
                .missing_in
                .iter()
                .filter(|locale| params.target_locales.contains(*locale))
                .cloned()
                .collect();

            if filtered.is_empty() {
                return Err(anyhow!(
                    "Key '{}' is not missing in the requested locales",
                    params.key
                ));
            }

            filtered
        };

        let mut files_to_edit = Vec::new();
        for locale in &target_locales {
            if let Some(path) = find_locale_file(&workspace, locale) {
                files_to_edit.push(path);
            }
        }

        let suggestion = missing.suggestion.unwrap_or(FixSuggestion {
            action: "add_translation".to_string(),
            files_to_edit: files_to_edit.clone(),
            context: Some(format!("Translation for '{}'", params.key)),
        });

        let response = FixSuggestionResponse {
            key: params.key,
            source_locale: missing.source_locale,
            source_value: missing.source_value,
            target_locales,
            files_to_edit,
            suggestion,
        };

        Ok(serde_json::to_value(response)?)
    }

    fn validate_placeholders(&self, params: ValidatePlaceholdersParams) -> Result<Value> {
        if params.key.trim().is_empty() {
            return Err(anyhow!("'key' is required"));
        }

        let workspace = self.resolve_workspace(params.workspace);
        let report = self.build_report(&workspace)?;
        let issues: Vec<PlaceholderIssue> = report
            .placeholder_issues
            .into_iter()
            .filter(|issue| issue.key == params.key)
            .collect();

        let valid = issues.is_empty();

        Ok(json!({
            "workspace": workspace,
            "key": params.key,
            "valid": valid,
            "issues": issues
        }))
    }

    fn resolve_workspace(&self, override_path: Option<PathBuf>) -> PathBuf {
        override_path.unwrap_or_else(|| self.workspace_root.clone())
    }

    fn build_report(&self, workspace: &Path) -> Result<AuditReport> {
        let (config, store) = self.load_store(workspace);
        let mut audit = AuditResult::new(workspace.to_path_buf(), config, store);
        audit.scan_codebase();
        Ok(audit.generate_report())
    }

    fn load_store(&self, workspace: &Path) -> (I18nConfig, TranslationStore) {
        let config = I18nConfig::load_from_workspace(workspace);
        let store = TranslationStore::new(workspace.to_path_buf());
        store.scan_and_load_config(&config);
        (config, store)
    }
}

fn strip_missing_suggestions(items: &[MissingTranslation]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            json!({
                "key": item.key,
                "source_value": item.source_value,
                "source_locale": item.source_locale,
                "missing_in": item.missing_in,
                "used_in": item.used_in
            })
        })
        .collect()
}

fn find_locale_file(workspace: &Path, locale: &str) -> Option<PathBuf> {
    let config = I18nConfig::load_from_workspace(workspace);

    for locale_path in config.locale_paths {
        let base = workspace.join(locale_path);
        if !base.exists() {
            continue;
        }

        for extension in ["json", "yaml", "yml", "arb", "php"] {
            let candidate = base.join(format!("{}.{}", locale, extension));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "audit_i18n",
            description: "Run a full i18n audit for the workspace and return missing, unused, and placeholder issues.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workspace": { "type": "string", "description": "Workspace path. Defaults to current directory." },
                    "scope": { "type": "string", "description": "Audit scope label for clients." },
                    "include_suggestions": { "type": "boolean", "default": false }
                }
            }),
        },
        ToolDefinition {
            name: "get_missing_translations",
            description: "List missing translation keys, optionally filtered by locale and with source usage context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "locales": { "type": "array", "items": { "type": "string" }, "default": [] },
                    "include_context": { "type": "boolean", "default": false }
                }
            }),
        },
        ToolDefinition {
            name: "suggest_translation_fixes",
            description: "Return actionable file targets and source text for adding missing translations.",
            input_schema: json!({
                "type": "object",
                "required": ["key"],
                "properties": {
                    "workspace": { "type": "string" },
                    "key": { "type": "string" },
                    "target_locales": { "type": "array", "items": { "type": "string" }, "default": [] }
                }
            }),
        },
        ToolDefinition {
            name: "validate_placeholders",
            description: "Check placeholder consistency for a specific translation key across locales.",
            input_schema: json!({
                "type": "object",
                "required": ["key"],
                "properties": {
                    "workspace": { "type": "string" },
                    "key": { "type": "string" }
                }
            }),
        },
    ]
}

fn read_message(reader: &mut impl Read) -> Result<Option<JsonRpcRequest>> {
    let mut content_length = None;
    let mut header_buffer = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            if header_buffer.is_empty() {
                return Ok(None);
            }
            return Err(anyhow!("Unexpected EOF while reading headers"));
        }

        header_buffer.push(byte[0]);

        if header_buffer.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8(header_buffer).context("Headers were not valid UTF-8")?;
    for line in headers.split("\r\n") {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>()?);
            }
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("Missing Content-Length header"))?;
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    let request: JsonRpcRequest = serde_json::from_slice(&body)?;
    if request.jsonrpc.as_deref().unwrap_or("2.0") != "2.0" {
        return Err(anyhow!("Only JSON-RPC 2.0 requests are supported"));
    }

    Ok(Some(request))
}

fn write_message(writer: &mut impl Write, response: &JsonRpcResponse) -> Result<()> {
    let body = serde_json::to_vec(response)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("intl_lens=info".parse()?))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let workspace_root = std::env::current_dir().context("Failed to resolve current directory")?;
    let server = McpServer::new(workspace_root);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    while let Some(request) = read_message(&mut reader)? {
        let is_notification = request.id.is_none();
        let response = server.handle_request(request);
        if !is_notification {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_tool_definitions() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 4);
        assert_eq!(tools[0].name, "audit_i18n");
    }

    #[test]
    fn parses_content_length_message() {
        let payload = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let raw = format!("Content-Length: {}\r\n\r\n{}", payload.len(), payload);
        let mut bytes = raw.as_bytes();
        let request = read_message(&mut bytes)
            .expect("request should parse")
            .expect("request should exist");

        assert_eq!(request.method, "ping");
        assert_eq!(request.id, Some(json!(1)));
    }

    #[test]
    fn strips_missing_suggestions_when_requested() {
        let stripped = strip_missing_suggestions(&[MissingTranslation {
            key: "common.save".to_string(),
            source_value: "Save".to_string(),
            source_locale: "en".to_string(),
            missing_in: vec!["vi".to_string()],
            used_in: vec![],
            suggestion: Some(FixSuggestion {
                action: "add_translation".to_string(),
                files_to_edit: vec![PathBuf::from("locales/vi.json")],
                context: None,
            }),
        }]);

        assert_eq!(stripped.len(), 1);
        assert!(stripped[0].get("suggestion").is_none());
    }

    #[test]
    fn writes_content_length_response() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0",
            id: Some(json!(1)),
            result: Some(json!({ "pong": true })),
            error: None,
        };
        let mut output = Vec::new();
        write_message(&mut output, &response).expect("response should serialize");

        let text = String::from_utf8(output).expect("utf8 output");
        assert!(text.starts_with("Content-Length: "));
        assert!(text.contains("\r\n\r\n{\"jsonrpc\":\"2.0\""));
    }
}
