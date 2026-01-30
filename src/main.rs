mod command_tree;

use anyhow::{Context, Result, anyhow};
use clap::{Arg, ArgAction, Command};
use command_tree::{BodyDef, CommandTree, InputField, Operation, ParamDef, SchemaDef};
use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::{Map, Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::time::Duration;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let tree = command_tree::load_command_tree();
    let cli = build_cli(&tree);
    let matches = cli.get_matches();

    if let Some(matches) = matches.subcommand_matches("list") {
        return handle_list(&tree, matches);
    }
    if let Some(matches) = matches.subcommand_matches("describe") {
        return handle_describe(&tree, matches);
    }
    if let Some(matches) = matches.subcommand_matches("tree") {
        return handle_tree(&tree, matches);
    }

    let api_key = env::var("N8N_API_KEY").context("N8N_API_KEY missing")?;
    let base_url = env::var("N8N_BASE_URL").context("N8N_BASE_URL missing")?;

    let pretty = matches.get_flag("pretty");
    let raw = matches.get_flag("raw");

    let (res_name, res_matches) = matches
        .subcommand()
        .ok_or_else(|| anyhow!("resource required"))?;
    let (op_name, op_matches) = res_matches
        .subcommand()
        .ok_or_else(|| anyhow!("operation required"))?;

    let op = find_op(&tree, res_name, op_name)
        .ok_or_else(|| anyhow!("unknown command {res_name} {op_name}"))?;

    let url = build_url(&base_url, &tree.base_path, op, op_matches)?;
    let body = build_body(op, op_matches)?;
    let response = send_request(&api_key, op, url, body)?;

    let output = if raw { response.raw } else { response.body };

    if pretty {
        write_stdout_line(&serde_json::to_string_pretty(&output)?)?;
    } else {
        write_stdout_line(&serde_json::to_string(&output)?)?;
    }

    if !response.ok {
        return Err(anyhow!("http error: {}", response.status));
    }

    Ok(())
}

fn build_cli(tree: &CommandTree) -> Command {
    let mut cmd = Command::new("n8n")
        .about("n8n CLI (auto-generated from OpenAPI)")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("pretty")
                .long("pretty")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Pretty-print JSON output"),
        )
        .arg(
            Arg::new("raw")
                .long("raw")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Return full HTTP response envelope"),
        );

    cmd = cmd.subcommand(
        Command::new("list")
            .about("List resources and operations")
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Emit machine-readable JSON"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("describe")
            .about("Describe a specific operation")
            .arg(Arg::new("resource").required(true))
            .arg(Arg::new("op").required(true))
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Emit machine-readable JSON"),
            ),
    );

    cmd = cmd.subcommand(
        Command::new("tree").about("Show full command tree").arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Emit machine-readable JSON"),
        ),
    );

    for resource in &tree.resources {
        let mut res_cmd = Command::new(resource.name.clone())
            .about(resource.name.clone())
            .subcommand_required(true)
            .arg_required_else_help(true);
        for op in &resource.ops {
            let mut op_cmd =
                Command::new(op.name.clone()).about(op.summary.clone().unwrap_or_default());
            for param in &op.params {
                op_cmd = op_cmd.arg(build_param_arg(param));
            }
            if let Some(body) = &op.body {
                op_cmd = op_cmd
                    .arg(
                        Arg::new("body")
                            .long("body")
                            .value_name("JSON")
                            .help("Raw JSON request body"),
                    )
                    .arg(
                        Arg::new("body-file")
                            .long("body-file")
                            .value_name("PATH")
                            .help("Path to JSON request body"),
                    );

                for field in &body.input_fields {
                    op_cmd = op_cmd.arg(build_input_field_arg(field));
                }
            }
            res_cmd = res_cmd.subcommand(op_cmd);
        }
        cmd = cmd.subcommand(res_cmd);
    }

    cmd
}

fn build_param_arg(param: &ParamDef) -> Arg {
    let mut arg_def = Arg::new(param.name.clone())
        .long(param.flag.clone())
        .value_name(schema_label(&param.schema));

    if param.schema.kind == "array" {
        arg_def = arg_def.action(ArgAction::Append);
    }

    if param.required {
        arg_def = arg_def.required(true);
    }

    arg_def
}

fn build_input_field_arg(field: &InputField) -> Arg {
    let key = input_field_key(field);
    let mut arg_def = Arg::new(key)
        .long(field.flag.clone())
        .value_name(schema_label(&field.schema));

    if field.schema.kind == "array" {
        arg_def = arg_def.action(ArgAction::Append);
    }

    arg_def
}

fn handle_list(tree: &CommandTree, matches: &clap::ArgMatches) -> Result<()> {
    if matches.get_flag("json") {
        let mut out = Vec::new();
        for res in &tree.resources {
            let ops: Vec<String> = res.ops.iter().map(|op| op.name.clone()).collect();
            out.push(json!({"resource": res.name, "ops": ops}));
        }
        write_stdout_line(&serde_json::to_string_pretty(&out)?)?;
        return Ok(());
    }

    for res in &tree.resources {
        write_stdout_line(&res.name)?;
        for op in &res.ops {
            write_stdout_line(&format!("  {}", op.name))?;
        }
    }
    Ok(())
}

fn handle_describe(tree: &CommandTree, matches: &clap::ArgMatches) -> Result<()> {
    let resource = matches
        .get_one::<String>("resource")
        .ok_or_else(|| anyhow!("resource required"))?;
    let op_name = matches
        .get_one::<String>("op")
        .ok_or_else(|| anyhow!("operation required"))?;

    let op = find_op(tree, resource, op_name)
        .ok_or_else(|| anyhow!("unknown command {resource} {op_name}"))?;

    if matches.get_flag("json") {
        write_stdout_line(&serde_json::to_string_pretty(op)?)?;
        return Ok(());
    }

    write_stdout_line(&format!("{} {}", resource, op.name))?;
    write_stdout_line(&format!("  method: {}", op.method))?;
    write_stdout_line(&format!("  path: {}", op.path))?;
    if let Some(summary) = &op.summary {
        if !summary.trim().is_empty() {
            write_stdout_line(&format!("  summary: {}", summary.trim()))?;
        }
    }
    if !op.params.is_empty() {
        write_stdout_line("  params:")?;
        for param in &op.params {
            write_stdout_line(&format!(
                "    --{}  {} ({})",
                param.flag, param.schema.kind, param.location
            ))?;
        }
    }
    if let Some(body) = &op.body {
        write_stdout_line(&format!("  body: {}", body.content_type))?;
        if !body.input_fields.is_empty() {
            write_stdout_line("  body fields:")?;
            for field in &body.input_fields {
                write_stdout_line(&format!("    --{}  {}", field.flag, field.schema.kind))?;
            }
        }
    }

    Ok(())
}

fn handle_tree(tree: &CommandTree, matches: &clap::ArgMatches) -> Result<()> {
    if matches.get_flag("json") {
        write_stdout_line(&serde_json::to_string_pretty(tree)?)?;
        return Ok(());
    }
    write_stdout_line("Run with --json for machine-readable output.")?;
    Ok(())
}

fn find_op<'a>(tree: &'a CommandTree, res: &str, op: &str) -> Option<&'a Operation> {
    tree.resources
        .iter()
        .find(|r| r.name == res)
        .and_then(|r| r.ops.iter().find(|o| o.name == op))
}

fn build_url(
    base_url: &str,
    base_path: &str,
    op: &Operation,
    matches: &clap::ArgMatches,
) -> Result<Url> {
    let base = base_url.trim_end_matches('/');
    let mut base_path = base_path.trim().to_string();
    if !base_path.starts_with('/') {
        base_path = format!("/{base_path}");
    }

    let api_base = if base.ends_with(&base_path) {
        base.to_string()
    } else {
        format!("{base}{base_path}")
    };

    let mut path = op.path.clone();
    for param in op.params.iter().filter(|p| p.location == "path") {
        let value = matches
            .get_one::<String>(&param.name)
            .ok_or_else(|| anyhow!("missing required param --{}", param.flag))?;
        let encoded = urlencoding::encode(value);
        path = path.replace(&format!("{{{}}}", param.name), encoded.as_ref());
    }

    let url_str = format!("{api_base}{path}");
    let mut url = Url::parse(&url_str).context("invalid N8N_BASE_URL")?;

    let mut query_pairs = Vec::new();
    for param in op.params.iter().filter(|p| p.location == "query") {
        append_query_param(&mut query_pairs, param, matches)?;
    }
    if !query_pairs.is_empty() {
        let mut qp = url.query_pairs_mut();
        for (k, v) in query_pairs {
            qp.append_pair(&k, &v);
        }
    }

    Ok(url)
}

fn append_query_param(
    out: &mut Vec<(String, String)>,
    param: &ParamDef,
    matches: &clap::ArgMatches,
) -> Result<()> {
    if param.schema.kind == "array" {
        if let Some(values) = matches.get_many::<String>(&param.name) {
            let values: Vec<String> = values.cloned().collect();
            let parsed = parse_list_for_query(&param.schema, &values)?;
            for value in parsed {
                out.push((param.name.clone(), value));
            }
        }
        return Ok(());
    }

    if let Some(value) = matches.get_one::<String>(&param.name) {
        out.push((param.name.clone(), value.clone()));
    }

    Ok(())
}

fn parse_list_for_query(schema: &SchemaDef, values: &[String]) -> Result<Vec<String>> {
    if values.len() == 1 && values[0].trim_start().starts_with('[') {
        let parsed: Value = serde_json::from_str(&values[0]).context("invalid JSON list")?;
        let items = parsed
            .as_array()
            .ok_or_else(|| anyhow!("expected JSON array"))?;
        return items.iter().map(value_to_query_string).collect();
    }

    let item_schema = schema.item.as_deref().unwrap_or(schema);
    values
        .iter()
        .map(|value| {
            let parsed = parse_scalar_value(item_schema, value)?;
            value_to_query_string(&parsed)
        })
        .collect()
}

fn value_to_query_string(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok("null".to_string()),
        _ => Ok(serde_json::to_string(value)?),
    }
}

fn build_body(op: &Operation, matches: &clap::ArgMatches) -> Result<Option<Value>> {
    let Some(body) = &op.body else {
        if matches.get_one::<String>("body").is_some()
            || matches.get_one::<String>("body-file").is_some()
        {
            return Err(anyhow!("request does not accept a body"));
        }
        return Ok(None);
    };

    let raw_body = matches.get_one::<String>("body");
    let body_file = matches.get_one::<String>("body-file");
    if raw_body.is_some() && body_file.is_some() {
        return Err(anyhow!("use only one of --body or --body-file"));
    }

    if let Some(raw) = raw_body {
        let parsed: Value = serde_json::from_str(raw).context("invalid JSON body")?;
        return Ok(Some(parsed));
    }

    if let Some(path) = body_file {
        let contents =
            fs::read_to_string(path).with_context(|| format!("failed to read body file {path}"))?;
        let parsed: Value = serde_json::from_str(&contents).context("invalid JSON body file")?;
        return Ok(Some(parsed));
    }

    if body.schema.kind == "object" && !body.input_fields.is_empty() {
        if let Some(obj) = build_body_from_inputs(body, matches)? {
            return Ok(Some(obj));
        }
    }

    if body.required {
        return Err(anyhow!("request body required"));
    }

    Ok(None)
}

fn build_body_from_inputs(body: &BodyDef, matches: &clap::ArgMatches) -> Result<Option<Value>> {
    let mut obj = Map::new();
    for field in &body.input_fields {
        let key = input_field_key(field);
        if field.schema.kind == "array" {
            if let Some(values) = matches.get_many::<String>(&key) {
                let values: Vec<String> = values.cloned().collect();
                let parsed = parse_list_value(&field.schema, &values)?;
                obj.insert(field.name.clone(), parsed);
            }
            continue;
        }

        if let Some(value) = matches.get_one::<String>(&key) {
            let parsed = parse_scalar_value(&field.schema, value)?;
            obj.insert(field.name.clone(), parsed);
        }
    }

    if obj.is_empty() {
        return Ok(None);
    }

    Ok(Some(Value::Object(obj)))
}

fn parse_list_value(schema: &SchemaDef, values: &[String]) -> Result<Value> {
    if values.len() == 1 && values[0].trim_start().starts_with('[') {
        let parsed: Value = serde_json::from_str(&values[0]).context("invalid JSON list")?;
        return Ok(parsed);
    }

    let mut out = Vec::new();
    let item_schema = schema.item.as_deref().unwrap_or(schema);
    for value in values {
        out.push(parse_scalar_value(item_schema, value)?);
    }
    Ok(Value::Array(out))
}

fn parse_scalar_value(schema: &SchemaDef, value: &str) -> Result<Value> {
    match schema.kind.as_str() {
        "integer" => Ok(Value::Number(value.parse::<i64>()?.into())),
        "number" => Ok(json!(value.parse::<f64>()?)),
        "boolean" => Ok(Value::Bool(parse_bool(value)?)),
        "string" => Ok(Value::String(value.to_string())),
        "object" | "array" | "unknown" => {
            let parsed: Value = serde_json::from_str(value).context("invalid JSON value")?;
            Ok(parsed)
        }
        _ => Ok(Value::String(value.to_string())),
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(anyhow!("invalid boolean: {value}")),
    }
}

fn input_field_key(field: &InputField) -> String {
    format!("body__{}", field.name)
}

fn schema_label(schema: &SchemaDef) -> String {
    if schema.kind == "array" {
        let item = schema
            .item
            .as_ref()
            .map(|s| s.kind.as_str())
            .unwrap_or("unknown");
        return format!("array<{}>", item);
    }
    schema.kind.clone()
}

struct HttpResponse {
    ok: bool,
    status: u16,
    body: Value,
    raw: Value,
}

fn send_request(
    api_key: &str,
    op: &Operation,
    url: Url,
    body: Option<Value>,
) -> Result<HttpResponse> {
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

    let method = op.method.parse().context("invalid method")?;
    let mut req = client.request(method, url).header("X-N8N-API-KEY", api_key);

    if let Some(body) = body {
        req = req.json(&body);
    }

    let res = req.send()?;
    let status = res.status();
    let headers = res.headers().clone();
    let text = res.text().unwrap_or_default();

    let body_value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };

    let mut header_map = Map::new();
    for (name, value) in headers.iter() {
        let value = value.to_str().unwrap_or("").to_string();
        header_map.insert(name.to_string(), Value::String(value));
    }

    let raw = json!({
        "status": status.as_u16(),
        "headers": header_map,
        "body": body_value.clone(),
    });

    Ok(HttpResponse {
        ok: status.is_success(),
        status: status.as_u16(),
        body: body_value,
        raw,
    })
}

fn write_stdout_line(value: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    if let Err(err) = out.write_all(value.as_bytes()) {
        if err.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        return Err(err.into());
    }
    if let Err(err) = out.write_all(b"\n") {
        if err.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        return Err(err.into());
    }
    Ok(())
}
