//! Contract for `imrule skills setup`: the built-in skills compiled into the
//! binary, project detection, install/update semantics, and the picker.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use imrule::application::ports::FileSystemPort;
use imrule::application::skills_setup_use_case::{
    SkillsSetupOptions, SkillsSetupPlan, SkillsSetupUseCase, skills_project_root,
};
use imrule::domain::builtin_skills::{
    BuiltinSkill, BuiltinSkillSetupStatus, BuiltinSkillState, ProjectSignals,
    build_builtin_catalog, detection_labels, installed_skill_revision, python_requirement_name,
    recommend_builtin_skills, resolve_builtin_skills,
};
use imrule::domain::skills::flatten_skill_name;
use imrule::domain::subagent::parse_frontmatter;
use imrule::infrastructure::builtin_skills::{builtin_catalog, collect_project_signals};
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::interface::skill_picker::{
    Picker, PickerAction, PickerItem, PickerKey, PickerSelection, display_width, picker_key,
    truncate,
};
use tempfile::tempdir;

// ---------------------------------------------------------------- catalog ---

const ALPHA_V1: &str = "---\nname: lang-alpha\ndescription: \"Alpha skill\"\nmetadata:\n  imrule-builtin: \"true\"\n  imrule-skill-version: \"1\"\n---\nbody v1\n";
const ALPHA_V2: &str = "---\nname: lang-alpha\ndescription: \"Alpha skill\"\nmetadata:\n  imrule-builtin: \"true\"\n  imrule-skill-version: \"2\"\n---\nbody v2\n";

fn catalog_v1() -> Vec<BuiltinSkill> {
    build_builtin_catalog(&[
        ("README.md", "authoring guide, not a skill"),
        ("lang/alpha/SKILL.md", ALPHA_V1),
        ("lang/alpha/scripts/check.py", "print('v1')\n"),
        ("lang/alpha/scripts/old.py", "print('dropped in v2')\n"),
        ("beta/SKILL.md", "---\nname: beta\ndescription: Beta\n---\n"),
    ])
}

fn catalog_v2() -> Vec<BuiltinSkill> {
    build_builtin_catalog(&[
        ("lang/alpha/SKILL.md", ALPHA_V2),
        ("lang/alpha/scripts/check.py", "print('v2')\n"),
        ("beta/SKILL.md", "---\nname: beta\ndescription: Beta\n---\n"),
    ])
}

#[test]
fn catalog_groups_files_under_the_skill_that_owns_them() {
    let catalog = catalog_v1();
    let summary: Vec<(&str, &str, &str, u32)> = catalog
        .iter()
        .map(|s| {
            (
                s.path.as_str(),
                s.name.as_str(),
                s.description.as_str(),
                s.revision,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("beta", "beta", "Beta", 0),
            ("lang/alpha", "lang-alpha", "Alpha skill", 1),
        ]
    );
    let alpha_files: Vec<&str> = catalog[1].files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        alpha_files,
        vec!["SKILL.md", "scripts/check.py", "scripts/old.py"]
    );
}

#[test]
fn skills_resolve_by_path_or_published_name_and_unknown_ones_are_reported() {
    let catalog = catalog_v1();
    assert_eq!(
        resolve_builtin_skills(
            &catalog,
            &["lang-alpha".into(), "beta/".into(), "lang/alpha".into()]
        )
        .unwrap(),
        vec!["lang/alpha", "beta"]
    );
    let error = resolve_builtin_skills(&catalog, &["nope".into()])
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown built-in skill: nope"), "{error}");
    assert!(error.contains("beta, lang/alpha"), "{error}");
}

#[test]
fn revisions_parse_from_strings_or_numbers_and_fall_back_to_zero() {
    let catalog = build_builtin_catalog(&[
        (
            "bad/SKILL.md",
            "---\nname: bad\ndescription: Bad\nmetadata:\n  imrule-skill-version: \"seven\"\n---\n",
        ),
        (
            "num/SKILL.md",
            "---\nname: num\ndescription: \"  Numeric  \"\nmetadata:\n  imrule-skill-version: 7\n---\n",
        ),
        ("orphan/notes.md", "belongs to no skill"),
        ("outer/SKILL.md", "---\nname: outer\n---\n"),
        ("outer/inner/SKILL.md", "---\nname: outer-inner\n---\n"),
        ("outer/inner/ref.md", "inner"),
        ("outer/notes.md", "outer"),
        ("plain/SKILL.md", "no frontmatter at all\n"),
    ]);
    let summary: Vec<(&str, &str, &str, u32)> = catalog
        .iter()
        .map(|s| {
            (
                s.path.as_str(),
                s.name.as_str(),
                s.description.as_str(),
                s.revision,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("bad", "bad", "Bad", 0),
            ("num", "num", "Numeric", 7),
            ("outer", "outer", "", 0),
            ("outer/inner", "outer-inner", "", 0),
            ("plain", "plain", "", 0),
        ]
    );
    // A file belongs to the deepest skill containing it, never to both.
    let files = |path: &str| -> Vec<String> {
        catalog
            .iter()
            .find(|s| s.path == path)
            .unwrap()
            .files
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    };
    assert_eq!(files("outer"), vec!["SKILL.md", "notes.md"]);
    assert_eq!(files("outer/inner"), vec!["SKILL.md", "ref.md"]);

    let with = |value: &str| format!("---\nmetadata:\n  imrule-skill-version: {value}\n---\n");
    assert_eq!(installed_skill_revision(&with("\" 12 \"")), 12);
    assert_eq!(installed_skill_revision(&with("3")), 3);
    assert_eq!(installed_skill_revision(&with("-1")), 0);
    assert_eq!(installed_skill_revision(&with("4294967296")), 0);
    assert_eq!(installed_skill_revision("# no frontmatter\n"), 0);
    assert_eq!(installed_skill_revision("---\nname: x\n---\n"), 0);
}

// -------------------------------------------------------------- detection ---

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn detection_recommends_skills_that_fit_the_project() {
    let rust_cli = ProjectSignals {
        cargo: true,
        rust_binary: true,
        rust_dependencies: set(&["clap", "serde"]),
        github_workflows: true,
        ..ProjectSignals::default()
    };
    assert_eq!(
        recommend_builtin_skills(&rust_cli),
        BTreeSet::from([
            "ci/github-actions",
            "cli",
            "imrule-issue",
            "make/setup",
            "release/versioning",
            "rust/cli",
            "vscode/setup"
        ])
    );

    let rust_server = ProjectSignals {
        cargo: true,
        rust_binary: true,
        rust_dependencies: set(&["axum", "tokio"]),
        ..ProjectSignals::default()
    };
    let recommended = recommend_builtin_skills(&rust_server);
    assert!(recommended.contains("rust/server") && recommended.contains("docker/setup"));
    assert!(
        !recommended.contains("rust/cli"),
        "a server binary without a CLI crate is not a CLI"
    );

    let python_both = ProjectSignals {
        pyproject: true,
        python_scripts: true,
        python_dependencies: set(&["fastapi", "pydantic"]),
        ..ProjectSignals::default()
    };
    let recommended = recommend_builtin_skills(&python_both);
    for path in ["python/cli", "python/server", "cli", "server"] {
        assert!(recommended.contains(path), "missing {path}");
    }

    // The issue-reporting skill is about imrule itself, so it fits everywhere.
    assert_eq!(
        recommend_builtin_skills(&ProjectSignals::default()),
        BTreeSet::from(["imrule-issue"])
    );
}

#[test]
fn every_detection_rule_names_a_shipped_skill_and_every_skill_has_one() {
    // A project showing every signal is recommended every rule's skill, so a
    // renamed skill leaves a dead rule and a new skill without a rule fails here.
    let everything = ProjectSignals {
        cargo: true,
        pyproject: true,
        makefile: true,
        docker: true,
        github_workflows: true,
        vscode: true,
        version_file: true,
        changelog: true,
        rust_dependencies: set(&["clap", "axum"]),
        rust_binary: true,
        python_dependencies: set(&["typer", "fastapi"]),
        python_scripts: true,
    };
    let catalog = builtin_catalog();
    let shipped: BTreeSet<&str> = catalog.iter().map(|skill| skill.path.as_str()).collect();
    assert_eq!(recommend_builtin_skills(&everything), shipped);
}

#[test]
fn python_requirements_normalize_to_distribution_names() {
    assert_eq!(
        python_requirement_name("FastAPI[standard]>=0.141").as_deref(),
        Some("fastapi")
    );
    assert_eq!(
        python_requirement_name("pydantic_settings ; python_version>'3.12'").as_deref(),
        Some("pydantic-settings")
    );
    assert_eq!(python_requirement_name(">=1"), None);
}

#[test]
fn project_signals_are_read_from_workspace_members_too() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("crates/app-server/src")).unwrap();
    fs::write(
        root.join("crates/app-server/Cargo.toml"),
        "[package]\nname = \"app-server\"\n\n[dependencies]\naxum = \"0.8\"\n",
    )
    .unwrap();
    fs::write(root.join("crates/app-server/src/main.rs"), "fn main() {}").unwrap();
    fs::create_dir_all(root.join("tools")).unwrap();
    fs::write(
        root.join("tools/pyproject.toml"),
        "[project]\nname = \"tools\"\ndependencies = [\"typer>=0.27\"]\n[project.scripts]\ntools = \"tools.cli:app\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::write(root.join(".github/workflows/ci.yml"), "on: push\n").unwrap();
    fs::create_dir_all(root.join(".vscode")).unwrap();
    fs::write(root.join("Dockerfile"), "FROM scratch\n").unwrap();
    // Vendored code must not make the project look like something it is not.
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::write(root.join("node_modules/pkg/pyproject.toml"), "[project]\n").unwrap();

    let signals = collect_project_signals(root);
    assert!(signals.cargo && signals.rust_binary && signals.docker && signals.github_workflows);
    assert!(signals.vscode);
    assert!(signals.rust_dependencies.contains("axum"));
    assert!(signals.pyproject && signals.python_scripts);
    assert!(signals.python_dependencies.contains("typer"));
    assert!(!signals.makefile && !signals.version_file);
}

#[test]
fn project_signals_cover_every_marker_and_ignore_what_does_not_count() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let write = |relative: &str, content: &str| {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    };
    write(
        "Cargo.toml",
        "[workspace.dependencies]\nclap = \"4\"\n\n[[bin]]\nname = \"tool\"\npath = \"tool.rs\"\n",
    );
    // Unparseable: still a Rust project, but it contributes nothing else.
    write("broken/Cargo.toml", "[dependencies\naxum = ");
    // Too deep, inside a build directory, or hidden: not part of the project.
    write("a/b/c/Cargo.toml", "[dependencies]\nrocket = \"0.5\"\n");
    write("target/pkg/Cargo.toml", "[dependencies]\nhyper = \"1\"\n");
    write(
        ".hidden/pyproject.toml",
        "[project]\ndependencies = [\"flask\"]\n",
    );
    write(
        "py/pyproject.toml",
        "[project]\nname = \"py\"\n\n[project.optional-dependencies]\nserve = [\"Uvicorn[standard]>=0.30\"]\n\n[project.scripts]\n",
    );
    write("GNUmakefile", "all:\n");
    write("compose.yaml", "services: {}\n");
    write("VERSION", "1.0.0\n");
    write("CHANGELOG.md", "# Changelog\n");
    write(".github/workflows/README.md", "not a workflow\n");

    let signals = collect_project_signals(root);
    assert!(signals.cargo && signals.rust_binary);
    assert_eq!(signals.rust_dependencies, set(&["clap"]));
    assert!(signals.pyproject);
    assert_eq!(signals.python_dependencies, set(&["uvicorn"]));
    assert!(
        !signals.python_scripts,
        "an empty [project.scripts] declares no command"
    );
    assert!(signals.makefile && signals.docker && signals.version_file && signals.changelog);
    assert!(
        !signals.github_workflows,
        "only .yml/.yaml files are workflows"
    );
    assert!(!signals.vscode);

    let other = tempdir().unwrap();
    let root = other.path();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    fs::create_dir_all(root.join("src/bin")).unwrap();
    fs::create_dir_all(root.join("docker")).unwrap();
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::write(root.join(".github/workflows/ci.yaml"), "on: push\n").unwrap();
    let signals = collect_project_signals(root);
    assert!(signals.rust_binary && signals.docker && signals.github_workflows);
    assert!(signals.rust_dependencies.is_empty() && !signals.makefile);
}

#[test]
fn detection_labels_name_each_detected_kind_in_a_stable_order() {
    let everything = ProjectSignals {
        cargo: true,
        pyproject: true,
        makefile: true,
        docker: true,
        github_workflows: true,
        vscode: true,
        version_file: true,
        changelog: true,
        rust_dependencies: set(&["axum"]),
        rust_binary: false,
        python_dependencies: set(&["typer"]),
        python_scripts: false,
    };
    assert_eq!(
        detection_labels(&everything),
        vec![
            "rust",
            "python",
            "cli",
            "server",
            "makefile",
            "docker",
            "github-actions"
        ]
    );
    assert!(detection_labels(&ProjectSignals::default()).is_empty());

    // A Python web app with no command entry point is a server, not a CLI.
    let python_server = ProjectSignals {
        pyproject: true,
        python_dependencies: set(&["django"]),
        ..ProjectSignals::default()
    };
    assert_eq!(detection_labels(&python_server), vec!["python", "server"]);
    let recommended = recommend_builtin_skills(&python_server);
    assert!(recommended.contains("python/server") && recommended.contains("docker/setup"));
    for absent in ["python/cli", "cli", "docker/optimize"] {
        assert!(!recommended.contains(absent), "{absent} recommended");
    }

    // A Rust server that also parses arguments with clap is both.
    let rust_both = ProjectSignals {
        cargo: true,
        rust_binary: true,
        rust_dependencies: set(&["axum", "clap"]),
        ..ProjectSignals::default()
    };
    let recommended = recommend_builtin_skills(&rust_both);
    assert!(recommended.contains("rust/cli") && recommended.contains("rust/server"));

    // Dependencies or scripts without their manifest detect nothing.
    let orphaned = ProjectSignals {
        rust_dependencies: set(&["clap"]),
        python_scripts: true,
        ..ProjectSignals::default()
    };
    assert!(detection_labels(&orphaned).is_empty());
}

// ------------------------------------------------------------ install/update ---

fn options(root: &Path) -> SkillsSetupOptions {
    SkillsSetupOptions {
        project_root: root.to_path_buf(),
        global: false,
    }
}

fn project() -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    tmp
}

#[test]
fn installing_then_rerunning_reports_unchanged() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);

    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert!(
        plan.entries
            .iter()
            .all(|e| e.state == BuiltinSkillState::NotInstalled)
    );

    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::Installed
    );
    assert!(result.changed());
    assert_eq!(
        fs::read_to_string(root.join(".imrule/skills/lang/alpha/scripts/check.py")).unwrap(),
        "print('v1')\n"
    );

    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::Unchanged
    );
    assert!(!result.changed());
}

/// Revision 1 of `lang/alpha` shipping the same files as revision 2.
fn catalog_v1_same_files() -> Vec<BuiltinSkill> {
    build_builtin_catalog(&[
        ("lang/alpha/SKILL.md", ALPHA_V1),
        ("lang/alpha/scripts/check.py", "print('v1')\n"),
    ])
}

/// Installs `catalog`'s `lang/alpha` into the project at `root`.
fn install_alpha(root: &Path, catalog: &[BuiltinSkill]) {
    let fs_port = FsFileSystem::new();
    let use_case = SkillsSetupUseCase::new(&fs_port, catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
}

fn state_in(plan: &SkillsSetupPlan, path: &str) -> BuiltinSkillState {
    plan.entries
        .iter()
        .find(|e| e.skill.path == path)
        .unwrap()
        .state
}

#[test]
fn an_older_revision_is_refreshed_only_when_it_holds_no_extra_files() {
    let fs_port = FsFileSystem::new();
    let new = catalog_v2();
    let use_case = SkillsSetupUseCase::new(&fs_port, &new);
    let alpha = ["lang/alpha".to_string()];

    // Same files, older revision: refreshed without --force.
    let tmp = project();
    let root = tmp.path();
    install_alpha(root, &catalog_v1_same_files());
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "lang/alpha"), BuiltinSkillState::Outdated);
    let result = use_case.install(&plan, &alpha, false, false).unwrap();
    assert_eq!(result.outcomes[0].status, BuiltinSkillSetupStatus::Updated);
    assert_eq!(
        fs::read_to_string(root.join(".imrule/skills/lang/alpha/scripts/check.py")).unwrap(),
        "print('v2')\n"
    );

    // Revision 1 shipped scripts/old.py, which revision 2 dropped. Setup cannot
    // tell it from a file the user added, so replacing the directory — which
    // deletes it — waits for --force.
    let tmp = project();
    let root = tmp.path();
    install_alpha(root, &catalog_v1());
    let skill_dir = root.join(".imrule/skills/lang/alpha");
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "lang/alpha"), BuiltinSkillState::Modified);
    let result = use_case.install(&plan, &alpha, false, false).unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::SkippedModified
    );
    assert!(skill_dir.join("scripts/old.py").is_file());
    assert_eq!(
        fs::read_to_string(skill_dir.join("scripts/check.py")).unwrap(),
        "print('v1')\n"
    );

    let result = use_case.install(&plan, &alpha, true, false).unwrap();
    assert_eq!(result.outcomes[0].status, BuiltinSkillSetupStatus::Updated);
    assert_eq!(
        fs::read_to_string(skill_dir.join("scripts/check.py")).unwrap(),
        "print('v2')\n"
    );
    assert!(!skill_dir.join("scripts/old.py").exists());
}

#[test]
fn an_outdated_built_in_with_a_file_the_user_added_is_kept_until_forced() {
    let tmp = project();
    let root = tmp.path();
    install_alpha(root, &catalog_v1_same_files());
    let skill_dir = root.join(".imrule/skills/lang/alpha");
    let notes = skill_dir.join("references/notes.md");
    fs::create_dir_all(notes.parent().unwrap()).unwrap();
    fs::write(&notes, "my notes\n").unwrap();

    let fs_port = FsFileSystem::new();
    let new = catalog_v2();
    let use_case = SkillsSetupUseCase::new(&fs_port, &new);
    let alpha = ["lang/alpha".to_string()];
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "lang/alpha"), BuiltinSkillState::Modified);

    let result = use_case.install(&plan, &alpha, false, false).unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::SkippedModified
    );
    assert!(!result.changed());
    assert_eq!(fs::read_to_string(&notes).unwrap(), "my notes\n");
    assert_eq!(
        fs::read_to_string(skill_dir.join("scripts/check.py")).unwrap(),
        "print('v1')\n"
    );

    let result = use_case.install(&plan, &alpha, true, false).unwrap();
    assert_eq!(result.outcomes[0].status, BuiltinSkillSetupStatus::Updated);
    assert!(!notes.exists());
    assert_eq!(
        fs::read_to_string(skill_dir.join("scripts/check.py")).unwrap(),
        "print('v2')\n"
    );
}

#[test]
fn a_skill_the_user_wrote_at_a_built_in_path_is_never_replaced_without_force() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v2();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let alpha = ["lang/alpha".to_string()];
    let skill_md = root.join(".imrule/skills/lang/alpha/SKILL.md");
    fs::create_dir_all(skill_md.parent().unwrap()).unwrap();

    for (mine, why) in [
        (
            "---\nname: lang-alpha\ndescription: Mine\n---\nmine\n",
            "no marker and no revision",
        ),
        (
            "---\nname: lang-alpha\nmetadata:\n  imrule-skill-version: \"1\"\n---\nmine\n",
            "a revision without the built-in marker",
        ),
        (
            "---\nname: lang-alpha\nmetadata:\n  imrule-builtin: \"true\"\n---\nmine\n",
            "the marker without a revision",
        ),
        ("# no frontmatter\n", "no frontmatter at all"),
    ] {
        fs::write(&skill_md, mine).unwrap();
        let plan = use_case.plan(&options(root), &ProjectSignals::default());
        assert_eq!(
            state_in(&plan, "lang/alpha"),
            BuiltinSkillState::Modified,
            "{why}"
        );
        let result = use_case.install(&plan, &alpha, false, false).unwrap();
        assert_eq!(
            result.outcomes[0].status,
            BuiltinSkillSetupStatus::SkippedModified,
            "{why}"
        );
        assert_eq!(fs::read_to_string(&skill_md).unwrap(), mine, "{why}");
        assert!(!root.join(".imrule/skills/lang/alpha/scripts").exists());
    }
}

#[test]
fn a_directory_without_skill_md_at_a_built_in_path_is_left_alone() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    // `beta` is a built-in path; here it is the user's grouping folder.
    let nested = root.join(".imrule/skills/beta/auth/SKILL.md");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::write(&nested, "---\nname: beta-auth\n---\nmine\n").unwrap();

    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "beta"), BuiltinSkillState::Modified);
    let result = use_case
        .install(&plan, &["beta".to_string()], false, false)
        .unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::SkippedModified
    );
    assert!(nested.is_file());
    assert!(!root.join(".imrule/skills/beta/SKILL.md").exists());
}

#[cfg(unix)]
#[test]
fn a_built_in_path_reached_through_a_symlink_is_not_emptied() {
    let tmp = project();
    let root = tmp.path();
    let outside = tempdir().unwrap();
    let kept = outside.path().join("alpha/notes.md");
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::write(&kept, "outside the project\n").unwrap();
    fs::create_dir_all(root.join(".imrule/skills")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join(".imrule/skills/lang")).unwrap();

    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "lang/alpha"), BuiltinSkillState::Modified);
    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::SkippedModified
    );
    assert_eq!(fs::read_to_string(&kept).unwrap(), "outside the project\n");
}

#[test]
fn install_refuses_to_replace_what_appeared_after_the_plan() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(
        state_in(&plan, "lang/alpha"),
        BuiltinSkillState::NotInstalled
    );

    // The user writes a skill there while the picker is open.
    let mine = root.join(".imrule/skills/lang/alpha/SKILL.md");
    fs::create_dir_all(mine.parent().unwrap()).unwrap();
    fs::write(&mine, "mine\n").unwrap();

    let error = use_case
        .install(&plan, &["lang/alpha".to_string()], true, false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("changed while setting up"), "{error}");
    assert_eq!(fs::read_to_string(&mine).unwrap(), "mine\n");
}

#[test]
fn only_consented_paths_overwrite_a_locally_modified_skill() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    use_case
        .install(&plan, &plan.all_paths(), false, false)
        .unwrap();
    let beta = root.join(".imrule/skills/beta/SKILL.md");
    let check = root.join(".imrule/skills/lang/alpha/scripts/check.py");
    fs::write(&beta, "---\nname: beta\n---\nmy beta\n").unwrap();
    fs::write(&check, "print('my tweak')\n").unwrap();

    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(state_in(&plan, "beta"), BuiltinSkillState::Modified);
    assert_eq!(state_in(&plan, "lang/alpha"), BuiltinSkillState::Modified);

    let result = use_case
        .install_with_consent(
            &plan,
            &["beta".to_string(), "lang/alpha".to_string()],
            &["lang/alpha".to_string()],
            false,
        )
        .unwrap();
    let statuses: Vec<(&str, BuiltinSkillSetupStatus)> = result
        .outcomes
        .iter()
        .map(|o| (o.path.as_str(), o.status))
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("beta", BuiltinSkillSetupStatus::SkippedModified),
            ("lang/alpha", BuiltinSkillSetupStatus::Updated),
        ]
    );
    assert_eq!(
        fs::read_to_string(&beta).unwrap(),
        "---\nname: beta\n---\nmy beta\n"
    );
    assert_eq!(fs::read_to_string(&check).unwrap(), "print('v1')\n");
}

#[test]
fn list_files_walks_the_tree_without_following_symlinked_directories() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("skill");
    fs::create_dir_all(dir.join("scripts/nested")).unwrap();
    fs::create_dir_all(dir.join("empty")).unwrap();
    fs::write(dir.join("SKILL.md"), "skill").unwrap();
    fs::write(dir.join(".hidden"), "hidden").unwrap();
    fs::write(dir.join("scripts/nested/deep.py"), "deep").unwrap();
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.md"), "not in the skill").unwrap();

    let mut expected: Vec<PathBuf> = [".hidden", "SKILL.md", "scripts/nested/deep.py"]
        .iter()
        .map(PathBuf::from)
        .collect();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, dir.join("linked")).unwrap();
        expected.push(PathBuf::from("linked"));
    }
    expected.sort();

    let fs_port = FsFileSystem::new();
    assert_eq!(fs_port.list_files(&dir).unwrap(), expected);
    assert!(fs_port.list_files(&tmp.path().join("missing")).is_err());
}

#[test]
fn a_locally_modified_skill_is_only_overwritten_on_request() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    let check = root.join(".imrule/skills/lang/alpha/scripts/check.py");
    fs::write(&check, "print('my tweak')\n").unwrap();

    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    assert_eq!(plan.entries[1].state, BuiltinSkillState::Modified);

    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    assert_eq!(
        result.outcomes[0].status,
        BuiltinSkillSetupStatus::SkippedModified
    );
    assert_eq!(fs::read_to_string(&check).unwrap(), "print('my tweak')\n");

    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], true, false)
        .unwrap();
    assert_eq!(result.outcomes[0].status, BuiltinSkillSetupStatus::Updated);
    assert_eq!(fs::read_to_string(&check).unwrap(), "print('v1')\n");
}

#[test]
fn dry_run_reports_without_writing() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    let result = use_case
        .install(&plan, &plan.all_paths(), false, true)
        .unwrap();
    assert!(
        result
            .outcomes
            .iter()
            .all(|o| o.status == BuiltinSkillSetupStatus::Installed)
    );
    assert!(!root.join(".imrule/skills").exists());
}

/// The state of an installed directory holding exactly `files`.
fn state_of(skill: &BuiltinSkill, files: &[(&str, &str)]) -> BuiltinSkillState {
    let listed: Vec<String> = files.iter().map(|(path, _)| path.to_string()).collect();
    BuiltinSkillState::compare(skill, Some(&listed), |relative: &str| {
        files
            .iter()
            .find(|(path, _)| *path == relative)
            .map(|(_, content)| content.to_string())
    })
}

#[test]
fn install_state_compares_contents_before_revisions() {
    let catalog = catalog_v2();
    let alpha = catalog.iter().find(|s| s.path == "lang/alpha").unwrap();
    let check = "print('v2')\n";
    let old_check = "print('v1')\n";

    assert_eq!(
        BuiltinSkillState::compare(alpha, None, |_| None),
        BuiltinSkillState::NotInstalled,
        "only a path where nothing exists is not installed"
    );
    assert_eq!(
        state_of(alpha, &[("scripts/check.py", check)]),
        BuiltinSkillState::Modified,
        "a directory without SKILL.md is not a copy ImRule installed"
    );
    assert_eq!(
        state_of(alpha, &[]),
        BuiltinSkillState::Modified,
        "neither is an empty one"
    );
    assert_eq!(
        state_of(
            alpha,
            &[("SKILL.md", ALPHA_V2), ("scripts/check.py", check)]
        ),
        BuiltinSkillState::UpToDate
    );
    assert_eq!(
        state_of(
            alpha,
            &[
                ("SKILL.md", ALPHA_V2),
                ("scripts/check.py", check),
                ("notes.md", "a file the user added"),
            ]
        ),
        BuiltinSkillState::Modified,
        "a file the embedded skill does not ship was added locally"
    );
    assert_eq!(
        state_of(
            alpha,
            &[
                ("SKILL.md", ALPHA_V1),
                ("scripts/check.py", old_check),
                (".DS_Store", "finder"),
                ("scripts/__pycache__/check.cpython-312.pyc", "bytecode"),
            ]
        ),
        BuiltinSkillState::Outdated,
        "hidden files and bytecode are not user content"
    );
    assert_eq!(
        state_of(alpha, &[("SKILL.md", ALPHA_V2)]),
        BuiltinSkillState::Modified,
        "a file deleted at the current revision is a local change"
    );
    assert_eq!(
        state_of(
            alpha,
            &[("SKILL.md", ALPHA_V1), ("scripts/check.py", old_check)]
        ),
        BuiltinSkillState::Outdated
    );
    assert_eq!(
        state_of(alpha, &[("SKILL.md", ALPHA_V1)]),
        BuiltinSkillState::Outdated,
        "an older copy may lack a file the newer revision added"
    );
    assert_eq!(
        state_of(
            alpha,
            &[
                ("SKILL.md", ALPHA_V1),
                ("scripts/check.py", old_check),
                ("scripts/mine.py", "added"),
            ]
        ),
        BuiltinSkillState::Modified,
        "an older copy with an added file is not refreshed silently"
    );
    let boolean_marker = ALPHA_V1.replace("\"true\"", "true");
    assert_eq!(
        state_of(alpha, &[("SKILL.md", boolean_marker.as_str())]),
        BuiltinSkillState::Outdated,
        "the marker may be a YAML boolean"
    );
    let unmarked = ALPHA_V1.replace("  imrule-builtin: \"true\"\n", "");
    assert_eq!(
        state_of(alpha, &[("SKILL.md", unmarked.as_str())]),
        BuiltinSkillState::Modified,
        "a SKILL.md without the built-in marker is the user's"
    );
    let revision_zero = ALPHA_V1.replace("\"1\"", "\"0\"");
    assert_eq!(
        state_of(alpha, &[("SKILL.md", revision_zero.as_str())]),
        BuiltinSkillState::Modified,
        "no built-in ever shipped revision 0"
    );
    let newer = ALPHA_V2.replace("\"2\"", "\"3\"");
    assert_eq!(
        state_of(
            alpha,
            &[("SKILL.md", newer.as_str()), ("scripts/check.py", check)]
        ),
        BuiltinSkillState::Modified,
        "a copy from a newer ImRule is never downgraded silently"
    );

    let shown: Vec<(&str, Option<&str>)> = [
        BuiltinSkillState::NotInstalled,
        BuiltinSkillState::UpToDate,
        BuiltinSkillState::Outdated,
        BuiltinSkillState::Modified,
    ]
    .into_iter()
    .map(|state| (state.label(), state.tag()))
    .collect();
    assert_eq!(
        shown,
        vec![
            ("not-installed", None),
            ("up-to-date", Some("installed")),
            ("outdated", Some("update available")),
            ("modified", Some("modified locally")),
        ]
    );
}

#[test]
fn install_rejects_a_path_the_plan_does_not_hold() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let catalog = catalog_v1();
    let use_case = SkillsSetupUseCase::new(&fs_port, &catalog);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());

    let error = use_case
        .install(
            &plan,
            &["lang/alpha".to_string(), "missing/skill".to_string()],
            false,
            false,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("unknown built-in skill: missing/skill"),
        "{error}"
    );
}

#[test]
fn setup_outcomes_say_what_would_happen_under_dry_run() {
    use BuiltinSkillSetupStatus::{Installed, SkippedModified, Unchanged, Updated};
    let labels: Vec<(&str, &str, bool)> = [Installed, Updated, Unchanged, SkippedModified]
        .into_iter()
        .map(|status| (status.label(false), status.label(true), status.writes()))
        .collect();
    assert_eq!(
        labels,
        vec![
            ("installed", "would install", true),
            ("updated", "would update", true),
            ("unchanged", "unchanged", false),
            (
                "modified locally, skipped — pass --force or toggle it on individually in the picker",
                "modified locally, skipped — pass --force or toggle it on individually in the picker",
                false
            ),
        ]
    );
}

// ----------------------------------------------------------------- picker ---

fn item(title: &str, description: &str, selected: bool) -> PickerItem {
    PickerItem {
        id: title.to_string(),
        title: title.to_string(),
        meta: vec![],
        description: description.to_string(),
        selected,
    }
}

fn picker() -> Picker {
    Picker::new(
        "Skills",
        "Detected: rust",
        vec![
            item("rust-cli", "Rust CLI 설정·검사", true),
            item("rust-server", "Rust 서버 설정·검사", false),
            item("python-cli", "Python CLI", false),
            item("docker-setup", "Dockerfile 검사", false),
        ],
    )
}

fn text(lines: &[Vec<imrule::interface::skill_picker::Span>]) -> String {
    lines
        .iter()
        .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn typing_filters_and_space_toggles_the_focused_item() {
    let mut picker = picker();
    for c in "rust".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    assert_eq!(picker.filtered(), vec![0, 1]);
    // Descriptions are searched too, and editing the query moves focus back
    // to the first match.
    for _ in 0..4 {
        picker.handle(PickerKey::Backspace, 30);
    }
    for c in "서버".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    assert_eq!(picker.query(), "서버");
    assert_eq!(picker.filtered(), vec![1]);

    assert_eq!(picker.handle(PickerKey::Toggle, 30), PickerAction::Continue);
    assert_eq!(picker.selected(), vec![0, 1]);
    assert_eq!(picker.handle(PickerKey::Confirm, 30), PickerAction::Confirm);
}

#[test]
fn a_name_match_ranks_above_items_that_only_mention_it() {
    // make-setup's description mentions docker-setup, and make-setup sits
    // first in the list; typing "docker" must still focus docker-setup.
    let mut picker = Picker::new(
        "Skills",
        "",
        vec![
            item("make-setup", "Docker 이미지 구성은 docker-setup", true),
            item("docker-setup", "Dockerfile 검사", false),
        ],
    );
    for c in "docker".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    assert_eq!(picker.filtered(), vec![1, 0]);
    picker.handle(PickerKey::Toggle, 30);
    assert_eq!(picker.selected(), vec![0, 1]);
}

#[test]
fn toggle_all_applies_to_the_filtered_items_only() {
    let mut picker = picker();
    for c in "cli".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    picker.handle(PickerKey::ToggleAll, 30);
    assert_eq!(picker.selected(), vec![0, 2]);
    picker.handle(PickerKey::ToggleAll, 30);
    assert!(picker.selected().is_empty());
    assert_eq!(picker.handle(PickerKey::Cancel, 30), PickerAction::Cancel);
}

#[test]
fn only_toggling_an_item_on_by_itself_records_consent() {
    let mut picker = picker();
    assert_eq!(picker.selected_ids(), vec!["rust-cli"]);
    assert!(
        picker.individually_selected_ids().is_empty(),
        "a preselection is not consent"
    );

    picker.handle(PickerKey::ToggleAll, 30);
    assert_eq!(picker.selected().len(), 4);
    assert!(
        picker.individually_selected_ids().is_empty(),
        "select-all is not consent"
    );

    // Focus rust-server, selected by select-all: off, then on by itself.
    picker.handle(PickerKey::Down, 30);
    picker.handle(PickerKey::Toggle, 30);
    assert!(picker.individually_selected_ids().is_empty());
    picker.handle(PickerKey::Toggle, 30);
    assert_eq!(
        picker.selection(),
        PickerSelection {
            selected: vec![
                "rust-cli".to_string(),
                "rust-server".to_string(),
                "python-cli".to_string(),
                "docker-setup".to_string(),
            ],
            individually_selected: vec!["rust-server".to_string()],
        }
    );

    picker.handle(PickerKey::Toggle, 30);
    assert!(
        picker.individually_selected_ids().is_empty(),
        "toggling it off withdraws consent"
    );

    // Consent withdrawn by deselect-all does not come back with select-all.
    picker.handle(PickerKey::Toggle, 30);
    assert_eq!(picker.individually_selected_ids(), vec!["rust-server"]);
    picker.handle(PickerKey::ToggleAll, 30);
    assert!(picker.selected().is_empty());
    picker.handle(PickerKey::ToggleAll, 30);
    assert_eq!(picker.selected().len(), 4);
    assert!(picker.individually_selected_ids().is_empty());
}

#[test]
fn render_shows_search_selection_and_what_is_below_the_fold() {
    let mut picker = picker();
    // Room for two items: 6 header rows + 2 footer rows + 2 × 3 item rows.
    let height = 14;
    let rendered = text(&picker.render(80, height));
    assert!(
        rendered.contains("Skills  (1 selected · 4/4)"),
        "{rendered}"
    );
    assert!(rendered.contains("⌕ Search…"));
    assert!(rendered.contains(" ❯ ● rust-cli"));
    assert!(rendered.contains("   ○ rust-server"));
    assert!(rendered.contains("↓ 2 more below"));
    assert!(rendered.contains("Space to toggle"));

    for _ in 0..3 {
        picker.handle(PickerKey::Down, Picker::list_height(height));
    }
    let rendered = text(&picker.render(80, height));
    assert!(rendered.contains(" ❯ ○ docker-setup"), "{rendered}");
    assert!(!rendered.contains("rust-cli"));
    assert!(!rendered.contains("more below"));
}

#[test]
fn truncation_counts_hangul_as_two_columns() {
    assert_eq!(display_width("검사 cli"), 8);
    assert_eq!(truncate("가나다라마", 7), "가나다…");
    assert_eq!(truncate("short", 10), "short");
}

#[test]
fn terminal_keys_map_onto_picker_keys() {
    let key = |code, modifiers| picker_key(KeyEvent::new(code, modifiers));
    let ctrl = KeyModifiers::CONTROL;
    let none = KeyModifiers::NONE;
    assert_eq!(key(KeyCode::Char('c'), ctrl), Some(PickerKey::Cancel));
    assert_eq!(key(KeyCode::Char('a'), ctrl), Some(PickerKey::ToggleAll));
    assert_eq!(key(KeyCode::Char('p'), ctrl), Some(PickerKey::Up));
    assert_eq!(key(KeyCode::Char('n'), ctrl), Some(PickerKey::Down));
    assert_eq!(
        key(KeyCode::Char('x'), ctrl),
        None,
        "an unbound Ctrl chord is not typed into the search"
    );
    assert_eq!(key(KeyCode::Char(' '), none), Some(PickerKey::Toggle));
    assert_eq!(key(KeyCode::Tab, none), Some(PickerKey::Toggle));
    assert_eq!(key(KeyCode::Char('c'), none), Some(PickerKey::Char('c')));
    assert_eq!(
        key(KeyCode::Char('A'), KeyModifiers::SHIFT),
        Some(PickerKey::Char('A'))
    );
    assert_eq!(key(KeyCode::Backspace, none), Some(PickerKey::Backspace));
    assert_eq!(key(KeyCode::Up, none), Some(PickerKey::Up));
    assert_eq!(key(KeyCode::Down, none), Some(PickerKey::Down));
    assert_eq!(key(KeyCode::PageUp, none), Some(PickerKey::PageUp));
    assert_eq!(key(KeyCode::PageDown, none), Some(PickerKey::PageDown));
    assert_eq!(key(KeyCode::Enter, none), Some(PickerKey::Confirm));
    assert_eq!(key(KeyCode::Esc, none), Some(PickerKey::Cancel));
    assert_eq!(key(KeyCode::F(1), none), None);
    // Terminals that report releases must not act on every key twice.
    assert_eq!(
        picker_key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            none,
            KeyEventKind::Release
        )),
        None
    );
}

#[test]
fn paging_scrolls_both_ways_and_an_empty_search_is_safe() {
    let items = (0..5)
        .map(|i| item(&format!("skill-{i}"), "desc", false))
        .collect();
    let mut picker = Picker::new("Skills", "", items);
    // Room for two items: 6 header rows + 2 footer rows + 2 × 3 item rows.
    let height = 14;
    let list = Picker::list_height(height);
    assert_eq!(
        Picker::list_height(2),
        3,
        "a tiny terminal still shows one item"
    );

    for _ in 0..3 {
        picker.handle(PickerKey::PageDown, list);
    }
    let rendered = text(&picker.render(80, height));
    assert!(rendered.contains(" ❯ ○ skill-4"), "{rendered}");
    assert!(!rendered.contains("skill-2"), "{rendered}");

    picker.handle(PickerKey::PageUp, list);
    let rendered = text(&picker.render(80, height));
    assert!(rendered.contains(" ❯ ○ skill-2"), "{rendered}");
    assert!(
        rendered.contains("skill-3") && !rendered.contains("skill-4"),
        "{rendered}"
    );
    assert!(rendered.contains("↓ 1 more below"), "{rendered}");

    for _ in 0..10 {
        picker.handle(PickerKey::Up, list);
    }
    assert!(text(&picker.render(80, height)).contains(" ❯ ○ skill-0"));

    for c in "zzz".chars() {
        picker.handle(PickerKey::Char(c), list);
    }
    for key in [
        PickerKey::Down,
        PickerKey::PageDown,
        PickerKey::Toggle,
        PickerKey::ToggleAll,
    ] {
        assert_eq!(picker.handle(key, list), PickerAction::Continue);
    }
    assert!(picker.selected().is_empty());
    let rendered = text(&picker.render(80, height));
    assert!(rendered.contains("(0 selected · 0/5)"), "{rendered}");
    assert!(rendered.contains("⌕ zzz▏"), "{rendered}");
    assert!(
        rendered.contains("No skills match your search."),
        "{rendered}"
    );
}

#[test]
fn meta_matches_rank_between_title_and_description_and_terms_combine() {
    let described = item("beta", "mentions detected here", false);
    let mut tagged = item("alpha", "plain", false);
    tagged.meta = vec!["rust/cli".to_string(), "detected".to_string()];
    let mut picker = Picker::new("Skills", "", vec![described, tagged]);

    for c in "detected".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    assert_eq!(picker.filtered(), vec![1, 0]);
    let rendered = text(&picker.render(80, 30));
    assert!(
        rendered.contains("alpha · rust/cli · detected"),
        "{rendered}"
    );

    for _ in 0.."detected".len() {
        picker.handle(PickerKey::Backspace, 30);
    }
    // Every term must match somewhere, case-insensitively.
    for c in "BETA here".chars() {
        picker.handle(PickerKey::Char(c), 30);
    }
    assert_eq!(picker.filtered(), vec![0]);
    assert!(picker.selected_ids().is_empty());
    picker.handle(PickerKey::Toggle, 30);
    assert_eq!(picker.selected_ids(), vec!["beta"]);
}

// ------------------------------------------------------- shipped skills ---

const SHIPPED: &[&str] = &[
    "ci/github-actions",
    "cli",
    "docker/optimize",
    "docker/setup",
    "imrule-issue",
    "make/setup",
    "python/cli",
    "python/server",
    "release/versioning",
    "rust/cli",
    "rust/server",
    "server",
    "vscode/setup",
];

fn helper_block(text: &str) -> Option<&str> {
    let start = text.find("# --- imrule check helpers")?;
    let end = text[start..].find("# --- end imrule check helpers ---")? + start;
    Some(&text[start..end])
}

#[test]
fn every_shipped_skill_is_complete_and_named_after_its_path() {
    let catalog = builtin_catalog();
    let paths: Vec<&str> = catalog.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, SHIPPED);

    let guide = fs::read_to_string("skills/README.md").unwrap();
    let reference_helpers = helper_block(&guide).expect("helper block in skills/README.md");

    for skill in &catalog {
        let file = |name: &str| {
            skill
                .files
                .iter()
                .find(|(path, _)| path == name)
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("{} is missing {name}", skill.path))
        };
        let skill_md = file("SKILL.md");
        let meta = parse_frontmatter(skill_md).unwrap().unwrap().meta;
        assert_eq!(
            meta["name"].as_str(),
            Some(flatten_skill_name(Path::new(&skill.path)).as_str()),
            "{}: frontmatter name must equal the published name",
            skill.path
        );
        assert!(
            !skill.description.is_empty() && skill.description.chars().count() <= 1024,
            "{}: description must be 1–1024 characters",
            skill.path
        );
        assert!(
            skill.revision >= 1,
            "{}: set metadata.imrule-skill-version",
            skill.path
        );
        assert!(
            skill_md.lines().count() <= 500,
            "{}: SKILL.md over 500 lines",
            skill.path
        );
        file("references/structure.md");
        file("references/convention.md");
        let check = file("scripts/check.py");
        assert_eq!(
            helper_block(check),
            Some(reference_helpers),
            "{}: check.py helper block drifted from skills/README.md",
            skill.path
        );
        assert!(
            !skill
                .files
                .iter()
                .any(|(path, _)| path != "SKILL.md" && path.ends_with("SKILL.md")),
            "{}: a nested SKILL.md is registered as a separate skill by Codex and Cursor",
            skill.path
        );
    }
}

#[test]
fn the_embedded_catalog_carries_no_hidden_files_or_bytecode() {
    // build.rs skips these so a local checker run never ships its leftovers.
    for skill in builtin_catalog() {
        for (path, _) in &skill.files {
            assert!(
                !path
                    .split('/')
                    .any(|part| part.starts_with('.') || part == "__pycache__"),
                "{}: {path} must not be embedded",
                skill.path
            );
        }
    }
}

// -------------------------------------------------------------------- cli ---

/// A project wired to Claude, inside a temporary directory whose sibling
/// `xdg/` serves as the config home, outside the project.
fn claude_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempdir().unwrap();
    let project = tmp.path().join("project");
    fs::create_dir_all(project.join(".imrule")).unwrap();
    fs::write(
        project.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    (tmp, project)
}

fn setup_cli(project: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("imrule")
        .unwrap()
        .env("XDG_CONFIG_HOME", project.parent().unwrap().join("xdg"))
        .args(["skills", "setup"])
        .args(args)
        .args(["--project-root", project.to_str().unwrap()])
        .output()
        .unwrap()
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn rust_cli_state(project: &Path, extra: &[&str]) -> String {
    let mut args = vec!["--list", "--json"];
    args.extend_from_slice(extra);
    let listed = setup_cli(project, &args);
    assert!(listed.status.success());
    let catalog: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    catalog["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["path"] == "rust/cli")
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn setup_rejects_unknown_names_conflicting_flags_and_a_missing_terminal() {
    let (_tmp, project) = claude_project();

    let unknown = setup_cli(&project, &["rust-cli", "nope"]);
    assert_eq!(unknown.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&unknown.stderr);
    assert!(stderr.contains("unknown built-in skill: nope"), "{stderr}");
    assert!(stderr.contains("Available: ci/github-actions"), "{stderr}");
    assert!(
        !project.join(".imrule/skills").exists(),
        "a known name beside an unknown one was installed"
    );

    for args in [
        &["--json"][..],
        &["--all", "rust-cli"][..],
        &["--yes", "rust-cli"][..],
        &["--yes", "--all"][..],
    ] {
        assert_eq!(setup_cli(&project, args).status.code(), Some(2), "{args:?}");
    }

    let no_terminal = setup_cli(&project, &[]);
    assert_eq!(no_terminal.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&no_terminal.stderr);
    assert!(
        stderr.contains("no terminal to pick skills interactively"),
        "{stderr}"
    );
    assert!(
        stderr.contains("--yes") && stderr.contains("--all"),
        "{stderr}"
    );
}

#[test]
fn setup_all_dry_run_reports_every_skill_without_writing_or_syncing() {
    let (_tmp, project) = claude_project();

    let output = setup_cli(&project, &["--all", "--dry-run"]);
    assert!(output.status.success());
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("Would set up built-in skill(s) in"),
        "{stdout}"
    );
    for skill in builtin_catalog() {
        assert!(
            stdout.contains(&format!("{} ({}) [would install]", skill.name, skill.path)),
            "{stdout}"
        );
    }
    assert!(!stdout.contains("Syncing"), "{stdout}");
    assert!(!project.join(".imrule/skills").exists());
    assert!(!project.join(".claude").exists());
}

#[test]
fn setup_skips_a_locally_modified_skill_until_forced() {
    let (_tmp, project) = claude_project();
    let embedded = fs::read_to_string("skills/rust/cli/SKILL.md").unwrap();
    let installed = project.join(".imrule/skills/rust/cli/SKILL.md");
    let published = project.join(".claude/skills/rust-cli/SKILL.md");

    assert!(setup_cli(&project, &["rust/cli"]).status.success());
    assert_eq!(rust_cli_state(&project, &[]), "up-to-date");
    let listed = stdout_of(&setup_cli(&project, &["--list"]));
    assert!(
        listed.contains("rust-cli [rust/cli, installed]"),
        "{listed}"
    );
    // Nothing in this project is detectable, and a skill with no tags to
    // show (path equals name, not detected, not installed) has no brackets.
    assert!(
        listed.starts_with("Built-in skills (nothing detected):"),
        "{listed}"
    );
    assert!(listed.lines().any(|line| line == "    cli"), "{listed}");

    fs::write(&installed, format!("{embedded}\nmy note\n")).unwrap();
    assert_eq!(rust_cli_state(&project, &[]), "modified");
    let listed = stdout_of(&setup_cli(&project, &["--list"]));
    assert!(
        listed.contains("rust-cli [rust/cli, modified locally]"),
        "{listed}"
    );

    let skipped = setup_cli(&project, &["rust-cli"]);
    assert!(skipped.status.success());
    let stdout = stdout_of(&skipped);
    assert!(
        stdout.contains(
            "rust-cli (rust/cli) [modified locally, skipped — pass --force or toggle it on individually in the picker]"
        ),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Syncing"),
        "nothing was written, so agents need no sync: {stdout}"
    );
    assert!(fs::read_to_string(&installed).unwrap().contains("my note"));

    let forced = setup_cli(&project, &["rust-cli", "--force"]);
    assert!(forced.status.success());
    assert!(stdout_of(&forced).contains("rust-cli (rust/cli) [updated]"));
    assert_eq!(fs::read_to_string(&installed).unwrap(), embedded);
    assert_eq!(fs::read_to_string(&published).unwrap(), embedded);

    // Every built-in starts at revision 1, so a copy claiming revision 0 was
    // not installed by ImRule: it stays the user's until forced. (Refreshing
    // a genuinely older revision is covered against a test catalog above.)
    let older = embedded
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("imrule-skill-version:") {
                "  imrule-skill-version: \"0\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&installed, &older).unwrap();
    assert_eq!(rust_cli_state(&project, &[]), "modified");
    let skipped = setup_cli(&project, &["rust-cli"]);
    assert!(stdout_of(&skipped).contains("rust-cli (rust/cli) [modified locally, skipped"));
    assert_eq!(fs::read_to_string(&installed).unwrap(), older);
}

#[test]
fn setup_from_a_subdirectory_detects_and_syncs_the_enclosing_project() {
    let (_tmp, project) = claude_project();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"demo\"\n\n[dependencies]\nclap = \"4\"\n",
    )
    .unwrap();
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();

    let output = Command::cargo_bin("imrule")
        .unwrap()
        .env("XDG_CONFIG_HOME", project.parent().unwrap().join("xdg"))
        .args(["skills", "setup", "--yes", "--project-root"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("rust-cli (rust/cli) [installed]"),
        "the project's Cargo.toml was not detected from src/: {stdout}"
    );
    assert!(
        stdout.contains("Skills synced to agent directories."),
        "{stdout}"
    );
    assert!(project.join(".imrule/skills/rust/cli/SKILL.md").is_file());
    assert!(project.join(".claude/skills/rust-cli/SKILL.md").is_file());
    assert!(!src.join(".imrule").exists() && !src.join(".claude").exists());
}

#[test]
fn skills_resolve_to_the_project_that_owns_their_imrule_directory() {
    let root = Path::new("/work/repo");
    let src = root.join("src/bin");
    assert_eq!(
        skills_project_root(&root.join(".imrule/skills"), &src, false),
        root
    );
    assert_eq!(
        skills_project_root(&root.join(".imrule/skills"), root, false),
        root
    );
    assert_eq!(
        skills_project_root(&root.join(".ruler/skills"), &src, false),
        root
    );
    assert_eq!(
        skills_project_root(Path::new(".imrule/skills"), Path::new("src"), false),
        Path::new(".")
    );
    // --global, the global fallback, or a directory that is not an ancestor
    // keep the requested root.
    for (install_dir, global) in [
        (root.join(".imrule/skills"), true),
        (PathBuf::from("/home/me/.config/imrule/skills"), false),
        (PathBuf::from("/elsewhere/.imrule/skills"), false),
        (root.join(".imrule/other"), false),
    ] {
        assert_eq!(
            skills_project_root(&install_dir, &src, global),
            src,
            "{install_dir:?}"
        );
    }
}

#[test]
fn setup_global_installs_into_the_config_home_without_syncing_the_project() {
    let (tmp, project) = claude_project();

    let output = setup_cli(&project, &["rust-cli", "--global"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout_of(&output).contains("Skipping agent sync"),
        "{}",
        stdout_of(&output)
    );
    assert!(
        tmp.path()
            .join("xdg/imrule/skills/rust/cli/SKILL.md")
            .is_file()
    );
    assert!(!project.join(".imrule/skills").exists());
    assert!(!project.join(".claude").exists());
    assert_eq!(rust_cli_state(&project, &["--global"]), "up-to-date");
}

#[test]
fn setup_lists_detected_skills_and_installs_named_ones_without_a_terminal() {
    let tmp = project();
    let root = tmp.path();
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\n\n[dependencies]\nclap = \"4\"\n",
    )
    .unwrap();
    let root_arg = root.to_str().unwrap();

    let listed = Command::cargo_bin("imrule")
        .unwrap()
        .args(["skills", "setup", "--list", "--project-root", root_arg])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(stdout.contains("detected: rust, cli"), "{stdout}");
    assert!(
        stdout.contains("* rust-cli [rust/cli, detected]"),
        "{stdout}"
    );

    let listed_json = Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "skills",
            "setup",
            "--list",
            "--json",
            "--project-root",
            root_arg,
        ])
        .output()
        .unwrap();
    assert!(listed_json.status.success());
    let catalog: serde_json::Value = serde_json::from_slice(&listed_json.stdout).unwrap();
    assert_eq!(catalog["detected"], serde_json::json!(["rust", "cli"]));
    let rust_cli = catalog["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["name"] == "rust-cli")
        .unwrap();
    assert_eq!(rust_cli["recommended"], true);
    assert_eq!(rust_cli["state"], "not-installed");

    // assert_cmd gives the child no terminal, so picking must be explicit.
    Command::cargo_bin("imrule")
        .unwrap()
        .args(["skills", "setup", "--project-root", root_arg])
        .assert()
        .code(2);

    Command::cargo_bin("imrule")
        .unwrap()
        .args(["skills", "setup", "rust-cli", "--project-root", root_arg])
        .assert()
        .success();
    assert!(root.join(".imrule/skills/rust/cli/SKILL.md").is_file());
    assert!(
        root.join(".claude/skills/rust-cli/SKILL.md").is_file(),
        "setup syncs agents like skills add does"
    );

    let rerun = Command::cargo_bin("imrule")
        .unwrap()
        .args(["skills", "setup", "--yes", "--project-root", root_arg])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    let stdout = String::from_utf8_lossy(&rerun.stdout);
    assert!(
        stdout.contains("rust-cli (rust/cli) [unchanged]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("make-setup (make/setup) [installed]"),
        "{stdout}"
    );

    let installed = Command::cargo_bin("imrule")
        .unwrap()
        .args(["skills", "list", "--json", "--project-root", root_arg])
        .output()
        .unwrap();
    assert!(installed.status.success());
    let installed: serde_json::Value = serde_json::from_slice(&installed.stdout).unwrap();
    let names: Vec<&str> = installed["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| skill["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"rust-cli") && names.contains(&"make-setup"),
        "{names:?}"
    );
}
