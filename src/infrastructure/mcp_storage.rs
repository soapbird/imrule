//! Native MCP config path and JSON IO implementing `McpPort`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::application::ports::McpPort;
use crate::domain::constants::LEGACY_DIR_NAME;
use crate::domain::error::ImruleError;
use crate::infrastructure::mcp_storage_toml::{is_toml_mcp_path, read_toml_mcp, write_toml_mcp};

pub struct JsonMcpStorage;

impl JsonMcpStorage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonMcpStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl McpPort for JsonMcpStorage {
    fn read_imrule_mcp_config(&self, project_root: &Path) -> Result<Option<Value>, ImruleError> {
        let path = project_root.join(".imrule/mcp.json");
        if path.exists() {
            let text = fs::read_to_string(&path).map_err(|e| {
                ImruleError::mcp(format!(
                    "could not read MCP config at {}: {e}",
                    path.display()
                ))
            })?;
            let parsed = serde_json::from_str(&text).map_err(|e| {
                ImruleError::mcp(format!(
                    "could not parse MCP config at {}: {e}",
                    path.display()
                ))
            })?;
            return Ok(Some(parsed));
        }
        let legacy_path = project_root.join(format!("{LEGACY_DIR_NAME}/mcp.json"));
        if legacy_path.exists() {
            let text = fs::read_to_string(&legacy_path).map_err(|e| {
                ImruleError::mcp(format!(
                    "could not read MCP config at {}: {e}",
                    legacy_path.display()
                ))
            })?;
            let parsed = serde_json::from_str(&text).map_err(|e| {
                ImruleError::mcp(format!(
                    "could not parse MCP config at {}: {e}",
                    legacy_path.display()
                ))
            })?;
            return Ok(Some(parsed));
        }
        Ok(None)
    }

    fn read_native_mcp(&self, file_path: &Path) -> Result<Value, ImruleError> {
        if is_toml_mcp_path(file_path) {
            return read_toml_mcp(file_path);
        }
        match fs::read_to_string(file_path) {
            Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({}))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
            Err(err) => Err(ImruleError::mcp(err.to_string())),
        }
    }

    fn write_native_mcp(&self, file_path: &Path, data: &Value) -> Result<(), ImruleError> {
        if is_toml_mcp_path(file_path) {
            return write_toml_mcp(file_path, data);
        }
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ImruleError::mcp(e.to_string()))?;
        }
        let text = serde_json::to_string_pretty(data).expect("serializable JSON value") + "\n";
        fs::write(file_path, text).map_err(|e| ImruleError::mcp(e.to_string()))
    }

    fn get_native_mcp_path(&self, adapter_name: &str, project_root: &Path) -> Option<PathBuf> {
        let candidates: Vec<PathBuf> = match adapter_name {
            "GitHub Copilot" => vec![project_root.join(".vscode/mcp.json")],
            "Visual Studio" => vec![
                project_root.join(".mcp.json"),
                project_root.join(".vs/mcp.json"),
            ],
            "Cursor" => vec![project_root.join(".cursor/mcp.json")],
            "Windsurf" => vec![project_root.join(".windsurf/mcp_config.json")],
            "Claude Code" => vec![project_root.join(".mcp.json")],
            "OpenAI Codex CLI" => vec![project_root.join(".codex/config.toml")],
            "Aider" => vec![project_root.join(".mcp.json")],
            "Open Hands" => vec![project_root.join("config.toml")],
            "Gemini CLI" => vec![project_root.join(".gemini/settings.json")],
            "Junie" => vec![project_root.join(".junie/mcp/mcp.json")],
            "Qwen Code" => vec![project_root.join(".qwen/settings.json")],
            "Kilo Code" => vec![project_root.join(".kilocode/mcp.json")],
            "Kiro" => vec![project_root.join(".kiro/settings/mcp.json")],
            "OpenCode" => vec![project_root.join("opencode.json")],
            "Firebase Studio" => vec![project_root.join(".idx/mcp.json")],
            "Factory Droid" => vec![project_root.join(".factory/mcp.json")],
            "Zed" => vec![project_root.join(".zed/settings.json")],
            "Mistral" => vec![project_root.join(".vibe/config.toml")],
            "Gajae Code" => vec![project_root.join(".gjc/mcp.json")],
            _ => return None,
        };

        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
        candidates.into_iter().next()
    }
}
