use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use imrule::application::mcp_use_case::{
    resolve_mcp_remote_version, McpAuthOptions, McpAuthRunnerPort, McpRemoteVersionResolverPort,
    McpUseCase,
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
        &McpRemoteTransport::McpRemote.into(),
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

// --- Gap coverage for resolve_mcp_remote_version() ---

#[test]
fn resolve_mcp_remote_version_persists_cache_when_requested() {
    let temporary = tempdir().unwrap();
    let project_root = temporary.path();
    let cache = JsonVersionCache::new();
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };

    let resolved = resolve_mcp_remote_version(&cache, &resolver, project_root, true).unwrap();
    assert_eq!(resolved.package_spec(), "mcp-remote@0.1.37");
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    // persist=true wrote the cache to disk.
    assert!(project_root.join(IMRULE_CACHE_PATH).exists());
}

#[test]
fn resolve_mcp_remote_version_skips_persistence_in_dry_run() {
    let temporary = tempdir().unwrap();
    let project_root = temporary.path();
    let cache = JsonVersionCache::new();
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };

    let resolved = resolve_mcp_remote_version(&cache, &resolver, project_root, false).unwrap();
    assert_eq!(resolved.package_spec(), "mcp-remote@0.1.37");
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    // persist=false: resolver ran but nothing was written to disk.
    assert!(!project_root.join(IMRULE_CACHE_PATH).exists());
}

#[test]
fn resolve_mcp_remote_version_does_not_call_resolver_when_cache_exists() {
    let temporary = tempdir().unwrap();
    let project_root = temporary.path();
    let cache = JsonVersionCache::new();
    // Seed the cache first.
    cache
        .write_mcp_remote_version_atomic(
            project_root,
            &McpRemoteVersionCache::new("0.1.36", 1_700_000_000).unwrap(),
        )
        .unwrap();

    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let resolved = resolve_mcp_remote_version(&cache, &resolver, project_root, true).unwrap();
    // Cache hit: resolver must NOT run and the cached version wins.
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert_eq!(resolved.resolved_version(), "0.1.36");
}

#[test]
fn resolve_mcp_remote_version_propagates_resolver_failure_without_writing_cache() {
    let temporary = tempdir().unwrap();
    let project_root = temporary.path();
    let cache = JsonVersionCache::new();

    struct FailingResolver;
    impl McpRemoteVersionResolverPort for FailingResolver {
        fn resolve_latest_version(&self) -> Result<String, ImruleError> {
            Err(ImruleError::mcp("registry unreachable"))
        }
    }

    let error =
        resolve_mcp_remote_version(&cache, &FailingResolver, project_root, true).unwrap_err();
    assert!(error.to_string().contains("registry unreachable"));
    // A failed resolution must not leave a partial cache file behind.
    assert!(!project_root.join(IMRULE_CACHE_PATH).exists());
}

// --- Gap coverage for McpUseCase::auth() skip/error branches ---

#[derive(Default)]
struct RecordingAuthRunner {
    calls: Mutex<Vec<(String, String)>>,
}

impl McpAuthRunnerPort for RecordingAuthRunner {
    fn authenticate(&self, package_spec: &str, url: &str) -> Result<(), ImruleError> {
        self.calls
            .lock()
            .unwrap()
            .push((package_spec.to_string(), url.to_string()));
        Ok(())
    }
}

struct FailingAuthRunner;

impl McpAuthRunnerPort for FailingAuthRunner {
    fn authenticate(&self, _package_spec: &str, _url: &str) -> Result<(), ImruleError> {
        Err(ImruleError::mcp("oauth handshake rejected"))
    }
}

fn auth_project_with(toml: &str) -> tempfile::TempDir {
    let temporary = tempdir().unwrap();
    fs::create_dir_all(temporary.path().join(".imrule")).unwrap();
    fs::write(temporary.path().join(".imrule/imrule.toml"), toml).unwrap();
    temporary
}

fn run_auth(
    project_root: &std::path::Path,
    resolver: &dyn McpRemoteVersionResolverPort,
    runner: &dyn McpAuthRunnerPort,
) -> imrule::application::mcp_use_case::McpAuthResult {
    let loader = TomlConfigLoader::new();
    let mcp_storage = JsonMcpStorage::new();
    let fs_port = FsFileSystem::new();
    let cache = JsonVersionCache::new();
    let use_case = McpUseCase::new(&loader, &loader);
    use_case
        .auth(
            McpAuthOptions {
                project_root: project_root.to_path_buf(),
                config_path: None,
            },
            &mcp_storage,
            &fs_port,
            &cache,
            resolver,
            runner,
        )
        .unwrap()
}

#[test]
fn mcp_auth_returns_empty_when_mcp_section_disabled() {
    let temporary = auth_project_with(
        r#"
[mcp]
enabled = false

[mcp_servers.remote]
transport = "http"
url = "https://example.test/mcp"
"#,
    );
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = RecordingAuthRunner::default();
    let result = run_auth(temporary.path(), &resolver, &runner);
    // Disabled MCP: nothing resolved, nothing authenticated.
    assert!(result.authenticated.is_empty());
    assert!(result.skipped.is_empty());
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn mcp_auth_skips_every_remote_when_transport_is_native() {
    let temporary = auth_project_with(
        r#"
[mcp]
remote_transport = "native"

[mcp_servers.remote]
transport = "http"
url = "https://example.test/mcp"
"#,
    );
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = RecordingAuthRunner::default();
    let result = run_auth(temporary.path(), &resolver, &runner);
    // Native transport: every remote server is skipped, no resolution, no runner.
    assert!(result.authenticated.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].server_name, "remote");
    assert_eq!(
        result.skipped[0].reason,
        "remote transport is configured as native"
    );
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn mcp_auth_follows_each_servers_remote_transport_override() {
    let temporary = auth_project_with(
        r#"
[mcp]
remote_transport = "mcp-remote"

[mcp_servers.bridged]
transport = "http"
url = "https://bridged.example.test/mcp"

[mcp_servers.native]
transport = "http"
url = "https://native.example.test/mcp"
remote_transport = "native"
"#,
    );
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = RecordingAuthRunner::default();
    let result = run_auth(temporary.path(), &resolver, &runner);
    assert_eq!(result.authenticated, vec!["bridged"]);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].server_name, "native");
    assert_eq!(
        result.skipped[0].reason,
        "remote transport is configured as native"
    );

    // A native project can still send one server through the bridge.
    let temporary = auth_project_with(
        r#"
[mcp]
remote_transport = "native"

[mcp_servers.bridged]
transport = "http"
url = "https://bridged.example.test/mcp"
remote_transport = "mcp-remote"
"#,
    );
    let runner = RecordingAuthRunner::default();
    let result = run_auth(temporary.path(), &resolver, &runner);
    assert_eq!(result.authenticated, vec!["bridged"]);
    assert!(result.skipped.is_empty());
}

#[test]
fn mcp_auth_skips_resolution_when_only_stdio_servers_exist() {
    let temporary = auth_project_with(
        r#"
[mcp]
remote_transport = "mcp-remote"

[mcp_servers.local]
transport = "stdio"
command = "node"
"#,
    );
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = RecordingAuthRunner::default();
    let result = run_auth(temporary.path(), &resolver, &runner);
    // No eligible remote servers: resolver must not be called at all.
    assert!(result.authenticated.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(
        result.skipped[0].reason,
        "stdio servers do not use remote OAuth"
    );
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(runner.calls.lock().unwrap().is_empty());
    // No cache file written because the eligible list was empty.
    assert!(!temporary.path().join(IMRULE_CACHE_PATH).exists());
}

#[test]
fn mcp_auth_propagates_runner_failure_on_first_eligible_server() {
    let temporary = auth_project_with(
        r#"
[mcp]
remote_transport = "mcp-remote"

[mcp_servers.first]
transport = "http"
url = "https://first.example.test/mcp"

[mcp_servers.second]
transport = "sse"
url = "https://second.example.test/sse"
"#,
    );
    let loader = TomlConfigLoader::new();
    let mcp_storage = JsonMcpStorage::new();
    let fs_port = FsFileSystem::new();
    let cache = JsonVersionCache::new();
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let use_case = McpUseCase::new(&loader, &loader);

    let error = use_case
        .auth(
            McpAuthOptions {
                project_root: temporary.path().to_path_buf(),
                config_path: None,
            },
            &mcp_storage,
            &fs_port,
            &cache,
            &resolver,
            &FailingAuthRunner,
        )
        .unwrap_err();
    // The first authentication fails and aborts the sequential run.
    assert!(error
        .to_string()
        .contains("authentication failed for MCP server 'first'"));
}

#[test]
fn mcp_auth_skips_non_http_remote_types() {
    // The "server is not an HTTP/SSE remote" skip branch can only be reached
    // from a raw .imrule/mcp.json entry: TOML [mcp_servers] normalizes `type`
    // via mcp_server_definition_to_json, so a websocket type survives only when
    // the user authors the JSON directly.
    let temporary = tempdir().unwrap();
    fs::create_dir_all(temporary.path().join(".imrule")).unwrap();
    fs::write(
        temporary.path().join(".imrule/imrule.toml"),
        r#"
[mcp]
remote_transport = "mcp-remote"
"#,
    )
    .unwrap();
    fs::write(
        temporary.path().join(".imrule/mcp.json"),
        r#"{"mcpServers":{"weird":{"type":"websocket","url":"wss://example.test/mcp"}}}"#,
    )
    .unwrap();

    let loader = TomlConfigLoader::new();
    let mcp_storage = JsonMcpStorage::new();
    let fs_port = FsFileSystem::new();
    let cache = JsonVersionCache::new();
    let resolver = ContractVersionResolver {
        calls: AtomicUsize::new(0),
    };
    let runner = RecordingAuthRunner::default();
    let use_case = McpUseCase::new(&loader, &loader);
    let result = use_case
        .auth(
            McpAuthOptions {
                project_root: temporary.path().to_path_buf(),
                config_path: None,
            },
            &mcp_storage,
            &fs_port,
            &cache,
            &resolver,
            &runner,
        )
        .unwrap();
    assert!(result.authenticated.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].reason, "server is not an HTTP/SSE remote");
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(runner.calls.lock().unwrap().is_empty());
}
