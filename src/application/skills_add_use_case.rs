//! Use case for `imrule skills add <source>`.

use std::path::{Path, PathBuf};

use crate::application::ports::{FileSystemPort, SkillFetcherPort};
use crate::domain::config::SkillInfo;
use crate::domain::error::ImruleError;
use crate::domain::skills::parse_skill_source;
use crate::infrastructure::skills::{copy_skills_directory, walk_skills_tree};

/// Runtime options for `imrule skills add`.
#[derive(Debug, Clone)]
pub struct SkillsAddOptions {
    pub project_root: PathBuf,
    pub source: String,
    pub skill_names: Option<Vec<String>>,
    pub list_only: bool,
    pub global: bool,
    pub verbose: bool,
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
}

impl<'a> SkillsAddUseCase<'a> {
    pub fn new(fetcher: &'a dyn SkillFetcherPort, fs_port: &'a dyn FileSystemPort) -> Self {
        Self { fetcher, fs_port }
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
        let skills_base = if options.global {
            let xdg = crate::domain::constants::xdg_config_home().join("imrule");
            xdg.join("skills")
        } else {
            let imrule_dir = self
                .fs_port
                .find_imrule_dir(&options.project_root, true)
                .unwrap_or_else(|| options.project_root.join(".imrule"));
            imrule_dir.join("skills")
        };

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

        Ok(SkillsAddResult {
            listed: Vec::new(),
            installed,
        })
    }
}

/// Discovers skills from a fetched remote/local source directory.
/// Searches for SKILL.md files in common locations compatible with vercel-labs/skills format.
fn discover_remote_skills(root: &Path) -> Result<Vec<SkillInfo>, ImruleError> {
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
