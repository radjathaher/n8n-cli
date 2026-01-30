use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;

#[derive(Debug, Serialize)]
struct CommandTree {
    version: String,
    base_path: String,
    resources: Vec<Resource>,
}

#[derive(Debug, Serialize)]
struct Resource {
    name: String,
    ops: Vec<Operation>,
}

#[derive(Debug, Serialize)]
struct Operation {
    name: String,
    method: String,
    path: String,
    summary: Option<String>,
    description: Option<String>,
    params: Vec<ParamDef>,
    body: Option<BodyDef>,
}

#[derive(Debug, Serialize)]
struct ParamDef {
    name: String,
    flag: String,
    location: String,
    required: bool,
    schema: SchemaDef,
}

#[derive(Debug, Serialize)]
struct BodyDef {
    required: bool,
    content_type: String,
    schema: SchemaDef,
    input_fields: Vec<InputField>,
}

#[derive(Debug, Serialize)]
struct InputField {
    name: String,
    flag: String,
    required: bool,
    schema: SchemaDef,
}

#[derive(Debug, Serialize, Clone)]
struct SchemaDef {
    kind: String,
    item: Option<Box<SchemaDef>>,
}

fn main() -> Result<()> {
    let mut input = "n8n-api.yaml".to_string();
    let mut output = "schemas/command_tree.json".to_string();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--in" => {
                input = args.next().context("missing value for --in")?;
            }
            "--out" => {
                output = args.next().context("missing value for --out")?;
            }
            _ => {}
        }
    }

    let raw = fs::read_to_string(&input).with_context(|| format!("read {input}"))?;
    let doc: Value = serde_yaml::from_str(&raw).context("parse yaml")?;

    let version = doc
        .get("info")
        .and_then(|v| v.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("0")
        .to_string();

    let base_path = doc
        .get("servers")
        .and_then(Value::as_array)
        .and_then(|servers| servers.first())
        .and_then(|server| server.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("/api/v1")
        .to_string();

    let paths = doc
        .get("paths")
        .and_then(Value::as_object)
        .context("paths missing")?;

    let mut resources: BTreeMap<String, Vec<Operation>> = BTreeMap::new();

    for (path, item) in paths {
        let path_params = item
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for method in ["get", "post", "put", "patch", "delete"] {
            let op = match item.get(method) {
                Some(op) => op,
                None => continue,
            };

            let op_obj = op.as_object().context("operation not object")?;
            let tag = op_obj
                .get("tags")
                .and_then(Value::as_array)
                .and_then(|tags| tags.first())
                .and_then(Value::as_str)
                .unwrap_or("default");
            let resource = to_kebab(tag);

            let op_id = op_obj
                .get("operationId")
                .and_then(Value::as_str)
                .or_else(|| op_obj.get("x-eov-operation-id").and_then(Value::as_str))
                .unwrap_or("call");

            let name = to_kebab(op_id);
            let summary = op_obj
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string);
            let description = op_obj
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);

            let op_params = op_obj
                .get("parameters")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let params = merge_params(&doc, &path_params, &op_params)?;
            let body = parse_request_body(&doc, op_obj.get("requestBody"))?;

            let op = Operation {
                name,
                method: method.to_uppercase(),
                path: path.to_string(),
                summary,
                description,
                params,
                body,
            };

            resources.entry(resource).or_default().push(op);
        }
    }

    let mut out_resources = Vec::new();
    for (name, mut ops) in resources {
        ops.sort_by(|a, b| a.name.cmp(&b.name));
        out_resources.push(Resource { name, ops });
    }

    let tree = CommandTree {
        version,
        base_path,
        resources: out_resources,
    };

    let json = serde_json::to_string_pretty(&tree)?;
    fs::write(&output, json).with_context(|| format!("write {output}"))?;

    Ok(())
}

fn merge_params(doc: &Value, path_params: &[Value], op_params: &[Value]) -> Result<Vec<ParamDef>> {
    let mut map: BTreeMap<(String, String), ParamDef> = BTreeMap::new();

    for param in path_params {
        if let Some(def) = parse_param(doc, param)? {
            let key = (def.location.clone(), def.name.clone());
            map.insert(key, def);
        }
    }

    for param in op_params {
        if let Some(def) = parse_param(doc, param)? {
            let key = (def.location.clone(), def.name.clone());
            map.insert(key, def);
        }
    }

    Ok(map.into_values().collect())
}

fn parse_param(doc: &Value, param: &Value) -> Result<Option<ParamDef>> {
    let param = resolve_ref(doc, param);
    let name = param
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Ok(None);
    }

    let location = param
        .get("in")
        .and_then(Value::as_str)
        .unwrap_or("query")
        .to_string();
    let required = param
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let schema = param.get("schema").unwrap_or(&Value::Null);
    let schema_def = schema_def(doc, schema);

    Ok(Some(ParamDef {
        name: name.clone(),
        flag: to_kebab(&name),
        location,
        required,
        schema: schema_def,
    }))
}

fn parse_request_body(doc: &Value, request_body: Option<&Value>) -> Result<Option<BodyDef>> {
    let Some(body) = request_body else {
        return Ok(None);
    };

    let body = resolve_ref(doc, body);
    let required = body
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let content = body.get("content").and_then(Value::as_object);
    let Some(content) = content else {
        return Ok(None);
    };

    let (content_type, schema) = if let Some(json) = content.get("application/json") {
        ("application/json".to_string(), json.get("schema"))
    } else {
        let first = content.iter().next();
        match first {
            Some((ct, item)) => (ct.clone(), item.get("schema")),
            None => return Ok(None),
        }
    };

    let schema = schema.unwrap_or(&Value::Null);
    let schema_def = schema_def(doc, schema);
    let input_fields = if schema_def.kind == "object" {
        input_fields_from_schema(doc, schema)
    } else {
        Vec::new()
    };

    Ok(Some(BodyDef {
        required,
        content_type,
        schema: schema_def,
        input_fields,
    }))
}

fn input_fields_from_schema(doc: &Value, schema: &Value) -> Vec<InputField> {
    let schema = resolve_ref(doc, schema);
    let properties = schema.get("properties").and_then(Value::as_object);
    let Some(properties) = properties else {
        return Vec::new();
    };

    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    let mut fields = Vec::new();
    for (name, prop) in properties {
        let schema_def = schema_def(doc, prop);
        fields.push(InputField {
            name: name.clone(),
            flag: format!("input-{}", to_kebab(name)),
            required: required.contains(name),
            schema: schema_def,
        });
    }

    fields.sort_by(|a, b| a.name.cmp(&b.name));
    fields
}

fn schema_def(doc: &Value, schema: &Value) -> SchemaDef {
    let schema = resolve_ref(doc, schema);

    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        if let Some(first) = all_of.first() {
            return schema_def(doc, first);
        }
    }

    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        if let Some(first) = one_of.first() {
            return schema_def(doc, first);
        }
    }

    let type_value = schema.get("type").and_then(Value::as_str);
    match type_value {
        Some("object") => SchemaDef {
            kind: "object".to_string(),
            item: None,
        },
        Some("array") => {
            let item = schema
                .get("items")
                .map(|item| schema_def(doc, item))
                .map(Box::new);
            SchemaDef {
                kind: "array".to_string(),
                item,
            }
        }
        Some(kind) => SchemaDef {
            kind: kind.to_string(),
            item: None,
        },
        None => {
            if schema.get("properties").is_some() {
                SchemaDef {
                    kind: "object".to_string(),
                    item: None,
                }
            } else if schema.get("items").is_some() {
                let item = schema
                    .get("items")
                    .map(|item| schema_def(doc, item))
                    .map(Box::new);
                SchemaDef {
                    kind: "array".to_string(),
                    item,
                }
            } else {
                SchemaDef {
                    kind: "unknown".to_string(),
                    item: None,
                }
            }
        }
    }
}

fn resolve_ref<'a>(doc: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return schema;
    };

    if !reference.starts_with("#/") {
        return schema;
    }

    let mut current = doc;
    for part in reference.trim_start_matches("#/").split('/') {
        if let Some(next) = current.get(part) {
            current = next;
        } else {
            return schema;
        }
    }

    current
}

fn to_kebab(value: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;

    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            if !out.ends_with('-') {
                out.push('-');
            }
            prev_lower = false;
            continue;
        }

        if ch.is_ascii_uppercase() {
            if prev_lower {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower = false;
            continue;
        }

        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }

    out.trim_matches('-').to_string()
}
