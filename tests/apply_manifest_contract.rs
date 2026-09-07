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
use imrule::application::ports::ManifestPort;
use imrule::domain::constants::{IMRULE_CACHE_PATH, IMRULE_MANIFEST_PATH};
use imrule::domain::manifest::{ApplyManifest, McpTarget, MANIFEST_VERSION};
use imrule::domain::mcp::{is_json_effectively_empty, is_native_mcp_content_empty};
use imrule::infrastructure::manifest::JsonApplyManifest;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

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
