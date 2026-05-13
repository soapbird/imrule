use std::fs;

use imrule::application::apply_use_case::get_agent_output_paths;
use imrule::application::ports::FileSystemPort;
use imrule::domain::agent::all_agents;
use imrule::domain::config::SubagentFrontmatter;
use imrule::domain::skills::{format_validation_warnings, get_skills_gitignore_paths};
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
    fs::create_dir_all(root.join(".ruler/skills/group/nested")).unwrap();
    fs::create_dir_all(root.join(".ruler/skills/solo")).unwrap();
    fs::create_dir_all(root.join(".ruler/skills/stray/empty")).unwrap();
    fs::write(root.join(".ruler/skills/group/nested/SKILL.md"), "nested").unwrap();
    fs::write(root.join(".ruler/skills/solo/SKILL.md"), "solo").unwrap();

    let discovered = discover_skills(root).unwrap();
    let names: Vec<_> = discovered
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["nested", "solo"]);
    assert_eq!(discovered.warnings, vec!["Directory 'stray' in .ruler/skills has no SKILL.md and contains no sub-skills. It may be malformed or stray."]);
    assert_eq!(format_validation_warnings(&discovered.warnings), "  - Directory 'stray' in .ruler/skills has no SKILL.md and contains no sub-skills. It may be malformed or stray.");

    copy_skills_directory(&root.join(".ruler/skills"), &root.join(".claude/skills")).unwrap();
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
    fs::create_dir_all(root.join(".ruler/agents")).unwrap();
    fs::write(root.join(".ruler/agents/bad.md"), "no frontmatter").unwrap();
    fs::write(
        root.join(".ruler/agents/good.md"),
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
