//! Use case for `imrule skills update`.
//!
//! Re-fetches every source recorded in `[skills.sources]` and refreshes the
//! installed copy from it. The registry is what makes this possible: without
//! it an installed skill is just a directory with no memory of the repository
//! it came from.

use std::path::PathBuf;

use crate::application::ports::{ConfigPort, FileSystemPort, SkillFetcherPort};
use crate::application::skills_add_use_case::{
    discover_remote_skills, effective_config_root, resolve_skills_base,
};
use crate::domain::config::SkillInfo;
use crate::domain::error::ImruleError;
use crate::domain::skills::{
    SkillUpdateOutcome, SkillUpdateStatus, group_skill_sources, parse_skill_source,
};
use crate::infrastructure::skills::{copy_skills_directory, skill_trees_match};

/// Runtime options for `imrule skills update`.
#[derive(Debug, Clone)]
pub struct SkillsUpdateOptions {
    pub project_root: PathBuf,
    /// Skills to update; `None` updates every recorded skill.
    pub skill_names: Option<Vec<String>>,
    pub global: bool,
    pub dry_run: bool,
}

/// Result of an update run.
#[derive(Debug, Clone, Default)]
pub struct SkillsUpdateResult {
    pub outcomes: Vec<SkillUpdateOutcome>,
    /// Directory the refreshed skills live in — the same one `skills add`
    /// installed them into.
    pub install_dir: PathBuf,
}

impl SkillsUpdateResult {
    /// Whether any skill on disk was (or would be) rewritten.
    pub fn changed(&self) -> bool {
        self.outcomes.iter().any(|outcome| {
            matches!(
                outcome.status,
                SkillUpdateStatus::Updated | SkillUpdateStatus::Reinstalled
            )
        })
    }

    /// Whether any source failed to fetch.
    pub fn has_failures(&self) -> bool {
        self.outcomes
            .iter()
            .any(|outcome| outcome.status == SkillUpdateStatus::Failed)
    }
}

/// Skills update use case.
pub struct SkillsUpdateUseCase<'a> {
    fetcher: &'a dyn SkillFetcherPort,
    fs_port: &'a dyn FileSystemPort,
    config_port: &'a dyn ConfigPort,
}

impl<'a> SkillsUpdateUseCase<'a> {
    pub fn new(
        fetcher: &'a dyn SkillFetcherPort,
        fs_port: &'a dyn FileSystemPort,
        config_port: &'a dyn ConfigPort,
    ) -> Self {
        Self {
            fetcher,
            fs_port,
            config_port,
        }
    }

    pub fn execute(&self, options: SkillsUpdateOptions) -> Result<SkillsUpdateResult, ImruleError> {
        let config_root = effective_config_root(&options.project_root, options.global);
        let config = self.config_port.load_config(&config_root, None, None)?;
        let recorded = config
            .skills
            .as_ref()
            .map(|skills| skills.sources.clone())
            .unwrap_or_default();

        let groups = group_skill_sources(&recorded, options.skill_names.as_deref())?;
        let skills_base = resolve_skills_base(self.fs_port, &options.project_root, options.global);

        let mut result = SkillsUpdateResult {
            install_dir: skills_base.clone(),
            ..Default::default()
        };
        for group in groups {
            // One fetch per source, however many skills came from it.
            let fetched = self.fetch_group(&group.source);
            let available = match fetched {
                Ok(available) => available,
                Err(error) => {
                    // A single unreachable source must not block the rest.
                    for name in group.skills {
                        result.outcomes.push(SkillUpdateOutcome {
                            name,
                            source: group.source.clone(),
                            status: SkillUpdateStatus::Failed,
                            detail: Some(error.to_string()),
                        });
                    }
                    continue;
                }
            };

            for name in group.skills {
                let status = self.refresh_skill(&options, &skills_base, &available, &name)?;
                result.outcomes.push(SkillUpdateOutcome {
                    name,
                    source: group.source.clone(),
                    status,
                    detail: None,
                });
            }
        }

        Ok(result)
    }

    fn fetch_group(&self, source: &str) -> Result<Vec<SkillInfo>, ImruleError> {
        let parsed = parse_skill_source(source)?;
        let fetched_path = self.fetcher.fetch_to_temp(&parsed)?;
        if !fetched_path.exists() {
            return Err(ImruleError::skills(format!(
                "fetched source path does not exist: {}",
                fetched_path.display()
            )));
        }
        discover_remote_skills(&fetched_path)
    }

    fn refresh_skill(
        &self,
        options: &SkillsUpdateOptions,
        skills_base: &std::path::Path,
        available: &[SkillInfo],
        name: &str,
    ) -> Result<SkillUpdateStatus, ImruleError> {
        let Some(fetched) = available.iter().find(|skill| skill.name == name) else {
            return Ok(SkillUpdateStatus::MissingInSource);
        };

        let dest = skills_base.join(name);
        if !dest.exists() {
            if !options.dry_run {
                copy_skills_directory(&fetched.path, &dest).map_err(|e| {
                    ImruleError::skills(format!("failed to install skill '{name}': {e}"))
                })?;
            }
            return Ok(SkillUpdateStatus::Reinstalled);
        }

        let identical = skill_trees_match(&fetched.path, &dest)
            .map_err(|e| ImruleError::skills(format!("failed to compare skill '{name}': {e}")))?;
        if identical {
            return Ok(SkillUpdateStatus::Unchanged);
        }

        if !options.dry_run {
            // Replace rather than overlay, so files dropped upstream disappear.
            self.fs_port.remove_dir_all(&dest)?;
            copy_skills_directory(&fetched.path, &dest).map_err(|e| {
                ImruleError::skills(format!("failed to update skill '{name}': {e}"))
            })?;
        }
        Ok(SkillUpdateStatus::Updated)
    }
}
