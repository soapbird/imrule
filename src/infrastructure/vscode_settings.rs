//! VS Code settings transforms for Augment MCP configuration.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::config::McpStrategy;

/// Augment MCP server configuration format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AugmentMcpServer {
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::BTreeMap<String, String>>,
}

/// Transforms standard Imrule MCP JSON into Augment's array format.
pub fn transform_ruler_to_augment_mcp(ruler_mcp_json: &Value) -> Vec<AugmentMcpServer> {
    let mut servers = Vec::new();
    let Some(mcp_servers) = ruler_mcp_json.get("mcpServers").and_then(Value::as_object) else {
        return servers;
    };

    for (name, server_config) in mcp_servers {
        let Some(config) = server_config.as_object() else {
            continue;
        };
        let Some(command) = config.get("command").and_then(Value::as_str) else {
            continue;
        };
        let args = config.get("args").and_then(Value::as_array).map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        });
        let env = config.get("env").and_then(Value::as_object).map(|entries| {
            entries
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|v| (key.clone(), v.to_string())))
                .collect()
        });
        servers.push(AugmentMcpServer {
            name: name.clone(),
            command: command.to_string(),
            args,
            env,
        });
    }

    servers
}

/// Merges Augment MCP servers into a VS Code settings JSON object.
pub fn merge_augment_mcp_servers(
    existing_settings: &Value,
    new_servers: &[AugmentMcpServer],
    strategy: McpStrategy,
) -> Value {
    let mut result = existing_settings.as_object().cloned().unwrap_or_default();
    let mut advanced = result
        .get("augment.advanced")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if strategy == McpStrategy::Overwrite {
        advanced.insert("mcpServers".to_string(), json!(new_servers));
    } else {
        let mut merged = advanced
            .get("mcpServers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for new_server in new_servers {
            let new_value = json!(new_server);
            if let Some(index) = merged.iter().position(|server| {
                server.get("name").and_then(Value::as_str) == Some(new_server.name.as_str())
            }) {
                merged[index] = new_value;
            } else {
                merged.push(new_value);
            }
        }
        advanced.insert("mcpServers".to_string(), Value::Array(merged));
    }

    result.insert("augment.advanced".to_string(), Value::Object(advanced));
    Value::Object(result)
}

/// Returns the project-local VS Code settings path.
pub fn get_vscode_settings_path(project_root: &Path) -> PathBuf {
    project_root.join(".vscode/settings.json")
}
