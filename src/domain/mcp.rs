//! Capability-based MCP config filtering and merge helpers.

use serde_json::{json, Map, Value};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::McpStrategy;

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
    new_base.remove("mcpServers");
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
