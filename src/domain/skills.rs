//! Skills domain types and pure helpers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::domain::agent::AgentDefinition;
use crate::domain::config::SkillInfo;
use crate::domain::constants::*;

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
