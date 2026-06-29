//! Gap-coverage tests for the `imrule clear` command.
//!
//! These tests cover code paths in `src/application/clear_use_case.rs` that were
//! not exercised by the existing tests in `cli_release_contract.rs`:
//!
//!   - Clear without --agents (defaults to ALL agents via all_agents())
//!   - Clear when no generated files exist (empty state / no-op)
//!   - Clear --remove-source with legacy .ruler directory
//!   - Clear --remove-source when neither .imrule nor .ruler exist
//!   - Clear --dry-run with subagents (dirs must survive)
//!   - Clear --dry-run with skills (dirs must survive)
//!   - Clear --dry-run with MCP config (file must survive unmodified)
//!   - Clear when mcp.json exists but has no mcpServers key
//!   - Clear removes .bak backup files alongside generated files

use std::fs;

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn clear_without_agents_flag_targets_all_agents() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "All-agents clear test.",
    )
    .unwrap();

    // Apply without --agents (already defaults to all, but we also apply explicitly for a few).
    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude,cline",
        ])
        .assert()
        .success();

    assert!(tmp.path().join("CLAUDE.md").exists());
    assert!(tmp.path().join(".clinerules").exists());

    // Clear WITHOUT --agents — should target all agents.
    Command::cargo_bin("imrule")
        .unwrap()
        .args(["clear", "--project-root", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    // Generated files for claude and cline should be removed.
    assert!(!tmp.path().join("CLAUDE.md").exists());
    assert!(!tmp.path().join(".clinerules").exists());

    // .imrule/ source preserved by default.
    assert!(tmp.path().join(".imrule/AGENTS.md").exists());
}

#[test]
fn clear_when_no_generated_files_is_a_noop() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Nothing to clear.").unwrap();

    // Do NOT apply. Clear on a project with no generated files.
    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    // .imrule/ must be untouched.
    assert!(tmp.path().join(".imrule/AGENTS.md").exists());
    // No CLAUDE.md should have been created.
    assert!(!tmp.path().join("CLAUDE.md").exists());
}

#[test]
fn clear_remove_source_deletes_legacy_ruler_dir() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".ruler")).unwrap();
    fs::write(
        tmp.path().join(".ruler/AGENTS.md"),
        "Legacy source removal.",
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    assert!(tmp.path().join("CLAUDE.md").exists());

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--remove-source",
        ])
        .assert()
        .success();

    assert!(!tmp.path().join("CLAUDE.md").exists());
    // .ruler/ legacy source directory must also be removed.
    assert!(!tmp.path().join(".ruler").exists());
}

#[test]
fn clear_remove_source_when_no_source_dirs_is_safe() {
    let tmp = tempdir().unwrap();
    // Do NOT create .imrule or .ruler — --remove-source should still succeed.
    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--remove-source",
        ])
        .assert()
        .success();
}

#[test]
fn clear_dry_run_preserves_subagent_directories() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule/agents")).unwrap();
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Subagent dry-run test.",
    )
    .unwrap();
    fs::write(
        tmp.path().join(".imrule/agents/coder.md"),
        "---\nname: coder\ndescription: Coder bot\n---\n\nDo stuff.\n",
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    assert!(tmp.path().join(".claude/agents/coder.md").exists());

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--dry-run",
        ])
        .assert()
        .success();

    // Subagent directory must survive dry-run.
    assert!(tmp.path().join(".claude/agents/coder.md").exists());
}

#[test]
fn clear_dry_run_preserves_skills_directories() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule/skills/util")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Skills dry-run.").unwrap();
    fs::write(
        tmp.path().join(".imrule/skills/util/SKILL.md"),
        "# Util skill",
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    assert!(tmp.path().join(".claude/skills/util/SKILL.md").exists());

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--dry-run",
        ])
        .assert()
        .success();

    // Skills directory must survive dry-run.
    assert!(tmp.path().join(".claude/skills/util/SKILL.md").exists());
}

#[test]
fn clear_dry_run_preserves_mcp_config() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "MCP dry-run.").unwrap();
    fs::write(
        tmp.path().join(".imrule/mcp.json"),
        r#"{"mcpServers":{"dry-tool":{"command":"node","args":["x.js"]}}}"#,
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
        ])
        .assert()
        .success();

    assert!(tmp.path().join(".cursor/mcp.json").exists());

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
            "--dry-run",
        ])
        .assert()
        .success();

    // MCP config must survive dry-run with imrule keys still present.
    let content = fs::read_to_string(tmp.path().join(".cursor/mcp.json")).unwrap();
    let config: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(config["mcpServers"]["dry-tool"].is_object());
}

#[test]
fn clear_with_empty_mcp_json_skips_mcp_cleanup() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Empty MCP test.").unwrap();
    // mcp.json exists but has no mcpServers — collect_mcp_keys returns empty vec.
    fs::write(tmp.path().join(".imrule/mcp.json"), r#"{"other":"data"}"#).unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    // Clear should succeed even though MCP keys list is empty.
    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    assert!(!tmp.path().join("CLAUDE.md").exists());
}

#[test]
fn clear_removes_mcp_servers_defined_in_imrule_toml() {
    // Regression: servers added via `imrule mcp add` land in the `[mcp_servers]`
    // table of imrule.toml (NOT .imrule/mcp.json). `apply` writes them into every
    // native agent config, so `clear` must enumerate them too. Previously
    // collect_mcp_keys read only .imrule/mcp.json and orphaned these servers.
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "TOML MCP test.").unwrap();
    // Mirrors what `imrule mcp add toml-tool --transport http --url ...` writes.
    fs::write(
        tmp.path().join(".imrule/imrule.toml"),
        "[mcp_servers.toml-tool]\ntransport = \"http\"\nurl = \"https://example.com/mcp\"\n",
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
        ])
        .assert()
        .success();

    // apply must have written the TOML-defined server into the native config.
    let applied = fs::read_to_string(tmp.path().join(".cursor/mcp.json")).unwrap();
    let applied: serde_json::Value = serde_json::from_str(&applied).unwrap();
    assert!(
        applied["mcpServers"]["toml-tool"].is_object(),
        "apply should write the imrule.toml MCP server into the native config"
    );

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
        ])
        .assert()
        .success();

    // clear must remove the TOML-defined server. The native file is removed once
    // empty; if it survives, the server key must be gone.
    let native = tmp.path().join(".cursor/mcp.json");
    if native.exists() {
        let content = fs::read_to_string(&native).unwrap();
        let config: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(
            config
                .get("mcpServers")
                .and_then(|s| s.get("toml-tool"))
                .is_none(),
            "clear must remove the imrule.toml MCP server from the native config"
        );
    }
}

#[test]
fn clear_removes_backup_files_alongside_generated() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Backup removal test.").unwrap();

    // Create a pre-existing CLAUDE.md (user-owned) so apply creates a .bak.
    fs::write(tmp.path().join("CLAUDE.md"), "User content").unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--backup",
        ])
        .assert()
        .success();

    // .bak should have been created by apply --backup.
    assert!(tmp.path().join("CLAUDE.md.bak").exists());

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    // Generated file removed.
    assert!(!tmp.path().join("CLAUDE.md").exists());
    // Backup file also removed.
    assert!(!tmp.path().join("CLAUDE.md.bak").exists());
}

#[test]
fn clear_removes_both_imrule_and_ruler_with_remove_source() {
    let tmp = tempdir().unwrap();
    // Create BOTH .imrule and .ruler — only .imrule is used by apply.
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::create_dir_all(tmp.path().join(".ruler")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Both dirs test.").unwrap();
    fs::write(tmp.path().join(".ruler/AGENTS.md"), "Legacy content.").unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "apply",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
        ])
        .assert()
        .success();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "claude",
            "--remove-source",
        ])
        .assert()
        .success();

    // Both source directories must be removed.
    assert!(!tmp.path().join(".imrule").exists());
    assert!(!tmp.path().join(".ruler").exists());
}

#[test]
fn clear_preserves_native_mcp_file_carrying_user_schema_key() {
    // A native MCP config may carry a top-level `$schema` hint that imrule never
    // writes — so it is always user/tool-authored. After `clear` strips the last
    // imrule-managed server, the file is left as `{ "$schema": "...",
    // "mcpServers": {} }`. That `$schema` is meaningful user data: the file must
    // be PRESERVED (with the managed server removed), not deleted.
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Schema kept on clear.",
    )
    .unwrap();
    fs::write(
        tmp.path().join(".imrule/imrule.toml"),
        "[mcp_servers.tool]\ntransport = \"http\"\nurl = \"https://example.com/mcp\"\n",
    )
    .unwrap();

    // Native config carrying a `$schema` key alongside the imrule-managed server.
    fs::create_dir_all(tmp.path().join(".cursor")).unwrap();
    fs::write(
        tmp.path().join(".cursor/mcp.json"),
        r#"{"$schema":"https://example.com/schema.json","mcpServers":{"tool":{"url":"https://example.com/mcp"}}}"#,
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
        ])
        .assert()
        .success();

    // The file survives because `$schema` is meaningful user data; the
    // imrule-managed server is gone.
    let path = tmp.path().join(".cursor/mcp.json");
    assert!(path.exists(), "file with user $schema must not be deleted");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        config["$schema"],
        serde_json::json!("https://example.com/schema.json")
    );
    assert!(config["mcpServers"].get("tool").is_none());
}

#[test]
fn clear_preserves_native_mcp_file_when_schema_accompanies_unmanaged_server() {
    // Inverse of the schema-only case: when a `$schema` key sits beside a server
    // imrule does NOT manage, the file still holds meaningful data and survives.
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Schema kept.").unwrap();
    fs::write(
        tmp.path().join(".imrule/imrule.toml"),
        "[mcp_servers.tool]\ntransport = \"http\"\nurl = \"https://example.com/mcp\"\n",
    )
    .unwrap();

    fs::create_dir_all(tmp.path().join(".cursor")).unwrap();
    fs::write(
        tmp.path().join(".cursor/mcp.json"),
        r#"{"$schema":"https://example.com/schema.json","mcpServers":{"tool":{"url":"https://example.com/mcp"},"keep-me":{"command":"node","args":["x.js"]}}}"#,
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "cursor",
        ])
        .assert()
        .success();

    // The unmanaged server keeps the file alive; the managed one is gone.
    let content = fs::read_to_string(tmp.path().join(".cursor/mcp.json")).unwrap();
    let config: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(config["mcpServers"]["keep-me"].is_object());
    assert!(config["mcpServers"].get("tool").is_none());
}

#[test]
fn clear_cleans_windsurf_orphan_after_mcp_capability_dropped() {
    // Regression: Windsurf's MCP capability was disabled, but a previous apply
    // may have left imrule-managed servers in .windsurf/mcp_config.json. Clear
    // must still strip imrule's keys instead of orphaning them forever.
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Windsurf orphan.").unwrap();
    fs::write(
        tmp.path().join(".imrule/imrule.toml"),
        "[mcp_servers.tool]\ntransport = \"http\"\nurl = \"https://example.com/mcp\"\n",
    )
    .unwrap();

    // Orphan native config left by an older version that still managed Windsurf MCP.
    fs::create_dir_all(tmp.path().join(".windsurf")).unwrap();
    fs::write(
        tmp.path().join(".windsurf/mcp_config.json"),
        r#"{"mcpServers":{"tool":{"url":"https://example.com/mcp"},"user-kept":{"command":"node"}}}"#,
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "windsurf",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(tmp.path().join(".windsurf/mcp_config.json")).unwrap();
    let config: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        config["mcpServers"].get("tool").is_none(),
        "imrule key must be cleaned"
    );
    assert!(
        config["mcpServers"]["user-kept"].is_object(),
        "user server preserved"
    );
}

#[test]
fn clear_cleans_legacy_kilocode_mcp_file() {
    // Regression: Kilo's native path moved to kilo.jsonc, but older versions
    // wrote .kilocode/mcp.json. Clear must still find and clean that legacy file.
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(tmp.path().join(".imrule/AGENTS.md"), "Kilo legacy.").unwrap();
    fs::write(
        tmp.path().join(".imrule/imrule.toml"),
        "[mcp_servers.tool]\ntransport = \"http\"\nurl = \"https://example.com/mcp\"\n",
    )
    .unwrap();

    fs::create_dir_all(tmp.path().join(".kilocode")).unwrap();
    fs::write(
        tmp.path().join(".kilocode/mcp.json"),
        r#"{"mcp":{"tool":{"type":"remote","url":"https://example.com/mcp","enabled":true}}}"#,
    )
    .unwrap();

    Command::cargo_bin("imrule")
        .unwrap()
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
            "--agents",
            "kilocode",
        ])
        .assert()
        .success();

    // Legacy file held only the imrule-managed server, so it is now empty/removed.
    let legacy = tmp.path().join(".kilocode/mcp.json");
    if legacy.exists() {
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&legacy).unwrap()).unwrap();
        assert!(
            config["mcp"].get("tool").is_none(),
            "legacy imrule key must be cleaned"
        );
    }
}
