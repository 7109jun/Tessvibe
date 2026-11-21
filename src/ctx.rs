//! Compilation context: symbol tables, id factory, block builder, diagnostics.
use crate::assets::AssetFile;
use crate::ids::IdFactory;
use crate::jsnum::num_to_string;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Insertion-ordered string map.
#[derive(Clone, Debug)]
pub struct OMap<V> {
    pub items: Vec<(String, V)>,
}

impl<V> Default for OMap<V> {
    fn default() -> Self {
        OMap { items: Vec::new() }
    }
}

impl<V> OMap<V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, k: &str) -> Option<&V> {
        self.items.iter().find(|(n, _)| n == k).map(|(_, v)| v)
    }
    pub fn has(&self, k: &str) -> bool {
        self.items.iter().any(|(n, _)| n == k)
    }
    pub fn set(&mut self, k: &str, v: V) {
        if let Some(slot) = self.items.iter_mut().find(|(n, _)| n == k) {
            slot.1 = v;
        } else {
            self.items.push((k.to_string(), v));
        }
    }
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|(k, _)| k.as_str())
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.items.iter().map(|(_, v)| v)
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[derive(Clone, Debug)]
pub struct VarInfo {
    pub id: String,
    pub name: String,
    pub variable_type: String,
}

#[derive(Clone, Debug)]
pub enum VarRef {
    Param(String),
    FuncLocal { id: String },
    Variable { entry: VarInfo },
    Ambiguous { name: String, owners: Vec<String> },
}

pub enum ResourceRef {
    Found(Value),
    Ambiguous(Vec<String>),
}

#[derive(Clone, Debug, Default)]
pub struct FunctionScope {
    pub name: String,
    pub params: OMap<String>,
    pub local_vars: OMap<(String, String)>,
}

#[derive(Clone)]
pub struct TableInfo {
    pub id: String,
}

pub struct CompiledObject {
    pub id: String,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub scene_id: String,
    pub node: Value,
    pub pictures: OMap<Value>,
    pub sounds: OMap<Value>,
    pub locals: OMap<VarInfo>,
    pub default_picture: Option<Value>,
    pub properties: OMap<Value>,
    pub box_size: Option<(f64, f64)>,
    pub center: Option<(f64, f64)>,
    pub script: Vec<Vec<Value>>,
}

#[derive(Clone)]
pub struct CompiledFunction {
    pub id: String,
    pub name: String,
    pub node: Option<Value>,
    pub owner: Option<String>,
    pub params: Vec<String>,
    pub boolean_params: HashSet<String>,
    pub param_types: OMap<String>,
    pub is_value: bool,
    pub local_variables: Vec<Value>,
    pub generated: bool,
    pub ty: Option<String>,
    pub content: Vec<Vec<Value>>,
}

pub use crate::validate::Diag;

#[derive(Default)]
pub struct Options {
    pub path: String,
    pub asset_dirs: Vec<String>,
    pub name: Option<String>,
    pub strict: bool,
    pub seed: Option<String>,
    pub asset_urls: bool,
}

pub struct Context {
    pub source: String,
    pub sources: HashMap<String, String>,
    pub options: Options,
    pub asset_files: Vec<AssetFile>,
    pub comments: HashMap<String, String>,
    pub new_id: IdFactory,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub notices: Vec<Diag>,
    pub ai_utilize_blocks: Vec<String>,
    pub forced_resource_ids: HashSet<String>,
    pub scenes: Vec<(String, String)>,
    pub scene_by_name: OMap<(String, String)>,
    pub objects: Vec<CompiledObject>,
    pub object_by_name: OMap<usize>,
    pub variables: Vec<Value>,
    pub globals: OMap<VarInfo>,
    pub messages: Vec<(String, String)>,
    pub tables: Vec<Value>,
    pub table_by_name: OMap<TableInfo>,
    pub expansion_blocks: Vec<String>,
    pub functions: Vec<CompiledFunction>,
    pub function_by_name: OMap<usize>,
    pub runtime_functions: OMap<usize>,
    pub object: Option<usize>,
    pub locals: OMap<VarInfo>,
    pub func_scope: Option<FunctionScope>,
    line_starts: HashMap<String, Vec<usize>>,
}

pub fn make_block(id: String, ty: &str, params: Vec<Value>, statements: Vec<Vec<Value>>) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(id));
    m.insert("x".into(), json!(0));
    m.insert("y".into(), json!(0));
    m.insert("type".into(), json!(ty));
    m.insert("params".into(), Value::Array(params));
    m.insert("statements".into(), Value::Array(statements.into_iter().map(Value::Array).collect()));
    m.insert("movable".into(), Value::Null);
    m.insert("deletable".into(), json!(1));
    m.insert("emphasized".into(), json!(false));
    m.insert("readOnly".into(), Value::Null);
    m.insert("copyable".into(), json!(true));
    m.insert("assemble".into(), json!(true));
    m.insert("extensions".into(), json!([]));
    Value::Object(m)
}

pub fn num(v: f64) -> Value {
    json!(v)
}

pub fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

pub fn ty(v: &Value) -> &str {
    v.get("type").and_then(|x| x.as_str()).unwrap_or("")
}

pub fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

pub fn offset_of(node: &Value) -> usize {
    node.get("loc").and_then(|l| l.get("start")).and_then(|x| x.as_u64()).unwrap_or(0) as usize
}

pub fn file_of(node: &Value) -> Option<String> {
    node.get("loc").and_then(|l| l.get("file")).and_then(|f| f.as_str()).map(|s| s.to_string())
}

pub fn comment_key(node: &Value) -> String {
    let loc = node.get("loc");
    let file = loc.and_then(|l| l.get("file")).and_then(|f| f.as_str()).unwrap_or("");
    let start = loc.and_then(|l| l.get("start")).and_then(|x| x.as_i64()).unwrap_or(-1);
    format!("{} {}", file, start)
}

pub fn make_comment(text: &str) -> Value {
    json!({
        "x": 240, "y": -10, "width": 160, "height": 100, "value": text, "readOnly": false,
        "visible": true, "display": true, "movable": true, "isOpened": true, "deletable": 1,
        "type": "comment"
    })
}

impl Context {
    pub fn new(source: &str, sources: HashMap<String, String>, comments: HashMap<String, String>, options: Options) -> Self {
        let seed = options.seed.clone().unwrap_or_else(|| source.to_string());
        Context {
            source: source.to_string(),
            sources,
            options,
            asset_files: vec![],
            comments,
            new_id: IdFactory::new(&seed),
            errors: vec![],
            warnings: vec![],
            notices: vec![],
            ai_utilize_blocks: vec![],
            forced_resource_ids: HashSet::new(),
            scenes: vec![],
            scene_by_name: OMap::new(),
            objects: vec![],
            object_by_name: OMap::new(),
            variables: vec![],
            globals: OMap::new(),
            messages: vec![],
            tables: vec![],
            table_by_name: OMap::new(),
            expansion_blocks: vec![],
            functions: vec![],
            function_by_name: OMap::new(),
            runtime_functions: OMap::new(),
            object: None,
            locals: OMap::new(),
            func_scope: None,
            line_starts: HashMap::new(),
        }
    }

    pub fn id(&mut self) -> String {
        self.new_id.next()
    }

    fn position(&mut self, file: Option<&str>, offset: usize) -> (usize, usize) {
        let key = file.unwrap_or("").to_string();
        if !self.line_starts.contains_key(&key) {
            let text = file.and_then(|f| self.sources.get(f)).cloned().unwrap_or_else(|| self.source.clone());
            let mut starts = vec![0usize];
            let mut at = 0usize;
            for c in text.chars() {
                at += c.len_utf16();
                if c == '\n' {
                    starts.push(at);
                }
            }
            starts.push(usize::MAX); // sentinel holding the length
            let len = at;
            let last = starts.len() - 1;
            starts[last] = len;
            self.line_starts.insert(key.clone(), starts);
        }
        let starts = &self.line_starts[&key];
        let len = *starts.last().unwrap();
        let line_starts = &starts[..starts.len() - 1];
        let target = offset.min(len);
        let (mut low, mut high) = (0usize, line_starts.len() - 1);
        while low < high {
            let mid = (low + high + 1) >> 1;
            if line_starts[mid] <= target {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        (low + 1, target - line_starts[low] + 1)
    }

    fn report(&mut self, kind: u8, node: &Value, message: String) {
        let offset = offset_of(node);
        let file = file_of(node);
        let (line, column) = self.position(file.as_deref(), offset);
        let d = Diag { line, column, offset, message, file, detail: None };
        match kind {
            0 => self.errors.push(d),
            1 => self.warnings.push(d),
            _ => self.notices.push(d),
        }
    }

    /// Records an error; returns None so callers can `return ctx.error(..)`.
    pub fn error<T>(&mut self, node: &Value, message: impl Into<String>) -> Option<T> {
        self.report(0, node, message.into());
        None
    }

    pub fn warn(&mut self, node: &Value, message: impl Into<String>) {
        self.report(1, node, message.into());
    }

    pub fn notice(&mut self, node: &Value, message: impl Into<String>) {
        self.report(2, node, message.into());
    }

    pub fn block(&mut self, ty: &str, params: Vec<Value>) -> Value {
        let id = self.id();
        make_block(id, ty, params, vec![])
    }

    pub fn block_with(&mut self, ty: &str, params: Vec<Value>, statements: Vec<Vec<Value>>) -> Value {
        let id = self.id();
        make_block(id, ty, params, statements)
    }

    pub fn apply_comment(&self, node: &Value, block: &mut Value) {
        if let Some(text) = self.comments.get(&comment_key(node)) {
            if !text.is_empty() {
                block["comment"] = make_comment(text);
            }
        }
    }

    pub fn number(&mut self, v: f64) -> Value {
        self.block("number", vec![Value::String(num_to_string(v))])
    }

    pub fn number_str(&mut self, v: &str) -> Value {
        self.block("number", vec![Value::String(v.to_string())])
    }

    pub fn text(&mut self, v: &str) -> Value {
        self.block("text", vec![Value::String(v.to_string())])
    }

    pub fn angle(&mut self, v: f64) -> Value {
        self.block("angle", vec![Value::String(num_to_string(v))])
    }

    pub fn lookup_variable(&self, name: &str) -> Option<VarRef> {
        if let Some(scope) = &self.func_scope {
            if scope.params.has(name) {
                return Some(VarRef::Param(name.to_string()));
            }
            if let Some((id, _)) = scope.local_vars.get(name) {
                return Some(VarRef::FuncLocal { id: id.clone() });
            }
            if let Some(e) = self.locals.get(name) {
                return Some(VarRef::Variable { entry: e.clone() });
            }
            if let Some(e) = self.globals.get(name) {
                return Some(VarRef::Variable { entry: e.clone() });
            }
            return self.lookup_object_local(name);
        }
        if let Some(e) = self.locals.get(name) {
            return Some(VarRef::Variable { entry: e.clone() });
        }
        self.globals.get(name).map(|e| VarRef::Variable { entry: e.clone() })
    }

    pub fn lookup_object_local(&self, name: &str) -> Option<VarRef> {
        let owners: Vec<&CompiledObject> = self.objects.iter().filter(|o| o.locals.has(name)).collect();
        if owners.is_empty() {
            return None;
        }
        if owners.len() > 1 {
            return Some(VarRef::Ambiguous { name: name.to_string(), owners: owners.iter().map(|o| o.name.clone()).collect() });
        }
        Some(VarRef::Variable { entry: owners[0].locals.get(name).unwrap().clone() })
    }

    pub fn lookup_object_resource(&self, sounds: bool, name: &str) -> Option<ResourceRef> {
        let owners: Vec<&CompiledObject> = self
            .objects
            .iter()
            .filter(|o| if sounds { o.sounds.has(name) } else { o.pictures.has(name) })
            .collect();
        if owners.is_empty() {
            return None;
        }
        if owners.len() > 1 {
            return Some(ResourceRef::Ambiguous(owners.iter().map(|o| o.name.clone()).collect()));
        }
        let shelf = if sounds { &owners[0].sounds } else { &owners[0].pictures };
        Some(ResourceRef::Found(shelf.get(name).unwrap().clone()))
    }

    pub fn object_id(&self, name: &str) -> Option<String> {
        self.object_by_name.get(name).map(|i| self.objects[*i].id.clone())
    }

    pub fn known_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.globals.keys().map(|s| s.to_string()).collect();
        names.extend(self.locals.keys().map(|s| s.to_string()));
        if let Some(scope) = &self.func_scope {
            names.extend(scope.params.keys().map(|s| s.to_string()));
            names.extend(scope.local_vars.keys().map(|s| s.to_string()));
            for o in &self.objects {
                names.extend(o.locals.keys().map(|s| s.to_string()));
            }
        }
        names
    }

    pub fn message_id(&mut self, name: &str) -> String {
        if let Some((_, id)) = self.messages.iter().find(|(n, _)| n == name) {
            return id.clone();
        }
        let id = self.id();
        self.messages.push((name.to_string(), id.clone()));
        id
    }

    pub fn add_expansion(&mut self, module: &str, ai: bool) {
        let list = if ai { &mut self.ai_utilize_blocks } else { &mut self.expansion_blocks };
        if !list.iter().any(|m| m == module) {
            list.push(module.to_string());
        }
    }

    pub fn cur_object(&self) -> Option<&CompiledObject> {
        self.object.map(|i| &self.objects[i])
    }
}

/// `JSON.stringify` with JavaScript number formatting.
pub fn js_stringify(v: &Value) -> String {
    let mut out = String::new();
    write_js(v, &mut out);
    out
}

fn write_js(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            out.push_str(&num_to_string(f));
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_js(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push(':');
                write_js(x, out);
            }
            out.push('}');
        }
    }
}

/// Pretty `JSON.stringify(v, null, 2)` with JavaScript number formatting.
pub fn js_stringify_pretty(v: &Value) -> String {
    let mut out = String::new();
    write_pretty(v, &mut out, 0);
    out
}

fn write_pretty(v: &Value, out: &mut String, depth: usize) {
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Value::Array(a) if !a.is_empty() => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                write_pretty(x, out, depth + 1);
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
        Value::Object(m) if !m.is_empty() => {
            out.push_str("{\n");
            let n = m.len();
            for (i, (k, x)) in m.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                write_pretty(x, out, depth + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
        other => write_js(other, out),
    }
}
