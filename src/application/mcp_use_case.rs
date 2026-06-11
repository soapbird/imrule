//! Use case for managing MCP server definitions in imrule.toml.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::application::ports::{ConfigPort, ConfigWritePort};
use crate::domain::config::{McpServerDefinition, McpTransport};
use crate::domain::error::ImruleError;

/// Runtime options for `imrule mcp add`.
#[derive(Debug, Clone)]
pub struct McpAddOptions {
    pub project_root: PathBuf,
    pub config_path: Option<PathBuf>,
    pub global: bool,
    pub dry_run: bool,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
}

/// Runtime options for `imrule mcp remove`.
#[derive(Debug, Clone)]
pub struct McpRemoveOptions {
    pub project_root: PathBuf,
    pub config_path: Option<PathBuf>,
    pub global: bool,
    pub dry_run: bool,
    pub name: String,
}

/// Use case for adding and removing MCP servers from imrule.toml.
pub struct McpUseCase<'a> {
    config_port: &'a dyn ConfigPort,
    config_write_port: &'a dyn ConfigWritePort,
}

impl<'a> McpUseCase<'a> {
    pub fn new(
        config_port: &'a dyn ConfigPort,
        config_write_port: &'a dyn ConfigWritePort,
    ) -> Self {
        Self {
            config_port,
            config_write_port,
        }
    }

    /// Adds or updates an MCP server definition in imrule.toml.
    pub fn add(&self, options: McpAddOptions) -> Result<(), ImruleError> {
        let effective_root = effective_project_root(options.project_root.clone(), options.global);
        let mut config =
            self.config_port
                .load_config(&effective_root, options.config_path.as_deref(), None)?;

        let definition = build_definition(&options)?;
        config.mcp_servers.insert(options.name.clone(), definition);

        if options.dry_run {
            return Ok(());
        }

        self.config_write_port
            .save_config(&effective_root, options.config_path.as_deref(), &config)
    }

    /// Removes an MCP server definition from imrule.toml.
    pub fn remove(&self, options: McpRemoveOptions) -> Result<(), ImruleError> {
        let effective_root = effective_project_root(options.project_root, options.global);
        let mut config =
            self.config_port
                .load_config(&effective_root, options.config_path.as_deref(), None)?;

        if !config.mcp_servers.contains_key(&options.name) {
            return Err(ImruleError::mcp(format!(
                "MCP server '{}' not found in configuration",
                options.name
            )));
        }

        config.mcp_servers.remove(&options.name);

        if options.dry_run {
            return Ok(());
        }

        self.config_write_port
            .save_config(&effective_root, options.config_path.as_deref(), &config)
    }
}

fn effective_project_root(project_root: PathBuf, global: bool) -> PathBuf {
    if global {
        crate::domain::constants::xdg_config_home().join("imrule")
    } else {
        project_root
    }
}

fn build_definition(options: &McpAddOptions) -> Result<McpServerDefinition, ImruleError> {
    match options.transport {
        McpTransport::Stdio => {
            let command = options
                .command
                .clone()
                .ok_or_else(|| ImruleError::mcp("stdio transport requires a command"))?;
            Ok(McpServerDefinition {
                transport: McpTransport::Stdio,
                url: None,
                command: Some(command),
                args: options.args.clone(),
                env: options.env.clone(),
                headers: BTreeMap::new(),
            })
        }
        McpTransport::Http | McpTransport::Sse => {
            let url = options
                .url
                .clone()
                .ok_or_else(|| ImruleError::mcp("remote transport requires a URL"))?;
            Ok(McpServerDefinition {
                transport: options.transport,
                url: Some(url),
                command: None,
                args: Vec::new(),
                env: BTreeMap::new(),
                headers: options.headers.clone(),
            })
        }
    }
}

/// Parses a `KEY=VALUE` string into its two parts.
pub fn parse_env_pair(pair: &str) -> Result<(String, String), ImruleError> {
    let Some((key, value)) = pair.split_once('=') else {
        return Err(ImruleError::mcp(format!(
            "invalid environment variable '{pair}', expected KEY=VALUE"
        )));
    };
    Ok((key.to_string(), value.to_string()))
}

/// Parses a list of `KEY=VALUE` strings into a map.
pub fn parse_env_pairs(pairs: &[String]) -> Result<BTreeMap<String, String>, ImruleError> {
    let mut map = BTreeMap::new();
    for pair in pairs {
        let (key, value) = parse_env_pair(pair)?;
        map.insert(key, value);
    }
    Ok(map)
}
