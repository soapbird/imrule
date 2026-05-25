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
        .args([
            "clear",
            "--project-root",
            tmp.path().to_str().unwrap(),
        ])
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
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Nothing to clear.",
    )
    .unwrap();

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
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Empty MCP test.",
    )
    .unwrap();
    // mcp.json exists but has no mcpServers — collect_mcp_keys returns empty vec.
    fs::write(
        tmp.path().join(".imrule/mcp.json"),
        r#"{"other":"data"}"#,
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
fn clear_removes_backup_files_alongside_generated() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".imrule")).unwrap();
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Backup removal test.",
    )
    .unwrap();

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
    fs::write(
        tmp.path().join(".imrule/AGENTS.md"),
        "Both dirs test.",
    )
    .unwrap();
    fs::write(
        tmp.path().join(".ruler/AGENTS.md"),
        "Legacy content.",
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
