//! Native subagent discovery and transformation helpers.

use serde_json::{Map, Value};

use crate::domain::config::SubagentFrontmatter;

/// Parsed YAML frontmatter and markdown body.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFrontmatter {
    pub meta: Value,
    pub body: String,
}

/// Copilot tool mapping result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotToolMapping {
    pub tools: Vec<String>,
    pub unknown: Vec<String>,
}

/// Copilot file transform result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotFile {
    pub content: String,
    pub warnings: Vec<String>,
}

/// Subagent discovery result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentsDiscovery {
    pub subagents: Vec<crate::domain::config::SubagentInfo>,
    pub warnings: Vec<String>,
}

/// Parses leading YAML frontmatter.
pub fn parse_frontmatter(content: &str) -> Result<Option<ParsedFrontmatter>, String> {
    let Some(rest) = content.strip_prefix("---") else {
        return Ok(None);
    };
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .ok_or_else(|| "missing newline after opening frontmatter".to_string())?;
    let Some((raw, body)) = split_frontmatter(rest) else {
        return Ok(None);
    };
    let meta = serde_norway::from_str::<Value>(raw).map_err(|err| err.to_string())?;
    Ok(Some(ParsedFrontmatter {
        meta: if meta.is_object() {
            meta
        } else {
            Value::Object(Map::new())
        },
        body: body.to_string(),
    }))
}

fn split_frontmatter(rest: &str) -> Option<(&str, &str)> {
    for delimiter in ["\n---\n", "\r\n---\r\n", "\n---\r\n", "\r\n---\n"] {
        if let Some(index) = rest.find(delimiter) {
            let raw = &rest[..index];
            let body = &rest[index + delimiter.len()..];
            return Some((raw, body));
        }
    }
    if let Some(raw) = rest.strip_suffix("\n---") {
        return Some((raw, ""));
    }
    None
}

/// Validates parsed frontmatter for a source subagent file.
pub fn validate_frontmatter(
    meta: &Value,
    expected_name: &str,
) -> Result<SubagentFrontmatter, String> {
    let object = meta
        .as_object()
        .ok_or_else(|| "missing or invalid required field \"name\"".to_string())?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing or invalid required field \"name\"".to_string())?;
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing or invalid required field \"description\"".to_string())?;
    if name != expected_name {
        return Err(format!(
            "frontmatter name \"{name}\" does not match filename stem \"{expected_name}\""
        ));
    }

    let tools = match object.get("tools") {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|item| {
                    item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                        "invalid \"tools\" field; expected string or string[]".to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(Value::String(value)) => Some(
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        ),
        Some(_) => return Err("invalid \"tools\" field; expected string or string[]".to_string()),
        None => None,
    };
    let model = optional_string(object, "model")?;
    let readonly = optional_bool(object, "readonly")?;
    let is_background = optional_bool(object, "is_background")?;

    Ok(SubagentFrontmatter {
        name: name.to_string(),
        description: description.to_string(),
        tools,
        model,
        readonly,
        is_background,
    })
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("invalid \"{key}\" field; expected string")),
        None => Ok(None),
    }
}

fn optional_bool(object: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match object.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("invalid \"{key}\" field; expected boolean")),
        None => Ok(None),
    }
}

/// Maps Claude tool names to Copilot aliases.
pub fn map_tools_for_copilot(source_tools: &[String]) -> CopilotToolMapping {
    let mut tools = Vec::<String>::new();
    let mut unknown = Vec::new();
    for tool in source_tools {
        let alias = match tool.as_str() {
            "Read" => Some("read"),
            "Grep" | "Glob" => Some("search"),
            "Bash" => Some("execute"),
            "Edit" | "Write" => Some("edit"),
            "WebFetch" | "WebSearch" => Some("web"),
            "TodoWrite" => Some("todo"),
            "Task" => Some("agent"),
            _ => None,
        };
        if let Some(alias) = alias {
            if !tools.iter().any(|existing| existing == alias) {
                tools.push(alias.to_string());
            }
        } else {
            unknown.push(tool.clone());
        }
    }
    CopilotToolMapping { tools, unknown }
}

pub fn build_claude_file(sub: &crate::domain::config::SubagentInfo) -> String {
    let fm = sub
        .frontmatter
        .as_ref()
        .expect("valid subagent frontmatter");
    let mut lines = vec![
        ("name".to_string(), YamlValue::String(fm.name.clone())),
        (
            "description".to_string(),
            YamlValue::String(fm.description.clone()),
        ),
    ];
    if let Some(tools) = &fm.tools {
        lines.push(("tools".into(), YamlValue::Array(tools.clone())));
    }
    if let Some(model) = &fm.model {
        lines.push(("model".into(), YamlValue::String(model.clone())));
    }
    if let Some(readonly) = fm.readonly {
        lines.push(("readonly".into(), YamlValue::Bool(readonly)));
    }
    if let Some(is_background) = fm.is_background {
        lines.push(("is_background".into(), YamlValue::Bool(is_background)));
    }
    format!(
        "{}\n{}",
        build_frontmatter_block(&lines),
        ensure_body_formatting(sub.body.as_deref())
    )
}

pub fn build_cursor_file(sub: &crate::domain::config::SubagentInfo) -> String {
    let fm = sub
        .frontmatter
        .as_ref()
        .expect("valid subagent frontmatter");
    let lines = vec![
        ("name".into(), YamlValue::String(fm.name.clone())),
        (
            "description".into(),
            YamlValue::String(fm.description.clone()),
        ),
        (
            "model".into(),
            YamlValue::String(fm.model.clone().unwrap_or_else(|| "inherit".into())),
        ),
        (
            "readonly".into(),
            YamlValue::Bool(fm.readonly.unwrap_or(false)),
        ),
        (
            "is_background".into(),
            YamlValue::Bool(fm.is_background.unwrap_or(false)),
        ),
    ];
    format!(
        "{}\n{}",
        build_frontmatter_block(&lines),
        ensure_body_formatting(sub.body.as_deref())
    )
}

pub fn build_codex_file(sub: &crate::domain::config::SubagentInfo) -> String {
    let fm = sub
        .frontmatter
        .as_ref()
        .expect("valid subagent frontmatter");
    let mut out = format!(
        "name = \"{}\"\ndescription = \"{}\"\ndeveloper_instructions = \"{}\"\n",
        escape_toml(&fm.name),
        escape_toml(&fm.description),
        escape_toml(&ensure_body_formatting(sub.body.as_deref()))
    );
    if let Some(model) = &fm.model {
        if model != "inherit" {
            out.push_str(&format!("model = \"{}\"\n", escape_toml(model)));
        }
    }
    if fm.readonly == Some(true) {
        out.push_str("sandbox_mode = \"read-only\"\n");
    }
    out
}

pub fn build_copilot_file(sub: &crate::domain::config::SubagentInfo) -> CopilotFile {
    let fm = sub
        .frontmatter
        .as_ref()
        .expect("valid subagent frontmatter");
    let mut lines = vec![
        ("name".into(), YamlValue::String(fm.name.clone())),
        (
            "description".into(),
            YamlValue::String(fm.description.clone()),
        ),
        ("user-invocable".into(), YamlValue::Bool(true)),
    ];
    let mut warnings = Vec::new();
    if let Some(source_tools) = &fm.tools {
        let mapped = map_tools_for_copilot(source_tools);
        if !mapped.tools.is_empty() {
            lines.push(("tools".into(), YamlValue::Array(mapped.tools)));
        }
        if !mapped.unknown.is_empty() {
            warnings.push(format!(
                "Subagent \"{}\": dropping tools not mappable to Copilot aliases: {}",
                fm.name,
                mapped.unknown.join(", ")
            ));
        }
    }
    if let Some(model) = &fm.model {
        if model != "inherit" {
            lines.push(("model".into(), YamlValue::String(model.clone())));
        }
    }
    if fm.readonly == Some(true) {
        lines.push(("disable-model-invocation".into(), YamlValue::Bool(true)));
    }
    CopilotFile {
        content: format!(
            "{}\n{}",
            build_frontmatter_block(&lines),
            ensure_body_formatting(sub.body.as_deref())
        ),
        warnings,
    }
}

enum YamlValue {
    String(String),
    Bool(bool),
    Array(Vec<String>),
}

fn build_frontmatter_block(lines: &[(String, YamlValue)]) -> String {
    let mut out = String::from("---\n");
    for (key, value) in lines {
        match value {
            YamlValue::String(value) => out.push_str(&format!("{key}: {value}\n")),
            YamlValue::Bool(value) => out.push_str(&format!("{key}: {value}\n")),
            YamlValue::Array(values) => {
                out.push_str(&format!("{key}:\n"));
                for value in values {
                    out.push_str(&format!("- {value}\n"));
                }
            }
        }
    }
    out.push_str("---\n");
    out
}

fn ensure_body_formatting(body: Option<&str>) -> String {
    let text = body.unwrap_or_default().trim_start_matches('\n');
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

fn escape_toml(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
