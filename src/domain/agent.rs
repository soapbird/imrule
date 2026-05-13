//! Agent metadata, registry, and shared write behavior for Rust migration.

use std::path::{Path, PathBuf};

use crate::domain::constants::DEFAULT_RULES_FILENAME;

/// Capability flags exposed by an agent adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentCapabilities {
    pub mcp_stdio: bool,
    pub mcp_remote: bool,
    pub mcp_timeout: bool,
    pub native_skills: bool,
    pub native_subagents: bool,
}

impl AgentCapabilities {
    pub const fn new(
        mcp_stdio: bool,
        mcp_remote: bool,
        mcp_timeout: bool,
        native_skills: bool,
        native_subagents: bool,
    ) -> Self {
        Self {
            mcp_stdio,
            mcp_remote,
            mcp_timeout,
            native_skills,
            native_subagents,
        }
    }
}

/// A single or multi-file output path declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOutputPaths {
    Single(PathBuf),
    Multiple(Vec<(String, PathBuf)>),
}

impl AgentOutputPaths {
    pub fn single(path: impl Into<PathBuf>) -> Self {
        Self::Single(path.into())
    }

    pub fn many<const N: usize>(paths: [(&str, &str); N]) -> Self {
        Self::Multiple(
            paths
                .into_iter()
                .map(|(key, value)| (key.to_string(), PathBuf::from(value)))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentOutputTemplate {
    Single(&'static str),
    Multiple(&'static [(&'static str, &'static str)]),
}

impl AgentOutputTemplate {
    fn resolve(self, project_root: &Path) -> AgentOutputPaths {
        match self {
            Self::Single(path) => AgentOutputPaths::Single(project_root.join(path)),
            Self::Multiple(paths) => AgentOutputPaths::Multiple(
                paths
                    .iter()
                    .map(|(key, value)| ((*key).to_string(), project_root.join(value)))
                    .collect(),
            ),
        }
    }
}

/// Static metadata for a supported agent adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDefinition {
    pub identifier: &'static str,
    pub name: &'static str,
    output_template: AgentOutputTemplate,
    pub mcp_server_key: &'static str,
    pub capabilities: AgentCapabilities,
}

impl AgentDefinition {
    pub fn default_output_paths(&self, project_root: &Path) -> AgentOutputPaths {
        self.output_template.resolve(project_root)
    }
}

const fn caps(
    mcp_stdio: bool,
    mcp_remote: bool,
    mcp_timeout: bool,
    native_skills: bool,
    native_subagents: bool,
) -> AgentCapabilities {
    AgentCapabilities::new(
        mcp_stdio,
        mcp_remote,
        mcp_timeout,
        native_skills,
        native_subagents,
    )
}

const CODEX_PATHS: &[(&str, &str)] = &[
    ("instructions", DEFAULT_RULES_FILENAME),
    ("config", ".codex/config.toml"),
];
const AIDER_PATHS: &[(&str, &str)] = &[
    ("instructions", DEFAULT_RULES_FILENAME),
    ("config", ".aider.conf.yml"),
];
const OPENCODE_PATHS: &[(&str, &str)] = &[
    ("instructions", DEFAULT_RULES_FILENAME),
    ("mcp", "opencode.json"),
];
const CRUSH_PATHS: &[(&str, &str)] = &[("instructions", "CRUSH.md"), ("mcp", ".crush.json")];
const ROO_PATHS: &[(&str, &str)] = &[
    ("instructions", DEFAULT_RULES_FILENAME),
    ("mcp", ".roo/mcp.json"),
];
const AMAZON_Q_PATHS: &[(&str, &str)] = &[
    ("instructions", ".amazonq/rules/imrule_q_rules.md"),
    ("mcp", ".amazonq/mcp.json"),
];
const FIREBENDER_PATHS: &[(&str, &str)] = &[
    ("instructions", "firebender.json"),
    ("mcp", "firebender.json"),
];
const MISTRAL_PATHS: &[(&str, &str)] = &[
    ("instructions", DEFAULT_RULES_FILENAME),
    ("config", ".vibe/config.toml"),
];

const AGENT_DEFINITIONS: &[AgentDefinition] = &[
    AgentDefinition {
        identifier: "copilot",
        name: "GitHub Copilot",
        output_template: AgentOutputTemplate::Single(".github/copilot-instructions.md"),
        mcp_server_key: "servers",
        capabilities: caps(true, true, false, true, true),
    },
    AgentDefinition {
        identifier: "claude",
        name: "Claude Code",
        output_template: AgentOutputTemplate::Single("CLAUDE.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, true),
    },
    AgentDefinition {
        identifier: "codex",
        name: "OpenAI Codex CLI",
        output_template: AgentOutputTemplate::Multiple(CODEX_PATHS),
        mcp_server_key: "mcp_servers",
        capabilities: caps(true, true, false, true, true),
    },
    AgentDefinition {
        identifier: "cursor",
        name: "Cursor",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, true),
    },
    AgentDefinition {
        identifier: "windsurf",
        name: "Windsurf",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "cline",
        name: "Cline",
        output_template: AgentOutputTemplate::Single(".clinerules"),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "aider",
        name: "Aider",
        output_template: AgentOutputTemplate::Multiple(AIDER_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "firebase",
        name: "Firebase Studio",
        output_template: AgentOutputTemplate::Single(".idx/airules.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, false, false, false, false),
    },
    AgentDefinition {
        identifier: "openhands",
        name: "Open Hands",
        output_template: AgentOutputTemplate::Single(".openhands/microagents/repo.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "gemini-cli",
        name: "Gemini CLI",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "jules",
        name: "Jules",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "junie",
        name: "Junie",
        output_template: AgentOutputTemplate::Single(".junie/guidelines.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "augmentcode",
        name: "AugmentCode",
        output_template: AgentOutputTemplate::Single(
            ".augment/rules/imrule_augment_instructions.md",
        ),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "kilocode",
        name: "Kilo Code",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "opencode",
        name: "OpenCode",
        output_template: AgentOutputTemplate::Multiple(OPENCODE_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, true, true, false),
    },
    AgentDefinition {
        identifier: "goose",
        name: "Goose",
        output_template: AgentOutputTemplate::Single(".goosehints"),
        mcp_server_key: "",
        capabilities: caps(false, false, false, true, false),
    },
    AgentDefinition {
        identifier: "crush",
        name: "Crush",
        output_template: AgentOutputTemplate::Multiple(CRUSH_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "amp",
        name: "Amp",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "",
        capabilities: caps(false, false, false, true, false),
    },
    AgentDefinition {
        identifier: "zed",
        name: "Zed",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "context_servers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "qwen",
        name: "Qwen Code",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "agentsmd",
        name: "AgentsMd",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "kiro",
        name: "Kiro",
        output_template: AgentOutputTemplate::Single(".kiro/steering/imrule_kiro_instructions.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "warp",
        name: "Warp",
        output_template: AgentOutputTemplate::Single("WARP.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "roo",
        name: "RooCode",
        output_template: AgentOutputTemplate::Multiple(ROO_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "trae",
        name: "Trae AI",
        output_template: AgentOutputTemplate::Single(".trae/rules/project_rules.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, false, false),
    },
    AgentDefinition {
        identifier: "amazonqcli",
        name: "Amazon Q CLI",
        output_template: AgentOutputTemplate::Multiple(AMAZON_Q_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "firebender",
        name: "Firebender",
        output_template: AgentOutputTemplate::Multiple(FIREBENDER_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, false, false),
    },
    AgentDefinition {
        identifier: "factory",
        name: "Factory Droid",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "antigravity",
        name: "Antigravity",
        output_template: AgentOutputTemplate::Single(".agent/rules/imrule.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, true, false),
    },
    AgentDefinition {
        identifier: "mistral",
        name: "Mistral",
        output_template: AgentOutputTemplate::Multiple(MISTRAL_PATHS),
        mcp_server_key: "mcpServers",
        capabilities: caps(true, true, false, true, false),
    },
    AgentDefinition {
        identifier: "pi",
        name: "Pi Coding Agent",
        output_template: AgentOutputTemplate::Single(DEFAULT_RULES_FILENAME),
        mcp_server_key: "",
        capabilities: caps(false, false, false, true, false),
    },
    AgentDefinition {
        identifier: "jetbrains-ai",
        name: "JetBrains AI Assistant",
        output_template: AgentOutputTemplate::Single(".aiassistant/rules/AGENTS.md"),
        mcp_server_key: "mcpServers",
        capabilities: caps(false, false, false, false, false),
    },
];

/// Returns all agent definitions in CLI help order.
pub fn all_agents() -> Vec<AgentDefinition> {
    AGENT_DEFINITIONS.to_vec()
}

/// Generates the comma-separated agent identifier list used by CLI help.
pub fn get_agent_identifiers_for_cli_help() -> String {
    let mut identifiers: Vec<_> = AGENT_DEFINITIONS
        .iter()
        .map(|agent| agent.identifier)
        .collect();
    identifiers.sort_unstable();
    if let Some(index) = identifiers.iter().position(|id| *id == "agentsmd") {
        if index > 0 {
            let agentsmd = identifiers.remove(index);
            identifiers.insert(0, agentsmd);
        }
    }
    identifiers.join(", ")
}
