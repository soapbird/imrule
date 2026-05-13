# Imrule

**Apply the same rules to all coding agents.**

Imrule lets you write project instructions, MCP server configurations, skills, and subagent definitions once in a `.ruler/` directory, then propagates them to every supported AI coding agent with a single command.

## Features

- **30+ supported agents** — Copilot, Claude Code, Codex, Cursor, Windsurf, Cline, Aider, Gemini CLI, and many more
- **MCP server config** — merge or overwrite `mcp.json` into each agent's native format
- **Skills propagation** — sync `.ruler/skills/` to agent-specific skill directories
- **Subagent propagation** — transform `.ruler/agents/` definitions to agent-native formats
- **Gitignore management** — automatically update `.gitignore` with generated paths
- **Backup & revert** — `.bak` files on apply, clean rollback with `imrule revert`
- **Dry-run mode** — preview all changes without writing files
- **Global config** — `~/.config/ruler/` for shared rules across projects
- **Nested projects** — discover `.ruler/` in parent directories

## Installation

### From source

```bash
git clone https://github.com/soapbird/imrule.git
cd imrule
make build
sudo make install     # copies binary to /usr/local/bin/imrule
```

### With Cargo

```bash
cargo install --path .
```

### Custom install prefix

```bash
make install PREFIX=~/.local    # installs to ~/.local/bin/imrule
```

### Uninstall

```bash
sudo make uninstall             # removes /usr/local/bin/imrule
```

## Quick Start

```bash
# 1. Initialize a .ruler directory
imrule init

# 2. Edit your instructions
vim .ruler/AGENTS.md

# 3. Apply to all agents
imrule apply

# 4. Revert when needed
imrule revert
```

## Usage

### `imrule init`

Scaffold a `.ruler/` directory with default files (`AGENTS.md`, `ruler.toml`).

```bash
imrule init                          # local .ruler/ in current directory
imrule init --global                 # ~/.config/ruler/
imrule init --project-root ~/myproj  # specify a different project
```

### `imrule apply`

Read `.ruler/` contents and write to each agent's native config files.

```bash
imrule apply                              # all agents, current directory
imrule apply --agents claude,copilot      # specific agents only
imrule apply --dry-run                    # preview without writing
imrule apply --no-mcp                     # skip MCP config
imrule apply --mcp-overwrite              # replace (not merge) MCP config
imrule apply --verbose                    # show file counts
imrule apply --local-only                 # ignore global config
imrule apply --backup false               # disable .bak files
imrule apply --project-root ~/myproj      # specify project root
imrule apply --config custom.toml         # use custom config file
```

### `imrule revert`

Restore agent config files from backups and remove generated content.

```bash
imrule revert                             # revert all agents
imrule revert --agents claude             # revert specific agent
imrule revert --dry-run                   # preview revert
imrule revert --keep-backups              # keep .bak files
```

## Supported Agents

| Identifier | Agent | Identifier | Agent |
|---|---|---|---|
| `agentsmd` | AgentsMd | `copilot` | GitHub Copilot |
| `claude` | Claude Code | `codex` | OpenAI Codex CLI |
| `cursor` | Cursor | `windsurf` | Windsurf |
| `cline` | Cline | `aider` | Aider |
| `firebase` | Firebase Studio | `openhands` | Open Hands |
| `gemini-cli` | Gemini CLI | `jules` | Jules |
| `junie` | Junie | `augmentcode` | AugmentCode |
| `kilocode` | Kilo Code | `opencode` | OpenCode |
| `goose` | Goose | `crush` | Crush |
| `amp` | Amp | `zed` | Zed |
| `qwen` | Qwen Code | `kiro` | Kiro |
| `warp` | Warp | `roo` | RooCode |
| `trae` | Trae AI | `amazonqcli` | Amazon Q CLI |
| `firebender` | Firebender | `factory` | Factory Droid |
| `antigravity` | Antigravity | `mistral` | Mistral |
| `pi` | Pi Coding Agent | `jetbrains-ai` | JetBrains AI Assistant |

## Configuration

### `.ruler/AGENTS.md`

Central markdown file for your coding guidelines, style guides, and project context. All `.md` files in `.ruler/` (including subdirectories) are concatenated, starting with `AGENTS.md` (if present), then remaining files in sorted order.

### `.ruler/ruler.toml`

```toml
# Default agents when --agents is not specified
# default_agents = ["claude", "copilot"]

# [agents.ClaudeCode]
# enabled = true
# output_path = "CLAUDE.md"

# [agents.GitHubCopilot]
# enabled = true
# output_path = ".github/copilot-instructions.md"

# [mcp]
# enabled = true
# strategy = "merge"    # or "overwrite"

# [gitignore]
# enabled = true
# local = false

# [skills]
# enabled = true

# [subagents]
# enabled = true
# include_in_rules = false
```

### `.ruler/mcp.json`

MCP server definitions in standard format:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["my-mcp-server"]
    }
  }
}
```

### `.ruler/skills/`

Place skill directories here. Each skill needs a `SKILL.md` file.

### `.ruler/agents/`

Place subagent definitions here as markdown files with YAML frontmatter.

## Development

Requires Rust 1.80+.

```bash
make              # fmt + lint + test + build
make build        # cargo build --release
make test         # cargo test
make test-e2e     # end-to-end tests against test-e2e/ fixtures
make install      # copy binary to /usr/local/bin/imrule (needs sudo)
make uninstall    # remove /usr/local/bin/imrule
make check        # fast compile check
make lint         # cargo clippy
make fmt          # check formatting
make fmt-fix      # auto-format
make clean        # cargo clean
```

## Architecture

```
src/
  domain/          — Pure business logic, zero I/O
  application/     — Use cases (apply, init, revert)
  infrastructure/  — Filesystem, config loader, gitignore, MCP storage
  interface/       — CLI adapter (clap)
```

## License

MIT
