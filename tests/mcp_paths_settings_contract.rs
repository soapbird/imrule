use std::fs;

use imrule::application::ports::McpPort;
use imrule::domain::agent::all_agents;
use imrule::domain::config::McpStrategy;
use imrule::domain::mcp::{
    agent_supports_mcp, filter_mcp_config_for_agent, get_agent_mcp_capabilities, merge_mcp,
};
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use imrule::infrastructure::vscode_settings::{
    get_vscode_settings_path, merge_augment_mcp_servers, transform_imrule_to_augment_mcp,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn filters_mcp_by_agent_capabilities() {
    let agents = all_agents();
    let firebase = agents
        .iter()
        .find(|agent| agent.identifier == "firebase")
        .unwrap();
    let cline = agents
        .iter()
        .find(|agent| agent.identifier == "cline")
        .unwrap();
    let copilot = agents
        .iter()
        .find(|agent| agent.identifier == "copilot")
        .unwrap();

    assert!(get_agent_mcp_capabilities(firebase).supports_stdio);
    assert!(get_agent_mcp_capabilities(firebase).supports_remote);
    assert!(!agent_supports_mcp(cline));
    assert!(agent_supports_mcp(copilot));

    let config = json!({
        "mcpServers": {
            "stdio": { "command": "node", "args": ["server.js"] },
            "remote": { "url": "https://example.test/mcp", "headers": { "Authorization": "Bearer token" } },
            "mixed": { "command": "node", "url": "https://bad.test" }
        }
    });

    assert_eq!(filter_mcp_config_for_agent(&config, cline), None);
    assert_eq!(
        filter_mcp_config_for_agent(&config, firebase),
        Some(json!({
            "mcpServers": {
                "remote": { "url": "https://example.test/mcp", "headers": { "Authorization": "Bearer token" } },
                "stdio": { "command": "node", "args": ["server.js"] }
            }
        }))
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, copilot),
        Some(json!({
            "mcpServers": {
                "remote": { "url": "https://example.test/mcp", "headers": { "Authorization": "Bearer token" } },
                "stdio": { "command": "node", "args": ["server.js"] }
            }
        }))
    );
}

#[test]
fn merges_mcp_configs_with_key_translation_and_strategy() {
    let base = json!({
        "keep": true,
        "mcpServers": {
            "old": { "command": "old" },
            "same": { "command": "base" }
        }
    });
    let incoming = json!({
        "mcpServers": {
            "same": { "command": "incoming" },
            "new": { "url": "https://new.test" }
        }
    });

    assert_eq!(
        merge_mcp(&base, &incoming, McpStrategy::Merge, "servers"),
        json!({
            "keep": true,
            // mcpServers from base is preserved — not removed — when writing to a different key.
            "mcpServers": {
                "old": { "command": "old" },
                "same": { "command": "base" }
            },
            "servers": {
                "old": { "command": "old" },
                "same": { "command": "incoming" },
                "new": { "url": "https://new.test" }
            }
        })
    );

    assert_eq!(
        merge_mcp(
            &base,
            &json!({ "mcp": { "only": { "command": "x" } } }),
            McpStrategy::Overwrite,
            "mcpServers"
        ),
        json!({ "mcpServers": { "only": { "command": "x" } } })
    );
}

#[test]
fn native_mcp_paths_match_agent_candidates_and_io_contract() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".vs")).unwrap();
    fs::write(root.join(".vs/mcp.json"), "{\"existing\":true}").unwrap();

    let mcp = JsonMcpStorage::new();
    assert_eq!(
        mcp.get_native_mcp_path("Visual Studio", root),
        Some(root.join(".vs/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Cursor", root),
        Some(root.join(".cursor/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Gajae Code", root),
        Some(root.join(".gjc/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Kimi CLI", root),
        Some(root.join(".kimi-code/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Kimi Code", root),
        Some(root.join(".kimi-code/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Kimi", root),
        Some(root.join(".kimi-code/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("RooCode", root),
        Some(root.join(".roo/mcp.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Kilo Code", root),
        Some(root.join("kilo.jsonc"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Crush", root),
        Some(root.join(".crush.json"))
    );
    assert_eq!(
        mcp.get_native_mcp_path("Amazon Q CLI", root),
        Some(root.join(".amazonq/mcp.json"))
    );
    // Firebender has NO native MCP path: firebender.json is its instructions
    // file, so a native MCP write would clobber the generated instructions.
    assert_eq!(mcp.get_native_mcp_path("Firebender", root), None);
    assert_eq!(
        mcp.get_native_mcp_path("Factory Droid", root),
        Some(root.join(".factory/mcp.json"))
    );
    assert_eq!(mcp.get_native_mcp_path("Unknown", root), None);

    let target = root.join(".cursor/mcp.json");
    mcp.write_native_mcp(
        &target,
        &json!({ "mcpServers": { "x": { "command": "node" } } }),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "{\n  \"mcpServers\": {\n    \"x\": {\n      \"command\": \"node\"\n    }\n  }\n}\n"
    );
    assert_eq!(
        mcp.read_native_mcp(&target).unwrap(),
        json!({ "mcpServers": { "x": { "command": "node" } } })
    );
    // A non-empty file that is not valid JSON must NOT collapse to `{}` — that
    // would let apply overwrite (and clear delete) user-authored config. It is
    // an error, and the file on disk is left untouched.
    fs::write(&target, "not json").unwrap();
    assert!(
        mcp.read_native_mcp(&target).is_err(),
        "unparseable non-empty config must error, not silently become {{}}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "not json");

    // An empty / whitespace-only file is still treated as "no config yet".
    fs::write(&target, "   \n").unwrap();
    assert_eq!(mcp.read_native_mcp(&target).unwrap(), json!({}));
}

#[test]
fn factory_mcp_output_matches_droid_schema_defaults() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let target = root.join(".factory/mcp.json");

    let mcp = JsonMcpStorage::new();
    mcp.write_native_mcp(
        &target,
        &json!({
            "mcpServers": {
                "remote": { "type": "http", "url": "https://mcp.example.test/mcp" },
                "stdio": { "type": "stdio", "command": "npx", "args": ["-y", "demo"] },
                "already-disabled": {
                    "type": "http",
                    "url": "https://disabled.example.test/mcp",
                    "disabled": true
                }
            }
        }),
    )
    .unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(written["mcpServers"]["remote"]["type"], json!("http"));
    assert_eq!(
        written["mcpServers"]["remote"]["url"],
        json!("https://mcp.example.test/mcp")
    );
    assert_eq!(written["mcpServers"]["remote"]["disabled"], json!(false));
    assert_eq!(written["mcpServers"]["stdio"]["disabled"], json!(false));
    assert_eq!(
        written["mcpServers"]["already-disabled"]["disabled"],
        json!(true)
    );
}

#[test]
fn native_mcp_output_matches_enablement_and_transport_schema_defaults() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let mcp = JsonMcpStorage::new();

    let roo_target = root.join(".roo/mcp.json");
    mcp.write_native_mcp(
        &roo_target,
        &json!({ "mcpServers": { "remote": { "type": "http", "url": "https://mcp.example.test/mcp" } } }),
    )
    .unwrap();
    let roo: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&roo_target).unwrap()).unwrap();
    assert_eq!(
        roo["mcpServers"]["remote"]["type"],
        json!("streamable-http")
    );
    assert_eq!(roo["mcpServers"]["remote"]["disabled"], json!(false));

    let kiro_target = root.join(".kiro/settings/mcp.json");
    mcp.write_native_mcp(
        &kiro_target,
        &json!({ "mcpServers": { "remote": { "type": "http", "url": "https://mcp.example.test/mcp" } } }),
    )
    .unwrap();
    let kiro: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&kiro_target).unwrap()).unwrap();
    assert_eq!(kiro["mcpServers"]["remote"]["type"], json!("http"));
    assert_eq!(kiro["mcpServers"]["remote"]["disabled"], json!(false));

    let opencode_target = root.join("opencode.json");
    mcp.write_native_mcp(
        &opencode_target,
        &json!({
            "mcp": {
                "local": { "type": "stdio", "command": "npx", "args": ["-y", "demo"] },
                "remote": { "type": "http", "url": "https://mcp.example.test/mcp" }
            }
        }),
    )
    .unwrap();
    let opencode: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&opencode_target).unwrap()).unwrap();
    assert_eq!(opencode["mcp"]["local"]["type"], json!("local"));
    assert_eq!(
        opencode["mcp"]["local"]["command"],
        json!(["npx", "-y", "demo"])
    );
    assert_eq!(opencode["mcp"]["local"]["enabled"], json!(true));
    assert_eq!(opencode["mcp"]["remote"]["type"], json!("remote"));
    assert_eq!(opencode["mcp"]["remote"]["enabled"], json!(true));

    let zed_target = root.join(".zed/settings.json");
    mcp.write_native_mcp(
        &zed_target,
        &json!({ "context_servers": { "remote": { "type": "http", "url": "https://mcp.example.test/mcp" } } }),
    )
    .unwrap();
    let zed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&zed_target).unwrap()).unwrap();
    assert!(zed["context_servers"]["remote"].get("type").is_none());

    let kimi_target = root.join(".kimi-code/mcp.json");
    mcp.write_native_mcp(
        &kimi_target,
        &json!({
            "mcpServers": {
                "stdio": { "type": "stdio", "command": "npx", "args": ["-y", "demo"] },
                "remote": { "type": "http", "url": "https://mcp.example.test/mcp" },
                "legacy": { "type": "sse", "url": "https://mcp.example.test/sse" }
            }
        }),
    )
    .unwrap();
    let kimi: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&kimi_target).unwrap()).unwrap();
    assert!(kimi["mcpServers"]["stdio"].get("type").is_none());
    assert!(kimi["mcpServers"]["remote"].get("type").is_none());
    assert_eq!(kimi["mcpServers"]["legacy"]["transport"], json!("sse"));
    assert!(kimi["mcpServers"]["legacy"].get("type").is_none());
    let firebender_target = root.join("firebender.json");
    mcp.write_native_mcp(
        &firebender_target,
        &json!({ "mcpServers": { "remote": { "type": "http", "url": "https://mcp.example.test/mcp" } } }),
    )
    .unwrap();
    let firebender: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&firebender_target).unwrap()).unwrap();
    assert!(firebender["mcpServers"]["remote"].get("type").is_none());
}

#[test]
fn vscode_augment_settings_transform_and_merge_match_native_contract() {
    let mcp_json = json!({
        "mcpServers": {
            "one": { "command": "node", "args": ["one.js"], "env": { "A": "B" } },
            "two": { "command": "python" }
        }
    });
    let servers = transform_imrule_to_augment_mcp(&mcp_json);
    assert_eq!(
        serde_json::to_value(&servers).unwrap(),
        json!([
            { "name": "one", "command": "node", "args": ["one.js"], "env": { "A": "B" } },
            { "name": "two", "command": "python" }
        ])
    );

    let existing = json!({
        "editor.tabSize": 2,
        "augment.advanced": {
            "keep": true,
            "mcpServers": [
                { "name": "one", "command": "old" },
                { "name": "old", "command": "old" }
            ]
        }
    });
    assert_eq!(
        merge_augment_mcp_servers(&existing, &servers, McpStrategy::Merge),
        json!({
            "editor.tabSize": 2,
            "augment.advanced": {
                "keep": true,
                "mcpServers": [
                    { "name": "one", "command": "node", "args": ["one.js"], "env": { "A": "B" } },
                    { "name": "old", "command": "old" },
                    { "name": "two", "command": "python" }
                ]
            }
        })
    );
    assert_eq!(
        merge_augment_mcp_servers(&existing, &servers, McpStrategy::Overwrite)["augment.advanced"]
            ["mcpServers"],
        json!(servers)
    );

    assert_eq!(
        get_vscode_settings_path(std::path::Path::new("/project")),
        std::path::PathBuf::from("/project/.vscode/settings.json")
    );
}

#[test]
fn read_imrule_mcp_config_falls_back_to_ruler_dir() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".ruler")).unwrap();
    fs::write(
        root.join(".ruler/mcp.json"),
        r#"{"mcpServers":{"demo":{"command":"node","args":["demo.js"]}}}"#,
    )
    .unwrap();

    let mcp = JsonMcpStorage::new();
    let config = mcp.read_imrule_mcp_config(root).unwrap();
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(
        config["mcpServers"]["demo"]["command"],
        serde_json::Value::String("node".to_string())
    );
}

#[test]
fn read_imrule_mcp_config_prefers_imrule_over_ruler() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::create_dir_all(root.join(".ruler")).unwrap();
    fs::write(
        root.join(".imrule/mcp.json"),
        r#"{"mcpServers":{"primary":{"command":"imrule"}}}"#,
    )
    .unwrap();
    fs::write(
        root.join(".ruler/mcp.json"),
        r#"{"mcpServers":{"legacy":{"command":"ruler"}}}"#,
    )
    .unwrap();

    let mcp = JsonMcpStorage::new();
    let config = mcp.read_imrule_mcp_config(root).unwrap().unwrap();
    assert!(config["mcpServers"].get("primary").is_some());
    assert!(config["mcpServers"].get("legacy").is_none());
}

#[test]
fn gemini_and_qwen_sse_servers_drop_transport_type() {
    // The HTTP branch (httpUrl rewrite) is already covered elsewhere; this pins
    // the sibling `sse` branch, which only strips the explicit `type` field.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let mcp = JsonMcpStorage::new();

    for relative_path in [".gemini/settings.json", ".qwen/settings.json"] {
        let target = root.join(relative_path);
        mcp.write_native_mcp(
            &target,
            &json!({
                "mcpServers": {
                    "legacy": { "type": "sse", "url": "https://mcp.example.test/sse" }
                }
            }),
        )
        .unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert!(
            written["mcpServers"]["legacy"].get("type").is_none(),
            "{relative_path} should drop the sse type field"
        );
        assert_eq!(
            written["mcpServers"]["legacy"]["url"],
            json!("https://mcp.example.test/sse")
        );
    }
}

#[test]
fn opencode_local_server_renames_env_to_environment() {
    // Exercises the `env` -> `environment` rename inside the opencode/kilo local
    // branch, which no apply fixture currently triggers.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let mcp = JsonMcpStorage::new();

    let target = root.join("opencode.json");
    mcp.write_native_mcp(
        &target,
        &json!({
            "mcp": {
                "local": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "demo"],
                    "env": { "TOKEN": "secret" }
                }
            }
        }),
    )
    .unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(
        written["mcp"]["local"]["environment"],
        json!({ "TOKEN": "secret" })
    );
    assert!(written["mcp"]["local"].get("env").is_none());
    assert_eq!(written["mcp"]["local"]["type"], json!("local"));
    assert_eq!(
        written["mcp"]["local"]["command"],
        json!(["npx", "-y", "demo"])
    );
}
