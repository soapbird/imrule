//! Contract tests for git index untracking of generated files.
//!
//! `.gitignore` only affects untracked files. Generated files that were
//! committed before ImRule ignored them (e.g. `.cursor/mcp.json`) must be
//! removed from the git index on apply while staying on disk.

use std::fs;
use std::path::Path;
use std::process::Command;

use imrule::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use imrule::application::ports::GitTrackingPort;
use imrule::infrastructure::agent_writer::DefaultAgentWriter;
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::git_tracking::GitUntracker;
use imrule::infrastructure::gitignore::GitignoreUpdater;
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use tempfile::tempdir;

/// Runs `git` in `root` with the global excludes file disabled so a
/// developer's `~/.gitignore_global` (e.g. an entry ignoring `mcp.json`)
/// can't make `git add` reject paths these tests need to stage. Every git
/// invocation in this file goes through here, keeping the tests hermetic.
fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "core.excludesFile="])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} failed", args);
}

fn git_tracked_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["-c", "core.excludesFile=", "ls-files"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn untrack_removes_staged_file_from_index_but_keeps_it_on_disk() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);

    fs::create_dir_all(root.join(".cursor")).unwrap();
    fs::write(root.join(".cursor/mcp.json"), "{}").unwrap();
    git(root, &["add", ".cursor/mcp.json"]);
    assert!(git_tracked_files(root).contains(&".cursor/mcp.json".to_string()));

    let untracked = GitUntracker::new()
        .untrack_generated_files(root, &[root.join(".cursor/mcp.json")])
        .unwrap();

    assert_eq!(untracked, vec![root.join(".cursor/mcp.json")]);
    assert!(!git_tracked_files(root).contains(&".cursor/mcp.json".to_string()));
    assert!(root.join(".cursor/mcp.json").exists());
}

#[test]
fn untrack_expands_directories_to_their_tracked_files() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);

    fs::create_dir_all(root.join(".cursor/skills/demo")).unwrap();
    fs::write(root.join(".cursor/skills/demo/SKILL.md"), "# demo").unwrap();
    git(root, &["add", ".cursor/skills"]);

    let untracked = GitUntracker::new()
        .untrack_generated_files(root, &[root.join(".cursor/skills")])
        .unwrap();

    assert_eq!(untracked, vec![root.join(".cursor/skills/demo/SKILL.md")]);
    assert!(git_tracked_files(root).is_empty());
    assert!(root.join(".cursor/skills/demo/SKILL.md").exists());
}

#[test]
fn untrack_returns_empty_outside_a_git_work_tree() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("CLAUDE.md"), "rules").unwrap();

    let untracked = GitUntracker::new()
        .untrack_generated_files(root, &[root.join("CLAUDE.md")])
        .unwrap();

    assert!(untracked.is_empty());
    assert!(root.join("CLAUDE.md").exists());
}

#[test]
fn untrack_returns_empty_when_nothing_is_tracked() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);

    fs::write(root.join("CLAUDE.md"), "rules").unwrap();

    let untracked = GitUntracker::new()
        .untrack_generated_files(root, &[root.join("CLAUDE.md")])
        .unwrap();

    assert!(untracked.is_empty());
}

#[test]
fn apply_untracks_generated_files_that_git_already_tracks() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);

    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    );
    let options = ApplyOptions {
        project_root: root.to_path_buf(),
        agents: Some(vec!["cursor".to_string()]),
        config: None,
        dry_run: false,
        backup: false,
    };

    // First apply generates the file AND adds it to the local .gitignore
    // managed block, so `-f` is needed to stage it (simulating a user who
    // committed the file before ImRule ignored it).
    let first = apply.execute(ApplyOptions { ..options.clone() }).unwrap();
    assert!(first.untracked.is_empty());
    git(root, &["add", "-f", "AGENTS.md"]);

    // Second apply must drop the tracked generated file from the index.
    let second = apply.execute(options).unwrap();
    assert_eq!(second.untracked, vec![root.join("AGENTS.md")]);
    assert!(!git_tracked_files(root).contains(&"AGENTS.md".to_string()));
    assert!(root.join("AGENTS.md").exists());
}

#[test]
fn apply_untracks_its_skill_copies_but_not_skills_the_user_committed_beside_them() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);
    fs::create_dir_all(root.join(".imrule/skills/cli")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();
    fs::write(root.join(".imrule/skills/cli/SKILL.md"), "cli").unwrap();
    fs::create_dir_all(root.join(".claude/skills/mine")).unwrap();
    fs::write(root.join(".claude/skills/mine/SKILL.md"), "mine").unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    );
    let options = ApplyOptions {
        project_root: root.to_path_buf(),
        agents: Some(vec!["claude".to_string()]),
        config: None,
        dry_run: false,
        backup: false,
    };

    apply.execute(options.clone()).unwrap();
    git(
        root,
        &[
            "add",
            "-f",
            ".claude/skills/mine/SKILL.md",
            ".claude/skills/cli/SKILL.md",
        ],
    );

    apply.execute(options).unwrap();

    let tracked = git_tracked_files(root);
    assert!(
        tracked.contains(&".claude/skills/mine/SKILL.md".to_string()),
        "a skill the user committed was dropped from the index: {tracked:?}"
    );
    assert!(!tracked.contains(&".claude/skills/cli/SKILL.md".to_string()));
    assert!(root.join(".claude/skills/cli/SKILL.md").exists());
}
