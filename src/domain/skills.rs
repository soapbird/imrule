//! Skills domain types and pure helpers.

use std::collections::BTreeMap;
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
///
/// `current_dir` and `path_exists` are injected so this stays pure: a bare
/// relative path is ambiguous with the `org/repo` shorthand, so the caller's
/// filesystem decides, and relative paths resolve against the caller's
/// working directory.
pub fn parse_skill_source(
    source: &str,
    current_dir: &Path,
    path_exists: impl Fn(&Path) -> bool,
) -> Result<RemoteSkillSource, ImruleError> {
    let trimmed = source.trim();

    // Local path: starts with . or / or exists on disk as a non-URL
    if trimmed.starts_with("./")
        || trimmed.starts_with('/')
        || trimmed.starts_with("../")
        || (path_exists(Path::new(trimmed)) && !trimmed.contains("://"))
    {
        let path = PathBuf::from(trimmed);
        return Ok(RemoteSkillSource::Local {
            path: if path.is_absolute() {
                path
            } else {
                current_dir.join(path)
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

/// The name a skill under a project skills root is published as: its path below
/// that root with the separators turned into hyphens, so `python/cli` becomes
/// `python-cli`. Agents only look one level deep, and publishing a grouped
/// skill under its leaf name alone let `python/cli` and `rust/cli` both land on
/// `cli`, one silently overwriting the other.
pub fn flatten_skill_name(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("-")
}

/// Rejects two skills that publish under the same name — `python/cli` next to a
/// top-level `python-cli` — since copying both would leave only one of them.
pub fn ensure_unique_skill_names(skills: &[SkillInfo], root: &Path) -> Result<(), ImruleError> {
    let mut seen: BTreeMap<&str, &Path> = BTreeMap::new();
    for skill in skills {
        if let Some(first) = seen.insert(skill.name.as_str(), skill.path.as_path()) {
            return Err(ImruleError::skills(format!(
                "skills '{}' and '{}' would both be published as '{}'; rename one of them",
                relative_key(root, first),
                relative_key(root, &skill.path),
                skill.name
            )));
        }
    }
    Ok(())
}

/// Gets native skill target paths generated for selected agents.
pub fn get_skills_gitignore_paths(project_root: &Path, agents: &[AgentDefinition]) -> Vec<PathBuf> {
    crate::domain::agent::selected_target_dirs(
        project_root,
        agents,
        |capabilities| capabilities.native_skills,
        &[
            (CLAUDE_SKILLS_PATH, &["claude", "copilot", "kilocode"]),
            (CODEX_SKILLS_PATH, &["codex"]),
            (OPENCODE_SKILLS_PATH, &["opencode"]),
            (PI_SKILLS_PATH, &["pi"]),
            (GOOSE_SKILLS_PATH, &["goose", "amp"]),
            (VIBE_SKILLS_PATH, &["mistral"]),
            (ROO_SKILLS_PATH, &["roo"]),
            (GEMINI_SKILLS_PATH, &["gemini-cli"]),
            (KIMI_SKILLS_PATH, &["kimi-cli", "kimi-code", "kimi"]),
            (JUNIE_SKILLS_PATH, &["junie"]),
            (CURSOR_SKILLS_PATH, &["cursor"]),
            (WINDSURF_SKILLS_PATH, &["windsurf"]),
            (FACTORY_SKILLS_PATH, &["factory"]),
            (ANTIGRAVITY_SKILLS_PATH, &["antigravity"]),
            (GJC_SKILLS_PATH, &["gjc"]),
        ],
    )
}

/// Every agent skills root, project-relative with forward slashes, whichever
/// agents are selected.
pub fn all_skills_roots() -> Vec<String> {
    get_skills_gitignore_paths(Path::new(""), &crate::domain::agent::all_agents())
        .iter()
        .map(|root| normalize_path_separators(&root.to_string_lossy()))
        .collect()
}

/// The string recorded for a source in `[skills.sources]`. Local paths are
/// stored resolved so a later `update` run from another working directory
/// still points at the same tree.
pub fn skill_source_key(source: &RemoteSkillSource, raw: &str) -> String {
    match source {
        RemoteSkillSource::Local { path } => path.to_string_lossy().to_string(),
        _ => raw.trim().to_string(),
    }
}

/// One recorded source together with the installed skills that came from it,
/// so an update fetches each source exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillUpdateGroup {
    pub source: String,
    pub skills: Vec<String>,
}

/// What an update did to one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillUpdateStatus {
    /// The installed copy differed from the freshly fetched source.
    Updated,
    /// Recorded in the config but absent on disk, so it was installed again.
    Reinstalled,
    /// The fetched source is byte-identical to what is installed.
    Unchanged,
    /// The recorded source no longer contains a skill by that name.
    MissingInSource,
    /// The source could not be fetched or read.
    Failed,
}

impl SkillUpdateStatus {
    /// Human-facing label for the update report, `would …` under `dry_run`.
    pub fn label(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Updated, false) => "updated",
            (Self::Updated, true) => "would update",
            (Self::Reinstalled, false) => "reinstalled",
            (Self::Reinstalled, true) => "would reinstall",
            (Self::Unchanged, _) => "unchanged",
            (Self::MissingInSource, _) => "missing in source",
            (Self::Failed, _) => "failed",
        }
    }
}

/// The outcome of updating one skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillUpdateOutcome {
    pub name: String,
    pub source: String,
    pub status: SkillUpdateStatus,
    /// Failure detail, set only for [`SkillUpdateStatus::Failed`].
    pub detail: Option<String>,
}

/// Groups recorded skill sources for an update run, optionally narrowed to
/// `requested` skill names. Errors when a requested name has no recorded
/// source, since silently skipping it would look like a successful update.
pub fn group_skill_sources(
    sources: &BTreeMap<String, String>,
    requested: Option<&[String]>,
) -> Result<Vec<SkillUpdateGroup>, ImruleError> {
    if let Some(requested) = requested {
        let unknown: Vec<&str> = requested
            .iter()
            .filter(|name| !sources.contains_key(*name))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(ImruleError::skills(format!(
                "no recorded source for: {}. Run `imrule skills add <source>` first.",
                unknown.join(", ")
            )));
        }
    }

    let mut groups: Vec<SkillUpdateGroup> = Vec::new();
    for (name, source) in sources {
        if let Some(requested) = requested {
            if !requested.iter().any(|wanted| wanted == name) {
                continue;
            }
        }
        match groups.iter_mut().find(|group| &group.source == source) {
            Some(group) => group.skills.push(name.clone()),
            None => groups.push(SkillUpdateGroup {
                source: source.clone(),
                skills: vec![name.clone()],
            }),
        }
    }
    Ok(groups)
}
