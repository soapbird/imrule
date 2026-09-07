//! Use case for `imrule skills add <source>`.

use std::path::{Path, PathBuf};

use crate::application::ports::{ConfigPort, ConfigWritePort, FileSystemPort, SkillFetcherPort};
use crate::domain::config::SkillInfo;
use crate::domain::error::ImruleError;
use crate::domain::skills::{parse_skill_source, skill_source_key};
use crate::infrastructure::skills::{copy_skills_directory, walk_skills_tree};

/// Runtime options for `imrule skills add`.
#[derive(Debug, Clone)]
pub struct SkillsAddOptions {
    pub project_root: PathBuf,
    pub source: String,
    pub skill_names: Option<Vec<String>>,
    pub list_only: bool,
    pub global: bool,
}

/// Result of a skills add operation.
#[derive(Debug, Clone)]
pub struct SkillsAddResult {
    pub listed: Vec<SkillInfo>,
    pub installed: Vec<String>,
}

/// Skills add use case.
pub struct SkillsAddUseCase<'a> {
    fetcher: &'a dyn SkillFetcherPort,
    fs_port: &'a dyn FileSystemPort,
    config_port: &'a dyn ConfigPort,
    config_write_port: &'a dyn ConfigWritePort,
}

impl<'a> SkillsAddUseCase<'a> {
    pub fn new(
        fetcher: &'a dyn SkillFetcherPort,
        fs_port: &'a dyn FileSystemPort,
        config_port: &'a dyn ConfigPort,
        config_write_port: &'a dyn ConfigWritePort,
    ) -> Self {
        Self {
            fetcher,
            fs_port,
            config_port,
            config_write_port,
        }
    }

    pub fn execute(&self, options: SkillsAddOptions) -> Result<SkillsAddResult, ImruleError> {
        let source = parse_skill_source(&options.source)?;

        let fetched_path = self.fetcher.fetch_to_temp(&source)?;
        if !fetched_path.exists() {
            return Err(ImruleError::skills(format!(
                "fetched source path does not exist: {}",
                fetched_path.display()
            )));
        }

        // Discover skill directories that contain SKILL.md in the fetched source.
        // The source repo may have skills at root level, in skills/, or in agent-specific dirs.
        let discovery = discover_remote_skills(&fetched_path)?;

        if options.list_only {
            return Ok(SkillsAddResult {
                listed: discovery.clone(),
                installed: Vec::new(),
            });
        }

        // Filter to requested skill names if specified.
        let selected: Vec<&SkillInfo> = if let Some(names) = &options.skill_names {
            let wildcard = names.iter().any(|n| n == "*");
            if wildcard {
                discovery.iter().collect()
            } else {
                discovery
                    .iter()
                    .filter(|skill| names.iter().any(|n| n == &skill.name))
                    .collect()
            }
        } else {
            discovery.iter().collect()
        };

        if selected.is_empty() {
            return Err(ImruleError::skills(
                "no skills found in the specified source",
            ));
        }

        // Determine target directory.
        let skills_base = resolve_skills_base(self.fs_port, &options.project_root, options.global);

        self.fs_port
            .ensure_dir_exists(&skills_base)
            .map_err(|e| ImruleError::filesystem(format!("failed to create skills dir: {e}")))?;

        let mut installed = Vec::new();
        for skill in &selected {
            let dest = skills_base.join(&skill.name);
            copy_skills_directory(&skill.path, &dest).map_err(|e| {
                ImruleError::skills(format!("failed to copy skill '{}': {e}", skill.name))
            })?;
            installed.push(skill.name.clone());
        }

        self.record_sources(
            &options,
            &skill_source_key(&source, &options.source),
            &installed,
        )?;

        Ok(SkillsAddResult {
            listed: Vec::new(),
            installed,
        })
    }

    /// Records where each installed skill came from, so `imrule skills update`
    /// can fetch the same source again later.
    fn record_sources(
        &self,
        options: &SkillsAddOptions,
        source_key: &str,
        installed: &[String],
    ) -> Result<(), ImruleError> {
        if installed.is_empty() {
            return Ok(());
        }
        let config_root = effective_config_root(&options.project_root, options.global);
        let mut config = self.config_port.load_config(&config_root, None, None)?;
        let skills = config.skills.get_or_insert_with(Default::default);
        for name in installed {
            skills.sources.insert(name.clone(), source_key.to_string());
        }
        self.config_write_port
            .save_config(&config_root, None, &config)
    }
}

/// Resolves the directory installed skills live in.
pub fn resolve_skills_base(
    fs_port: &dyn FileSystemPort,
    project_root: &Path,
    global: bool,
) -> PathBuf {
    if global {
        crate::domain::constants::xdg_config_home()
            .join("imrule")
            .join("skills")
    } else {
        let imrule_dir = fs_port
            .find_imrule_dir(project_root, true)
            .unwrap_or_else(|| project_root.join(".imrule"));
        imrule_dir.join("skills")
    }
}

/// Resolves the root the skill source registry is read from and written to.
pub fn effective_config_root(project_root: &Path, global: bool) -> PathBuf {
    if global {
        crate::domain::constants::xdg_config_home().join("imrule")
    } else {
        project_root.to_path_buf()
    }
}

/// Discovers skills from a fetched remote/local source directory.
/// Searches for SKILL.md files in common locations compatible with vercel-labs/skills format.
pub fn discover_remote_skills(root: &Path) -> Result<Vec<SkillInfo>, ImruleError> {
    let mut all_skills = Vec::new();

    // Direct walk of the root — finds skills at any depth.
    let discovery = walk_skills_tree(root)
        .map_err(|e| ImruleError::skills(format!("failed to walk skills tree: {e}")))?;

    // Filter to only valid skills (those with SKILL.md).
    for skill in discovery.skills {
        if skill.valid && skill.has_skill_md {
            all_skills.push(skill);
        }
    }

    // If nothing found, check if root itself is a skill.
    if all_skills.is_empty() && root.join("SKILL.md").is_file() {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root-skill".to_string());
        all_skills.push(SkillInfo {
            name,
            path: root.to_path_buf(),
            has_skill_md: true,
            valid: true,
            error: None,
        });
    }

    Ok(all_skills)
}
