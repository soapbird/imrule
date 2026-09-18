//! Use case for `imrule skills setup`: installs ImRule's built-in skills.

use std::path::{Path, PathBuf};

use crate::application::ports::FileSystemPort;
use crate::application::skills_add_use_case::resolve_skills_base;
use crate::domain::builtin_skills::{
    BuiltinSkill, BuiltinSkillSetupStatus, BuiltinSkillState, ProjectSignals, detection_labels,
    is_builtin_skill_md, previous_builtin_paths, recommend_builtin_skills, resolve_builtin_skills,
};
use crate::domain::constants::SKILL_MD_FILENAME;
use crate::domain::constants::{LEGACY_DIR_NAME, SKILLS_DIR, normalize_path_separators};
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
    /// A copy ImRule installed under the path the skill had before it was
    /// renamed (`python/cli` for `cli-python`), still on disk.
    pub previous: Option<PreviousInstall>,
}

impl SkillsSetupEntry<'_> {
    /// Whether some copy of the skill is installed, under its current path or
    /// a previous one.
    pub fn installed(&self) -> bool {
        self.state != BuiltinSkillState::NotInstalled || self.previous.is_some()
    }
}

/// A built-in skill's copy under a path it no longer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousInstall {
    /// The old path below the install directory (`python/cli`).
    pub path: String,
    /// How that copy compares to the current embedded skill: `Outdated` when
    /// it may be moved without asking, `Modified` when only with consent.
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

    /// Paths of the skills installed under their current or a previous path,
    /// in catalog order: what `setup --update` refreshes.
    pub fn installed_paths(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.installed())
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
    /// The previous path a copy was moved from, when one was.
    pub moved_from: Option<String>,
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

/// The project a skills install directory belongs to: the root detection
/// reads and `apply` syncs.
///
/// When `install_dir` is `<root>/.imrule/skills` (or the legacy
/// `.ruler/skills`) and `<root>` is `requested_root` or one of its ancestors,
/// that is `<root>` — so running from `repo/src` sets up `repo`. Otherwise
/// (`--global`, or the global fallback when no `.imrule/` is found) it is
/// `requested_root`.
pub fn skills_project_root(install_dir: &Path, requested_root: &Path, global: bool) -> PathBuf {
    if global || install_dir.file_name().and_then(|name| name.to_str()) != Some(SKILLS_DIR) {
        return requested_root.to_path_buf();
    }
    let owner = install_dir
        .parent()
        .filter(|imrule_dir| {
            imrule_dir
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".imrule" || name == LEGACY_DIR_NAME)
        })
        .and_then(Path::parent)
        .filter(|root| requested_root.starts_with(root));
    match owner {
        // `.imrule/` found beside a relative root's first component.
        Some(root) if root.as_os_str().is_empty() => PathBuf::from("."),
        Some(root) => root.to_path_buf(),
        None => requested_root.to_path_buf(),
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

    /// The directory skills are installed into: the nearest `.imrule/skills`
    /// at or above the project root, or the global one.
    pub fn install_dir(&self, options: &SkillsSetupOptions) -> PathBuf {
        resolve_skills_base(self.fs_port, &options.project_root, options.global)
    }

    /// Compares the catalog with the project: which skills fit, and which are
    /// already installed. `signals` come from the project on disk.
    pub fn plan(
        &self,
        options: &SkillsSetupOptions,
        signals: &ProjectSignals,
    ) -> SkillsSetupPlan<'a> {
        self.plan_in(self.install_dir(options), signals)
    }

    /// [`plan`](Self::plan) for an install directory already resolved with
    /// [`install_dir`](Self::install_dir).
    pub fn plan_in(&self, install_dir: PathBuf, signals: &ProjectSignals) -> SkillsSetupPlan<'a> {
        let recommended = recommend_builtin_skills(signals);
        let entries = self
            .catalog
            .iter()
            .map(|skill| SkillsSetupEntry {
                skill,
                recommended: recommended.contains(skill.path.as_str()),
                state: self.state_of(&install_dir, skill),
                previous: self.previous_install(&install_dir, skill),
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
    /// overwritten when `overwrite_modified` is set (`--force`); everything
    /// else that differs is refreshed. With `dry_run`, reports without writing.
    pub fn install(
        &self,
        plan: &SkillsSetupPlan<'_>,
        paths: &[String],
        overwrite_modified: bool,
        dry_run: bool,
    ) -> Result<SkillsSetupResult, ImruleError> {
        let consented: &[String] = if overwrite_modified { paths } else { &[] };
        self.install_with_consent(plan, paths, consented, dry_run)
    }

    /// Installs the skills at `paths`. A skill modified locally is only
    /// overwritten when its path is in `consented`, and reported as skipped
    /// otherwise; everything else that differs is refreshed. With `dry_run`,
    /// reports without writing.
    pub fn install_with_consent(
        &self,
        plan: &SkillsSetupPlan<'_>,
        paths: &[String],
        consented: &[String],
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
            let consent = consented.contains(path);
            let mut status = match entry.state {
                BuiltinSkillState::NotInstalled => BuiltinSkillSetupStatus::Installed,
                BuiltinSkillState::UpToDate => BuiltinSkillSetupStatus::Unchanged,
                BuiltinSkillState::Outdated => BuiltinSkillSetupStatus::Updated,
                BuiltinSkillState::Modified if consent => BuiltinSkillSetupStatus::Updated,
                BuiltinSkillState::Modified => BuiltinSkillSetupStatus::SkippedModified,
            };
            // A copy under the skill's old path moves to the new one: without
            // asking when ImRule's own older copy, otherwise only with consent.
            // Left in place, both would be published to agents side by side.
            let movable = entry
                .previous
                .as_ref()
                .filter(|previous| previous.state == BuiltinSkillState::Outdated || consent);
            match (&entry.previous, movable, entry.state) {
                (Some(_), None, BuiltinSkillState::NotInstalled) => {
                    status = BuiltinSkillSetupStatus::SkippedModified;
                }
                (Some(_), Some(_), BuiltinSkillState::NotInstalled) => {
                    status = BuiltinSkillSetupStatus::Updated;
                }
                (Some(_), Some(_), _) if status == BuiltinSkillSetupStatus::Unchanged => {
                    status = BuiltinSkillSetupStatus::Updated;
                }
                _ => {}
            }
            let moved_from = movable
                .filter(|_| status.writes())
                .map(|previous| previous.path.clone());
            if status.writes() && !dry_run {
                if entry.state != BuiltinSkillState::UpToDate {
                    self.write_skill(&plan.install_dir, skill, entry.state, consent)?;
                }
                if let Some(previous) = movable {
                    self.remove_previous(&plan.install_dir, skill, previous)?;
                }
            }
            outcomes.push(SkillsSetupOutcome {
                path: skill.path.clone(),
                name: skill.name.clone(),
                status,
                moved_from,
            });
        }
        Ok(SkillsSetupResult { outcomes })
    }

    fn state_of(&self, install_dir: &Path, skill: &BuiltinSkill) -> BuiltinSkillState {
        self.state_at(&install_dir.join(&skill.path), skill)
    }

    /// The first copy ImRule installed under one of the skill's previous
    /// paths. A directory there without the built-in marker is the user's own
    /// skill that happens to share an old name, and is none of setup's business.
    fn previous_install(
        &self,
        install_dir: &Path,
        skill: &BuiltinSkill,
    ) -> Option<PreviousInstall> {
        previous_builtin_paths(&skill.path).find_map(|path| {
            let dir = install_dir.join(path);
            let skill_md = self.fs_port.read_text(&dir.join(SKILL_MD_FILENAME)).ok()?;
            if !self.fs_port.dir_exists(&dir) || !is_builtin_skill_md(&skill_md) {
                return None;
            }
            Some(PreviousInstall {
                path: path.to_string(),
                state: self.state_at(&dir, skill),
            })
        })
    }

    /// How the copy in `skill_dir` compares to the embedded skill.
    fn state_at(&self, skill_dir: &Path, skill: &BuiltinSkill) -> BuiltinSkillState {
        let skill_dir = skill_dir.to_path_buf();
        let installed_files: Vec<String> = if self.fs_port.dir_exists(&skill_dir) {
            match self.fs_port.list_files(&skill_dir) {
                Ok(files) => files
                    .iter()
                    .map(|file| normalize_path_separators(&file.to_string_lossy()))
                    .collect(),
                // What cannot be listed cannot be shown to be ImRule's alone.
                Err(_) => return BuiltinSkillState::Modified,
            }
        } else if self.fs_port.file_exists(&skill_dir) {
            // Something other than a directory occupies the path.
            Vec::new()
        } else {
            return BuiltinSkillState::compare(skill, None, |_| None);
        };
        BuiltinSkillState::compare(skill, Some(&installed_files), |relative| {
            self.fs_port.read_text(&skill_dir.join(relative)).ok()
        })
    }

    /// Removes a skill's copy under its previous path once the current path
    /// holds it, then the grouping directories that emptied (`python/`).
    /// Re-checks the copy first, as [`write_skill`](Self::write_skill) does.
    fn remove_previous(
        &self,
        install_dir: &Path,
        skill: &BuiltinSkill,
        planned: &PreviousInstall,
    ) -> Result<(), ImruleError> {
        let dir = install_dir.join(&planned.path);
        let current = self.previous_install(install_dir, skill);
        if current.as_ref() != Some(planned) {
            return Err(ImruleError::skills(format!(
                "{} changed while setting up; it was left in place, \
                 run `imrule skills setup` again",
                dir.display()
            )));
        }
        if !self.fs_port.resolves_within(&dir, install_dir) {
            return Err(ImruleError::skills(format!(
                "refusing to remove {}: it resolves outside {}",
                dir.display(),
                install_dir.display()
            )));
        }
        self.fs_port.remove_dir_all(&dir)?;
        let mut parent = dir.parent();
        while let Some(group) = parent.filter(|group| *group != install_dir) {
            if !self.fs_port.remove_dir_if_empty(group)? {
                break;
            }
            parent = group.parent();
        }
        Ok(())
    }

    /// Replaces the skill directory with the embedded files, so a file dropped
    /// from a newer revision does not linger.
    ///
    /// The only place a skill directory is removed, so it re-checks what is on
    /// disk: it must still be in the `planned` state, and that state must be
    /// one that may be replaced — nothing there yet, an older revision ImRule
    /// installed, or a local modification the user `consent`ed to overwrite.
    /// Anything else is refused rather than deleted.
    fn write_skill(
        &self,
        install_dir: &Path,
        skill: &BuiltinSkill,
        planned: BuiltinSkillState,
        consent: bool,
    ) -> Result<(), ImruleError> {
        let skill_dir = install_dir.join(&skill.path);
        let current = self.state_of(install_dir, skill);
        if current != planned {
            return Err(ImruleError::skills(format!(
                "{} changed while setting up (was {}, now {}); nothing was replaced, \
                 run `imrule skills setup` again",
                skill_dir.display(),
                planned.label(),
                current.label()
            )));
        }
        let replaceable = match current {
            BuiltinSkillState::NotInstalled | BuiltinSkillState::Outdated => true,
            BuiltinSkillState::Modified => consent,
            BuiltinSkillState::UpToDate => false,
        };
        if !replaceable {
            return Err(ImruleError::skills(format!(
                "refusing to replace {} ({}) without consent to overwrite it",
                skill_dir.display(),
                current.label()
            )));
        }
        if current != BuiltinSkillState::NotInstalled {
            // A grouping directory linked elsewhere (`.imrule/skills/docker ->
            // ~/somewhere`) would make this delete files outside the install dir.
            if !self.fs_port.resolves_within(&skill_dir, install_dir) {
                return Err(ImruleError::skills(format!(
                    "refusing to replace {}: it resolves outside {}",
                    skill_dir.display(),
                    install_dir.display()
                )));
            }
            self.fs_port.remove_dir_all(&skill_dir)?;
        }
        for (relative, content) in &skill.files {
            self.fs_port
                .write_text(&skill_dir.join(relative), content)?;
        }
        Ok(())
    }
}
