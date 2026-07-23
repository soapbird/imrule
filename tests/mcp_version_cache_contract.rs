use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use imrule::application::mcp_use_case::{
    McpAuthOptions, McpAuthRunnerPort, McpRemoteVersionResolverPort, McpUseCase,
};
use imrule::application::ports::CachePort;
use imrule::domain::agent::all_agents;
use imrule::domain::config::McpRemoteTransport;
use imrule::domain::constants::{
    IMRULE_CACHE_PATH, MCP_REMOTE_LATEST_PACKAGE_SPEC, MCP_REMOTE_PACKAGE,
};
use imrule::domain::error::ImruleError;
use imrule::domain::mcp::{filter_mcp_config_for_agent_with_version_cache, McpRemoteVersionCache};
use imrule::infrastructure::config_loader::TomlConfigLoader;
use imrule::infrastructure::file_system::FsFileSystem;
use imrule::infrastructure::mcp_storage::JsonMcpStorage;
use imrule::infrastructure::version_cache::JsonVersionCache;
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn version_cache_round_trips_only_the_closed_schema() {
    let temporary = tempdir().unwrap();
    let storage = JsonVersionCache::new();
    assert_eq!(
        storage.read_mcp_remote_version(temporary.path()).unwrap(),
        None
    );

    let cache = McpRemoteVersionCache::new("0.1.37", 1_700_000_000).unwrap();
    storage
        .write_mcp_remote_version_atomic(temporary.path(), &cache)
        .unwrap();

    assert_eq!(
        storage.read_mcp_remote_version(temporary.path()).unwrap(),
        Some(cache.clone())
    );
    assert_eq!(cache.package(), MCP_REMOTE_PACKAGE);
    assert_eq!(cache.resolved_version(), "0.1.37");
    assert_eq!(cache.resolved_at(), 1_700_000_000);
    assert_eq!(cache.package_spec(), "mcp-remote@0.1.37");

    let written: Value = serde_json::from_str(
        &fs::read_to_string(temporary.path().join(IMRULE_CACHE_PATH)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        written,
        json!({
            "package": "mcp-remote",
            "resolved_version": "0.1.37",
            "resolved_at": 1_700_000_000_u64
        })
    );
}

#[test]
fn version_cache_rejects_unknown_or_non_concrete_data() {
    let temporary = tempdir().unwrap();
    let cache_path = temporary.path().join(IMRULE_CACHE_PATH);
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    let storage = JsonVersionCache::new();

    for forbidden_key in ["url", "headers", "token", "oauth"] {
        let mut data = json!({
            "package": "mcp-remote",
            "resolved_version": "0.1.37",
            "resolved_at": 1_700_000_000_u64
        });
        data.as_object_mut()
            .unwrap()
            .insert(forbidden_key.to_string(), json!("sensitive-value"));
        fs::write(&cache_path, serde_json::to_vec(&data).unwrap()).unwrap();

        let error = storage
            .read_mcp_remote_version(temporary.path())
            .unwrap_err();
        assert!(error.to_string().contains("could not parse version cache"));
        assert!(!error.to_string().contains("sensitive-value"));
    }

    fs::write(
        &cache_path,
        serde_json::to_vec(&json!({
            "package": "other-package",
            "resolved_version": "0.1.37",
            "resolved_at": 1_700_000_000_u64
        }))
        .unwrap(),
    )
    .unwrap();
    let error = storage
        .read_mcp_remote_version(temporary.path())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("version cache package must be 'mcp-remote'"));

    assert!(McpRemoteVersionCache::new("latest", 1_700_000_000).is_err());
    assert!(McpRemoteVersionCache::new("https://example.test", 1_700_000_000).is_err());
    assert!(McpRemoteVersionCache::new("0.1.37", 0).is_err());
    assert!(McpRemoteVersionCache::new("01.2.3", 1_700_000_000).is_err());
    assert!(McpRemoteVersionCache::new("1.2.3-01", 1_700_000_000).is_err());
    assert!(McpRemoteVersionCache::new("1.2.3+", 1_700_000_000).is_err());
    assert!(McpRemoteVersionCache::new("1.2.3-beta.1+build.7", 1_700_000_000).is_ok());
}

#[test]
fn atomic_cache_replacement_leaves_no_temporary_file() {
    let temporary = tempdir().unwrap();
    let storage = JsonVersionCache::new();
    storage
        .write_mcp_remote_version_atomic(
            temporary.path(),
            &McpRemoteVersionCache::new("0.1.36", 1_700_000_000).unwrap(),
        )
        .unwrap();
    storage
        .write_mcp_remote_version_atomic(
            temporary.path(),
            &McpRemoteVersionCache::new("0.1.37", 1_700_000_001).unwrap(),
        )
        .unwrap();

    let entries: Vec<_> = fs::read_dir(temporary.path().join(".imrule"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec!["cache.json"]);
    assert_eq!(
        storage
            .read_mcp_remote_version(temporary.path())
            .unwrap()
            .unwrap()
            .package_spec(),
        "mcp-remote@0.1.37"
    );
}

#[test]
fn apply_filter_accepts_the_cached_concrete_package_spec() {
    let agents = all_agents();
    let agent = agents
        .iter()
        .find(|agent| agent.identifier == "firebase")
        .unwrap();
    let config = json!({
        "mcpServers": {
            "remote": {
                "type": "http",
                "url": "https://example.test/mcp"
            }
        }
    });
    let cache = McpRemoteVersionCache::new("0.1.37", 1_700_000_000).unwrap();

    let filtered = filter_mcp_config_for_agent_with_version_cache(
        &config,
        agent,
        McpRemoteTransport::McpRemote,
        &cache,
    )
    .unwrap();
    assert_eq!(
        filtered["mcpServers"]["remote"]["args"],
        json!(["-y", "mcp-remote@0.1.37", "https://example.test/mcp"])
    );
    assert_eq!(MCP_REMOTE_LATEST_PACKAGE_SPEC, "mcp-remote@latest");
}

struct ContractVersionResolver {
    calls: AtomicUsize,
}

impl McpRemoteVersionResolverPort for ContractVersionResolver {
    fn resolve_latest_version(&self) -> Result<String, ImruleError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("0.1.37".to_string())
    }
}

#[derive(Default)]
struct ContractAuthRunner {
    calls: Mutex<Vec<(String, String)>>,
}

impl McpAuthRunnerPort for ContractAuthRunner {
    fn authenticate(&self, package_spec: &str, url: &str) -> Result<(), ImruleError> {
        self.calls
            .lock()
            .unwrap()
            .push((package_spec.to_string(), url.to_string()));
        Ok(())
    }
}

#[test]
fn mcp_auth_reuses_cache_and_runs_only_eligible_servers_sequentially() {
    let temporary = tempdir().unwrap();
    let project_root = temporary.path();
    fs::create_dir_all(project_root.join(".imrule")).unwrap();
    fs::write(
        project_root.join(".imrule/imrule.toml"),
        r#"
[mcp]
remote_transport = "mcp-remote"

[mcp_servers.eligible_a]
transport = "http"
url = "https://a.example.test/mcp"

[mcp_servers.eligible_b]
transport = "sse"
url = "https://b.example.test/sse"

[mcp_servers.headered]
transport = "http"
url = "https://headered.example.test/mcp"
headers = { Authorization = "Bearer contract-secret" }

[mcp_servers.placeholder]
transport = "http"
url = "https://${MISSING_HOST}/mcp"

[mcp_servers.stdio]
transport = "stdio"
command = "server-command"
"#,
    )
    .unwrap();

    let loader = TomlConfigLoader::new();
    let mcp_storage = JsonMcpStorage::new();
    let fs_port = FsFileSystem::new();
    let cache = JsonVersionCache::new();
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = ContractAuthRunner::default();
    let use_case = McpUseCase::new(&loader, &loader);
    let options = McpAuthOptions {
        project_root: project_root.to_path_buf(),
        config_path: None,
    };

    let first = use_case
        .auth(
            options.clone(),
            &mcp_storage,
            &fs_port,
            &cache,
            &resolver,
            &runner,
        )
        .unwrap();
    let second = use_case
        .auth(options, &mcp_storage, &fs_port, &cache, &resolver, &runner)
        .unwrap();

    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.authenticated, vec!["eligible_a", "eligible_b"]);
    assert_eq!(second.authenticated, first.authenticated);
    assert_eq!(first.skipped.len(), 3);
    assert_eq!(
        runner.calls.lock().unwrap().as_slice(),
        [
            (
                "mcp-remote@0.1.37".to_string(),
                "https://a.example.test/mcp".to_string()
            ),
            (
                "mcp-remote@0.1.37".to_string(),
                "https://b.example.test/sse".to_string()
            ),
            (
                "mcp-remote@0.1.37".to_string(),
                "https://a.example.test/mcp".to_string()
            ),
            (
                "mcp-remote@0.1.37".to_string(),
                "https://b.example.test/sse".to_string()
            ),
        ]
    );

    let cached: Value =
        serde_json::from_str(&fs::read_to_string(project_root.join(IMRULE_CACHE_PATH)).unwrap())
            .unwrap();
    assert_eq!(
        cached.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["package", "resolved_at", "resolved_version"]
    );
    assert!(!cached.to_string().contains("contract-secret"));
    assert!(!cached.to_string().contains("example.test"));
}
