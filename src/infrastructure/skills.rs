//! Skills discovery and propagation helpers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::config::SkillInfo;
use crate::domain::constants::{normalize_path_separators, *};
use crate::domain::skills::SkillsDiscovery;

/// Discovers skills in `.imrule/skills`.
pub fn discover_skills(project_root: &Path) -> io::Result<SkillsDiscovery> {
    let skills_dir = project_root.join(IMRULE_SKILLS_PATH);
    if !skills_dir.exists() {
        return Ok(SkillsDiscovery::default());
    }
    walk_skills_tree(&skills_dir)
}

/// Walks a skills root, returning valid skills plus validation warnings.
pub fn walk_skills_tree(root: &Path) -> io::Result<SkillsDiscovery> {
    let mut result = SkillsDiscovery::default();
    walk(root, Path::new(""), &mut result)?;
    Ok(result)
}

fn walk(current_path: &Path, relative_path: &Path, result: &mut SkillsDiscovery) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current_path)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let entry_relative = if relative_path.as_os_str().is_empty() {
            PathBuf::from(entry.file_name())
        } else {
            relative_path.join(entry.file_name())
        };

        if has_skill_md(&entry_path) {
            result.skills.push(SkillInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry_path,
                has_skill_md: true,
                valid: true,
                error: None,
            });
        } else if is_grouping_dir(&entry_path) {
            walk(&entry_path, &entry_relative, result)?;
        } else {
            result.warnings.push(format!(
                "Directory '{}' in .imrule/skills has no SKILL.md and contains no sub-skills. It may be malformed or stray.",
                normalize_path_separators(&entry_relative.to_string_lossy())
            ));
        }
    }
    Ok(())
}

/// Checks whether a directory contains `SKILL.md`.
pub fn has_skill_md(dir_path: &Path) -> bool {
    dir_path.join(SKILL_MD_FILENAME).is_file()
}

/// Checks whether a directory groups nested skills.
pub fn is_grouping_dir(dir_path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir_path) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && (has_skill_md(&path) || is_grouping_dir(&path)) {
            return true;
        }
    }
    false
}

/// Recursively copies a skills directory.
pub fn copy_skills_directory(src_dir: &Path, dest_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dest_dir)?;
    copy_recursive(src_dir, dest_dir)
}

fn copy_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dest)?;
    }
    Ok(())
}
