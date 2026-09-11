//! CLI adapter that wires concrete infrastructure to application use cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode, Stdio};

use clap::{CommandFactory, Parser};

use crate::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use crate::application::clear_use_case::{ClearOptions, ClearUseCase};
use crate::application::init_use_case::{InitOptions, InitUseCase};
use crate::application::mcp_use_case::{
    McpAddOptions, McpAuthOptions, McpAuthRunnerPort, McpRemoteVersionResolverPort,
    McpRemoveOptions, McpUseCase, parse_env_pairs,
};

use crate::application::ports::FileSystemPort;
use crate::application::skills_add_use_case::{
    SkillsAddOptions, SkillsAddUseCase, list_installed_skills,
};
use crate::application::skills_update_use_case::{SkillsUpdateOptions, SkillsUpdateUseCase};
use crate::infrastructure::agent_writer::DefaultAgentWriter;
use crate::infrastructure::config_loader::TomlConfigLoader;
use crate::infrastructure::file_system::FsFileSystem;
use crate::infrastructure::git_tracking::GitUntracker;
use crate::infrastructure::gitignore::GitignoreUpdater;
use crate::infrastructure::manifest::JsonApplyManifest;
use crate::infrastructure::mcp_storage::JsonMcpStorage;
use crate::infrastructure::skill_fetcher::GitSkillFetcher;
use crate::infrastructure::version_cache::JsonVersionCache;
use crate::interface::cli::{Cli, Command, McpCommand, SkillsCommand, parse_agents};

struct ProcessMcpRemoteRunner;

/// Parses the JSON output of `npm view <pkg> version --json` into a version string.
/// Expects a quoted JSON string like `"1.2.3"`.
pub fn parse_npm_version_output(output: &str) -> Result<String, crate::domain::error::ImruleError> {
    serde_json::from_str::<String>(output.trim()).map_err(|_| {
        crate::domain::error::ImruleError::mcp(
            "npm returned an invalid mcp-remote version response",
        )
    })
}

impl McpRemoteVersionResolverPort for ProcessMcpRemoteRunner {
    fn resolve_latest_version(&self) -> Result<String, crate::domain::error::ImruleError> {
        let output = ProcessCommand::new("npm")
            .args(["view", "mcp-remote@latest", "version", "--json"])
            .output()
            .map_err(|error| {
                crate::domain::error::ImruleError::mcp(format!(
                    "could not start npm version resolution: {error}"
                ))
            })?;
        if !output.status.success() {
            return Err(crate::domain::error::ImruleError::mcp(
                "npm could not resolve a concrete mcp-remote version",
            ));
        }
        let stdout = std::str::from_utf8(&output.stdout).map_err(|_| {
            crate::domain::error::ImruleError::mcp("npm returned a non-UTF-8 mcp-remote version")
        })?;
        parse_npm_version_output(stdout)
    }
}

impl McpAuthRunnerPort for ProcessMcpRemoteRunner {
    fn authenticate(
        &self,
        package_spec: &str,
        url: &str,
    ) -> Result<(), crate::domain::error::ImruleError> {
        let mut child = ProcessCommand::new("npx")
            .args(["-y", "-p", package_spec, "mcp-remote-client", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                crate::domain::error::ImruleError::mcp(format!(
                    "could not start mcp-remote authentication: {error}"
                ))
            })?;

        let status = child.wait().map_err(|error| {
            crate::domain::error::ImruleError::mcp(format!(
                "could not wait for mcp-remote authentication: {error}"
            ))
        })?;
        if !status.success() {
            return Err(crate::domain::error::ImruleError::mcp(
                "mcp-remote authentication process failed",
            ));
        }
        Ok(())
    }
}

/// Entry point for the CLI.
pub fn run() -> ExitCode {
    match run_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError { code, message }) => {
            eprintln!("[imrule] error: {message}");
            ExitCode::from(code)
        }
    }
}

fn init_tracing(verbose: bool) {
    let max_level = if verbose {
        tracing::Level::INFO
    } else {
        tracing::Level::WARN
    };
    tracing_subscriber::fmt()
        .with_max_level(max_level)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

/// Explains why a follow-up `apply` would be pointless, or `None` when the
/// skills belong to the project `apply` targets.
///
/// `apply` only ever syncs the skills under `<project>/.imrule/skills`, so a
/// global install — `--global`, or a directory that has no `.imrule/` of its
/// own — has nothing for it to pick up. Running it anyway would write generated
/// agent files into a directory the user never initialized, and surface
/// unrelated failures from the global config while doing it.
fn skills_sync_skip_reason(install_dir: &Path, project_root: &Path) -> Option<String> {
    if canonical_or_self(install_dir).starts_with(canonical_or_self(project_root)) {
        return None;
    }
    Some(format!(
        "Skipping agent sync: `imrule apply` syncs skills from a project's .imrule/skills/, \
         and these live in {}.\nRun `imrule init` in the project that should use them, \
         then re-run this command there.",
        install_dir.display()
    ))
}

fn canonical_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The project root a command works in: the `--project-root` argument, or the
/// current directory when omitted.
fn resolve_project_root(project_root: &Option<PathBuf>) -> PathBuf {
    project_root
        .clone()
        .unwrap_or_else(|| FsFileSystem.current_dir())
}

/// The infrastructure wiring every skills command needs to run `apply`.
struct AgentSync<'a> {
    fs: &'a FsFileSystem,
    config: &'a TomlConfigLoader,
    gitignore: &'a GitignoreUpdater,
    git_untracker: &'a GitUntracker,
    mcp: &'a JsonMcpStorage,
    manifest: &'a JsonApplyManifest,
}

impl AgentSync<'_> {
    /// Runs `apply` so skills just written to `install_dir` reach every agent's
    /// native skills directory — unless they live outside the project `apply`
    /// syncs, which is explained instead. Every skills command that writes
    /// skills ends here.
    fn sync_skills(&self, install_dir: &Path, project_root: PathBuf) -> Result<(), CliError> {
        if let Some(reason) = skills_sync_skip_reason(install_dir, &project_root) {
            println!("{reason}");
            return Ok(());
        }
        println!("Syncing skills to agent directories (running apply)...");
        let agent_writer = DefaultAgentWriter::new(self.fs);
        ApplyUseCase::new(
            self.config,
            self.fs,
            self.gitignore,
            self.git_untracker,
            self.mcp,
            &agent_writer,
        )
        .with_manifest(self.manifest)
        .execute(ApplyOptions {
            project_root,
            agents: None,
            config: None,
            dry_run: false,
            backup: false,
        })?;
        println!("Skills synced to agent directories.");
        Ok(())
    }
}

fn run_inner() -> Result<(), CliError> {
    let cli = Cli::parse();

    let fs = FsFileSystem::new();
    let config = TomlConfigLoader::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp = JsonMcpStorage::new();
    let version_cache = JsonVersionCache::new();
    let manifest = JsonApplyManifest::new();
    let mcp_remote_runner = ProcessMcpRemoteRunner;

    match cli.command {
        Command::Apply(args) => {
            init_tracing(args.verbose);
            let project_root = resolve_project_root(&args.project_root);
            let agents = parse_agents(args.agents);
            let agent_writer = DefaultAgentWriter::new(&fs);
            let use_case = ApplyUseCase::new(
                &config,
                &fs,
                &gitignore,
                &git_untracker,
                &mcp,
                &agent_writer,
            )
            .with_manifest(&manifest)
            .with_mcp_remote_version_cache(&version_cache, &mcp_remote_runner);
            let result = use_case.execute(ApplyOptions {
                project_root,
                agents,
                config: args.config,
                dry_run: args.dry_run,
                backup: args.backup,
            })?;
            if args.dry_run {
                println!("ImRule apply dry run completed successfully.");
            } else {
                println!("ImRule apply completed successfully.");
            }
            if !result.untracked.is_empty() {
                println!(
                    "Removed {} generated file(s) from the git index (kept on disk):",
                    result.untracked.len()
                );
                for path in &result.untracked {
                    println!("  - {}", path.display());
                }
            }
            if args.verbose {
                println!("Files considered: {}", result.written.len());
            }
            Ok(())
        }
        Command::Init(args) => {
            init_tracing(false);
            let use_case = InitUseCase::new(&fs);
            let root = use_case.execute(InitOptions {
                project_root: args.project_root,
                global: args.global,
            })?;
            println!("ImRule initialized at {}", root.display());
            Ok(())
        }
        Command::Mcp(mcp_args) => match mcp_args.command {
            McpCommand::Add(args) => {
                init_tracing(false);
                let project_root = resolve_project_root(&args.project_root);
                let env_map = parse_env_pairs(&args.env.unwrap_or_default())?;
                let header_map = parse_env_pairs(&args.header.unwrap_or_default())?;

                let (command, args_list, url) = match args.transport.into() {
                    crate::domain::config::McpTransport::Stdio => {
                        if args.remote_transport.is_some() {
                            return Err(CliError::new(
                                1,
                                "--remote-transport applies only to http/sse servers".to_string(),
                            ));
                        }
                        if args.rest.is_empty() {
                            return Err(CliError::new(
                                1,
                                "stdio transport requires a command (e.g. `imrule mcp add name -- npx -y @server/package`)".to_string(),
                            ));
                        }
                        let mut rest = args.rest.clone();
                        let cmd = rest.remove(0);
                        (Some(cmd), rest, None)
                    }
                    crate::domain::config::McpTransport::Http
                    | crate::domain::config::McpTransport::Sse => {
                        if args.rest.len() != 1 {
                            return Err(CliError::new(
                                1,
                                "remote transport requires exactly one URL argument".to_string(),
                            ));
                        }
                        (None, Vec::new(), Some(args.rest[0].clone()))
                    }
                };

                let use_case = McpUseCase::new(&config, &config);
                use_case.add(McpAddOptions {
                    project_root,
                    config_path: None,
                    global: args.global,
                    dry_run: args.dry_run,
                    name: args.name,
                    transport: args.transport.into(),
                    command,
                    args: args_list,
                    url,
                    env: env_map,
                    headers: header_map,
                    timeout: args.timeout,
                    remote_transport: args.remote_transport.map(Into::into),
                })?;

                if args.dry_run {
                    println!("ImRule mcp add dry run completed successfully.");
                } else {
                    println!("ImRule mcp add completed successfully.");
                }
                Ok(())
            }
            McpCommand::Remove(args) => {
                init_tracing(false);
                let project_root = resolve_project_root(&args.project_root);
                let use_case = McpUseCase::new(&config, &config);
                use_case.remove(McpRemoveOptions {
                    project_root,
                    config_path: None,
                    global: args.global,
                    dry_run: args.dry_run,
                    name: args.name,
                })?;

                if args.dry_run {
                    println!("ImRule mcp remove dry run completed successfully.");
                } else {
                    println!("ImRule mcp remove completed successfully.");
                }
                Ok(())
            }
            McpCommand::Auth(args) => {
                init_tracing(false);
                let project_root = resolve_project_root(&args.project_root);
                let use_case = McpUseCase::new(&config, &config);
                let result = use_case.auth(
                    McpAuthOptions {
                        project_root,
                        config_path: args.config,
                    },
                    &mcp,
                    &fs,
                    &version_cache,
                    &mcp_remote_runner,
                    &mcp_remote_runner,
                )?;

                for server_name in &result.authenticated {
                    println!("Authenticated MCP server '{server_name}'.");
                }
                for skipped in &result.skipped {
                    println!(
                        "Skipped MCP server '{}': {}.",
                        skipped.server_name, skipped.reason
                    );
                }
                println!(
                    "ImRule mcp auth completed: {} authenticated, {} skipped.",
                    result.authenticated.len(),
                    result.skipped.len()
                );
                Ok(())
            }
        },
        Command::Completions(args) => {
            init_tracing(false);
            let mut cmd = Cli::command();
            clap_complete::generate(args.shell, &mut cmd, "imrule", &mut std::io::stdout());
            Ok(())
        }
        Command::Man => {
            init_tracing(false);
            let cmd = Cli::command();
            let man = clap_mangen::Man::new(cmd);
            man.render(&mut std::io::stdout())
                .map_err(|err| CliError::new(1, err.to_string()))?;
            Ok(())
        }
        Command::Clear(args) => {
            init_tracing(args.verbose);
            let project_root = resolve_project_root(&args.project_root);
            let use_case =
                ClearUseCase::new(&config, &fs, &gitignore, &mcp).with_manifest(&manifest);
            let removed = use_case.execute(ClearOptions {
                project_root,
                agents: parse_agents(args.agents),
                config: args.config,
                dry_run: args.dry_run,
                remove_source: args.remove_source,
            })?;
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
        Command::Skills(skills_args) => {
            let sync = AgentSync {
                fs: &fs,
                config: &config,
                gitignore: &gitignore,
                git_untracker: &git_untracker,
                mcp: &mcp,
                manifest: &manifest,
            };
            match skills_args.command {
                SkillsCommand::Add(args) => {
                    init_tracing(args.verbose);
                    let project_root = resolve_project_root(&args.project_root);
                    let fetcher = GitSkillFetcher::new()?;
                    let use_case = SkillsAddUseCase::new(&fetcher, &fs, &config, &config);
                    let result = use_case.execute(SkillsAddOptions {
                        project_root: project_root.clone(),
                        source: args.source,
                        skill_names: args.skill,
                        list_only: args.list,
                        global: args.global,
                    })?;

                    if !result.listed.is_empty() {
                        println!("Available skills:");
                        for skill in &result.listed {
                            println!("  - {}", skill.name);
                        }
                    }
                    if !result.installed.is_empty() {
                        println!(
                            "Installed {} skill(s) in {}:",
                            result.installed.len(),
                            result.install_dir.display()
                        );
                        for name in &result.installed {
                            println!("  - {name}");
                        }
                        sync.sync_skills(&result.install_dir, project_root)?;
                    }
                    Ok(())
                }
                SkillsCommand::Update(args) => {
                    init_tracing(args.verbose);
                    let project_root = resolve_project_root(&args.project_root);
                    let fetcher = GitSkillFetcher::new()?;
                    let use_case = SkillsUpdateUseCase::new(&fetcher, &fs, &config);
                    let skill_names = if args.skills.is_empty() {
                        None
                    } else {
                        Some(args.skills.clone())
                    };
                    let result = use_case.execute(SkillsUpdateOptions {
                        project_root: project_root.clone(),
                        skill_names,
                        global: args.global,
                        dry_run: args.dry_run,
                    })?;

                    if result.outcomes.is_empty() {
                        println!(
                            "No registered skill sources. Run `imrule skills add <source>` first."
                        );
                        return Ok(());
                    }

                    for outcome in &result.outcomes {
                        println!(
                            "  - {} [{}] ({})",
                            outcome.name,
                            outcome.status.label(args.dry_run),
                            outcome.source
                        );
                        if let Some(detail) = &outcome.detail {
                            println!("      {detail}");
                        }
                    }

                    if result.changed() && !args.dry_run {
                        sync.sync_skills(&result.install_dir, project_root)?;
                    }

                    if result.has_failures() {
                        return Err(CliError::new(1, "some skill sources could not be fetched"));
                    }
                    Ok(())
                }
                SkillsCommand::List(args) => {
                    init_tracing(false);
                    let project_root = resolve_project_root(&args.project_root);
                    let (skills_dir, discovery) =
                        list_installed_skills(&fs, &project_root, args.global)?;
                    if args.json {
                        let skills: Vec<serde_json::Value> = discovery
                        .skills
                        .iter()
                        .map(|skill| serde_json::json!({ "name": skill.name, "path": skill.path }))
                        .collect();
                        return print_json(&serde_json::json!({
                            "dir": skills_dir,
                            "skills": skills,
                            "warnings": discovery.warnings,
                        }));
                    }
                    if discovery.skills.is_empty() {
                        println!("No skills installed in {}.", skills_dir.display());
                    } else {
                        println!("Installed skills ({}):", skills_dir.display());
                        for skill in &discovery.skills {
                            println!("  - {} ({})", skill.name, skill.path.display());
                        }
                    }
                    if !discovery.warnings.is_empty() {
                        eprintln!(
                            "Warnings:\n{}",
                            crate::domain::skills::format_validation_warnings(&discovery.warnings)
                        );
                    }
                    Ok(())
                }
            }
        }
    }
}

/// Prints one JSON document on stdout, the whole output of a `--json` run.
fn print_json(document: &serde_json::Value) -> Result<(), CliError> {
    let text =
        serde_json::to_string_pretty(document).map_err(|err| CliError::new(1, err.to_string()))?;
    println!("{text}");
    Ok(())
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

/// A use case failure exits with code 1 and its message.
impl From<crate::domain::error::ImruleError> for CliError {
    fn from(err: crate::domain::error::ImruleError) -> Self {
        Self::new(1, err.to_string())
    }
}
