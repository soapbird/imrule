//! Project-scoped JSON version cache implementing `CachePort`.

use std::fs;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;

use crate::application::ports::CachePort;
use crate::domain::constants::IMRULE_CACHE_PATH;
use crate::domain::error::ImruleError;
use crate::domain::mcp::McpRemoteVersionCache;

/// Stores the resolved `mcp-remote` version in `.imrule/cache.json`.
pub struct JsonVersionCache;

impl JsonVersionCache {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonVersionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CachePort for JsonVersionCache {
    fn read_mcp_remote_version(
        &self,
        project_root: &Path,
    ) -> Result<Option<McpRemoteVersionCache>, ImruleError> {
        let cache_path = project_root.join(IMRULE_CACHE_PATH);
        let bytes = match fs::read(&cache_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ImruleError::mcp(format!(
                    "could not read version cache at {}: {error}",
                    cache_path.display()
                )))
            }
        };
        let cache: McpRemoteVersionCache = serde_json::from_slice(&bytes).map_err(|error| {
            ImruleError::mcp(format!(
                "could not parse version cache at {}: {error}",
                cache_path.display()
            ))
        })?;
        cache.validate()?;
        Ok(Some(cache))
    }

    fn write_mcp_remote_version_atomic(
        &self,
        project_root: &Path,
        cache: &McpRemoteVersionCache,
    ) -> Result<(), ImruleError> {
        cache.validate()?;
        let cache_path = project_root.join(IMRULE_CACHE_PATH);
        let parent = cache_path.parent().ok_or_else(|| {
            ImruleError::mcp(format!(
                "version cache path has no parent: {}",
                cache_path.display()
            ))
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            ImruleError::mcp(format!(
                "could not create version cache directory at {}: {error}",
                parent.display()
            ))
        })?;

        let mut temporary = NamedTempFile::new_in(parent).map_err(|error| {
            ImruleError::mcp(format!(
                "could not create temporary version cache in {}: {error}",
                parent.display()
            ))
        })?;
        serde_json::to_writer_pretty(&mut temporary, cache).map_err(|error| {
            ImruleError::mcp(format!("could not serialize version cache: {error}"))
        })?;
        temporary.write_all(b"\n").map_err(|error| {
            ImruleError::mcp(format!("could not write temporary version cache: {error}"))
        })?;
        temporary.as_file().sync_all().map_err(|error| {
            ImruleError::mcp(format!("could not sync temporary version cache: {error}"))
        })?;
        temporary.persist(&cache_path).map_err(|error| {
            ImruleError::mcp(format!(
                "could not atomically replace version cache at {}: {}",
                cache_path.display(),
                error.error
            ))
        })?;
        Ok(())
    }
}
