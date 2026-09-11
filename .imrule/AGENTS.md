# ImRule — Agent Guide

## Project Overview

ImRule is a Rust CLI tool that lets you write project instructions, MCP server configurations, skills, and subagent definitions once in a `.imrule/` directory, then propagates them to 30+ supported AI coding agents with a single command.

The binary provides these subcommands:
- `imrule init` — scaffolds a `.imrule/` directory with default files.
- `imrule apply` — reads `.imrule/` contents and writes them to each agent's native config files.
- `imrule clear` — removes the assets `apply` generated, leaving skills and subagents the user placed in agent directories alone.
- `imrule mcp` — manages MCP server definitions in `imrule.toml` (`add`/`remove`) and authenticates remote servers (`auth`).
- `imrule skills` — manages agent skills: `add`/`update` from remote or local sources, `list`, and `setup` for ImRule's built-in skills (searchable multi-select picker preselected by project detection).
- `imrule completions` / `imrule man` — generate shell completions and the man page.

This is a native Rust project (edition 2024, MSRV 1.85). It was rewritten from a prior TypeScript/npm runtime; no JS/TS artifacts remain in the source tree.

## Technology Stack

- **Language**: Rust 1.85+
- **Build system**: Cargo + Make
- **CLI parser**: `clap` (derive macros)
- **Serialization**: `serde`, `serde_json`, `serde_norway` (YAML frontmatter), `toml`
- **Config editing**: `toml_edit` (preserves comments/formatting when mutating `imrule.toml`)
- **Error handling**: `thiserror` (typed domain errors)
- **Terminal UI**: `crossterm` (raw-mode picker for `skills setup`)
- **Dev tools**: `assert_cmd` (binary-level testing), `tempfile`

## Build and Test Commands

```bash
make                 # list targets (help is the default goal)
make setup           # rustup component add rustfmt clippy + cargo fetch

# CI gate (read-only): fmt-check + lint + test
make check

# Individual targets
make fmt             # cargo fmt --all (writes)
make fmt-check       # cargo fmt --all --check
make lint            # cargo clippy --all-targets --all-features -- -D warnings
make test            # cargo test (integration/contract tests)
make build           # cargo build --release
make run ARGS="..."  # cargo run -- ...
make clean           # cargo clean
make test-e2e        # scripts/test-e2e.sh against the release binary and test-e2e/ fixtures
make test-e2e-skills # scripts/test-e2e-skills.sh (includes a remote GitHub source)
make coverage        # cargo llvm-cov → lcov.info
make changelog       # git cliff --unreleased --prepend CHANGELOG.md
make deny            # cargo deny check (licenses, advisories, sources per deny.toml)
make install         # copy binary to $HOME/.local/bin/imrule
make install-system  # copy binary to /usr/local/bin/imrule (needs sudo)
make uninstall       # remove installed binary
```

Tooling configuration: `rust-toolchain.toml` pins 1.85 (the MSRV, also used by CI and release builds), `[lints]` in `Cargo.toml` forbids `unsafe` and warns on `dbg!`/`todo!`, `src/lib.rs` warns on `unwrap`/`expect` in production code (a crate-root attribute, so tests and benches may still unwrap), and `deny.toml` holds the cargo-deny license/advisory/source policy (`make deny`). rustfmt uses defaults.

## Code Organization

The codebase follows **Hexagonal / Clean Architecture** with strict layer boundaries enforced by convention and by `tests/architecture_contract.rs`.

```
src/
  main.rs              # Binary entry point (7 lines); calls imrule::run_cli()
  lib.rs               # Library root; re-exports domain, application, infrastructure
  domain/              # Pure business logic — zero I/O, zero external dependencies
  application/         # Use cases + port traits (abstract I/O boundaries)
  infrastructure/      # Concrete I/O adapters (filesystem, config loader, gitignore, MCP storage)
  interface/           # CLI adapter (clap derive + wiring)
skills/                # Built-in skills embedded into the binary by build.rs (see skills/README.md)
```

### Domain (`src/domain/`)
- `builtin_skills.rs` — Built-in skill catalog built from embedded files, `ProjectSignals` → `recommend_builtin_skills` detection rules, and `BuiltinSkillState` (not installed / up to date / outdated revision / modified locally).
- `agent.rs` — Compile-time `const` array of 36 `AgentDefinition`s (identifier, name, output paths, MCP keys, capabilities). This is the single source of truth for the agent registry. `AGENT_ALIASES` maps alternate names users type (`droid` → `factory`) onto registry identifiers; resolve names through `find_agent`/`canonical_agent_identifier` rather than comparing `identifier` directly.
- `config.rs` — Config structs: `LoadedConfig`, `AgentConfig`, `McpConfig`, `McpServerDefinition`, `McpTransport`, `GitignoreConfig`, `SkillsConfig`, `SubagentsConfig`, `SubagentFrontmatter`.
- `constants.rs` — Shared constants (the generated-file marker, subagent and GJC config paths, `.imrule/cache.json`, the legacy `.ruler` directory name) and `relative_key`, the project-relative form paths are recorded under in the manifest.
- `error.rs` — Unified `ImruleError` enum (`thiserror`) with variants: `UnknownAgent`, `Config`, `Mcp`, `Subagent`, `Rules`, `Skills`, `Filesystem`, `Gitignore`, `GitTracking`.
- `gjc_config.rs` — Gajae Code `.gjc/config.yml` helpers: turn on its opt-in native skill discovery keys on `apply` and strip them again on `clear` (`strip_gjc_skill_discovery`), preserving the user's own keys.
- `manifest.rs` — `ApplyManifest`/`McpTarget`: the record of what a run generated, plus the pure diffs (`stale_paths`, `stale_mcp_targets`, `stale_mcp_servers`, `stale_skills`, `merged_with`) that let a later run clean up what it no longer produces.
- `mcp.rs` — MCP capability filtering, remote→stdio transformation, merge/overwrite logic, conversion of `McpServerDefinition` values to agent-native JSON, and native-config emptiness checks (`is_json_effectively_empty`, `is_native_mcp_content_empty`).
- `rules.rs` — Markdown concatenation with `<!-- Source: relative/path -->` markers.
- `skills.rs` — Skills discovery types, gitignore path helpers, `flatten_skill_name` (a grouped skill `python/cli` is published as `python-cli`) with collision detection, and the pure grouping/status types behind `skills update`.
- `subagent.rs` — YAML frontmatter parsing/validation and agent-native file builders (Claude, Cursor, Codex, Copilot).

### Application (`src/application/`)
- `ports.rs` — Trait boundaries: `ConfigPort`, `ConfigWritePort`, `FileSystemPort`, `GitignorePort`, `GitTrackingPort`, `CachePort`, `ManifestPort`, `McpPort`, `AgentWriterPort`, `SkillFetcherPort`. Besides reads and writes, `FileSystemPort` answers the questions every delete or rewrite must pass: `resolves_within` (the path's parent, links resolved, lies inside the project; a link at the path itself counts as inside), `target_resolves_within` (the path itself, links followed, lies inside; for files read and written back), `is_same_entry` (two paths name one file or directory, as on a case-insensitive filesystem), and `list_files` (a tree's non-directory entries, never following a symlinked directory).
- `apply_use_case.rs` — Orchestrates reading `.imrule/`, concatenating rules, writing agent files, merging MCP configs (from `.imrule/mcp.json` and `[mcp_servers]` in `imrule.toml`), updating gitignore, untracking generated files that git still tracks despite being ignored, and reconciling against the previous run's manifest so outputs it no longer generates are removed rather than orphaned.
- `clear_use_case.rs` — Removes what `apply` generated across every supported agent, including outputs the current config no longer describes (found via the manifest): rule files carrying the generated marker and their `.bak` backups, skill copies and subagent files the manifest recorded or that match today's `.imrule/` output byte for byte, ImRule's keys in native MCP configs and `.gjc/config.yml`, and the gitignore block. A directory goes only once it is empty, nothing that resolves outside the project is removed or rewritten, and a full clear (no `--agents`) also drops the manifest and `.imrule/cache.json`.
- `init_use_case.rs` — Scaffolds `.imrule/` or `~/.config/imrule/` non-destructively.
- `mcp_use_case.rs` — Adds and removes MCP server definitions in `imrule.toml`.
- `skills_add_use_case.rs` — Installs skills from remote or local sources, records each one's origin in `[skills.sources]`, and syncs them to agent directories.
- `skills_setup_use_case.rs` — Plans built-in skills against the project (recommended + install state) and installs the selected ones, refreshing outdated revisions and skipping locally modified skills unless asked to overwrite.
- `skills_update_use_case.rs` — Re-fetches the sources recorded in `[skills.sources]` and refreshes each installed skill, reporting per-skill status (updated / unchanged / reinstalled / missing in source / failed).

### Infrastructure (`src/infrastructure/`)
- `builtin_skills.rs` — `builtin_catalog()` over the file table `build.rs` generates from `skills/`, and `collect_project_signals` (Cargo/pyproject manifests at depth ≤ 2, Makefile, Docker, workflows, VERSION/CHANGELOG).
- `agent_writer.rs` — `DefaultAgentWriter`: prepends `<!-- Generated by ImRule -->`, compares existing content to avoid redundant writes, and creates `.bak` backups only when content actually changes.
- `config_loader.rs` — `TomlConfigLoader`: resolves `imrule.toml` (local → global fallback), parses all TOML sections including `[mcp_servers]`, supports legacy `subagents` table alias, and implements `ConfigWritePort` using `toml_edit`.
- `file_system.rs` — `FsFileSystem`: real filesystem ops, recursive markdown discovery (skips `skills/`, optionally skips `agents/`), walks up parent directories to find `.imrule/`, finds nested `.imrule` dirs deepest-first, and implements the link-aware containment checks (`resolves_within`, `target_resolves_within`, `is_same_entry`).
- `gitignore.rs` — `GitignoreUpdater`: managed block updates between `# START ImRule Generated Files` and `# END ImRule Generated Files`.
- `git_tracking.rs` — `GitUntracker`: removes generated files from the git index (`git rm --cached`, kept on disk) when git still tracks them despite the gitignore entry; no-op outside a git work tree.
- `manifest.rs` — `JsonApplyManifest`: atomic read/write of `.imrule/manifest.json`; an unparseable or future-version manifest degrades to `None` rather than failing apply.
- `mcp_storage.rs` — `JsonMcpStorage`: reads/writes native MCP JSON, resolves agent-specific config paths, graceful degradation for missing files.
- `mcp_storage_toml.rs` / `mcp_storage_openhands_toml.rs` — Native MCP read/write for the TOML-configured agents (Codex, Mistral, OpenHands; OpenHands' `[mcp]` stdio/`shttp_servers`/`sse_servers` layout lives in its own module), edited with `toml_edit`.
- `skill_fetcher.rs` — `SkillFetcherPort` implementation that `git clone`s a remote skill source into a temporary directory.
- `skills.rs` — Skill discovery, validation warnings, recursive copy for propagation, and byte-for-byte tree comparison so an update never rewrites an unchanged skill.
- `subagents.rs` — Subagent discovery from `.imrule/agents/` (the gitignore paths now come from `domain::subagent::subagents_gitignore_paths`).
- `version_cache.rs` — Project-scoped JSON version cache (`.imrule/cache.json`) implementing `CachePort`; pins the `mcp-remote` version that `apply` and `mcp auth` use.
- `vscode_settings.rs` — Augment (VS Code) MCP transform into `.vscode/settings.json` array format.

### Interface (`src/interface/`)
- `cli.rs` — Clap derive definitions for `Cli`/`Command` and each subcommand's arguments: `ApplyArgs`, `ClearArgs`, `InitArgs`, `McpArgs` (`McpAddArgs`, `McpRemoveArgs`, `McpAuthArgs`), `SkillsArgs` (`SkillsAddArgs`, `SkillsUpdateArgs`, `SkillsListArgs`, `SkillsSetupArgs`), `CompletionsArgs`.
- `cli_adapter.rs` — Wires concrete infrastructure to use cases, maps errors to `CliError` with exit codes, prints verbose output.
- `skill_picker.rs` — Searchable multi-select picker: a terminal-free `Picker` state machine and renderer (tested directly) plus the thin crossterm loop `run_picker`, drawn on stderr.

## Testing Strategy

There are **zero unit tests inside `src/`**. All testing happens via integration/contract tests in `tests/`.

| Test File | What It Validates |
|---|---|
| `agent_layer_contract.rs` | Full agent registry (36 agents): identifiers, names, output paths, MCP keys, capabilities. `DefaultAgentWriter` idempotency and custom output path overrides. |
| `apply_manifest_contract.rs` | `ApplyManifest` diffs and `JsonApplyManifest` round-trip; apply/clear reconciliation end-to-end: dropping MCP servers or agents removes what they generated, a user-owned file or server is never touched, `--dry-run` and `--agents` prune nothing. Grouped skills publish under hyphen-joined path names and removed ones are pruned while hand-placed skills (and a skills root still holding them) survive; a colliding name fails before anything is written; a tampered manifest cannot reach outside the project; pre-0.5 leaf-named copies are removed only when identical to the source. |
| `architecture_contract.rs` | Layer boundaries: `main.rs` only calls `imrule::run_cli()`, no direct domain/infra imports; layers depend only inward (`crate::<layer>` references); the application layer reaches the filesystem only through `FileSystemPort` (no `std::fs`, `.exists()`, `.is_dir()`, `.is_file()`); the domain stays free of `std::fs`, filesystem probes and `env::current_dir`. Confirms `Cargo.toml` metadata and no tracked TypeScript artifacts. |
| `clear_coverage_contract.rs` | `clear` through the binary: every agent without `--agents`, a no-op on a clean project, `--remove-source` with `.imrule/` and legacy `.ruler/`, `--dry-run` keeping subagent and skill directories and MCP configs, `[mcp_servers]` servers and `.bak` backups removed, native MCP files holding a user `$schema` key or unmanaged servers preserved, Windsurf orphans and legacy Kilo Code MCP files cleaned. |
| `cli_release_contract.rs` | Binary-level tests via `assert_cmd::Command`: `--version`, `--help`, real `apply`/`clear` round-trip, `--dry-run`, `init` idempotency, `init --global` with `XDG_CONFIG_HOME`. |
| `config_fs_rules_contract.rs` | `concatenate_rules` format, `read_markdown_files` ordering/skipping, `GitignoreUpdater` managed block behavior, `TomlConfigLoader` parsing (including `[mcp_servers]`), parent-directory creation on write. |
| `git_tracking_contract.rs` | `GitUntracker` index removal (file kept on disk, directories expanded, no-op outside a work tree or when nothing is tracked); apply-level untracking of previously committed generated files, including apply's own skill copies but never the skills a user committed beside them. |
| `mcp_command_contract.rs` | `imrule mcp add/remove` persistence to `imrule.toml`, env/header parsing, TOML precedence over `.imrule/mcp.json`, apply propagation to agent-native MCP files. |
| `mcp_native_apply_contract.rs` | `apply` writing each agent's native MCP shape (Codex/Mistral/OpenHands TOML, Kimi, Roo, Kilo Code, Crush, Zed, OpenCode, Gemini/Qwen), skipping agents without project MCP support (Aider, Windsurf, Firebender), expanding `$VAR` from `.env` files, aborting rather than clobbering an invalid native config, bridging remote servers by default while honoring a per-server `native` override, and persisting the `mcp-remote` version cache (never in `--dry-run`). |
| `mcp_paths_settings_contract.rs` | MCP capability filtering (stdio vs remote), remote→stdio transformation, `merge_mcp` key translation, native MCP path resolution, Augment VS Code settings transform. |
| `mcp_version_cache_contract.rs` | `.imrule/cache.json` closed schema and atomic replacement; `mcp-remote` version resolution (cache reuse, persistence, dry-run, resolver failure); `imrule mcp auth` eligibility (`[mcp]` disabled, `native` transport globally or per server, stdio-only and non-HTTP remotes) and sequential runs that stop at the first failure. |
| `scaffold_contract.rs` | Domain defaults: `McpStrategy::default()`, `McpConfig::default()`, `SubagentFrontmatter` shape, `AgentDefinition` exposure. |
| `skills_setup_contract.rs` | Built-in catalog grouping, name resolution and revision parsing; detection rules and project signal collection (workspace members included); install/update/modified/dry-run semantics; ownership rules: an outdated skill holding a file the user added, a skill the user wrote at a built-in path, a directory without `SKILL.md`, or a path reached through a symlink is never replaced without `--force` or per-item consent, and install refuses what appeared after planning or a path the plan does not hold; `list_files` not following symlinked directories; picker filtering, ranking, toggling, paging, rendering and consent recording (only toggling an item on by itself counts); every shipped skill complete (name = path, revision, references, `check.py` helper block identical to `skills/README.md`) and the embedded catalog free of hidden files and bytecode; `skills setup` through the binary: `--list`/names/`--yes`/`--all --dry-run`/`--global`, flag conflicts and the missing-terminal error, and project resolution from a subdirectory or below a home directory with its own `.imrule/`. |
| `skills_subagents_apply_clear_contract.rs` | Skills discovery/warnings/copy/gitignore paths, grouped-skill naming and name collisions, re-copies that leave unchanged files untouched but carry permission changes; remote/local skill source parsing and `skills add` install/list/filter; skill source registry recording and `skills update` statuses (an unreachable source does not abort the run; a project skills directory linked outside the project is refused); `.ruler/` fallbacks for skills and subagents; subagent frontmatter parse/validate/load; Claude/Cursor/Codex/Copilot file builders; GJC skill-discovery config merge/strip; apply path collection; filesystem-level operations used by `clear`. |

Additionally, `make test-e2e` runs shell-based end-to-end tests using the release binary against `test-e2e/` fixtures.

When modifying behavior, add or update the relevant contract test in `tests/`. The architecture contract must continue to pass — do not let `main.rs` import domain or infrastructure modules directly.

## Code Style Guidelines

- **Architecture**: Keep the hexagonal boundary strict. Domain has no I/O. Application defines ports. Infrastructure implements ports. Interface wires everything.
- **Error handling**: Use the typed `ImruleError` enum everywhere. Avoid introducing `anyhow` in production code (it is in `Cargo.toml` but not used in the current source).
- **Agent registry changes**: If adding or modifying an agent, update `AGENT_DEFINITIONS` in `domain/agent.rs` and ensure `agent_layer_contract.rs` is updated.
- **Path handling**: Use `std::path::PathBuf`. Use `normalize_path_separators` for forward-slash normalization in gitignore and source markers.
- **Idempotency**: When writing files, compare against existing content and skip the write (and skip creating `.bak`) if unchanged. This is codified in `DefaultAgentWriter`.
- **Non-destructive defaults**: `init` must never overwrite existing files. `apply` must only back up when content changes.
- **Generated marker**: All rule files must start with `<!-- Generated by ImRule -->\n`.

## Security Considerations

- The tool reads arbitrary `.md` files from `.imrule/` and writes them to agent config paths. It does not execute file contents.
- MCP config merging reads and writes JSON files but does not execute commands described inside them.
- Backup files (`.bak`) are created in the same directory as the original file only when `apply` overwrites user-owned content. Use `clear` to remove generated files: it deletes a rule file only when it carries the ImRule generated marker, and a skill copy or subagent file only when the manifest recorded it or it matches today's `.imrule/` output byte for byte. Skills and subagents placed in agent directories by hand stay, and a directory is removed only once it is empty.
- Every path `apply`, `clear`, `skills update` and `skills setup` delete, and every native MCP config they rewrite, is checked on disk (through `FileSystemPort::resolves_within` / `target_resolves_within`) to lie inside the project. A `.imrule/manifest.json` shipped with a cloned repository (absolute paths, `..` steps) or a committed symlink cannot redirect them outside it.
- No secrets or credentials are handled by the Rust code itself; MCP server configs are passed through as opaque JSON.

## Deployment / Release

- The project is released as a native binary, not a library crate.
- `Cargo.toml` includes a `package.metadata.imrule` section with `release-channel = "native-rust"`.
- `make install` installs to `$HOME/.local/bin` by default; `make install-system` installs to `/usr/local/bin`.

## Useful Patterns for Agents Working on This Codebase

- **Adding a new agent**: Add a new `AgentDefinition` to the `AGENT_DEFINITIONS` const array in `domain/agent.rs`. Update `agent_layer_contract.rs` expectations. If the agent supports MCP, skills, or subagents, set the appropriate `AgentCapabilities` flags.
- **Adding a new CLI flag**: Add it to the clap derive struct in `interface/cli.rs`, thread it through `cli_adapter.rs` into the use-case options struct, and read it in the use case. Add a contract test in `cli_release_contract.rs` or `config_fs_rules_contract.rs`.
- **Changing file write behavior**: Modify `infrastructure/agent_writer.rs` or `infrastructure/file_system.rs`, then update `config_fs_rules_contract.rs` or `skills_subagents_apply_clear_contract.rs`.
- **Changing config parsing**: Modify `infrastructure/config_loader.rs` and update `config_fs_rules_contract.rs`.
- **Changing MCP logic**: Modify `domain/mcp.rs` or `infrastructure/mcp_storage.rs` and update `mcp_paths_settings_contract.rs` or `mcp_command_contract.rs`.
- **Adding a new generated output**: Make sure `apply` returns its path in `written_paths`. That list feeds the gitignore block, git untracking, *and* the manifest — a path missing from it is orphaned on disk the moment the config that produced it changes. Update `apply_manifest_contract.rs`.
- **Adding or changing a built-in skill**: Edit files under `skills/<group>/<kind>/` following `skills/README.md` (frontmatter `name` = path joined with `-`, bump `metadata.imrule-skill-version` on every content change, keep the `check.py` helper block identical). `build.rs` embeds the tree; update the `SHIPPED` list in `skills_setup_contract.rs` and, for a new skill, its detection rule in `recommend_builtin_skills`.
- **Changing `imrule mcp` behavior**: Modify `application/mcp_use_case.rs` and `interface/cli.rs`, then update `mcp_command_contract.rs` and `cli_release_contract.rs`.