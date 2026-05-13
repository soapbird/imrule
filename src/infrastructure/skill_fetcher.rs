//! Concrete skill fetcher using git clone.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::application::ports::SkillFetcherPort;
use crate::domain::error::ImruleError;
use crate::domain::skills::RemoteSkillSource;

pub struct GitSkillFetcher {
    temp_dir: tempfile::TempDir,
}

impl GitSkillFetcher {
    pub fn new() -> Result<Self, ImruleError> {
        let temp_dir = tempfile::TempDir::new()
            .map_err(|e| ImruleError::skills(format!("failed to create temp dir: {e}")))?;
        Ok(Self { temp_dir })
    }

    pub fn temp_path(&self) -> &Path {
        self.temp_dir.path()
    }
}

impl SkillFetcherPort for GitSkillFetcher {
    fn fetch_to_temp(&self, source: &RemoteSkillSource) -> Result<PathBuf, ImruleError> {
        match source {
            RemoteSkillSource::Local { path } => {
                if !path.exists() {
                    return Err(ImruleError::skills(format!(
                        "local path does not exist: {}",
                        path.display()
                    )));
                }
                Ok(path.clone())
            }
            RemoteSkillSource::Github {
                owner,
                repo,
                subpath,
            } => {
                let url = format!("https://github.com/{owner}/{repo}.git");
                let clone_dir = self.temp_dir.path().join(repo);
                git_clone_shallow(&url, &clone_dir)?;
                Ok(if let Some(sub) = subpath {
                    clone_dir.join(sub)
                } else {
                    clone_dir
                })
            }
            RemoteSkillSource::Gitlab { url } => {
                let repo_name = extract_repo_name(url);
                let clone_dir = self.temp_dir.path().join(repo_name);
                git_clone_shallow(url, &clone_dir)?;
                Ok(clone_dir)
            }
            RemoteSkillSource::GitSsh { url } => {
                let repo_name = extract_repo_name(url);
                let clone_dir = self.temp_dir.path().join(repo_name);
                git_clone_shallow(url, &clone_dir)?;
                Ok(clone_dir)
            }
        }
    }
}

fn git_clone_shallow(url: &str, target: &Path) -> Result<(), ImruleError> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(target)
        .output()
        .map_err(|e| ImruleError::skills(format!("failed to run git clone: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ImruleError::skills(format!(
            "git clone failed for {url}: {stderr}"
        )));
    }
    Ok(())
}

fn extract_repo_name(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or("repo")
        .to_string()
}
