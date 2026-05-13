//! CLI adapter that wires concrete infrastructure to application use cases.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use crate::application::clear_use_case::{ClearOptions, ClearUseCase};
use crate::application::init_use_case::{InitOptions, InitUseCase};
use crate::application::revert_use_case::{RevertOptions, RevertUseCase};
use crate::application::skills_add_use_case::{SkillsAddOptions, SkillsAddUseCase};
use crate::infrastructure::agent_writer::DefaultAgentWriter;
use crate::infrastructure::config_loader::TomlConfigLoader;
use crate::infrastructure::file_system::FsFileSystem;
use crate::infrastructure::gitignore::GitignoreUpdater;
use crate::infrastructure::mcp_storage::JsonMcpStorage;
use crate::infrastructure::skill_fetcher::GitSkillFetcher;
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
            let agent_writer = DefaultAgentWriter::new(&fs);
            let use_case = ApplyUseCase::new(&config, &fs, &gitignore, &mcp, &agent_writer);
            let written = use_case
                .execute(ApplyOptions {
                    project_root,
                    agents,
                    config: args.config,
                    dry_run: args.dry_run,
                    backup: args.backup,
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
                    dry_run: args.dry_run,
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            println!("ImRule revert completed successfully.");
            if args.verbose {
                println!("Files changed: {}", changed.len());
            }
            Ok(())
        }
        Command::Clear(args) => {
            let project_root = args
                .project_root
                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let use_case = ClearUseCase::new(&config, &fs, &gitignore, &mcp);
            let removed = use_case
                .execute(ClearOptions {
                    project_root,
                    agents: parse_agents(args.agents),
                    config: args.config,
                    dry_run: args.dry_run,
                    remove_source: args.remove_source,
                })
                .map_err(|err| CliError::new(1, err.to_string()))?;
            if args.dry_run {
                println!("ImRule clear dry run completed successfully.");
            } else {
                println!("ImRule clear completed successfully.");
            }
            if args.verbose {
                println!("Files removed: {}", removed.len());
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

                    println!("Syncing skills to agent directories (running apply)...");
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
                            dry_run: false,
                            backup: false,
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
                let imrule_skills = project_root.join(".imrule").join("skills");
                let legacy_skills = project_root.join(".ruler").join("skills");
                let skills_dir = if args.global {
                    crate::domain::constants::xdg_config_home()
                        .join("imrule")
                        .join("skills")
                } else if imrule_skills.exists() {
                    imrule_skills
                } else if legacy_skills.exists() {
                    legacy_skills
                } else {
                    imrule_skills
                };
                let discovery = if skills_dir.exists() {
                    crate::infrastructure::skills::walk_skills_tree(&skills_dir)
                        .map_err(|e| CliError::new(1, e.to_string()))?
                } else {
                    crate::domain::skills::SkillsDiscovery::default()
                };
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
