//! Port traits that abstract infrastructure concerns from application use cases.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::agent::AgentDefinition;
use crate::domain::config::{AgentConfig, LoadedConfig};
use crate::domain::error::ImruleError;
use crate::domain::manifest::ApplyManifest;
use crate::domain::mcp::McpRemoteVersionCache;
use crate::domain::skills::{RemoteSkillSource, SkillsDiscovery};
use crate::domain::subagent::SubagentsDiscovery;

/// Loads and parses ImRule configuration.
pub trait ConfigPort: Send + Sync {
    /// Loads local/global ImRule TOML configuration.
    fn load_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        cli_agents: Option<Vec<String>>,
    ) -> Result<LoadedConfig, ImruleError>;
}

/// Persists ImRule configuration back to the resolved TOML file.
pub trait ConfigWritePort: Send + Sync {
    /// Saves the given configuration to the resolved config file.
    fn save_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        config: &LoadedConfig,
    ) -> Result<(), ImruleError>;
}

/// Abstracts filesystem operations so use cases remain testable.
pub trait FileSystemPort: Send + Sync {
    /// Read a text file.
    fn read_text(&self, path: &Path) -> Result<String, ImruleError>;

    /// Write text to a file, creating parent directories.
    fn write_text(&self, path: &Path, content: &str) -> Result<(), ImruleError>;

    /// Backup an existing file to `<file>.bak`.
    fn backup_file(&self, path: &Path) -> Result<(), ImruleError>;

    /// Ensure a directory exists.
    fn ensure_dir_exists(&self, path: &Path) -> Result<(), ImruleError>;

    /// Remove a file.
    fn remove_file(&self, path: &Path) -> Result<(), ImruleError>;

    /// Recursively remove a directory and all its contents.
    fn remove_dir_all(&self, path: &Path) -> Result<(), ImruleError>;

    /// Remove a directory only if it is empty.
    /// Returns `true` if the directory was removed, `false` if it was not empty or didn't exist.
    fn remove_dir_if_empty(&self, path: &Path) -> Result<bool, ImruleError>;

    /// Copy a file.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), ImruleError>;

    /// Check whether a path exists as a file.
    fn file_exists(&self, path: &Path) -> bool;

    /// Check whether a path exists as a directory.
    fn dir_exists(&self, path: &Path) -> bool;

    /// The process working directory, falling back to `.` — the base relative
    /// skill sources resolve against.
    fn current_dir(&self) -> PathBuf;

    /// Searches upwards for `.imrule`, optionally falling back to global config.
    fn find_imrule_dir(&self, start_path: &Path, check_global: bool) -> Option<PathBuf>;

    /// Recursively reads markdown files from a `.imrule` directory.
    fn read_markdown_files(
        &self,
        imrule_dir: &Path,
        include_agents: bool,
    ) -> Result<Vec<(PathBuf, String)>, ImruleError>;

    /// Finds all `.imrule` directories below `start_path`, deepest first.
    fn find_all_imrule_dirs(&self, start_path: &Path) -> Vec<PathBuf>;

    /// Discovers the project's skills (`.imrule/skills`, falling back to
    /// `.ruler/skills`), each named as `apply` publishes it.
    fn discover_skills(&self, project_root: &Path) -> Result<SkillsDiscovery, ImruleError>;

    /// Walks a fetched skill source at any depth, keeping each skill's own
    /// directory name.
    fn walk_skills_tree(&self, root: &Path) -> Result<SkillsDiscovery, ImruleError>;

    /// Recursively copies a directory into `to`, creating it as needed.
    fn copy_dir(&self, from: &Path, to: &Path) -> Result<(), ImruleError>;

    /// Whether two directory trees hold byte-identical files.
    fn dirs_match(&self, left: &Path, right: &Path) -> Result<bool, ImruleError>;

    /// Discovers subagent definitions (`.imrule/agents`, falling back to
    /// `.ruler/agents`).
    fn discover_subagents(&self, project_root: &Path) -> Result<SubagentsDiscovery, ImruleError>;
}

/// Updates ignore files with generated paths.
pub trait GitignorePort: Send + Sync {
    /// Updates an ignore file with an ImRule-managed block.
    fn update_gitignore(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
        ignore_file: &str,
    ) -> Result<(), ImruleError>;
}

/// Removes generated files from the git index while keeping them on disk.
pub trait GitTrackingPort: Send + Sync {
    /// Untracks the given generated paths from git (`git rm --cached`).
    /// Returns the file paths actually removed from the index.
    /// Returns an empty vec when git is unavailable or the project is not
    /// inside a git work tree.
    fn untrack_generated_files(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
    ) -> Result<Vec<PathBuf>, ImruleError>;
}

/// Reads and atomically records the project-scoped MCP package version cache.
pub trait CachePort: Send + Sync {
    /// Reads `.imrule/cache.json`, returning `None` when it does not exist.
    fn read_mcp_remote_version(
        &self,
        project_root: &Path,
    ) -> Result<Option<McpRemoteVersionCache>, ImruleError>;

    /// Atomically replaces `.imrule/cache.json` with the supplied
    /// closed-schema value.
    fn write_mcp_remote_version_atomic(
        &self,
        project_root: &Path,
        cache: &McpRemoteVersionCache,
    ) -> Result<(), ImruleError>;
}

/// Reads and records what the previous `apply` generated.
pub trait ManifestPort: Send + Sync {
    /// Reads `.imrule/manifest.json`. Returns `None` when it does not exist or
    /// cannot be understood — a corrupt or future-version manifest must degrade
    /// to "nothing known" rather than abort an otherwise valid apply.
    fn read_manifest(&self, project_root: &Path) -> Result<Option<ApplyManifest>, ImruleError>;

    /// Atomically replaces `.imrule/manifest.json`.
    fn write_manifest(
        &self,
        project_root: &Path,
        manifest: &ApplyManifest,
    ) -> Result<(), ImruleError>;

    /// Deletes `.imrule/manifest.json`; a no-op when it is already gone.
    fn remove_manifest(&self, project_root: &Path) -> Result<(), ImruleError>;
}

/// Reads and writes MCP configuration files.
pub trait McpPort: Send + Sync {
    /// Reads `.imrule/mcp.json` when present.
    fn read_imrule_mcp_config(&self, project_root: &Path) -> Result<Option<Value>, ImruleError>;

    /// Reads a native JSON MCP config, returning `{}` when missing or empty.
    /// A non-empty file that fails to parse is an error, not `{}` — callers must
    /// not overwrite a config they could not understand.
    fn read_native_mcp(&self, path: &Path) -> Result<Value, ImruleError>;

    /// Writes native JSON MCP config, normalizing servers to the target agent's
    /// native schema (e.g. `url` -> `httpUrl`). Use for apply.
    fn write_native_mcp(&self, path: &Path, data: &Value) -> Result<(), ImruleError>;

    /// Writes native JSON MCP config verbatim, WITHOUT schema normalization.
    /// Use for clear, which must only remove imrule-managed keys and never
    /// reshape the user's own remaining servers.
    fn write_native_mcp_raw(&self, path: &Path, data: &Value) -> Result<(), ImruleError>;

    /// Determines the native MCP config path for a given adapter display name.
    fn get_native_mcp_path(&self, adapter_name: &str, project_root: &Path) -> Option<PathBuf>;
}

/// Writes generated rule files for individual agents.
pub trait AgentWriterPort: Send + Sync {
    /// Writes rules for a single agent, returning the path when written.
    fn write_agent_rules(
        &self,
        agent: &AgentDefinition,
        rules: &str,
        project_root: &Path,
        agent_config: Option<&AgentConfig>,
        backup: bool,
        dry_run: bool,
    ) -> Result<Option<PathBuf>, ImruleError>;
}

/// Fetches skill sources to a local directory.
pub trait SkillFetcherPort: Send + Sync {
    /// Fetches a remote skill source to a temporary directory.
    /// Returns the path to the directory containing skill files.
    fn fetch_to_temp(&self, source: &RemoteSkillSource) -> Result<PathBuf, ImruleError>;
}
