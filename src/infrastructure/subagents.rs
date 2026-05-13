//! Native subagent discovery helpers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::SubagentInfo;
use crate::domain::constants::*;
use crate::domain::subagent::{parse_frontmatter, validate_frontmatter, SubagentsDiscovery};

/// Loads and validates one subagent file.
pub fn load_subagent_file(file_path: &Path) -> io::Result<SubagentInfo> {
    let stem = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    let content = fs::read_to_string(file_path)?;
    let parsed = match parse_frontmatter(&content) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            return Ok(SubagentInfo::invalid(
                stem.clone(),
                file_path.to_path_buf(),
                format!("{stem}.md: missing YAML frontmatter"),
            ))
        }
        Err(err) => {
            return Ok(SubagentInfo::invalid(
                stem.clone(),
                file_path.to_path_buf(),
                format!("{stem}.md: invalid YAML frontmatter: {err}"),
            ))
        }
    };
    match validate_frontmatter(&parsed.meta, &stem) {
        Ok(frontmatter) => Ok(SubagentInfo {
            name: stem,
            path: file_path.to_path_buf(),
            frontmatter: Some(frontmatter),
            body: Some(parsed.body),
            valid: true,
            error: None,
        }),
        Err(err) => Ok(SubagentInfo::invalid(
            stem.clone(),
            file_path.to_path_buf(),
            format!("{stem}.md: {err}"),
        )),
    }
}

/// Discovers valid subagents from `.imrule/agents` (falls back to `.ruler/agents`).
pub fn discover_subagents(project_root: &Path) -> io::Result<SubagentsDiscovery> {
    let dir = project_root.join(IMRULE_SUBAGENTS_PATH);
    if dir.exists() {
        return discover_subagents_from_dir(&dir);
    }
    let legacy_dir = project_root.join(LEGACY_SUBAGENTS_PATH);
    if legacy_dir.exists() {
        return discover_subagents_from_dir(&legacy_dir);
    }
    Ok(SubagentsDiscovery::default())
}

fn discover_subagents_from_dir(dir: &Path) -> io::Result<SubagentsDiscovery> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    let mut result = SubagentsDiscovery::default();
    for entry in entries {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            let info = load_subagent_file(&path)?;
            if info.valid {
                result.subagents.push(info);
            } else if let Some(error) = info.error {
                result.warnings.push(error);
            }
        }
    }
    Ok(result)
}

/// Gets native subagent target paths generated for selected agents.
pub fn get_subagents_gitignore_paths(
    project_root: &Path,
    agents: &[AgentDefinition],
) -> io::Result<Vec<PathBuf>> {
    if !project_root.join(IMRULE_SUBAGENTS_PATH).exists()
        && !project_root.join(LEGACY_SUBAGENTS_PATH).exists()
    {
        return Ok(Vec::new());
    }
    let selected: std::collections::BTreeSet<_> = agents
        .iter()
        .filter(|agent| agent.capabilities.native_subagents)
        .map(|agent| agent.identifier)
        .collect();
    let target_specs: &[(&str, &[&str])] = &[
        (CLAUDE_SUBAGENTS_PATH, &["claude"]),
        (CURSOR_SUBAGENTS_PATH, &["cursor"]),
        (CODEX_SUBAGENTS_PATH, &["codex"]),
        (COPILOT_SUBAGENTS_PATH, &["copilot"]),
    ];
    Ok(target_specs
        .iter()
        .filter(|(_, ids)| ids.iter().any(|id| selected.contains(id)))
        .map(|(path, _)| project_root.join(path))
        .collect())
}
