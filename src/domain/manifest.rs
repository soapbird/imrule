//! Record of what the previous `apply` produced, so a later run can clean up
//! artifacts it no longer generates.
//!
//! Without this record every output is derived from the *current* configuration.
//! The moment the configuration shrinks — an MCP server removed, an agent
//! dropped from `default_agents` — the files written by the previous run become
//! unreachable: `apply` stops listing them (so they fall out of the managed
//! `.gitignore` block and reappear as committable files) and `clear` cannot
//! find the keys to strip either. The manifest closes that gap by carrying the
//! previous run's outputs forward.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::constants::relative_key;

/// Schema version of the on-disk manifest. Bump when the shape changes
/// incompatibly; readers treat an unrecognized version as "no manifest".
pub const MANIFEST_VERSION: u32 = 1;

/// A native MCP config file written by `apply`, paired with the section name
/// that agent nests its servers under (`mcpServers`, `mcp`, `context_servers`,
/// …). The key is recorded because a stale target's agent may no longer be
/// selected, leaving no other way to find the servers to strip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTarget {
    /// Project-relative path with forward slashes.
    pub path: String,
    /// The agent's `mcp_server_key`; empty when the agent has none.
    #[serde(default)]
    pub server_key: String,
}

/// What one `apply` run produced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyManifest {
    pub version: u32,
    /// Every generated path, project-relative with forward slashes.
    #[serde(default)]
    pub paths: Vec<String>,
    /// MCP server names propagated into native configs.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Native MCP config files those servers were written into.
    #[serde(default)]
    pub mcp_targets: Vec<McpTarget>,
    /// Skill directories copied into agent skill roots, project-relative. Kept
    /// apart from `paths`, which feed `.gitignore` — each root already covers
    /// its skills there — so that a skill renamed or removed from
    /// `.imrule/skills/` is pruned instead of lingering beside its successor.
    #[serde(default)]
    pub skills: Vec<String>,
}

impl ApplyManifest {
    /// Builds a manifest from absolute paths, normalizing them against
    /// `project_root` and sorting for a stable on-disk diff.
    pub fn new(
        project_root: &Path,
        paths: &[std::path::PathBuf],
        mcp_servers: &[String],
        mcp_targets: &[(std::path::PathBuf, String)],
    ) -> Self {
        let mcp_servers: BTreeSet<String> = mcp_servers.iter().cloned().collect();
        let mut targets: Vec<McpTarget> = mcp_targets
            .iter()
            .map(|(path, server_key)| McpTarget {
                path: relative_key(project_root, path),
                server_key: server_key.clone(),
            })
            .collect();
        targets.sort_by(|a, b| a.path.cmp(&b.path));
        targets.dedup_by(|a, b| a.path == b.path);

        Self {
            version: MANIFEST_VERSION,
            paths: relative_keys(project_root, paths),
            mcp_servers: mcp_servers.into_iter().collect(),
            mcp_targets: targets,
            skills: Vec::new(),
        }
    }

    /// Records the skill directories this run copied into agent skill roots.
    pub fn with_skills(mut self, project_root: &Path, skills: &[std::path::PathBuf]) -> Self {
        self.skills = relative_keys(project_root, skills);
        self
    }

    /// Folds an earlier manifest into this one, keeping every entry from both.
    ///
    /// Used when `--agents` narrows a run: this manifest describes only the
    /// agents that ran, so anything the earlier one recorded for the rest must
    /// carry forward or a later full run would see it as stale and delete it.
    /// Where both describe the same MCP target, this run's entry wins.
    pub fn merged_with(&self, earlier: &ApplyManifest) -> Self {
        let paths: BTreeSet<String> = self.paths.iter().chain(&earlier.paths).cloned().collect();
        let mcp_servers: BTreeSet<String> = self
            .mcp_servers
            .iter()
            .chain(&earlier.mcp_servers)
            .cloned()
            .collect();
        let mut mcp_targets = self.mcp_targets.clone();
        for target in &earlier.mcp_targets {
            if !mcp_targets.iter().any(|kept| kept.path == target.path) {
                mcp_targets.push(target.clone());
            }
        }
        mcp_targets.sort_by(|a, b| a.path.cmp(&b.path));
        let skills: BTreeSet<String> = self.skills.iter().chain(&earlier.skills).cloned().collect();

        Self {
            version: MANIFEST_VERSION,
            paths: paths.into_iter().collect(),
            mcp_servers: mcp_servers.into_iter().collect(),
            mcp_targets,
            skills: skills.into_iter().collect(),
        }
    }

    /// Skill directories this manifest recorded that `current` no longer copies.
    pub fn stale_skills(&self, current: &ApplyManifest) -> Vec<String> {
        stale_of(&self.skills, current.skills.iter().map(String::as_str))
    }

    /// Paths this manifest recorded that `current` no longer produces.
    pub fn stale_paths(&self, current: &ApplyManifest) -> Vec<String> {
        stale_of(
            &self.paths,
            current.paths.iter().map(String::as_str).chain(
                current
                    .mcp_targets
                    .iter()
                    .map(|target| target.path.as_str()),
            ),
        )
    }

    /// Native MCP configs this manifest recorded that `current` no longer writes.
    pub fn stale_mcp_targets(&self, current: &ApplyManifest) -> Vec<McpTarget> {
        let kept: BTreeSet<&str> = current
            .mcp_targets
            .iter()
            .map(|target| target.path.as_str())
            .collect();
        self.mcp_targets
            .iter()
            .filter(|target| !kept.contains(target.path.as_str()))
            .cloned()
            .collect()
    }

    /// Server names this manifest propagated that `current` no longer defines.
    /// These linger inside configs `apply` still writes, because the merge
    /// strategy only adds keys.
    pub fn stale_mcp_servers(&self, current: &ApplyManifest) -> Vec<String> {
        stale_of(
            &self.mcp_servers,
            current.mcp_servers.iter().map(String::as_str),
        )
    }

    /// Returns `true` when the manifest was written by a schema this build
    /// understands. An unknown version is ignored rather than misread.
    pub fn is_readable(&self) -> bool {
        self.version == MANIFEST_VERSION
    }
}

/// `paths` as project-relative keys, sorted and deduplicated.
fn relative_keys(project_root: &Path, paths: &[std::path::PathBuf]) -> Vec<String> {
    let keys: BTreeSet<String> = paths
        .iter()
        .map(|path| relative_key(project_root, path))
        .collect();
    keys.into_iter().collect()
}

/// Entries from `previous` that `kept` no longer contains.
fn stale_of<'a>(previous: &[String], kept: impl Iterator<Item = &'a str>) -> Vec<String> {
    let kept: BTreeSet<&str> = kept.collect();
    previous
        .iter()
        .filter(|entry| !kept.contains(entry.as_str()))
        .cloned()
        .collect()
}
