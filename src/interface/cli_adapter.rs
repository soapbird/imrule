//! CLI adapter that wires concrete infrastructure to application use cases.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use crate::application::init_use_case::{InitOptions, InitUseCase};
use crate::application::revert_use_case::{RevertOptions, RevertUseCase};
use crate::application::skills_add_use_case::{SkillsAddOptions, SkillsAddUseCase};
use crate::infrastructure::agent_writer::DefaultAgentWriter;
use crate::infrastructure::config_loader::TomlConfigLoader;
use crate::infrastructure::file_system::FsFileSystem;
use crate::infrastructure::gitignore::GitignoreUpdater;
use crate::infrastructure::mcp_storage::JsonMcpStorage;
use crate::infrastructure::skill_fetcher::GitSkillFetcher;
use crate::infrastructure::skills::discover_skills;
use crate::interface::cli::{parse_agents, Cli, Command, SkillsCommand};

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
                    skills: args.skills.unwrap_or(true),
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            if args.dry_run {
                println!("ImRule apply dry run completed successfully.");
            } else {
                println!("ImRule apply completed successfully.");
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
            println!("ImRule initialized at {}", root.display());
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
            println!("ImRule revert completed successfully.");
            if args.verbose {
                println!("Files changed: {}", changed.len());
            }
            Ok(())
        }
        Command::Skills(skills_args) => match skills_args.command {
            SkillsCommand::Add(args) => {
                let project_root = args
                    .project_root
                    .clone()
                    .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                let fetcher =
                    GitSkillFetcher::new().map_err(|e| CliError::new(1, e.to_string()))?;
                let use_case = SkillsAddUseCase::new(&fetcher, &fs);
                let result = use_case
                    .execute(SkillsAddOptions {
                        project_root,
                        source: args.source,
                        skill_names: args.skill,
                        list_only: args.list,
                        global: args.global,
                        verbose: args.verbose,
                    })
                    .map_err(|err| CliError::new(1, err.to_string()))?;

                if !result.listed.is_empty() {
                    println!("Available skills:");
                    for skill in &result.listed {
                        println!("  - {}", skill.name);
                    }
                }
                if !result.installed.is_empty() {
                    println!("Installed {} skill(s):", result.installed.len());
                    for name in &result.installed {
                        println!("  - {name}");
                    }

                    // Run apply to sync skills to agent directories.
                    let agent_writer = DefaultAgentWriter::new(&fs);
                    let apply_use_case =
                        ApplyUseCase::new(&config, &fs, &gitignore, &mcp, &agent_writer);
                    let project_root_for_apply = args.project_root.clone().unwrap_or_else(|| {
                        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                    apply_use_case
                        .execute(ApplyOptions {
                            project_root: project_root_for_apply,
                            agents: None,
                            config: None,
                            mcp: false,
                            mcp_overwrite: false,
                            gitignore: None,
                            gitignore_local: false,
                            dry_run: false,
                            local_only: false,
                            backup: true,
                            skills: true,
                        })
                        .map_err(|err| CliError::new(1, err.to_string()))?;
                    println!("Skills synced to agent directories.");
                }
                Ok(())
            }
            SkillsCommand::List(args) => {
                let project_root = args
                    .project_root
                    .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                let skills_dir = if args.global {
                    crate::domain::constants::xdg_config_home()
                        .join("imrule")
                        .join("skills")
                } else {
                    project_root.join(".imrule").join("skills")
                };
                let discovery = discover_skills(&if args.global {
                    crate::domain::constants::xdg_config_home().join("imrule")
                } else {
                    project_root.clone()
                })
                .map_err(|e| CliError::new(1, e.to_string()))?;
                if discovery.skills.is_empty() {
                    println!("No skills installed in {}.", skills_dir.display());
                } else {
                    println!("Installed skills ({}):", skills_dir.display());
                    for skill in &discovery.skills {
                        println!("  - {} ({})", skill.name, skill.path.display());
                    }
                }
                Ok(())
            }
        },
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
