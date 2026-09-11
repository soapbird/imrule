use std::fs;

use imrule::application::ports::{ConfigPort, McpPort};
use imrule::domain::agent::all_agents;
use imrule::domain::config::{McpConfig, McpRemoteTransport, McpStrategy};
use imrule::domain::mcp::{
    agent_supports_mcp, build_imrule_mcp_config, expand_mcp_environment_variables,
    filter_mcp_config_for_agent, get_agent_mcp_capabilities, merge_mcp,
    validate_mcp_config_for_remote_transport, McpRemoteTransportPolicy,
};
use imrule::infrastructure::config_loader::TomlConfigLoader;
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

    assert_eq!(
        filter_mcp_config_for_agent(&config, cline, &McpRemoteTransport::Native.into()),
        None
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, firebase, &McpRemoteTransport::Native.into()),
        Some(json!({
            "mcpServers": {
                "remote": { "url": "https://example.test/mcp", "headers": { "Authorization": "Bearer token" } },
                "stdio": { "command": "node", "args": ["server.js"] }
            }
        }))
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, copilot, &McpRemoteTransport::Native.into()),
        Some(json!({
            "mcpServers": {
                "remote": { "url": "https://example.test/mcp", "headers": { "Authorization": "Bearer token" } },
                "stdio": { "command": "node", "args": ["server.js"] }
            }
        }))
    );
}

#[test]
fn servers_without_a_declared_timeout_get_the_default_connection_window() {
    let agents = all_agents();
    let gjc = agents
        .iter()
        .find(|agent| agent.identifier == "gjc")
        .unwrap();

    let config = json!({
        "mcpServers": {
            "declared": { "type": "stdio", "command": "npx", "args": ["-y", "pkg"], "timeout": 30000 },
            "stdio": { "type": "stdio", "command": "npx", "args": ["-y", "pkg"] },
            "remote": { "type": "http", "url": "https://example.test/mcp" }
        }
    });

    // GJC tears down anything still connecting 250 ms into startup unless the
    // server declared a window, so ImRule writes a default for servers that
    // declare none and preserves an explicit one.
    assert_eq!(
        filter_mcp_config_for_agent(&config, gjc, &McpRemoteTransport::Native.into()),
        Some(json!({
            "mcpServers": {
                "declared": { "type": "stdio", "command": "npx", "args": ["-y", "pkg"], "timeout": 30000 },
                "remote": { "type": "http", "url": "https://example.test/mcp", "timeout": 15000 },
                "stdio": { "type": "stdio", "command": "npx", "args": ["-y", "pkg"], "timeout": 15000 }
            }
        }))
    );

    // Every timeout-aware agent gets the same treatment; the rest never see the key.
    for agent in agents.iter().filter(|agent| agent_supports_mcp(agent)) {
        let Some(filtered) =
            filter_mcp_config_for_agent(&config, agent, &McpRemoteTransport::Native.into())
        else {
            continue;
        };
        let id = agent.identifier;
        for (name, server) in filtered["mcpServers"].as_object().unwrap() {
            let timeout = server.get("timeout");
            if agent.capabilities.mcp_timeout {
                let expected = if name == "declared" { 30000 } else { 15000 };
                assert_eq!(
                    timeout.and_then(serde_json::Value::as_u64),
                    Some(expected),
                    "{id} server {name} timeout"
                );
            } else {
                assert!(timeout.is_none(), "{id} server {name} kept a timeout");
            }
        }
    }
}

#[test]
fn mcp_remote_mode_bridges_only_url_remote_servers_for_stdio_agents() {
    assert_eq!(McpRemoteTransport::default(), McpRemoteTransport::McpRemote);
    assert_eq!(
        McpConfig::default().remote_transport,
        McpRemoteTransport::McpRemote
    );

    let agents = all_agents();
    let both = *agents
        .iter()
        .find(|agent| agent.identifier == "firebase")
        .unwrap();
    let mut stdio_only = both;
    stdio_only.capabilities.mcp_remote = false;
    let mut remote_only = both;
    remote_only.capabilities.mcp_stdio = false;
    let mut unsupported = both;
    unsupported.capabilities.mcp_stdio = false;
    unsupported.capabilities.mcp_remote = false;

    let config = json!({
        "mcpServers": {
            "http": {
                "type": "http",
                "url": "https://example.test/mcp"
            },
            "sse": {
                "type": "sse",
                "url": "https://example.test/events"
            },
            "stdio": {
                "type": "stdio",
                "command": "node",
                "args": ["server.js"]
            },
            "headers": {
                "type": "http",
                "url": "https://secret.example.test/mcp",
                "headers": { "Authorization": "Bearer secret" }
            },
            "mixed": {
                "type": "http",
                "url": "https://bad.example.test/mcp",
                "command": "node"
            },
            "unknown-remote": {
                "type": "websocket",
                "url": "wss://bad.example.test/mcp"
            }
        }
    });
    let expected = Some(json!({
        "mcpServers": {
            "http": {
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "mcp-remote@latest", "https://example.test/mcp"]
            },
            "sse": {
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "mcp-remote@latest", "https://example.test/events"]
            },
            "stdio": {
                "type": "stdio",
                "command": "node",
                "args": ["server.js"]
            }
        }
    }));

    assert_eq!(
        filter_mcp_config_for_agent(&config, &both, &McpRemoteTransport::McpRemote.into()),
        expected
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, &stdio_only, &McpRemoteTransport::McpRemote.into()),
        expected
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, &remote_only, &McpRemoteTransport::McpRemote.into()),
        None
    );
    assert_eq!(
        filter_mcp_config_for_agent(&config, &unsupported, &McpRemoteTransport::McpRemote.into()),
        None
    );
}
#[test]
fn mcp_remote_mode_rejects_static_headers() {
    let config = json!({
        "mcpServers": {
            "protected": {
                "type": "http",
                "url": "https://example.test/mcp",
                "headers": { "Authorization": "Bearer token" }
            }
        }
    });

    let error =
        validate_mcp_config_for_remote_transport(&config, &McpRemoteTransport::McpRemote.into())
            .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("'protected'"));
    assert!(message.contains("[mcp_servers.protected]"));
    assert!(message.contains("under [mcp] for every server"));
    assert!(
        validate_mcp_config_for_remote_transport(&config, &McpRemoteTransport::Native.into())
            .is_ok()
    );
}

#[test]
fn a_per_server_remote_transport_overrides_the_project_default() {
    let agents = all_agents();
    let firebase = agents
        .iter()
        .find(|agent| agent.identifier == "firebase")
        .unwrap();
    let config = json!({
        "mcpServers": {
            "bridged": { "type": "http", "url": "https://bridged.example.test/mcp" },
            "protected": {
                "type": "http",
                "url": "http://127.0.0.1:8765/mcp/",
                "headers": { "Authorization": "Bearer token" }
            }
        }
    });
    let expected = Some(json!({
        "mcpServers": {
            "bridged": {
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "mcp-remote@latest", "https://bridged.example.test/mcp"]
            },
            "protected": {
                "type": "http",
                "url": "http://127.0.0.1:8765/mcp/",
                "headers": { "Authorization": "Bearer token" }
            }
        }
    }));

    // The header server goes native while the rest of the project stays on the bridge.
    let bridge_by_default = McpRemoteTransportPolicy::from(McpRemoteTransport::McpRemote)
        .with_override("protected", McpRemoteTransport::Native);
    assert!(validate_mcp_config_for_remote_transport(&config, &bridge_by_default).is_ok());
    assert_eq!(
        filter_mcp_config_for_agent(&config, firebase, &bridge_by_default),
        expected
    );

    // A native project can likewise keep a single server on the bridge.
    let native_by_default = McpRemoteTransportPolicy::from(McpRemoteTransport::Native)
        .with_override("bridged", McpRemoteTransport::McpRemote);
    assert!(validate_mcp_config_for_remote_transport(&config, &native_by_default).is_ok());
    assert_eq!(
        filter_mcp_config_for_agent(&config, firebase, &native_by_default),
        expected
    );
}

#[test]
fn loads_per_server_remote_transport_from_both_server_sources() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        r#"
[mcp]
remote_transport = "mcp-remote"

[mcp_servers.agent-mail]
transport = "http"
url = "http://127.0.0.1:8765/mcp/"
remote_transport = "native"

[mcp_servers.linear]
url = "https://mcp.linear.app/mcp"

[mcp_servers.typo]
url = "https://typo.example.test/mcp"
remote_transport = "nativ"

[mcp_servers.shadowed]
url = "https://shadowed.example.test/mcp"
"#,
    )
    .unwrap();

    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let loaded = loader.load_config(root, None, None).unwrap();
    assert_eq!(
        loaded.mcp_servers["agent-mail"].remote_transport,
        Some(McpRemoteTransport::Native)
    );
    assert_eq!(loaded.mcp_servers["linear"].remote_transport, None);
    assert_eq!(loaded.mcp_servers["typo"].remote_transport, None);

    let json_config = json!({
        "mcpServers": {
            "from-json": { "url": "https://json.example.test/mcp", "remote_transport": "native" },
            "shadowed": { "url": "https://shadowed.example.test/mcp", "remote_transport": "native" }
        }
    });
    let policy = McpRemoteTransportPolicy::from_sources(
        loaded.mcp.as_ref(),
        Some(&json_config),
        &loaded.mcp_servers,
    );
    assert_eq!(policy.for_server("agent-mail"), McpRemoteTransport::Native);
    assert_eq!(policy.for_server("linear"), McpRemoteTransport::McpRemote);
    assert_eq!(
        policy.for_server("typo"),
        McpRemoteTransport::McpRemote,
        "an unrecognized value inherits the project default"
    );
    assert_eq!(policy.for_server("from-json"), McpRemoteTransport::Native);
    assert_eq!(
        policy.for_server("shadowed"),
        McpRemoteTransport::McpRemote,
        "the TOML definition replaces the JSON one, override included"
    );

    // The key is ImRule's alone and never reaches an agent's native config.
    let merged = build_imrule_mcp_config(Some(&json_config), &loaded.mcp_servers).unwrap();
    for (name, server) in merged["mcpServers"].as_object().unwrap() {
        assert!(
            server.get("remote_transport").is_none(),
            "{name} kept remote_transport"
        );
    }
}

#[test]
fn loads_mcp_remote_transport_mode_from_mcp_config() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        "[mcp]\nremote_transport = \"mcp-remote\"\n",
    )
    .unwrap();

    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let loaded = loader.load_config(root, None, None).unwrap();
    assert_eq!(
        loaded.mcp.unwrap().remote_transport,
        McpRemoteTransport::McpRemote
    );

    fs::write(root.join(".imrule/imrule.toml"), "[mcp]\nenabled = true\n").unwrap();
    let loaded = loader.load_config(root, None, None).unwrap();
    assert_eq!(
        loaded.mcp.unwrap().remote_transport,
        McpRemoteTransport::McpRemote
    );

    fs::write(
        root.join(".imrule/imrule.toml"),
        "[mcp]\nremote_transport = \"native\"\n",
    )
    .unwrap();
    let loaded = loader.load_config(root, None, None).unwrap();
    assert_eq!(
        loaded.mcp.unwrap().remote_transport,
        McpRemoteTransport::Native,
        "an explicit native mode still wins"
    );

    // A missing, empty, or unrecognized value falls back to the bridge rather
    // than to each agent's native remote transport.
    for contents in [
        "",
        "[mcp]\n",
        "[mcp]\nremote_transport = \"\"\n",
        "[mcp]\nremote_transport = \"nativ\"\n",
        "[mcp]\nremote_transport = 3\n",
    ] {
        fs::write(root.join(".imrule/imrule.toml"), contents).unwrap();
        let loaded = loader.load_config(root, None, None).unwrap();
        let transport = loaded
            .mcp
            .map(|mcp| mcp.remote_transport)
            .unwrap_or_default();
        assert_eq!(
            transport,
            McpRemoteTransport::McpRemote,
            "unset remote_transport should default to the bridge for {contents:?}"
        );
    }
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
fn merge_keeps_agent_written_oauth_credentials_on_managed_servers() {
    // GJC's `/mcp reauth` writes the credential back into the native file under
    // the same server name ImRule manages; `apply` must not drop it.
    let native = json!({
        "mcpServers": {
            "notion": {
                "type": "http",
                "url": "https://mcp.notion.com/mcp",
                "auth": { "type": "oauth", "credentialId": "cred-1", "tokenUrl": "https://example.test/token" },
                "oauth": { "clientId": "client-1" }
            }
        }
    });
    let incoming = json!({
        "mcpServers": {
            "notion": { "type": "http", "url": "https://mcp.notion.com/mcp", "timeout": 15000 }
        }
    });

    let merged = merge_mcp(&native, &incoming, McpStrategy::Merge, "mcpServers");
    assert_eq!(
        merged["mcpServers"]["notion"],
        json!({
            "type": "http",
            "url": "https://mcp.notion.com/mcp",
            "timeout": 15000,
            "auth": { "type": "oauth", "credentialId": "cred-1", "tokenUrl": "https://example.test/token" },
            "oauth": { "clientId": "client-1" }
        })
    );

    // An incoming definition that declares its own auth still wins.
    let explicit = json!({
        "mcpServers": {
            "notion": {
                "type": "http",
                "url": "https://mcp.notion.com/mcp",
                "auth": { "type": "apikey" }
            }
        }
    });
    let merged = merge_mcp(&native, &explicit, McpStrategy::Merge, "mcpServers");
    assert_eq!(
        merged["mcpServers"]["notion"]["auth"],
        json!({ "type": "apikey" })
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

// --- Gap coverage: expand_mcp_environment_variables edge cases ---
// The apply fixture exercises the common ${VAR} / $VAR / override / missing
// cases, but the parser has several branches that are never directly asserted:
// `$` at end of string, `$` followed by a digit (not a valid env name), an
// unclosed `${`, and recursion through nested arrays/objects.

#[test]
fn expand_environment_variables_handles_dollar_edge_cases() {
    let mut vars = std::collections::BTreeMap::new();
    vars.insert("TOKEN".to_string(), "secret".to_string());

    // `$` at end of string is preserved verbatim (no name follows).
    let mut a = json!("prefix$");
    expand_mcp_environment_variables(&mut a, &vars);
    assert_eq!(a, json!("prefix$"));

    // `$` followed by a digit is NOT a valid env name — left untouched.
    let mut b = json!("price:$5");
    expand_mcp_environment_variables(&mut b, &vars);
    assert_eq!(b, json!("price:$5"));

    // Unclosed `${` — the brace is never terminated; the literal is preserved.
    let mut c = json!("${TOKEN");
    expand_mcp_environment_variables(&mut c, &vars);
    assert_eq!(c, json!("${TOKEN"));

    // Empty `${}` is not a valid env name — left untouched.
    let mut d = json!("${}");
    expand_mcp_environment_variables(&mut d, &vars);
    assert_eq!(d, json!("${}"));

    // `${VAR}` and `$VAR` both expand; unknown vars are left as-is.
    let mut e = json!("${TOKEN} and $TOKEN and $MISSING");
    expand_mcp_environment_variables(&mut e, &vars);
    assert_eq!(e, json!("secret and secret and $MISSING"));

    // A bare `$` with no following identifier character is preserved.
    let mut f = json!("cost $$ total");
    expand_mcp_environment_variables(&mut f, &vars);
    assert_eq!(f, json!("cost $$ total"));
}

#[test]
fn expand_environment_variables_recurses_through_arrays_and_objects() {
    let mut vars = std::collections::BTreeMap::new();
    vars.insert("HOST".to_string(), "example.test".to_string());
    vars.insert("PORT".to_string(), "8080".to_string());

    let mut config = json!({
        "mcpServers": {
            "remote": {
                "url": "https://${HOST}:${PORT}/mcp",
                "headers": { "X-Trace": "$HOST" },
                "tags": ["$HOST", "literal", "$PORT"]
            }
        }
    });
    expand_mcp_environment_variables(&mut config, &vars);
    assert_eq!(
        config["mcpServers"]["remote"]["url"],
        json!("https://example.test:8080/mcp")
    );
    assert_eq!(
        config["mcpServers"]["remote"]["headers"]["X-Trace"],
        json!("example.test")
    );
    assert_eq!(
        config["mcpServers"]["remote"]["tags"],
        json!(["example.test", "literal", "8080"])
    );

    // Non-string scalars (numbers, bools, nulls) pass through untouched.
    let mut mixed = json!({ "count": 42, "flag": true, "none": null });
    expand_mcp_environment_variables(&mut mixed, &vars);
    assert_eq!(mixed["count"], json!(42));
    assert_eq!(mixed["flag"], json!(true));
    assert_eq!(mixed["none"], serde_json::Value::Null);
}
