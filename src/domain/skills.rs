//! Skills domain types and pure helpers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::SkillInfo;
use crate::domain::constants::*;
use crate::domain::error::ImruleError;

/// Parsed source for skill installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteSkillSource {
    Github {
        owner: String,
        repo: String,
        subpath: Option<String>,
    },
    Gitlab {
        url: String,
    },
    GitSsh {
        url: String,
    },
    Local {
        path: PathBuf,
    },
}

/// Parses a skill source string into a `RemoteSkillSource`.
///
/// Supported formats:
/// - `org/repo` shorthand
/// - `https://github.com/org/repo`
/// - `https://github.com/org/repo/tree/<branch>/<path>`
/// - `https://gitlab.com/org/repo`
/// - `git@github.com:org/repo.git`
/// - `./local/path` or `/abs/path`
pub fn parse_skill_source(source: &str) -> Result<RemoteSkillSource, ImruleError> {
    let trimmed = source.trim();

    // Local path: starts with . or / or has a path separator on non-URL
    if trimmed.starts_with("./")
        || trimmed.starts_with('/')
        || trimmed.starts_with("../")
        || (Path::new(trimmed).exists() && !trimmed.contains("://"))
    {
        let path = PathBuf::from(trimmed);
        return Ok(RemoteSkillSource::Local {
            path: if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            },
        });
    }

    // Git SSH: git@host:org/repo.git
    if trimmed.starts_with("git@") {
        return Ok(RemoteSkillSource::GitSsh {
            url: trimmed.to_string(),
        });
    }

    // GitHub URL: https://github.com/org/repo or https://github.com/org/repo/tree/branch/path
    if trimmed.starts_with("https://github.com/") {
        let rest = trimmed.strip_prefix("https://github.com/").unwrap_or("");
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(ImruleError::skills(format!("invalid GitHub URL: {source}")));
        }
        let owner = parts[0].to_string();
        let remainder = parts[1];

        // Check for /tree/<branch>/<subpath>
        let subpath = if let Some(tree_idx) = remainder.find("/tree/") {
            let after_tree = &remainder[tree_idx + "/tree/".len()..];
            // Skip the branch name (first segment after /tree/)
            after_tree
                .find('/')
                .map(|slash_idx| after_tree[slash_idx + 1..].to_string())
        } else {
            None
        };

        // Repo name: strip .git suffix and /tree/... suffix
        let repo = remainder
            .split('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".git")
            .to_string();

        if repo.is_empty() {
            return Err(ImruleError::skills(format!("invalid GitHub URL: {source}")));
        }

        return Ok(RemoteSkillSource::Github {
            owner,
            repo,
            subpath,
        });
    }

    // GitLab URL
    if trimmed.starts_with("https://gitlab.com/") {
        return Ok(RemoteSkillSource::Gitlab {
            url: trimmed.to_string(),
        });
    }

    // org/repo shorthand: must be exactly two segments with no path separators
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        let repo = parts[1].trim_end_matches(".git").to_string();
        return Ok(RemoteSkillSource::Github {
            owner: parts[0].to_string(),
            repo,
            subpath: None,
        });
    }

    Err(ImruleError::skills(format!(
        "unrecognized skill source format: {source}"
    )))
}

/// Skills discovery result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillsDiscovery {
    pub skills: Vec<SkillInfo>,
    pub warnings: Vec<String>,
}

/// Formats validation warnings for display.
pub fn format_validation_warnings(warnings: &[String]) -> String {
    warnings
        .iter()
        .map(|warning| format!("  - {warning}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Gets native skill target paths generated for selected agents.
pub fn get_skills_gitignore_paths(project_root: &Path, agents: &[AgentDefinition]) -> Vec<PathBuf> {
    let selected: BTreeSet<_> = agents
        .iter()
        .filter(|agent| agent.capabilities.native_skills)
        .map(|agent| agent.identifier)
        .collect();
    let target_specs: &[(&str, &[&str])] = &[
        (CLAUDE_SKILLS_PATH, &["claude", "copilot", "kilocode"]),
        (CODEX_SKILLS_PATH, &["codex"]),
        (OPENCODE_SKILLS_PATH, &["opencode"]),
        (PI_SKILLS_PATH, &["pi"]),
        (GOOSE_SKILLS_PATH, &["goose", "amp"]),
        (VIBE_SKILLS_PATH, &["mistral"]),
        (ROO_SKILLS_PATH, &["roo"]),
        (GEMINI_SKILLS_PATH, &["gemini-cli"]),
        (JUNIE_SKILLS_PATH, &["junie"]),
        (CURSOR_SKILLS_PATH, &["cursor"]),
        (WINDSURF_SKILLS_PATH, &["windsurf"]),
        (FACTORY_SKILLS_PATH, &["factory"]),
        (ANTIGRAVITY_SKILLS_PATH, &["antigravity"]),
    ];
    target_specs
        .iter()
        .filter(|(_, ids)| ids.iter().any(|id| selected.contains(id)))
        .map(|(path, _)| project_root.join(path))
        .collect()
}
