//! Compiler-generated Entry functions (scale setters, power refiner, save manager).
use crate::ctx::*;
use serde_json::{json, Value};

pub const STORE_PREFIX: &str = "@";
pub const STORE_SAVE_FUNCTION: &str = "@저장";
pub const STORE_SAVE_ASYNC_FUNCTION: &str = "@비동기저장";
pub const STORE_FLAG_VARIABLE: &str = "@확장프로그램";

const NULL: Value = Value::Null;
const MEASURE: f64 = 100000.0;

pub fn is_store_flag(name: &str) -> bool {
    name.split_whitespace().collect::<String>() == STORE_FLAG_VARIABLE
}

/// 'wait' / 'async' / None for a function name that is a save function.
pub fn save_function_kind(name: &str) -> Option<&'static str> {
    let mut clean = name.trim().to_string();
    if let Some(i) = clean.rfind('(') {
        if clean.ends_with(')') && !clean[i + 1..clean.len() - 1].contains(')') {
            clean = clean[..i].trim_end().to_string();
        }
    }
    if let Some(rest) = clean.strip_prefix('@') {
        clean = format!("@{}", rest.trim_start());
    }
    if clean == STORE_SAVE_FUNCTION {
        Some("wait")
    } else if clean == STORE_SAVE_ASYNC_FUNCTION {
        Some("async")
    } else {
        None
    }
}

fn calc(ctx: &mut Context, l: Value, op: &str, r: Value) -> Value {
    ctx.block("calc_basic", vec![l, json!(op), r])
}

fn field_chain(ctx: &mut Context, label: &str, params: &[String], types: &OMap<String>) -> Value {
    let mut next = NULL;
    for name in params.iter().rev() {
        let t = types.get(name).unwrap().clone();
        let inner = ctx.block(&t, vec![]);
        next = ctx.block("function_field_string", vec![inner, next]);
    }
    ctx.block("function_field_label", vec![json!(label), next])
}

pub fn require_scale_setter(property: &str, ctx: &mut Context) -> usize {
    if let Some(&i) = ctx.runtime_functions.get(property) {
        return i;
    }
    let (setter, probe, label) = if property == "scale_x" {
        ("WIDTH", "HEIGHT", "[Tess] 가로 비율 정하기")
    } else {
        ("HEIGHT", "WIDTH", "[Tess] 세로 비율 정하기")
    };
    let id = ctx.id();
    let params: Vec<String> = vec!["비율".into(), "원래 배율".into()];
    let mut param_types: OMap<String> = OMap::new();
    for p in &params {
        let t = format!("stringParam_{}", ctx.id());
        param_types.set(p, t);
    }
    let mut locals: OMap<String> = OMap::new();
    let mut local_variables = Vec::new();
    for name in ["현재 크기", "지금 축", "원래 크기", "원래 축"] {
        let vid = format!("{}_{}", id, ctx.id());
        locals.set(name, vid.clone());
        local_variables.push(json!({"id": vid, "name": name, "value": 0}));
    }
    macro_rules! size { () => { ctx.block("coordinate_object", vec![NULL, json!("self"), NULL, json!("size")]) }; }
    macro_rules! local { ($n:expr) => {{ let lid = locals.get($n).unwrap().clone(); ctx.block("get_func_variable", vec![json!(lid), NULL]) }}; }
    macro_rules! set_local { ($n:expr, $v:expr) => {{ let lid = locals.get($n).unwrap().clone(); let v = $v; ctx.block("set_func_variable", vec![json!(lid), v, NULL]) }}; }
    macro_rules! param { ($n:expr) => {{ let t = param_types.get($n).unwrap().clone(); ctx.block(&t, vec![]) }}; }
    macro_rules! number { ($v:expr) => { ctx.number($v) }; }
    macro_rules! stretch { ($d:expr, $v:expr) => {{ let v = $v; ctx.block("stretch_scale_size", vec![json!($d), v, NULL]) }}; }
    macro_rules! measured { ($baseline:expr) => {{
        let two = number!(2.0);
        let l1 = local!($baseline);
        let a = calc(ctx, two, "MULTI", l1);
        let s1 = size!();
        let l2 = local!($baseline);
        let diff = calc(ctx, s1, "MINUS", l2);
        let b = calc(ctx, a, "MULTI", diff);
        let k = number!(1.0 / MEASURE);
        calc(ctx, b, "MULTI", k)
    }}; }

    let mut body: Vec<Value> = Vec::new();
    body.push(set_local!("현재 크기", size!()));
    body.push(stretch!(probe, number!(MEASURE)));
    body.push(set_local!("지금 축", measured!("현재 크기")));
    body.push(ctx.block("reset_scale_size", vec![NULL]));
    body.push(set_local!("원래 크기", size!()));
    body.push(stretch!(probe, number!(MEASURE)));
    body.push(set_local!("원래 축", measured!("원래 크기")));
    body.push(ctx.block("reset_scale_size", vec![NULL]));
    {
        let s1 = size!();
        let l1 = local!("지금 축");
        let a = calc(ctx, s1, "MULTI", l1);
        let l2 = local!("원래 축");
        let d = calc(ctx, a, "DIVIDE", l2);
        body.push(ctx.block("set_scale_size", vec![d, NULL]));
    }
    {
        let s1 = size!();
        let p1 = param!("비율");
        let l1 = local!("원래 축");
        let a = calc(ctx, p1, "MULTI", l1);
        let hundred = number!(100.0);
        let p2 = param!("원래 배율");
        let b = calc(ctx, hundred, "MULTI", p2);
        let l2 = local!("지금 축");
        let c = calc(ctx, b, "MULTI", l2);
        let d = calc(ctx, a, "DIVIDE", c);
        let one = number!(1.0);
        let e = calc(ctx, d, "MINUS", one);
        let f = calc(ctx, s1, "MULTI", e);
        body.push(stretch!(setter, f));
    }
    let field = field_chain(ctx, label, &params, &param_types);
    let mut create = ctx.block_with("function_create", vec![field, NULL], vec![body]);
    create["x"] = json!(50);
    create["y"] = json!(30);
    let fnc = CompiledFunction {
        id,
        name: label.to_string(),
        node: None,
        owner: None,
        params,
        boolean_params: Default::default(),
        param_types,
        is_value: false,
        local_variables,
        generated: true,
        ty: Some("normal".into()),
        content: vec![vec![create]],
    };
    ctx.functions.push(fnc);
    let idx = ctx.functions.len() - 1;
    ctx.runtime_functions.set(property, idx);
    idx
}

/// Returns the id of the power refiner function, creating it on first use.
pub fn require_power_refiner(ctx: &mut Context) -> String {
    if let Some(&i) = ctx.runtime_functions.get("power") {
        return ctx.functions[i].id.clone();
    }
    let id = ctx.id();
    let params: Vec<String> = vec!["어림값".into(), "밑".into(), "지수".into()];
    let mut param_types: OMap<String> = OMap::new();
    for p in &params {
        let t = format!("stringParam_{}", ctx.id());
        param_types.set(p, t);
    }
    macro_rules! param { ($n:expr) => {{ let t = param_types.get($n).unwrap().clone(); ctx.block(&t, vec![]) }}; }
    macro_rules! ln { ($v:expr) => {{ let v = $v; ctx.block("calc_operation", vec![NULL, v, NULL, json!("ln")]) }}; }
    let p1 = param!("어림값");
    let one = ctx.number(1.0);
    let pe = param!("지수");
    let pb = param!("밑");
    let lnb = ln!(pb);
    let m = calc(ctx, pe, "MULTI", lnb);
    let plus = calc(ctx, one, "PLUS", m);
    let pa = param!("어림값");
    let lna = ln!(pa);
    let minus = calc(ctx, plus, "MINUS", lna);
    let refined = calc(ctx, p1, "MULTI", minus);
    let field = field_chain(ctx, "[Tess] 거듭제곱 다듬기", &params, &param_types);
    let mut create = ctx.block_with("function_create_value", vec![field, NULL, NULL, refined], vec![vec![]]);
    create["x"] = json!(50);
    create["y"] = json!(30);
    let fnc = CompiledFunction {
        id: id.clone(),
        name: "[Tess] 거듭제곱 다듬기".into(),
        node: None,
        owner: None,
        params,
        boolean_params: Default::default(),
        param_types,
        is_value: true,
        local_variables: vec![],
        generated: true,
        ty: Some("value".into()),
        content: vec![vec![create]],
    };
    ctx.functions.push(fnc);
    let idx = ctx.functions.len() - 1;
    ctx.runtime_functions.set("power", idx);
    id
}

pub fn require_save_function(is_async: bool, ctx: &mut Context) -> String {
    let key = if is_async { "save-async" } else { "save" };
    if let Some(&i) = ctx.runtime_functions.get(key) {
        return ctx.functions[i].id.clone();
    }
    let name = if is_async { STORE_SAVE_ASYNC_FUNCTION } else { STORE_SAVE_FUNCTION };
    let want = if is_async { "async" } else { "wait" };
    if let Some(i) = ctx.functions.iter().position(|f| f.name == name || save_function_kind(&f.name) == Some(want)) {
        ctx.runtime_functions.set(key, i);
        return ctx.functions[i].id.clone();
    }
    let id = ctx.id();
    let label = ctx.block("function_field_label", vec![json!(name), NULL]);
    let mut create = ctx.block_with("function_create", vec![label, NULL], vec![vec![]]);
    create["x"] = json!(50);
    create["y"] = json!(30);
    let fnc = CompiledFunction {
        id: id.clone(),
        name: name.into(),
        node: None,
        owner: None,
        params: vec![],
        boolean_params: Default::default(),
        param_types: OMap::new(),
        is_value: false,
        local_variables: vec![],
        generated: true,
        ty: Some("normal".into()),
        content: vec![vec![create]],
    };
    ctx.functions.push(fnc);
    let idx = ctx.functions.len() - 1;
    ctx.runtime_functions.set(key, idx);
    id
}

/// Returns the id of the `@확장프로그램` flag variable, adding it when missing.
pub fn require_store_flag(ctx: &mut Context) -> String {
    for v in &ctx.variables {
        let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if is_store_flag(name) && v.get("object").map(|o| o.is_null()).unwrap_or(false) {
            return v["id"].as_str().unwrap().to_string();
        }
    }
    let id = ctx.id();
    ctx.variables.push(json!({
        "name": STORE_FLAG_VARIABLE, "id": id, "visible": false, "value": 0, "variableType": "variable",
        "isCloud": false, "isRealTime": false, "cloudDate": false, "object": null, "x": 0, "y": 0
    }));
    id
}
