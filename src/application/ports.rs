//! Port traits that abstract infrastructure concerns from application use cases.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::agent::AgentDefinition;
use crate::domain::config::{AgentConfig, LoadedConfig};
use crate::domain::error::ImruleError;

/// Loads and parses ImRule configuration.
pub trait ConfigPort {
    /// Loads local/global ImRule TOML configuration.
    fn load_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        cli_agents: Option<Vec<String>>,
    ) -> Result<LoadedConfig, ImruleError>;
}

/// Abstracts filesystem operations so use cases remain testable.
pub trait FileSystemPort {
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

    /// Copy a file.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), ImruleError>;

    /// Check whether a path exists as a file.
    fn file_exists(&self, path: &Path) -> bool;

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
}

/// Updates ignore files with generated paths.
pub trait GitignorePort {
    /// Updates an ignore file with an ImRule-managed block.
    fn update_gitignore(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
        ignore_file: &str,
    ) -> Result<(), ImruleError>;
}

/// Reads and writes MCP configuration files.
pub trait McpPort {
    /// Reads `.imrule/mcp.json` when present.
    fn read_imrule_mcp_config(&self, project_root: &Path) -> Result<Option<Value>, ImruleError>;

    /// Reads a native JSON MCP config, returning `{}` when missing or invalid.
    fn read_native_mcp(&self, path: &Path) -> Result<Value, ImruleError>;

    /// Writes native JSON MCP config.
    fn write_native_mcp(&self, path: &Path, data: &Value) -> Result<(), ImruleError>;

    /// Determines the native MCP config path for a given adapter display name.
    fn get_native_mcp_path(&self, adapter_name: &str, project_root: &Path) -> Option<PathBuf>;
}

/// Writes generated rule files for individual agents.
pub trait AgentWriterPort {
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
