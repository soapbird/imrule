use std::fs;

use imrule::application::apply_use_case::get_agent_output_paths;
use imrule::application::ports::FileSystemPort;
use imrule::domain::agent::all_agents;
use imrule::domain::config::SubagentFrontmatter;
use imrule::domain::skills::{
    RemoteSkillSource, format_validation_warnings, get_skills_gitignore_paths, parse_skill_source,
};
use imrule::domain::subagent::{
    build_claude_file, build_codex_file, build_copilot_file, build_cursor_file,
    map_tools_for_copilot, parse_frontmatter, subagents_gitignore_paths, validate_frontmatter,
};
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::skills::{copy_skills_directory, discover_skills};
use imrule::infrastructure::subagents::{discover_subagents, load_subagent_file};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn discovers_skills_groupings_warnings_copies_and_gitignore_targets() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule/skills/group/nested")).unwrap();
    fs::create_dir_all(root.join(".imrule/skills/solo")).unwrap();
    fs::create_dir_all(root.join(".imrule/skills/stray/empty")).unwrap();
    fs::write(root.join(".imrule/skills/group/nested/SKILL.md"), "nested").unwrap();
    fs::write(root.join(".imrule/skills/solo/SKILL.md"), "solo").unwrap();

    let discovered = discover_skills(root).unwrap();
    let names: Vec<_> = discovered
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["group-nested", "solo"]);
    assert_eq!(
        discovered.warnings,
        vec![
            "Directory 'stray' in skills has no SKILL.md and contains no sub-skills. It may be malformed or stray."
        ]
    );
    assert_eq!(
        format_validation_warnings(&discovered.warnings),
        "  - Directory 'stray' in skills has no SKILL.md and contains no sub-skills. It may be malformed or stray."
    );

    copy_skills_directory(&root.join(".imrule/skills"), &root.join(".claude/skills")).unwrap();
    assert_eq!(
        fs::read_to_string(root.join(".claude/skills/solo/SKILL.md")).unwrap(),
        "solo"
    );

    let agents = all_agents();
    let selected: Vec<_> = agents
        .iter()
        .filter(|agent| {
            [
                "claude",
                "codex",
                "mistral",
                "factory",
                "kimi-cli",
                "kimi-code",
                "kimi",
            ]
            .contains(&agent.identifier)
        })
        .copied()
        .collect();
    assert_eq!(
        get_skills_gitignore_paths(root, &selected),
        vec![
            root.join(".claude/skills"),
            root.join(".codex/skills"),
            root.join(".vibe/skills"),
            root.join(".kimi-code/skills"),
            root.join(".factory/skills"),
        ]
    );
}

#[test]
fn grouped_skills_are_named_by_their_path_below_the_skills_root() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    // `python/cli` and `rust/cli` share a leaf name; published under it alone,
    // one would overwrite the other in `.claude/skills/cli`.
    for (dir, name) in [
        ("cli", "cli"),
        ("make/setup", "make-setup"),
        ("python/cli", "python-cli"),
        ("rust/cli", "cli"),
    ] {
        write_source_skill(
            &root.join(".imrule/skills"),
            dir,
            &format!("---\nname: {name}\ndescription: test\n---\nbody\n"),
        );
    }

    let discovered = discover_skills(root).unwrap();
    let names: Vec<_> = discovered
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["cli", "make-setup", "python-cli", "rust-cli"]);
    // Several agents refuse a skill whose `name` differs from its directory.
    assert_eq!(
        discovered.warnings,
        vec![
            "Skill 'rust/cli' declares name 'cli' but is published as 'rust-cli'; agents that require the name to match the directory will skip it."
        ]
    );
}

#[test]
fn two_skills_publishing_under_one_name_are_rejected() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    for dir in ["python/cli", "python-cli"] {
        write_source_skill(&root.join(".imrule/skills"), dir, "x");
    }

    let error = discover_skills(root).unwrap_err().to_string();
    assert!(
        error.contains("'python/cli' and 'python-cli' would both be published as 'python-cli'"),
        "unexpected error: {error}"
    );
}

#[test]
fn parses_validates_and_loads_subagent_frontmatter() {
    let parsed = parse_frontmatter("---\nname: helper\ndescription: Helps\ntools: Read, Grep, Unknown\nmodel: inherit\nreadonly: true\nis_background: false\n---\n\nBody\n").unwrap().unwrap();
    let fm = validate_frontmatter(&parsed.meta, "helper").unwrap();
    assert_eq!(
        fm,
        SubagentFrontmatter {
            name: "helper".to_string(),
            description: "Helps".to_string(),
            tools: Some(vec![
                "Read".to_string(),
                "Grep".to_string(),
                "Unknown".to_string()
            ]),
            model: Some("inherit".to_string()),
            readonly: Some(true),
            is_background: Some(false),
        }
    );
    assert_eq!(parsed.body, "\nBody\n");
    assert!(
        validate_frontmatter(&json!({ "name": "other", "description": "x" }), "helper")
            .unwrap_err()
            .contains("does not match filename stem")
    );

    let tmp = tempdir().unwrap();
    let file = tmp.path().join("helper.md");
    fs::write(&file, "---\nname: helper\ndescription: Helps\n---\nBody\n").unwrap();
    let loaded = load_subagent_file(&file).unwrap();
    assert!(loaded.valid);
    assert_eq!(loaded.name, "helper");
    assert_eq!(loaded.body.unwrap(), "Body\n");
}

#[test]
fn transforms_subagents_for_claude_cursor_codex_and_copilot() {
    let sub = imrule::domain::config::SubagentInfo {
        name: "helper".to_string(),
        path: std::path::PathBuf::from("helper.md"),
        frontmatter: Some(SubagentFrontmatter {
            name: "helper".to_string(),
            description: "Helps".to_string(),
            tools: Some(vec![
                "Read".to_string(),
                "Grep".to_string(),
                "Unknown".to_string(),
            ]),
            model: Some("gpt-5.4".to_string()),
            readonly: Some(true),
            is_background: Some(false),
        }),
        body: Some("\nDo work".to_string()),
        valid: true,
        error: None,
    };

    assert!(build_claude_file(&sub).contains("tools:\n- Read\n- Grep\n- Unknown"));
    assert!(build_cursor_file(&sub).contains("model: gpt-5.4"));
    assert!(build_codex_file(&sub).contains("sandbox_mode = \"read-only\""));
    let copilot = build_copilot_file(&sub);
    assert!(copilot.content.contains("user-invocable: true"));
    assert!(copilot.content.contains("tools:\n- read\n- search"));
    assert!(copilot.content.contains("disable-model-invocation: true"));
    assert_eq!(
        copilot.warnings,
        vec!["Subagent \"helper\": dropping tools not mappable to Copilot aliases: Unknown"]
    );
    assert_eq!(
        map_tools_for_copilot(&["Read".into(), "Glob".into(), "Nope".into()]).unknown,
        vec!["Nope"]
    );
}

#[test]
fn discovers_subagents_and_computes_gitignore_targets() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule/agents")).unwrap();
    fs::write(root.join(".imrule/agents/bad.md"), "no frontmatter").unwrap();
    fs::write(
        root.join(".imrule/agents/good.md"),
        "---\nname: good\ndescription: Good\n---\nBody\n",
    )
    .unwrap();

    let discovered = discover_subagents(root).unwrap();
    assert_eq!(discovered.subagents.len(), 1);
    assert_eq!(discovered.subagents[0].name, "good");
    assert_eq!(
        discovered.warnings,
        vec!["bad.md: missing YAML frontmatter"]
    );

    let agents = all_agents();
    let selected: Vec<_> = agents
        .iter()
        .filter(|agent| ["claude", "cursor", "codex", "copilot"].contains(&agent.identifier))
        .copied()
        .collect();
    assert_eq!(
        subagents_gitignore_paths(root, &selected),
        vec![
            root.join(".claude/agents"),
            root.join(".cursor/agents"),
            root.join(".codex/agents"),
            root.join(".github/agents"),
        ]
    );
}

#[test]
fn apply_path_collection_and_file_system_operations_match_contract() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let agents = all_agents();
    let selected: Vec<_> = agents
        .iter()
        .filter(|agent| ["aider", "roo", "claude"].contains(&agent.identifier))
        .copied()
        .collect();
    assert_eq!(
        get_agent_output_paths(root, &selected),
        vec![
            root.join("CLAUDE.md"),
            root.join("AGENTS.md"),
            root.join(".aider.conf.yml"),
            root.join("AGENTS.md"),
            root.join(".roo/mcp.json"),
        ]
    );

    let fs = FsFileSystem::new();

    let file = root.join("RULES.md");
    fs::write(&file, "generated").unwrap();
    assert!(fs.file_exists(&file));
    fs.remove_file(&file).unwrap();
    assert!(!fs.file_exists(&file));

    fs::write(&file, "current").unwrap();
    fs::write(root.join("RULES.md.bak"), "backup").unwrap();
    fs.copy_file(&root.join("RULES.md.bak"), &file).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "backup");
    fs.remove_file(&root.join("RULES.md.bak")).unwrap();
    assert!(!root.join("RULES.md.bak").exists());
}

/// Parses with a fixed working directory and no local paths on disk, so each
/// test decides purely by the source string's shape.
fn parse_source(source: &str) -> Result<RemoteSkillSource, imrule::domain::error::ImruleError> {
    parse_skill_source(source, std::path::Path::new("/"), |_| false)
}

#[test]
fn parses_github_shorthand_source() {
    let source = parse_source("vercel-labs/agent-skills").unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::Github {
            owner: "vercel-labs".into(),
            repo: "agent-skills".into(),
            subpath: None,
        }
    );
}

#[test]
fn parses_github_url_source() {
    let source = parse_source("https://github.com/vercel-labs/agent-skills").unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::Github {
            owner: "vercel-labs".into(),
            repo: "agent-skills".into(),
            subpath: None,
        }
    );
}

#[test]
fn parses_github_url_with_subpath_source() {
    let source =
        parse_source("https://github.com/vercel-labs/agent-skills/tree/main/skills/design")
            .unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::Github {
            owner: "vercel-labs".into(),
            repo: "agent-skills".into(),
            subpath: Some("skills/design".into()),
        }
    );
}

#[test]
fn parses_gitlab_url_source() {
    let source = parse_source("https://gitlab.com/org/repo").unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::Gitlab {
            url: "https://gitlab.com/org/repo".into(),
        }
    );
}

#[test]
fn parses_git_ssh_source() {
    let source = parse_source("git@github.com:vercel-labs/agent-skills.git").unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::GitSsh {
            url: "git@github.com:vercel-labs/agent-skills.git".into(),
        }
    );
}

#[test]
fn parses_local_path_source() {
    let tmp = tempdir().unwrap();
    let local_path = tmp.path().join("my-skills");
    fs::create_dir_all(&local_path).unwrap();
    let source = parse_source(local_path.to_str().unwrap()).unwrap();
    match source {
        RemoteSkillSource::Local { path } => {
            assert_eq!(path, local_path);
        }
        _ => panic!("expected Local variant"),
    }
}

#[test]
fn parses_relative_path_source() {
    let source = parse_source("./my-skills").unwrap();
    match source {
        RemoteSkillSource::Local { path } => {
            assert!(path.is_absolute());
            assert!(path.to_string_lossy().contains("my-skills"));
        }
        _ => panic!("expected Local variant"),
    }
}

#[test]
fn rejects_invalid_source() {
    assert!(parse_source("invalid-no-slash").is_err());
}

#[test]
fn a_bare_relative_source_is_local_only_when_it_exists() {
    let existing = parse_skill_source("looks/repo-like", std::path::Path::new("/"), |path| {
        path == std::path::Path::new("looks/repo-like")
    })
    .unwrap();
    match existing {
        RemoteSkillSource::Local { path } => {
            assert_eq!(path, std::path::Path::new("/looks/repo-like"));
        }
        _ => panic!("expected Local variant"),
    }
    // The same shape parses as a GitHub shorthand when nothing exists there.
    assert!(matches!(
        parse_skill_source("looks/repo-like", std::path::Path::new("/"), |_| false).unwrap(),
        RemoteSkillSource::Github { .. }
    ));
}

#[test]
fn installs_skills_from_local_source_to_imrule_skills_dir() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Set up a local source with a skill.
    let source_dir = root.join("source-repo");
    let skill_dir = source_dir.join("my-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: my-skill\ndescription: Test\n---\n# My Skill\n",
    )
    .unwrap();

    // Set up .imrule directory.
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# Rules\n").unwrap();

    // Create a simple fetcher that returns the local path.
    struct LocalFetcher;
    impl imrule::application::ports::SkillFetcherPort for LocalFetcher {
        fn fetch_to_temp(
            &self,
            source: &RemoteSkillSource,
        ) -> Result<std::path::PathBuf, imrule::domain::error::ImruleError> {
            match source {
                RemoteSkillSource::Local { path } => Ok(path.clone()),
                _ => panic!("expected local source"),
            }
        }
    }

    let fs_port = FsFileSystem::new();
    let fetcher = LocalFetcher;
    // Isolated XDG home so recording sources never touches the caller's global config.
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let use_case = imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    );

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: None,
            list_only: false,
            global: false,
        })
        .unwrap();

    assert_eq!(result.installed, vec!["my-skill"]);
    assert!(root.join(".imrule/skills/my-skill/SKILL.md").exists());
    assert_eq!(
        fs::read_to_string(root.join(".imrule/skills/my-skill/SKILL.md")).unwrap(),
        "---\nname: my-skill\ndescription: Test\n---\n# My Skill\n"
    );
}

#[test]
fn lists_skills_without_installing() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let source_dir = root.join("source-repo");
    let skill_a = source_dir.join("skill-a");
    let skill_b = source_dir.join("skill-b");
    fs::create_dir_all(&skill_a).unwrap();
    fs::create_dir_all(&skill_b).unwrap();
    fs::write(
        skill_a.join("SKILL.md"),
        "---\nname: skill-a\ndescription: A\n---\n",
    )
    .unwrap();
    fs::write(
        skill_b.join("SKILL.md"),
        "---\nname: skill-b\ndescription: B\n---\n",
    )
    .unwrap();

    struct LocalFetcher;
    impl imrule::application::ports::SkillFetcherPort for LocalFetcher {
        fn fetch_to_temp(
            &self,
            source: &RemoteSkillSource,
        ) -> Result<std::path::PathBuf, imrule::domain::error::ImruleError> {
            match source {
                RemoteSkillSource::Local { path } => Ok(path.clone()),
                _ => panic!("expected local source"),
            }
        }
    }

    let fs_port = FsFileSystem::new();
    let fetcher = LocalFetcher;
    // Isolated XDG home so recording sources never touches the caller's global config.
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let use_case = imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    );

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: None,
            list_only: true,
            global: false,
        })
        .unwrap();

    assert!(result.installed.is_empty());
    assert_eq!(result.listed.len(), 2);
    let names: Vec<_> = result.listed.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"skill-a"));
    assert!(names.contains(&"skill-b"));
}

#[test]
fn filters_skills_by_name_when_adding() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let source_dir = root.join("source-repo");
    let skill_a = source_dir.join("skill-a");
    let skill_b = source_dir.join("skill-b");
    fs::create_dir_all(&skill_a).unwrap();
    fs::create_dir_all(&skill_b).unwrap();
    fs::write(
        skill_a.join("SKILL.md"),
        "---\nname: skill-a\ndescription: A\n---\n",
    )
    .unwrap();
    fs::write(
        skill_b.join("SKILL.md"),
        "---\nname: skill-b\ndescription: B\n---\n",
    )
    .unwrap();

    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# Rules\n").unwrap();

    struct LocalFetcher;
    impl imrule::application::ports::SkillFetcherPort for LocalFetcher {
        fn fetch_to_temp(
            &self,
            source: &RemoteSkillSource,
        ) -> Result<std::path::PathBuf, imrule::domain::error::ImruleError> {
            match source {
                RemoteSkillSource::Local { path } => Ok(path.clone()),
                _ => panic!("expected local source"),
            }
        }
    }

    let fs_port = FsFileSystem::new();
    let fetcher = LocalFetcher;
    // Isolated XDG home so recording sources never touches the caller's global config.
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let use_case = imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    );

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: Some(vec!["skill-a".into()]),
            list_only: false,
            global: false,
        })
        .unwrap();

    assert_eq!(result.installed, vec!["skill-a"]);
    assert!(root.join(".imrule/skills/skill-a/SKILL.md").exists());
    assert!(!root.join(".imrule/skills/skill-b").exists());
}

// --- Legacy .ruler/ fallback tests ---

#[test]
fn discover_skills_falls_back_to_ruler_dir() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".ruler/skills/demo")).unwrap();
    fs::write(root.join(".ruler/skills/demo/SKILL.md"), "legacy skill").unwrap();

    let discovered = discover_skills(root).unwrap();
    assert_eq!(discovered.skills.len(), 1);
    assert_eq!(discovered.skills[0].name, "demo");
}

#[test]
fn discover_skills_prefers_imrule_over_ruler() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule/skills/primary")).unwrap();
    fs::create_dir_all(root.join(".ruler/skills/legacy")).unwrap();
    fs::write(root.join(".imrule/skills/primary/SKILL.md"), "new").unwrap();
    fs::write(root.join(".ruler/skills/legacy/SKILL.md"), "old").unwrap();

    let discovered = discover_skills(root).unwrap();
    assert_eq!(discovered.skills.len(), 1);
    assert_eq!(discovered.skills[0].name, "primary");
}

#[test]
fn discover_subagents_falls_back_to_ruler_dir() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".ruler/agents")).unwrap();
    fs::write(
        root.join(".ruler/agents/worker.md"),
        "---\nname: worker\nmodel: inherit\ndescription: test worker\n---\nDo work.\n",
    )
    .unwrap();

    let discovered = discover_subagents(root).unwrap();
    assert_eq!(discovered.subagents.len(), 1);
    assert_eq!(discovered.subagents[0].name, "worker");
}

#[test]
fn discover_subagents_prefers_imrule_over_ruler() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule/agents")).unwrap();
    fs::create_dir_all(root.join(".ruler/agents")).unwrap();
    fs::write(
        root.join(".imrule/agents/primary.md"),
        "---\nname: primary\nmodel: inherit\ndescription: primary\n---\nPrimary.\n",
    )
    .unwrap();
    fs::write(
        root.join(".ruler/agents/legacy.md"),
        "---\nname: legacy\nmodel: inherit\ndescription: legacy\n---\nLegacy.\n",
    )
    .unwrap();

    let discovered = discover_subagents(root).unwrap();
    assert_eq!(discovered.subagents.len(), 1);
    assert_eq!(discovered.subagents[0].name, "primary");
}

// --- GJC skill discovery config tests ---

#[test]
fn gjc_skill_config_enables_discovery_from_scratch() {
    let yaml = imrule::domain::gjc_config::enable_gjc_skill_discovery(None).unwrap();
    let parsed: serde_json::Value = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(parsed["skills"]["enabled"], serde_json::Value::Bool(true));
    assert_eq!(
        parsed["skills"]["enablePiProject"],
        serde_json::Value::Bool(true)
    );
}

#[test]
fn gjc_skill_config_merges_preserving_existing_keys() {
    let existing = "theme:\n  dark: red-claw\n  light: blue-crab\n";
    let yaml = imrule::domain::gjc_config::enable_gjc_skill_discovery(Some(existing)).unwrap();
    let parsed: serde_json::Value = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(parsed["skills"]["enabled"], serde_json::Value::Bool(true));
    assert_eq!(
        parsed["skills"]["enablePiProject"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        parsed["theme"]["dark"],
        serde_json::Value::String("red-claw".into())
    );
    assert_eq!(
        parsed["theme"]["light"],
        serde_json::Value::String("blue-crab".into())
    );
}

#[test]
fn gjc_skill_config_enable_is_idempotent() {
    let once = imrule::domain::gjc_config::enable_gjc_skill_discovery(None).unwrap();
    let twice = imrule::domain::gjc_config::enable_gjc_skill_discovery(Some(&once)).unwrap();
    let parsed: serde_json::Value = serde_norway::from_str(&twice).unwrap();
    assert_eq!(parsed["skills"]["enabled"], serde_json::Value::Bool(true));
    assert_eq!(
        parsed["skills"]["enablePiProject"],
        serde_json::Value::Bool(true)
    );
}

#[test]
fn gjc_skill_config_strip_returns_none_when_only_managed_keys() {
    let yaml = imrule::domain::gjc_config::enable_gjc_skill_discovery(None).unwrap();
    let result = imrule::domain::gjc_config::strip_gjc_skill_discovery(&yaml).unwrap();
    assert!(result.is_none());
}

#[test]
fn gjc_skill_config_strip_preserves_unmanaged_keys() {
    let yaml =
        imrule::domain::gjc_config::enable_gjc_skill_discovery(Some("goal:\n  enabled: false\n"))
            .unwrap();
    let remaining = imrule::domain::gjc_config::strip_gjc_skill_discovery(&yaml)
        .unwrap()
        .unwrap();
    let parsed: serde_json::Value = serde_norway::from_str(&remaining).unwrap();
    assert_eq!(parsed["goal"]["enabled"], serde_json::Value::Bool(false));
    assert!(parsed.get("skills").is_none());
}

// --- Skill source registry and `imrule skills update` ---

/// Builds a local source repo holding one skill with the given SKILL.md body.
fn write_source_skill(source_dir: &std::path::Path, name: &str, body: &str) {
    let skill_dir = source_dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), body).unwrap();
}

fn skills_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let source_dir = root.join("source-repo");
    write_source_skill(&source_dir, "my-skill", "v1");
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# Rules\n").unwrap();
    (tmp, root, source_dir)
}

#[test]
fn records_the_source_of_every_installed_skill_in_the_config() {
    let (_tmp, root, source_dir) = skills_fixture();
    let fs_port = FsFileSystem::new();
    let fetcher = imrule::infrastructure::skill_fetcher::GitSkillFetcher::new().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));
    let use_case = imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    );

    use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.clone(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: None,
            list_only: false,
            global: false,
        })
        .unwrap();

    let written = fs::read_to_string(root.join(".imrule/imrule.toml")).unwrap();
    assert!(
        written.contains("[skills.sources]"),
        "expected a source registry, got:\n{written}"
    );
    assert!(written.contains("my-skill = "));

    // The registry survives a reload as a usable source string.
    let config =
        imrule::application::ports::ConfigPort::load_config(&loader, &root, None, None).unwrap();
    assert_eq!(
        config.skills.unwrap().sources.get("my-skill"),
        Some(&source_dir.to_string_lossy().to_string())
    );
}

#[test]
fn update_refetches_registered_sources_and_reports_per_skill_status() {
    use imrule::application::skills_update_use_case::{SkillsUpdateOptions, SkillsUpdateUseCase};
    use imrule::domain::skills::SkillUpdateStatus;

    let (_tmp, root, source_dir) = skills_fixture();
    let fs_port = FsFileSystem::new();
    let fetcher = imrule::infrastructure::skill_fetcher::GitSkillFetcher::new().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));

    imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    )
    .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
        project_root: root.clone(),
        source: source_dir.to_string_lossy().to_string(),
        skill_names: None,
        list_only: false,
        global: false,
    })
    .unwrap();

    let installed = root.join(".imrule/skills/my-skill/SKILL.md");
    assert_eq!(fs::read_to_string(&installed).unwrap(), "v1");

    let updater = SkillsUpdateUseCase::new(&fetcher, &fs_port, &loader);
    let options = |dry_run: bool| SkillsUpdateOptions {
        project_root: root.clone(),
        skill_names: None,
        global: false,
        dry_run,
    };

    // Nothing changed upstream yet.
    let result = updater.execute(options(false)).unwrap();
    assert_eq!(result.outcomes.len(), 1);
    assert_eq!(result.outcomes[0].status, SkillUpdateStatus::Unchanged);
    assert!(!result.changed());

    // Upstream moves on: a changed file and a dropped file.
    fs::write(source_dir.join("my-skill/SKILL.md"), "v2").unwrap();
    fs::write(
        root.join(".imrule/skills/my-skill/stale.md"),
        "gone upstream",
    )
    .unwrap();

    // A dry run reports the update without touching the installed copy.
    let result = updater.execute(options(true)).unwrap();
    assert_eq!(result.outcomes[0].status, SkillUpdateStatus::Updated);
    assert!(result.changed());
    assert_eq!(fs::read_to_string(&installed).unwrap(), "v1");

    let result = updater.execute(options(false)).unwrap();
    assert_eq!(result.outcomes[0].status, SkillUpdateStatus::Updated);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "v2");
    assert!(
        !root.join(".imrule/skills/my-skill/stale.md").exists(),
        "an update replaces the skill instead of overlaying it"
    );

    // A skill deleted from disk is installed again from its recorded source.
    fs::remove_dir_all(root.join(".imrule/skills/my-skill")).unwrap();
    let result = updater.execute(options(false)).unwrap();
    assert_eq!(result.outcomes[0].status, SkillUpdateStatus::Reinstalled);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "v2");

    // A skill that vanished upstream is reported, not silently reported as fresh.
    fs::remove_dir_all(source_dir.join("my-skill")).unwrap();
    let result = updater.execute(options(false)).unwrap();
    assert_eq!(
        result.outcomes[0].status,
        SkillUpdateStatus::MissingInSource
    );
    assert!(installed.exists(), "the installed copy is left in place");
}

#[test]
fn update_reports_an_unreachable_source_without_aborting_the_run() {
    use imrule::application::skills_update_use_case::{SkillsUpdateOptions, SkillsUpdateUseCase};
    use imrule::domain::skills::SkillUpdateStatus;

    let (_tmp, root, source_dir) = skills_fixture();
    let fs_port = FsFileSystem::new();
    let fetcher = imrule::infrastructure::skill_fetcher::GitSkillFetcher::new().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(root.join("xdg"));

    imrule::application::skills_add_use_case::SkillsAddUseCase::new(
        &fetcher, &fs_port, &loader, &loader,
    )
    .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
        project_root: root.clone(),
        source: source_dir.to_string_lossy().to_string(),
        skill_names: None,
        list_only: false,
        global: false,
    })
    .unwrap();

    fs::remove_dir_all(&source_dir).unwrap();

    let result = SkillsUpdateUseCase::new(&fetcher, &fs_port, &loader)
        .execute(SkillsUpdateOptions {
            project_root: root.clone(),
            skill_names: None,
            global: false,
            dry_run: false,
        })
        .unwrap();
    assert_eq!(result.outcomes[0].status, SkillUpdateStatus::Failed);
    assert!(result.outcomes[0].detail.is_some());
    assert!(result.has_failures());
}

#[test]
fn groups_recorded_sources_and_rejects_unregistered_names() {
    use imrule::domain::skills::{SkillUpdateGroup, group_skill_sources};
    use std::collections::BTreeMap;

    let sources: BTreeMap<String, String> = [
        ("a".to_string(), "org/one".to_string()),
        ("b".to_string(), "org/two".to_string()),
        ("c".to_string(), "org/one".to_string()),
    ]
    .into_iter()
    .collect();

    // One fetch per source, however many skills came from it.
    assert_eq!(
        group_skill_sources(&sources, None).unwrap(),
        vec![
            SkillUpdateGroup {
                source: "org/one".to_string(),
                skills: vec!["a".to_string(), "c".to_string()],
            },
            SkillUpdateGroup {
                source: "org/two".to_string(),
                skills: vec!["b".to_string()],
            },
        ]
    );

    let narrowed = group_skill_sources(&sources, Some(&["c".to_string()])).unwrap();
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].skills, vec!["c".to_string()]);

    let error = group_skill_sources(&sources, Some(&["nope".to_string()])).unwrap_err();
    assert!(error.to_string().contains("nope"));

    assert!(
        group_skill_sources(&BTreeMap::new(), None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn skill_trees_match_compares_contents_not_timestamps() {
    use imrule::infrastructure::skills::skill_trees_match;

    let tmp = tempdir().unwrap();
    let root = tmp.path();
    for name in ["left", "right"] {
        fs::create_dir_all(root.join(name).join("nested")).unwrap();
        fs::write(root.join(name).join("SKILL.md"), "same").unwrap();
        fs::write(root.join(name).join("nested/extra.md"), "same").unwrap();
    }
    assert!(skill_trees_match(&root.join("left"), &root.join("right")).unwrap());

    fs::write(root.join("right/nested/extra.md"), "different").unwrap();
    assert!(!skill_trees_match(&root.join("left"), &root.join("right")).unwrap());

    fs::write(root.join("right/nested/extra.md"), "same").unwrap();
    fs::write(root.join("right/added.md"), "new file").unwrap();
    assert!(!skill_trees_match(&root.join("left"), &root.join("right")).unwrap());
}
