//! The built-in skills embedded at build time, and project signal collection
//! for detecting which of them fit.

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::builtin_skills::{
    BuiltinSkill, ProjectSignals, build_builtin_catalog, python_requirement_name,
};

include!(concat!(env!("OUT_DIR"), "/builtin_skills.rs"));

/// The catalog of skills compiled into this binary.
pub fn builtin_catalog() -> Vec<BuiltinSkill> {
    build_builtin_catalog(BUILTIN_SKILL_FILES)
}

/// Reads the files that reveal what kind of project `root` is. Looks at
/// `root`, `root/*` and `root/*/*` (`crates/*`, `apps/*/server`, …) so
/// workspaces are recognized by their members. Missing or unparseable files
/// count as absent.
pub fn collect_project_signals(root: &Path) -> ProjectSignals {
    let mut signals = ProjectSignals {
        makefile: root.join("Makefile").is_file() || root.join("GNUmakefile").is_file(),
        docker: [
            "Dockerfile",
            "compose.yaml",
            "compose.yml",
            "docker-compose.yml",
            "docker-compose.yaml",
        ]
        .iter()
        .any(|name| root.join(name).is_file())
            || root.join("docker").is_dir(),
        github_workflows: has_workflows(&root.join(".github/workflows")),
        vscode: root.join(".vscode").is_dir(),
        version_file: root.join("VERSION").is_file(),
        changelog: root.join("CHANGELOG.md").is_file(),
        ..ProjectSignals::default()
    };

    let (cargo_manifests, pyproject_manifests) = manifests(root);
    for manifest in cargo_manifests {
        signals.cargo = true;
        let Some(table) = read_toml(&manifest) else {
            continue;
        };
        for section in [
            table.get("dependencies"),
            table.get("workspace").and_then(|w| w.get("dependencies")),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(deps) = section.as_table() {
                signals.rust_dependencies.extend(deps.keys().cloned());
            }
        }
        let crate_dir = manifest.parent().unwrap_or(root);
        if table.get("bin").is_some()
            || crate_dir.join("src/main.rs").is_file()
            || crate_dir.join("src/bin").is_dir()
        {
            signals.rust_binary = true;
        }
    }

    for manifest in pyproject_manifests {
        signals.pyproject = true;
        let Some(table) = read_toml(&manifest) else {
            continue;
        };
        let Some(project) = table.get("project") else {
            continue;
        };
        let mut requirements: Vec<&str> = project
            .get("dependencies")
            .and_then(|deps| deps.as_array())
            .map(|deps| deps.iter().filter_map(|d| d.as_str()).collect())
            .unwrap_or_default();
        if let Some(extras) = project
            .get("optional-dependencies")
            .and_then(|e| e.as_table())
        {
            for group in extras.values().filter_map(|g| g.as_array()) {
                requirements.extend(group.iter().filter_map(|d| d.as_str()));
            }
        }
        signals
            .python_dependencies
            .extend(requirements.into_iter().filter_map(python_requirement_name));
        if project
            .get("scripts")
            .and_then(|s| s.as_table())
            .is_some_and(|s| !s.is_empty())
        {
            signals.python_scripts = true;
        }
    }

    signals
}

/// The `Cargo.toml` and `pyproject.toml` files at `root`, `root/*`, and
/// `root/*/*`, found in one walk that skips hidden, vendored, and build
/// directories.
fn manifests(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut cargo = Vec::new();
    let mut pyproject = Vec::new();
    let mut frontier = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = frontier.pop() {
        for (file, found) in [
            ("Cargo.toml", &mut cargo),
            ("pyproject.toml", &mut pyproject),
        ] {
            let candidate = dir.join(file);
            if candidate.is_file() {
                found.push(candidate);
            }
        }
        if depth == 2 {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let skipped = name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "target" | "node_modules" | "thirdparty" | "references" | "vendor" | "dist"
                );
            if skipped {
                continue;
            }
            // The entry's type comes with the listing; only a symlink needs a
            // stat to learn whether it points at a directory.
            let is_dir = entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()));
            if is_dir {
                frontier.push((entry.path(), depth + 1));
            }
        }
    }
    cargo.sort();
    pyproject.sort();
    (cargo, pyproject)
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    fs::read_to_string(path).ok()?.parse::<toml::Table>().ok()
}

fn has_workflows(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.ends_with(".yml") || name.ends_with(".yaml")
        })
    })
}
