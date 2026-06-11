use serde_json::{json, Map, Value};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table};

pub fn read_openhands_mcp(doc: &DocumentMut) -> Value {
    let Some(mcp) = doc.get("mcp").and_then(Item::as_table) else {
        return json!({});
    };
    let mut server_map = Map::new();
    read_stdio(mcp, &mut server_map);
    read_remotes(mcp, "shttp_servers", "http", &mut server_map);
    read_remotes(mcp, "sse_servers", "sse", &mut server_map);
    json!({ "mcpServers": server_map })
}

pub fn write_openhands_mcp(doc: &mut DocumentMut, data: &Value) {
    let mut mcp_table = Table::new();
    let mut stdio_servers = Array::default();
    let mut shttp_servers = Array::default();
    let mut sse_servers = Array::default();

    for (server_name, server_config) in extract_server_object(data) {
        let Some(server_config) = server_config.as_object() else {
            continue;
        };
        if server_config.contains_key("command") {
            stdio_servers.push(inline_server_table(&server_name, server_config, true));
        } else if server_config.get("type").and_then(Value::as_str) == Some("sse") {
            sse_servers.push(inline_server_table(&server_name, server_config, false));
        } else {
            shttp_servers.push(inline_server_table(&server_name, server_config, false));
        }
    }

    if !stdio_servers.is_empty() {
        mcp_table.insert(
            "stdio_servers",
            Item::Value(toml_edit::Value::Array(stdio_servers)),
        );
    }
    if !shttp_servers.is_empty() {
        mcp_table.insert(
            "shttp_servers",
            Item::Value(toml_edit::Value::Array(shttp_servers)),
        );
    }
    if !sse_servers.is_empty() {
        mcp_table.insert(
            "sse_servers",
            Item::Value(toml_edit::Value::Array(sse_servers)),
        );
    }
    doc.insert("mcp", Item::Table(mcp_table));
}

fn extract_server_object(data: &Value) -> Map<String, Value> {
    data.get("mcp_servers")
        .or_else(|| data.get("mcpServers"))
        .or_else(|| data.get("mcp"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn read_stdio(mcp: &Table, server_map: &mut Map<String, Value>) {
    let Some(servers) = mcp.get("stdio_servers").and_then(Item::as_value) else {
        return;
    };
    let Some(servers) = servers.as_array() else {
        return;
    };
    for server in servers {
        if let toml_edit::Value::InlineTable(table) = server {
            let mut object = inline_table_to_json(table);
            let name = object
                .remove("name")
                .and_then(|value| value.as_str().map(str::to_string));
            if let Some(name) = name {
                object.insert("type".to_string(), Value::String("stdio".to_string()));
                server_map.insert(name, Value::Object(object));
            }
        }
    }
}

fn read_remotes(mcp: &Table, key: &str, transport: &str, server_map: &mut Map<String, Value>) {
    let Some(servers) = mcp.get(key).and_then(Item::as_value) else {
        return;
    };
    let Some(servers) = servers.as_array() else {
        return;
    };
    for server in servers {
        let mut object = match server {
            toml_edit::Value::String(url) => {
                let mut object = Map::new();
                object.insert("url".to_string(), Value::String(url.value().to_string()));
                object
            }
            toml_edit::Value::InlineTable(table) => inline_table_to_json(table),
            _ => continue,
        };
        let Some(url) = object
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        object.insert("type".to_string(), Value::String(transport.to_string()));
        server_map.insert(url, Value::Object(object));
    }
}

fn inline_server_table(
    server_name: &str,
    server_config: &Map<String, Value>,
    include_name: bool,
) -> InlineTable {
    let mut table = InlineTable::new();
    if include_name {
        table.insert("name", toml_edit::Value::from(server_name));
    }
    for (key, value) in server_config {
        if key != "type" {
            insert_inline_json_value(&mut table, key, value);
        }
    }
    table
}

fn inline_table_to_json(table: &InlineTable) -> Map<String, Value> {
    let mut object = Map::new();
    for (key, value) in table {
        if let Ok(json_value) = toml_value_to_json(value) {
            object.insert(key.to_string(), json_value);
        }
    }
    object
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
        toml_edit::Value::InlineTable(table) => Value::Object(inline_table_to_json(table)),
        toml_edit::Value::Datetime(value) => Value::String(value.to_string()),
    })
}

fn insert_inline_json_value(table: &mut InlineTable, key: &str, json_value: &Value) {
    match json_value {
        Value::String(value) => {
            table.insert(key, toml_edit::Value::from(value.as_str()));
        }
        Value::Bool(value) => {
            table.insert(key, toml_edit::Value::from(*value));
        }
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                table.insert(key, toml_edit::Value::from(value));
            } else if let Some(value) = value.as_f64() {
                table.insert(key, toml_edit::Value::from(value));
            }
        }
        Value::Array(values) => {
            let mut array = Array::default();
            for value in values {
                if let Some(value) = value.as_str() {
                    array.push(value);
                }
            }
            table.insert(key, toml_edit::Value::Array(array));
        }
        Value::Object(_) | Value::Null => {}
    }
}
