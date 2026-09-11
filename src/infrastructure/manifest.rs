//! Project-scoped apply manifest implementing `ManifestPort`.

use std::fs;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;

use crate::application::ports::ManifestPort;
use crate::domain::constants::IMRULE_MANIFEST_PATH;
use crate::domain::error::ImruleError;
use crate::domain::manifest::ApplyManifest;

/// Stores the previous apply's outputs in `.imrule/manifest.json`.
pub struct JsonApplyManifest;

impl JsonApplyManifest {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonApplyManifest {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestPort for JsonApplyManifest {
    fn read_manifest(&self, project_root: &Path) -> Result<Option<ApplyManifest>, ImruleError> {
        let path = project_root.join(IMRULE_MANIFEST_PATH);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ImruleError::filesystem(format!(
                    "could not read apply manifest at {}: {error}",
                    path.display()
                )));
            }
        };
        // A manifest that cannot be parsed, or that a newer imrule wrote, is
        // treated as absent. Cleanup then skips this run instead of failing an
        // apply that is otherwise entirely valid.
        match serde_json::from_slice::<ApplyManifest>(&bytes) {
            Ok(manifest) if manifest.is_readable() => {
                let (manifest, dropped) = manifest.without_escaping_entries();
                if dropped > 0 {
                    tracing::warn!(
                        dropped,
                        "apply manifest names paths outside the project; ignoring those entries"
                    );
                }
                Ok(Some(manifest))
            }
            Ok(manifest) => {
                tracing::warn!(
                    version = manifest.version,
                    "apply manifest has an unsupported version; ignoring it"
                );
                Ok(None)
            }
            Err(error) => {
                tracing::warn!(%error, "apply manifest is unreadable; ignoring it");
                Ok(None)
            }
        }
    }

    fn write_manifest(
        &self,
        project_root: &Path,
        manifest: &ApplyManifest,
    ) -> Result<(), ImruleError> {
        let path = project_root.join(IMRULE_MANIFEST_PATH);
        let parent = path.parent().ok_or_else(|| {
            ImruleError::filesystem(format!("manifest path has no parent: {}", path.display()))
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            ImruleError::filesystem(format!(
                "could not create manifest directory at {}: {error}",
                parent.display()
            ))
        })?;

        let mut temporary = NamedTempFile::new_in(parent).map_err(|error| {
            ImruleError::filesystem(format!(
                "could not create temporary manifest in {}: {error}",
                parent.display()
            ))
        })?;
        serde_json::to_writer_pretty(&mut temporary, manifest).map_err(|error| {
            ImruleError::filesystem(format!("could not serialize apply manifest: {error}"))
        })?;
        temporary.write_all(b"\n").map_err(|error| {
            ImruleError::filesystem(format!("could not write temporary manifest: {error}"))
        })?;
        temporary.as_file().sync_all().map_err(|error| {
            ImruleError::filesystem(format!("could not sync temporary manifest: {error}"))
        })?;
        temporary.persist(&path).map_err(|error| {
            ImruleError::filesystem(format!(
                "could not atomically replace apply manifest at {}: {}",
                path.display(),
                error.error
            ))
        })?;
        Ok(())
    }

    fn remove_manifest(&self, project_root: &Path) -> Result<(), ImruleError> {
        let path = project_root.join(IMRULE_MANIFEST_PATH);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ImruleError::filesystem(format!(
                "could not remove apply manifest at {}: {error}",
                path.display()
            ))),
        }
    }
}
