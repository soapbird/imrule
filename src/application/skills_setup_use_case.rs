//! Use case for `imrule skills setup`: installs ImRule's built-in skills.

use std::path::{Path, PathBuf};

use crate::application::ports::FileSystemPort;
use crate::application::skills_add_use_case::resolve_skills_base;
use crate::domain::builtin_skills::{
    BuiltinSkill, BuiltinSkillSetupStatus, BuiltinSkillState, ProjectSignals, detection_labels,
    recommend_builtin_skills, resolve_builtin_skills,
};
use crate::domain::error::ImruleError;

/// Runtime options for `imrule skills setup`.
#[derive(Debug, Clone)]
pub struct SkillsSetupOptions {
    pub project_root: PathBuf,
    pub global: bool,
}

/// One built-in skill as it relates to the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsSetupEntry<'a> {
    pub skill: &'a BuiltinSkill,
    /// Detection says the skill fits this project.
    pub recommended: bool,
    pub state: BuiltinSkillState,
}

/// Everything needed to choose skills: the catalog against the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsSetupPlan<'a> {
    pub install_dir: PathBuf,
    /// What detection found (`rust`, `cli`, `docker`, …).
    pub detected: Vec<&'static str>,
    pub entries: Vec<SkillsSetupEntry<'a>>,
}

impl SkillsSetupPlan<'_> {
    /// Paths of the recommended skills, in catalog order.
    pub fn recommended_paths(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.recommended)
            .map(|entry| entry.skill.path.clone())
            .collect()
    }

    /// Paths of every skill, in catalog order.
    pub fn all_paths(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.skill.path.clone())
            .collect()
    }
}

/// What happened to one selected skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsSetupOutcome {
    pub path: String,
    pub name: String,
    pub status: BuiltinSkillSetupStatus,
}

/// Result of an install run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsSetupResult {
    pub outcomes: Vec<SkillsSetupOutcome>,
}

impl SkillsSetupResult {
    /// Whether any skill was written, so agents need a sync.
    pub fn changed(&self) -> bool {
        self.outcomes.iter().any(|outcome| outcome.status.writes())
    }
}

/// Skills setup use case.
pub struct SkillsSetupUseCase<'a> {
    fs_port: &'a dyn FileSystemPort,
    catalog: &'a [BuiltinSkill],
}

impl<'a> SkillsSetupUseCase<'a> {
    pub fn new(fs_port: &'a dyn FileSystemPort, catalog: &'a [BuiltinSkill]) -> Self {
        Self { fs_port, catalog }
    }

    /// Compares the catalog with the project: which skills fit, and which are
    /// already installed. `signals` come from the project on disk.
    pub fn plan(
        &self,
        options: &SkillsSetupOptions,
        signals: &ProjectSignals,
    ) -> SkillsSetupPlan<'a> {
        let install_dir = resolve_skills_base(self.fs_port, &options.project_root, options.global);
        let recommended = recommend_builtin_skills(signals);
        let entries = self
            .catalog
            .iter()
            .map(|skill| SkillsSetupEntry {
                skill,
                recommended: recommended.contains(skill.path.as_str()),
                state: self.state_of(&install_dir, skill),
            })
            .collect();
        SkillsSetupPlan {
            install_dir,
            detected: detection_labels(signals),
            entries,
        }
    }

    /// Resolves user-typed names (path or published name) to catalog paths.
    pub fn resolve(&self, requested: &[String]) -> Result<Vec<String>, ImruleError> {
        resolve_builtin_skills(self.catalog, requested)
    }

    /// Installs the skills at `paths`. A skill modified locally is only
    /// overwritten when `overwrite_modified` is set; everything else that
    /// differs is refreshed. With `dry_run`, reports without writing.
    pub fn install(
        &self,
        plan: &SkillsSetupPlan<'_>,
        paths: &[String],
        overwrite_modified: bool,
        dry_run: bool,
    ) -> Result<SkillsSetupResult, ImruleError> {
        let mut outcomes = Vec::new();
        for path in paths {
            let Some(entry) = plan.entries.iter().find(|entry| &entry.skill.path == path) else {
                return Err(ImruleError::skills(format!(
                    "unknown built-in skill: {path}"
                )));
            };
            let skill = entry.skill;
            let status = match entry.state {
                BuiltinSkillState::NotInstalled => BuiltinSkillSetupStatus::Installed,
                BuiltinSkillState::UpToDate => BuiltinSkillSetupStatus::Unchanged,
                BuiltinSkillState::Outdated => BuiltinSkillSetupStatus::Updated,
                BuiltinSkillState::Modified if overwrite_modified => {
                    BuiltinSkillSetupStatus::Updated
                }
                BuiltinSkillState::Modified => BuiltinSkillSetupStatus::SkippedModified,
            };
            if status.writes() && !dry_run {
                self.write_skill(&plan.install_dir, skill)?;
            }
            outcomes.push(SkillsSetupOutcome {
                path: skill.path.clone(),
                name: skill.name.clone(),
                status,
            });
        }
        Ok(SkillsSetupResult { outcomes })
    }

    fn state_of(&self, install_dir: &Path, skill: &BuiltinSkill) -> BuiltinSkillState {
        let skill_dir = install_dir.join(&skill.path);
        BuiltinSkillState::compare(skill, |relative| {
            self.fs_port.read_text(&skill_dir.join(relative)).ok()
        })
    }

    /// Replaces the skill directory with the embedded files, so a file dropped
    /// from a newer revision does not linger.
    fn write_skill(&self, install_dir: &Path, skill: &BuiltinSkill) -> Result<(), ImruleError> {
        let skill_dir = install_dir.join(&skill.path);
        if self.fs_port.dir_exists(&skill_dir) {
            self.fs_port.remove_dir_all(&skill_dir)?;
        }
        for (relative, content) in &skill.files {
            self.fs_port
                .write_text(&skill_dir.join(relative), content)?;
        }
        Ok(())
    }
}
