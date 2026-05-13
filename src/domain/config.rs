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
    pub gitignore: Option<GitignoreConfig>,
    pub skills: Option<SkillsConfig>,
    pub subagents: Option<SubagentsConfig>,
    pub nested: bool,
    pub nested_defined: bool,
}
