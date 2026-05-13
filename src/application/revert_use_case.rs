//! Native revert engine use case.

use std::path::{Path, PathBuf};

use crate::application::apply_use_case::{get_agent_output_paths, resolve_selected_agents};
use crate::application::ports::{ConfigPort, FileSystemPort, GitignorePort};
use crate::domain::error::ImruleError;

/// Runtime options for `imrule revert`.
#[derive(Debug, Clone)]
pub struct RevertOptions {
    pub project_root: PathBuf,
    pub agents: Option<Vec<String>>,
    pub config: Option<PathBuf>,
    pub keep_backups: bool,
    pub dry_run: bool,
    pub local_only: bool,
}

/// Revert use case.
pub struct RevertUseCase<'a> {
    config_port: &'a dyn ConfigPort,
    fs_port: &'a dyn FileSystemPort,
    gitignore_port: &'a dyn GitignorePort,
}

impl<'a> RevertUseCase<'a> {
    pub fn new(
        config_port: &'a dyn ConfigPort,
        fs_port: &'a dyn FileSystemPort,
        gitignore_port: &'a dyn GitignorePort,
    ) -> Self {
        Self {
            config_port,
            fs_port,
            gitignore_port,
        }
    }

    /// Reverts generated files for selected agents.
    pub fn execute(&self, options: RevertOptions) -> Result<Vec<PathBuf>, ImruleError> {
        let config = self.config_port.load_config(
            &options.project_root,
            options.config.as_deref(),
            options.agents.clone(),
        )?;
        let selected_agents = resolve_selected_agents(&config, options.agents.as_deref())?;
        let paths = get_agent_output_paths(&options.project_root, &selected_agents);
        let mut changed = Vec::new();

        for path in &paths {
            let restored = self.restore_from_backup(path, options.dry_run)?;
            let removed = if restored {
                false
            } else {
                self.remove_generated_file(path, options.dry_run)?
            };
            if !options.keep_backups {
                self.remove_backup_file(path, options.dry_run)?;
            }
            if restored || removed {
                changed.push(path.clone());
            }
        }

        if !options.dry_run {
            self.gitignore_port
                .update_gitignore(&options.project_root, &[], ".gitignore")?;
        }

        Ok(changed)
    }

    fn backup_path(&self, file_path: &Path) -> PathBuf {
        let mut backup = file_path.as_os_str().to_os_string();
        backup.push(".bak");
        PathBuf::from(backup)
    }

    fn restore_from_backup(&self, file_path: &Path, dry_run: bool) -> Result<bool, ImruleError> {
        let backup = self.backup_path(file_path);
        if !self.fs_port.file_exists(&backup) {
            return Ok(false);
        }
        if !dry_run {
            self.fs_port.copy_file(&backup, file_path)?;
        }
        Ok(true)
    }

    fn remove_generated_file(&self, file_path: &Path, dry_run: bool) -> Result<bool, ImruleError> {
        if !self.fs_port.file_exists(file_path)
            || self.fs_port.file_exists(&self.backup_path(file_path))
        {
            return Ok(false);
        }
        if !dry_run {
            self.fs_port.remove_file(file_path)?;
        }
        Ok(true)
    }

    fn remove_backup_file(&self, file_path: &Path, dry_run: bool) -> Result<bool, ImruleError> {
        let backup = self.backup_path(file_path);
        if !self.fs_port.file_exists(&backup) {
            return Ok(false);
        }
        if !dry_run {
            self.fs_port.remove_file(&backup)?;
        }
        Ok(true)
    }
}
