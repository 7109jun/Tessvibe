//! Semantic checks over the AST (mirrors the reference validator).
use crate::core::*;
use crate::parser::line_and_column;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct Diag {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub message: String,
    pub file: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone)]
struct Ctx {
    is_text: bool,
    in_function: bool,
    loop_depth: usize,
    scope: HashSet<String>,
    object_locals: HashSet<String>,
    functions: HashSet<String>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

fn ty(v: &Value) -> &str {
    s(v, "type")
}

fn declared_names(body: &[Value]) -> HashSet<String> {
    body.iter()
        .filter(|m| matches!(ty(m), "VarDecl" | "ListDecl" | "TableDecl"))
        .map(|m| s(m, "name").to_string())
        .collect()
}

fn function_names(body: &[Value], out: &mut Vec<String>) {
    for m in body {
        if ty(m) == "FunctionDecl" {
            out.push(s(m, "name").to_string());
        }
        if matches!(ty(m), "Scene" | "Object") {
            function_names(arr(m, "body"), out);
        }
    }
}

fn walk_local_names(stmts: &[Value], names: &mut HashSet<String>) {
    for st in stmts {
        if matches!(ty(st), "VarDecl" | "ListDecl") {
            names.insert(s(st, "name").to_string());
        }
        for key in ["consequent", "alternate", "body"] {
            walk_local_names(arr(st, key), names);
        }
    }
}

fn object_local_names(object: &Value) -> HashSet<String> {
    let mut names = declared_names(arr(object, "body"));
    for m in arr(object, "body") {
        if ty(m) == "Event" {
            walk_local_names(arr(m, "body"), &mut names);
        }
    }
    names
}

fn collect_local_owners(body: &[Value], globals: &HashSet<String>, owners: &mut Vec<(String, Vec<String>)>) {
    for item in body {
        if ty(item) == "Scene" {
            collect_local_owners(arr(item, "body"), globals, owners);
        }
        if ty(item) != "Object" {
            continue;
        }
        let mut names: Vec<String> = object_local_names(item).into_iter().collect();
        names.sort_by_key(|n| {
            // keep declaration order: position of first occurrence is not tracked; sorted for determinism
            n.clone()
        });
        for name in names {
            if globals.contains(&name) {
                continue;
            }
            let obj = s(item, "name").to_string();
            match owners.iter_mut().find(|(n, _)| *n == name) {
                Some((_, list)) => {
                    if !list.contains(&obj) {
                        list.push(obj);
                    }
                }
                None => owners.push((name, vec![obj])),
            }
        }
    }
}

fn contains_use(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().any(contains_use),
        Value::Object(m) => {
            if m.get("type").and_then(|t| t.as_str()) == Some("Use") {
                return true;
            }
            m.iter().any(|(k, v)| k != "loc" && contains_use(v))
        }
        _ => false,
    }
}

pub struct Validated {
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
}

struct V<'a> {
    source: &'a str,
    sources: Option<&'a HashMap<String, String>>,
    errors: Vec<Diag>,
    warnings: Vec<Diag>,
    globals: HashSet<String>,
    known_functions: HashSet<String>,
    local_owners: Vec<(String, Vec<String>)>,
    has_use: bool,
    project_count: usize,
}

impl<'a> V<'a> {
    fn report(&mut self, error: bool, node: &Value, message: String) {
        let file = node.get("loc").and_then(|l| l.get("file")).and_then(|f| f.as_str()).map(|f| f.to_string());
        let text = file.as_ref().and_then(|f| self.sources.and_then(|m| m.get(f))).map(|x| x.as_str()).unwrap_or(self.source);
        let start = node.get("loc").and_then(|l| l.get("start")).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let (line, column) = line_and_column(text, start);
        let d = Diag { line, column, offset: start, message, file, detail: None };
        if error { self.errors.push(d) } else { self.warnings.push(d) }
    }

    fn visit_top(&mut self, item: &Value) {
        match ty(item) {
            "Project" => {
                self.project_count += 1;
                if self.project_count == 2 {
                    self.report(true, item, "project 블록은 작품 전체에 하나만 선언할 수 있습니다.".into());
                }
            }
            "Scene" => {
                for m in arr(item, "body") {
                    self.visit_top(m);
                }
            }
            "Object" => self.visit_object(item),
            "FunctionDecl" => self.visit_function(item, &HashSet::new()),
            _ => {}
        }
    }

    fn visit_object(&mut self, object: &Value) {
        let is_text = s(object, "kind") == "text";
        let locals = declared_names(arr(object, "body"));
        let mut ofn = Vec::new();
        function_names(arr(object, "body"), &mut ofn);
        for member in arr(object, "body") {
            match ty(member) {
                "Property" => {
                    if !is_text && TEXT_ONLY_PROPERTIES.contains(&s(member, "name")) {
                        self.report(true, member, format!("'{}' 은(는) 글상자(text) 전용 속성입니다. object 에서는 쓸 수 없습니다.", s(member, "name")));
                    }
                }
                "BoxSize" => {
                    if !is_text {
                        self.report(true, member, "'size 가로 세로' 는 글상자(text) 전용입니다. 모양 크기는 costume 뒤에 적으세요.".into());
                    }
                }
                "Center" => {
                    if is_text {
                        self.report(true, member, "'center 가로 세로' 는 오브젝트(object) 전용입니다. 글상자는 중심점을 옮길 수 없습니다.".into());
                    }
                }
                "FunctionDecl" => self.visit_function(member, &locals),
                "Event" => {
                    let mut scope: HashSet<String> = self.globals.clone();
                    scope.extend(locals.iter().cloned());
                    let mut functions = self.known_functions.clone();
                    functions.extend(ofn.iter().cloned());
                    let ctx = Ctx { is_text, in_function: false, loop_depth: 0, scope, object_locals: locals.clone(), functions };
                    let mut ctx = ctx;
                    self.walk_statements(arr(member, "body"), &mut ctx);
                }
                _ => {}
            }
        }
    }

    fn visit_function(&mut self, f: &Value, object_locals: &HashSet<String>) {
        let mut scope = self.globals.clone();
        for p in arr(f, "params") {
            scope.insert(p.as_str().unwrap_or("").to_string());
        }
        let mut ctx = Ctx { is_text: true, in_function: true, loop_depth: 0, scope, object_locals: object_locals.clone(), functions: self.known_functions.clone() };
        self.walk_statements(arr(f, "body"), &mut ctx);
    }

    fn walk_statements(&mut self, stmts: &[Value], ctx: &mut Ctx) {
        for st in stmts {
            self.walk_statement(st, ctx);
        }
    }

    fn walk_statement(&mut self, st: &Value, ctx: &mut Ctx) {
        match ty(st) {
            "VarDecl" | "ListDecl" => {
                self.walk_exprs(st.get("value").unwrap_or(&Value::Null), ctx);
                ctx.scope.insert(s(st, "name").to_string());
                return;
            }
            "Return" => {
                if !ctx.in_function {
                    self.report(true, st, "return 은 function 블록 안에서만 쓸 수 있습니다.".into());
                }
            }
            t @ ("Break" | "Continue" | "Skip") => {
                if ctx.loop_depth == 0 {
                    let word = match t { "Break" => "break", "Continue" => "continue", _ => "skip" };
                    self.report(true, st, format!("{} 은(는) 반복문(repeat/while/until/forever) 안에서만 쓸 수 있습니다.", word));
                }
            }
            "TextWrite" => {
                if !ctx.is_text {
                    self.report(true, st, format!("'{}' 은(는) 글상자(text) 전용 명령입니다.", s(st, "mode")));
                }
            }
            "Clear" => {
                if s(st, "target") == "text" && !ctx.is_text {
                    self.report(true, st, "'clear text' 는 글상자(text) 전용 명령입니다.".into());
                }
            }
            "Assign" => {
                let t = &st["target"];
                if !ctx.is_text && ty(t) == "Identifier" && TEXT_ONLY_PROPERTIES.contains(&s(t, "name")) && !ctx.scope.contains(s(t, "name")) {
                    self.report(true, st, format!("'{}' 은(는) 글상자(text) 전용 속성입니다.", s(t, "name")));
                }
            }
            _ => {}
        }
        if ty(st) == "Hide" && st["target"].get("name").and_then(|n| n.as_str()) == Some("chart") && !ctx.scope.contains("chart") {
            return;
        }
        let blocks: Vec<&str> = match ty(st) {
            "If" => if st["alternate"].is_null() { vec!["consequent"] } else { vec!["consequent", "alternate"] },
            "Repeat" | "While" | "Until" | "Forever" => vec!["body"],
            _ => vec![],
        };
        let is_loop = matches!(ty(st), "Repeat" | "While" | "Until" | "Forever");
        if let Value::Object(m) = st {
            for (k, v) in m {
                if k == "loc" || k == "type" || blocks.contains(&k.as_str()) {
                    continue;
                }
                self.walk_exprs(v, ctx);
            }
        }
        if is_loop {
            ctx.loop_depth += 1;
        }
        for key in blocks {
            self.walk_statements(arr(st, key), ctx);
        }
        if is_loop {
            ctx.loop_depth -= 1;
        }
    }

    fn walk_exprs(&mut self, v: &Value, ctx: &Ctx) {
        match v {
            Value::Array(a) => a.iter().for_each(|x| self.walk_exprs(x, ctx)),
            Value::Object(m) => {
                match ty(v) {
                    "Identifier" => self.check_identifier(v, ctx),
                    "Call" => self.check_call(v, ctx),
                    _ => {}
                }
                for (k, c) in m {
                    if k == "loc" || k == "type" {
                        continue;
                    }
                    self.walk_exprs(c, ctx);
                }
            }
            _ => {}
        }
    }

    fn check_identifier(&mut self, id: &Value, ctx: &Ctx) {
        let name = s(id, "name");
        if ctx.scope.contains(name) {
            return;
        }
        if STATE_VALUES.contains(&name) || OPTION_KEYWORDS.contains(&name) || OBJECT_PROPERTIES.contains(&name) || TEXT_ONLY_PROPERTIES.contains(&name) {
            return;
        }
        if ctx.in_function && ctx.object_locals.contains(name) {
            return;
        }
        if ctx.in_function {
            if let Some((_, owners)) = self.local_owners.iter().find(|(n, _)| n == name) {
                let owners = owners.clone();
                if owners.len() == 1 {
                    self.report(false, id, format!("'{}' 은(는) {} 의 지역 변수입니다. 이 함수를 {} 안에 선언하는 편이 좋습니다.", name, owners[0], owners[0]));
                } else {
                    self.report(true, id, format!("'{}' 은(는) {} 가 저마다 가진 지역 변수라 어느 것인지 알 수 없습니다. 이 함수를 그 오브젝트 안에 선언하거나, 값을 매개변수로 전달하세요.", name, owners.join(", ")));
                }
                return;
            }
        }
        if !self.has_use && !ctx.object_locals.contains(name) {
            let mut known: Vec<String> = ctx.scope.iter().cloned().collect();
            known.extend(ctx.object_locals.iter().cloned());
            known.extend(STATE_VALUES.iter().map(|x| x.to_string()));
            let hint = did_you_mean(name, known.iter().map(|x| x.as_str()));
            self.report(false, id, format!("선언되지 않은 이름 '{}' 입니다.{}", name, hint));
        }
    }

    fn check_call(&mut self, call: &Value, ctx: &Ctx) {
        let callee = s(call, "callee");
        if is_builtin_function(callee) || ctx.functions.contains(callee) {
            return;
        }
        if !self.has_use {
            let mut known: Vec<String> = builtin_function_names().iter().map(|x| x.to_string()).collect();
            known.extend(ctx.functions.iter().cloned());
            let hint = did_you_mean(callee, known.iter().map(|x| x.as_str()));
            self.report(false, call, format!("선언되지 않은 함수 '{}' 를 호출했습니다.{}", callee, hint));
        }
    }
}

pub fn validate(program: &Value, source: &str, sources: Option<&HashMap<String, String>>) -> Validated {
    let body = arr(program, "body");
    let globals = declared_names(body);
    let mut fnames = Vec::new();
    function_names(body, &mut fnames);
    let mut owners = Vec::new();
    collect_local_owners(body, &globals, &mut owners);
    let mut v = V {
        source,
        sources,
        errors: vec![],
        warnings: vec![],
        globals,
        known_functions: fnames.into_iter().collect(),
        local_owners: owners,
        has_use: contains_use(program),
        project_count: 0,
    };
    for item in body {
        v.visit_top(item);
    }
    Validated { errors: v.errors, warnings: v.warnings }
}
