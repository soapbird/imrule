use std::fs;

use imrule::application::apply_use_case::get_agent_output_paths;
use imrule::application::ports::FileSystemPort;
use imrule::domain::agent::all_agents;
use imrule::domain::config::SubagentFrontmatter;
use imrule::domain::skills::{
    format_validation_warnings, get_skills_gitignore_paths, parse_skill_source, RemoteSkillSource,
};
use imrule::domain::subagent::{
    build_claude_file, build_codex_file, build_copilot_file, build_cursor_file,
    map_tools_for_copilot, parse_frontmatter, validate_frontmatter,
};
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::skills::{copy_skills_directory, discover_skills};
use imrule::infrastructure::subagents::{
    discover_subagents, get_subagents_gitignore_paths, load_subagent_file,
};
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
    assert_eq!(names, vec!["nested", "solo"]);
    assert_eq!(discovered.warnings, vec!["Directory 'stray' in skills has no SKILL.md and contains no sub-skills. It may be malformed or stray."]);
    assert_eq!(format_validation_warnings(&discovered.warnings), "  - Directory 'stray' in skills has no SKILL.md and contains no sub-skills. It may be malformed or stray.");

    copy_skills_directory(&root.join(".imrule/skills"), &root.join(".claude/skills")).unwrap();
    assert_eq!(
        fs::read_to_string(root.join(".claude/skills/solo/SKILL.md")).unwrap(),
        "solo"
    );

    let agents = all_agents();
    let selected: Vec<_> = agents
        .iter()
        .filter(|agent| ["claude", "codex", "mistral", "factory"].contains(&agent.identifier))
        .copied()
        .collect();
    assert_eq!(
        get_skills_gitignore_paths(root, &selected),
        vec![
            root.join(".claude/skills"),
            root.join(".codex/skills"),
            root.join(".vibe/skills"),
            root.join(".factory/skills"),
        ]
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
        get_subagents_gitignore_paths(root, &selected).unwrap(),
        vec![
            root.join(".claude/agents"),
            root.join(".cursor/agents"),
            root.join(".codex/agents"),
            root.join(".github/agents"),
        ]
    );
}

#[test]
fn apply_path_collection_and_revert_file_operations_match_contract() {
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

#[test]
fn parses_github_shorthand_source() {
    let source = parse_skill_source("vercel-labs/agent-skills").unwrap();
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
    let source = parse_skill_source("https://github.com/vercel-labs/agent-skills").unwrap();
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
        parse_skill_source("https://github.com/vercel-labs/agent-skills/tree/main/skills/design")
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
    let source = parse_skill_source("https://gitlab.com/org/repo").unwrap();
    assert_eq!(
        source,
        RemoteSkillSource::Gitlab {
            url: "https://gitlab.com/org/repo".into(),
        }
    );
}

#[test]
fn parses_git_ssh_source() {
    let source = parse_skill_source("git@github.com:vercel-labs/agent-skills.git").unwrap();
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
    let source = parse_skill_source(local_path.to_str().unwrap()).unwrap();
    match source {
        RemoteSkillSource::Local { path } => {
            assert_eq!(path, local_path);
        }
        _ => panic!("expected Local variant"),
    }
}

#[test]
fn parses_relative_path_source() {
    let source = parse_skill_source("./my-skills").unwrap();
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
    assert!(parse_skill_source("invalid-no-slash").is_err());
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
    let use_case =
        imrule::application::skills_add_use_case::SkillsAddUseCase::new(&fetcher, &fs_port);

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: None,
            list_only: false,
            global: false,
            verbose: false,
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
    let use_case =
        imrule::application::skills_add_use_case::SkillsAddUseCase::new(&fetcher, &fs_port);

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: None,
            list_only: true,
            global: false,
            verbose: false,
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
    let use_case =
        imrule::application::skills_add_use_case::SkillsAddUseCase::new(&fetcher, &fs_port);

    let result = use_case
        .execute(imrule::application::skills_add_use_case::SkillsAddOptions {
            project_root: root.to_path_buf(),
            source: source_dir.to_string_lossy().to_string(),
            skill_names: Some(vec!["skill-a".into()]),
            list_only: false,
            global: false,
            verbose: false,
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
