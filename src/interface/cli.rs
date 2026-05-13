//! CLI argument definitions using clap.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

const AGENT_IDENTIFIERS: &str = "agentsmd, aider, amazonqcli, amp, antigravity, augmentcode, claude, cline, codex, copilot, crush, cursor, factory, firebase, firebender, gemini-cli, goose, jetbrains-ai, jules, junie, kilocode, kiro, mistral, opencode, openhands, pi, qwen, roo, trae, warp, windsurf, zed";

#[derive(Debug, Parser)]
#[command(name = "ruler")]
#[command(version)]
#[command(about = "Ruler — apply the same rules to all coding agents")]
#[command(override_usage = "ruler <command> [options]")]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply ruler configurations to supported AI agents.
    Apply(ApplyArgs),
    /// Scaffold a .ruler directory with default files.
    Init(InitArgs),
    /// Revert ruler configurations from supported AI agents.
    Revert(RevertArgs),
}

#[derive(Debug, Args)]
#[command(after_help = AGENT_IDENTIFIERS)]
pub struct ApplyArgs {
    /// Project root directory.
    #[arg(long = "project-root", value_name = "DIR")]
    pub project_root: Option<PathBuf>,
    /// Comma-separated list of agent identifiers.
    #[arg(long, value_name = "IDS")]
    pub agents: Option<String>,
    /// Path to TOML configuration file.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Enable applying MCP server config.
    #[arg(long, alias = "with-mcp", default_value_t = true, action = ArgAction::SetTrue)]
    pub mcp: bool,
    /// Disable applying MCP server config.
    #[arg(long = "no-mcp", default_value_t = false, action = ArgAction::SetTrue)]
    pub no_mcp: bool,
    /// Replace (not merge) the native MCP config(s).
    #[arg(long = "mcp-overwrite", default_value_t = false)]
    pub mcp_overwrite: bool,
    /// Enable/disable automatic .gitignore updates.
    #[arg(long, action = ArgAction::Set)]
    pub gitignore: Option<bool>,
    /// Write generated ignore entries to .git/info/exclude instead of .gitignore.
    #[arg(long = "gitignore-local", action = ArgAction::SetTrue)]
    pub gitignore_local: bool,
    /// Enable verbose logging.
    #[arg(long, short = 'v', default_value_t = false)]
    pub verbose: bool,
    /// Preview changes without writing files.
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,
    /// Only search for local .ruler directories, ignore global config.
    #[arg(long = "local-only", default_value_t = false)]
    pub local_only: bool,
    /// Enable/disable creation of .bak backup files.
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub backup: bool,
    /// Enable/disable skills support.
    #[arg(long, action = ArgAction::Set)]
    pub skills: Option<bool>,
    /// Enable/disable subagents support.
    #[arg(long, action = ArgAction::Set)]
    pub subagents: Option<bool>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Project root directory.
    #[arg(long = "project-root", value_name = "DIR")]
    pub project_root: Option<PathBuf>,
    /// Initialize in global config directory (XDG_CONFIG_HOME/ruler).
    #[arg(long = "global", default_value_t = false)]
    pub global: bool,
}

#[derive(Debug, Args)]
#[command(after_help = AGENT_IDENTIFIERS)]
pub struct RevertArgs {
    /// Project root directory.
    #[arg(long = "project-root", value_name = "DIR")]
    pub project_root: Option<PathBuf>,
    /// Comma-separated list of agent identifiers.
    #[arg(long, value_name = "IDS")]
    pub agents: Option<String>,
    /// Path to TOML configuration file.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Keep backup files after revert.
    #[arg(long = "keep-backups", default_value_t = false)]
    pub keep_backups: bool,
    /// Enable verbose logging.
    #[arg(long, short = 'v', default_value_t = false)]
    pub verbose: bool,
    /// Preview changes without writing files.
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,
    /// Only search for local .ruler directories, ignore global config.
    #[arg(long = "local-only", default_value_t = false)]
    pub local_only: bool,
}

pub fn parse_agents(agents: Option<String>) -> Option<Vec<String>> {
    agents.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
}
