use std::collections::BTreeMap;
use std::fs;

use imrule::application::ports::AgentWriterPort;
use imrule::domain::agent::{all_agents, get_agent_identifiers_for_cli_help, AgentOutputPaths};
use imrule::domain::config::AgentConfig;
use imrule::infrastructure::agent_writer::DefaultAgentWriter;
use imrule::infrastructure::file_system::FsFileSystem;
use tempfile::tempdir;

#[test]
fn agent_registry_matches_native_identifiers_order_and_cli_help() {
    let agents = all_agents();
    let ids: Vec<_> = agents.iter().map(|agent| agent.identifier).collect();

    assert_eq!(
        ids,
        vec![
            "copilot",
            "claude",
            "codex",
            "cursor",
            "windsurf",
            "cline",
            "aider",
            "firebase",
            "openhands",
            "gemini-cli",
            "jules",
            "junie",
            "augmentcode",
            "kilocode",
            "opencode",
            "goose",
            "crush",
            "amp",
            "zed",
            "qwen",
            "agentsmd",
            "kiro",
            "warp",
            "roo",
            "trae",
            "amazonqcli",
            "firebender",
            "factory",
            "antigravity",
            "mistral",
            "pi",
            "jetbrains-ai",
            "gjc",
        ]
    );

    assert_eq!(
        get_agent_identifiers_for_cli_help(),
        "agentsmd, aider, amazonqcli, amp, antigravity, augmentcode, claude, cline, codex, copilot, crush, cursor, factory, firebase, firebender, gemini-cli, gjc, goose, jetbrains-ai, jules, junie, kilocode, kiro, mistral, opencode, openhands, pi, qwen, roo, trae, warp, windsurf, zed"
    );
}

#[test]
fn agent_registry_matches_native_names_paths_mcp_keys_and_capabilities() {
    let root = std::path::Path::new("/project");
    let agents = all_agents();
    let by_id: BTreeMap<_, _> = agents
        .iter()
        .map(|agent| (agent.identifier, agent))
        .collect();

    let expectations = [
        (
            "copilot",
            "GitHub Copilot",
            AgentOutputPaths::single("/project/.github/copilot-instructions.md"),
            "servers",
            true,
            true,
            false,
            true,
            true,
        ),
        (
            "claude",
            "Claude Code",
            AgentOutputPaths::single("/project/CLAUDE.md"),
            "mcpServers",
            true,
            true,
            false,
            true,
            true,
        ),
        (
            "codex",
            "OpenAI Codex CLI",
            AgentOutputPaths::many([
                ("instructions", "/project/AGENTS.md"),
                ("config", "/project/.codex/config.toml"),
            ]),
            "mcp_servers",
            true,
            true,
            false,
            true,
            true,
        ),
        (
            "cursor",
            "Cursor",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcpServers",
            true,
            true,
            false,
            true,
            true,
        ),
        (
            "windsurf",
            "Windsurf",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcpServers",
            false,
            false,
            false,
            true,
            false,
        ),
        (
            "cline",
            "Cline",
            AgentOutputPaths::single("/project/.clinerules"),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "aider",
            "Aider",
            AgentOutputPaths::many([
                ("instructions", "/project/AGENTS.md"),
                ("config", "/project/.aider.conf.yml"),
            ]),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "firebase",
            "Firebase Studio",
            AgentOutputPaths::single("/project/.idx/airules.md"),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "openhands",
            "Open Hands",
            AgentOutputPaths::single("/project/.openhands/microagents/repo.md"),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "gemini-cli",
            "Gemini CLI",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcpServers",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "jules",
            "Jules",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "junie",
            "Junie",
            AgentOutputPaths::single("/project/.junie/guidelines.md"),
            "mcpServers",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "augmentcode",
            "AugmentCode",
            AgentOutputPaths::single("/project/.augment/rules/imrule_augment_instructions.md"),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "kilocode",
            "Kilo Code",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcp",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "opencode",
            "OpenCode",
            AgentOutputPaths::many([
                ("instructions", "/project/AGENTS.md"),
                ("mcp", "/project/opencode.json"),
            ]),
            "mcp",
            true,
            true,
            true,
            true,
            false,
        ),
        (
            "goose",
            "Goose",
            AgentOutputPaths::single("/project/.goosehints"),
            "",
            false,
            false,
            false,
            true,
            false,
        ),
        (
            "crush",
            "Crush",
            AgentOutputPaths::many([
                ("instructions", "/project/CRUSH.md"),
                ("mcp", "/project/.crush.json"),
            ]),
            "mcp",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "amp",
            "Amp",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "",
            false,
            false,
            false,
            true,
            false,
        ),
        (
            "zed",
            "Zed",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "context_servers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "qwen",
            "Qwen Code",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "agentsmd",
            "AgentsMd",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "kiro",
            "Kiro",
            AgentOutputPaths::single("/project/.kiro/steering/imrule_kiro_instructions.md"),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "warp",
            "Warp",
            AgentOutputPaths::single("/project/WARP.md"),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "roo",
            "RooCode",
            AgentOutputPaths::many([
                ("instructions", "/project/AGENTS.md"),
                ("mcp", "/project/.roo/mcp.json"),
            ]),
            "mcpServers",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "trae",
            "Trae AI",
            AgentOutputPaths::single("/project/.trae/rules/project_rules.md"),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "amazonqcli",
            "Amazon Q CLI",
            AgentOutputPaths::many([
                ("instructions", "/project/.amazonq/rules/imrule_q_rules.md"),
                ("mcp", "/project/.amazonq/mcp.json"),
            ]),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "firebender",
            "Firebender",
            AgentOutputPaths::many([
                ("instructions", "/project/firebender.json"),
                ("mcp", "/project/firebender.json"),
            ]),
            "mcpServers",
            true,
            true,
            false,
            false,
            false,
        ),
        (
            "factory",
            "Factory Droid",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "mcpServers",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "antigravity",
            "Antigravity",
            AgentOutputPaths::single("/project/.agent/rules/imrule.md"),
            "mcpServers",
            false,
            false,
            false,
            true,
            false,
        ),
        (
            "mistral",
            "Mistral",
            AgentOutputPaths::many([
                ("instructions", "/project/AGENTS.md"),
                ("config", "/project/.vibe/config.toml"),
            ]),
            "mcp_servers",
            true,
            true,
            false,
            true,
            false,
        ),
        (
            "pi",
            "Pi Coding Agent",
            AgentOutputPaths::single("/project/AGENTS.md"),
            "",
            false,
            false,
            false,
            true,
            false,
        ),
        (
            "jetbrains-ai",
            "JetBrains AI Assistant",
            AgentOutputPaths::single("/project/.aiassistant/rules/AGENTS.md"),
            "mcpServers",
            false,
            false,
            false,
            false,
            false,
        ),
        (
            "gjc",
            "Gajae Code",
            AgentOutputPaths::many([
                ("instructions", "/project/.gjc/RULES.md"),
                ("mcp", "/project/.gjc/mcp.json"),
            ]),
            "mcpServers",
            true,
            true,
            false,
            true,
            false,
        ),
    ];

    for (id, name, paths, mcp_key, stdio, remote, timeout, skills, subagents) in expectations {
        let agent = by_id[id];
        assert_eq!(agent.name, name, "name for {id}");
        assert_eq!(agent.default_output_paths(root), paths, "paths for {id}");
        assert_eq!(agent.mcp_server_key, mcp_key, "mcp key for {id}");
        assert_eq!(agent.capabilities.mcp_stdio, stdio, "stdio for {id}");
        assert_eq!(agent.capabilities.mcp_remote, remote, "remote for {id}");
        assert_eq!(agent.capabilities.mcp_timeout, timeout, "timeout for {id}");
        assert_eq!(agent.capabilities.native_skills, skills, "skills for {id}");
        assert_eq!(
            agent.capabilities.native_subagents, subagents,
            "subagents for {id}"
        );
    }
}

#[test]
fn agents_md_apply_adds_marker_and_is_idempotent_with_backup() {
    let tmp = tempdir().unwrap();
    let fs = FsFileSystem::new();
    let writer = DefaultAgentWriter::new(&fs);
    let target = tmp.path().join("AGENTS.md");

    writer
        .write_agent_rules(
            &all_agents()
                .into_iter()
                .find(|a| a.identifier == "agentsmd")
                .unwrap(),
            "\n\n<!-- Source: .imrule/AGENTS.md -->\n\nRules\n",
            tmp.path(),
            None,
            true,
            false,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "<!-- Generated by ImRule -->\n\n\n<!-- Source: .imrule/AGENTS.md -->\n\nRules\n"
    );
    assert!(!target.with_extension("md.bak").exists());

    writer
        .write_agent_rules(
            &all_agents()
                .into_iter()
                .find(|a| a.identifier == "agentsmd")
                .unwrap(),
            "\n\n<!-- Source: .imrule/AGENTS.md -->\n\nRules\n",
            tmp.path(),
            None,
            true,
            false,
        )
        .unwrap();
    assert!(!target.with_extension("md.bak").exists());

    writer
        .write_agent_rules(
            &all_agents()
                .into_iter()
                .find(|a| a.identifier == "agentsmd")
                .unwrap(),
            "changed",
            tmp.path(),
            None,
            true,
            false,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "<!-- Generated by ImRule -->\nchanged"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("AGENTS.md.bak")).unwrap(),
        "<!-- Generated by ImRule -->\n\n\n<!-- Source: .imrule/AGENTS.md -->\n\nRules\n"
    );
}

#[test]
fn agents_md_apply_honors_configured_output_path() {
    let tmp = tempdir().unwrap();
    let fs = FsFileSystem::new();
    let writer = DefaultAgentWriter::new(&fs);
    let agent_config = AgentConfig {
        output_path: Some(tmp.path().join("custom/TEAM.md")),
        ..AgentConfig::default()
    };

    writer
        .write_agent_rules(
            &all_agents()
                .into_iter()
                .find(|a| a.identifier == "agentsmd")
                .unwrap(),
            "rules",
            tmp.path(),
            Some(&agent_config),
            false,
            false,
        )
        .unwrap();

    assert_eq!(
        fs::read_to_string(tmp.path().join("custom/TEAM.md")).unwrap(),
        "<!-- Generated by ImRule -->\nrules"
    );
}
