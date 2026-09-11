//! Built-in skills: the catalog ImRule ships inside its binary, and the pure
//! rules that decide which of them fit a project.
//!
//! The infrastructure layer embeds the files and collects [`ProjectSignals`]
//! from disk; everything here only interprets that data.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::constants::SKILL_MD_FILENAME;
use crate::domain::error::ImruleError;
use crate::domain::skills::flatten_skill_name;
use crate::domain::subagent::parse_frontmatter;

/// Frontmatter `metadata` key carrying a built-in skill's revision. Bumped
/// whenever the skill's content changes, so `setup` can tell an installed copy
/// that merely predates the current release from one the user edited.
pub const BUILTIN_SKILL_VERSION_KEY: &str = "imrule-skill-version";

/// Frontmatter `metadata` key every built-in `SKILL.md` sets to `"true"`. A
/// `SKILL.md` without it was not installed by `setup`, so it belongs to the user.
pub const BUILTIN_SKILL_MARKER_KEY: &str = "imrule-builtin";

/// One built-in skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinSkill {
    /// Path below the skills root, with forward slashes (`python/cli`).
    pub path: String,
    /// Published name: the path joined with hyphens (`python-cli`).
    pub name: String,
    /// The `description` from `SKILL.md` frontmatter.
    pub description: String,
    /// The `imrule-skill-version` revision; `0` when absent.
    pub revision: u32,
    /// Files relative to the skill directory, with their contents.
    pub files: Vec<(String, &'static str)>,
}

/// Builds the catalog from `(path below skills root, contents)` pairs. Every
/// directory holding a `SKILL.md` is a skill; other files belong to the skill
/// whose directory contains them. Files outside any skill are ignored.
pub fn build_builtin_catalog(files: &[(&'static str, &'static str)]) -> Vec<BuiltinSkill> {
    let suffix = format!("/{SKILL_MD_FILENAME}");
    let mut skills: BTreeMap<String, BuiltinSkill> = files
        .iter()
        .filter_map(|(path, content)| {
            let skill_path = path.strip_suffix(&suffix)?;
            let (description, revision) = skill_metadata(content);
            Some((
                skill_path.to_string(),
                BuiltinSkill {
                    path: skill_path.to_string(),
                    name: flatten_skill_name(std::path::Path::new(skill_path)),
                    description,
                    revision,
                    files: Vec::new(),
                },
            ))
        })
        .collect();

    for &(path, content) in files {
        // Walking up from the file, the first skill directory met is the
        // deepest one containing it.
        let mut dir = path;
        while let Some((parent, _)) = dir.rsplit_once('/') {
            if let Some(skill) = skills.get_mut(parent) {
                skill
                    .files
                    .push((path[parent.len() + 1..].to_string(), content));
                break;
            }
            dir = parent;
        }
    }

    skills
        .into_values()
        .map(|mut skill| {
            skill.files.sort_by(|a, b| a.0.cmp(&b.0));
            skill
        })
        .collect()
}

/// Reads `description` and the revision out of a `SKILL.md`.
fn skill_metadata(content: &str) -> (String, u32) {
    let Ok(Some(parsed)) = parse_frontmatter(content) else {
        return (String::new(), 0);
    };
    let description = parsed
        .meta
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    (description, revision_of(&parsed.meta))
}

/// The revision recorded in frontmatter metadata, accepting a string or number.
fn revision_of(meta: &serde_json::Value) -> u32 {
    match meta
        .get("metadata")
        .and_then(|metadata| metadata.get(BUILTIN_SKILL_VERSION_KEY))
    {
        Some(serde_json::Value::String(text)) => text.trim().parse().unwrap_or(0),
        Some(serde_json::Value::Number(number)) => number
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
        _ => 0,
    }
}

/// The revision of an installed `SKILL.md`, `0` when it records none.
pub fn installed_skill_revision(skill_md: &str) -> u32 {
    match parse_frontmatter(skill_md) {
        Ok(Some(parsed)) => revision_of(&parsed.meta),
        _ => 0,
    }
}

/// Finds a built-in skill by path (`rust/cli`) or published name (`rust-cli`).
pub fn find_builtin_skill<'a>(
    catalog: &'a [BuiltinSkill],
    query: &str,
) -> Option<&'a BuiltinSkill> {
    let query = query.trim().trim_matches('/');
    catalog
        .iter()
        .find(|skill| skill.path == query || skill.name == query)
}

/// Resolves requested names to catalog paths, rejecting any it does not know.
pub fn resolve_builtin_skills(
    catalog: &[BuiltinSkill],
    requested: &[String],
) -> Result<Vec<String>, ImruleError> {
    let mut resolved = Vec::new();
    let mut unknown = Vec::new();
    for query in requested {
        match find_builtin_skill(catalog, query) {
            Some(skill) if !resolved.contains(&skill.path) => resolved.push(skill.path.clone()),
            Some(_) => {}
            None => unknown.push(query.as_str()),
        }
    }
    if !unknown.is_empty() {
        let available: Vec<&str> = catalog.iter().map(|skill| skill.path.as_str()).collect();
        return Err(ImruleError::skills(format!(
            "unknown built-in skill: {}. Available: {}",
            unknown.join(", "),
            available.join(", ")
        )));
    }
    Ok(resolved)
}

/// What the infrastructure layer found in a project, for detection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectSignals {
    pub cargo: bool,
    pub pyproject: bool,
    pub makefile: bool,
    /// A `Dockerfile` or compose file.
    pub docker: bool,
    pub github_workflows: bool,
    /// A `.vscode/` directory.
    pub vscode: bool,
    pub version_file: bool,
    pub changelog: bool,
    /// Crate names from `[dependencies]` / `[workspace.dependencies]`.
    pub rust_dependencies: BTreeSet<String>,
    /// A binary target: `src/main.rs`, `src/bin/`, or `[[bin]]`.
    pub rust_binary: bool,
    /// Normalized distribution names from `[project] dependencies` and extras.
    pub python_dependencies: BTreeSet<String>,
    /// A non-empty `[project.scripts]`.
    pub python_scripts: bool,
}

const RUST_CLI_CRATES: &[&str] = &["clap", "argh", "bpaf", "lexopt", "pico-args"];
const RUST_SERVER_CRATES: &[&str] = &[
    "axum",
    "actix-web",
    "hyper",
    "poem",
    "rocket",
    "salvo",
    "tonic",
    "warp",
];
const PYTHON_CLI_PACKAGES: &[&str] = &["click", "cyclopts", "fire", "typer"];
const PYTHON_SERVER_PACKAGES: &[&str] = &[
    "aiohttp",
    "django",
    "fastapi",
    "flask",
    "granian",
    "litestar",
    "sanic",
    "starlette",
    "uvicorn",
];

/// Project kinds inferred from [`ProjectSignals`].
#[derive(Debug, Clone, Copy)]
struct Detection {
    rust_cli: bool,
    rust_server: bool,
    python_cli: bool,
    python_server: bool,
}

impl Detection {
    fn cli(self) -> bool {
        self.rust_cli || self.python_cli
    }

    fn server(self) -> bool {
        self.rust_server || self.python_server
    }

    fn from_signals(signals: &ProjectSignals) -> Self {
        let has = |set: &BTreeSet<String>, names: &[&str]| names.iter().any(|n| set.contains(*n));
        let rust_server = signals.cargo && has(&signals.rust_dependencies, RUST_SERVER_CRATES);
        let rust_cli = signals.cargo
            && (has(&signals.rust_dependencies, RUST_CLI_CRATES)
                || (signals.rust_binary && !rust_server));
        let python_server =
            signals.pyproject && has(&signals.python_dependencies, PYTHON_SERVER_PACKAGES);
        let python_cli = signals.pyproject
            && (signals.python_scripts || has(&signals.python_dependencies, PYTHON_CLI_PACKAGES));
        Self {
            rust_cli,
            rust_server,
            python_cli,
            python_server,
        }
    }
}

/// Short labels describing what was detected, for display.
pub fn detection_labels(signals: &ProjectSignals) -> Vec<&'static str> {
    let detection = Detection::from_signals(signals);
    let mut labels = Vec::new();
    if signals.cargo {
        labels.push("rust");
    }
    if signals.pyproject {
        labels.push("python");
    }
    if detection.cli() {
        labels.push("cli");
    }
    if detection.server() {
        labels.push("server");
    }
    if signals.makefile {
        labels.push("makefile");
    }
    if signals.docker {
        labels.push("docker");
    }
    if signals.github_workflows {
        labels.push("github-actions");
    }
    labels
}

/// Built-in skill paths that fit a project.
pub fn recommend_builtin_skills(signals: &ProjectSignals) -> BTreeSet<&'static str> {
    let detection = Detection::from_signals(signals);
    let server = detection.server();
    let rules = [
        ("cli", detection.cli()),
        ("server", server),
        (
            "make/setup",
            signals.makefile || signals.cargo || signals.pyproject,
        ),
        ("python/cli", detection.python_cli),
        ("python/server", detection.python_server),
        ("rust/cli", detection.rust_cli),
        ("rust/server", detection.rust_server),
        (
            "release/versioning",
            signals.version_file || signals.changelog || signals.cargo || signals.pyproject,
        ),
        ("ci/github-actions", signals.github_workflows),
        ("docker/setup", signals.docker || server),
        // Optimizing needs an image to measure; a server without one starts
        // from docker/setup.
        ("docker/optimize", signals.docker),
        (
            "vscode/setup",
            signals.vscode || signals.cargo || signals.pyproject,
        ),
        // Reporting imrule's own problems applies to every project using it.
        ("imrule-issue", true),
    ];
    rules
        .into_iter()
        .filter(|(_, fits)| *fits)
        .map(|(path, _)| path)
        .collect()
}

/// Litter an operating system or a checker run leaves in a skill folder
/// (`path` is relative, with forward slashes): never part of an embedded
/// skill, never user content. Other hidden files, such as `.env`, are the
/// user's.
fn is_incidental_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini")
        || path.split('/').any(|part| part == "__pycache__")
}

/// Whether frontmatter metadata carries the built-in marker.
fn is_builtin_marked(meta: &serde_json::Value) -> bool {
    match meta
        .get("metadata")
        .and_then(|metadata| metadata.get(BUILTIN_SKILL_MARKER_KEY))
    {
        Some(serde_json::Value::String(text)) => text.trim() == "true",
        Some(serde_json::Value::Bool(marked)) => *marked,
        _ => false,
    }
}

/// How an installed copy of a built-in skill compares to the embedded one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSkillState {
    /// Nothing exists at the skill's install path.
    NotInstalled,
    /// Every embedded file matches what is installed, and nothing else is there.
    UpToDate,
    /// Installed by ImRule from an older revision (marker set, revision at
    /// least 1 and below the embedded one) with no files beyond the embedded
    /// ones; safe to refresh.
    Outdated,
    /// Anything else that differs: edited or added to locally, a directory or
    /// `SKILL.md` the user owns, or a copy from a newer ImRule. Only
    /// overwritten with the user's consent.
    Modified,
}

impl BuiltinSkillState {
    /// Decides the state of an installed copy.
    ///
    /// `installed_files` lists every file below the skill's install directory,
    /// relative to it with forward slashes, or is `None` when nothing exists
    /// at that path. `installed` reads one of those files by relative path.
    pub fn compare(
        skill: &BuiltinSkill,
        installed_files: Option<&[String]>,
        installed: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let Some(installed_files) = installed_files else {
            return Self::NotInstalled;
        };
        // A directory without a readable SKILL.md is not a copy ImRule
        // installed — a grouping folder of the user's own skills, say.
        let Some(installed_skill_md) = installed(SKILL_MD_FILENAME) else {
            return Self::Modified;
        };
        // A file the embedded skill does not ship was added by the user, and
        // replacing the directory would delete it. Only operating-system litter
        // (`.DS_Store`) and a checker's `__pycache__` are left out.
        let embedded: BTreeSet<&str> = skill.files.iter().map(|(path, _)| path.as_str()).collect();
        if installed_files
            .iter()
            .filter(|file| !is_incidental_file(file))
            .any(|file| !embedded.contains(file.as_str()))
        {
            return Self::Modified;
        }
        let identical = skill
            .files
            .iter()
            .all(|(relative, content)| installed(relative).as_deref() == Some(*content));
        if identical {
            return Self::UpToDate;
        }
        let Ok(Some(parsed)) = parse_frontmatter(&installed_skill_md) else {
            return Self::Modified;
        };
        let revision = revision_of(&parsed.meta);
        // An embedded file missing from an older copy may be one the newer
        // revision added, so only extra files rule a refresh out.
        if is_builtin_marked(&parsed.meta) && revision >= 1 && revision < skill.revision {
            Self::Outdated
        } else {
            Self::Modified
        }
    }

    /// Stable identifier for machine-readable output (`--json`).
    pub fn label(self) -> &'static str {
        match self {
            Self::NotInstalled => "not-installed",
            Self::UpToDate => "up-to-date",
            Self::Outdated => "outdated",
            Self::Modified => "modified",
        }
    }

    /// Human-facing tag for listings; `None` when there is nothing to show.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::NotInstalled => None,
            Self::UpToDate => Some("installed"),
            Self::Outdated => Some("update available"),
            Self::Modified => Some("modified locally"),
        }
    }
}

/// What `setup` did with one selected skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSkillSetupStatus {
    Installed,
    Updated,
    Unchanged,
    /// Left alone because it was modified locally and overwriting was not asked for.
    SkippedModified,
}

impl BuiltinSkillSetupStatus {
    /// Whether this outcome writes the skill to disk.
    pub fn writes(self) -> bool {
        matches!(self, Self::Installed | Self::Updated)
    }

    /// Human-facing label for the install report, `would …` under `dry_run`.
    pub fn label(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Installed, false) => "installed",
            (Self::Installed, true) => "would install",
            (Self::Updated, false) => "updated",
            (Self::Updated, true) => "would update",
            (Self::Unchanged, _) => "unchanged",
            (Self::SkippedModified, _) => {
                "modified locally, skipped — pass --force or toggle it on individually in the picker"
            }
        }
    }
}

/// A normalized Python distribution name from a PEP 508 requirement string
/// (`FastAPI[standard]>=0.1` → `fastapi`).
pub fn python_requirement_name(requirement: &str) -> Option<String> {
    let end = requirement
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .unwrap_or(requirement.len());
    let name = requirement[..end].trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_ascii_lowercase().replace(['_', '.'], "-"))
}
