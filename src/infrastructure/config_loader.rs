//! TOML configuration loader implementing `ConfigPort`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::application::ports::{ConfigPort, ConfigWritePort};
use crate::domain::config::{
    AgentConfig, GitignoreConfig, LoadedConfig, McpConfig, McpServerDefinition, McpStrategy,
    McpTransport, SkillsConfig, SubagentsConfig,
};
use crate::domain::constants::{xdg_config_home, LEGACY_CONFIG_FILENAME, LEGACY_DIR_NAME};
use crate::domain::error::ImruleError;

const SUBAGENT_RESERVED_KEYS: &[&str] = &["enabled", "include_in_rules"];

#[derive(Default)]
pub struct TomlConfigLoader {
    xdg_home_override: Option<PathBuf>,
}

impl TomlConfigLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the XDG config home directory used for the global config
    /// fallback. Useful in tests that need to isolate themselves from the
    /// caller's `~/.config/imrule/imrule.toml`.
    pub fn with_xdg_home(mut self, xdg_home: PathBuf) -> Self {
        self.xdg_home_override = Some(xdg_home);
        self
    }

    fn resolve_xdg_home(&self) -> PathBuf {
        self.xdg_home_override
            .clone()
            .unwrap_or_else(xdg_config_home)
    }
}

impl ConfigPort for TomlConfigLoader {
    fn load_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        cli_agents: Option<Vec<String>>,
    ) -> Result<LoadedConfig, ImruleError> {
        let config_file = resolve_config_file(project_root, config_path, &self.resolve_xdg_home());
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

        let mcp_servers = table
            .and_then(|table| table.get("mcp_servers"))
            .and_then(|value| value.as_table())
            .map(parse_mcp_servers)
            .unwrap_or_default();
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
            mcp_servers,
            gitignore,
            skills,
            subagents: Some(subagents),
            nested,
            nested_defined,
        })
    }
}

fn resolve_config_file(
    project_root: &Path,
    config_path: Option<&Path>,
    xdg_home: &Path,
) -> PathBuf {
    if let Some(config_path) = config_path {
        if config_path.is_absolute() {
            config_path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(config_path)
        }
    } else {
        let mut current = project_root.to_path_buf();
        loop {
            let local = current.join(".imrule/imrule.toml");
            if local.exists() {
                return local;
            }
            let legacy_toml = current.join(format!("{LEGACY_DIR_NAME}/{LEGACY_CONFIG_FILENAME}"));
            if legacy_toml.exists() {
                return legacy_toml;
            }
            let legacy_imrule = current.join(format!("{LEGACY_DIR_NAME}/imrule.toml"));
            if legacy_imrule.exists() {
                return legacy_imrule;
            }
            if !current.pop() {
                break;
            }
        }
        xdg_home.join("imrule/imrule.toml")
    }
}

fn resolve_write_config_file(
    project_root: &Path,
    config_path: Option<&Path>,
    xdg_home: &Path,
) -> PathBuf {
    if let Some(config_path) = config_path {
        if config_path.is_absolute() {
            return config_path.to_path_buf();
        }
        return env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(config_path);
    }

    // If a config file already exists (local, legacy, or global), update it.
    let existing = resolve_config_file(project_root, None, xdg_home);
    if existing.exists() {
        return existing;
    }

    // Otherwise create a new project-local config.
    project_root.join(".imrule/imrule.toml")
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

fn parse_mcp_servers(
    table: &toml::map::Map<String, Value>,
) -> BTreeMap<String, McpServerDefinition> {
    let mut servers = BTreeMap::new();
    for (name, value) in table {
        let Some(server_table) = value.as_table() else {
            continue;
        };
        let transport = server_table
            .get("transport")
            .or_else(|| server_table.get("type"))
            .and_then(Value::as_str)
            .and_then(parse_mcp_transport)
            .unwrap_or(McpTransport::Stdio);

        let mut def = McpServerDefinition {
            transport,
            url: server_table
                .get("url")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            command: server_table
                .get("command")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            args: server_table
                .get("args")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            env: parse_string_map(server_table.get("env")),
            headers: parse_string_map(server_table.get("headers")),
        };

        // If the TOML table uses `command = ["npx", "-y", ...]` instead of separate args,
        // treat the first element as the command and the rest as args.
        if def.command.is_none() {
            if let Some(array) = server_table.get("command").and_then(Value::as_array) {
                let parts: Vec<String> = array
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect();
                if let Some((first, rest)) = parts.split_first() {
                    def.command = Some(first.clone());
                    def.args = rest.to_vec();
                }
            }
        }

        servers.insert(name.clone(), def);
    }
    servers
}

fn parse_string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_table)
        .map(|table| {
            table
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_mcp_transport(value: &str) -> Option<McpTransport> {
    match value {
        "stdio" => Some(McpTransport::Stdio),
        "http" => Some(McpTransport::Http),
        "sse" => Some(McpTransport::Sse),
        _ => None,
    }
}

impl ConfigWritePort for TomlConfigLoader {
    fn save_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        config: &LoadedConfig,
    ) -> Result<(), ImruleError> {
        let config_file =
            resolve_write_config_file(project_root, config_path, &self.resolve_xdg_home());

        let existing_text = fs::read_to_string(&config_file).unwrap_or_default();
        let mut document = existing_text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| {
                ImruleError::config(format!(
                    "could not parse config file at {}: {e}",
                    config_file.display()
                ))
            })?;

        // Synchronise the [mcp_servers] table without touching other sections.
        sync_mcp_servers_table(&mut document, &config.mcp_servers);

        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ImruleError::config(format!(
                    "could not create config directory at {}: {e}",
                    parent.display()
                ))
            })?;
        }

        fs::write(&config_file, document.to_string()).map_err(|e| {
            ImruleError::config(format!(
                "could not write config file at {}: {e}",
                config_file.display()
            ))
        })
    }
}

fn sync_mcp_servers_table(
    document: &mut toml_edit::DocumentMut,
    servers: &BTreeMap<String, McpServerDefinition>,
) {
    let root = document.as_table_mut();

    // Ensure the parent [mcp_servers] table exists without replacing it, so
    // comments/decorations attached to the header are preserved.
    if !root.contains_key("mcp_servers") {
        root.insert(
            "mcp_servers",
            toml_edit::Item::Table(toml_edit::Table::new()),
        );
    }

    let servers_table = root
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_mut)
        .expect("mcp_servers table was just ensured");

    // Remove child tables for servers no longer present.
    let keys_to_remove: Vec<String> = servers_table
        .iter()
        .filter_map(|(key, _)| {
            if !servers.contains_key(key) {
                Some(key.to_string())
            } else {
                None
            }
        })
        .collect();
    for key in keys_to_remove {
        servers_table.remove(&key);
    }

    // Upsert each server definition.
    for (name, def) in servers {
        let server_table = servers_table
            .entry(name)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .expect("mcp_servers children are tables");

        server_table.clear();
        server_table.insert(
            "transport",
            toml_edit::Item::Value(transport_to_toml_value(&def.transport)),
        );

        match def.transport {
            McpTransport::Stdio => {
                if let Some(command) = &def.command {
                    server_table.insert(
                        "command",
                        toml_edit::Item::Value(toml_edit::Value::from(command.clone())),
                    );
                }
                if !def.args.is_empty() {
                    server_table.insert(
                        "args",
                        toml_edit::Item::Value(toml_edit::Value::Array(
                            def.args
                                .iter()
                                .cloned()
                                .map(toml_edit::Value::from)
                                .collect(),
                        )),
                    );
                }
                if !def.env.is_empty() {
                    server_table.insert(
                        "env",
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            string_map_to_inline_table(&def.env),
                        )),
                    );
                }
            }
            McpTransport::Http | McpTransport::Sse => {
                if let Some(url) = &def.url {
                    server_table.insert(
                        "url",
                        toml_edit::Item::Value(toml_edit::Value::from(url.clone())),
                    );
                }
                if !def.headers.is_empty() {
                    server_table.insert(
                        "headers",
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            string_map_to_inline_table(&def.headers),
                        )),
                    );
                }
            }
        }
    }
}

fn transport_to_toml_value(transport: &McpTransport) -> toml_edit::Value {
    match transport {
        McpTransport::Stdio => toml_edit::Value::from("stdio"),
        McpTransport::Http => toml_edit::Value::from("http"),
        McpTransport::Sse => toml_edit::Value::from("sse"),
    }
}

fn string_map_to_inline_table(map: &BTreeMap<String, String>) -> toml_edit::InlineTable {
    let mut table = toml_edit::InlineTable::new();
    for (key, value) in map {
        table.insert(key, toml_edit::Value::from(value.clone()));
    }
    table
}
