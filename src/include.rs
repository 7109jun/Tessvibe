//! Resolves `use`, `useobject` and `usetext` into a single program AST.
use crate::assets::{basename, dirname, extname, host_resolve};
use crate::ctx::Diag;
use crate::parser::{line_and_column, parse_with, StartRule};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Top,
    Scene,
    Object,
}

pub struct Loaded {
    pub ast: Option<Value>,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub sources: HashMap<String, String>,
}

struct Loader<'a> {
    read: &'a dyn Fn(&str) -> Option<String>,
    errors: Vec<Diag>,
    sources: HashMap<String, String>,
    visiting: HashSet<String>,
}

fn stamp_source(node: &mut Value, file: &str) {
    match node {
        Value::Array(a) => a.iter_mut().for_each(|c| stamp_source(c, file)),
        Value::Object(m) => {
            if let Some(Value::Object(loc)) = m.get_mut("loc") {
                let missing = loc.get("file").map(|f| f.is_null() || f.as_str() == Some("")).unwrap_or(true);
                if missing {
                    loc.insert("file".into(), json!(file));
                }
            }
            for (k, v) in m.iter_mut() {
                if k != "loc" {
                    stamp_source(v, file);
                }
            }
        }
        _ => {}
    }
}

impl<'a> Loader<'a> {
    fn where_(&self, item: &Value, file: &str, message: String) -> Diag {
        let offset = item.get("loc").and_then(|l| l.get("start")).and_then(|s| s.as_u64()).unwrap_or(0) as usize;
        let text = self.sources.get(file).cloned().unwrap_or_default();
        let (line, column) = line_and_column(&text, offset);
        Diag { line, column, offset, message, file: Some(file.to_string()), detail: None }
    }

    fn load(&mut self, text: &str, file: &str, part: Part) -> Option<Value> {
        let rule = match part {
            Part::Top => StartRule::Program,
            Part::Scene => StartRule::SceneFragment,
            Part::Object => StartRule::ObjectFragment,
        };
        match parse_with(text, rule) {
            Err(f) => {
                let (line, column) = line_and_column(text, f.offset);
                self.errors.push(Diag {
                    line,
                    column,
                    offset: f.offset,
                    message: f.message,
                    file: Some(file.to_string()),
                    detail: Some(f.detail),
                });
                None
            }
            Ok(mut ast) => {
                stamp_source(&mut ast, file);
                Some(ast)
            }
        }
    }

    fn expand(&mut self, items: &[Value], file: &str, part: Part) -> Vec<Value> {
        let mut output = Vec::new();
        for item in items {
            let kind = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if kind == "UseObject" {
                if let Some(w) = self.expand_use_object(item, file) {
                    output.push(w);
                }
                continue;
            }
            if kind != "Use" {
                output.push(self.expand_nested(item, file));
                continue;
            }
            let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
            let target = host_resolve(&dirname(file), &path);
            if self.visiting.contains(&target) {
                let d = self.where_(item, file, format!("use 가 순환합니다: {}", path));
                self.errors.push(d);
                continue;
            }
            let Some(text) = (self.read)(&target) else {
                let d = self.where_(item, file, format!("불러올 파일이 없습니다: {}", path));
                self.errors.push(d);
                continue;
            };
            self.sources.insert(target.clone(), text.clone());
            self.visiting.insert(target.clone());
            if let Some(included) = self.load(&text, &target, part) {
                let body: Vec<Value> = if part == Part::Top {
                    included.get("body").and_then(|b| b.as_array()).cloned().unwrap_or_default()
                } else {
                    included.as_array().cloned().unwrap_or_default()
                };
                output.extend(self.expand(&body, &target, part));
            }
            self.visiting.remove(&target);
        }
        output
    }

    fn expand_use_object(&mut self, item: &Value, file: &str) -> Option<Value> {
        let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
        let target = host_resolve(&dirname(file), &path);
        if self.visiting.contains(&target) {
            let d = self.where_(item, file, format!("use 가 순환합니다: {}", path));
            self.errors.push(d);
            return None;
        }
        let Some(text) = (self.read)(&target) else {
            let d = self.where_(item, file, format!("불러올 파일이 없습니다: {}", path));
            self.errors.push(d);
            return None;
        };
        self.sources.insert(target.clone(), text.clone());
        self.visiting.insert(target.clone());
        let members = self.load(&text, &target, Part::Object);
        self.visiting.remove(&target);
        let members = members?;
        let list = members.as_array().cloned().unwrap_or_default();
        let ext = extname(&target);
        let base = basename(&target);
        let name = if !ext.is_empty() && base.ends_with(&ext) { &base[..base.len() - ext.len()] } else { base };
        let body = self.expand(&list, &target, Part::Object);
        Some(json!({
            "type": "Object",
            "kind": item.get("kind").cloned().unwrap_or(Value::Null),
            "name": name,
            "body": body,
            "loc": item.get("loc").cloned().unwrap_or(Value::Null),
        }))
    }

    fn expand_nested(&mut self, item: &Value, file: &str) -> Value {
        let kind = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let part = match kind {
            "Scene" => Part::Scene,
            "Object" => Part::Object,
            _ => return item.clone(),
        };
        let body = item.get("body").and_then(|b| b.as_array()).cloned().unwrap_or_default();
        let expanded = self.expand(&body, file, part);
        let mut m: Map<String, Value> = item.as_object().cloned().unwrap_or_default();
        m.insert("body".into(), Value::Array(expanded));
        Value::Object(m)
    }
}

/// Parses `source` and splices every `use` file into the program body.
pub fn load_program(source: &str, path: &str, read: &dyn Fn(&str) -> Option<String>) -> Loaded {
    let mut loader = Loader { read, errors: vec![], sources: HashMap::new(), visiting: HashSet::new() };
    loader.sources.insert(path.to_string(), source.to_string());
    let Some(mut program) = loader.load(source, path, Part::Top) else {
        return Loaded { ast: None, errors: loader.errors, warnings: vec![], sources: loader.sources };
    };
    loader.visiting.insert(host_resolve(".", path));
    let body = program.get("body").and_then(|b| b.as_array()).cloned().unwrap_or_default();
    let expanded = loader.expand(&body, path, Part::Top);
    program["body"] = Value::Array(expanded);
    Loaded { ast: Some(program), errors: loader.errors, warnings: vec![], sources: loader.sources }
}

/// Reads UTF-8 text from disk; invalid bytes become U+FFFD.
pub fn read_text(file: &str) -> Option<String> {
    std::fs::read(file).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}
