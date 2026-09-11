//! Native apply-engine use case.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::application::mcp_use_case::{McpRemoteVersionResolverPort, resolve_mcp_remote_version};
use crate::application::ports::{
    AgentWriterPort, CachePort, ConfigPort, FileSystemPort, GitTrackingPort, GitignorePort,
    ManifestPort, McpPort,
};
use crate::domain::agent::{AgentDefinition, AgentOutputPaths, all_agents, find_agent};
use crate::domain::config::{AgentConfig, LoadedConfig, McpRemoteTransport, McpStrategy};
use crate::domain::constants::{
    GENERATED_BY_IMRULE_MARKER, IMRULE_GENERATED_STATE_PATHS, normalize_path_separators,
};
use crate::domain::error::ImruleError;
use crate::domain::manifest::ApplyManifest;
use crate::domain::mcp::{
    McpRemoteTransportPolicy, McpRemoteVersionCache, build_imrule_mcp_config,
    expand_mcp_environment_variables, filter_mcp_config_for_agent,
    filter_mcp_config_for_agent_with_package_spec, is_native_mcp_content_empty, merge_mcp,
    validate_mcp_config_for_remote_transport,
};
use crate::domain::rules::concatenate_rules;
use crate::domain::skills::get_skills_gitignore_paths;

/// Runtime options for `imrule apply`.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub project_root: PathBuf,
    pub agents: Option<Vec<String>>,
    pub config: Option<PathBuf>,
    pub dry_run: bool,
    pub backup: bool,
}

/// What the MCP stage of an apply run propagated, kept together so the manifest
/// can record both the files written and the server names written into them.
#[derive(Debug, Default)]
struct McpApplyOutcome {
    /// Native MCP config path paired with that agent's server section key.
    targets: Vec<(PathBuf, String)>,
    /// Server names propagated into those files.
    servers: Vec<String>,
}

/// Outcome of an apply run.
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Files written or considered during apply.
    pub written: Vec<PathBuf>,
    /// Generated files removed from the git index (kept on disk) because they
    /// were tracked even though ImRule ignores them.
    pub untracked: Vec<PathBuf>,
}

/// Apply use case orchestrating domain logic through ports.
pub struct ApplyUseCase<'a> {
    config_port: &'a dyn ConfigPort,
    fs_port: &'a dyn FileSystemPort,
    gitignore_port: &'a dyn GitignorePort,
    git_tracking_port: &'a dyn GitTrackingPort,
    mcp_port: &'a dyn McpPort,
    agent_writer: &'a dyn AgentWriterPort,
    cache_port: Option<&'a dyn CachePort>,
    version_resolver: Option<&'a dyn McpRemoteVersionResolverPort>,
    manifest_port: Option<&'a dyn ManifestPort>,
}

impl<'a> ApplyUseCase<'a> {
    pub fn new(
        config_port: &'a dyn ConfigPort,
        fs_port: &'a dyn FileSystemPort,
        gitignore_port: &'a dyn GitignorePort,
        git_tracking_port: &'a dyn GitTrackingPort,
        mcp_port: &'a dyn McpPort,
        agent_writer: &'a dyn AgentWriterPort,
    ) -> Self {
        Self {
            config_port,
            fs_port,
            gitignore_port,
            git_tracking_port,
            mcp_port,
            agent_writer,
            cache_port: None,
            version_resolver: None,
            manifest_port: None,
        }
    }

    /// Enables reconciliation against the previous run: outputs this apply no
    /// longer produces are cleaned up instead of being orphaned on disk.
    pub fn with_manifest(mut self, manifest_port: &'a dyn ManifestPort) -> Self {
        self.manifest_port = Some(manifest_port);
        self
    }

    /// Enables project-scoped concrete `mcp-remote` version reuse.
    pub fn with_mcp_remote_version_cache(
        mut self,
        cache_port: &'a dyn CachePort,
        version_resolver: &'a dyn McpRemoteVersionResolverPort,
    ) -> Self {
        self.cache_port = Some(cache_port);
        self.version_resolver = Some(version_resolver);
        self
    }

    /// Applies ImRule rules using the Rust-native engine.
    pub fn execute(&self, options: ApplyOptions) -> Result<ApplyResult, ImruleError> {
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

        let mut mcp_outcome = McpApplyOutcome::default();
        if config.mcp.as_ref().and_then(|mcp| mcp.enabled) != Some(false) {
            mcp_outcome = self.apply_mcp_configs(&options, &config, &selected_agents)?;
            written_paths.extend(mcp_outcome.targets.iter().map(|(path, _)| path.clone()));
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

        // Reconcile against the previous run before touching `.gitignore`, so
        // outputs this run no longer produces are removed from disk instead of
        // silently falling out of the managed block and becoming committable.
        //
        // Only a full run may do this. `--agents` narrows a single invocation;
        // the agents it leaves out have not been dropped from the project, so
        // their files must survive and stay recorded.
        let full_run = options.agents.is_none();
        let mut manifest = ApplyManifest::new(
            &options.project_root,
            &written_paths,
            &mcp_outcome.servers,
            &mcp_outcome.targets,
        );
        if !options.dry_run {
            if let Some(manifest_port) = self.manifest_port {
                if let Some(previous) = manifest_port.read_manifest(&options.project_root)? {
                    if full_run {
                        self.prune_stale_outputs(&options.project_root, &previous, &manifest)?;
                    } else {
                        manifest = manifest.merged_with(&previous);
                    }
                }
                manifest_port.write_manifest(&options.project_root, &manifest)?;
                // The manifest and the version cache live inside `.imrule/` but
                // are generated, not authored — ignore them like any other output.
                for generated in IMRULE_GENERATED_STATE_PATHS {
                    let path = options.project_root.join(generated);
                    if self.fs_port.file_exists(&path) && !written_paths.contains(&path) {
                        written_paths.push(path);
                    }
                }
            }
        }

        let gitignore_enabled = config
            .gitignore
            .as_ref()
            .and_then(|gitignore| gitignore.enabled)
            .unwrap_or(true);
        let mut untracked = Vec::new();
        if gitignore_enabled && !options.dry_run {
            let gitignore_paths = collapse_gitignore_paths(&written_paths, &options.project_root);
            self.gitignore_port.update_gitignore(
                &options.project_root,
                &gitignore_paths,
                ".gitignore",
            )?;
            // `.gitignore` has no effect on files git already tracks, so drop
            // generated files from the index while keeping them on disk.
            untracked = self
                .git_tracking_port
                .untrack_generated_files(&options.project_root, &written_paths)?;
            if !untracked.is_empty() {
                tracing::info!(count = untracked.len(), "untracked generated files");
            }
        }

        tracing::info!(written_count = written_paths.len(), "apply completed");
        Ok(ApplyResult {
            written: written_paths,
            untracked,
        })
    }

    /// Merges and writes MCP server configs to every selected agent's native config file.
    ///
    /// # Security note
    ///
    /// Environment variables referenced via `$TOKEN` in `.env` files are resolved
    /// and written in plaintext to each agent's native MCP config. This means
    /// resolved secret values are materialized on disk in multiple locations.
    /// Generated files are added to `.gitignore` and untracked from the git index
    /// to mitigate accidental commits.
    fn apply_mcp_configs(
        &self,
        options: &ApplyOptions,
        config: &LoadedConfig,
        selected_agents: &[AgentDefinition],
    ) -> Result<McpApplyOutcome, ImruleError> {
        let json_mcp = self
            .mcp_port
            .read_imrule_mcp_config(&options.project_root)?;
        let Some(mut imrule_mcp) = build_imrule_mcp_config(json_mcp.as_ref(), &config.mcp_servers)
        else {
            return Ok(McpApplyOutcome::default());
        };
        let environment = self.load_mcp_environment(&options.project_root)?;
        expand_mcp_environment_variables(&mut imrule_mcp, &environment);
        let strategy = config
            .mcp
            .as_ref()
            .map(|mcp| mcp.strategy)
            .unwrap_or(McpStrategy::Merge);
        let remote_transport = McpRemoteTransportPolicy::from_sources(
            config.mcp.as_ref(),
            json_mcp.as_ref(),
            &config.mcp_servers,
        );
        validate_mcp_config_for_remote_transport(&imrule_mcp, &remote_transport)?;
        let version_cache = self.mcp_remote_version_cache(
            options,
            &imrule_mcp,
            selected_agents,
            &remote_transport,
        )?;
        let candidates: Vec<_> = {
            let cached_spec = version_cache.as_ref().map(|c| c.package_spec());
            selected_agents
                .par_iter()
                .filter_map(|agent| {
                    let filtered = match cached_spec.as_deref() {
                        Some(spec) => filter_mcp_config_for_agent_with_package_spec(
                            &imrule_mcp,
                            agent,
                            &remote_transport,
                            spec,
                        )?,
                        None => filter_mcp_config_for_agent(&imrule_mcp, agent, &remote_transport)?,
                    };
                    let path = self
                        .mcp_port
                        .get_native_mcp_path(agent.name, &options.project_root)?;
                    Some((agent, filtered, path))
                })
                .collect()
        };

        // Dedup by target path. Several agent aliases (e.g. the Kimi trio
        // kimi/kimi-cli/kimi-code) resolve to the same native MCP file; writing
        // them concurrently below is a read-modify-write race on one file. Keep
        // the first occurrence per path and drop the rest.
        let mut seen_paths = HashSet::new();
        let unique: Vec<_> = candidates
            .into_iter()
            .filter(|(_, _, path)| seen_paths.insert(path.clone()))
            .collect();

        let written: Result<Vec<_>, ImruleError> = unique
            .par_iter()
            .map(|(agent, filtered, path)| {
                let target = (path.clone(), agent.mcp_server_key.to_string());
                if options.dry_run {
                    return Ok(target);
                }
                let existing = self.mcp_port.read_native_mcp(path)?;
                let merged = merge_mcp(&existing, filtered, strategy, agent.mcp_server_key);
                self.mcp_port.write_native_mcp(path, &merged)?;
                Ok(target)
            })
            .collect();

        Ok(McpApplyOutcome {
            targets: written?,
            servers: imrule_mcp
                .get("mcpServers")
                .and_then(serde_json::Value::as_object)
                .map(|servers| servers.keys().cloned().collect())
                .unwrap_or_default(),
        })
    }

    /// Removes outputs the previous run produced that this run no longer does.
    ///
    /// Three kinds of drift are reconciled:
    /// 1. A native MCP config no longer written at all (MCP disabled, the agent
    ///    deselected) — strip the servers the previous run put there and delete
    ///    the file when nothing meaningful is left.
    /// 2. A server dropped from the config while its target file is still
    ///    written — the merge strategy only ever adds keys, so remove it here.
    /// 3. A generated rule file, skills root, or subagent directory no longer
    ///    produced — delete it, but only when it still carries ImRule's marker,
    ///    so a path the user has since taken over is left alone.
    fn prune_stale_outputs(
        &self,
        project_root: &Path,
        previous: &ApplyManifest,
        current: &ApplyManifest,
    ) -> Result<(), ImruleError> {
        let dropped_servers = previous.stale_mcp_servers(current);

        for target in previous.stale_mcp_targets(current) {
            let path = project_root.join(&target.path);
            if !self.fs_port.file_exists(&path) {
                continue;
            }
            self.remove_mcp_servers(&path, &target.server_key, &previous.mcp_servers)?;
            self.prune_empty_parents(&path, project_root)?;
        }

        if !dropped_servers.is_empty() {
            for target in &current.mcp_targets {
                let path = project_root.join(&target.path);
                if !self.fs_port.file_exists(&path) {
                    continue;
                }
                self.remove_mcp_servers(&path, &target.server_key, &dropped_servers)?;
                self.prune_empty_parents(&path, project_root)?;
            }
        }

        for stale in previous.stale_paths(current) {
            let path = project_root.join(&stale);
            if !self.fs_port.file_exists(&path) {
                continue;
            }
            if self.fs_port.dir_exists(&path) {
                self.fs_port.remove_dir_all(&path)?;
            } else if self
                .fs_port
                .read_text(&path)
                .is_ok_and(|content| content.starts_with(GENERATED_BY_IMRULE_MARKER))
            {
                self.fs_port.remove_file(&path)?;
            } else {
                continue;
            }
            tracing::info!(path = %path.display(), "removed stale generated output");
            self.prune_empty_parents(&path, project_root)?;
        }

        Ok(())
    }

    /// Strips `servers` from a native MCP config, deleting the file when nothing
    /// meaningful remains. Uses the raw write path so the user's own remaining
    /// servers are never reshaped.
    fn remove_mcp_servers(
        &self,
        native_path: &Path,
        server_key: &str,
        servers: &[String],
    ) -> Result<(), ImruleError> {
        if servers.is_empty() {
            return Ok(());
        }
        let mut config = self.mcp_port.read_native_mcp(native_path)?;
        let Some(object) = config.as_object_mut() else {
            return Ok(());
        };
        for section in [server_key, "mcpServers"] {
            if section.is_empty() {
                continue;
            }
            if let Some(entries) = object.get_mut(section).and_then(|v| v.as_object_mut()) {
                for name in servers {
                    entries.remove(name.as_str());
                }
            }
        }
        self.mcp_port.write_native_mcp_raw(native_path, &config)?;

        let content = self.fs_port.read_text(native_path)?;
        if is_native_mcp_content_empty(&content) {
            self.fs_port.remove_file(native_path)?;
            tracing::info!(path = %native_path.display(), "removed emptied MCP config");
        }
        Ok(())
    }

    /// Removes now-empty directories from `path`'s parent up to `project_root`.
    fn prune_empty_parents(&self, path: &Path, project_root: &Path) -> Result<(), ImruleError> {
        let mut dir = match path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Ok(()),
        };
        while dir != project_root && dir.starts_with(project_root) {
            if !self.fs_port.remove_dir_if_empty(&dir)? {
                break;
            }
            if !dir.pop() {
                break;
            }
        }
        Ok(())
    }

    fn mcp_remote_version_cache(
        &self,
        options: &ApplyOptions,
        mcp_config: &serde_json::Value,
        selected_agents: &[AgentDefinition],
        remote_transport: &McpRemoteTransportPolicy,
    ) -> Result<Option<McpRemoteVersionCache>, ImruleError> {
        if !selected_agents
            .iter()
            .any(|agent| agent.capabilities.mcp_stdio)
            || !mcp_config
                .get("mcpServers")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|servers| {
                    servers.iter().any(|(server_name, server)| {
                        remote_transport.for_server(server_name) == McpRemoteTransport::McpRemote
                            && server.as_object().is_some_and(|server| {
                                server.contains_key("url")
                                    && !server.contains_key("command")
                                    && matches!(
                                        server.get("type").and_then(serde_json::Value::as_str),
                                        None | Some("http") | Some("sse")
                                    )
                            })
                    })
                })
        {
            return Ok(None);
        }

        match (self.cache_port, self.version_resolver) {
            (Some(cache_port), Some(version_resolver)) => {
                match resolve_mcp_remote_version(
                    cache_port,
                    version_resolver,
                    &options.project_root,
                    !options.dry_run,
                ) {
                    Ok(cache) => Ok(Some(cache)),
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            "failed to resolve mcp-remote version; falling back to @latest"
                        );
                        Ok(None)
                    }
                }
            }
            _ => Ok(None),
        }
    }
    fn load_mcp_environment(
        &self,
        project_root: &Path,
    ) -> Result<BTreeMap<String, String>, ImruleError> {
        crate::application::load_mcp_environment(self.fs_port, project_root)
    }

    fn apply_subagents(
        &self,
        options: &ApplyOptions,
        selected_agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let discovery = self.fs_port.discover_subagents(&options.project_root)?;
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

        // Discovery found subagents above, so the source directory exists and
        // every selected target is really written.
        let gitignore_paths = crate::domain::subagent::subagents_gitignore_paths(
            &options.project_root,
            selected_agents,
        );
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
        let discovery = self.fs_port.discover_skills(project_root)?;
        if discovery.skills.is_empty() {
            return Ok(Vec::new());
        }

        let mut written = Vec::new();

        for target_dir in get_skills_gitignore_paths(project_root, selected_agents) {
            if !dry_run {
                let copy_results: Result<Vec<_>, ImruleError> = discovery
                    .skills
                    .par_iter()
                    .map(|skill| {
                        let dest = target_dir.join(&skill.name);
                        self.fs_port.copy_dir(&skill.path, &dest)?;
                        Ok(dest)
                    })
                    .collect();
                copy_results?;
            }
            written.push(target_dir);
        }

        // GJC gates native skill discovery behind opt-in settings that default
        // to false, so copying skills to .gjc/skills/ is not enough — the agent
        // must also be told to scan that directory. Enable project-scoped
        // discovery by merging into .gjc/config.yml whenever gjc was processed.
        let gjc_selected = selected_agents
            .iter()
            .any(|agent| agent.identifier == "gjc" && agent.capabilities.native_skills);
        if gjc_selected {
            let config_path = project_root.join(crate::domain::constants::GJC_CONFIG_PATH);
            if dry_run {
                written.push(config_path);
            } else {
                let existing = self.fs_port.read_text(&config_path).ok();
                let merged =
                    crate::domain::gjc_config::enable_gjc_skill_discovery(existing.as_deref())?;
                self.fs_port
                    .write_text(&config_path, &merged)
                    .map_err(|e| {
                        ImruleError::skills(format!("failed to write .gjc/config.yml: {e}"))
                    })?;
                written.push(config_path);
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
        .or_else(|| config.agents.clone());
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
        let Some(agent) = find_agent(id) else {
            return Err(ImruleError::unknown_agent(id));
        };
        selected.push(agent);
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
