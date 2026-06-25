//! Init use case for creating ImRule configuration scaffolds.

use std::path::PathBuf;

use crate::application::ports::FileSystemPort;
use crate::domain::constants::xdg_config_home;
use crate::domain::error::ImruleError;

/// Runtime options for `imrule init`.
#[derive(Debug, Clone)]
pub struct InitOptions {
    pub project_root: Option<PathBuf>,
    pub global: bool,
}

/// Init use case.
pub struct InitUseCase<'a> {
    fs_port: &'a dyn FileSystemPort,
}

impl<'a> InitUseCase<'a> {
    pub fn new(fs_port: &'a dyn FileSystemPort) -> Self {
        Self { fs_port }
    }

    /// Creates `.imrule` or global ImRule config files without overwriting existing files.
    pub fn execute(&self, options: InitOptions) -> Result<PathBuf, ImruleError> {
        let root = if options.global {
            xdg_config_home().join("imrule")
        } else {
            options
                .project_root
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join(".imrule")
        };
        tracing::info!(root = %root.display(), global = options.global, "initializing imrule");

        self.fs_port.ensure_dir_exists(&root)?;
        self.write_if_missing(root.join("AGENTS.md"), DEFAULT_INSTRUCTIONS)?;
        self.write_if_missing(root.join("imrule.toml"), DEFAULT_TOML)?;
        Ok(root)
    }

    fn write_if_missing(&self, path: PathBuf, content: &str) -> Result<(), ImruleError> {
        if self.fs_port.file_exists(&path) {
            return Ok(());
        }
        self.fs_port.write_text(&path, content)
    }
}

const DEFAULT_INSTRUCTIONS: &str = "# AGENTS.md\n\nCentralised AI agent instructions. Add coding guidelines, style guides, and project context here.\n\nImRule concatenates all .md files in this directory (and subdirectories), starting with AGENTS.md (if present), then remaining files in sorted order.\n";

const DEFAULT_TOML: &str = r#"# ImRule Configuration File
# See https://github.com/soapbird/imrule for documentation.

# To specify which agents are active by default when --agents is not used,
# uncomment and populate the following line. If omitted, all agents are active.
# default_agents = ["copilot", "claude"]

# Enable nested rule loading from nested .imrule directories
# nested = false

# [gitignore]
# enabled = true
# local = false

# [agents.aider]
# enabled = true
# output_path_instructions = "AGENTS.md"
# output_path_config = ".aider.conf.yml"

# [mcp_servers.example_stdio]
# command = "your-mcp-server"
# args = ["--stdio"]
"#;
