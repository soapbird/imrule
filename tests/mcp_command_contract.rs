use std::fs;

use imrule::application::mcp_use_case::{
    parse_env_pair, parse_env_pairs, McpAddOptions, McpRemoveOptions, McpUseCase,
};
use imrule::application::ports::ConfigPort;
use imrule::domain::config::McpTransport;
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn mcp_add_http_server_writes_to_imrule_toml() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let fs_port = FsFileSystem::new();
    let use_case = McpUseCase::new(&loader, &loader);

    use_case
        .add(McpAddOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "linear".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: vec![],
            url: Some("https://mcp.linear.app/mcp".to_string()),
            env: Default::default(),
            headers: Default::default(),
        })
        .unwrap();

    let config = loader.load_config(root, None, None).unwrap();
    let linear = config.mcp_servers.get("linear").unwrap();
    assert_eq!(linear.transport, McpTransport::Http);
    assert_eq!(linear.url.as_deref(), Some("https://mcp.linear.app/mcp"));

    let written = fs::read_to_string(root.join(".imrule/imrule.toml")).unwrap();
    assert!(written.contains("[mcp_servers.linear]"));
    assert!(written.contains("transport = \"http\""));
    assert!(written.contains("url = \"https://mcp.linear.app/mcp\""));

    // Verify apply uses the new definition.
    let mcp_storage = JsonMcpStorage::new();
    let imrule_mcp =
        imrule::domain::mcp::build_imrule_mcp_config(None, &config.mcp_servers).unwrap();
    assert_eq!(
        imrule_mcp["mcpServers"]["linear"],
        json!({ "type": "http", "url": "https://mcp.linear.app/mcp" })
    );

    // Verify it is written to Claude's native config on apply.
    use imrule::application::apply_use_case::{ApplyOptions, ApplyUseCase};
    use imrule::infrastructure::agent_writer::DefaultAgentWriter;
    use imrule::infrastructure::gitignore::GitignoreUpdater;
    let agent_writer = DefaultAgentWriter::new(&fs_port);
    let gitignore = GitignoreUpdater::new();
    let apply = ApplyUseCase::new(&loader, &fs_port, &gitignore, &mcp_storage, &agent_writer);
    let written_paths = apply
        .execute(ApplyOptions {
            project_root: root.to_path_buf(),
            agents: Some(vec!["claude".to_string()]),
            config: None,
            dry_run: false,
            backup: false,
        })
        .unwrap();

    let claude_mcp_path = root.join(".mcp.json");
    assert!(written_paths.contains(&claude_mcp_path));
    let claude_mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&claude_mcp_path).unwrap()).unwrap();
    assert_eq!(
        claude_mcp["mcpServers"]["linear"],
        json!({ "type": "http", "url": "https://mcp.linear.app/mcp" })
    );
}

#[test]
fn mcp_add_stdio_server_writes_command_and_args() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let use_case = McpUseCase::new(&loader, &loader);

    use_case
        .add(McpAddOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "github".to_string(),
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            url: None,
            env: Default::default(),
            headers: Default::default(),
        })
        .unwrap();

    let config = loader.load_config(root, None, None).unwrap();
    let github = config.mcp_servers.get("github").unwrap();
    assert_eq!(github.transport, McpTransport::Stdio);
    assert_eq!(github.command.as_deref(), Some("npx"));
    assert_eq!(
        github.args,
        vec!["-y", "@modelcontextprotocol/server-github"]
    );
}

#[test]
fn mcp_add_records_environment_variables() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let use_case = McpUseCase::new(&loader, &loader);

    let mut env = std::collections::BTreeMap::new();
    env.insert("GITHUB_TOKEN".to_string(), "secret".to_string());

    use_case
        .add(McpAddOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "github".to_string(),
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            url: None,
            env,
            headers: Default::default(),
        })
        .unwrap();

    let config = loader.load_config(root, None, None).unwrap();
    let github = config.mcp_servers.get("github").unwrap();
    assert_eq!(
        github.env.get("GITHUB_TOKEN").map(String::as_str),
        Some("secret")
    );

    let written = fs::read_to_string(root.join(".imrule/imrule.toml")).unwrap();
    assert!(written.contains("GITHUB_TOKEN = \"secret\""));
}

#[test]
fn mcp_remove_deletes_server_from_imrule_toml() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let use_case = McpUseCase::new(&loader, &loader);

    use_case
        .add(McpAddOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "linear".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: vec![],
            url: Some("https://mcp.linear.app/mcp".to_string()),
            env: Default::default(),
            headers: Default::default(),
        })
        .unwrap();

    use_case
        .remove(McpRemoveOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "linear".to_string(),
        })
        .unwrap();

    let config = loader.load_config(root, None, None).unwrap();
    assert!(!config.mcp_servers.contains_key("linear"));
}

#[test]
fn mcp_remove_missing_server_returns_error() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let use_case = McpUseCase::new(&loader, &loader);

    let result = use_case.remove(McpRemoveOptions {
        project_root: root.to_path_buf(),
        config_path: None,
        global: false,
        dry_run: false,
        name: "missing".to_string(),
    });

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn mcp_toml_servers_take_precedence_over_mcp_json() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".imrule")).unwrap();
    fs::write(
        root.join(".imrule/mcp.json"),
        r#"{"mcpServers":{"linear":{"url":"https://old.example/mcp"}}}"#,
    )
    .unwrap();

    let xdg_home = tempdir().unwrap();
    let loader = TomlConfigLoader::new().with_xdg_home(xdg_home.path().to_path_buf());
    let use_case = McpUseCase::new(&loader, &loader);

    use_case
        .add(McpAddOptions {
            project_root: root.to_path_buf(),
            config_path: None,
            global: false,
            dry_run: false,
            name: "linear".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: vec![],
            url: Some("https://new.example/mcp".to_string()),
            env: Default::default(),
            headers: Default::default(),
        })
        .unwrap();

    let config = loader.load_config(root, None, None).unwrap();
    let json_config = Some(json!({"mcpServers": {"linear": {"url": "https://old.example/mcp"}}}));
    let imrule_mcp =
        imrule::domain::mcp::build_imrule_mcp_config(json_config.as_ref(), &config.mcp_servers)
            .unwrap();
    assert_eq!(
        imrule_mcp["mcpServers"]["linear"]["url"],
        json!("https://new.example/mcp")
    );
}

#[test]
fn parse_env_pair_splits_key_value() {
    assert_eq!(
        parse_env_pair("KEY=VALUE").unwrap(),
        ("KEY".to_string(), "VALUE".to_string())
    );
    assert!(parse_env_pair("NOEQUALS").is_err());
}

#[test]
fn parse_env_pairs_builds_map() {
    let pairs = vec!["A=1".to_string(), "B=2".to_string()];
    let map = parse_env_pairs(&pairs).unwrap();
    assert_eq!(map.get("A").map(String::as_str), Some("1"));
    assert_eq!(map.get("B").map(String::as_str), Some("2"));
}
