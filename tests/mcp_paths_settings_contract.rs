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
fn filters_mcp_by_agent_capabilities_and_transforms_remote_to_stdio_when_needed() {
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
    assert!(!get_agent_mcp_capabilities(firebase).supports_remote);
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
                "remote": {
                    "type": "stdio",
                    "args": ["-y", "mcp-remote@latest", "https://example.test/mcp"],
                    "command": "npx",
                    "headers": { "Authorization": "Bearer token" }
                },
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
    fs::write(&target, "not json").unwrap();
    assert_eq!(mcp.read_native_mcp(&target).unwrap(), json!({}));
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
