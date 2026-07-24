//! Git index management: untracks generated files that git still tracks.
//!
//! `.gitignore` only affects untracked files. When a generated file (e.g.
//! `.cursor/mcp.json`) was committed before ImRule started ignoring it, the
//! ignore entry has no effect and the file keeps showing up in `git status`.
//! `GitUntracker` removes such generated files from the index
//! (`git rm --cached`) while leaving them on disk.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::application::ports::GitTrackingPort;
use crate::domain::error::ImruleError;

pub struct GitUntracker;

impl GitUntracker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitUntracker {
    fn default() -> Self {
        Self::new()
    }
}

impl GitTrackingPort for GitUntracker {
    fn untrack_generated_files(
        &self,
        project_root: &Path,
        paths: &[PathBuf],
    ) -> Result<Vec<PathBuf>, ImruleError> {
        let relative_paths: Vec<PathBuf> = paths
            .iter()
            .filter_map(|path| path.strip_prefix(project_root).ok().map(PathBuf::from))
            .collect();
        if relative_paths.is_empty() || !is_git_work_tree(project_root) {
            return Ok(Vec::new());
        }

        // `git ls-files -- <paths>` resolves directories recursively, so skills
        // or subagent target directories expand to their tracked files.
        let output = Command::new("git")
            .arg("ls-files")
            .arg("--")
            .args(&relative_paths)
            .current_dir(project_root)
            .output()
            .map_err(|e| ImruleError::git_tracking(format!("failed to run git ls-files: {e}")))?;
        if !output.status.success() {
            // git ls-files failure is non-fatal: it may mean no tracked files
            // match, or the repo is in an unusual state. Either way, there is
            // nothing to untrack, so return an empty list.
            return Ok(Vec::new());
        }

        let tracked: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        if tracked.is_empty() {
            return Ok(Vec::new());
        }

        let status = Command::new("git")
            .arg("rm")
            .arg("--cached")
            .arg("--quiet")
            .arg("--ignore-unmatch")
            .arg("--")
            .args(&tracked)
            .current_dir(project_root)
            .status()
            .map_err(|e| {
                ImruleError::git_tracking(format!("failed to run git rm --cached: {e}"))
            })?;
        if !status.success() {
            return Err(ImruleError::git_tracking(format!(
                "git rm --cached failed with status {status}"
            )));
        }

        Ok(tracked.iter().map(|path| project_root.join(path)).collect())
    }
}

fn is_git_work_tree(project_root: &Path) -> bool {
    Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(project_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
