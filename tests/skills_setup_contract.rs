//! Contract for `imrule skills setup`: the built-in skills compiled into the
//! binary, project detection, install/update semantics, and the picker.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use imrule::application::skills_setup_use_case::{SkillsSetupOptions, SkillsSetupUseCase};
use imrule::domain::builtin_skills::{
    BuiltinSkill, BuiltinSkillSetupStatus, BuiltinSkillState, ProjectSignals,
    build_builtin_catalog, python_requirement_name, recommend_builtin_skills,
    resolve_builtin_skills,
};
use imrule::domain::skills::flatten_skill_name;
use imrule::domain::subagent::parse_frontmatter;
use imrule::infrastructure::builtin_skills::{builtin_catalog, collect_project_signals};
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::interface::skill_picker::{
    Picker, PickerAction, PickerItem, PickerKey, display_width, truncate,
};
use tempfile::tempdir;

// ---------------------------------------------------------------- catalog ---

const ALPHA_V1: &str = "---\nname: lang-alpha\ndescription: \"Alpha skill\"\nmetadata:\n  imrule-skill-version: \"1\"\n---\nbody v1\n";
const ALPHA_V2: &str = "---\nname: lang-alpha\ndescription: \"Alpha skill\"\nmetadata:\n  imrule-skill-version: \"2\"\n---\nbody v2\n";

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

#[test]
fn an_older_revision_is_refreshed_and_files_it_dropped_are_removed() {
    let tmp = project();
    let root = tmp.path();
    let fs_port = FsFileSystem::new();
    let old = catalog_v1();
    let old_use_case = SkillsSetupUseCase::new(&fs_port, &old);
    let plan = old_use_case.plan(&options(root), &ProjectSignals::default());
    old_use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();

    let new = catalog_v2();
    let use_case = SkillsSetupUseCase::new(&fs_port, &new);
    let plan = use_case.plan(&options(root), &ProjectSignals::default());
    let alpha = plan
        .entries
        .iter()
        .find(|e| e.skill.path == "lang/alpha")
        .unwrap();
    assert_eq!(alpha.state, BuiltinSkillState::Outdated);

    let result = use_case
        .install(&plan, &["lang/alpha".to_string()], false, false)
        .unwrap();
    assert_eq!(result.outcomes[0].status, BuiltinSkillSetupStatus::Updated);
    let skill_dir = root.join(".imrule/skills/lang/alpha");
    assert_eq!(
        fs::read_to_string(skill_dir.join("scripts/check.py")).unwrap(),
        "print('v2')\n"
    );
    assert!(!skill_dir.join("scripts/old.py").exists());
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

// -------------------------------------------------------------------- cli ---

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
