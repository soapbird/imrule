//! Port traits that abstract infrastructure concerns from application use cases.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::agent::AgentDefinition;
use crate::domain::config::{AgentConfig, LoadedConfig};
use crate::domain::error::RulerError;

/// Loads and parses Ruler configuration.
pub trait ConfigPort {
    /// Loads local/global Ruler TOML configuration.
    fn load_config(
        &self,
        project_root: &Path,
        config_path: Option<&Path>,
        cli_agents: Option<Vec<String>>,
    ) -> Result<LoadedConfig, RulerError>;
}

/// Abstracts filesystem operations so use cases remain testable.
pub trait FileSystemPort {
    /// Read a text file.
    fn read_text(&self, path: &Path) -> Result<String, RulerError>;

    /// Write text to a file, creating parent directories.
    fn write_text(&self, path: &Path, content: &str) -> Result<(), RulerError>;

    /// Backup an existing file to `<file>.bak`.
    fn backup_file(&self, path: &Path) -> Result<(), RulerError>;

    /// Ensure a directory exists.
    fn ensure_dir_exists(&self, path: &Path) -> Result<(), RulerError>;

    /// Remove a file.
    fn remove_file(&self, path: &Path) -> Result<(), RulerError>;

    /// Copy a file.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), RulerError>;

    /// Check whether a path exists as a file.
    fn file_exists(&self, path: &Path) -> bool;

    /// Searches upwards for `.ruler`, optionally falling back to global config.
    fn find_ruler_dir(&self, start_path: &Path, check_global: bool) -> Option<PathBuf>;

    /// Recursively reads markdown files from a `.ruler` directory.
    fn read_markdown_files(
        &self,
        ruler_dir: &Path,
        include_agents: bool,
    ) -> Result<Vec<(PathBuf, String)>, RulerError>;

    /// Finds all `.ruler` directories below `start_path`, deepest first.
    fn find_all_ruler_dirs(&self, start_path: &Path) -> Vec<PathBuf>;
}

/// Updates ignore files with generated paths.
pub trait GitignorePort {
    /// Updates an ignore file with a Ruler-managed block.
    fn update_gitignore(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
        ignore_file: &str,
    ) -> Result<(), RulerError>;
}

/// Reads and writes MCP configuration files.
pub trait McpPort {
    /// Reads `.ruler/mcp.json` when present.
    fn read_ruler_mcp_config(&self, project_root: &Path) -> Result<Option<Value>, RulerError>;

    /// Reads a native JSON MCP config, returning `{}` when missing or invalid.
    fn read_native_mcp(&self, path: &Path) -> Result<Value, RulerError>;

    /// Writes native JSON MCP config.
    fn write_native_mcp(&self, path: &Path, data: &Value) -> Result<(), RulerError>;

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
    ) -> Result<Option<PathBuf>, RulerError>;
}
