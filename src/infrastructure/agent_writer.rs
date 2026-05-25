//! Concrete agent file writer implementing `AgentWriterPort`.

use std::path::{Path, PathBuf};

use crate::application::apply_use_case::instruction_output_path;
use crate::application::ports::{AgentWriterPort, FileSystemPort};
use crate::domain::agent::AgentDefinition;
use crate::domain::config::AgentConfig;
use crate::domain::constants::GENERATED_BY_IMRULE_MARKER;
use crate::domain::error::ImruleError;

pub struct DefaultAgentWriter<'a> {
    fs: &'a dyn FileSystemPort,
}

impl<'a> DefaultAgentWriter<'a> {
    pub fn new(fs: &'a dyn FileSystemPort) -> Self {
        Self { fs }
    }
}

impl AgentWriterPort for DefaultAgentWriter<'_> {
    fn write_agent_rules(
        &self,
        agent: &AgentDefinition,
        rules: &str,
        project_root: &Path,
        agent_config: Option<&AgentConfig>,
        backup: bool,
        dry_run: bool,
    ) -> Result<Option<PathBuf>, ImruleError> {
        let Some(path) = instruction_output_path(project_root, agent, agent_config) else {
            return Ok(None);
        };
        if dry_run {
            return Ok(Some(path));
        }

        if let Some(parent) = path.parent() {
            self.fs.ensure_dir_exists(parent)?;
        }

        let content = format!("{GENERATED_BY_IMRULE_MARKER}\n{rules}");
        if self.fs.read_text(&path).ok().as_deref() == Some(content.as_str()) {
            return Ok(Some(path));
        }

        if backup {
            self.fs.backup_file(&path)?;
        }
        self.fs.write_text(&path, &content)?;
        Ok(Some(path))
    }
}
