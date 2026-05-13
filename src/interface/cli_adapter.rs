//! CLI adapter that wires concrete infrastructure to application use cases.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use crate::application::init_use_case::{InitOptions, InitUseCase};
use crate::application::revert_use_case::{RevertOptions, RevertUseCase};
use crate::infrastructure::agent_writer::DefaultAgentWriter;
use crate::infrastructure::config_loader::TomlConfigLoader;
use crate::infrastructure::file_system::FsFileSystem;
use crate::infrastructure::gitignore::GitignoreUpdater;
use crate::infrastructure::mcp_storage::JsonMcpStorage;
use crate::interface::cli::{parse_agents, Cli, Command};

/// Entry point for the CLI.
pub fn run() -> ExitCode {
    match run_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError { code, message }) => {
            eprintln!("[imrule] {message}");
            ExitCode::from(code)
        }
    }
}

fn run_inner() -> Result<(), CliError> {
    let cli = Cli::parse();

    let fs = FsFileSystem::new();
    let config = TomlConfigLoader::new();
    let gitignore = GitignoreUpdater::new();
    let mcp = JsonMcpStorage::new();

    match cli.command {
        Command::Apply(args) => {
            let project_root = args
                .project_root
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let agents = parse_agents(args.agents);
            let mcp_enabled = args.mcp && !args.no_mcp;
            let agent_writer = DefaultAgentWriter::new(&fs);
            let use_case = ApplyUseCase::new(&config, &fs, &gitignore, &mcp, &agent_writer);
            let written = use_case
                .execute(ApplyOptions {
                    project_root,
                    agents,
                    config: args.config,
                    mcp: mcp_enabled,
                    mcp_overwrite: args.mcp_overwrite,
                    gitignore: args.gitignore,
                    gitignore_local: args.gitignore_local,
                    dry_run: args.dry_run,
                    local_only: args.local_only,
                    backup: args.backup,
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            if args.dry_run {
                println!("Imrule apply dry run completed successfully.");
            } else {
                println!("Imrule apply completed successfully.");
            }
            if args.verbose {
                println!("Files considered: {}", written.len());
            }
            Ok(())
        }
        Command::Init(args) => {
            let use_case = InitUseCase::new(&fs);
            let root = use_case
                .execute(InitOptions {
                    project_root: args.project_root,
                    global: args.global,
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            println!("Imrule initialized at {}", root.display());
            Ok(())
        }
        Command::Revert(args) => {
            let project_root = args
                .project_root
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let use_case = RevertUseCase::new(&config, &fs, &gitignore);
            let changed = use_case
                .execute(RevertOptions {
                    project_root,
                    agents: parse_agents(args.agents),
                    config: args.config,
                    keep_backups: args.keep_backups,
                    dry_run: args.dry_run,
                    local_only: args.local_only,
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            println!("Imrule revert completed successfully.");
            if args.verbose {
                println!("Files changed: {}", changed.len());
            }
            Ok(())
        }
    }
}

#[derive(Debug)]
struct CliError {
    code: u8,
    message: String,
}

impl CliError {
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
