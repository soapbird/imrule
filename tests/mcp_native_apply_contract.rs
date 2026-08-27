use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use imrule::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use imrule::application::mcp_use_case::McpRemoteVersionResolverPort;
use imrule::application::ports::McpPort;
use imrule::domain::error::ImruleError;
use imrule::domain::mcp::McpRemoteVersionCache;
use imrule::infrastructure::agent_writer::DefaultAgentWriter;
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::git_tracking::GitUntracker;
use imrule::infrastructure::gitignore::GitignoreUpdater;
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use imrule::infrastructure::version_cache::JsonVersionCache;
use serde_json::json;
use tempfile::tempdir;

fn configure_native_remote_transport(root: &Path) {
    let config_path = root.join(".imrule/imrule.toml");
    let existing = fs::read_to_string(&config_path).unwrap();
    fs::write(
        config_path,
        format!("{existing}\n[mcp]\nremote_transport = \"native\"\n"),
    )
    .unwrap();
}

fn apply_for(root: &Path, agents: &[&str]) -> Vec<std::path::PathBuf> {
    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    );

    configure_native_remote_transport(root);
    apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(agents.iter().map(|agent| (*agent).to_string()).collect()),
            config: None,
            dry_run: false,
            backup: false,
        })
        .unwrap()
        .written
}

fn write_imrule_fixture(root: &Path) {
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        r#"
[mcp_servers.github]
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.linear]
transport = "http"
url = "https://mcp.linear.app/mcp"
"#,
    )
    .unwrap();
}

#[test]
fn apply_writes_codex_mcp_servers_to_project_config_toml() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["codex"]);

    let codex_config_path = root.join(".codex/config.toml");
    assert!(written.contains(&codex_config_path));
    let codex_config = fs::read_to_string(codex_config_path).unwrap();
    assert!(codex_config.contains("[mcp_servers.github]"));
    assert!(codex_config.contains("command = \"npx\""));
    assert!(codex_config.contains("args = [\"-y\", \"@modelcontextprotocol/server-github\"]"));
    assert!(codex_config.contains("[mcp_servers.linear]"));
    assert!(codex_config.contains("url = \"https://mcp.linear.app/mcp\""));
}

#[test]
fn apply_skips_aider_mcp_because_aider_has_no_native_mcp_support() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["aider"]);

    assert!(!written.contains(&root.join(".mcp.json")));
    assert!(!root.join(".mcp.json").exists());
}

#[test]
fn apply_skips_windsurf_mcp_without_project_mcp_contract() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["windsurf"]);

    assert!(!written.contains(&root.join(".windsurf/mcp_config.json")));
    assert!(!root.join(".windsurf/mcp_config.json").exists());
}

#[test]
fn apply_writes_codex_remote_headers_as_http_headers() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        r#"
[mcp_servers.docs]
transport = "http"
url = "https://example.test/mcp"

[mcp_servers.docs.headers]
Authorization = "Bearer token"
"#,
    )
    .unwrap();

    apply_for(root, &["codex"]);

    let codex_config = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(codex_config.contains("[mcp_servers.docs.http_headers]"));
    assert!(codex_config.contains("Authorization = \"Bearer token\""));
    assert!(!codex_config.contains("[mcp_servers.docs.headers]"));
}
#[test]
fn apply_expands_mcp_variables_from_project_environment_files() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();
    fs::write(
        root.join(".env"),
        "IMRULE_MCP_ROOT_TOKEN=root-secret\nIMRULE_MCP_OVERRIDE=root-value\n",
    )
    .unwrap();
    fs::write(
        root.join(".imrule/.env"),
        "IMRULE_MCP_LOCAL_TOKEN=local-secret\nIMRULE_MCP_OVERRIDE=imrule-value\n",
    )
    .unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        r#"
[mcp_servers.local]
transport = "stdio"
command = "npx"
env = { ROOT_TOKEN = "${IMRULE_MCP_ROOT_TOKEN}", LOCAL_TOKEN = "$IMRULE_MCP_LOCAL_TOKEN", OVERRIDE = "${IMRULE_MCP_OVERRIDE}", MISSING = "${IMRULE_MCP_MISSING}" }

[mcp_servers.remote]
transport = "http"
url = "https://example.test/mcp"
headers = { Authorization = "Bearer ${IMRULE_MCP_LOCAL_TOKEN}" }
"#,
    )
    .unwrap();

    apply_for(root, &["codex"]);

    let codex_config = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(codex_config.contains("ROOT_TOKEN = \"root-secret\""));
    assert!(codex_config.contains("LOCAL_TOKEN = \"local-secret\""));
    assert!(codex_config.contains("OVERRIDE = \"imrule-value\""));
    assert!(codex_config.contains("MISSING = \"${IMRULE_MCP_MISSING}\""));
    assert!(codex_config.contains("Authorization = \"Bearer local-secret\""));
}

#[test]
fn apply_writes_gemini_and_qwen_http_servers_with_http_url() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["gemini-cli", "qwen"]);

    for relative_path in [".gemini/settings.json", ".qwen/settings.json"] {
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join(relative_path)).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["linear"],
            json!({
                "httpUrl": "https://mcp.linear.app/mcp"
            }),
            "{relative_path} should use Streamable HTTP's httpUrl key"
        );
    }
}

#[test]
fn apply_writes_kimi_mcp_servers_to_project_config() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["kimi-cli", "kimi-code", "kimi"]);

    let config_path = root.join(".kimi-code/mcp.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
    assert_eq!(
        config["mcpServers"]["github"],
        json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "timeout": 15000
        }),
        "Kimi stdio MCP servers do not use an explicit type field"
    );
    assert_eq!(
        config["mcpServers"]["linear"],
        json!({
            "url": "https://mcp.linear.app/mcp",
            "timeout": 15000
        }),
        "Kimi HTTP MCP servers use a plain url without an explicit type field"
    );
}

#[test]
fn apply_writes_roo_http_servers_as_streamable_http() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["roo"]);

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".roo/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        config["mcpServers"]["linear"]["type"],
        json!("streamable-http"),
        "Roo expects streamable-http transport name"
    );
    assert_eq!(
        config["mcpServers"]["linear"]["url"],
        json!("https://mcp.linear.app/mcp")
    );
    assert_eq!(
        config["mcpServers"]["linear"]["disabled"],
        json!(false),
        "Roo expects explicit enabled-by-default server state"
    );
}

#[test]
fn apply_writes_kilo_servers_to_current_project_config() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["kilocode"]);

    let config_path = root.join("kilo.jsonc");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config["mcp"]["github"],
        json!({
            "type": "local",
            "command": ["npx", "-y", "@modelcontextprotocol/server-github"],
            "enabled": true
        })
    );
    assert_eq!(
        config["mcp"]["linear"],
        json!({
            "type": "remote",
            "url": "https://mcp.linear.app/mcp",
            "enabled": true
        })
    );
    assert!(!root.join("kilo.json").exists());
    assert!(!root.join(".kilocode/mcp.json").exists());
}

#[test]
fn apply_writes_crush_servers_under_native_mcp_key() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["crush"]);

    let crush_config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".crush.json")).unwrap()).unwrap();
    assert!(crush_config.get("mcpServers").is_none());
    assert_eq!(
        crush_config["mcp"]["linear"]["type"],
        json!("http"),
        "Crush expects the top-level mcp key, not mcpServers"
    );
}

#[test]
fn apply_writes_zed_servers_without_transport_type() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["zed"]);

    let zed_config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".zed/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        zed_config["context_servers"]["linear"],
        json!({ "url": "https://mcp.linear.app/mcp" })
    );
    assert_eq!(
        zed_config["context_servers"]["github"],
        json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"]
        })
    );
}

#[test]
fn apply_does_not_write_native_mcp_to_firebender_instructions_file() {
    // firebender.json is Firebender's INSTRUCTIONS file. It must NOT also receive
    // a native MCP write, or the MCP JSON would overwrite the generated
    // instructions (and vice versa). Apply writes only the instructions there.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    apply_for(root, &["firebender"]);

    // The instructions file exists and is NOT clobbered into MCP JSON: it has no
    // top-level `mcpServers` object (it is the rules markdown).
    let contents = fs::read_to_string(root.join("firebender.json")).unwrap();
    let parsed_as_mcp = serde_json::from_str::<serde_json::Value>(&contents)
        .ok()
        .and_then(|v| v.get("mcpServers").cloned());
    assert!(
        parsed_as_mcp.is_none(),
        "firebender.json must keep instructions, not be overwritten with MCP JSON"
    );
}

#[test]
fn apply_writes_opencode_servers_under_native_mcp_key() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["opencode"]);

    let opencode_config_path = root.join("opencode.json");
    assert!(written.contains(&opencode_config_path));
    let opencode_config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(opencode_config_path).unwrap()).unwrap();
    assert_eq!(
        opencode_config["mcp"],
        json!({
            "github": {
                "type": "local",
                "command": ["npx", "-y", "@modelcontextprotocol/server-github"],
                "enabled": true,
                "timeout": 15000
            },
            "linear": {
                "type": "remote",
                "url": "https://mcp.linear.app/mcp",
                "enabled": true,
                "timeout": 15000
            }
        })
    );
    assert!(opencode_config.get("mcpServers").is_none());
}

#[test]
fn apply_writes_mistral_servers_as_array_of_tables() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["mistral"]);

    let mistral_config_path = root.join(".vibe/config.toml");
    assert!(written.contains(&mistral_config_path));
    let mistral_config = fs::read_to_string(mistral_config_path).unwrap();
    assert!(mistral_config.contains("[[mcp_servers]]"));
    assert!(mistral_config.contains("name = \"github\""));
    assert!(mistral_config.contains("transport = \"stdio\""));
    assert!(mistral_config.contains("command = \"npx\""));
    assert!(mistral_config.contains("name = \"linear\""));
    assert!(mistral_config.contains("transport = \"http\""));
    assert!(mistral_config.contains("url = \"https://mcp.linear.app/mcp\""));
    assert!(!mistral_config.contains("[mcp_servers.github]"));
}

#[test]
fn apply_writes_openhands_servers_under_mcp_section() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let written = apply_for(root, &["openhands"]);

    let openhands_config_path = root.join("config.toml");
    assert!(written.contains(&openhands_config_path));
    let openhands_config = fs::read_to_string(openhands_config_path).unwrap();
    assert!(openhands_config.contains("[mcp]"));
    assert!(openhands_config.contains(
        "stdio_servers = [{ name = \"github\", args = [\"-y\", \"@modelcontextprotocol/server-github\"], command = \"npx\" }]"
    ));
    assert!(openhands_config.contains("shttp_servers = [{ url = \"https://mcp.linear.app/mcp\" }]"));
    assert!(!openhands_config.contains("[mcp_servers.github]"));
}

/// Like `apply_for`, but returns the raw `Result` so error paths can be asserted.
fn try_apply_for(
    root: &Path,
    agents: &[&str],
) -> Result<Vec<std::path::PathBuf>, imrule::domain::error::ImruleError> {
    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    );

    apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(agents.iter().map(|agent| (*agent).to_string()).collect()),
            config: None,
            dry_run: false,
            backup: false,
        })
        .map(|result| result.written)
}

#[test]
fn apply_aborts_without_clobbering_invalid_existing_native_config() {
    // Regression: a comment-bearing / invalid JSON native config (e.g. a real
    // JSONC kilo file) must NOT be silently parsed as `{}` and overwritten with
    // imrule-only servers. Apply errors and leaves the file byte-for-byte intact.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let original = "{\n  // user's own server, with a comment\n  \"mcp\": { \"mine\": { \"command\": \"node\" } },\n}\n";
    fs::write(root.join("kilo.jsonc"), original).unwrap();

    let result = try_apply_for(root, &["kilocode"]);
    assert!(
        result.is_err(),
        "apply must abort on an unparseable native config"
    );
    assert_eq!(
        fs::read_to_string(root.join("kilo.jsonc")).unwrap(),
        original,
        "the user's config must be left untouched"
    );
}

#[test]
fn apply_reuses_existing_kilo_config_at_non_default_candidate() {
    // Kilo's path list is first-existing-wins. If a config already exists at a
    // non-default candidate (.kilo/kilo.json), apply must reuse it rather than
    // creating a fresh kilo.jsonc.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);
    fs::create_dir_all(root.join(".kilo")).unwrap();
    fs::write(root.join(".kilo/kilo.json"), "{}").unwrap();

    apply_for(root, &["kilocode"]);

    assert!(root.join(".kilo/kilo.json").exists());
    assert!(!root.join("kilo.jsonc").exists());
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".kilo/kilo.json")).unwrap()).unwrap();
    assert!(config["mcp"]["github"].is_object());
}

#[test]
fn codex_keeps_explicit_http_headers_over_headers_alias() {
    // Regression: `headers` is the imrule alias for codex's `http_headers`. When
    // a server carries both, the explicit `http_headers` must win and the alias
    // must be dropped — never double-written so one set silently clobbers the
    // other.
    let tmp = tempdir().unwrap();
    let target = tmp.path().join(".codex/config.toml");
    let mcp = JsonMcpStorage::new();
    mcp.write_native_mcp(
        &target,
        &json!({
            "mcpServers": {
                "svc": {
                    "type": "http",
                    "url": "https://example.test/mcp",
                    "headers": { "X-Alias": "from-headers" },
                    "http_headers": { "X-Explicit": "from-http-headers" }
                }
            }
        }),
    )
    .unwrap();

    let written = fs::read_to_string(&target).unwrap();
    assert!(
        written.contains("X-Explicit"),
        "explicit http_headers must be kept"
    );
    assert!(
        !written.contains("X-Alias"),
        "the headers alias must be dropped when http_headers is explicit"
    );
}

// --- Gap coverage: apply end-to-end with the mcp-remote version cache wired ---

struct CountingResolver {
    calls: AtomicUsize,
}

impl McpRemoteVersionResolverPort for CountingResolver {
    fn resolve_latest_version(&self) -> Result<String, ImruleError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("0.1.37".to_string())
    }
}

#[test]
fn apply_with_version_cache_writes_concrete_bridge_spec_and_persists_cache() {
    // Most apply tests construct ApplyUseCase::new(...) WITHOUT
    // .with_mcp_remote_version_cache(...), so the private
    // `mcp_remote_version_cache()` method always returns Ok(None) and the
    // `filter_mcp_config_for_agent_with_version_cache` branch is never reached
    // through execute(). This wires the cache + resolver and asserts that:
    //   1. apply resolves a concrete version on first run,
    //   2. the bridged stdio args embed mcp-remote@<version> (not @latest),
    //   3. the cache is persisted to .imrule/cache.json,
    //   4. a second apply reuses the cache without re-resolving.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);
    // Do NOT set remote_transport = "native" — the default mcp-remote mode is
    // what triggers version resolution.

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let version_cache = JsonVersionCache::new();
    let resolver = CountingResolver {
        calls: AtomicUsize::new(0),
    };

    let make_apply = || {
        let apply = ApplyUseCase::new(
            &loader,
            &fs_port,
            &gitignore,
            &git_untracker,
            &mcp_storage,
            &agent_writer,
        )
        .with_mcp_remote_version_cache(&version_cache, &resolver);
        apply
            .execute(ApplyOptions {
                project_root: root.to_path_buf(),
                agents: Some(vec!["claude".to_string()]),
                config: None,
                dry_run: false,
                backup: false,
            })
            .unwrap()
    };

    let first = make_apply();
    assert!(first.written.iter().any(|p| p.ends_with(".mcp.json")));
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);

    let claude_mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".mcp.json")).unwrap()).unwrap();
    // The cached concrete version replaces the @latest placeholder.
    assert_eq!(
        claude_mcp["mcpServers"]["linear"],
        json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "mcp-remote@0.1.37", "https://mcp.linear.app/mcp"]
        })
    );

    // The cache was persisted.
    let cache_path = root.join(".imrule/cache.json");
    assert!(
        cache_path.exists(),
        "apply must persist the resolved version cache"
    );
    let cached: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cache_path).unwrap()).unwrap();
    assert_eq!(cached["resolved_version"], json!("0.1.37"));
    assert_eq!(cached["package"], json!("mcp-remote"));

    // Second apply reuses the cache — resolver is NOT called again.
    let _second = make_apply();
    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        1,
        "second apply must reuse the persisted cache"
    );
}

#[test]
fn apply_dry_run_does_not_persist_version_cache() {
    // Dry-run must resolve the version (so the filtered output is accurate) but
    // must NOT write the cache file — persist is gated on !options.dry_run.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    write_imrule_fixture(root);

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let version_cache = JsonVersionCache::new();
    let resolver = CountingResolver {
        calls: AtomicUsize::new(0),
    };
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    )
    .with_mcp_remote_version_cache(&version_cache, &resolver);

    let result = apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(vec!["claude".to_string()]),
            config: None,
            dry_run: true,
            backup: false,
        })
        .unwrap();
    assert!(result.written.iter().any(|p| p.ends_with(".mcp.json")));
    // Resolver ran because dry-run still resolves to render accurate output.
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    // But no cache file was written.
    assert!(!root.join(".imrule/cache.json").exists());
    // And no native MCP file was written either (dry-run).
    assert!(!root.join(".mcp.json").exists());
}

#[test]
fn apply_with_version_cache_skips_resolution_for_stdio_only_servers() {
    // When no server is a remote URL, the version resolver must never be
    // invoked even when the cache is wired — there is nothing to bridge.
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(root.join(".imrule/AGENTS.md"), "Project rules.").unwrap();
    fs::write(
        root.join(".imrule/imrule.toml"),
        r#"
[mcp_servers.local]
transport = "stdio"
command = "npx"
args = ["-y", "demo"]
"#,
    )
    .unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let git_untracker = GitUntracker::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let version_cache = JsonVersionCache::new();
    let resolver = CountingResolver {
        calls: AtomicUsize::new(0),
    };
    let apply = ApplyUseCase::new(
        &loader,
        &fs_port,
        &gitignore,
        &git_untracker,
        &mcp_storage,
        &agent_writer,
    )
    .with_mcp_remote_version_cache(&version_cache, &resolver);

    apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(vec!["claude".to_string()]),
            config: None,
            dry_run: false,
            backup: false,
        })
        .unwrap();

    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        0,
        "resolver must not run when no remote servers need bridging"
    );
    assert!(!root.join(".imrule/cache.json").exists());
}

#[test]
fn version_cache_struct_round_trip_through_serde_roundabout() {
    // Validates the manual Deserialize impl + TryFrom path that runs whenever
    // the on-disk cache is read: a well-formed value round-trips, and the
    // deny_unknown_fields guard rejects extra keys at the serde layer.
    let cache = McpRemoteVersionCache::new("1.2.3-rc.1+build.4", 1_700_000_001).unwrap();
    let serialized = serde_json::to_string(&cache).unwrap();
    let deserialized: McpRemoteVersionCache = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.resolved_version(), "1.2.3-rc.1+build.4");
    assert_eq!(deserialized.resolved_at(), 1_700_000_001);

    let with_extra = format!(
        r#"{{"package":"mcp-remote","resolved_version":"0.1.0","resolved_at":1,"extra":"leak"}}"#
    );
    assert!(serde_json::from_str::<McpRemoteVersionCache>(&with_extra).is_err());
}
