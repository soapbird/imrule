//! Use case for managing MCP server definitions in imrule.toml.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::application::ports::{CachePort, ConfigPort, ConfigWritePort, FileSystemPort, McpPort};
use crate::domain::config::{McpRemoteTransport, McpServerDefinition, McpTransport};
use crate::domain::error::ImruleError;
use crate::domain::mcp::{
    build_imrule_mcp_config, expand_mcp_environment_variables, McpRemoteVersionCache,
};

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
    /// Optional connection window in milliseconds for agents that honor it.
    pub timeout: Option<u64>,
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

/// Runtime options for `imrule mcp auth`.
#[derive(Debug, Clone)]
pub struct McpAuthOptions {
    pub project_root: PathBuf,
    pub config_path: Option<PathBuf>,
}

/// An MCP server that was intentionally not passed to the authentication child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuthSkip {
    pub server_name: String,
    pub reason: &'static str,
}

/// Outcome of a sequential MCP authentication run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct McpAuthResult {
    pub authenticated: Vec<String>,
    pub skipped: Vec<McpAuthSkip>,
}

/// Resolves the concrete npm version used by the remote MCP bridge.
pub trait McpRemoteVersionResolverPort: Send + Sync {
    fn resolve_latest_version(&self) -> Result<String, ImruleError>;
}

/// Starts one interactive `mcp-remote` authentication flow.
pub trait McpAuthRunnerPort: Send + Sync {
    fn authenticate(&self, package_spec: &str, url: &str) -> Result<(), ImruleError>;
}

/// Reads the project cache or resolves and optionally persists a concrete version.
pub fn resolve_mcp_remote_version(
    cache_port: &dyn CachePort,
    resolver: &dyn McpRemoteVersionResolverPort,
    project_root: &Path,
    persist: bool,
) -> Result<McpRemoteVersionCache, ImruleError> {
    if let Some(cache) = cache_port.read_mcp_remote_version(project_root)? {
        return Ok(cache);
    }

    let resolved_version = resolver.resolve_latest_version()?;
    let resolved_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ImruleError::mcp(format!("system clock is before Unix epoch: {error}")))?
        .as_secs();
    let cache = McpRemoteVersionCache::new(resolved_version, resolved_at)?;
    if persist {
        cache_port.write_mcp_remote_version_atomic(project_root, &cache)?;
    }
    Ok(cache)
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
        tracing::info!(
            name = %options.name,
            global = options.global,
            dry_run = options.dry_run,
            "adding mcp server"
        );
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
        tracing::info!(
            name = %options.name,
            global = options.global,
            dry_run = options.dry_run,
            "removing mcp server"
        );
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

    /// Authenticates eligible remote servers sequentially through `mcp-remote`.
    pub fn auth(
        &self,
        options: McpAuthOptions,
        mcp_port: &dyn McpPort,
        fs_port: &dyn FileSystemPort,
        cache_port: &dyn CachePort,
        resolver: &dyn McpRemoteVersionResolverPort,
        runner: &dyn McpAuthRunnerPort,
    ) -> Result<McpAuthResult, ImruleError> {
        let config = self.config_port.load_config(
            &options.project_root,
            options.config_path.as_deref(),
            None,
        )?;
        if config.mcp.as_ref().and_then(|mcp| mcp.enabled) == Some(false) {
            return Ok(McpAuthResult::default());
        }
        let json_mcp = mcp_port.read_imrule_mcp_config(&options.project_root)?;
        let Some(mut effective_mcp) =
            build_imrule_mcp_config(json_mcp.as_ref(), &config.mcp_servers)
        else {
            return Ok(McpAuthResult::default());
        };
        let environment = load_mcp_environment(fs_port, &options.project_root)?;
        expand_mcp_environment_variables(&mut effective_mcp, &environment);

        let remote_transport = config
            .mcp
            .as_ref()
            .map(|mcp| mcp.remote_transport)
            .unwrap_or(McpRemoteTransport::McpRemote);
        let mut result = McpAuthResult::default();
        let mut eligible = Vec::new();

        if let Some(servers) = effective_mcp.get("mcpServers").and_then(Value::as_object) {
            for (server_name, server) in servers {
                let skip_reason = classify_auth_eligibility(server, &remote_transport);
                match skip_reason {
                    Some(reason) => result.skipped.push(McpAuthSkip {
                        server_name: server_name.clone(),
                        reason,
                    }),
                    None => {
                        let url = server
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        eligible.push((server_name.clone(), url));
                    }
                }
            }
        }

        if eligible.is_empty() {
            return Ok(result);
        }

        let cache = resolve_mcp_remote_version(cache_port, resolver, &options.project_root, true)?;
        let package_spec = cache.package_spec();
        for (server_name, url) in eligible {
            runner.authenticate(&package_spec, &url).map_err(|error| {
                ImruleError::mcp(format!(
                    "authentication failed for MCP server '{server_name}': {error}"
                ))
            })?;
            result.authenticated.push(server_name);
        }
        Ok(result)
    }
}

fn effective_project_root(project_root: PathBuf, global: bool) -> PathBuf {
    if global {
        crate::domain::constants::xdg_config_home().join("imrule")
    } else {
        project_root
    }
}

/// Classifies a server definition's eligibility for remote OAuth authentication.
/// Returns `Some(reason)` if the server should be skipped, or `None` if eligible.
fn classify_auth_eligibility(
    server: &Value,
    remote_transport: &McpRemoteTransport,
) -> Option<&'static str> {
    let server = server.as_object()?;
    if server.contains_key("command") {
        return Some("stdio servers do not use remote OAuth");
    }
    let url = server.get("url").and_then(Value::as_str)?;
    if !matches!(
        server.get("type").and_then(Value::as_str),
        None | Some("http") | Some("sse")
    ) {
        return Some("server is not an HTTP/SSE remote");
    }
    if *remote_transport == McpRemoteTransport::Native {
        return Some("remote transport is configured as native");
    }
    if server.contains_key("headers") {
        return Some("servers with static headers are not eligible for OAuth");
    }
    if has_unresolved_environment_reference(url) {
        return Some("server URL contains an unresolved environment placeholder");
    }
    None
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
                timeout: options.timeout,
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
                timeout: options.timeout,
            })
        }
    }
}

fn has_unresolved_environment_reference(value: &str) -> bool {
    let mut characters = value.char_indices();
    while let Some((_, character)) = characters.next() {
        if character != '$' {
            continue;
        }
        if characters
            .clone()
            .next()
            .is_some_and(|(_, next)| next == '{' || next == '_' || next.is_ascii_alphabetic())
        {
            return true;
        }
    }
    false
}

fn load_mcp_environment(
    fs_port: &dyn FileSystemPort,
    project_root: &Path,
) -> Result<BTreeMap<String, String>, ImruleError> {
    crate::application::load_mcp_environment(fs_port, project_root)
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
