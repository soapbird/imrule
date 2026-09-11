//! Skills discovery and propagation helpers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::config::SkillInfo;
use crate::domain::constants::{
    IMRULE_SKILLS_PATH, LEGACY_SKILLS_PATH, SKILL_MD_FILENAME, normalize_path_separators,
    relative_key,
};
use crate::domain::error::ImruleError;
use crate::domain::skills::{SkillsDiscovery, ensure_unique_skill_names, flatten_skill_name};

/// Discovers skills in `.imrule/skills` (falls back to `.ruler/skills`), each
/// named as `apply` publishes it.
pub fn discover_skills(project_root: &Path) -> Result<SkillsDiscovery, ImruleError> {
    let skills_dir = project_root.join(IMRULE_SKILLS_PATH);
    if skills_dir.exists() {
        return walk_project_skills_tree(&skills_dir);
    }
    let legacy_dir = project_root.join(LEGACY_SKILLS_PATH);
    if legacy_dir.exists() {
        return walk_project_skills_tree(&legacy_dir);
    }
    Ok(SkillsDiscovery::default())
}

/// Walks a project (or global) skills root, naming each skill by its flattened
/// path below the root (`python/cli` → `python-cli`). Fails when two skills
/// flatten to the same name, and warns when a skill's frontmatter `name`
/// disagrees with it.
pub fn walk_project_skills_tree(root: &Path) -> Result<SkillsDiscovery, ImruleError> {
    let mut discovery = walk_skills_tree(root).map_err(|e| ImruleError::skills(e.to_string()))?;
    let mut mismatches = Vec::new();
    for skill in &mut discovery.skills {
        let relative = skill.path.strip_prefix(root).unwrap_or(&skill.path);
        skill.name = flatten_skill_name(relative);
        if let Some(declared) = declared_skill_name(&skill.path.join(SKILL_MD_FILENAME)) {
            if declared != skill.name {
                mismatches.push(format!(
                    "Skill '{}' declares name '{declared}' but is published as '{}'; agents that require the name to match the directory will skip it.",
                    relative_key(root, &skill.path),
                    skill.name
                ));
            }
        }
    }
    ensure_unique_skill_names(&discovery.skills, root)?;
    discovery.warnings.extend(mismatches);
    Ok(discovery)
}

/// The `name` a `SKILL.md` declares in its frontmatter, if it declares one.
fn declared_skill_name(skill_md: &Path) -> Option<String> {
    let content = fs::read_to_string(skill_md).ok()?;
    let parsed = crate::domain::subagent::parse_frontmatter(&content).ok()??;
    parsed.meta.get("name")?.as_str().map(str::to_string)
}

/// Walks a skills root, returning valid skills plus validation warnings. Skills
/// keep their directory's own name, which is what a fetched source repository
/// is matched by.
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
                "Directory '{}' in skills has no SKILL.md and contains no sub-skills. It may be malformed or stray.",
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

/// Compares two skill trees byte for byte. Used by `imrule skills update` to
/// tell an actual update from a re-fetch that changed nothing, so an unchanged
/// skill is never removed and rewritten.
pub fn skill_trees_match(left: &Path, right: &Path) -> io::Result<bool> {
    if left.is_dir() != right.is_dir() {
        return Ok(false);
    }
    if !left.is_dir() {
        return Ok(fs::read(left)? == fs::read(right)?);
    }

    let left_entries = sorted_entry_names(left)?;
    let right_entries = sorted_entry_names(right)?;
    if left_entries != right_entries {
        return Ok(false);
    }
    for name in left_entries {
        if !skill_trees_match(&left.join(&name), &right.join(&name))? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn sorted_entry_names(dir: &Path) -> io::Result<Vec<std::ffi::OsString>> {
    let mut names: Vec<_> = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort();
    Ok(names)
}
