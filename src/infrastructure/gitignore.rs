//! Managed ignore-file updates implementing `GitignorePort`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::application::ports::GitignorePort;
use crate::domain::constants::normalize_path_separators;
use crate::domain::error::ImruleError;

const IMRULE_START_MARKER: &str = "# START ImRule Generated Files";
const IMRULE_END_MARKER: &str = "# END ImRule Generated Files";

pub struct GitignoreUpdater;

impl GitignoreUpdater {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitignoreUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl GitignorePort for GitignoreUpdater {
    fn update_gitignore(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
        ignore_file: &str,
    ) -> Result<(), ImruleError> {
        let gitignore_path = project_root.join(ignore_file);
        let existing_content = match fs::read_to_string(&gitignore_path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(ImruleError::gitignore(err.to_string())),
        };

        let existing_paths = get_existing_paths_excluding_ruler_block(&existing_content);
        let mut new_paths = BTreeSet::new();

        for path in paths {
            let relative = normalize_output_path(project_root, path);
            if is_ruler_input_path(&relative) {
                continue;
            }
            let rooted = if relative.starts_with('/') {
                relative
            } else {
                format!("/{relative}")
            };
            if !existing_paths.contains(&rooted) {
                new_paths.insert(rooted);
            }
        }

        let ruler_paths: Vec<_> = new_paths.into_iter().collect();
        let new_content = update_gitignore_content(&existing_content, &ruler_paths);

        if let Some(parent) = gitignore_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ImruleError::gitignore(e.to_string()))?;
        }
        fs::write(gitignore_path, new_content).map_err(|e| ImruleError::gitignore(e.to_string()))
    }
}

fn normalize_output_path(project_root: &Path, path: &Path) -> String {
    let relative = if path.is_absolute() {
        path.strip_prefix(project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        let normalized = path.components().collect::<PathBuf>();
        let project_basename = project_root.file_name().and_then(|name| name.to_str());
        let normalized_string = normalized.to_string_lossy().to_string();
        if let Some(project_basename) = project_basename {
            let sep = std::path::MAIN_SEPARATOR;
            let prefix = format!("{project_basename}{sep}");
            if normalized_string.starts_with(&prefix) {
                normalized_string[prefix.len()..].to_string()
            } else {
                normalized_string
            }
        } else {
            normalized_string
        }
    };
    normalize_path_separators(&relative)
}

fn is_ruler_input_path(path: &str) -> bool {
    path.contains("/.imrule/") || path.starts_with(".imrule/")
}

fn get_existing_paths_excluding_ruler_block(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_ruler_block = false;

    for line in content.split('\n') {
        let trimmed = line.trim();
        if trimmed == IMRULE_START_MARKER {
            in_ruler_block = true;
            continue;
        }
        if trimmed == IMRULE_END_MARKER {
            in_ruler_block = false;
            continue;
        }
        if !in_ruler_block && !trimmed.is_empty() && !trimmed.starts_with('#') {
            paths.push(trimmed.to_string());
        }
    }

    paths
}

fn update_gitignore_content(existing_content: &str, ruler_paths: &[String]) -> String {
    let lines: Vec<_> = existing_content.split('\n').collect();
    let mut new_lines = Vec::new();
    let mut in_first_ruler_block = false;
    let mut has_ruler_block = false;
    let mut processed_first_block = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed == IMRULE_START_MARKER && !processed_first_block {
            in_first_ruler_block = true;
            has_ruler_block = true;
            new_lines.push(line.to_string());
            new_lines.extend(ruler_paths.iter().cloned());
            continue;
        }
        if trimmed == IMRULE_END_MARKER && in_first_ruler_block {
            in_first_ruler_block = false;
            processed_first_block = true;
            new_lines.push(line.to_string());
            continue;
        }
        if !in_first_ruler_block {
            new_lines.push(line.to_string());
        }
    }

    if !has_ruler_block {
        if !existing_content.trim().is_empty() && !existing_content.ends_with("\n\n") {
            new_lines.push(String::new());
        }
        new_lines.push(IMRULE_START_MARKER.to_string());
        new_lines.extend(ruler_paths.iter().cloned());
        new_lines.push(IMRULE_END_MARKER.to_string());
    }

    let mut result = new_lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}
