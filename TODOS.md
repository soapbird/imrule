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
  **Note:** Remaining untested paths: mcp `--dry-run` early-return, `command = [array]` TOML
  parsing, CLI input-validation errors (stdio-no-command / remote-multiple-URLs), sse transport
  + headers serialization round-trip, and `build_imrule_mcp_config(None, empty)` → None.
  (Partially addressed in v0.2.0.0: `mcp --global`, `mcp auth` CLI, `parse_npm_version_output`,
  and `load_mcp_environment` error paths are now tested.)

- [ ] Deduplicate toml↔json conversion helpers across mcp_storage modules  
  **Priority:** P3  
  **Note:** `toml_value_to_json`, `extract_server_object`, and the JSON→toml insert helpers
  are duplicated verbatim between `mcp_storage_toml.rs` and `mcp_storage_openhands_toml.rs`.
  Extract into a shared private module.

- [ ] JSONC-aware parsing for `.jsonc` / editor-settings native configs  
  **Priority:** P2  
  **Note:** `read_native_mcp` now aborts (instead of destroying the file) when an existing
  native config isn't strictly valid JSON. That keeps comment-bearing `kilo.jsonc`,
  `.zed/settings.json`, etc. safe, but blocks `apply` for users who legitimately use JSONC.
  Add a tolerant JSONC parser for those paths so apply can merge without erroring. (v0.1.4.0 follow-up.)

- [ ] Proper Firebender native MCP support (unified rules + MCP JSON)  
  **Priority:** P2  
  **Note:** Firebender's native MCP path was removed in v0.1.4.0 because `firebender.json` is
  also its instructions file and a native MCP write clobbered the generated instructions.
  Implement firebender.json as a single JSON document carrying both rules and `mcpServers`.

- [ ] `apply` normalizes ALL servers in a native file, not just imrule-managed ones  
  **Priority:** P3  
  **Note:** `write_native_mcp` runs `normalize_native_mcp_json` over the whole merged file, so a
  user's hand-written servers are reshaped to the agent's schema on every apply. `clear` was
  fixed (raw write) in v0.1.4.0; consider restricting apply-side normalization to imrule-managed
  server keys too. Currently treated as intended schema-conformance.

- [ ] Rule-write `par_iter` has no output-path dedup  
  **Priority:** P3  
  **Note:** `apply_use_case.rs` writes rules in parallel; agents that share an output file
  (e.g. the ~11 agents writing root `AGENTS.md`) race on concurrent writes. Content is identical
  so it's benign today (and worse only with `--backup`), but dedup output paths like the MCP
  write now does. Pre-existing; MCP path dedup landed in v0.1.4.0.

## Adversarial Review Follow-ups

Deferred from the v0.2.0.0 adversarial review (develop → main, 2026-07-24).

### GJC skill discovery (v0.2.1.0)

Surfaced during the develop → main release-promotion pre-landing review
(2026-07-31). Two INFORMATIONAL findings; neither blocks the release.

- [ ] GJC config write bypasses DefaultAgentWriter  
  **Priority:** P2  
  **Note:** `apply_use_case.rs` writes `.gjc/config.yml` via `fs_port.write_text`
  with no content comparison (rewrites every apply even when unchanged) and no
  `.bak`, unlike the `DefaultAgentWriter` idempotency/backup invariants codified
  in AGENTS.md. Because the merge round-trips through `serde_json::Value` +
  `serde_norway`, user comments/formatting in `config.yml` are silently dropped
  on first apply with no backup. Route the GJC write through `DefaultAgentWriter`
  (or replicate its compare-and-backup) so the project invariant holds. Confidence 7/10.

- [ ] strip vs enable asymmetry on non-mapping YAML root  
  **Priority:** P3  
  **Note:** `gjc_config.rs` `enable_gjc_skill_discovery` returns `Err` when the
  existing `config.yml` root is a non-mapping scalar/array, but `strip_gjc_skill_discovery`
  returns `Ok(None)` for the identical condition (→ `clear` then deletes the file).
  Unlikely for a documented-mapping file and uncovered by tests, but the
  correctness inconsistency is real. Make `strip` also treat a non-mapping root
  as an error (or a no-op) instead of deleting the file. Confidence 5/10.

- [ ] `mcp auth` can hang forever with no output  
  **Priority:** P2  
  **Note:** `ProcessMcpRemoteRunner::authenticate` spawns `npx ... mcp-remote-client <url>`
  with stdin/stdout/stderr set to `Stdio::null()` and `child.wait()` with no timeout. The user
  gets zero feedback and if the OAuth flow never completes, the process blocks indefinitely.
  Inherit stdio or capture for live output, and add a deadline on `wait()`.

- [ ] Version cache never expires (stale pin forever)  
  **Priority:** P2  
  **Note:** `McpRemoteVersionCache` stores `resolved_at` but nothing checks age. Once pinned,
  that exact `mcp-remote` version is reused until someone manually deletes `.imrule/cache.json`.
  Add a TTL check (e.g. 7-30 days) and re-resolve on expiry.

- [ ] Corrupt `cache.json` treated as hard failure by resolver  
  **Priority:** P2  
  **Note:** `resolve_mcp_remote_version` returns `Err` on any parse/validation failure of the
  cache (including `deny_unknown_fields` rejecting a future-version key). Apply now catches this
  and falls back to `@latest` (v0.2.0.0 fix), but `mcp auth` still propagates the error. Make
  `mcp auth` also treat a corrupt cache as a cache miss.

- [ ] `.bak` backups leak prior secret values and are not gitignored  
  **Priority:** P2  
  **Note:** `--backup` writes `<file>.bak` with previous content (which may hold old resolved
  secrets). `.bak` files are not in the managed `.gitignore` block and `untrack_generated_files`
  never touches them. Add `.bak` to gitignore paths or restrict backup to non-secret files.

- [ ] `skills sync` path uses `@latest` instead of pinned version  
  **Priority:** P3  
  **Note:** The `skills add` code path constructs `ApplyUseCase` without
  `.with_mcp_remote_version_cache(...)`, so `apply` within skills sync writes `mcp-remote@latest`
  instead of the pinned version. Wire the version cache into the skills sync apply path too.

- [ ] Transport inference misclassifies stdio servers with stray `url`  
  **Priority:** P3  
  **Note:** `infer_mcp_transport` returns `Http` whenever `url` is present. A stdio server with
  a legacy `url` field is treated as HTTP. Consider inferring from `command` presence first.

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
