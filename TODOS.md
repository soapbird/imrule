# TODOS

## MCP

Deferred from the pre-landing review of the `imrule mcp` + TOML MCP storage feature
(develop → main, 2026-06-12). The verified clear-orphaning bug (collect_mcp_keys) was
fixed in that PR; the items below are the related apply↔clear lifecycle hardening.

- [ ] OpenHands remote servers keyed by URL not name in `read_remotes`  
  **Priority:** P1  
  **Note:** `mcp_storage_openhands_toml.rs:99` inserts remote servers under their URL,
  not their name. `clear` removes by name → can't match them; re-apply accumulates
  duplicates. Persist the server name on write and key by it on read. Confidence 7/10.

- [ ] Legacy MCP key blocks orphaned on upgrade (opencode/mistral key flips)  
  **Priority:** P1  
  **Note:** `agent.rs` flipped opencode `mcpServers`→`mcp` and mistral `mcpServers`→
  `mcp_servers`. `apply` writes the new key but never strips the old one, so a re-apply
  (without clear) leaves two competing MCP blocks. Strip the prior key on write. Confidence 6/10.

- [ ] Overwrite strategy clobbers user-authored native MCP servers  
  **Priority:** P1  
  **Note:** `mcp_storage_toml.rs` write_codex_mcp/write_mistral_mcp replace the entire
  `mcp_servers` table. Under `McpStrategy::Overwrite`, unmanaged hand-written servers are
  silently deleted on apply. Replace per-key, or guard/document the destructive behavior. Confidence 6/10.

- [ ] Emptied TOML native files never deleted by clear  
  **Priority:** P2  
  **Note:** `clear_use_case.rs` remove_mcp_file_if_empty uses a JSON-only emptiness check,
  so emptied `.codex/config.toml` / `.vibe/config.toml` survive (asymmetric with the JSON
  path). Detect TOML paths and emptiness-check the parsed document. Confidence 6/10.

- [ ] Malformed native TOML aborts the whole apply run  
  **Priority:** P2  
  **Note:** `mcp_storage_toml.rs:48` hard-errors on any parse failure, so one bad
  `.codex/config.toml` blocks applying rules to every other agent. Skip/log that single
  agent instead, matching the JSON path's fallback. Confidence 6/10.

- [ ] `parse_env_pair` accepts empty/whitespace env keys  
  **Priority:** P2  
  **Note:** `mcp_use_case.rs` splits on first `=`; `=value` yields an empty key that gets
  serialized into agent configs and may break their TOML parsers. Validate against
  `[A-Za-z_][A-Za-z0-9_]*`. Confidence 7/10.

- [ ] MCP secrets written world-readable (no 0600)  
  **Priority:** P2  
  **Note:** `config_loader.rs` save_config and `mcp_storage.rs` write_native_mcp write env
  tokens / Authorization headers to disk under the default umask (0644). Single-user CLI
  trust model, but harden with 0600 on Unix (PermissionsExt) or document plaintext storage. Confidence 7/10.

- [ ] MCP coverage gaps (test-only)  
  **Priority:** P3  
  **Note:** Untested paths from the coverage audit (gate passed at 82%): mcp `--dry-run`
  early-return, mcp `--global` flag, `command = [array]` TOML parsing, CLI input-validation
  errors (stdio-no-command / remote-multiple-URLs), sse transport + headers serialization
  round-trip, and `build_imrule_mcp_config(None, empty)` → None.

- [ ] Deduplicate toml↔json conversion helpers across mcp_storage modules  
  **Priority:** P3  
  **Note:** `toml_value_to_json`, `extract_server_object`, and the JSON→toml insert helpers
  are duplicated verbatim between `mcp_storage_toml.rs` and `mcp_storage_openhands_toml.rs`.
  Extract into a shared private module.

## Coverage

- [ ] Test `GitSkillFetcher` (GitHub/GitLab/SSH fetch paths)  
  **Priority:** P3  
  **Note:** Requires network/git access; needs fixture or mock-based approach.

## Completed

- [x] Rewrite CLI from TypeScript to native Rust  
  **Completed:** v0.1.0.0 (2026-05-13)

- [x] Add `imrule clear` command  
  **Completed:** v0.1.0.0 (2026-05-13)

- [x] Fix 15 post-migration review findings (security, correctness, dead code)  
  **Completed:** v0.1.0.1 (2026-05-13)

- [x] Add 7 contract tests covering apply_subagents, clear MCP/skills, disabled agent, skills list, unknown agent error, global dir fallback  
  **Completed:** v0.1.0.1 (2026-05-13)
