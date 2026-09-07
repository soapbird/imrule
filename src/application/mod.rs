//! Application use cases for the native ImRule CLI.

use std::collections::BTreeMap;
use std::path::Path;

use crate::application::ports::FileSystemPort;
use crate::domain::error::ImruleError;

pub mod apply_use_case;
pub mod clear_use_case;
pub mod init_use_case;
pub mod mcp_use_case;
pub mod ports;
pub mod skills_add_use_case;
pub mod skills_update_use_case;

pub use apply_use_case::ApplyResult;
pub use apply_use_case::{ApplyOptions, ApplyUseCase};
pub use clear_use_case::{ClearOptions, ClearUseCase};
pub use init_use_case::{InitOptions, InitUseCase};
pub use mcp_use_case::{McpAddOptions, McpRemoveOptions, McpUseCase};
pub use skills_update_use_case::{SkillsUpdateOptions, SkillsUpdateUseCase};

/// Loads environment variables from `.env` and `.imrule/.env` files,
/// then overlays the process environment. Used by both apply and mcp auth.
pub fn load_mcp_environment(
    fs_port: &dyn FileSystemPort,
    project_root: &Path,
) -> Result<BTreeMap<String, String>, ImruleError> {
    let mut variables = BTreeMap::new();
    for path in [
        project_root.join(".env"),
        project_root.join(".imrule").join(".env"),
    ] {
        if !fs_port.file_exists(&path) {
            continue;
        }
        for entry in dotenvy::from_path_iter(&path).map_err(|error| {
            ImruleError::config(format!("failed to read {}: {error}", path.display()))
        })? {
            let (key, value) = entry.map_err(|error| {
                ImruleError::config(format!("failed to parse {}: {error}", path.display()))
            })?;
            variables.insert(key, value);
        }
    }
    variables.extend(std::env::vars());
    Ok(variables)
}
