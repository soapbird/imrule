# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **`imrule skills setup` installs ImRule's built-in skills**: keeping many projects on one set of patterns meant explaining the same setup, structure and convention rules to agents again in every repository. ImRule now ships thirteen skills — language-agnostic `cli`, `server`, `make/setup`, `release/versioning`, `ci/github-actions`, `docker/setup`, `docker/optimize` (measured image size, build-cache, supply-chain and runtime-hardening improvements with before/after evidence) and `vscode/setup` (a `.vscode/` tuned to the project: settings, extensions, launch and tasks), plus `python/cli`, `python/server`, `rust/cli` and `rust/server` — each of which sets a project up or audits it with a bundled `scripts/check.py` and reports PASS/WARN/FAIL. The thirteenth, `imrule-issue`, is offered in every project: when an imrule command or skill misbehaves, or a feature is missing, it collects redacted diagnostics, searches for duplicates, and files an issue on `soapbird/imrule` only after the user approves the full draft. With no arguments, `setup` detects the project (Cargo and pyproject dependencies such as clap, axum, typer or fastapi; a Makefile, Dockerfile, workflows or `.vscode/`) and opens a searchable multi-select list with the fitting skills preselected. Skill names, `--yes` (the detected ones) or `--all` work without a terminal, and `--list` prints the catalog. Re-running refreshes skills installed from an older revision and leaves identical ones alone. A skill at a built-in path that `setup` cannot prove it installed — edited locally, written by the user, missing its `imrule-builtin` marker, holding a file the built-in does not ship, or a directory with no `SKILL.md` — is never replaced unless `--force` is given or that skill is toggled on by itself in the list; selecting everything with `Ctrl+A` does not count. Run from a subdirectory, `setup` detects the project that owns the `.imrule/` it installs into (never the home directory), syncs agents only for the directory it ran in, as `apply` does, and the list restores the terminal even if ImRule panics.

- **`--json` for `imrule skills list` and `imrule skills setup --list`**: scripts and agents had to scrape the human-readable lists. Both now print a single JSON document on stdout when given `--json` — installed skills with their paths and discovery warnings, or the built-in catalog with what was detected, each skill's revision, whether it fits the project, and its install state.

### Changed

- **Grouped skills are published under their path**: `.imrule/skills/` could group skills in folders, but `apply` copied each one under its leaf directory name — so `python/cli` and `rust/cli` both landed in `.claude/skills/cli`, and which one survived depended on copy order. A grouped skill is now published under its path joined with hyphens (`python-cli`, `rust-cli`), the one-level layout Claude Code and Gemini CLI discover. Two skills that would share a name fail `apply` instead of overwriting each other, and `imrule skills list` warns when a `SKILL.md` declares a `name` other than the published one, since VS Code Copilot, Cursor and OpenCode skip such skills. Top-level skills keep their names. Two skills that would share a name now fail before `apply` writes anything. A copy an earlier release left under its leaf name is removed on the first `apply` when it still matches the source byte for byte; one edited since stays, so delete it by hand.

- **`apply` leaves unchanged skill files alone**: every run rewrote every skill file into every selected agent's skills root, refreshing mtimes that editors and agents watch. A file whose copy already holds the same bytes is now skipped, while a permission change such as a script made executable still carries over.

### Fixed

- **Skills removed from `.imrule/skills/` lingered in agent directories**: `apply` only ever copied skills in, so a renamed or deleted skill stayed in `.claude/skills/` and every other agent's skills root until `imrule clear`. The manifest now records each skill copy, and the next `apply` removes the copies it no longer makes — a skill placed in an agent directory by hand is never recorded, so it is never touched. Renaming a skill only by case (`Foo` to `foo`) keeps the copy on case-insensitive filesystems.

- **`apply` and `clear` deleted skills and subagents placed in agent directories by hand**: once `.imrule/skills/` was empty or `[skills] enabled = false` was set, the next `apply` removed the whole `.claude/skills/` (and every other agent's skills root), and dropping every subagent removed the whole `.claude/agents/` the same way; `clear` always removed both wholesale. Both now remove only what `apply` made — the skill copies and subagent files it recorded, plus `clear` removing copies and subagent files identical to what today's sources produce — and a directory only once nothing else is left in it. `apply` also stopped dropping those directories wholesale from the git index, which staged skills and agents the user had committed there for deletion, and no longer removes a directory the user put at a path it once wrote a file to.

### Security

- **A committed `.imrule/manifest.json` could make `apply` delete directories outside the project**: `apply` deleted the stale entries a manifest listed without checking them, so an absolute path or a `..` step in a manifest shipped with a cloned repository removed that directory on the first run. What `apply`, `clear`, `skills update` and `skills setup` delete, and the native MCP configs they rewrite, are now checked on disk to lie inside the project, so neither such an entry nor a committed symlink can redirect them, and a recorded skill copy must sit directly inside a known agent skills root.

## [0.4.2.0] - 2026-09-11

### Added

- **Per-server `remote_transport`**: `[mcp] remote_transport` was the only switch, so one server needing a static `Authorization` header — which the `mcp-remote` bridge cannot carry — forced the whole project to `native` and took every other remote server off the bridge and its uniform OAuth flow. A server can now override the project default for itself with `remote_transport = "native"` (or `"mcp-remote"`) under `[mcp_servers.<name>]` or on its `.imrule/mcp.json` entry, and `imrule mcp add --remote-transport` records it. `apply` validates headers, bridges, and resolves the `mcp-remote` version per server; `imrule mcp auth` skips only the servers that resolve to `native`. The key is ImRule's own and never reaches an agent's native config.

## [0.4.1.0] - 2026-09-08

### Fixed

- **`imrule skills add` wrote into projects it never installed into**: with no `.imrule/` in the current directory the skills base walks up to `~/.config/imrule`, so the skills were installed globally — and the follow-up apply then ran against that same uninitialized directory. apply only syncs `<project>/.imrule/skills`, so it had nothing to pick up; what it did instead was write generated rule files into a project the user never initialized, and fail on whatever the global config happened to hold (`unknown agent identifier: 'droid'`), exiting `1` after the install had already succeeded. `skills add` and `skills update` now report where the skills landed and run apply only when that directory belongs to the project apply targets, explaining the skip otherwise.

- **Skills mirrored in their source were installed twice**: a source repo commonly ships each skill in both `skills/` and an agent-native directory such as `.openclaw/skills/`, and discovery walked the tree at any depth — so `imrule skills add https://github.com/dietrichgebert/ponytail` reported and copied 12 skills for a repository of 6. Discovery now keeps one directory per skill name, preferring the canonical copy over a mirror under a dot-directory.

### Added

- **`droid` is accepted wherever `factory` is**: Factory's CLI ships as `droid`, so that is the name users reach for in `agents`, `default_agents` or `--agents`, and it was rejected as an unknown agent identifier. The registry now carries an alias table and every selection resolves through it; `[agent.droid]` configures the same adapter `factory` names. Unknown identifiers are still reported as unknown, and the alias stays out of the canonical identifier listing.

## [0.4.0.0] - 2026-09-07

### Added

- **`imrule skills update`**: installed skills had no memory of where they came from, so the only way to pick up an upstream change was to remember the original source and run `imrule skills add` again. `imrule skills add` now records each installed skill's origin under `[skills.sources]` in `imrule.toml`, and `imrule skills update` re-fetches those sources — a fresh shallow clone for remote ones — and refreshes the installed copies. It accepts skill names to narrow the run, `--dry-run` to report without writing, and `--global` for `~/.config/imrule/skills/`; `up` is an alias. Any real change re-runs `apply`, so agent skill directories stay in sync.

- Each skill is reported with what happened to it: `updated`, `unchanged`, `reinstalled` (recorded but missing on disk), `missing in source` (gone upstream, installed copy left alone) or `failed`. A source that cannot be fetched fails only its own skills — the rest of the run continues — and the command exits `1`.

- **`[skills.sources]` in `imrule.toml`**: the skill source registry, written through `toml_edit` so surrounding comments and formatting survive. Skills installed before this release carry no entry; adding one by hand (`<skill-name> = "<source>"`) is enough to bring them under `update`.

### Changed

- An update replaces a skill directory rather than overlaying it, so files dropped upstream also disappear locally. The fetched tree is compared byte for byte first, so an unchanged skill is never removed and rewritten.

- `save_config` no longer creates an empty `[mcp_servers]` table. Writing the config is no longer exclusive to `imrule mcp`, and a project with no MCP servers should not grow the table because a skill was installed.

## [0.3.0.0] - 2026-09-07

### Fixed

- **`apply` orphaned the files it stopped generating**: every output was derived from the current configuration, so the moment that configuration shrank the previous run's files became unreachable. Removing the last MCP server from `imrule.toml` made `apply` stop listing `.mcp.json`, `.codex/config.toml`, `kilo.jsonc` and `opencode.json`, which dropped them out of the managed `.gitignore` block while leaving them on disk — so the next `git add .` swept generated files into the commit. `clear` could not reach them either, since it looks up the server keys in the config that no longer names them. `apply` now records what it produced in `.imrule/manifest.json` and reconciles the next run against it, removing native MCP configs it no longer writes, servers dropped from the config but still present in files it does write, and rule files, skills roots or subagent directories no longer produced. A path the user has taken over — one no longer carrying the `<!-- Generated by ImRule -->` marker — is always left alone, as are MCP servers ImRule never wrote.

- **`.imrule/cache.json` could never be ignored**: the gitignore writer skips everything under `.imrule/` because that directory is committed source. The MCP version cache is generated there, so it surfaced as an untracked file on every project. It and the new manifest are now exempt from that rule.

- **`imrule mcp add`'s contract test asserted npm's latest release**: it expected `mcp-remote@0.1.38` in the generated args while nothing pinned that value, so it passed only when the npm lookup failed. The test now seeds the project cache before `apply`.

- **Gajae Code (GJC) dropped every propagated MCP server at startup**: GJC blocks session start for only 250 ms when no server in the batch declares a `timeout`, and tears down everything still connecting at that point — an `npx`-spawned stdio server needs seconds, so every server ImRule wrote to `.gjc/mcp.json` died with "MCP server connection timed out during startup". `apply` now writes a default `timeout` of `15000` for every server that declares none, which keeps them connecting in the background while startup still blocks for at most 1.75 s.

- **`apply` destroyed agent-written OAuth credentials**: an agent that runs its own OAuth flow (GJC's `/mcp reauth`) writes the resulting `auth`/`oauth` block back into the native MCP file, under the same server name ImRule manages. The merge replaced each server object wholesale, so the credential was dropped on the next `apply` and the server went back to `HTTP 401` for good. Those two keys now survive the merge unless the ImRule definition sets them itself.

### Added

- **Per-server `timeout` for MCP definitions**: `[mcp_servers.<name>].timeout` in `imrule.toml` and `imrule mcp add --timeout <ms>` declare a connection window. `apply` fills in `15000` for every server that declares none, keeping an explicit value as written. The key is emitted only for agents whose native MCP format understands it (GJC, OpenCode, Kimi/Kimi CLI/Kimi Code) and stripped for the rest.

- **Apply manifest** (`.imrule/manifest.json`): a project-scoped record of the paths, MCP servers and native MCP targets one `apply` produced. New `domain::manifest` types with pure diffs, a `ManifestPort`, and the `JsonApplyManifest` adapter, which degrades to "no manifest" on an unparseable or future-version file rather than failing the run. A run narrowed with `--agents` folds into the existing manifest instead of replacing it, and never prunes.

### Changed

- `clear` now removes `.imrule/manifest.json` and `.imrule/cache.json` on a full run, and consults the manifest to strip MCP servers the current config has already forgotten. A run narrowed with `--agents` leaves all three alone.

## [0.2.1.0] - 2026-07-29

### Added

- **Gajae Code (GJC) skill discovery auto-enablement**: `apply` now writes `.gjc/config.yml` with `skills.enabled: true` and `skills.enablePiProject: true` when GJC is among the selected agents. GJC gates native skill discovery behind opt-in settings that default to `false`, so skills propagated to `.gjc/skills/` were copied correctly but never scanned at runtime. Existing user keys in `config.yml` are preserved via a non-destructive YAML merge.
- `clear` strips only the ImRule-managed skill keys from `.gjc/config.yml`, keeping the rest of the user's GJC config intact and deleting the file only when nothing meaningful remains.

### Changed

- Added the `GJC_CONFIG_PATH` constant (`.gjc/config.yml`) and a new `infrastructure::gjc_config` module powering the merge/strip helpers.

## [0.2.0.1] - 2026-07-24

### Fixed

- **`--version` was stuck at `0.1.0`**: `Cargo.toml` was never bumped alongside `VERSION` and `CHANGELOG.md`, so `imrule --version` reported the wrong version after every release. A new `build.rs` now reads the `VERSION` file at compile time and injects it into `clap`'s `--version` output, making `VERSION` the single source of truth for the full 4-component version (e.g. `0.2.0.1`). `Cargo.toml`'s semver `version` field is kept in sync with the major.minor.patch prefix.

## [0.2.0.0] - 2026-07-24

### Added

- **`imrule mcp auth` subcommand**: authenticates eligible remote MCP servers through `mcp-remote` with a sequential OAuth flow, skipping stdio and header-bearing servers automatically.
- **Project-scoped MCP auth cache** (`.imrule/cache.json`): resolves and pins the `mcp-remote` npm package version so `apply` and `auth` use a concrete version instead of `@latest`. The cache is written atomically and validated against strict semver.
- **Environment variable expansion**: `$VAR` and `${VAR}` references in MCP server configs are expanded from `.env` and `.imrule/.env` files, then overlaid with process environment variables.

### Changed

- The `default_agents` config key in `imrule.toml` is now `agents` (backward compatible: the legacy key still works).
- `apply` now resolves the `mcp-remote` bridge version once per invocation instead of per-agent, reducing redundant allocations in the parallel apply loop.
- Git index operations (`git rm --cached`, `git ls-files`) now report errors through a dedicated `GitTracking` error variant instead of reusing `Gitignore`.

### Fixed

- **Claude Code MCP path**: corrected from `.claude/mcp.json` to `.mcp.json` in the project root, the path Claude Code actually reads. Previous versions silently wrote MCP configs to a location Claude Code ignored.
- **`apply` no longer hard-fails offline**: when `npm` or network is unavailable for `mcp-remote` version resolution, `apply` falls back to `mcp-remote@latest` and continues instead of aborting.
- Native MCP config mappings corrected for multiple agents (Kilo Code, Crush, Gemini/Qwen, RooCode, OpenCode, Factory).
- `apply` and `clear` no longer destroy native config files that aren't strictly valid JSON.
- Duplicate `load_mcp_environment` implementations in apply and mcp auth paths consolidated into a single shared helper.

## [0.1.4.0] - 2026-06-29

### Added

- Support for **Kimi** (`kimi`, `kimi-cli`, `kimi-code`) — rules propagate to `.kimi-code/AGENTS.md`, MCP servers to `.kimi-code/mcp.json`, and skills to `.kimi-code/skills/`.
- Support for **Gajae Code (gjc)** — rules propagate to `.gjc/RULES.md`, MCP servers to `.gjc/mcp.json`, and skills to `.gjc/skills/`.

### Changed

- Native MCP config output now matches each agent's current schema: Kilo Code and Crush write under the `mcp` key; Gemini/Qwen HTTP servers use `httpUrl`; RooCode uses `streamable-http` with an explicit `disabled` default; OpenCode/Kilo use `local`/`remote` server types with `command` arrays and `environment`; Kiro/Factory default `disabled: false`. Kilo Code now writes to `kilo.jsonc` (reusing an existing `.kilo/kilo.json*` if present).
- Windsurf and Aider native MCP writing is disabled, matching what those tools actually support; `apply` no longer creates MCP files for them.

### Fixed

- `apply` and `clear` no longer destroy a native config file that isn't strictly valid JSON (e.g. a comment-bearing `kilo.jsonc` or editor settings file). imrule now stops with a clear error instead of silently overwriting or deleting your file.
- Firebender: a native MCP write no longer overwrites the generated `firebender.json` instructions.
- `clear` no longer reshapes your own (non-imrule) MCP servers while removing imrule-managed ones — it now writes them back untouched.
- `clear` no longer deletes a native config file that still carries a user-authored `$schema` key.
- `clear` now cleans up imrule MCP entries left behind for agents whose MCP support was later disabled (e.g. Windsurf) and from the legacy `.kilocode/mcp.json` path.
- Applying multiple Kimi aliases no longer races when writing the shared `.kimi-code` config.
- Codex: an explicit `http_headers` value is preserved instead of being clobbered by the `headers` alias.

## [0.1.3.0] - 2026-06-12

### Added

- **New `imrule mcp` command** for managing MCP servers directly from the CLI. `imrule mcp add <name> --transport <stdio|http|sse>` writes a server definition into the `[mcp_servers]` table of `imrule.toml`; stdio servers take their command and args after `--` (e.g. `imrule mcp add github -- npx -y @modelcontextprotocol/server-github`), while http/sse servers take the URL as a trailing positional argument. `--env`/`-e` and `--header` (both `KEY=VALUE`) attach environment variables and headers. `imrule mcp remove <name>` deletes a server. Both subcommands support `--global`/`-g` (writes to the XDG config home) and `--dry-run`.
- **TOML-based native MCP storage** for agents that use TOML config: Codex (`.codex/config.toml`), OpenCode, Mistral (`.vibe/config.toml`, array-of-tables), and OpenHands (`config.toml`). `imrule apply` now reads and writes these native TOML configs in addition to the JSON-based ones, merging imrule-managed servers without disturbing the rest of the file.
- `imrule.toml` `[mcp_servers]` definitions are unioned with `.imrule/mcp.json` when applying, so MCP servers can be declared in either source.

### Changed

- Native MCP config keys aligned with each agent's current schema: OpenCode now writes under `mcp` (was `mcpServers`) and Mistral under `mcp_servers` (was `mcpServers`).

### Fixed

- `imrule clear` now removes MCP servers declared in the `imrule.toml` `[mcp_servers]` table, not just those in `.imrule/mcp.json`. Servers added via `imrule mcp add` were previously written into every native agent config by `apply` but never cleaned up by `clear`, leaving stale entries behind. `collect_mcp_keys` now enumerates both sources, restoring the guarantee that `clear` removes everything `apply` generated.

## [0.1.2.0] - 2026-05-26

### Changed

- **MSRV bumped from 1.80 to 1.85.** Required because the upstream dependency graph (`indexmap`, `getrandom`, `tempfile`, `assert_cmd`, and others) now uses Rust `edition2024`, which is only stabilized in 1.85+. Trying to keep MSRV at 1.80 forced manual lockfile pinning of nearly every transitive dep, which is not sustainable. Users on Rust 1.80–1.84 must upgrade their toolchain to build from source.
- CI matrix updated: the `Test (MSRV 1.85)` job now runs on Rust 1.85, and the release build job uses 1.85 as well. The `Test (stable)` job continues to run on the latest stable toolchain.

### Fixed

- Two clippy lints surfaced by the 1.85 upgrade are addressed: `clippy::unnecessary_map_or` (replaced `map_or(true, ...)` with `is_none_or(...)`) and `clippy::needless_lifetimes` (elided `'a` lifetime on `impl AgentWriterPort for DefaultAgentWriter<'_>`). No behavior changes.

## [0.1.1.1] - 2026-05-26

### Fixed

- `TomlConfigLoader` no longer pulls in the caller's global `~/.config/imrule/imrule.toml` during in-process library tests. A new `TomlConfigLoader::with_xdg_home(...)` builder lets callers (currently tests) override the XDG config home that the loader falls back to when no project-local `imrule.toml` is found. Production CLI behavior is unchanged.

## [0.1.1.0] - 2026-05-22

### Fixed

- `clear` now removes subagent directories (`.claude/agents/`, `.cursor/agents/`, `.codex/agents/`, `.github/agents/`) that were previously left behind
- `clear` now removes entire skills directories (`.claude/skills/`, `.codex/skills/`, etc.) instead of only individual skill subdirectories, and works even when `.imrule/skills/` source no longer exists
- `clear` now removes empty MCP config files after removing imrule-managed keys
- `clear` now removes empty parent directories left after file deletion (`.agent/`, `.claude/`, `.codex/`, `.cursor/`, etc.)
- `clear` now respects custom `output_path` overrides from `imrule.toml` agent configs, so files written to non-default locations are properly cleaned up

## [0.1.0.1] - 2026-05-13

### Fixed

- `revert` no longer deletes user-owned files; it now checks for the ImRule generated marker before removing
- `clear` now removes only the skill subdirectories that ImRule manages, not entire agent skill roots
- MCP configurations written to agent-specific keys (e.g. Copilot `servers`) no longer lose the original `mcpServers` entries
- TOML-format MCP paths (Codex, OpenHands) are now skipped during read/write to prevent corruption
- Aider now correctly propagates MCP servers (mcp_server_key was empty)
- Subagent propagation to Claude, Cursor, Codex, and Copilot native directories is now wired into `apply`
- `skills add` prints a progress notice before the implicit apply sync
- `skills list` searches from the correct directory in global mode
- Path traversal in GitHub skill subpaths is blocked (only `Component::Normal` segments accepted)
- Symlink traversal outside the project root is blocked in markdown discovery

### Changed

- CI workflow added for Rust: `cargo fmt`, `cargo clippy`, `cargo test`, and release build on push/PR

### Removed

- Unused `PACKAGE_NAME`, `VERSION`, and `ERROR_PREFIX` constants
- Unused `verbose` field from `SkillsAddOptions`
- Unused `anyhow` dependency from `Cargo.toml`

## [0.1.0.0] - 2026-05-13

### Added

- Complete rewrite from TypeScript/Node.js to native Rust (edition 2021, MSRV 1.80)
- `imrule init` — scaffolds `.imrule/` directory with default files, supports `--global` for `~/.config/imrule/`
- `imrule apply` — reads `.imrule/` contents and propagates to 32 supported AI coding agents
- `imrule revert` — restores agent config files from `.bak` backups and removes generated content
- `imrule skills add <source>` — install skills from GitHub repos, local paths, or git URLs (vercel-labs/skills compatible)
- MCP server configuration propagation with stdio/remote filtering and merge support
- Subagent definition propagation for Claude, Cursor, Codex, and Copilot
- Skills discovery, grouping, validation warnings, and recursive copy propagation
- Gitignore managed block management (`# START ImRule Generated Files` / `# END ImRule Generated Files`)
- Generated file marker (`<!-- Generated by ImRule -->`) with idempotent write detection
- Backward compatibility for legacy `.ruler/` directory (prefers `.imrule/` when both exist)
- Hexagonal/clean architecture with strict layer boundaries (domain, application, infrastructure, interface)
- 7 contract test files covering 20 integration tests with full pass rate
- Makefile with build, test, lint, format, install, and e2e targets

### Changed

- Rebranded from "ruler" to "ImRule" across all code, configs, and documentation
- `.ruler/` directory renamed to `.imrule/`, `ruler.toml` renamed to `imrule.toml`
- Build system changed from npm/package.json to Cargo with native binary output
- CI workflows removed (to be re-added for Rust CI pipeline)

### Removed

- All TypeScript/Node.js source code and dependencies (package.json, tsconfig, eslint, jest, etc.)
- Legacy CI workflow files (.github/workflows/ruler.yml, ci.yml, release.yml)
- Development artifacts (prettierrc, prettierignore, eslint config)
