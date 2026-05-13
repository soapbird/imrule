//! TOML configuration loader implementing `ConfigPort`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::application::ports::ConfigPort;
use crate::domain::config::{
    AgentConfig, GitignoreConfig, LoadedConfig, McpConfig, McpStrategy, SkillsConfig,
    SubagentsConfig,
};
use crate::domain::constants::{xdg_config_home, LEGACY_CONFIG_FILENAME, LEGACY_DIR_NAME};
use crate::domain::error::ImruleError;

const SUBAGENT_RESERVED_KEYS: &[&str] = &["enabled", "include_in_rules"];

pub struct TomlConfigLoader;

impl TomlConfigLoader {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TomlConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigPort for TomlConfigLoader {
    fn load_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        cli_agents: Option<Vec<String>>,
    ) -> Result<LoadedConfig, ImruleError> {
        let config_file = resolve_config_file(project_root, config_path);
        let raw = match fs::read_to_string(&config_file) {
            Ok(text) if text.trim().is_empty() => Value::Table(Default::default()),
            Ok(text) => text.parse::<Value>().map_err(|e| {
                ImruleError::config(format!(
                    "could not parse config file at {}: {e}",
                    config_file.display()
                ))
            })?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Value::Table(Default::default())
            }
            Err(err) => {
                return Err(ImruleError::config(format!(
                    "could not read config file at {}: {err}",
                    config_file.display()
                )));
            }
        };

        let table = raw.as_table();
        let default_agents = table
            .and_then(|table| table.get("default_agents"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().unwrap_or_default().to_string())
                    .collect()
            });

        let agents_section = table
            .and_then(|table| table.get("agents"))
            .and_then(|value| value.as_table());

        let mut agent_configs = BTreeMap::new();
        if let Some(agents_section) = agents_section {
            for (name, section) in agents_section {
                if SUBAGENT_RESERVED_KEYS.contains(&name.as_str()) {
                    continue;
                }
                if let Some(section) = section.as_table() {
                    let cfg = AgentConfig {
                        enabled: section.get("enabled").and_then(|value| value.as_bool()),
                        output_path: section
                            .get("output_path")
                            .and_then(|value| value.as_str())
                            .map(|value| project_root.join(value)),
                        output_path_instructions: section
                            .get("output_path_instructions")
                            .and_then(|value| value.as_str())
                            .map(|value| project_root.join(value)),
                        output_path_config: section
                            .get("output_path_config")
                            .and_then(|value| value.as_str())
                            .map(|value| project_root.join(value)),
                        mcp: section
                            .get("mcp")
                            .and_then(|value| value.as_table())
                            .map(parse_mcp_config),
                    };
                    agent_configs.insert(name.clone(), cfg);
                }
            }
        }

        let mcp = Some(
            table
                .and_then(|table| table.get("mcp"))
                .and_then(|value| value.as_table())
                .map(parse_mcp_config)
                .unwrap_or_else(empty_mcp_config),
        );
        let gitignore = Some(
            table
                .and_then(|table| table.get("gitignore"))
                .and_then(|value| value.as_table())
                .map(parse_gitignore_config)
                .unwrap_or_default(),
        );
        let skills = Some(
            table
                .and_then(|table| table.get("skills"))
                .and_then(|value| value.as_table())
                .map(parse_skills_config)
                .unwrap_or_default(),
        );

        let legacy_subagents = table
            .and_then(|table| table.get("subagents"))
            .and_then(|value| value.as_table());
        let subagents = SubagentsConfig {
            enabled: agents_section
                .and_then(|agents| agents.get("enabled"))
                .and_then(|value| value.as_bool())
                .or_else(|| {
                    legacy_subagents
                        .and_then(|s| s.get("enabled"))
                        .and_then(|v| v.as_bool())
                }),
            include_in_rules: agents_section
                .and_then(|agents| agents.get("include_in_rules"))
                .and_then(|value| value.as_bool())
                .or_else(|| {
                    legacy_subagents
                        .and_then(|s| s.get("include_in_rules"))
                        .and_then(|v| v.as_bool())
                }),
        };

        let nested_defined = table
            .and_then(|table| table.get("nested"))
            .and_then(|value| value.as_bool())
            .is_some();
        let nested = table
            .and_then(|table| table.get("nested"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        Ok(LoadedConfig {
            default_agents,
            agent_configs,
            cli_agents,
            mcp,
            gitignore,
            skills,
            subagents: Some(subagents),
            nested,
            nested_defined,
        })
    }
}

fn resolve_config_file(project_root: &Path, config_path: Option<&Path>) -> PathBuf {
    if let Some(config_path) = config_path {
        if config_path.is_absolute() {
            config_path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(config_path)
        }
    } else {
        let local = project_root.join(".imrule/imrule.toml");
        if local.exists() {
            return local;
        }
        let legacy_toml = project_root.join(format!("{LEGACY_DIR_NAME}/{LEGACY_CONFIG_FILENAME}"));
        if legacy_toml.exists() {
            return legacy_toml;
        }
        let legacy_imrule = project_root.join(format!("{LEGACY_DIR_NAME}/imrule.toml"));
        if legacy_imrule.exists() {
            return legacy_imrule;
        }
        xdg_config_home().join("imrule/imrule.toml")
    }
}

fn parse_mcp_config(table: &toml::map::Map<String, Value>) -> McpConfig {
    let mut config = empty_mcp_config();
    config.enabled = table.get("enabled").and_then(|value| value.as_bool());
    if let Some(strategy) = table.get("merge_strategy").and_then(|value| value.as_str()) {
        config.strategy = parse_mcp_strategy(strategy).unwrap_or(McpStrategy::Merge);
    }
    config
}

fn empty_mcp_config() -> McpConfig {
    McpConfig {
        enabled: None,
        strategy: McpStrategy::Merge,
    }
}

fn parse_gitignore_config(table: &toml::map::Map<String, Value>) -> GitignoreConfig {
    GitignoreConfig {
        enabled: table.get("enabled").and_then(|value| value.as_bool()),
        local: table.get("local").and_then(|value| value.as_bool()),
    }
}

fn parse_skills_config(table: &toml::map::Map<String, Value>) -> SkillsConfig {
    SkillsConfig {
        enabled: table.get("enabled").and_then(|value| value.as_bool()),
    }
}

fn parse_mcp_strategy(value: &str) -> Option<McpStrategy> {
    match value {
        "merge" => Some(McpStrategy::Merge),
        "overwrite" => Some(McpStrategy::Overwrite),
        _ => None,
    }
}
