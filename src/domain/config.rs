//! Configuration domain types.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// MCP merge behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpStrategy {
    #[default]
    Merge,
    Overwrite,
}

/// MCP transport types recognised by ImRule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

/// A single MCP server definition stored in imrule.toml under `[mcp_servers.<name>]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerDefinition {
    /// Transport protocol. The `type` alias is accepted for compatibility with
    /// Gajae Code's `gjc mcp add --type` flag and common MCP config conventions.
    #[serde(alias = "type")]
    pub transport: McpTransport,
    /// URL for remote transports (`http`, `sse`).
    pub url: Option<String>,
    /// Command for `stdio` transport.
    pub command: Option<String>,
    /// Arguments for `stdio` transport.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables for `stdio` transport.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Optional headers for remote transports.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

impl McpServerDefinition {
    pub fn stdio(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            transport: McpTransport::Stdio,
            url: None,
            command: Some(command.into()),
            args,
            env: BTreeMap::new(),
            headers: BTreeMap::new(),
        }
    }

    pub fn remote(transport: McpTransport, url: impl Into<String>) -> Self {
        Self {
            transport,
            url: Some(url.into()),
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            headers: BTreeMap::new(),
        }
    }
}

/// MCP configuration for global or agent-specific settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: Option<bool>,
    #[serde(default)]
    pub strategy: McpStrategy,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            strategy: McpStrategy::Merge,
        }
    }
}

/// Gitignore configuration for generated outputs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitignoreConfig {
    pub enabled: Option<bool>,
    pub local: Option<bool>,
}

/// Skills propagation configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillsConfig {
    pub enabled: Option<bool>,
}

/// Native subagent propagation configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentsConfig {
    pub enabled: Option<bool>,
    pub include_in_rules: Option<bool>,
}

/// Frontmatter fields recognised on a source subagent definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentFrontmatter {
    pub name: String,
    pub description: String,
    pub tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub readonly: Option<bool>,
    pub is_background: Option<bool>,
}

/// Information about a discovered skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    pub has_skill_md: bool,
    pub valid: bool,
    pub error: Option<String>,
}

/// Information about a discovered subagent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentInfo {
    pub name: String,
    pub path: PathBuf,
    pub frontmatter: Option<SubagentFrontmatter>,
    pub body: Option<String>,
    pub valid: bool,
    pub error: Option<String>,
}

impl SubagentInfo {
    pub fn invalid(name: String, path: PathBuf, error: String) -> Self {
        Self {
            name,
            path,
            frontmatter: None,
            body: None,
            valid: false,
            error: Some(error),
        }
    }
}

/// Configuration for a specific coding-agent integration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentConfig {
    pub enabled: Option<bool>,
    pub output_path: Option<PathBuf>,
    pub output_path_instructions: Option<PathBuf>,
    pub output_path_config: Option<PathBuf>,
    pub mcp: Option<McpConfig>,
}

/// Parsed ImRule configuration values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedConfig {
    pub default_agents: Option<Vec<String>>,
    pub agent_configs: BTreeMap<String, AgentConfig>,
    pub cli_agents: Option<Vec<String>>,
    pub mcp: Option<McpConfig>,
    pub mcp_servers: BTreeMap<String, McpServerDefinition>,
    pub gitignore: Option<GitignoreConfig>,
    pub skills: Option<SkillsConfig>,
    pub subagents: Option<SubagentsConfig>,
    pub nested: bool,
    pub nested_defined: bool,
}
