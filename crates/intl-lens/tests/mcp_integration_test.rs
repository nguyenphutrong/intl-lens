use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

fn write_workspace() -> TempDir {
    let dir = TempDir::new().expect("temp workspace");
    fs::create_dir_all(dir.path().join("locales")).expect("locales dir");
    fs::create_dir_all(dir.path().join("src")).expect("src dir");

    fs::write(
        dir.path().join(".i18nlens.json"),
        r#"{"localePaths":["locales"],"sourceLocale":"en"}"#,
    )
    .expect("config");
    fs::write(
        dir.path().join("locales/en.json"),
        r#"{"checkout":{"submit":"Submit order"},"greeting":"Hello {name}","legacy":"Legacy"}"#,
    )
    .expect("en locale");
    fs::write(
        dir.path().join("locales/vi.json"),
        r#"{"greeting":"Xin chao"}"#,
    )
    .expect("vi locale");
    fs::write(
        dir.path().join("src/App.tsx"),
        r#"export const App = () => <>{t("checkout.submit")}{t("greeting")}</>;"#,
    )
    .expect("source file");

    dir
}

fn call_mcp(workspace: &Path, request: Value) -> Value {
    let body = serde_json::to_string(&request).expect("request json");
    let raw = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let bin = assert_cmd::cargo::cargo_bin("i18nlens-mcp");
    let mut child = Command::new(bin)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(raw.as_bytes())
        .expect("write request");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("mcp output");
    assert!(
        output.status.success(),
        "mcp failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    parse_message(&output.stdout)
}

fn parse_message(output: &[u8]) -> Value {
    let text = String::from_utf8(output.to_vec()).expect("utf8 response");
    let (_, body) = text.split_once("\r\n\r\n").expect("response body");
    serde_json::from_str(body).expect("response json")
}

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    })
}

#[test]
fn lists_all_mcp_tools_and_resources() {
    let workspace = write_workspace();

    let tools = call_mcp(
        workspace.path(),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    );
    let tool_names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(
        tool_names,
        vec![
            "audit_i18n",
            "get_missing_translations",
            "suggest_translation_fixes",
            "translate_missing_keys",
            "apply_translation_patch",
            "validate_placeholders",
            "get_translation_context",
            "review_i18n_pr",
            "extract_hardcoded_strings"
        ]
    );

    let resources = call_mcp(
        workspace.path(),
        json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
    );
    let resource_uris: Vec<&str> = resources["result"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .map(|resource| resource["uri"].as_str().expect("resource uri"))
        .collect();
    assert_eq!(
        resource_uris,
        vec![
            "i18nlens://config",
            "i18nlens://audit/latest",
            "i18nlens://translations/index"
        ]
    );
}

#[test]
fn mcp_tools_return_structured_i18n_data() {
    let workspace = write_workspace();

    let audit = call_mcp(
        workspace.path(),
        tool_call(
            "audit_i18n",
            json!({"scope":"workspace","include_suggestions":true}),
        ),
    );
    let audit_content = &audit["result"]["structuredContent"];
    assert_eq!(audit_content["summary"]["missing_translations"], 2);
    assert_eq!(audit_content["summary"]["placeholder_mismatches"], 1);
    assert_eq!(audit_content["scope"], "workspace");

    let missing = call_mcp(
        workspace.path(),
        tool_call(
            "get_missing_translations",
            json!({"locales":["vi"],"include_context":true}),
        ),
    );
    let missing_content = &missing["result"]["structuredContent"];
    assert_eq!(missing_content["requested_locales"], json!(["vi"]));
    assert!(missing_content["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .any(|item| item["key"] == "checkout.submit" && item.get("used_in").is_some()));

    let fixes = call_mcp(
        workspace.path(),
        tool_call(
            "suggest_translation_fixes",
            json!({"key":"checkout.submit","target_locales":["vi"]}),
        ),
    );
    let fixes_content = &fixes["result"]["structuredContent"];
    assert_eq!(fixes_content["key"], "checkout.submit");
    assert_eq!(fixes_content["target_locales"], json!(["vi"]));
    assert_eq!(fixes_content["suggestion"]["action"], "add_translation");

    let placeholders = call_mcp(
        workspace.path(),
        tool_call("validate_placeholders", json!({"key":"greeting"})),
    );
    let placeholder_content = &placeholders["result"]["structuredContent"];
    assert_eq!(placeholder_content["key"], "greeting");
    assert_eq!(placeholder_content["valid"], false);
    assert!(placeholder_content["issues"]
        .as_array()
        .expect("issues")
        .iter()
        .any(|issue| issue["locale_values"].get("vi").is_some()));

    let context = call_mcp(
        workspace.path(),
        tool_call(
            "get_translation_context",
            json!({"key":"checkout.submit","include_usage":true}),
        ),
    );
    let context_content = &context["result"]["structuredContent"];
    assert_eq!(context_content["key"], "checkout.submit");
    assert_eq!(context_content["source_locale"], "en");
    assert_eq!(context_content["source_value"], "Submit order");
    assert_eq!(context_content["missing_in"], json!(["vi"]));
    assert!(context_content["used_in"]
        .as_array()
        .expect("usage")
        .iter()
        .any(|usage| usage["file"]
            .as_str()
            .expect("usage file")
            .ends_with("src/App.tsx")));
    assert!(context_content["files_to_edit"]
        .as_array()
        .expect("files")
        .iter()
        .any(|file| file.as_str().expect("file").ends_with("locales/vi.json")));

    let review = call_mcp(
        workspace.path(),
        tool_call(
            "review_i18n_pr",
            json!({"fail_on":["missing","placeholder"]}),
        ),
    );
    let review_content = &review["result"]["structuredContent"];
    assert_eq!(review_content["blocking"], true);
    assert_eq!(review_content["summary"]["missing_translations"], 2);
    assert!(review_content["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .any(|finding| finding["kind"] == "missing" && finding["key"] == "checkout.submit"));
    assert!(review_content["markdown"]
        .as_str()
        .expect("markdown")
        .contains("I18n Lens Review"));
}

#[test]
fn mcp_translate_missing_keys_returns_dry_run_patch() {
    let workspace = write_workspace();

    let patch = call_mcp(
        workspace.path(),
        tool_call(
            "translate_missing_keys",
            json!({
                "translations": [{
                    "key": "checkout.submit",
                    "locale": "vi",
                    "value": "Gui don hang"
                }]
            }),
        ),
    );

    let content = &patch["result"]["structuredContent"];
    assert_eq!(content["dry_run"], true);
    assert_eq!(content["patches"][0]["key"], "checkout.submit");
    assert_eq!(content["patches"][0]["locale"], "vi");
    assert!(content["patches"][0]["unified_diff"]
        .as_str()
        .expect("diff")
        .contains("+    \"submit\": \"Gui don hang\""));

    let vi_json = fs::read_to_string(workspace.path().join("locales/vi.json")).expect("vi json");
    assert!(!vi_json.contains("Gui don hang"));
}

#[test]
fn mcp_translate_missing_keys_rejects_placeholder_mismatch() {
    let workspace = write_workspace();
    fs::write(
        workspace.path().join("locales/en.json"),
        r#"{"checkout":{"submit":"Submit order"},"greeting":"Hello {name}","legacy":"Legacy","welcome":"Welcome {name}"}"#,
    )
    .expect("add placeholder missing key");

    let patch = call_mcp(
        workspace.path(),
        tool_call(
            "translate_missing_keys",
            json!({
                "translations": [{
                    "key": "welcome",
                    "locale": "vi",
                    "value": "Xin chao"
                }]
            }),
        ),
    );

    let content = &patch["result"]["structuredContent"];
    assert!(content["patches"].as_array().expect("patches").is_empty());
    assert_eq!(content["skipped"][0]["reason"], "placeholder mismatch");
    assert_eq!(
        content["skipped"][0]["expected_placeholders"],
        json!(["name"])
    );
}

#[test]
fn mcp_apply_translation_patch_defaults_to_dry_run() {
    let workspace = write_workspace();

    let patch = call_mcp(
        workspace.path(),
        tool_call(
            "apply_translation_patch",
            json!({
                "translations": [{
                    "key": "checkout.submit",
                    "locale": "vi",
                    "value": "Gui don hang"
                }]
            }),
        ),
    );

    let content = &patch["result"]["structuredContent"];
    assert_eq!(content["dry_run"], true);
    assert_eq!(content["applied"], 0);
    assert_eq!(content["patches"][0]["key"], "checkout.submit");

    let vi_json = fs::read_to_string(workspace.path().join("locales/vi.json")).expect("vi json");
    assert!(!vi_json.contains("Gui don hang"));
}

#[test]
fn mcp_apply_translation_patch_can_write_missing_translation() {
    let workspace = write_workspace();

    let patch = call_mcp(
        workspace.path(),
        tool_call(
            "apply_translation_patch",
            json!({
                "dry_run": false,
                "translations": [{
                    "key": "checkout.submit",
                    "locale": "vi",
                    "value": "Gui don hang"
                }]
            }),
        ),
    );

    let content = &patch["result"]["structuredContent"];
    assert_eq!(content["dry_run"], false);
    assert_eq!(content["applied"], 1);

    let vi_json = fs::read_to_string(workspace.path().join("locales/vi.json")).expect("vi json");
    assert!(vi_json.contains("Gui don hang"));

    let audit = call_mcp(
        workspace.path(),
        tool_call("audit_i18n", json!({"include_suggestions": true})),
    );
    assert_eq!(
        audit["result"]["structuredContent"]["summary"]["missing_translations"],
        1
    );
}

#[test]
fn mcp_extract_hardcoded_strings_returns_candidates() {
    let workspace = write_workspace();
    fs::write(
        workspace.path().join("src/Hardcoded.tsx"),
        r#"export const Hardcoded = () => <button>Submit order</button>;
export const label = "Checkout title";
export const translated = t("checkout.submit");
"#,
    )
    .expect("hardcoded source");

    let extraction = call_mcp(
        workspace.path(),
        tool_call(
            "extract_hardcoded_strings",
            json!({"paths":["src/Hardcoded.tsx"]}),
        ),
    );

    let content = &extraction["result"]["structuredContent"];
    assert_eq!(content["count"], 2);
    assert!(content["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .any(|candidate| candidate["text"] == "Submit order" && candidate["kind"] == "jsx_text"));
    assert!(content["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .any(|candidate| candidate["text"] == "Checkout title"
            && candidate["kind"] == "string_literal"));
}

#[test]
fn mcp_resources_read_current_workspace_state() {
    let workspace = write_workspace();

    let config = call_mcp(
        workspace.path(),
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"resources/read",
            "params":{"uri":"i18nlens://config"}
        }),
    );
    let config_text = config["result"]["contents"][0]["text"]
        .as_str()
        .expect("config text");
    let config_json: Value = serde_json::from_str(config_text).expect("config json");
    assert_eq!(config_json["sourceLocale"], "en");
    assert_eq!(config_json["localePaths"], json!(["locales"]));

    let latest = call_mcp(
        workspace.path(),
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"resources/read",
            "params":{"uri":"i18nlens://audit/latest"}
        }),
    );
    let latest_text = latest["result"]["contents"][0]["text"]
        .as_str()
        .expect("audit text");
    let latest_json: Value = serde_json::from_str(latest_text).expect("audit json");
    assert_eq!(latest_json["summary"]["missing_translations"], 2);

    let inventory = call_mcp(
        workspace.path(),
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"resources/read",
            "params":{"uri":"i18nlens://translations/index"}
        }),
    );
    let inventory_text = inventory["result"]["contents"][0]["text"]
        .as_str()
        .expect("inventory text");
    let inventory_json: Value = serde_json::from_str(inventory_text).expect("inventory json");
    let mut locales: Vec<&str> = inventory_json["locales"]
        .as_array()
        .expect("locales")
        .iter()
        .map(|locale| locale.as_str().expect("locale"))
        .collect();
    locales.sort();
    assert_eq!(locales, vec!["en", "vi"]);
    assert_eq!(inventory_json["total_keys"], 3);
}
