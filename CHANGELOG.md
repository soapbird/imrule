# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.3.0] - 2026-06-12

### Added

- **New `imrule mcp` command** for managing MCP servers directly from the CLI. `imrule mcp add <name>` writes a server definition (stdio, http, or sse transport, with `--command`/`--args`/`--url`/`--env`/`--header`) into the `[mcp_servers]` table of `imrule.toml`, and `imrule mcp remove <name>` deletes it. Both support `--global` (writes to the XDG config home) and `--dry-run`.
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
