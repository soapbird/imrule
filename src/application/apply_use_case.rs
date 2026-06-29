//! Native apply-engine use case.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::application::ports::{
    AgentWriterPort, ConfigPort, FileSystemPort, GitignorePort, McpPort,
};
use crate::domain::agent::{all_agents, AgentDefinition, AgentOutputPaths};
use crate::domain::config::{AgentConfig, LoadedConfig, McpStrategy};
use crate::domain::constants::normalize_path_separators;
use crate::domain::error::ImruleError;
use crate::domain::mcp::{build_imrule_mcp_config, filter_mcp_config_for_agent, merge_mcp};
use crate::domain::rules::concatenate_rules;
use crate::domain::skills::get_skills_gitignore_paths;
use crate::infrastructure::skills::{copy_skills_directory, discover_skills};

/// Runtime options for `imrule apply`.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub project_root: PathBuf,
    pub agents: Option<Vec<String>>,
    pub config: Option<PathBuf>,
    pub dry_run: bool,
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

    /// Applies ImRule rules using the Rust-native engine.
    pub fn execute(&self, options: ApplyOptions) -> Result<Vec<PathBuf>, ImruleError> {
        tracing::info!(
            project_root = %options.project_root.display(),
            dry_run = options.dry_run,
            "starting apply"
        );
        let config = self.config_port.load_config(
            &options.project_root,
            options.config.as_deref(),
            options.agents.clone(),
        )?;
        let selected_agents = resolve_selected_agents(&config, options.agents.as_deref())?;
        tracing::info!(agent_count = selected_agents.len(), "selected agents");
        let imrule_dir = self
            .fs_port
            .find_imrule_dir(&options.project_root, true)
            .ok_or_else(|| {
                ImruleError::rules(format!(
                    "could not find .imrule or .ruler directory from {}",
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
            .read_markdown_files(&imrule_dir, include_agents)?;
        tracing::info!(
            markdown_count = rule_files.len(),
            "discovered markdown files"
        );
        let rules = concatenate_rules(&rule_files, imrule_dir.parent());

        let rule_results: Result<Vec<_>, ImruleError> = selected_agents
            .par_iter()
            .filter(|agent| {
                let agent_config = config.agent_configs.get(agent.identifier);
                agent_config.and_then(|cfg| cfg.enabled) != Some(false)
            })
            .map(|agent| {
                let agent_config = config.agent_configs.get(agent.identifier);
                self.agent_writer.write_agent_rules(
                    agent,
                    &rules,
                    &options.project_root,
                    agent_config,
                    options.backup,
                    options.dry_run,
                )
            })
            .collect();
        let mut written_paths: Vec<PathBuf> = rule_results?.into_iter().flatten().collect();

        if config.mcp.as_ref().and_then(|mcp| mcp.enabled) != Some(false) {
            let mcp_paths = self.apply_mcp_configs(&options, &config, &selected_agents)?;
            written_paths.extend(mcp_paths);
        }

        let skills_enabled = config
            .skills
            .as_ref()
            .and_then(|s| s.enabled)
            .unwrap_or(true);
        if skills_enabled {
            let skills_paths =
                self.apply_skills(&options.project_root, &selected_agents, options.dry_run)?;
            written_paths.extend(skills_paths);
        }

        let subagents_enabled = config
            .subagents
            .as_ref()
            .and_then(|s| s.enabled)
            .unwrap_or(true);
        if subagents_enabled {
            let subagents_paths = self.apply_subagents(&options, &selected_agents)?;
            written_paths.extend(subagents_paths);
        }

        let gitignore_enabled = config
            .gitignore
            .as_ref()
            .and_then(|gitignore| gitignore.enabled)
            .unwrap_or(true);
        if gitignore_enabled && !options.dry_run {
            let gitignore_paths = collapse_gitignore_paths(&written_paths, &options.project_root);
            self.gitignore_port.update_gitignore(
                &options.project_root,
                &gitignore_paths,
                ".gitignore",
            )?;
        }

        tracing::info!(written_count = written_paths.len(), "apply completed");
        Ok(written_paths)
    }

    fn apply_mcp_configs(
        &self,
        options: &ApplyOptions,
        config: &LoadedConfig,
        selected_agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let json_mcp = self
            .mcp_port
            .read_imrule_mcp_config(&options.project_root)?;
        let Some(imrule_mcp) = build_imrule_mcp_config(json_mcp.as_ref(), &config.mcp_servers)
        else {
            return Ok(Vec::new());
        };
        let strategy = config
            .mcp
            .as_ref()
            .map(|mcp| mcp.strategy)
            .unwrap_or(McpStrategy::Merge);
        let written: Result<Vec<_>, ImruleError> = selected_agents
            .par_iter()
            .filter_map(|agent| {
                let filtered = filter_mcp_config_for_agent(&imrule_mcp, agent)?;
                let path = self
                    .mcp_port
                    .get_native_mcp_path(agent.name, &options.project_root)?;
                Some((agent, filtered, path))
            })
            .map(|(agent, filtered, path)| {
                if options.dry_run {
                    return Ok(path);
                }
                let existing = self.mcp_port.read_native_mcp(&path)?;
                let merged = merge_mcp(&existing, &filtered, strategy, agent.mcp_server_key);
                self.mcp_port.write_native_mcp(&path, &merged)?;
                Ok(path)
            })
            .collect();
        written
    }

    fn apply_subagents(
        &self,
        options: &ApplyOptions,
        selected_agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let discovery = crate::infrastructure::subagents::discover_subagents(&options.project_root)
            .map_err(|e| ImruleError::subagent(e.to_string()))?;
        if discovery.subagents.is_empty() {
            return Ok(Vec::new());
        }

        let target_map: &[(&str, &str, &str)] = &[
            (
                "claude",
                crate::domain::constants::CLAUDE_SUBAGENTS_PATH,
                "claude",
            ),
            (
                "codex",
                crate::domain::constants::CODEX_SUBAGENTS_PATH,
                "codex",
            ),
            (
                "cursor",
                crate::domain::constants::CURSOR_SUBAGENTS_PATH,
                "cursor",
            ),
            (
                "copilot",
                crate::domain::constants::COPILOT_SUBAGENTS_PATH,
                "copilot",
            ),
        ];

        let mut written = Vec::new();
        let mut seen_targets = std::collections::BTreeSet::new();

        for agent in selected_agents {
            if !agent.capabilities.native_subagents {
                continue;
            }
            let Some(&(_, target_rel, agent_type)) =
                target_map.iter().find(|(id, _, _)| *id == agent.identifier)
            else {
                continue;
            };

            let target_dir = options.project_root.join(target_rel);
            let key = target_dir.to_string_lossy().to_string();
            if seen_targets.contains(&key) {
                continue;
            }
            seen_targets.insert(key);

            if options.dry_run {
                written.push(target_dir);
                continue;
            }

            self.fs_port
                .ensure_dir_exists(&target_dir)
                .map_err(|e| ImruleError::subagent(e.to_string()))?;

            let sub_results: Result<Vec<_>, ImruleError> = discovery
                .subagents
                .par_iter()
                .map(|sub| {
                    let content = match agent_type {
                        "claude" => crate::domain::subagent::build_claude_file(sub),
                        "cursor" => crate::domain::subagent::build_cursor_file(sub),
                        "codex" => crate::domain::subagent::build_codex_file(sub),
                        "copilot" => crate::domain::subagent::build_copilot_file(sub).content,
                        _ => return Ok(None),
                    };
                    let dest = target_dir.join(format!("{}.md", sub.name));
                    self.fs_port.write_text(&dest, &content).map_err(|e| {
                        ImruleError::subagent(format!(
                            "failed to write subagent '{}': {e}",
                            sub.name
                        ))
                    })?;
                    Ok(Some(dest))
                })
                .collect();
            written.extend(sub_results?.into_iter().flatten());
        }

        let gitignore_paths = crate::infrastructure::subagents::get_subagents_gitignore_paths(
            &options.project_root,
            selected_agents,
        )
        .map_err(|e| {
            ImruleError::subagent(format!("failed to get subagent gitignore paths: {e}"))
        })?;
        for path in gitignore_paths {
            if !written.contains(&path) {
                written.push(path);
            }
        }

        Ok(written)
    }

    fn apply_skills(
        &self,
        project_root: &Path,
        selected_agents: &[AgentDefinition],
        dry_run: bool,
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let imrule_skills_dir = project_root.join(crate::domain::constants::IMRULE_SKILLS_PATH);
        let legacy_skills_dir = project_root.join(crate::domain::constants::LEGACY_SKILLS_PATH);
        if !imrule_skills_dir.exists() && !legacy_skills_dir.exists() {
            return Ok(Vec::new());
        }

        let discovery =
            discover_skills(project_root).map_err(|e| ImruleError::skills(e.to_string()))?;
        if discovery.skills.is_empty() {
            return Ok(Vec::new());
        }

        let agent_skill_paths: &[(&str, &str)] = &[
            ("claude", crate::domain::constants::CLAUDE_SKILLS_PATH),
            ("copilot", crate::domain::constants::CLAUDE_SKILLS_PATH),
            ("kilocode", crate::domain::constants::CLAUDE_SKILLS_PATH),
            ("codex", crate::domain::constants::CODEX_SKILLS_PATH),
            ("opencode", crate::domain::constants::OPENCODE_SKILLS_PATH),
            ("pi", crate::domain::constants::PI_SKILLS_PATH),
            ("goose", crate::domain::constants::GOOSE_SKILLS_PATH),
            ("amp", crate::domain::constants::GOOSE_SKILLS_PATH),
            ("mistral", crate::domain::constants::VIBE_SKILLS_PATH),
            ("roo", crate::domain::constants::ROO_SKILLS_PATH),
            ("gemini-cli", crate::domain::constants::GEMINI_SKILLS_PATH),
            ("kimi-cli", crate::domain::constants::KIMI_SKILLS_PATH),
            ("kimi-code", crate::domain::constants::KIMI_SKILLS_PATH),
            ("kimi", crate::domain::constants::KIMI_SKILLS_PATH),
            ("junie", crate::domain::constants::JUNIE_SKILLS_PATH),
            ("cursor", crate::domain::constants::CURSOR_SKILLS_PATH),
            ("windsurf", crate::domain::constants::WINDSURF_SKILLS_PATH),
            ("factory", crate::domain::constants::FACTORY_SKILLS_PATH),
            (
                "antigravity",
                crate::domain::constants::ANTIGRAVITY_SKILLS_PATH,
            ),
            ("gjc", crate::domain::constants::GJC_SKILLS_PATH),
        ];

        let mut written = Vec::new();
        let mut seen_targets = std::collections::BTreeSet::new();

        for agent in selected_agents {
            if !agent.capabilities.native_skills {
                continue;
            }
            let Some(&target_rel) = agent_skill_paths
                .iter()
                .find(|(id, _)| id == &agent.identifier)
                .map(|(_, path)| path)
            else {
                continue;
            };

            let target_dir = project_root.join(target_rel);
            let target_key = target_dir.to_string_lossy().to_string();
            if seen_targets.contains(&target_key) {
                continue;
            }
            seen_targets.insert(target_key.clone());

            if dry_run {
                written.push(target_dir);
                continue;
            }

            let copy_results: Result<Vec<_>, ImruleError> = discovery
                .skills
                .par_iter()
                .map(|skill| {
                    let dest = target_dir.join(&skill.name);
                    copy_skills_directory(&skill.path, &dest)
                        .map_err(|e| ImruleError::skills(e.to_string()))?;
                    Ok(dest)
                })
                .collect();
            copy_results?;
            written.push(target_dir);
        }

        let gitignore_skill_paths = get_skills_gitignore_paths(project_root, selected_agents);
        for path in gitignore_skill_paths {
            if !written.contains(&path) {
                written.push(path);
            }
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

/// Collapses generated paths under agent-specific directories to the directory itself.
///
/// Gajae Code stores rules, MCP config, and skills under `.gjc/`, and the directory
/// also contains runtime state that should not be committed. Ignore the whole
/// directory instead of individual files.
fn collapse_gitignore_paths(paths: &[PathBuf], project_root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut collapsed_gjc = false;

    for path in paths {
        let relative = path.strip_prefix(project_root).unwrap_or(path);
        let relative_str = normalize_path_separators(&relative.to_string_lossy());
        if relative_str.starts_with(".gjc/") {
            if !collapsed_gjc {
                result.push(project_root.join(".gjc"));
                collapsed_gjc = true;
            }
        } else {
            result.push(path.clone());
        }
    }

    result
}
