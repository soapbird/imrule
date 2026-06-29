use std::fs;
use std::path::Path;

use serde_json::{json, Map, Value};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::domain::error::ImruleError;
use crate::infrastructure::mcp_storage_openhands_toml::{read_openhands_mcp, write_openhands_mcp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TomlMcpFormat {
    Codex,
    Mistral,
    OpenHands,
}

pub fn is_toml_mcp_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("toml")
}

pub fn read_toml_mcp(file_path: &Path) -> Result<Value, ImruleError> {
    let text = match fs::read_to_string(file_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(err) => return Err(ImruleError::mcp(err.to_string())),
    };
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|err| ImruleError::mcp(err.to_string()))?;
    match format_for_path(file_path) {
        TomlMcpFormat::Codex => Ok(read_codex_mcp(&doc)),
        TomlMcpFormat::Mistral => Ok(read_mistral_mcp(&doc)),
        TomlMcpFormat::OpenHands => Ok(read_openhands_mcp(&doc)),
    }
}

pub fn write_toml_mcp(file_path: &Path, data: &Value) -> Result<(), ImruleError> {
    let text = match fs::read_to_string(file_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(ImruleError::mcp(err.to_string())),
    };
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|err| ImruleError::mcp(err.to_string()))?;

    match format_for_path(file_path) {
        TomlMcpFormat::Codex => write_codex_mcp(&mut doc, data),
        TomlMcpFormat::Mistral => write_mistral_mcp(&mut doc, data),
        TomlMcpFormat::OpenHands => write_openhands_mcp(&mut doc, data),
    }

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|err| ImruleError::mcp(err.to_string()))?;
    }
    fs::write(file_path, doc.to_string()).map_err(|err| ImruleError::mcp(err.to_string()))
}

fn format_for_path(path: &Path) -> TomlMcpFormat {
    let text = path.to_string_lossy();
    if text.ends_with(".vibe/config.toml") {
        TomlMcpFormat::Mistral
    } else if text.ends_with(".codex/config.toml") {
        TomlMcpFormat::Codex
    } else {
        TomlMcpFormat::OpenHands
    }
}

fn read_codex_mcp(doc: &DocumentMut) -> Value {
    let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table) else {
        return json!({});
    };
    let mut server_map = Map::new();
    for (name, item) in servers {
        if let Some(table) = item.as_table() {
            server_map.insert(name.to_string(), toml_table_to_json(table));
        }
    }
    json!({ "mcp_servers": server_map })
}

fn write_codex_mcp(doc: &mut DocumentMut, data: &Value) {
    let mut servers_table = Table::new();
    for (server_name, server_config) in extract_server_object(data) {
        let Some(server_config) = server_config.as_object() else {
            continue;
        };
        let mut server_table = Table::new();
        for (key, value) in server_config {
            match key.as_str() {
                "type" => {}
                "headers" => insert_json_value(&mut server_table, "http_headers", value),
                _ => insert_json_value(&mut server_table, key, value),
            }
        }
        servers_table.insert(&server_name, Item::Table(server_table));
    }
    doc.insert("mcp_servers", Item::Table(servers_table));
}

fn read_mistral_mcp(doc: &DocumentMut) -> Value {
    let Some(servers) = doc.get("mcp_servers").and_then(Item::as_array_of_tables) else {
        return json!({});
    };
    let mut server_map = Map::new();
    for table in servers {
        let name = table
            .get("name")
            .and_then(Item::as_value)
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if let Some(name) = name {
            server_map.insert(name, mistral_table_to_json(table));
        }
    }
    json!({ "mcp_servers": server_map })
}

fn write_mistral_mcp(doc: &mut DocumentMut, data: &Value) {
    let mut servers = ArrayOfTables::new();
    for (server_name, server_config) in extract_server_object(data) {
        let Some(server_config) = server_config.as_object() else {
            continue;
        };
        let mut table = Table::new();
        table.insert("name", toml_edit::value(&server_name));
        table.insert(
            "transport",
            toml_edit::value(mistral_transport(server_config)),
        );
        for (key, value) in server_config {
            if key != "type" {
                insert_json_value(&mut table, key, value);
            }
        }
        servers.push(table);
    }
    doc.insert("mcp_servers", Item::ArrayOfTables(servers));
}

fn extract_server_object(data: &Value) -> Map<String, Value> {
    data.get("mcp_servers")
        .or_else(|| data.get("mcpServers"))
        .or_else(|| data.get("mcp"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn mistral_table_to_json(table: &Table) -> Value {
    let mut value = toml_table_to_json(table);
    if let Some(object) = value.as_object_mut() {
        object.remove("name");
        if let Some(transport) = object.remove("transport") {
            object.insert("type".to_string(), transport);
        }
    }
    value
}

fn mistral_transport(server_config: &Map<String, Value>) -> &str {
    server_config
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if server_config.contains_key("command") {
                "stdio"
            } else {
                "http"
            }
        })
}

fn toml_table_to_json(table: &Table) -> Value {
    let mut object = Map::new();
    for (key, item) in table {
        if let Some(value) = toml_item_to_json(item) {
            object.insert(key.to_string(), value);
        }
    }
    Value::Object(object)
}

fn toml_item_to_json(item: &Item) -> Option<Value> {
    match item {
        Item::Value(value) => Some(toml_value_to_json(value).unwrap_or(Value::Null)),
        Item::Table(table) => Some(toml_table_to_json(table)),
        Item::ArrayOfTables(_) | Item::None => None,
    }
}

fn toml_value_to_json(value: &toml_edit::Value) -> Result<Value, ()> {
    Ok(match value {
        toml_edit::Value::String(value) => Value::String(value.value().to_string()),
        toml_edit::Value::Integer(value) => Value::Number((*value.value()).into()),
        toml_edit::Value::Float(value) => serde_json::Number::from_f64(*value.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml_edit::Value::Boolean(value) => Value::Bool(*value.value()),
        toml_edit::Value::Array(array) => Value::Array(
            array
                .iter()
                .filter_map(|value| toml_value_to_json(value).ok())
                .collect(),
        ),
        toml_edit::Value::InlineTable(table) => {
            let mut object = Map::new();
            for (key, value) in table {
                if let Ok(json_value) = toml_value_to_json(value) {
                    object.insert(key.to_string(), json_value);
                }
            }
            Value::Object(object)
        }
        toml_edit::Value::Datetime(value) => Value::String(value.to_string()),
    })
}

fn insert_json_value(table: &mut Table, key: &str, json_value: &Value) {
    match json_value {
        Value::String(value) => {
            table.insert(key, toml_edit::value(value));
        }
        Value::Bool(value) => {
            table.insert(key, toml_edit::value(*value));
        }
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                table.insert(key, toml_edit::value(value));
            } else if let Some(value) = value.as_f64() {
                table.insert(key, toml_edit::value(value));
            }
        }
        Value::Array(values) => {
            let mut array = Array::default();
            for value in values {
                if let Some(value) = value.as_str() {
                    array.push(value);
                }
            }
            table.insert(key, Item::Value(toml_edit::Value::Array(array)));
        }
        Value::Object(values) => {
            let mut nested = Table::new();
            for (nested_key, nested_value) in values {
                insert_json_value(&mut nested, nested_key, nested_value);
            }
            table.insert(key, Item::Table(nested));
        }
        Value::Null => {}
    }
}
