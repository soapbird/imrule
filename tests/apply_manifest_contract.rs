//! Contract for the apply manifest: the record of a run's outputs that lets a
//! later run clean up what it no longer generates.
//!
//! Without it every output is derived from the current configuration, so
//! shrinking that configuration (dropping an MCP server, dropping an agent)
//! stranded the previous run's files — out of the managed `.gitignore` block,
//! out of `clear`'s reach, and into the next `git add .`.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use imrule::application::ports::FileSystemPort;
use imrule::application::ports::ManifestPort;
use imrule::domain::constants::{IMRULE_CACHE_PATH, IMRULE_MANIFEST_PATH};
use imrule::domain::manifest::{ApplyManifest, MANIFEST_VERSION, McpTarget, is_project_relative};
use imrule::domain::mcp::{is_json_effectively_empty, is_native_mcp_content_empty};
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::manifest::JsonApplyManifest;
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

// ---------------------------------------------------------------- domain ---

fn manifest(paths: &[&str], servers: &[&str], targets: &[(&str, &str)]) -> ApplyManifest {
    let root = PathBuf::from("/project");
    ApplyManifest::new(
        &root,
        &paths.iter().map(|p| root.join(p)).collect::<Vec<_>>(),
        &servers.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &targets
            .iter()
            .map(|(path, key)| (root.join(path), key.to_string()))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn manifest_records_project_relative_sorted_entries() {
    let recorded = manifest(
        &["CLAUDE.md", "AGENTS.md", "AGENTS.md"],
        &["notion", "linear"],
        &[(".mcp.json", "mcpServers"), (".mcp.json", "mcpServers")],
    );

    assert_eq!(recorded.version, MANIFEST_VERSION);
    assert_eq!(recorded.paths, vec!["AGENTS.md", "CLAUDE.md"]);
    assert_eq!(recorded.mcp_servers, vec!["linear", "notion"]);
    assert_eq!(
        recorded.mcp_targets,
        vec![McpTarget {
            path: ".mcp.json".to_string(),
            server_key: "mcpServers".to_string(),
        }]
    );
    assert!(recorded.is_readable());
}

#[test]
fn stale_paths_are_the_ones_the_current_run_no_longer_writes() {
    let previous = manifest(&["AGENTS.md", "CLAUDE.md", ".codex/skills"], &[], &[]);
    let current = manifest(&["CLAUDE.md"], &[], &[]);

    assert_eq!(
        previous.stale_paths(&current),
        vec![".codex/skills", "AGENTS.md"]
    );
    assert!(current.stale_paths(&previous).is_empty());
}

#[test]
fn a_path_still_written_as_an_mcp_target_is_not_stale() {
    // Native MCP configs appear both in `paths` and in `mcp_targets`. They are
    // reconciled by the MCP branch, which strips only imrule's keys, so the
    // generic path branch must not delete the whole file behind its back.
    let previous = manifest(&[".mcp.json"], &["linear"], &[(".mcp.json", "mcpServers")]);
    let current = manifest(&[], &["linear"], &[(".mcp.json", "mcpServers")]);

    assert!(previous.stale_paths(&current).is_empty());
    assert!(previous.stale_mcp_targets(&current).is_empty());
}

#[test]
fn stale_mcp_targets_and_servers_track_a_shrinking_config() {
    let previous = manifest(
        &[],
        &["linear", "notion"],
        &[(".mcp.json", "mcpServers"), ("kilo.jsonc", "mcp")],
    );
    let current = manifest(&[], &["linear"], &[(".mcp.json", "mcpServers")]);

    assert_eq!(
        previous.stale_mcp_targets(&current),
        vec![McpTarget {
            path: "kilo.jsonc".to_string(),
            server_key: "mcp".to_string(),
        }]
    );
    assert_eq!(previous.stale_mcp_servers(&current), vec!["notion"]);
}

#[test]
fn stale_skills_are_the_copies_the_current_run_no_longer_makes() {
    let root = PathBuf::from("/project");
    let copies = |names: &[&str]| {
        names
            .iter()
            .map(|name| root.join(".claude/skills").join(name))
            .collect::<Vec<_>>()
    };
    let previous = manifest(&[".claude/skills"], &[], &[])
        .with_skills(&root, &copies(&["rust-cli", "cli", "cli"]));
    let current = manifest(&[".claude/skills"], &[], &[]).with_skills(&root, &copies(&["cli"]));

    assert_eq!(
        previous.skills,
        vec![".claude/skills/cli", ".claude/skills/rust-cli"]
    );
    assert_eq!(
        previous.stale_skills(&current),
        vec![".claude/skills/rust-cli"]
    );
    // Skill copies are reconciled on their own, never through `paths`.
    assert!(previous.stale_paths(&current).is_empty());
    assert_eq!(current.merged_with(&previous).skills, previous.skills);
}

#[test]
fn native_mcp_emptiness_covers_json_and_toml_and_spares_user_data() {
    assert!(is_json_effectively_empty(&json!({"mcpServers": {}})));
    assert!(!is_json_effectively_empty(
        &json!({"$schema": "https://example.test/schema.json"})
    ));

    assert!(is_native_mcp_content_empty(""));
    assert!(is_native_mcp_content_empty("{\n  \"mcpServers\": {}\n}"));
    assert!(is_native_mcp_content_empty("[mcp_servers]\n"));
    assert!(!is_native_mcp_content_empty(
        "[mcp_servers.mine]\ncommand = \"node\"\n"
    ));
    assert!(!is_native_mcp_content_empty("model = \"gpt-5\"\n"));
    // Something that parses as neither is assumed to be the user's.
    assert!(!is_native_mcp_content_empty("not: [valid"));
}

// -------------------------------------------------------- infrastructure ---

#[test]
fn manifest_round_trips_and_degrades_to_none_when_unusable() {
    let temporary = tempdir().unwrap();
    let store = JsonApplyManifest::new();
    fs::create_dir_all(temporary.path().join(".imrule")).unwrap();

    assert_eq!(store.read_manifest(temporary.path()).unwrap(), None);

    let recorded = manifest(&["AGENTS.md"], &["linear"], &[(".mcp.json", "mcpServers")]);
    store.write_manifest(temporary.path(), &recorded).unwrap();
    assert_eq!(
        store.read_manifest(temporary.path()).unwrap(),
        Some(recorded)
    );

    let path = temporary.path().join(IMRULE_MANIFEST_PATH);
    fs::write(&path, "{ not json").unwrap();
    assert_eq!(store.read_manifest(temporary.path()).unwrap(), None);

    fs::write(&path, json!({"version": 999}).to_string()).unwrap();
    assert_eq!(store.read_manifest(temporary.path()).unwrap(), None);

    store.remove_manifest(temporary.path()).unwrap();
    assert!(!path.exists());
    // Removing an absent manifest is a no-op, not an error.
    store.remove_manifest(temporary.path()).unwrap();
}

#[test]
fn a_manifest_written_before_skill_copies_were_recorded_still_loads() {
    // Releases before `skills` was added wrote no such key. Degrading such a
    // manifest to `None` would silently skip every prune on the first run
    // after upgrading, so it must load with an empty skill list instead.
    let temporary = tempdir().unwrap();
    let store = JsonApplyManifest::new();
    fs::create_dir_all(temporary.path().join(".imrule")).unwrap();
    fs::write(
        temporary.path().join(IMRULE_MANIFEST_PATH),
        json!({
            "version": MANIFEST_VERSION,
            "paths": ["CLAUDE.md", ".claude/skills"],
            "mcp_servers": ["linear"],
            "mcp_targets": [{"path": ".mcp.json", "server_key": "mcpServers"}],
        })
        .to_string(),
    )
    .unwrap();

    let loaded = store
        .read_manifest(temporary.path())
        .unwrap()
        .expect("a pre-skills manifest is still usable");
    assert_eq!(loaded.paths, vec!["CLAUDE.md", ".claude/skills"]);
    assert!(loaded.skills.is_empty());
}

#[test]
fn skill_and_mcp_entries_that_could_reach_outside_the_project_are_dropped_on_read() {
    let recorded = ApplyManifest {
        version: MANIFEST_VERSION,
        paths: vec![
            "CLAUDE.md".to_string(),
            "../victim".to_string(),
            "/etc".to_string(),
            "docs/../../victim".to_string(),
            String::new(),
        ],
        mcp_servers: vec!["linear".to_string()],
        mcp_targets: vec![
            McpTarget {
                path: ".mcp.json".to_string(),
                server_key: "mcpServers".to_string(),
            },
            McpTarget {
                path: "../.mcp.json".to_string(),
                server_key: "mcpServers".to_string(),
            },
        ],
        skills: vec![
            ".claude/skills/cli".to_string(),
            ".codex/skills/python-cli".to_string(),
            ".claude/skills/../../victim".to_string(),
            ".claude/skills".to_string(),
            ".claude/skills/python/cli".to_string(),
            "docs/cli".to_string(),
            "/elsewhere/.claude/skills/cli".to_string(),
        ],
    };

    let (kept, dropped) = recorded.clone().without_escaping_entries();

    // Paths stay known, so an output written outside the project is not
    // forgotten; apply checks each one on disk before deleting it. Native MCP
    // configs always live inside the project, so an MCP target that does not
    // is dropped with the skill copies.
    assert_eq!(kept.paths, recorded.paths);
    assert_eq!(
        kept.mcp_targets,
        vec![McpTarget {
            path: ".mcp.json".to_string(),
            server_key: "mcpServers".to_string(),
        }]
    );
    assert_eq!(
        kept.skills,
        vec![".claude/skills/cli", ".codex/skills/python-cli"]
    );
    assert_eq!(dropped, 6);
    for (entry, contained) in [
        ("CLAUDE.md", true),
        (".claude/skills", true),
        ("../victim", false),
        ("/etc", false),
        ("docs/../../victim", false),
        ("", false),
    ] {
        assert_eq!(is_project_relative(entry), contained, "{entry:?}");
    }
}

#[test]
fn stale_skills_compare_names_exactly_and_leave_case_to_the_disk() {
    // On a case-sensitive filesystem `Foo` and `foo` are two copies and the old
    // one must go; on a case-insensitive one apply sees they are one directory.
    let previous = ApplyManifest {
        skills: vec![
            ".claude/skills/Foo".to_string(),
            ".claude/skills/gone".to_string(),
        ],
        ..ApplyManifest::default()
    };
    let current = ApplyManifest {
        skills: vec![".claude/skills/foo".to_string()],
        ..ApplyManifest::default()
    };

    assert_eq!(
        previous.stale_skills(&current),
        vec![".claude/skills/Foo", ".claude/skills/gone"]
    );
}

#[cfg(unix)]
#[test]
fn the_filesystem_port_resolves_links_before_answering_containment_and_identity() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("project");
    let outside = workspace.path().join("outside");
    fs::create_dir_all(root.join("real/skill")).unwrap();
    fs::create_dir_all(outside.join("victim")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("docs")).unwrap();
    std::os::unix::fs::symlink(root.join("real/skill"), root.join("real/alias")).unwrap();
    let fs_port = FsFileSystem::new();

    assert!(fs_port.resolves_within(&root.join("real/skill"), &root));
    assert!(!fs_port.resolves_within(&root.join("docs/victim"), &root));
    // A link at the path itself is inside; removing it never follows it.
    assert!(fs_port.resolves_within(&root.join("docs"), &root));
    assert!(!fs_port.resolves_within(&root.join("missing"), &root));

    assert!(fs_port.is_same_entry(&root.join("real/skill"), &root.join("real/alias")));
    assert!(!fs_port.is_same_entry(&root.join("real/skill"), &root.join("real")));
    assert!(!fs_port.is_same_entry(&root.join("real/skill"), &root.join("missing")));
}

#[cfg(unix)]
#[test]
fn a_committed_symlink_cannot_carry_a_manifest_entry_outside_the_project() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("project");
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# rules\n").unwrap();
    let outside = workspace.path().join("outside");
    for victim in ["Documents", "skills/keep"] {
        fs::create_dir_all(outside.join(victim)).unwrap();
        fs::write(outside.join(victim).join("keep"), "keep").unwrap();
    }
    std::os::unix::fs::symlink(&outside, root.join("docs")).unwrap();
    apply(&root, &[]);

    // `.claude` swapped for a link after the first run, as a pull could do.
    fs::remove_dir_all(root.join(".claude")).ok();
    std::os::unix::fs::symlink(&outside, root.join(".claude")).unwrap();
    let manifest_path = root.join(IMRULE_MANIFEST_PATH);
    let mut recorded: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    recorded["paths"] = json!(["docs/Documents"]);
    recorded["skills"] = json!([".claude/skills/keep"]);
    fs::write(&manifest_path, recorded.to_string()).unwrap();
    apply(&root, &[]);

    assert!(outside.join("Documents/keep").exists(), "followed docs/");
    assert!(
        outside.join("skills/keep/keep").exists(),
        "followed .claude/"
    );
}

/// A project wired to claude only, beside an `outside` directory the tests
/// try to reach through a tampered manifest.
fn project_beside_outside() -> (TempDir, PathBuf, PathBuf) {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("project");
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# rules\n").unwrap();
    let outside = workspace.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    (workspace, root, outside)
}

fn tamper_manifest(root: &std::path::Path, edit: impl Fn(&mut Value)) {
    let manifest_path = root.join(IMRULE_MANIFEST_PATH);
    let mut recorded: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    edit(&mut recorded);
    fs::write(&manifest_path, recorded.to_string()).unwrap();
}

fn clear(root: &std::path::Path) {
    Command::cargo_bin("imrule")
        .unwrap()
        .args(["clear", "--project-root", root.to_str().unwrap()])
        .assert()
        .success();
}

const OUTSIDE_MCP: &str =
    "{\"mcpServers\": {\"github\": {\"command\": \"gh\"}, \"mine\": {\"command\": \"mine\"}}}";
const OUTSIDE_RULES: &str = "<!-- Generated by ImRule -->\nanother project's rules\n";

#[test]
fn a_manifest_cannot_point_apply_or_clear_at_files_outside_the_project() {
    let (_workspace, root, outside) = project_beside_outside();
    let victim_config = outside.join(".mcp.json");
    let victim_rules = outside.join("CLAUDE.md");
    fs::write(&victim_config, OUTSIDE_MCP).unwrap();
    fs::write(&victim_rules, OUTSIDE_RULES).unwrap();
    apply(&root, &[]);

    let point_outside = |recorded: &mut Value| {
        recorded["mcp_servers"] = json!(["github", "mine"]);
        recorded["mcp_targets"] = json!([
            {"path": "../outside/.mcp.json", "server_key": "mcpServers"},
            {"path": victim_config.to_str().unwrap(), "server_key": "mcpServers"},
        ]);
        recorded["paths"] = json!(["../outside/CLAUDE.md", victim_rules.to_str().unwrap()]);
    };
    tamper_manifest(&root, point_outside);
    apply(&root, &[]);
    assert_eq!(fs::read_to_string(&victim_config).unwrap(), OUTSIDE_MCP);
    assert_eq!(fs::read_to_string(&victim_rules).unwrap(), OUTSIDE_RULES);

    tamper_manifest(&root, point_outside);
    clear(&root);
    assert_eq!(fs::read_to_string(&victim_config).unwrap(), OUTSIDE_MCP);
    assert_eq!(fs::read_to_string(&victim_rules).unwrap(), OUTSIDE_RULES);
}

#[cfg(unix)]
#[test]
fn a_symlink_cannot_carry_an_mcp_rewrite_or_a_marked_file_outside_the_project() {
    let (_workspace, root, outside) = project_beside_outside();
    let victim_config = outside.join("mcp.json");
    fs::write(&victim_config, OUTSIDE_MCP).unwrap();
    fs::write(outside.join("CLAUDE.md"), OUTSIDE_RULES).unwrap();
    apply(&root, &[]);
    fs::create_dir_all(root.join(".cursor")).unwrap();
    std::os::unix::fs::symlink(&victim_config, root.join(".cursor/mcp.json")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("docs")).unwrap();

    let through_links = |recorded: &mut Value| {
        recorded["mcp_servers"] = json!(["github", "mine"]);
        recorded["mcp_targets"] = json!([{"path": ".cursor/mcp.json", "server_key": "mcpServers"}]);
        recorded["paths"] = json!(["docs/CLAUDE.md"]);
    };
    tamper_manifest(&root, through_links);
    apply(&root, &[]);
    assert_eq!(fs::read_to_string(&victim_config).unwrap(), OUTSIDE_MCP);
    assert_eq!(
        fs::read_to_string(outside.join("CLAUDE.md")).unwrap(),
        OUTSIDE_RULES
    );

    tamper_manifest(&root, through_links);
    clear(&root);
    assert_eq!(fs::read_to_string(&victim_config).unwrap(), OUTSIDE_MCP);
}

#[test]
fn a_skills_root_kept_for_hand_placed_skills_stays_ignored() {
    // A 0.4.2 manifest records the root but not the copies it made, so after
    // the last skill is removed the root survives with those copies inside.
    let temporary = project("\"claude\"", "");
    let root = temporary.path();
    write_skill(root, "cli");
    apply(root, &[]);
    let manifest_path = root.join(IMRULE_MANIFEST_PATH);
    let mut recorded: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    recorded.as_object_mut().unwrap().remove("skills");
    fs::write(&manifest_path, recorded.to_string()).unwrap();

    fs::remove_dir_all(root.join(".imrule/skills")).unwrap();
    apply(root, &[]);

    assert!(root.join(".claude/skills/cli/SKILL.md").exists());
    assert!(
        ignore_block(root)
            .iter()
            .any(|line| line.contains(".claude/skills")),
        "the surviving root fell out of .gitignore: {:?}",
        ignore_block(root)
    );
}

#[test]
fn a_tampered_manifest_cannot_delete_anything_outside_the_project() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("project");
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "# rules\n").unwrap();
    let victims = ["victim-skill", "victim-path"].map(|name| workspace.path().join(name));
    for victim in &victims {
        fs::create_dir_all(victim.join("deep")).unwrap();
        fs::write(victim.join("deep/keep"), "keep").unwrap();
    }
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("keep"), "keep").unwrap();
    let outside_path = outside.path().to_str().unwrap();
    apply(&root, &[]);

    // A manifest is a plain file; a cloned repository can ship any content.
    let manifest_path = root.join(IMRULE_MANIFEST_PATH);
    let mut recorded: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    recorded["skills"] = json!([
        "../victim-skill/deep",
        ".claude/skills/../../victim-skill",
        outside_path,
    ]);
    recorded["paths"] = json!(["../victim-path", outside_path]);
    fs::write(&manifest_path, recorded.to_string()).unwrap();
    apply(&root, &[]);

    for victim in &victims {
        assert!(victim.join("deep/keep").exists(), "{victim:?} was deleted");
    }
    assert!(
        outside.path().join("keep").exists(),
        "{outside_path} was deleted"
    );
}

#[test]
fn removing_every_skill_keeps_the_skills_placed_in_an_agent_root_by_hand() {
    let temporary = project("\"claude\", \"codex\"", "");
    let root = temporary.path();
    write_skill(root, "cli");
    fs::create_dir_all(root.join(".claude/skills/mine")).unwrap();
    fs::write(root.join(".claude/skills/mine/SKILL.md"), "mine").unwrap();
    apply(root, &[]);
    assert!(root.join(".codex/skills/cli/SKILL.md").exists());

    fs::remove_dir_all(root.join(".imrule/skills")).unwrap();
    apply(root, &[]);
    assert!(!root.join(".claude/skills/cli").exists());
    assert!(
        root.join(".claude/skills/mine/SKILL.md").exists(),
        "pruning the last copy removed a skill imrule never copied"
    );
    assert!(
        !root.join(".codex/skills").exists(),
        "a skills root left empty should still be pruned"
    );

    // Turning skills off prunes the same way as removing them.
    write_skill(root, "cli");
    apply(root, &[]);
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\", \"codex\"]\n\n[skills]\nenabled = false\n",
    )
    .unwrap();
    apply(root, &[]);
    assert!(!root.join(".claude/skills/cli").exists());
    assert!(root.join(".claude/skills/mine/SKILL.md").exists());
}

#[test]
fn copies_left_under_a_pre_0_5_leaf_name_are_removed_only_when_they_match_the_source() {
    let temporary = project("\"claude\", \"codex\"", "");
    let root = temporary.path();
    write_skill(root, "python/cli");
    // What 0.4.2 left behind: the grouped skill copied under its leaf name,
    // once untouched and once edited in the agent directory.
    for (skills_root, content) in [
        (".claude/skills", "python/cli"),
        (".codex/skills", "edited by hand"),
    ] {
        let legacy = root.join(skills_root).join("cli");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("SKILL.md"), content).unwrap();
    }

    apply(root, &[]);

    assert!(root.join(".claude/skills/python-cli/SKILL.md").exists());
    assert!(
        !root.join(".claude/skills/cli").exists(),
        "an identical pre-0.5 copy was left behind"
    );
    assert_eq!(
        fs::read_to_string(root.join(".codex/skills/cli/SKILL.md")).unwrap(),
        "edited by hand"
    );
}

// ------------------------------------------------------------------- cli ---

/// A project wired to two agents and one stdio MCP server. Stdio is deliberate:
/// a remote server would trigger `mcp-remote` version resolution over the
/// network, which has nothing to do with what these tests assert.
fn project(agents: &str, servers: &str) -> TempDir {
    let temporary = tempdir().unwrap();
    fs::create_dir_all(temporary.path().join(".imrule")).unwrap();
    fs::write(
        temporary.path().join(".imrule/imrule.toml"),
        format!("default_agents = [{agents}]\n\n{servers}"),
    )
    .unwrap();
    fs::write(temporary.path().join(".imrule/AGENTS.md"), "# rules\n").unwrap();
    fs::write(
        temporary.path().join(".gitignore"),
        "# Python\n__pycache__/\n",
    )
    .unwrap();
    temporary
}

const LINEAR_AND_NOTION: &str = "[mcp_servers.linear]\n\
     transport = \"stdio\"\n\
     command = \"linear-mcp\"\n\n\
     [mcp_servers.notion]\n\
     transport = \"stdio\"\n\
     command = \"notion-mcp\"\n";

fn apply(root: &std::path::Path, extra: &[&str]) {
    let mut command = Command::cargo_bin("imrule").unwrap();
    command.args(["apply", "--project-root", root.to_str().unwrap()]);
    command.args(extra);
    command.assert().success();
}

fn ignore_block(root: &std::path::Path) -> Vec<String> {
    let content = fs::read_to_string(root.join(".gitignore")).unwrap();
    content
        .lines()
        .skip_while(|line| line.trim() != "# START ImRule Generated Files")
        .skip(1)
        .take_while(|line| line.trim() != "# END ImRule Generated Files")
        .map(str::to_string)
        .collect()
}

#[test]
fn dropping_every_mcp_server_removes_the_configs_the_previous_apply_wrote() {
    let temporary = project("\"claude\", \"codex\", \"opencode\"", LINEAR_AND_NOTION);
    let root = temporary.path();

    apply(root, &[]);
    assert!(root.join(".mcp.json").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(root.join("opencode.json").exists());
    assert!(ignore_block(root).contains(&"/.mcp.json".to_string()));

    // The user removes MCP from the config entirely.
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\", \"codex\", \"opencode\"]\n",
    )
    .unwrap();
    apply(root, &[]);

    // Previously the files stayed on disk while dropping out of the ignore
    // block, so `git add .` swept them into the next commit.
    assert!(!root.join(".mcp.json").exists());
    assert!(!root.join(".codex/config.toml").exists());
    assert!(!root.join("opencode.json").exists());
    let block = ignore_block(root);
    assert!(!block.iter().any(|line| line.contains("mcp")));
    assert!(block.contains(&"/CLAUDE.md".to_string()));
}

#[test]
fn dropping_one_mcp_server_leaves_the_others_and_the_users_own_untouched() {
    let temporary = project("\"claude\"", LINEAR_AND_NOTION);
    let root = temporary.path();

    fs::write(
        root.join(".mcp.json"),
        json!({"mcpServers": {"mine": {"command": "node"}}}).to_string(),
    )
    .unwrap();
    apply(root, &[]);

    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n\n\
         [mcp_servers.linear]\n\
         transport = \"stdio\"\n\
         command = \"linear-mcp\"\n",
    )
    .unwrap();
    apply(root, &[]);

    let native: Value =
        serde_json::from_str(&fs::read_to_string(root.join(".mcp.json")).unwrap()).unwrap();
    let servers = native["mcpServers"].as_object().unwrap();
    assert!(servers.contains_key("linear"));
    assert!(
        servers.contains_key("mine"),
        "user's own server was removed"
    );
    assert!(
        !servers.contains_key("notion"),
        "a server dropped from the config lingered: merge only ever adds keys"
    );
}

#[test]
fn dropping_an_agent_removes_its_generated_files_and_prunes_its_directory() {
    let temporary = project("\"claude\", \"codex\"", LINEAR_AND_NOTION);
    let root = temporary.path();

    apply(root, &[]);
    assert!(root.join("AGENTS.md").exists());
    assert!(root.join(".codex/config.toml").exists());

    fs::write(
        root.join(".imrule/imrule.toml"),
        format!("default_agents = [\"claude\"]\n\n{LINEAR_AND_NOTION}"),
    )
    .unwrap();
    apply(root, &[]);

    assert!(!root.join("AGENTS.md").exists());
    assert!(!root.join(".codex").exists(), "empty parent was not pruned");
    assert!(root.join("CLAUDE.md").exists());
    assert!(root.join(".mcp.json").exists());
}

#[test]
fn a_generated_file_the_user_has_taken_over_is_never_removed() {
    let temporary = project("\"claude\", \"codex\"", "");
    let root = temporary.path();

    apply(root, &[]);
    // The user drops the generated marker and makes the file their own.
    fs::write(root.join("AGENTS.md"), "my own notes\n").unwrap();

    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    apply(root, &[]);

    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "my own notes\n"
    );
}

fn write_skill(root: &std::path::Path, dir: &str) {
    let path = root.join(".imrule/skills").join(dir);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), dir).unwrap();
}

#[test]
fn grouped_skills_publish_under_path_names_and_dropped_ones_are_pruned() {
    let temporary = project("\"claude\", \"codex\"", "");
    let root = temporary.path();
    write_skill(root, "cli");
    write_skill(root, "python/cli");
    write_skill(root, "rust/cli");
    // A skill the user put into the agent root by hand, not through imrule.
    fs::create_dir_all(root.join(".claude/skills/mine")).unwrap();
    fs::write(root.join(".claude/skills/mine/SKILL.md"), "mine").unwrap();

    apply(root, &[]);
    for skills_root in [".claude/skills", ".codex/skills"] {
        for (published, source) in [
            ("cli", "cli"),
            ("python-cli", "python/cli"),
            ("rust-cli", "rust/cli"),
        ] {
            assert_eq!(
                fs::read_to_string(root.join(skills_root).join(published).join("SKILL.md"))
                    .unwrap(),
                source,
                "{skills_root}/{published}"
            );
        }
    }

    fs::remove_dir_all(root.join(".imrule/skills/rust")).unwrap();
    apply(root, &[]);

    assert!(!root.join(".claude/skills/rust-cli").exists());
    assert!(!root.join(".codex/skills/rust-cli").exists());
    assert!(root.join(".claude/skills/python-cli").exists());
    assert!(
        root.join(".claude/skills/mine/SKILL.md").exists(),
        "a skill imrule never copied was removed"
    );
}

#[test]
fn skills_that_would_publish_under_one_name_fail_apply_and_list() {
    let temporary = project("\"claude\"", "");
    let root = temporary.path();
    write_skill(root, "python/cli");
    write_skill(root, "python-cli");

    for command in [&["apply"][..], &["skills", "list"][..]] {
        let output = Command::cargo_bin("imrule")
            .unwrap()
            .args(command)
            .args(["--project-root", root.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{command:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr
                .contains("'python/cli' and 'python-cli' would both be published as 'python-cli'"),
            "{command:?}: {stderr}"
        );
    }
    assert!(
        !root.join(".claude/skills").exists(),
        "neither colliding skill may be copied"
    );
    assert!(
        !root.join("CLAUDE.md").exists(),
        "apply wrote rule files before failing on the collision"
    );
}

#[test]
fn skill_copies_survive_narrowed_and_dry_runs_until_a_full_run_prunes_them() {
    let temporary = project("\"claude\", \"codex\"", "");
    let root = temporary.path();
    write_skill(root, "keep");
    write_skill(root, "rust/cli");
    apply(root, &[]);

    fs::remove_dir_all(root.join(".imrule/skills/rust")).unwrap();
    apply(root, &["--dry-run"]);
    assert!(root.join(".claude/skills/rust-cli").exists());
    assert!(root.join(".codex/skills/rust-cli").exists());

    // A narrowed run never prunes, and folds the copies it did not make into
    // the manifest, so the codex copy is still known to the next full run.
    apply(root, &["--agents", "claude"]);
    assert!(root.join(".claude/skills/rust-cli").exists());
    assert!(root.join(".codex/skills/rust-cli").exists());

    apply(root, &[]);
    assert!(!root.join(".claude/skills/rust-cli").exists());
    assert!(
        !root.join(".codex/skills/rust-cli").exists(),
        "the codex copy recorded before the narrowed run was forgotten"
    );
    assert!(root.join(".claude/skills/keep/SKILL.md").exists());
    assert!(root.join(".codex/skills/keep/SKILL.md").exists());
}

#[test]
fn dry_run_neither_prunes_nor_records() {
    let temporary = project("\"claude\", \"codex\"", LINEAR_AND_NOTION);
    let root = temporary.path();

    apply(root, &[]);
    let recorded = fs::read_to_string(root.join(IMRULE_MANIFEST_PATH)).unwrap();

    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\"]\n",
    )
    .unwrap();
    apply(root, &["--dry-run"]);

    assert!(root.join("AGENTS.md").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(root.join(".mcp.json").exists());
    assert_eq!(
        fs::read_to_string(root.join(IMRULE_MANIFEST_PATH)).unwrap(),
        recorded
    );
}

#[test]
fn narrowing_a_run_with_agents_never_deletes_the_agents_it_skipped() {
    let temporary = project("\"claude\", \"codex\", \"opencode\"", LINEAR_AND_NOTION);
    let root = temporary.path();

    apply(root, &[]);
    apply(root, &["--agents", "claude"]);

    // `--agents` scopes one invocation; it does not say codex and opencode were
    // dropped from the project.
    assert!(root.join("AGENTS.md").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(root.join("opencode.json").exists());
    assert!(root.join("CLAUDE.md").exists());

    // The narrowed run still records what it wrote, by folding into the
    // existing manifest rather than replacing it.
    let recorded: Value =
        serde_json::from_str(&fs::read_to_string(root.join(IMRULE_MANIFEST_PATH)).unwrap())
            .unwrap();
    let paths: Vec<&str> = recorded["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(paths.contains(&"AGENTS.md"));
    assert!(paths.contains(&"CLAUDE.md"));

    // A later full run sees no drift and still leaves everything in place.
    apply(root, &[]);
    assert!(root.join("AGENTS.md").exists());
    assert!(root.join(".codex/config.toml").exists());
}

#[test]
fn narrowing_a_clear_with_agents_never_deletes_the_agents_it_skipped() {
    let temporary = project("\"claude\", \"codex\"", LINEAR_AND_NOTION);
    let root = temporary.path();
    apply(root, &[]);

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            root.to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    assert!(!root.join("CLAUDE.md").exists());
    assert!(root.join("AGENTS.md").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(
        root.join(IMRULE_MANIFEST_PATH).exists(),
        "the manifest still accounts for the agents this clear skipped"
    );
}

#[test]
fn generated_state_files_are_ignored_but_imrule_sources_are_not() {
    let temporary = project("\"claude\"", LINEAR_AND_NOTION);
    let root = temporary.path();
    apply(root, &[]);

    let block = ignore_block(root);
    assert!(block.contains(&format!("/{IMRULE_MANIFEST_PATH}")));
    assert!(
        !block
            .iter()
            .any(|line| line.contains("AGENTS.md") && line.contains(".imrule")),
        "the .imrule/ source directory must stay committable"
    );
}

#[test]
fn clear_reaches_mcp_configs_the_config_no_longer_describes() {
    let temporary = project("\"claude\", \"codex\", \"opencode\"", LINEAR_AND_NOTION);
    let root = temporary.path();
    apply(root, &[]);

    // The servers are gone from the config, so `clear` has no keys to look for
    // — except the ones the manifest remembers.
    fs::write(
        root.join(".imrule/imrule.toml"),
        "default_agents = [\"claude\", \"codex\", \"opencode\"]\n",
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args(["clear", "--project-root", root.to_str().unwrap()])
        .assert()
        .success();

    assert!(!root.join(".mcp.json").exists());
    assert!(!root.join("opencode.json").exists());
    assert!(!root.join(".codex").exists());
    assert!(!root.join(IMRULE_MANIFEST_PATH).exists());
    assert!(!root.join(IMRULE_CACHE_PATH).exists());
    assert!(ignore_block(root).is_empty());
    assert!(
        root.join(".imrule/AGENTS.md").exists(),
        "clear must leave the source directory alone"
    );
}
