//! Capability-based MCP config filtering and merge helpers.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::{McpRemoteTransport, McpServerDefinition, McpStrategy, McpTransport};
use crate::domain::constants::{MCP_REMOTE_LATEST_PACKAGE_SPEC, MCP_REMOTE_PACKAGE};
use crate::domain::error::ImruleError;

/// Project-scoped resolution of the `mcp-remote` npm package.
///
/// The closed schema intentionally cannot represent server URLs, headers, tokens,
/// or any other authentication material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpRemoteVersionCache {
    package: String,
    resolved_version: String,
    resolved_at: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct McpRemoteVersionCacheData {
    package: String,
    resolved_version: String,
    resolved_at: u64,
}

impl TryFrom<McpRemoteVersionCacheData> for McpRemoteVersionCache {
    type Error = ImruleError;

    fn try_from(value: McpRemoteVersionCacheData) -> Result<Self, Self::Error> {
        let cache = Self {
            package: value.package,
            resolved_version: value.resolved_version,
            resolved_at: value.resolved_at,
        };
        cache.validate()?;
        Ok(cache)
    }
}

impl<'de> Deserialize<'de> for McpRemoteVersionCache {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = McpRemoteVersionCacheData::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

impl McpRemoteVersionCache {
    /// Creates a cache value for a concrete `mcp-remote` version resolved at the
    /// given Unix timestamp.
    pub fn new(resolved_version: impl Into<String>, resolved_at: u64) -> Result<Self, ImruleError> {
        let cache = Self {
            package: MCP_REMOTE_PACKAGE.to_string(),
            resolved_version: resolved_version.into(),
            resolved_at,
        };
        cache.validate()?;
        Ok(cache)
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    pub fn resolved_version(&self) -> &str {
        &self.resolved_version
    }

    pub fn resolved_at(&self) -> u64 {
        self.resolved_at
    }

    /// Returns the concrete npm package spec shared by apply and auth.
    pub fn package_spec(&self) -> String {
        format!("{}@{}", self.package, self.resolved_version)
    }

    /// Validates values deserialized from the on-disk cache.
    pub fn validate(&self) -> Result<(), ImruleError> {
        if self.package != MCP_REMOTE_PACKAGE {
            return Err(ImruleError::mcp(format!(
                "version cache package must be '{MCP_REMOTE_PACKAGE}'"
            )));
        }
        if !is_concrete_npm_version(&self.resolved_version) {
            return Err(ImruleError::mcp(
                "version cache must contain a concrete mcp-remote version",
            ));
        }
        if self.resolved_at == 0 {
            return Err(ImruleError::mcp(
                "version cache resolved_at timestamp must be greater than zero",
            ));
        }
        Ok(())
    }
}

fn is_concrete_npm_version(version: &str) -> bool {
    let (without_build, build) = version
        .split_once('+')
        .map_or((version, None), |(base, build)| (base, Some(build)));
    if build.is_some_and(|build| !is_valid_semver_identifiers(build, false)) {
        return false;
    }

    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(core, prerelease)| {
            (core, Some(prerelease))
        });
    if prerelease.is_some_and(|prerelease| !is_valid_semver_identifiers(prerelease, true)) {
        return false;
    }

    let mut components = core.split('.');
    (0..3).all(|_| components.next().is_some_and(is_valid_semver_number))
        && components.next().is_none()
}

fn is_valid_semver_number(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.chars().all(|character| character.is_ascii_digit())
}

fn is_valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
                && (!reject_numeric_leading_zero
                    || !identifier
                        .chars()
                        .all(|character| character.is_ascii_digit())
                    || is_valid_semver_number(identifier))
        })
}

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
/// Rejects static-header servers when the OAuth bridge mode cannot preserve them.
pub fn validate_mcp_config_for_remote_transport(
    mcp_config: &Value,
    remote_transport: McpRemoteTransport,
) -> Result<(), ImruleError> {
    if remote_transport != McpRemoteTransport::McpRemote {
        return Ok(());
    }

    let Some(servers) = mcp_config.get("mcpServers").and_then(Value::as_object) else {
        return Ok(());
    };

    for (server_name, server_config) in servers {
        let Some(config) = server_config.as_object() else {
            continue;
        };
        if config.contains_key("url")
            && !config.contains_key("command")
            && config.contains_key("headers")
        {
            return Err(ImruleError::mcp(format!(
                "MCP server '{server_name}' uses static headers, which mcp-remote mode does not support; set [mcp] remote_transport = \"native\" for this server"
            )));
        }
    }

    Ok(())
}

/// Filters standard `{ mcpServers }` config by agent capabilities and remote transport mode.
pub fn filter_mcp_config_for_agent(
    mcp_config: &Value,
    agent: &AgentDefinition,
    remote_transport: McpRemoteTransport,
) -> Option<Value> {
    filter_mcp_config_for_agent_with_package_spec(
        mcp_config,
        agent,
        remote_transport,
        MCP_REMOTE_LATEST_PACKAGE_SPEC,
    )
}

/// Filters MCP config using the concrete bridge version from the project cache.
pub fn filter_mcp_config_for_agent_with_version_cache(
    mcp_config: &Value,
    agent: &AgentDefinition,
    remote_transport: McpRemoteTransport,
    cache: &McpRemoteVersionCache,
) -> Option<Value> {
    let package_spec = cache.package_spec();
    filter_mcp_config_for_agent_with_package_spec(
        mcp_config,
        agent,
        remote_transport,
        &package_spec,
    )
}

fn filter_mcp_config_for_agent_with_package_spec(
    mcp_config: &Value,
    agent: &AgentDefinition,
    remote_transport: McpRemoteTransport,
    mcp_remote_package_spec: &str,
) -> Option<Value> {
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

        if is_stdio && capabilities.supports_stdio {
            filtered.insert(server_name.clone(), server_config.clone());
            continue;
        }
        if !is_remote {
            continue;
        }

        match remote_transport {
            McpRemoteTransport::Native if capabilities.supports_remote => {
                filtered.insert(server_name.clone(), server_config.clone());
            }
            McpRemoteTransport::Native if capabilities.supports_stdio => {
                if let Some(transformed) =
                    transform_remote_to_stdio(config, true, mcp_remote_package_spec)
                {
                    filtered.insert(server_name.clone(), transformed);
                }
            }
            McpRemoteTransport::McpRemote
                if capabilities.supports_stdio
                    && is_http_or_sse(config)
                    && !config.contains_key("headers") =>
            {
                if let Some(transformed) =
                    transform_remote_to_stdio(config, false, mcp_remote_package_spec)
                {
                    filtered.insert(server_name.clone(), transformed);
                }
            }
            McpRemoteTransport::Native | McpRemoteTransport::McpRemote => {}
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

fn is_http_or_sse(config: &Map<String, Value>) -> bool {
    match config.get("type") {
        None => true,
        Some(Value::String(transport)) => matches!(transport.as_str(), "http" | "sse"),
        Some(_) => false,
    }
}

fn transform_remote_to_stdio(
    config: &Map<String, Value>,
    preserve_metadata: bool,
    mcp_remote_package_spec: &str,
) -> Option<Value> {
    let url = config.get("url").and_then(Value::as_str)?;
    let mut transformed = Map::new();
    transformed.insert("type".to_string(), json!("stdio"));
    transformed.insert("command".to_string(), json!("npx"));
    transformed.insert(
        "args".to_string(),
        json!(["-y", mcp_remote_package_spec, url]),
    );

    if preserve_metadata {
        for (key, value) in config {
            if !matches!(key.as_str(), "type" | "url" | "command" | "args") {
                transformed.insert(key.clone(), value.clone());
            }
        }
    }

    Some(Value::Object(transformed))
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
/// Replaces `$NAME` and `${NAME}` references in every MCP configuration string.
pub fn expand_mcp_environment_variables(config: &mut Value, variables: &BTreeMap<String, String>) {
    match config {
        Value::String(value) => *value = expand_environment_references(value, variables),
        Value::Array(items) => {
            for item in items {
                expand_mcp_environment_variables(item, variables);
            }
        }
        Value::Object(entries) => {
            for value in entries.values_mut() {
                expand_mcp_environment_variables(value, variables);
            }
        }
        _ => {}
    }
}

fn expand_environment_references(value: &str, variables: &BTreeMap<String, String>) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut remaining = value;

    while let Some(dollar_index) = remaining.find('$') {
        expanded.push_str(&remaining[..dollar_index]);
        let after_dollar = &remaining[dollar_index + 1..];

        let (name, consumed) = if let Some(braced) = after_dollar.strip_prefix('{') {
            let Some(end_index) = braced.find('}') else {
                expanded.push('$');
                remaining = after_dollar;
                continue;
            };
            (&braced[..end_index], end_index + 3)
        } else {
            let name_length = after_dollar
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .count();
            if name_length == 0 {
                expanded.push('$');
                remaining = after_dollar;
                continue;
            }
            (&after_dollar[..name_length], name_length + 1)
        };

        if is_environment_variable_name(name) {
            if let Some(replacement) = variables.get(name) {
                expanded.push_str(replacement);
            } else {
                expanded.push_str(&remaining[dollar_index..dollar_index + consumed]);
            }
            remaining = &remaining[dollar_index + consumed..];
        } else {
            expanded.push('$');
            remaining = after_dollar;
        }
    }

    expanded.push_str(remaining);
    expanded
}

fn is_environment_variable_name(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_alphabetic() || character == '_')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}
