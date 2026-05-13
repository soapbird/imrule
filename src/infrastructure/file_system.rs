//! Filesystem helpers implementing `FileSystemPort`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::application::ports::FileSystemPort;
use crate::domain::constants::{
    normalize_path_separators, xdg_config_home, GENERATED_BY_IMRULE_MARKER, LEGACY_DIR_NAME,
    SKILLS_DIR,
};
use crate::domain::error::ImruleError;
const SUBAGENTS_DIR_NAME: &str = "agents";

pub struct FsFileSystem;

impl FsFileSystem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemPort for FsFileSystem {
    fn read_text(&self, path: &Path) -> Result<String, ImruleError> {
        fs::read_to_string(path)
            .map_err(|e| ImruleError::filesystem(format!("{}: {e}", path.display())))
    }

    fn write_text(&self, path: &Path, content: &str) -> Result<(), ImruleError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ImruleError::filesystem(format!("{}: {e}", parent.display())))?;
        }
        fs::write(path, content)
            .map_err(|e| ImruleError::filesystem(format!("{}: {e}", path.display())))
    }

    fn backup_file(&self, path: &Path) -> Result<(), ImruleError> {
        if path.exists() {
            let mut backup = path.as_os_str().to_os_string();
            backup.push(".bak");
            fs::copy(path, PathBuf::from(backup))
                .map_err(|e| ImruleError::filesystem(e.to_string()))?;
        }
        Ok(())
    }

    fn ensure_dir_exists(&self, path: &Path) -> Result<(), ImruleError> {
        fs::create_dir_all(path)
            .map_err(|e| ImruleError::filesystem(format!("{}: {e}", path.display())))
    }

    fn remove_file(&self, path: &Path) -> Result<(), ImruleError> {
        fs::remove_file(path)
            .map_err(|e| ImruleError::filesystem(format!("{}: {e}", path.display())))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), ImruleError> {
        if path.exists() {
            fs::remove_dir_all(path)
                .map_err(|e| ImruleError::filesystem(format!("{}: {e}", path.display())))?;
        }
        Ok(())
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), ImruleError> {
        fs::copy(from, to).map_err(|e| ImruleError::filesystem(e.to_string()))?;
        Ok(())
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn find_imrule_dir(&self, start_path: &Path, check_global: bool) -> Option<PathBuf> {
        let mut current = start_path.to_path_buf();
        loop {
            let candidate = current.join(".imrule");
            if candidate.is_dir() {
                return Some(candidate);
            }
            let legacy = current.join(LEGACY_DIR_NAME);
            if legacy.is_dir() {
                eprintln!(
                    "[imrule] Warning: using legacy '{LEGACY_DIR_NAME}/' directory. \
                     Run 'imrule init' to create '.imrule/' and migrate."
                );
                return Some(legacy);
            }
            if !current.pop() {
                break;
            }
        }

        if check_global {
            let global = xdg_config_home().join("imrule");
            if global.is_dir() {
                return Some(global);
            }
        }

        None
    }

    fn read_markdown_files(
        &self,
        imrule_dir: &Path,
        include_agents: bool,
    ) -> Result<Vec<(PathBuf, String)>, ImruleError> {
        let mut md_files = Vec::new();
        let mut saw_excluded_agents = false;
        walk_markdown(
            imrule_dir,
            imrule_dir,
            include_agents,
            &mut saw_excluded_agents,
            &mut md_files,
        )
        .map_err(|e| ImruleError::filesystem(e.to_string()))?;

        let top_level_agents = imrule_dir.join("AGENTS.md");
        let top_level_legacy = imrule_dir.join("instructions.md");

        let mut primary: Option<(PathBuf, String)> = md_files
            .iter()
            .find(|(path, _)| same_path(path, &top_level_agents))
            .cloned();
        if primary.is_none() {
            primary = md_files
                .iter()
                .find(|(path, _)| same_path(path, &top_level_legacy))
                .cloned();
        }

        let mut others: Vec<(PathBuf, String)> = md_files
            .into_iter()
            .filter(|(path, _)| primary.as_ref().map_or(true, |(p, _)| !same_path(path, p)))
            .collect();
        others.sort_by(|a, b| a.0.to_string_lossy().cmp(&b.0.to_string_lossy()));

        let mut ordered = Vec::new();
        if let Some(primary) = primary {
            ordered.push(primary);
        }
        ordered.extend(others);

        let repo_root = imrule_dir.parent().unwrap_or_else(|| Path::new("."));
        let root_agents = repo_root.join("AGENTS.md");
        if !same_path(&root_agents, &top_level_agents) {
            if let Ok(content) = fs::read_to_string(&root_agents) {
                let is_generated = content.starts_with(GENERATED_BY_IMRULE_MARKER);
                let has_imrule_files = !ordered.is_empty() || saw_excluded_agents;
                let contains_imrule_sources = content.contains("<!-- Source: .imrule/")
                    || content.contains("<!-- Source: imrule/")
                    || content.contains("<!-- Source: .ruler/");
                let is_probably_generated =
                    is_generated || (contains_imrule_sources && has_imrule_files);
                if !is_probably_generated || !has_imrule_files {
                    ordered.insert(0, (root_agents, content));
                }
            }
        }

        Ok(ordered)
    }

    fn find_all_imrule_dirs(&self, start_path: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let root = start_path
            .canonicalize()
            .unwrap_or_else(|_| start_path.to_path_buf());
        find_all_imrule_dirs_inner(start_path, &root, &mut found);
        found.sort_by(|a, b| {
            let depth_a = a.components().count();
            let depth_b = b.components().count();
            depth_b
                .cmp(&depth_a)
                .then_with(|| a.to_string_lossy().cmp(&b.to_string_lossy()))
        });
        found
    }
}

fn walk_markdown(
    imrule_dir: &Path,
    dir: &Path,
    include_agents: bool,
    saw_excluded_agents: &mut bool,
    md_files: &mut Vec<(PathBuf, String)>,
) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let full_path = entry.path();
        let file_type = entry.file_type()?;
        let metadata = if file_type.is_symlink() {
            match fs::metadata(&full_path) {
                Ok(metadata) => {
                    // Guard against symlinks that escape the project root.
                    if let (Ok(canonical), Some(project_root)) =
                        (full_path.canonicalize(), imrule_dir.parent())
                    {
                        let root_canonical = project_root
                            .canonicalize()
                            .unwrap_or_else(|_| project_root.to_path_buf());
                        if !canonical.starts_with(&root_canonical) {
                            continue;
                        }
                    }
                    metadata
                }
                Err(_) => continue,
            }
        } else {
            entry.metadata()?
        };

        if metadata.is_dir() {
            let relative = full_path.strip_prefix(imrule_dir).unwrap_or(&full_path);
            let relative_string = normalize_path_separators(&relative.to_string_lossy());
            if relative_string == SKILLS_DIR
                || relative_string.starts_with(&format!("{SKILLS_DIR}/"))
            {
                continue;
            }
            if (relative_string == SUBAGENTS_DIR_NAME
                || relative_string.starts_with(&format!("{SUBAGENTS_DIR_NAME}/")))
                && !include_agents
            {
                *saw_excluded_agents = true;
                continue;
            }
            walk_markdown(
                imrule_dir,
                &full_path,
                include_agents,
                saw_excluded_agents,
                md_files,
            )?;
        } else if metadata.is_file()
            && full_path.extension().and_then(|ext| ext.to_str()) == Some("md")
        {
            md_files.push((full_path.clone(), fs::read_to_string(&full_path)?));
        }
    }

    Ok(())
}

fn find_all_imrule_dirs_inner(dir: &Path, root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        if entry.file_name() == ".imrule" || entry.file_name() == LEGACY_DIR_NAME {
            found.push(path);
        } else if !entry.file_name().to_string_lossy().starts_with('.') {
            let git_dir = path.join(".git");
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if git_dir.is_dir() && canonical != root {
                continue;
            }
            find_all_imrule_dirs_inner(&path, root, found);
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b || (a.is_absolute() && b.is_absolute() && a.components().eq(b.components()))
}
