//! Capability-based MCP config filtering and merge helpers.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::{McpServerDefinition, McpStrategy, McpTransport};

/// MCP transport capabilities for an agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct McpCapabilities {
    pub supports_stdio: bool,
    pub supports_remote: bool,
}

/// Derives MCP capabilities from an agent definition.
pub fn get_agent_mcp_capabilities(agent: &AgentDefinition) -> McpCapabilities {
    McpCapabilities {
        supports_stdio: agent.capabilities.mcp_stdio,
        supports_remote: agent.capabilities.mcp_remote,
    }
}

/// Checks whether the agent supports any MCP transport.
pub fn agent_supports_mcp(agent: &AgentDefinition) -> bool {
    let capabilities = get_agent_mcp_capabilities(agent);
    capabilities.supports_stdio || capabilities.supports_remote
}

/// Filters standard `{ mcpServers }` config by agent capabilities.
pub fn filter_mcp_config_for_agent(mcp_config: &Value, agent: &AgentDefinition) -> Option<Value> {
    let capabilities = get_agent_mcp_capabilities(agent);
    if !agent_supports_mcp(agent) {
        return None;
    }

    let servers = mcp_config.get("mcpServers")?.as_object()?;
    let mut filtered = Map::new();

    for (server_name, server_config) in servers {
        let Some(config) = server_config.as_object() else {
            continue;
        };
        let has_command = config.contains_key("command");
        let has_url = config.contains_key("url");
        let is_stdio = has_command && !has_url;
        let is_remote = has_url && !has_command;

        if (is_stdio && capabilities.supports_stdio) || (is_remote && capabilities.supports_remote)
        {
            filtered.insert(server_name.clone(), server_config.clone());
        } else if is_remote && !capabilities.supports_remote && capabilities.supports_stdio {
            let Some(url) = config.get("url").and_then(Value::as_str) else {
                continue;
            };
            let mut transformed = Map::new();
            transformed.insert("type".to_string(), json!("stdio"));
            transformed.insert("command".to_string(), json!("npx"));
            transformed.insert("args".to_string(), json!(["-y", "mcp-remote@latest", url]));
            for (key, value) in config {
                if key != "url" {
                    transformed.insert(key.clone(), value.clone());
                }
            }
            filtered.insert(server_name.clone(), Value::Object(transformed));
        }
    }

    if filtered.is_empty() {
        None
    } else {
        let mut result = Map::new();
        result.insert("mcpServers".to_string(), Value::Object(filtered));
        Some(Value::Object(result))
    }
}

/// Merges native and incoming MCP server configurations according to strategy.
pub fn merge_mcp(base: &Value, incoming: &Value, strategy: McpStrategy, server_key: &str) -> Value {
    if strategy == McpStrategy::Overwrite {
        let mut result = Map::new();
        result.insert(
            server_key.to_string(),
            Value::Object(extract_servers(incoming, server_key)),
        );
        return Value::Object(result);
    }

    let mut merged = extract_servers(base, server_key);
    for (key, value) in extract_servers(incoming, server_key) {
        merged.insert(key, value);
    }

    let mut new_base = base.as_object().cloned().unwrap_or_default();
    new_base.insert(server_key.to_string(), Value::Object(merged));
    Value::Object(new_base)
}

fn extract_servers(config: &Value, server_key: &str) -> Map<String, Value> {
    config
        .get(server_key)
        .or_else(|| config.get("mcpServers"))
        .or_else(|| config.get("mcp"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Converts an ImRule MCP server definition to the JSON shape expected by agent configs.
pub fn mcp_server_definition_to_json(def: &McpServerDefinition) -> Value {
    match def.transport {
        McpTransport::Stdio => {
            let mut obj = Map::new();
            obj.insert("type".to_string(), Value::String("stdio".to_string()));
            if let Some(command) = &def.command {
                obj.insert("command".to_string(), Value::String(command.clone()));
            }
            if !def.args.is_empty() {
                obj.insert(
                    "args".to_string(),
                    Value::Array(def.args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !def.env.is_empty() {
                let env_map: Map<String, Value> = def
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                obj.insert("env".to_string(), Value::Object(env_map));
            }
            Value::Object(obj)
        }
        McpTransport::Http | McpTransport::Sse => {
            let mut obj = Map::new();
            let type_value = match def.transport {
                McpTransport::Http => "http",
                McpTransport::Sse => "sse",
                McpTransport::Stdio => unreachable!(),
            };
            obj.insert("type".to_string(), Value::String(type_value.to_string()));
            if let Some(url) = &def.url {
                obj.insert("url".to_string(), Value::String(url.clone()));
            }
            if !def.headers.is_empty() {
                let header_map: Map<String, Value> = def
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                obj.insert("headers".to_string(), Value::Object(header_map));
            }
            Value::Object(obj)
        }
    }
}

/// Builds the effective ImRule MCP configuration by combining an optional JSON config
/// (usually from `.imrule/mcp.json`) with TOML-managed `[mcp_servers]` definitions.
/// TOML-managed servers take precedence over JSON-managed servers with the same name.
pub fn build_imrule_mcp_config(
    json_config: Option<&Value>,
    toml_servers: &BTreeMap<String, McpServerDefinition>,
) -> Option<Value> {
    if json_config.is_none() && toml_servers.is_empty() {
        return None;
    }

    let mut servers = Map::new();

    if let Some(json_config) = json_config {
        if let Some(existing) = json_config.get("mcpServers").and_then(Value::as_object) {
            for (key, value) in existing {
                servers.insert(key.clone(), value.clone());
            }
        }
    }

    for (name, def) in toml_servers {
        servers.insert(name.clone(), mcp_server_definition_to_json(def));
    }

    let mut result = Map::new();
    result.insert("mcpServers".to_string(), Value::Object(servers));
    Some(Value::Object(result))
}
