//! Project compilation pipeline: collect symbols, compile scripts, assemble the Entry project.
use crate::assets::{dirname, host_resolve, make_asset, AssetDecl, AssetFile, Probe};
use crate::comments::build_comment_map;
use crate::core::{did_you_mean, key_code};
use crate::ctx::*;
use crate::expression::{boolean_text, compile_value, key_names};
use crate::include::{load_program, read_text};
use crate::jsnum::{num_to_string, round, to_number};
use crate::runtime::STORE_PREFIX;
use crate::statement::{compile_statement, compile_statements, g, nn};
use crate::validate::validate;
use serde_json::{json, Map, Value};

const DEFAULT_SCENE_NAME: &str = "장면 1";
const DEFAULT_FONT: &str = "Nanum Gothic";
const BRUSH_DEFAULT_PROPERTIES: &[&str] = &["draw_color", "fill_color", "draw_width", "draw_alpha"];
const CHART_KINDS: &[&str] = &["bar", "line", "pie", "scatter"];
const NULL: Value = Value::Null;

pub struct CompileResult {
    pub ok: bool,
    pub project: Option<Value>,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub notices: Vec<Diag>,
    pub assets: Vec<AssetFile>,
}

fn spot_of(d: &Diag) -> String {
    format!("{}:{}", d.file.clone().unwrap_or_default(), d.offset)
}

fn without_duplicates(items: Vec<Diag>, errors: &[Diag]) -> Vec<Diag> {
    if errors.is_empty() {
        return items;
    }
    let spots: Vec<String> = errors.iter().map(spot_of).collect();
    items.into_iter().filter(|w| !spots.contains(&spot_of(w))).collect()
}

fn list_of(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default()
}

pub fn compile_project(source: &str, mut options: Options) -> CompileResult {
    let file_path = if options.path.is_empty() { "<input>".to_string() } else { options.path.clone() };
    let loaded = load_program(source, &file_path, &|p| read_text(p));
    let Some(ast) = loaded.ast.clone() else {
        return CompileResult { ok: false, project: None, errors: loaded.errors, warnings: loaded.warnings, notices: vec![], assets: vec![] };
    };
    let semantic = validate(&ast, source, Some(&loaded.sources));
    let comments = build_comment_map(&ast, &loaded.sources);
    if options.asset_dirs.is_empty() {
        options.asset_dirs = vec![dirname(&host_resolve(".", &file_path))];
    }
    let mut ctx = Context::new(source, loaded.sources.clone(), comments, options);
    ctx.errors.extend(loaded.errors);
    ctx.errors.extend(semantic.errors);
    ctx.warnings.extend(loaded.warnings);
    ctx.warnings.extend(semantic.warnings);

    collect_scenes(&ast, &mut ctx);
    collect_globals(&ast, &mut ctx);
    collect_objects(&ast, &mut ctx);
    collect_functions(&ast, &mut ctx);
    collect_messages(&ast, &mut ctx);

    compile_functions(&mut ctx);
    compile_objects(&mut ctx);

    let project = assemble(&ast, &mut ctx);

    let ok = ctx.errors.is_empty();
    let strict = ctx.options.strict;
    let mut warnings = std::mem::take(&mut ctx.warnings);
    let mut notices = std::mem::take(&mut ctx.notices);
    if strict {
        warnings.extend(notices.drain(..));
    }
    CompileResult {
        ok,
        project: if ok || !strict { Some(project) } else { None },
        warnings: without_duplicates(warnings, &ctx.errors),
        notices: if strict { vec![] } else { without_duplicates(notices, &ctx.errors) },
        errors: std::mem::take(&mut ctx.errors),
        assets: std::mem::take(&mut ctx.asset_files),
    }
}

// ---------------------------------------------------------------- scenes

fn collect_scenes(program: &Value, ctx: &mut Context) {
    for item in list_of(program, "body") {
        if ty(&item) != "Scene" {
            continue;
        }
        let name = s(&item, "name");
        if ctx.scene_by_name.has(&name) {
            let _: Option<()> = ctx.error(&item, format!("'{}' 장면이 이미 있습니다.", name));
            continue;
        }
        let id = ctx.id();
        let display = scene_display_name(&item, ctx).unwrap_or_else(|| name.clone());
        ctx.scenes.push((id.clone(), display.clone()));
        ctx.scene_by_name.set(&name, (id, display));
    }
    if ctx.scenes.is_empty() {
        let id = ctx.id();
        ctx.scenes.insert(0, (id.clone(), DEFAULT_SCENE_NAME.to_string()));
        ctx.scene_by_name.set(DEFAULT_SCENE_NAME, (id, DEFAULT_SCENE_NAME.to_string()));
    }
}

fn scene_display_name(item: &Value, ctx: &mut Context) -> Option<String> {
    let mut value: Option<Value> = None;
    for member in list_of(item, "body") {
        if ty(&member) == "Property" && s(&member, "name") == "name" {
            value = Some(g(&member, "value").clone());
        }
    }
    let value = value?;
    if ty(&value) != "String" {
        return ctx.error(&value, "장면 이름(name)에는 문자열만 쓸 수 있습니다.");
    }
    Some(s(&value, "value"))
}

// ---------------------------------------------------------------- globals

fn collect_globals(program: &Value, ctx: &mut Context) {
    for item in list_of(program, "body") {
        let t = ty(&item).to_string();
        if t == "VarDecl" || t == "ListDecl" {
            if let Some(entry) = make_variable(&item, ctx, None) {
                let info = VarInfo { id: s(&entry, "id"), name: s(&entry, "name"), variable_type: s(&entry, "variableType") };
                ctx.variables.push(entry);
                ctx.globals.set(&s(&item, "name"), info);
            }
        }
        if t == "TableDecl" {
            collect_table(&item, ctx);
        }
    }
}

#[derive(Clone)]
enum Const {
    Num(f64),
    Str(String),
}

impl Const {
    fn number(&self) -> f64 {
        match self {
            Const::Num(n) => *n,
            Const::Str(t) => to_number(t),
        }
    }
    fn string(&self) -> String {
        match self {
            Const::Num(n) => num_to_string(*n),
            Const::Str(t) => t.clone(),
        }
    }
    fn value(&self) -> Value {
        match self {
            Const::Num(n) => num(*n),
            Const::Str(t) => json!(t),
        }
    }
}

fn finite(v: f64) -> Value {
    if v.is_finite() { num(v) } else { NULL }
}

fn constant_of(node: &Value, ctx: &mut Context) -> Option<Const> {
    match ty(node) {
        "Number" => Some(Const::Num(g(node, "value").as_f64().unwrap_or(0.0))),
        "String" | "Color" => Some(Const::Str(s(node, "value"))),
        "Boolean" => Some(Const::Str(boolean_text(g(node, "value").as_bool().unwrap_or(false)).to_string())),
        "Transparent" => Some(Const::Str("transparent".into())),
        "Unary" if s(node, "operator") == "-" && ty(g(node, "argument")) == "Number" => {
            Some(Const::Num(-g(g(node, "argument"), "value").as_f64().unwrap_or(0.0)))
        }
        _ => ctx.error(node, "변수·리스트의 초기값은 상수여야 합니다. 계산이 필요하면 when start 안에서 대입하세요."),
    }
}

fn chart_spec(chart: &Value, node: &Value, columns: usize, ctx: &mut Context) -> Option<Value> {
    let kind = s(chart, "kind");
    if !CHART_KINDS.contains(&kind.as_str()) {
        return ctx.error(node, format!("'{}' 차트는 없습니다. {} 중에서 고르세요.", kind, CHART_KINDS.join(" · ")));
    }
    let mut column = |value: Option<&Value>, ctx: &mut Context| -> Option<i64> {
        let Some(value) = value else { return Some(-1) };
        let constant = constant_of(value, ctx)?;
        let index = constant.number();
        if index.fract() != 0.0 || !index.is_finite() || index < 1.0 || index > columns as f64 {
            let _: Option<()> = ctx.error(
                node,
                format!("테이블 '{}' 에는 {}번째 열이 없습니다. (열은 {}개입니다)", s(node, "name"), num_to_string(index), columns),
            );
            return None;
        }
        Some(index as i64 - 1)
    };
    let x = column(nn(chart, "x"), ctx);
    let y = column(nn(chart, "y"), ctx);
    let (Some(x), Some(y)) = (x, y) else { return None };
    let mut categories = Vec::new();
    for value in list_of(chart, "series") {
        let index = column(Some(&value), ctx)?;
        categories.push(json!(index));
    }
    Some(json!({
        "type": kind,
        "title": chart.get("title").and_then(|t| t.as_str()).unwrap_or(""),
        "xIndex": x,
        "yIndex": y,
        "categoryIndexes": categories,
    }))
}

fn collect_table(node: &Value, ctx: &mut Context) {
    let name = s(node, "name");
    if ctx.table_by_name.has(&name) {
        let _: Option<()> = ctx.error(node, format!("'{}' 테이블이 이미 있습니다.", name));
        return;
    }
    fn cells(row: &[Value], ctx: &mut Context) -> Option<Vec<String>> {
        let values: Vec<Option<Const>> = row.iter().map(|c| constant_of(c, ctx)).collect();
        if values.iter().any(|v| v.is_none()) {
            return None;
        }
        Some(values.into_iter().map(|v| v.unwrap().string()).collect())
    }
    let Some(fields) = cells(&list_of(node, "columns"), ctx) else { return };
    let mut data: Vec<Value> = Vec::new();
    for row in list_of(node, "rows") {
        let row = row.as_array().cloned().unwrap_or_default();
        let Some(values) = cells(&row, ctx) else { return };
        if values.len() != fields.len() {
            let _: Option<()> = ctx.error(
                node,
                format!("테이블 '{}' 의 줄은 열 개수({})와 같아야 합니다. 이 줄은 {}개입니다.", name, fields.len(), values.len()),
            );
            return;
        }
        data.push(json!(values));
    }
    let mut charts = Vec::new();
    for chart in list_of(node, "charts") {
        let Some(built) = chart_spec(&chart, node, fields.len(), ctx) else { return };
        charts.push(built);
    }
    let id = ctx.id();
    let display = nn(node, "displayName").and_then(|d| d.as_str()).map(|d| d.to_string()).unwrap_or_else(|| name.clone());
    let entry = json!({"id": id, "name": display, "object": NULL, "fields": fields, "data": data, "chart": charts});
    ctx.tables.push(entry);
    ctx.table_by_name.set(&name, TableInfo { id });
}

fn make_variable(node: &Value, ctx: &mut Context, object_id: Option<&str>) -> Option<Value> {
    let scope = nn(node, "scope").and_then(|x| x.as_str()).map(|x| x.to_string());
    if scope.is_some() && object_id.is_some() {
        let _: Option<()> = ctx.error(
            node,
            format!("'{}' 는 오브젝트 안의 변수·리스트에는 쓸 수 없습니다. 전역 선언에만 붙일 수 있습니다.", scope.clone().unwrap()),
        );
    }
    let entry_name = nn(node, "displayName").and_then(|d| d.as_str()).map(|d| d.to_string()).unwrap_or_else(|| s(node, "name"));
    let name = if scope.as_deref() == Some("store") { format!("{}{}", STORE_PREFIX, entry_name) } else { entry_name };
    let id = ctx.id();
    let at = nn(node, "at").cloned();
    let (x, y) = match &at {
        Some(a) => {
            let x = constant_of(g(a, "x"), ctx).map(|c| c.number()).unwrap_or(0.0);
            let y = constant_of(g(a, "y"), ctx).map(|c| c.number()).unwrap_or(0.0);
            (x, y)
        }
        None => (0.0, 0.0),
    };
    let mut base = Map::new();
    base.insert("name".into(), json!(name));
    base.insert("id".into(), json!(id));
    base.insert("visible".into(), json!(g(node, "shown").as_bool().unwrap_or(false)));
    base.insert("value".into(), json!(0));
    base.insert("variableType".into(), json!("variable"));
    base.insert("isCloud".into(), json!(scope.as_deref() == Some("shared")));
    base.insert("isRealTime".into(), json!(scope.as_deref() == Some("realtime")));
    base.insert("cloudDate".into(), json!(false));
    base.insert("object".into(), object_id.map(|o| json!(o)).unwrap_or(NULL));
    base.insert("x".into(), finite(x));
    base.insert("y".into(), finite(y));

    if ty(node) == "ListDecl" {
        let value = g(node, "value");
        let elements = if ty(value) == "ListLiteral" { list_of(value, "elements") } else { vec![] };
        let items: Vec<Option<Const>> = elements.iter().map(|e| constant_of(e, ctx)).collect();
        if items.iter().any(|i| i.is_none()) {
            return None;
        }
        base.insert("value".into(), json!(0));
        base.insert("variableType".into(), json!("list"));
        base.insert("array".into(), Value::Array(items.into_iter().map(|i| json!({"data": i.unwrap().value()})).collect()));
        let size = nn(node, "size").cloned();
        let width = match &size {
            Some(sz) => 100f64.max(constant_of(g(sz, "width"), ctx).map(|c| c.number()).unwrap_or(100.0)),
            None => 100.0,
        };
        let height = match &size {
            Some(sz) => 100f64.max(constant_of(g(sz, "height"), ctx).map(|c| c.number()).unwrap_or(120.0)),
            None => 120.0,
        };
        base.insert("width".into(), finite(width));
        base.insert("height".into(), finite(height));
        return Some(Value::Object(base));
    }

    let value = constant_of(g(node, "value"), ctx)?;
    let Some(range) = nn(node, "range").cloned() else {
        base.insert("value".into(), value.value());
        return Some(Value::Object(base));
    };
    let min = constant_of(g(&range, "min"), ctx);
    let max = constant_of(g(&range, "max"), ctx);
    let (Some(min), Some(max)) = (min, max) else { return None };
    if min.number().is_nan() || max.number().is_nan() {
        return ctx.error(node, "슬라이더의 최솟값과 최댓값은 숫자여야 합니다.");
    }
    base.insert("value".into(), value.value());
    base.insert("variableType".into(), json!("slide"));
    base.insert("minValue".into(), num(min.number()));
    base.insert("maxValue".into(), num(max.number()));
    Some(Value::Object(base))
}

// ---------------------------------------------------------------- objects

fn register_object(node: &Value, scene_id: &str, ctx: &mut Context) {
    let name = s(node, "name");
    if ctx.object_by_name.has(&name) {
        let _: Option<()> = ctx.error(node, format!("'{}' 오브젝트가 이미 있습니다.", name));
        return;
    }
    let id = ctx.id();
    ctx.objects.push(CompiledObject {
        id,
        key: name.clone(),
        name: name.clone(),
        kind: s(node, "kind"),
        scene_id: scene_id.to_string(),
        node: node.clone(),
        pictures: OMap::new(),
        sounds: OMap::new(),
        locals: OMap::new(),
        default_picture: None,
        properties: OMap::new(),
        box_size: None,
        center: None,
        script: vec![],
    });
    let idx = ctx.objects.len() - 1;
    ctx.object_by_name.set(&name, idx);
}

fn collect_objects(program: &Value, ctx: &mut Context) {
    for item in list_of(program, "body") {
        if ty(&item) == "Object" {
            let scene = ctx.scenes[0].0.clone();
            register_object(&item, &scene, ctx);
        }
        if ty(&item) == "Scene" {
            let scene = ctx.scene_by_name.get(&s(&item, "name")).map(|x| x.0.clone()).unwrap_or_default();
            for member in list_of(&item, "body") {
                if ty(&member) == "Object" {
                    register_object(&member, &scene, ctx);
                }
            }
        }
    }
    for i in 0..ctx.objects.len() {
        collect_object_members(i, ctx);
    }
}

fn resource_id(member: &Value, ctx: &mut Context) -> Option<String> {
    let Some(force) = nn(member, "forceId").and_then(|f| f.as_str()).map(|f| f.to_string()) else {
        return Some(ctx.id());
    };
    if ctx.new_id.has(&force) {
        return ctx.error(member, format!("force id \"{}\" 는 이미 다른 모양·소리·오브젝트가 쓰고 있습니다.", force));
    }
    ctx.new_id.reserve(&force);
    ctx.forced_resource_ids.insert(force.clone());
    Some(force)
}

fn make_resource(kind: &str, member: &Value, ctx: &mut Context) -> Value {
    let id = match resource_id(member, ctx) {
        Some(id) => id,
        None => ctx.id(),
    };
    let file = s(member, "file");
    let name = nn(member, "displayName").and_then(|d| d.as_str()).map(|d| d.to_string()).unwrap_or_else(|| s(member, "id"));
    let decl = AssetDecl {
        id,
        file: &file,
        name: Some(name),
        width: nn(member, "width").and_then(|v| v.as_f64()),
        height: nn(member, "height").and_then(|v| v.as_f64()),
        duration: nn(member, "duration").and_then(|v| v.as_f64()),
    };
    let mut warnings = Vec::new();
    let probe = Probe { asset_dirs: &ctx.options.asset_dirs, asset_urls: ctx.options.asset_urls };
    let asset = make_asset(kind, decl, &probe, &mut ctx.asset_files, &mut warnings);
    for w in warnings {
        ctx.warn(member, w);
    }
    asset
}

fn collect_object_members(i: usize, ctx: &mut Context) {
    ctx.object = Some(i);
    let node = ctx.objects[i].node.clone();
    let body = list_of(&node, "body");
    let object_id = ctx.objects[i].id.clone();

    for member in &body {
        match ty(member) {
            "Costume" => {
                let asset = make_resource("image", member, ctx);
                let o = &mut ctx.objects[i];
                o.pictures.set(&s(member, "id"), asset.clone());
                if g(member, "isDefault").as_bool().unwrap_or(false) || o.default_picture.is_none() {
                    o.default_picture = Some(asset);
                }
            }
            "Sound" => {
                let asset = make_resource("sound", member, ctx);
                ctx.objects[i].sounds.set(&s(member, "id"), asset);
            }
            "Property" => {
                ctx.objects[i].properties.set(&s(member, "name"), g(member, "value").clone());
            }
            "BoxSize" => {
                ctx.objects[i].box_size = Some((g(member, "width").as_f64().unwrap_or(0.0), g(member, "height").as_f64().unwrap_or(0.0)));
            }
            "Center" => {
                ctx.objects[i].center = Some((g(member, "x").as_f64().unwrap_or(0.0), g(member, "y").as_f64().unwrap_or(0.0)));
            }
            "VarDecl" | "ListDecl" => {
                if let Some(entry) = make_variable(member, ctx, Some(&object_id)) {
                    let info = VarInfo { id: s(&entry, "id"), name: s(&entry, "name"), variable_type: s(&entry, "variableType") };
                    ctx.variables.push(entry);
                    ctx.objects[i].locals.set(&s(member, "name"), info);
                }
            }
            _ => {}
        }
    }

    for member in &body {
        let t = ty(member);
        if t != "Costume" && t != "Sound" {
            continue;
        }
        let Some(display) = nn(member, "displayName").and_then(|d| d.as_str()) else { continue };
        if display.is_empty() || display == s(member, "id") {
            continue;
        }
        let o = &mut ctx.objects[i];
        let shelf = if t == "Costume" { &mut o.pictures } else { &mut o.sounds };
        if !shelf.has(display) {
            if let Some(a) = shelf.get(&s(member, "id")).cloned() {
                shelf.set(display, a);
            }
        }
    }

    for member in &body {
        if ty(member) == "Event" {
            collect_handler_variables(&list_of(member, "body"), i, ctx);
        }
    }
    ctx.object = None;
}

fn collect_handler_variables(statements: &[Value], i: usize, ctx: &mut Context) {
    let object_id = ctx.objects[i].id.clone();
    for statement in statements {
        let t = ty(statement);
        if t == "VarDecl" || t == "ListDecl" {
            let name = s(statement, "name");
            if !ctx.objects[i].locals.has(&name) && !ctx.globals.has(&name) {
                let node = if t == "VarDecl" {
                    let mut copy = statement.clone();
                    copy["value"] = json!({"type": "Number", "value": 0, "loc": g(statement, "loc")});
                    copy
                } else {
                    statement.clone()
                };
                if let Some(entry) = make_variable(&node, ctx, Some(&object_id)) {
                    let info = VarInfo { id: s(&entry, "id"), name: s(&entry, "name"), variable_type: s(&entry, "variableType") };
                    ctx.variables.push(entry);
                    ctx.objects[i].locals.set(&name, info);
                }
            }
        }
        for key in ["consequent", "alternate", "body"] {
            if let Some(Value::Array(block)) = statement.get(key) {
                collect_handler_variables(block, i, ctx);
            }
        }
    }
}

// ---------------------------------------------------------------- functions

fn find_returns(statements: &[Value], found: &mut Vec<Value>) {
    for statement in statements {
        if ty(statement) == "Return" {
            found.push(statement.clone());
        }
        for key in ["consequent", "alternate", "body"] {
            if let Some(Value::Array(block)) = statement.get(key) {
                find_returns(block, found);
            }
        }
    }
}

fn register_function(node: &Value, owner: Option<&str>, ctx: &mut Context) {
    let name = s(node, "name");
    if ctx.function_by_name.has(&name) {
        let _: Option<()> = ctx.error(node, format!("'{}' 함수가 이미 있습니다. 엔트리 함수는 작품 전체에서 이름이 하나여야 합니다.", name));
        return;
    }
    let body = list_of(node, "body");
    let mut returns = Vec::new();
    find_returns(&body, &mut returns);
    let is_tail_return = body.last().map(|t| ty(t) == "Return").unwrap_or(false) && returns.len() == 1;
    if !returns.is_empty() && !is_tail_return {
        let _: Option<()> = ctx.error(&returns[0], "엔트리 함수는 중간에서 값을 돌려줄 수 없습니다. return 은 함수의 마지막 문장에만 쓸 수 있습니다.");
    }
    let params: Vec<String> = list_of(node, "params").iter().map(|p| p.as_str().unwrap_or("").to_string()).collect();
    let booleans: std::collections::HashSet<String> =
        list_of(node, "booleanParams").iter().map(|p| p.as_str().unwrap_or("").to_string()).collect();
    let id = ctx.id();
    let mut param_types = OMap::new();
    for p in &params {
        let kind = if booleans.contains(p) { "booleanParam" } else { "stringParam" };
        let type_id = format!("{}_{}", kind, ctx.id());
        param_types.set(p, type_id);
    }
    ctx.functions.push(CompiledFunction {
        id,
        name: name.clone(),
        node: Some(node.clone()),
        owner: owner.map(|o| o.to_string()),
        params,
        boolean_params: booleans,
        param_types,
        is_value: is_tail_return,
        local_variables: vec![],
        generated: false,
        ty: None,
        content: vec![],
    });
    let idx = ctx.functions.len() - 1;
    ctx.function_by_name.set(&name, idx);
}

fn collect_functions(program: &Value, ctx: &mut Context) {
    for item in list_of(program, "body") {
        match ty(&item) {
            "FunctionDecl" => register_function(&item, None, ctx),
            "Object" => {
                for member in list_of(&item, "body") {
                    if ty(&member) == "FunctionDecl" {
                        register_function(&member, Some(&s(&item, "name")), ctx);
                    }
                }
            }
            "Scene" => {
                for object in list_of(&item, "body") {
                    if ty(&object) != "Object" {
                        continue;
                    }
                    for member in list_of(&object, "body") {
                        if ty(&member) == "FunctionDecl" {
                            register_function(&member, Some(&s(&object, "name")), ctx);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_messages(node: &Value, ctx: &mut Context) {
    match node {
        Value::Array(a) => a.iter().for_each(|c| collect_messages(c, ctx)),
        Value::Object(m) => {
            let t = m.get("type").and_then(|t| t.as_str());
            if t == Some("Send") {
                if let Some(sig) = m.get("signal") {
                    if ty(sig) == "String" {
                        ctx.message_id(&s(sig, "value"));
                    }
                }
            }
            if t == Some("Event") && m.get("event").and_then(|e| e.as_str()) == Some("signal") {
                if let Some(sig) = m.get("signal").and_then(|x| x.as_str()) {
                    ctx.message_id(sig);
                }
            }
            for (k, v) in m {
                if k != "loc" {
                    collect_messages(v, ctx);
                }
            }
        }
        _ => {}
    }
}

fn collect_function_locals(statements: &[Value], ctx: &mut Context, fid: &str) {
    for statement in statements {
        let t = ty(statement);
        if t == "VarDecl" {
            let name = s(statement, "name");
            let exists = ctx.func_scope.as_ref().map(|sc| sc.local_vars.has(&name)).unwrap_or(false);
            if !exists {
                let id = format!("{}_{}", fid, ctx.id());
                let display = nn(statement, "displayName").and_then(|d| d.as_str()).map(|d| d.to_string()).unwrap_or_else(|| name.clone());
                if let Some(sc) = ctx.func_scope.as_mut() {
                    sc.local_vars.set(&name, (id, display));
                }
            }
        }
        if t == "ListDecl" {
            let _: Option<()> = ctx.error(statement, "함수 안에서는 리스트를 선언할 수 없습니다. 전역 리스트를 쓰세요.");
        }
        for key in ["consequent", "alternate", "body"] {
            if let Some(Value::Array(block)) = statement.get(key) {
                collect_function_locals(block, ctx, fid);
            }
        }
    }
}

fn build_function_fields(idx: usize, ctx: &mut Context) -> Value {
    let (params, booleans, types, name) = {
        let f = &ctx.functions[idx];
        (f.params.clone(), f.boolean_params.clone(), f.param_types.clone(), f.name.clone())
    };
    let mut next = NULL;
    for i in (0..params.len()).rev() {
        let pname = &params[i];
        let type_name = types.get(pname).cloned().unwrap_or_default();
        if booleans.contains(pname) {
            let inner = ctx.block(&type_name, vec![NULL]);
            next = ctx.block("function_field_boolean", vec![inner, next]);
        } else {
            let inner = ctx.block(&type_name, vec![]);
            next = ctx.block("function_field_string", vec![inner, next]);
        }
        if *pname != crate::core::auto_param_name(i) {
            next = ctx.block("function_field_label", vec![json!(pname), next]);
        }
    }
    ctx.block("function_field_label", vec![json!(name), next])
}

fn set_xy(block: &mut Value, x: f64, y: f64) {
    block["x"] = json!(x);
    block["y"] = json!(y);
}

fn compile_functions(ctx: &mut Context) {
    let mut i = 0usize;
    while i < ctx.functions.len() {
        let idx = i;
        i += 1;
        if ctx.functions[idx].generated {
            continue;
        }
        let (owner, fname, param_types, is_value, declared, fid) = {
            let f = &ctx.functions[idx];
            (f.owner.clone(), f.name.clone(), f.param_types.clone(), f.is_value, f.node.clone().unwrap_or(NULL), f.id.clone())
        };
        ctx.object = owner.and_then(|o| ctx.object_by_name.get(&o).copied());
        ctx.locals = ctx.object.map(|o| ctx.objects[o].locals.clone()).unwrap_or_default();
        ctx.func_scope = Some(FunctionScope { name: fname, params: param_types, local_vars: OMap::new() });

        let body = list_of(&declared, "body");
        collect_function_locals(&body, ctx, &fid);

        let compiled_body: &[Value] = if is_value { &body[..body.len().saturating_sub(1)] } else { &body[..] };
        let statements = compile_statements(compiled_body, ctx);
        let tail = body.last().cloned();
        let return_value = match (&tail, is_value) {
            (Some(t), true) => compile_value(g(t, "value"), ctx).unwrap_or(NULL),
            _ => NULL,
        };

        let field = build_function_fields(idx, ctx);
        let params = if is_value { vec![field, NULL, NULL, return_value] } else { vec![field, NULL] };
        let mut create = ctx.block_with(if is_value { "function_create_value" } else { "function_create" }, params, vec![statements]);
        set_xy(&mut create, 50.0, 30.0);

        let locals: Vec<Value> = ctx
            .func_scope
            .as_ref()
            .map(|sc| sc.local_vars.values().map(|(id, display)| json!({"id": id, "name": display, "value": 0})).collect())
            .unwrap_or_default();
        let f = &mut ctx.functions[idx];
        f.local_variables = locals;
        f.content = vec![vec![create]];
        f.ty = Some(if is_value { "value" } else { "normal" }.to_string());

        ctx.func_scope = None;
        ctx.locals = OMap::new();
        ctx.object = None;
    }
}

// ---------------------------------------------------------------- scripts

fn compile_objects(ctx: &mut Context) {
    for i in 0..ctx.objects.len() {
        ctx.object = Some(i);
        ctx.locals = ctx.objects[i].locals.clone();
        ctx.func_scope = None;

        let mut row = 0usize;
        if let Some(mut defaults) = compile_brush_defaults(i, ctx) {
            set_xy(&mut defaults[0], 50.0, 30.0);
            row += 1;
            ctx.objects[i].script.push(defaults);
        }
        let node = ctx.objects[i].node.clone();
        for member in list_of(&node, "body") {
            if ty(&member) != "Event" {
                continue;
            }
            let Some(mut thread) = compile_event(&member, ctx) else { continue };
            ctx.apply_comment(&member, &mut thread[0]);
            set_xy(&mut thread[0], 50.0, (30 + row * 260) as f64);
            row += 1;
            ctx.objects[i].script.push(thread);
        }
        ctx.object = None;
        ctx.locals = OMap::new();
    }
}

fn unknown_key(event: &Value, ctx: &mut Context) {
    let key = s(event, "key");
    let names = key_names();
    let hint = did_you_mean(&key, names.iter().map(|k| k.as_str()));
    let _: Option<()> = ctx.error(event, format!("알 수 없는 키 이름 \"{}\" 입니다.{}", key, hint));
}

fn compile_event(event: &Value, ctx: &mut Context) -> Option<Vec<Value>> {
    let body = list_of(event, "body");
    let kind = s(event, "event");
    let simple = |name: &str, ctx: &mut Context| -> Option<Vec<Value>> {
        let head = ctx.block(name, vec![NULL]);
        let mut out = vec![head];
        out.extend(compile_statements(&body, ctx));
        Some(out)
    };
    match kind.as_str() {
        "start" => simple("when_run_button_click", ctx),
        "scene_start" => simple("when_scene_start", ctx),
        "click" => simple("when_object_click", ctx),
        "click_up" => simple("when_object_click_canceled", ctx),
        "stage_click" => simple("mouse_clicked", ctx),
        "stage_click_up" => simple("mouse_click_cancled", ctx),
        "cloned" => simple("when_clone_start", ctx),
        "signal" => {
            let id = ctx.message_id(&s(event, "signal"));
            let head = ctx.block("when_message_cast", vec![NULL, json!(id)]);
            let mut out = vec![head];
            out.extend(compile_statements(&body, ctx));
            Some(out)
        }
        "key" => {
            let Some(code) = key_code(&s(event, "key")) else {
                unknown_key(event, ctx);
                return None;
            };
            let head = ctx.block("when_some_key_pressed", vec![NULL, json!(code)]);
            let mut out = vec![head];
            out.extend(compile_statements(&body, ctx));
            Some(out)
        }
        "key_up" => {
            let Some(code) = key_code(&s(event, "key")) else {
                unknown_key(event, ctx);
                return None;
            };
            let p1 = ctx.block("is_press_some_key", vec![json!(code), NULL]);
            let w1 = ctx.block("wait_until_true", vec![p1, NULL]);
            let p2 = ctx.block("is_press_some_key", vec![json!(code), NULL]);
            let not = ctx.block("boolean_not", vec![NULL, p2, NULL]);
            let w2 = ctx.block("wait_until_true", vec![not, NULL]);
            let mut inner = vec![w1, w2];
            inner.extend(compile_statements(&body, ctx));
            let repeat = ctx.block_with("repeat_inf", vec![NULL, NULL], vec![inner]);
            let head = ctx.block("when_run_button_click", vec![NULL]);
            Some(vec![head, repeat])
        }
        _ => {
            let _: Option<()> = ctx.error(event, format!("'{}' 이벤트는 엔트리 블록으로 바꿀 수 없습니다.", kind));
            None
        }
    }
}

fn compile_brush_defaults(i: usize, ctx: &mut Context) -> Option<Vec<Value>> {
    let present: Vec<&str> = BRUSH_DEFAULT_PROPERTIES.iter().copied().filter(|n| ctx.objects[i].properties.has(n)).collect();
    if present.is_empty() {
        return None;
    }
    let mut blocks = vec![ctx.block("when_run_button_click", vec![NULL])];
    for name in present {
        let value = ctx.objects[i].properties.get(name).cloned().unwrap_or(NULL);
        let loc = g(&value, "loc").clone();
        let assign = json!({
            "type": "Assign",
            "operator": "=",
            "target": {"type": "Identifier", "name": name, "loc": loc},
            "value": value,
            "loc": loc,
        });
        blocks.extend(compile_statement(&assign, ctx));
    }
    Some(blocks)
}

// ---------------------------------------------------------------- assembly

fn field_value(program: &Value, name: &str) -> Option<Value> {
    let mut found = None;
    for item in list_of(program, "body") {
        if ty(&item) == "Project" {
            for f in list_of(&item, "fields") {
                if s(&f, "field") == name {
                    found = Some(g(&f, "value").clone());
                }
            }
            break;
        }
    }
    found
}

fn assemble(program: &Value, ctx: &mut Context) -> Value {
    let text_field = |name: &str| -> Option<String> {
        let v = field_value(program, name)?;
        if ty(&v) == "String" { Some(s(&v, "value")) } else { None }
    };
    let number_field = |name: &str| -> Option<f64> {
        let v = field_value(program, name)?;
        if ty(&v) == "Number" { g(&v, "value").as_f64() } else { None }
    };

    let objects: Vec<Value> = (0..ctx.objects.len()).map(|i| build_object(i, ctx)).collect();
    add_system_variables(ctx);

    let first_id = objects.first().map(|o| g(o, "id").clone()).unwrap_or(NULL);
    let scenes: Vec<Value> = ctx.scenes.iter().map(|(id, name)| json!({"id": id, "name": name})).collect();
    let messages: Vec<Value> = ctx.messages.iter().map(|(name, id)| json!({"id": id, "name": name})).collect();
    let functions: Vec<Value> = ctx
        .functions
        .iter()
        .map(|f| {
            let content = Value::Array(f.content.iter().map(|c| Value::Array(c.clone())).collect());
            json!({
                "id": f.id,
                "type": f.ty.clone().unwrap_or_else(|| "normal".into()),
                "localVariables": f.local_variables,
                "useLocalVariables": !f.local_variables.is_empty(),
                "content": js_stringify(&content),
            })
        })
        .collect();

    let mut project = Map::new();
    project.insert("objects".into(), Value::Array(objects));
    project.insert("scenes".into(), Value::Array(scenes));
    project.insert("variables".into(), Value::Array(ctx.variables.clone()));
    project.insert("messages".into(), Value::Array(messages));
    project.insert("functions".into(), Value::Array(functions));
    project.insert("tables".into(), Value::Array(ctx.tables.clone()));
    project.insert("speed".into(), num(number_field("fps").unwrap_or(60.0)));
    project.insert("interface".into(), json!({"menuWidth": 280, "canvasWidth": 480, "object": first_id}));
    project.insert("expansionBlocks".into(), json!(ctx.expansion_blocks));
    project.insert("aiUtilizeBlocks".into(), json!(ctx.ai_utilize_blocks));
    project.insert("hardwareLiteBlocks".into(), json!([]));
    project.insert("externalModules".into(), json!([]));
    project.insert("externalModulesLite".into(), json!([]));
    let name = ctx.options.name.clone().or_else(|| text_field("title")).unwrap_or_else(|| "Tess 작품".to_string());
    project.insert("name".into(), json!(name));
    project.insert("isPracticalCourse".into(), json!(false));
    if let Some(d) = text_field("description") {
        if !d.is_empty() {
            project.insert("description".into(), json!(d));
        }
    }
    Value::Object(project)
}

fn num_prop(props: &OMap<Value>, name: &str, ctx: &mut Context) -> Option<f64> {
    let value = props.get(name)?;
    if ty(value) == "Number" {
        return g(value, "value").as_f64();
    }
    if ty(value) == "Unary" && s(value, "operator") == "-" && ty(g(value, "argument")) == "Number" {
        return g(g(value, "argument"), "value").as_f64().map(|v| -v);
    }
    let _: Option<()> = ctx.error(value, format!("속성 '{}' 에는 숫자만 쓸 수 있습니다.", name));
    None
}

fn str_prop(props: &OMap<Value>, name: &str, ctx: &mut Context) -> Option<String> {
    let value = props.get(name)?;
    match ty(value) {
        "String" | "Color" => Some(s(value, "value")),
        "Transparent" => Some("transparent".into()),
        "Keyword" | "Identifier" => Some(s(value, "name")),
        _ => {
            let _: Option<()> = ctx.error(value, format!("속성 '{}' 에는 문자열만 쓸 수 있습니다.", name));
            None
        }
    }
}

fn bool_prop(props: &OMap<Value>, name: &str, ctx: &mut Context) -> Option<bool> {
    let value = props.get(name)?;
    if ty(value) == "Boolean" {
        return g(value, "value").as_bool();
    }
    let _: Option<()> = ctx.error(value, format!("속성 '{}' 에는 true 또는 false 만 쓸 수 있습니다.", name));
    None
}

fn unique_by_id(shelf: &OMap<Value>) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for v in shelf.values() {
        let id = s(v, "id");
        if !seen.contains(&id) {
            seen.push(id);
            out.push(v.clone());
        }
    }
    out
}

fn build_object(i: usize, ctx: &mut Context) -> Value {
    let (props, is_text, picture, key, id, scene_id, node, box_size, center, script, pictures, sounds) = {
        let o = &ctx.objects[i];
        (
            o.properties.clone(),
            o.kind == "text",
            o.default_picture.clone(),
            o.key.clone(),
            o.id.clone(),
            o.scene_id.clone(),
            o.node.clone(),
            o.box_size,
            o.center,
            o.script.clone(),
            unique_by_id(&o.pictures),
            unique_by_id(&o.sounds),
        )
    };
    let size = num_prop(&props, "size", ctx);
    let scale_x = num_prop(&props, "scale_x", ctx).unwrap_or(size.unwrap_or(100.0)) / 100.0;
    let scale_y = num_prop(&props, "scale_y", ctx).unwrap_or(size.unwrap_or(100.0)) / 100.0;

    let x = num_prop(&props, "x", ctx).unwrap_or(0.0);
    let y = num_prop(&props, "y", ctx).unwrap_or(0.0);
    let rotation = num_prop(&props, "angle", ctx).unwrap_or(0.0);
    let direction = num_prop(&props, "way", ctx).unwrap_or(90.0);
    let visible = bool_prop(&props, "visible", ctx).unwrap_or(true);

    let mut entity = Map::new();
    entity.insert("x".into(), num(x));
    entity.insert("y".into(), num(y));
    entity.insert("regX".into(), json!(0));
    entity.insert("regY".into(), json!(0));
    entity.insert("scaleX".into(), num(scale_x));
    entity.insert("scaleY".into(), num(scale_y));
    entity.insert("rotation".into(), num(rotation));
    entity.insert("direction".into(), num(direction));
    entity.insert("width".into(), json!(100));
    entity.insert("height".into(), json!(100));
    entity.insert("font".into(), json!("undefinedpx "));
    entity.insert("visible".into(), json!(visible));

    if !is_text {
        let dim = picture.as_ref().and_then(|p| p.get("dimension"));
        let width = dim.and_then(|d| d.get("width")).and_then(|v| v.as_f64()).unwrap_or(100.0);
        let height = dim.and_then(|d| d.get("height")).and_then(|v| v.as_f64()).unwrap_or(100.0);
        entity.insert("regX".into(), num(center.map(|c| c.0).unwrap_or(width / 2.0)));
        entity.insert("regY".into(), num(center.map(|c| c.1).unwrap_or(height / 2.0)));
        entity.insert("width".into(), num(width));
        entity.insert("height".into(), num(height));
    }

    let name = str_prop(&props, "name", ctx).unwrap_or(key.clone());
    let rotate_method = str_prop(&props, "rotation", ctx).unwrap_or_else(|| "free".into());
    let lock = bool_prop(&props, "lock", ctx).unwrap_or(false);

    let mut result = Map::new();
    result.insert("id".into(), json!(id));
    result.insert("name".into(), json!(name));
    result.insert("script".into(), json!(js_stringify(&Value::Array(script.into_iter().map(Value::Array).collect()))));
    result.insert("objectType".into(), json!(if is_text { "textBox" } else { "sprite" }));
    result.insert("rotateMethod".into(), json!(rotate_method));
    result.insert("scene".into(), json!(scene_id));
    result.insert("sprite".into(), json!({"pictures": pictures, "sounds": sounds}));
    result.insert("lock".into(), json!(lock));

    let mut text_value: Option<String> = None;
    let mut selected: Option<Value> = None;
    if is_text {
        let text = str_prop(&props, "text_content", ctx).unwrap_or_default();
        let font_size = num_prop(&props, "font_size", ctx).unwrap_or(20.0);
        let family = str_prop(&props, "font", ctx).unwrap_or_else(|| DEFAULT_FONT.to_string());
        let bold = bool_prop(&props, "text_bold", ctx).unwrap_or(false);
        let italic = bool_prop(&props, "text_italic", ctx).unwrap_or(false);
        let align_name = str_prop(&props, "text_align", ctx).unwrap_or_else(|| "center".into());
        let align = match align_name.as_str() {
            "center" => 0,
            "left" => 1,
            "right" => 2,
            _ => 0,
        };
        let mut font_parts: Vec<String> = Vec::new();
        if bold {
            font_parts.push("bold".into());
        }
        if italic {
            font_parts.push("italic".into());
        }
        font_parts.push(format!("{}px", num_to_string(font_size)));
        if !family.is_empty() {
            font_parts.push(family);
        }
        entity.insert("font".into(), json!(font_parts.join(" ")));
        let colour = str_prop(&props, "font_color", ctx).unwrap_or_else(|| "#000000".into());
        entity.insert("colour".into(), json!(colour));
        entity.insert("text".into(), json!(text));
        entity.insert("textAlign".into(), json!(align));
        let line_break = bool_prop(&props, "line_break", ctx).unwrap_or(false);
        entity.insert("lineBreak".into(), json!(line_break));
        let bg = str_prop(&props, "bg_color", ctx).unwrap_or_else(|| "#ffffff".into());
        entity.insert("bgColor".into(), json!(bg));
        let underline = bool_prop(&props, "text_underline", ctx).unwrap_or(false);
        entity.insert("underLine".into(), json!(underline));
        let strike = bool_prop(&props, "text_strikethrough", ctx).unwrap_or(false);
        entity.insert("strike".into(), json!(strike));
        entity.insert("fontSize".into(), num(font_size));
        let text_len = text.encode_utf16().count() as f64;
        let width = box_size.map(|b| b.0).unwrap_or_else(|| (text_len * font_size * 0.85).max(font_size));
        let height = box_size.map(|b| b.1).unwrap_or_else(|| round(font_size * 1.1));
        entity.insert("width".into(), num(width));
        entity.insert("height".into(), num(height));
        text_value = Some(text);
    } else {
        selected = Some(picture.as_ref().and_then(|p| p.get("id").cloned()).unwrap_or(NULL));
        if picture.is_none() {
            ctx.warn(&node, format!("오브젝트 '{}' 에 모양(costume)이 없습니다.", key));
        }
    }
    result.insert("entity".into(), Value::Object(entity));
    if let Some(t) = text_value {
        result.insert("text".into(), json!(t));
    }
    if let Some(sel) = selected {
        result.insert("selectedPictureId".into(), sel);
    }
    Value::Object(result)
}

fn add_system_variables(ctx: &mut Context) {
    let id = ctx.id();
    ctx.variables.push(json!({
        "name": "초시계", "id": id, "visible": false, "value": 0, "variableType": "timer",
        "isCloud": false, "isRealTime": false, "cloudDate": false, "object": NULL, "x": 134, "y": -70
    }));
    let id = ctx.id();
    ctx.variables.push(json!({
        "name": "대답", "id": id, "visible": false, "value": 0, "variableType": "answer",
        "isCloud": false, "isRealTime": false, "cloudDate": false, "object": NULL, "x": 150, "y": -100
    }));
}
