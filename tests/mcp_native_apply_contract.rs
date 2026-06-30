use std::fs;
use std::path::Path;

use imrule::application::apply_use_case::{ApplyOptions, ApplyUseCase};
use imrule::application::ports::McpPort;
use imrule::infrastructure::agent_writer::DefaultAgentWriter;
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::gitignore::GitignoreUpdater;
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use serde_json::json;
use tempfile::tempdir;

fn apply_for(root: &Path, agents: &[&str]) -> Vec<std::path::PathBuf> {
    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let gitignore = GitignoreUpdater::new();
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(&loader, &fs_port, &gitignore, &mcp_storage, &agent_writer);

    apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(agents.iter().map(|agent| (*agent).to_string()).collect()),
            config: None,
            dry_run: false,
            backup: false,
        })
        .unwrap()
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
            "args": ["-y", "@modelcontextprotocol/server-github"]
        }),
        "Kimi stdio MCP servers do not use an explicit type field"
    );
    assert_eq!(
        config["mcpServers"]["linear"],
        json!({
            "url": "https://mcp.linear.app/mcp"
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
                "enabled": true
            },
            "linear": {
                "type": "remote",
                "url": "https://mcp.linear.app/mcp",
                "enabled": true
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
    let mcp_storage = JsonMcpStorage::new();
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let apply = ApplyUseCase::new(&loader, &fs_port, &gitignore, &mcp_storage, &agent_writer);

    apply.execute(ApplyOptions {
        project_root: root.to_path_buf(),
        agents: Some(agents.iter().map(|agent| (*agent).to_string()).collect()),
        config: None,
        dry_run: false,
        backup: false,
    })
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
