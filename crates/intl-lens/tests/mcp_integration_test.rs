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
        dir.path().join(".intl-lens.json"),
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
    let bin = assert_cmd::cargo::cargo_bin("intl-lens-mcp");
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
            "validate_placeholders"
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
            "intl-lens://config",
            "intl-lens://audit/latest",
            "intl-lens://translations/index"
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
            "params":{"uri":"intl-lens://config"}
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
            "params":{"uri":"intl-lens://audit/latest"}
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
            "params":{"uri":"intl-lens://translations/index"}
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
