//! Minimal MCP stdio server (newline-delimited JSON-RPC) exposing check/build/reference tools.
use crate::bundle::make_entry_bundle;
use crate::ctx::{js_stringify, Diag, Options};
use crate::index::compile_project;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const LANGUAGE_REF: &str = include_str!("../docs/tess-language.md");
pub const COMPILE_REF: &str = include_str!("../docs/tess-compile.md");
pub const WORKFLOW: &str = include_str!("../docs/WORKFLOW.md");

fn diag(d: &Diag) -> Value {
    json!({"file": d.file, "line": d.line, "column": d.column, "message": d.message})
}

fn tool_list() -> Value {
    json!([
        {
            "name": "tess_reference",
            "description": "Returns the Tess language reference. topic: language (default), compile, workflow.",
            "inputSchema": {"type": "object", "properties": {"topic": {"type": "string", "enum": ["language", "compile", "workflow"]}}}
        },
        {
            "name": "tess_check",
            "description": "Compiles Tess source in memory and returns errors, warnings and notices (line/column/message).",
            "inputSchema": {"type": "object", "required": ["source"], "properties": {
                "source": {"type": "string", "description": "Tess source text"},
                "assets_dir": {"type": "string", "description": "Folder holding costume/sound files and `use` targets (default: server working directory)"},
                "strict": {"type": "boolean"}
            }}
        },
        {
            "name": "tess_build",
            "description": "Compiles Tess source and writes an Entry .ent file (or project.json when the output ends in .json).",
            "inputSchema": {"type": "object", "required": ["source", "output"], "properties": {
                "source": {"type": "string", "description": "Tess source text"},
                "output": {"type": "string", "description": "Output path, e.g. out/game.ent"},
                "assets_dir": {"type": "string"},
                "name": {"type": "string", "description": "Project name override"},
                "strict": {"type": "boolean"}
            }}
        }
    ])
}

fn options(args: &Value) -> Options {
    let dir = args
        .get("assets_dir")
        .and_then(|d| d.as_str())
        .map(|d| crate::assets::normalize(std::path::Path::new(d)))
        .unwrap_or_else(|| crate::assets::normalize(std::path::Path::new(".")));
    Options {
        path: format!("{}/main.tess", dir.trim_end_matches('/')),
        asset_dirs: vec![dir],
        name: args.get("name").and_then(|n| n.as_str()).map(|n| n.to_string()),
        strict: args.get("strict").and_then(|s| s.as_bool()).unwrap_or(false),
        seed: None,
        asset_urls: false,
    }
}

fn call_tool(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "tess_reference" => Ok(match args.get("topic").and_then(|t| t.as_str()).unwrap_or("language") {
            "compile" => COMPILE_REF,
            "workflow" => WORKFLOW,
            _ => LANGUAGE_REF,
        }
        .to_string()),
        "tess_check" | "tess_build" => {
            let source = args.get("source").and_then(|s| s.as_str()).ok_or("source is required")?;
            let result = compile_project(source, options(args));
            let mut out = json!({
                "ok": result.ok,
                "errors": result.errors.iter().map(diag).collect::<Vec<_>>(),
                "warnings": result.warnings.iter().map(diag).collect::<Vec<_>>(),
                "notices": result.notices.iter().map(diag).collect::<Vec<_>>(),
            });
            if name == "tess_build" {
                let output = args.get("output").and_then(|o| o.as_str()).ok_or("output is required")?;
                match &result.project {
                    None => out["written"] = json!(false),
                    Some(project) => {
                        if let Some(parent) = std::path::Path::new(output).parent() {
                            if !parent.as_os_str().is_empty() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                        }
                        let json_text = js_stringify(project);
                        let bytes = if output.ends_with(".json") {
                            Ok(json_text.into_bytes())
                        } else {
                            make_entry_bundle(&json_text, &result.assets)
                        };
                        match bytes.and_then(|b| std::fs::write(output, b).map_err(|e| e.to_string())) {
                            Ok(()) => {
                                out["written"] = json!(true);
                                out["output"] = json!(output);
                            }
                            Err(e) => return Err(format!("write failed: {}", e)),
                        }
                    }
                }
            }
            Ok(crate::ctx::js_stringify_pretty(&out))
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn fail(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub fn handle(request: &Value) -> Option<Value> {
    let method = request.get("method").and_then(|m| m.as_str())?;
    let id = request.get("id").cloned();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let id = match id {
        Some(i) => i,
        None => return None,
    };
    Some(match method {
        "initialize" => reply(
            &id,
            json!({
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2024-11-05")),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "tess-ent", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Write Tess source, validate with tess_check, then export with tess_build. Call tess_reference first to learn the language."
            }),
        ),
        "ping" => reply(&id, json!({})),
        "tools/list" => reply(&id, json!({"tools": tool_list()})),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args) {
                Ok(text) => reply(&id, json!({"content": [{"type": "text", "text": text}]})),
                Err(e) => reply(&id, json!({"isError": true, "content": [{"type": "text", "text": e}]})),
            }
        }
        _ => fail(&id, -32601, "method not found"),
    })
}

pub fn serve() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&req),
            Err(_) => Some(fail(&Value::Null, -32700, "parse error")),
        };
        if let Some(r) = response {
            let _ = writeln!(stdout, "{}", r);
            let _ = stdout.flush();
        }
    }
}
