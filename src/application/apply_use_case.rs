//! Native apply-engine use case.

use std::path::{Path, PathBuf};

use crate::application::ports::{
    AgentWriterPort, ConfigPort, FileSystemPort, GitignorePort, McpPort,
};
use crate::domain::agent::{all_agents, AgentDefinition, AgentOutputPaths};
use crate::domain::config::{AgentConfig, LoadedConfig, McpStrategy};
use crate::domain::error::ImruleError;
use crate::domain::mcp::{filter_mcp_config_for_agent, merge_mcp};
use crate::domain::rules::concatenate_rules;

/// Runtime options for `imrule apply`.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub project_root: PathBuf,
    pub agents: Option<Vec<String>>,
    pub config: Option<PathBuf>,
    pub mcp: bool,
    pub mcp_overwrite: bool,
    pub gitignore: Option<bool>,
    pub gitignore_local: bool,
    pub dry_run: bool,
    pub local_only: bool,
    pub backup: bool,
}

/// Apply use case orchestrating domain logic through ports.
pub struct ApplyUseCase<'a> {
    config_port: &'a dyn ConfigPort,
    fs_port: &'a dyn FileSystemPort,
    gitignore_port: &'a dyn GitignorePort,
    mcp_port: &'a dyn McpPort,
    agent_writer: &'a dyn AgentWriterPort,
}

impl<'a> ApplyUseCase<'a> {
    pub fn new(
        config_port: &'a dyn ConfigPort,
        fs_port: &'a dyn FileSystemPort,
        gitignore_port: &'a dyn GitignorePort,
        mcp_port: &'a dyn McpPort,
        agent_writer: &'a dyn AgentWriterPort,
    ) -> Self {
        Self {
            config_port,
            fs_port,
            gitignore_port,
            mcp_port,
            agent_writer,
        }
    }

    /// Applies Imrule rules using the Rust-native engine.
    pub fn execute(&self, options: ApplyOptions) -> Result<Vec<PathBuf>, ImruleError> {
        let config = self.config_port.load_config(
            &options.project_root,
            options.config.as_deref(),
            options.agents.clone(),
        )?;
        let selected_agents = resolve_selected_agents(&config, options.agents.as_deref())?;
        let ruler_dir = self
            .fs_port
            .find_ruler_dir(&options.project_root, !options.local_only)
            .ok_or_else(|| {
                ImruleError::rules(format!(
                    "could not find .ruler directory from {}",
                    options.project_root.display()
                ))
            })?;

        let include_agents = config
            .subagents
            .as_ref()
            .and_then(|subagents| subagents.include_in_rules)
            .unwrap_or(false);
        let rule_files = self
            .fs_port
            .read_markdown_files(&ruler_dir, include_agents)?;
        let rules = concatenate_rules(&rule_files, ruler_dir.parent());
        let mut written_paths = Vec::new();

        for agent in &selected_agents {
            let agent_config = config.agent_configs.get(agent.identifier);
            if agent_config.and_then(|cfg| cfg.enabled) == Some(false) {
                continue;
            }
            if let Some(path) = self.agent_writer.write_agent_rules(
                agent,
                &rules,
                &options.project_root,
                agent_config,
                options.backup,
                options.dry_run,
            )? {
                written_paths.push(path);
            }
        }

        if options.mcp && config.mcp.as_ref().and_then(|mcp| mcp.enabled) != Some(false) {
            let mcp_paths = self.apply_mcp_configs(&options, &config, &selected_agents)?;
            written_paths.extend(mcp_paths);
        }

        let gitignore_enabled = options
            .gitignore
            .or_else(|| {
                config
                    .gitignore
                    .as_ref()
                    .and_then(|gitignore| gitignore.enabled)
            })
            .unwrap_or(true);
        if gitignore_enabled {
            let ignore_file = if options.gitignore_local
                || config
                    .gitignore
                    .as_ref()
                    .and_then(|gitignore| gitignore.local)
                    == Some(true)
            {
                ".git/info/exclude"
            } else {
                ".gitignore"
            };
            if !options.dry_run {
                self.gitignore_port.update_gitignore(
                    &options.project_root,
                    &written_paths,
                    ignore_file,
                )?;
            }
        }

        Ok(written_paths)
    }

    fn apply_mcp_configs(
        &self,
        options: &ApplyOptions,
        config: &LoadedConfig,
        selected_agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let Some(ruler_mcp) = self.mcp_port.read_ruler_mcp_config(&options.project_root)? else {
            return Ok(Vec::new());
        };
        let strategy = if options.mcp_overwrite {
            McpStrategy::Overwrite
        } else {
            config
                .mcp
                .as_ref()
                .map(|mcp| mcp.strategy)
                .unwrap_or(McpStrategy::Merge)
        };
        let mut written = Vec::new();
        for agent in selected_agents {
            let Some(filtered) = filter_mcp_config_for_agent(&ruler_mcp, agent) else {
                continue;
            };
            let Some(path) = self
                .mcp_port
                .get_native_mcp_path(agent.name, &options.project_root)
            else {
                continue;
            };
            if options.dry_run {
                written.push(path);
                continue;
            }
            let existing = self.mcp_port.read_native_mcp(&path)?;
            let merged = merge_mcp(&existing, &filtered, strategy, agent.mcp_server_key);
            self.mcp_port.write_native_mcp(&path, &merged)?;
            written.push(path);
        }
        Ok(written)
    }
}

/// Returns all generated output paths for selected agents in supplied order.
pub fn get_agent_output_paths(project_root: &Path, agents: &[AgentDefinition]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for agent in agents {
        match agent.default_output_paths(project_root) {
            AgentOutputPaths::Single(path) => paths.push(path),
            AgentOutputPaths::Multiple(items) => {
                paths.extend(items.into_iter().map(|(_, path)| path))
            }
        }
    }
    paths
}

pub fn resolve_selected_agents(
    config: &LoadedConfig,
    cli_agents: Option<&[String]>,
) -> Result<Vec<AgentDefinition>, ImruleError> {
    let requested = cli_agents
        .map(|agents| agents.to_vec())
        .or_else(|| config.default_agents.clone());
    let all = all_agents();
    let Some(requested) = requested else {
        return Ok(all);
    };

    let mut selected = Vec::new();
    for raw in requested {
        let id = raw.trim();
        if id.is_empty() {
            continue;
        }
        let Some(agent) = all.iter().find(|agent| agent.identifier == id) else {
            return Err(ImruleError::unknown_agent(id));
        };
        selected.push(*agent);
    }
    Ok(selected)
}

pub fn instruction_output_path(
    project_root: &Path,
    agent: &AgentDefinition,
    agent_config: Option<&AgentConfig>,
) -> Option<PathBuf> {
    if let Some(path) = agent_config.and_then(|config| {
        config
            .output_path_instructions
            .as_ref()
            .or(config.output_path.as_ref())
    }) {
        return Some(resolve_project_path(project_root, path));
    }

    match agent.default_output_paths(project_root) {
        AgentOutputPaths::Single(path) => Some(path),
        AgentOutputPaths::Multiple(paths) => paths
            .into_iter()
            .find(|(key, _)| key == "instructions")
            .map(|(_, path)| path),
    }
}

fn resolve_project_path(project_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}
