//! Tess expressions to Entry value/judgement blocks.
use crate::core::*;
use crate::ctx::*;
use crate::jsnum::num_to_string;
use crate::runtime::{require_power_refiner, require_store_flag};
use serde_json::{json, Value};

const NULL: Value = Value::Null;

fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn bool_text(b: bool) -> &'static str {
    if b { "TRUE" } else { "FALSE" }
}

pub fn boolean_text(b: bool) -> &'static str {
    bool_text(b)
}

fn compare_op(op: &str) -> Option<&'static str> {
    Some(match op {
        "==" => "EQUAL", "!=" => "NOT_EQUAL", ">" => "GREATER", "<" => "LESS",
        ">=" => "GREATER_OR_EQUAL", "<=" => "LESS_OR_EQUAL", _ => return None,
    })
}

fn arithmetic_op(op: &str) -> Option<&'static str> {
    Some(match op { "+" => "PLUS", "-" => "MINUS", "*" => "MULTI", "/" => "DIVIDE", _ => return None })
}

fn math_op(name: &str) -> Option<&'static str> {
    Some(match name {
        "sin" => "sin", "cos" => "cos", "tan" => "tan", "asin" => "asin_radian", "acos" => "acos_radian",
        "atan" => "atan_radian", "ln" => "ln", "log10" => "log", "floor" => "floor", "ceil" => "ceil",
        "round" => "round", "abs" => "abs", "factorial" => "factorial", _ => return None,
    })
}

fn object_coordinate(name: &str) -> Option<&'static str> {
    Some(match name {
        "x" => "x", "y" => "y", "angle" => "rotation", "way" => "direction", "size" => "size",
        "costume" => "picture_name", "costume_number" => "picture_index", _ => return None,
    })
}

fn state_block(name: &str) -> Option<&'static str> {
    Some(match name {
        "mouse_down" => "is_clicked", "clicked" => "is_object_clicked", "boost_mode" => "is_boost_mode",
        "touchable" => "is_touch_supported", "user_id" => "get_user_name", "nickname" => "get_nickname",
        "timer" => "get_project_timer_value", "answer" => "get_canvas_input_value", _ => return None,
    })
}

pub fn block_type(v: &Value) -> &str {
    v.get("type").and_then(|t| t.as_str()).unwrap_or("")
}

pub fn is_boolean_block(node: &Value) -> bool {
    if !node.is_object() {
        return false;
    }
    let t = block_type(node);
    if expansion_block(t).map(|e| e.boolean).unwrap_or(false) {
        return true;
    }
    matches!(t, "True" | "False" | "boolean_basic_operator" | "boolean_and_or" | "boolean_not" | "is_clicked"
        | "is_object_clicked" | "is_press_some_key" | "reach_something" | "is_type" | "is_boost_mode"
        | "is_current_device_type" | "is_touch_supported" | "is_included_in_list")
        || t.starts_with("booleanParam_")
}

pub fn compile_boolean(node: &Value, ctx: &mut Context) -> Option<Value> {
    if ty(node) == "Boolean" {
        let t = if node["value"].as_bool().unwrap_or(false) { "True" } else { "False" };
        return Some(ctx.block(t, vec![NULL]));
    }
    if matches!(ty(node), "Number" | "String" | "Color" | "Transparent") {
        return Some(ctx.block("True", vec![NULL]));
    }
    let compiled = compile_any_value(node, ctx)?;
    if is_boolean_block(&compiled) {
        return Some(compiled);
    }
    let t = ctx.text(bool_text(true));
    Some(ctx.block("boolean_basic_operator", vec![compiled, json!("EQUAL"), t]))
}

pub fn compile_value(node: &Value, ctx: &mut Context) -> Option<Value> {
    let compiled = compile_any_value(node, ctx)?;
    if is_boolean_block(&compiled) {
        Some(ctx.block("get_boolean_value", vec![compiled]))
    } else {
        Some(compiled)
    }
}

pub fn compile_any_value(node: &Value, ctx: &mut Context) -> Option<Value> {
    if node.is_null() {
        return None;
    }
    match ty(node) {
        "Number" => Some(ctx.number(f(node, "value"))),
        "String" => Some(ctx.text(&s(node, "value"))),
        "Boolean" => {
            let t = if node["value"].as_bool().unwrap_or(false) { "True" } else { "False" };
            Some(ctx.block(t, vec![NULL]))
        }
        "Color" => Some(ctx.text(&s(node, "value"))),
        "Transparent" => Some(ctx.text("transparent")),
        "Identifier" => compile_identifier(node, ctx),
        "Binary" => compile_binary(node, ctx),
        "Unary" => compile_unary(node, ctx),
        "Call" => compile_call(node, ctx),
        "Index" => compile_index(node, ctx),
        "ListLiteral" => ctx.error(node, "리스트 리터럴은 list 선언에서만 쓸 수 있습니다."),
        t => ctx.error(node, format!("표현식 '{}' 은(는) 엔트리 블록으로 바꿀 수 없습니다.", t)),
    }
}

fn compile_identifier(node: &Value, ctx: &mut Context) -> Option<Value> {
    let name = s(node, "name");
    if let Some(found) = ctx.lookup_variable(&name) {
        match found {
            VarRef::Ambiguous { name, owners } => return ambiguous_local_error(node, &name, &owners, ctx),
            VarRef::Param(_) => {
                let t = ctx.func_scope.as_ref().unwrap().params.get(&name).unwrap().clone();
                let params = if t.starts_with("booleanParam_") { vec![NULL] } else { vec![] };
                return Some(ctx.block(&t, params));
            }
            VarRef::FuncLocal { id } => return Some(ctx.block("get_func_variable", vec![json!(id), NULL])),
            VarRef::Variable { entry } => {
                if entry.variable_type == "list" {
                    ctx.warn(node, format!("리스트 '{}' 을(를) 값으로 바로 썼습니다. 실행할 때 값을 찾지 못합니다 — {}[i] 처럼 항목을 지정하세요.", name, name));
                }
                return Some(ctx.block("get_variable", vec![json!(entry.id), NULL]));
            }
        }
    }
    if name == "block_count" {
        return Some(ctx.block("get_block_count", vec![json!("all")]));
    }
    if name == "can_save" {
        let flag = require_store_flag(ctx);
        let var = ctx.block("get_variable", vec![json!(flag), NULL]);
        let one = ctx.number(1.0);
        return Some(ctx.block("boolean_basic_operator", vec![var, json!("EQUAL"), one]));
    }
    if let Some(b) = state_block(&name) {
        let count = match b { "get_nickname" | "get_user_name" => 0, "get_project_timer_value" => 2, _ => 1 };
        return Some(ctx.block(b, vec![NULL; count]));
    }
    if name == "device" {
        return ctx.error(node, "device 는 홀로 쓸 수 없습니다. device == \"mobile\" 처럼 비교해서 쓰세요.");
    }
    if let Some(c) = object_coordinate(&name) {
        return Some(ctx.block("coordinate_object", vec![NULL, json!("self"), NULL, json!(c)]));
    }
    if name == "sound_volume" {
        return Some(ctx.block("get_sound_volume", vec![NULL]));
    }
    if name == "sound_speed" {
        return Some(ctx.block("get_sound_speed", vec![NULL]));
    }
    if OPTION_KEYWORDS.contains(&name.as_str()) || STATE_VALUES.contains(&name.as_str()) {
        return ctx.error(node, format!("'{}' 은(는) 이 자리에서 값으로 쓸 수 없습니다.", name));
    }
    let hint = did_you_mean(&name, ctx.known_names().iter().map(|x| x.as_str()));
    ctx.error(node, format!("선언되지 않은 이름 '{}' 입니다.{}", name, hint))
}

pub fn ambiguous_local_error<T>(node: &Value, name: &str, owners: &[String], ctx: &mut Context) -> Option<T> {
    ctx.error(
        node,
        format!("'{}' 은(는) {} 가 저마다 가진 지역 변수라 어느 것인지 알 수 없습니다. 이 함수를 그 오브젝트 안에 선언하거나, 값을 매개변수로 전달하세요.", name, owners.join(", ")),
    )
}

fn compile_binary(node: &Value, ctx: &mut Context) -> Option<Value> {
    let op = s(node, "operator");
    if op == "and" || op == "or" {
        let left = compile_boolean(&node["left"], ctx);
        let right = compile_boolean(&node["right"], ctx);
        let (l, r) = (left?, right?);
        return Some(ctx.block("boolean_and_or", vec![l, json!(if op == "and" { "AND" } else { "OR" }), r]));
    }
    if compare_op(&op).is_some() {
        return compile_comparison(node, ctx);
    }
    if let Some(a) = arithmetic_op(&op) {
        let left = compile_value(&node["left"], ctx);
        let right = compile_value(&node["right"], ctx);
        let (l, r) = (left?, right?);
        return Some(ctx.block("calc_basic", vec![l, json!(a), r]));
    }
    if op == "%" || op == "//" {
        let left = compile_value(&node["left"], ctx);
        let right = compile_value(&node["right"], ctx);
        let (l, r) = (left?, right?);
        let action = if op == "%" { "MOD" } else { "QUOTIENT" };
        return Some(ctx.block("quotient_and_mod", vec![NULL, l, NULL, r, NULL, json!(action)]));
    }
    if op == "**" {
        return compile_power(node, ctx);
    }
    ctx.error(node, format!("연산자 '{}' 를 엔트리 블록으로 바꿀 수 없습니다.", op))
}

pub fn fold_constant(node: &Value) -> Option<f64> {
    if node.is_null() {
        return None;
    }
    match ty(node) {
        "Number" => Some(f(node, "value")),
        "Unary" => {
            if s(node, "operator") != "-" {
                return None;
            }
            fold_constant(&node["argument"]).map(|v| -v)
        }
        "Binary" => {
            let l = fold_constant(&node["left"]);
            let r = fold_constant(&node["right"]);
            let (l, r) = (l?, r?);
            match s(node, "operator").as_str() {
                "+" => Some(l + r),
                "-" => Some(l - r),
                "*" => Some(l * r),
                "/" => if r == 0.0 { None } else { Some(l / r) },
                "%" => if r == 0.0 { None } else { Some(l % r) },
                "//" => if r == 0.0 { None } else { Some((l / r).floor()) },
                "**" => Some(l.powf(r)),
                _ => None,
            }
        }
        _ => None,
    }
}

const FRACTION_BITS: usize = 20;

fn compile_power(node: &Value, ctx: &mut Context) -> Option<Value> {
    let exponent = match fold_constant(&node["right"]) {
        Some(e) => e,
        None => {
            return ctx.error(node, "거듭제곱의 지수는 숫자로 정해져 있어야 합니다. (엔트리에는 거듭제곱 블록이 없어서 컴파일할 때 펼쳐 넣습니다)");
        }
    };
    build_power(&node["left"], exponent, node, ctx)
}

struct PowState {
    uses: usize,
    failed: bool,
}

fn pw_base(base_node: &Value, st: &mut PowState, ctx: &mut Context) -> Value {
    st.uses += 1;
    match compile_value(base_node, ctx) {
        Some(c) => c,
        None => {
            st.failed = true;
            ctx.number(1.0)
        }
    }
}

fn pw_square(ctx: &mut Context, v: Value) -> Value {
    ctx.block("calc_operation", vec![NULL, v, NULL, json!("square")])
}

fn pw_multiply(ctx: &mut Context, l: Value, r: Value) -> Value {
    ctx.block("calc_basic", vec![l, json!("MULTI"), r])
}

fn integer_power(n: i64, base_node: &Value, st: &mut PowState, ctx: &mut Context) -> Option<Value> {
    if n <= 0 {
        return None;
    }
    if n == 1 {
        return Some(pw_base(base_node, st, ctx));
    }
    let half = integer_power(n / 2, base_node, st, ctx).unwrap();
    let squared = pw_square(ctx, half);
    if n % 2 == 1 {
        let b = pw_base(base_node, st, ctx);
        Some(pw_multiply(ctx, squared, b))
    } else {
        Some(squared)
    }
}

fn fraction_power(bits: &[u8], index: usize, base_node: &Value, st: &mut PowState, ctx: &mut Context) -> Option<Value> {
    if index >= bits.len() {
        return None;
    }
    let rest = fraction_power(bits, index + 1, base_node, st, ctx);
    let mut inner = rest.clone();
    if bits[index] == 1 {
        inner = match rest {
            Some(r) => {
                let b = pw_base(base_node, st, ctx);
                Some(pw_multiply(ctx, b, r))
            }
            None => Some(pw_base(base_node, st, ctx)),
        };
    }
    inner.map(|i| ctx.block("calc_operation", vec![NULL, i, NULL, json!("root")]))
}

fn fraction_bits(fraction: f64) -> (Vec<u8>, bool) {
    let mut bits = Vec::new();
    let mut rest = fraction;
    while bits.len() < FRACTION_BITS && rest > 0.0 {
        rest *= 2.0;
        if rest >= 1.0 {
            bits.push(1);
            rest -= 1.0;
        } else {
            bits.push(0);
        }
    }
    while bits.last() == Some(&0) {
        bits.pop();
    }
    (bits, rest == 0.0)
}

fn contains_random(node: &Value) -> bool {
    match node {
        Value::Array(a) => a.iter().any(contains_random),
        Value::Object(m) => {
            if ty(node) == "Call" && matches!(s(node, "callee").as_str(), "random" | "random_color") {
                return true;
            }
            m.iter().any(|(k, v)| k != "loc" && contains_random(v))
        }
        _ => false,
    }
}

pub fn build_power(base_node: &Value, exponent: f64, node: &Value, ctx: &mut Context) -> Option<Value> {
    if !exponent.is_finite() {
        return ctx.error(node, "거듭제곱의 지수가 올바르지 않습니다.");
    }
    if exponent == 0.0 {
        return Some(ctx.number(1.0));
    }
    if exponent < 0.0 {
        let positive = build_power(base_node, -exponent, node, ctx)?;
        let one = ctx.number(1.0);
        return Some(ctx.block("calc_basic", vec![one, json!("DIVIDE"), positive]));
    }
    let mut st = PowState { uses: 0, failed: false };
    let whole = exponent.floor();
    let (bits, exact) = fraction_bits(exponent - whole);
    let whole_part = integer_power(whole as i64, base_node, &mut st, ctx);
    let fraction_part = fraction_power(&bits, 0, base_node, &mut st, ctx);
    let mut result = match (whole_part, fraction_part) {
        (Some(w), Some(fp)) => Some(pw_multiply(ctx, w, fp)),
        (Some(w), None) => Some(w),
        (None, fp) => fp,
    };
    if let (Some(r), false) = (&result, exact) {
        let refiner_id = require_power_refiner(ctx);
        let b = pw_base(base_node, &mut st, ctx);
        let n = ctx.number(exponent);
        let blk = ctx.block(&format!("func_{}", refiner_id), vec![r.clone(), b, n]);
        result = Some(blk);
    }
    if st.failed {
        return None;
    }
    if st.uses > 1 && contains_random(base_node) {
        return ctx.error(node, "이 지수는 밑을 여러 번 써야 해서 random() 이 들어간 값에는 쓸 수 없습니다. 변수에 먼저 담아 두고 쓰세요.");
    }
    match result {
        Some(r) => Some(r),
        None => Some(ctx.number(1.0)),
    }
}

fn type_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "number" | "숫자" => "number",
        "ko" | "korean" | "hangul" | "한글" => "ko",
        "en" | "english" | "영문" | "영어" | "string" | "문자" => "en",
        _ => return None,
    })
}

fn compile_comparison(node: &Value, ctx: &mut Context) -> Option<Value> {
    let op = s(node, "operator");
    let operator = compare_op(&op).unwrap();

    if let Some((value, kind)) = match_type_check(node) {
        let v = compile_value(&value, ctx)?;
        let k = match type_kind(&kind) {
            Some(k) => k,
            None => return ctx.error(node, "type() 비교값은 \"한글\", \"숫자\", \"영어\"(\"ko\", \"number\", \"en\") 중 하나여야 합니다."),
        };
        let check = ctx.block("is_type", vec![v, NULL, json!(k), NULL]);
        return if op == "==" { Some(check) } else { Some(ctx.block("boolean_not", vec![NULL, check, NULL])) };
    }

    if let Some(device) = match_device_check(node) {
        if !["desktop", "tablet", "mobile"].contains(&device.as_str()) {
            return ctx.error(node, "device 는 \"desktop\", \"tablet\", \"mobile\" 하고만 비교할 수 있습니다.");
        }
        let check = ctx.block("is_current_device_type", vec![json!(device)]);
        return if op == "==" { Some(check) } else { Some(ctx.block("boolean_not", vec![NULL, check, NULL])) };
    }

    let left = compile_value(&node["left"], ctx);
    let right = compile_value(&node["right"], ctx);
    let (l, r) = (left?, right?);
    Some(ctx.block("boolean_basic_operator", vec![l, json!(operator), r]))
}

fn match_type_check(node: &Value) -> Option<(Value, String)> {
    let op = s(node, "operator");
    if op != "==" && op != "!=" {
        return None;
    }
    for (call, other) in [(&node["left"], &node["right"]), (&node["right"], &node["left"])] {
        if ty(call) == "Call" && s(call, "callee") == "type" && ty(other) == "String" {
            let arg = call["arguments"].get(0).cloned().unwrap_or(Value::Null);
            return Some((arg, s(other, "value")));
        }
    }
    None
}

fn match_device_check(node: &Value) -> Option<String> {
    let op = s(node, "operator");
    if op != "==" && op != "!=" {
        return None;
    }
    for (id, other) in [(&node["left"], &node["right"]), (&node["right"], &node["left"])] {
        if ty(id) == "Identifier" && s(id, "name") == "device" && ty(other) == "String" {
            return Some(s(other, "value"));
        }
    }
    None
}

fn compile_unary(node: &Value, ctx: &mut Context) -> Option<Value> {
    if s(node, "operator") == "not" {
        let value = compile_boolean(&node["argument"], ctx)?;
        return Some(ctx.block("boolean_not", vec![NULL, value, NULL]));
    }
    if ty(&node["argument"]) == "Number" {
        return Some(ctx.number(-f(&node["argument"], "value")));
    }
    let value = compile_value(&node["argument"], ctx)?;
    let zero = ctx.number(0.0);
    Some(ctx.block("calc_basic", vec![zero, json!("MINUS"), value]))
}

fn compile_index(node: &Value, ctx: &mut Context) -> Option<Value> {
    let target = &node["target"];
    let table = if ty(target) == "Identifier" { ctx.table_by_name.get(&s(target, "name")).cloned() } else { None };
    if let Some(table) = table {
        let first = compile_value(&node["index"], ctx)?;
        if node["column"].is_null() {
            return Some(ctx.block("get_value_from_cell", vec![json!(table.id), first, NULL]));
        }
        let column = compile_value(&node["column"], ctx)?;
        return Some(ctx.block("get_value_from_table", vec![json!(table.id), first, column, NULL]));
    }
    if !node["column"].is_null() {
        let label = if ty(target) == "Identifier" { s(target, "name") } else { String::new() };
        return ctx.error(node, format!("'{}' 은(는) 테이블이 아니라서 [행, 열] 로 읽을 수 없습니다.", label));
    }
    let list = resolve_list(target, ctx);
    let index = compile_value(&node["index"], ctx)?;
    if let Some(l) = list {
        return Some(ctx.block("value_of_index_from_list", vec![NULL, json!(l.id), NULL, index, NULL]));
    }
    let t = compile_value(target, ctx)?;
    Some(ctx.block("char_at", vec![NULL, t, NULL, index, NULL]))
}

pub fn compile_call_arguments(boolean_flags: &[bool], args: &[Value], ctx: &mut Context) -> Vec<Option<Value>> {
    args.iter()
        .enumerate()
        .map(|(i, a)| if boolean_flags.get(i).copied().unwrap_or(false) { compile_boolean(a, ctx) } else { compile_value(a, ctx) })
        .collect()
}

fn resolve_sound_value(node: &Value, ctx: &mut Context) -> Option<String> {
    if ty(node) != "String" {
        return ctx.error(node, "sound_duration() 은 소리 이름을 문자열로 적어야 합니다.");
    }
    let name = s(node, "value");
    if let Some(o) = ctx.cur_object() {
        if let Some(snd) = o.sounds.get(&name) {
            return Some(s(snd, "id"));
        }
    }
    if ctx.forced_resource_ids.contains(&name) {
        return Some(name);
    }
    if ctx.object.is_none() {
        match ctx.lookup_object_resource(true, &name) {
            Some(ResourceRef::Found(a)) => return Some(s(&a, "id")),
            None => ctx.notice(node, format!("'{}' 소리가 어느 오브젝트에도 없습니다. 실행할 때 이름으로 찾습니다.", name)),
            _ => {}
        }
        return Some(name);
    }
    let keys: Vec<String> = ctx.cur_object().map(|o| o.sounds.keys().map(|k| k.to_string()).collect()).unwrap_or_default();
    let hint = or_hint(&name, keys.iter().map(|x| x.as_str()), &format!("sound {} \"파일명\" 으로 먼저 등록하세요.", name));
    ctx.notice(node, format!("'{}' 소리가 이 오브젝트에 없습니다. 실행할 때 이름으로 찾습니다.{}", name, hint));
    Some(name)
}

pub fn resolve_list(node: &Value, ctx: &Context) -> Option<VarInfo> {
    if node.is_null() || ty(node) != "Identifier" {
        return None;
    }
    match ctx.lookup_variable(&s(node, "name")) {
        Some(VarRef::Variable { entry }) if entry.variable_type == "list" => Some(entry),
        _ => None,
    }
}

fn table_calc(name: &str) -> Option<&'static str> {
    Some(match name {
        "sum" => "SUM", "average" => "AVG", "maximum" => "MAX", "minimum" => "MIN",
        "stdev" => "STDEV", "median" => "MEDIAN", _ => return None,
    })
}

fn table_argument(node: &Value, callee: &str, ctx: &mut Context) -> Option<TableInfo> {
    if ty(node) == "Identifier" {
        if let Some(t) = ctx.table_by_name.get(&s(node, "name")) {
            return Some(t.clone());
        }
    }
    let name = if ty(node) == "Identifier" { s(node, "name") } else { String::new() };
    let keys: Vec<String> = ctx.table_by_name.keys().map(|k| k.to_string()).collect();
    let hint = did_you_mean(&name, keys.iter().map(|x| x.as_str()));
    ctx.error(node, format!("{}() 의 첫 번째 인자는 테이블이어야 합니다.{}", callee, hint))
}

fn compile_expansion(node: &Value, ctx: &mut Context) -> Option<Value> {
    let callee = s(node, "callee");
    let args = arr(node, "arguments");
    let ex = expansion_block(&callee).unwrap();
    let slots: Vec<char> = ex.slots.chars().collect();
    if args.len() != slots.len() {
        return ctx.error(node, format!("{}() 는 인자가 {}개여야 합니다. ({}개를 받았습니다)", callee, slots.len(), args.len()));
    }
    let mut params: Vec<Option<Value>> = Vec::new();
    for (i, slot) in slots.iter().enumerate() {
        let a = &args[i];
        let p = if *slot == 'v' {
            compile_value(a, ctx)
        } else if ty(a) == "String" {
            Some(json!(s(a, "value")))
        } else if ty(a) == "Number" {
            Some(json!(num_to_string(f(a, "value"))))
        } else {
            ctx.error(a, format!("{}() 의 {}번째 칸은 목록에서 고르는 자리라 \"...\" 로 직접 적어야 합니다.", callee, i + 1))
        };
        params.push(p);
    }
    if params.iter().any(|p| p.is_none()) {
        return None;
    }
    ctx.add_expansion(ex.module, ex.ai);
    Some(ctx.block(&callee, params.into_iter().map(|p| p.unwrap()).collect()))
}

fn arity(node: &Value, count: usize, ctx: &mut Context) -> bool {
    let args = arr(node, "arguments");
    if args.len() == count {
        return true;
    }
    let _: Option<()> = ctx.error(node, format!("{}() 는 인자가 {}개여야 합니다. ({}개를 받았습니다)", s(node, "callee"), count, args.len()));
    false
}

fn compile_call(node: &Value, ctx: &mut Context) -> Option<Value> {
    let callee = s(node, "callee");
    let args: Vec<Value> = arr(node, "arguments").to_vec();
    let value = |i: usize, ctx: &mut Context| compile_value(args.get(i).unwrap_or(&Value::Null), ctx);

    if let Some(&fi) = ctx.function_by_name.get(&callee) {
        let (is_value, nparams, flags, fid) = {
            let fnc = &ctx.functions[fi];
            (fnc.is_value, fnc.params.len(), fnc.params.iter().map(|p| fnc.boolean_params.contains(p)).collect::<Vec<bool>>(), fnc.id.clone())
        };
        if !is_value {
            ctx.notice(node, format!("함수 '{}' 는 값을 돌려주지 않습니다. 값 자리에서도 돌기는 하지만 자리에는 아무 값도 남지 않습니다.", callee));
        }
        if args.len() != nparams {
            return ctx.error(node, format!("함수 '{}' 는 인자가 {}개여야 합니다.", callee, nparams));
        }
        let params = compile_call_arguments(&flags, &args, ctx);
        if params.iter().any(|p| p.is_none()) {
            return None;
        }
        return Some(ctx.block(&format!("func_{}", fid), params.into_iter().map(|p| p.unwrap()).collect()));
    }

    if let Some(op) = math_op(&callee) {
        if !arity(node, 1, ctx) {
            return None;
        }
        let a = value(0, ctx)?;
        return Some(ctx.block("calc_operation", vec![NULL, a, NULL, json!(op)]));
    }

    match callee.as_str() {
        "log2" => {
            if !arity(node, 1, ctx) { return None; }
            let a = value(0, ctx)?;
            let ln1 = ctx.block("calc_operation", vec![NULL, a, NULL, json!("ln")]);
            let two = ctx.number(2.0);
            let ln2 = ctx.block("calc_operation", vec![NULL, two, NULL, json!("ln")]);
            Some(ctx.block("calc_basic", vec![ln1, json!("DIVIDE"), ln2]))
        }
        "random" => {
            if !arity(node, 2, ctx) { return None; }
            let from = value(0, ctx);
            let to = value(1, ctx);
            let (fr, t) = (from?, to?);
            Some(ctx.block("calc_rand", vec![NULL, fr, NULL, t, NULL]))
        }
        "root" => {
            if !arity(node, 2, ctx) { return None; }
            let degree = fold_constant(&args[1]);
            match degree {
                None => ctx.error(node, "root(값, n) 의 n 은 0이 아닌 숫자로 정해져 있어야 합니다."),
                Some(d) if d == 0.0 => ctx.error(node, "root(값, n) 의 n 은 0이 아닌 숫자로 정해져 있어야 합니다."),
                Some(d) => build_power(&args[0], 1.0 / d, node, ctx),
            }
        }
        "key_down" => {
            if !arity(node, 1, ctx) { return None; }
            let code = literal_key_code(&args[0], ctx, node)?;
            Some(ctx.block("is_press_some_key", vec![json!(code), NULL]))
        }
        "touching" => {
            if !arity(node, 1, ctx) { return None; }
            let t = resolve_target(&args[0], ctx, TargetOptions { wall: true, ..Default::default() })?;
            Some(ctx.block("reach_something", vec![NULL, json!(t), NULL]))
        }
        "type" => ctx.error(node, "type() 은 == \"number\", == \"ko\", == \"en\" 처럼 비교해서만 쓸 수 있습니다."),
        "distance" => {
            if !arity(node, 1, ctx) { return None; }
            let t = resolve_target(&args[0], ctx, TargetOptions { mouse: true, ..Default::default() })?;
            Some(ctx.block("distance_something", vec![NULL, json!(t), NULL]))
        }
        "x" | "y" | "angle" | "way" | "size" | "costume" | "costume_number" => {
            if !arity(node, 1, ctx) { return None; }
            let coordinate = object_coordinate(&callee).unwrap();
            if ty(&args[0]) == "String" && s(&args[0], "value") == "mouse" {
                if callee != "x" && callee != "y" {
                    return ctx.error(node, "마우스는 x(), y() 만 조회할 수 있습니다.");
                }
                return Some(ctx.block("coordinate_mouse", vec![NULL, json!(coordinate), NULL]));
            }
            let t = resolve_target(&args[0], ctx, TargetOptions { self_: true, ..Default::default() })?;
            Some(ctx.block("coordinate_object", vec![NULL, json!(t), NULL, json!(coordinate)]))
        }
        "text_content" => {
            if !arity(node, 1, ctx) { return None; }
            let t = resolve_target(&args[0], ctx, TargetOptions { self_: true, ..Default::default() })?;
            Some(ctx.block("text_read", vec![json!(t), NULL]))
        }
        "block_count" => {
            if !arity(node, 1, ctx) { return None; }
            let t = resolve_target(&args[0], ctx, TargetOptions { self_: true, all: true, ..Default::default() })?;
            Some(ctx.block("get_block_count", vec![json!(t)]))
        }
        "sound_duration" => {
            if !arity(node, 1, ctx) { return None; }
            let snd = resolve_sound_value(&args[0], ctx)?;
            Some(ctx.block("get_sound_duration", vec![NULL, json!(snd), NULL]))
        }
        "length" => {
            if !arity(node, 1, ctx) { return None; }
            if let Some(l) = resolve_list(&args[0], ctx) {
                return Some(ctx.block("length_of_list", vec![NULL, json!(l.id), NULL]));
            }
            let a = value(0, ctx)?;
            Some(ctx.block("length_of_string", vec![NULL, a, NULL]))
        }
        "contains" => {
            if !arity(node, 2, ctx) { return None; }
            let l = match resolve_list(&args[0], ctx) {
                Some(l) => l,
                None => return ctx.error(node, "contains() 의 첫 번째 인자는 리스트여야 합니다."),
            };
            let item = value(1, ctx)?;
            Some(ctx.block("is_included_in_list", vec![NULL, json!(l.id), NULL, item, NULL]))
        }
        "slice" => {
            if !arity(node, 3, ctx) { return None; }
            let st = value(0, ctx);
            let start = value(1, ctx);
            let end = value(2, ctx);
            let (a, b, c) = (st?, start?, end?);
            Some(ctx.block("substring", vec![NULL, a, NULL, b, NULL, c, NULL]))
        }
        "count" => {
            if !arity(node, 2, ctx) { return None; }
            let st = value(0, ctx);
            let t = value(1, ctx);
            let (a, b) = (st?, t?);
            Some(ctx.block("count_match_string", vec![a, NULL, b, NULL]))
        }
        "join" => {
            if !arity(node, 2, ctx) { return None; }
            let a = value(0, ctx);
            let b = value(1, ctx);
            let (a, b) = (a?, b?);
            Some(ctx.block("combine_something", vec![NULL, a, NULL, b, NULL]))
        }
        "index_of" => {
            if !arity(node, 2, ctx) { return None; }
            let a = value(0, ctx);
            let b = value(1, ctx);
            let (a, b) = (a?, b?);
            Some(ctx.block("index_of_string", vec![NULL, a, NULL, b, NULL]))
        }
        "replace" => {
            if !arity(node, 3, ctx) { return None; }
            let a = value(0, ctx);
            let b = value(1, ctx);
            let c = value(2, ctx);
            let (a, b, c) = (a?, b?, c?);
            Some(ctx.block("replace_string", vec![NULL, a, NULL, b, NULL, c, NULL]))
        }
        "reverse" => {
            if !arity(node, 1, ctx) { return None; }
            let a = value(0, ctx)?;
            Some(ctx.block("reverse_of_string", vec![NULL, a, NULL]))
        }
        "uppercase" | "lowercase" => {
            if !arity(node, 1, ctx) { return None; }
            let a = value(0, ctx)?;
            let mode = if callee == "uppercase" { "toUpperCase" } else { "toLowerCase" };
            Some(ctx.block("change_string_case", vec![NULL, a, NULL, json!(mode), NULL]))
        }
        "now" => {
            if !arity(node, 1, ctx) { return None; }
            let unit = if ty(&args[0]) == "String" {
                match s(&args[0], "value").as_str() {
                    "year" => Some("YEAR"), "month" => Some("MONTH"), "day" => Some("DAY"), "hour" => Some("HOUR"),
                    "minute" => Some("MINUTE"), "second" => Some("SECOND"), "weekday" => Some("DAY_OF_WEEK"),
                    _ => None,
                }
            } else { None };
            match unit {
                None => ctx.error(node, "now() 는 \"year\", \"month\", \"day\", \"hour\", \"minute\", \"second\", \"weekday\" 만 받습니다."),
                Some(u) => Some(ctx.block("get_date", vec![NULL, json!(u), NULL])),
            }
        }
        "to_hex" => {
            if !arity(node, 3, ctx) { return None; }
            let r = value(0, ctx);
            let g = value(1, ctx);
            let b = value(2, ctx);
            let (r, g, b) = (r?, g?, b?);
            Some(ctx.block("change_rgb_to_hex", vec![r, g, b]))
        }
        "from_hex" => {
            if !arity(node, 2, ctx) { return None; }
            let channel = if ty(&args[1]) == "Identifier" {
                match s(&args[1], "name").as_str() { "red" => Some("r"), "green" => Some("g"), "blue" => Some("b"), _ => None }
            } else { None };
            let channel = match channel {
                Some(c) => c,
                None => return ctx.error(node, "from_hex() 의 두 번째 인자는 red, green, blue 중 하나여야 합니다."),
            };
            let color = value(0, ctx)?;
            Some(ctx.block("change_hex_to_rgb", vec![color, json!(channel)]))
        }
        "random_color" => ctx.error(node, "random_color() 는 draw_color = random_color() 형태로만 쓸 수 있습니다."),
        "row_count" | "column_count" => {
            if !arity(node, 1, ctx) { return None; }
            let t = table_argument(&args[0], &callee, ctx)?;
            Some(ctx.block("get_table_count", vec![json!(t.id), json!(if callee == "row_count" { "ROW" } else { "COL" }), NULL]))
        }
        "last_row" => {
            if !arity(node, 2, ctx) { return None; }
            let t = table_argument(&args[0], &callee, ctx);
            let c = value(1, ctx);
            let (t, c) = (t?, c?);
            Some(ctx.block("get_value_from_last_row", vec![json!(t.id), c, NULL]))
        }
        "correlation" => {
            if !arity(node, 3, ctx) { return None; }
            let t = table_argument(&args[0], &callee, ctx);
            let x = value(1, ctx);
            let y = value(2, ctx);
            let (t, x, y) = (t?, x?, y?);
            Some(ctx.block("get_coefficient", vec![json!(t.id), x, y, NULL]))
        }
        "lookup" => {
            if !arity(node, 4, ctx) { return None; }
            let t = table_argument(&args[0], &callee, ctx);
            let a = value(1, ctx);
            let b = value(2, ctx);
            let c = value(3, ctx);
            let (t, a, b, c) = (t?, a?, b?, c?);
            Some(ctx.block("get_value_v_lookup", vec![json!(t.id), a, b, c, NULL]))
        }
        _ => {
            if expansion_block(&callee).is_some() {
                return compile_expansion(node, ctx);
            }
            if let Some(calc) = table_calc(&callee) {
                if !arity(node, 2, ctx) { return None; }
                let t = table_argument(&args[0], &callee, ctx);
                let c = value(1, ctx);
                let (t, c) = (t?, c?);
                return Some(ctx.block("calc_values_from_table", vec![json!(t.id), c, json!(calc), NULL]));
            }
            let mut known: Vec<String> = builtin_function_names().iter().map(|x| x.to_string()).collect();
            known.extend(ctx.function_by_name.keys().map(|k| k.to_string()));
            let hint = did_you_mean(&callee, known.iter().map(|x| x.as_str()));
            ctx.error(node, format!("알 수 없는 함수 '{}' 입니다.{}", callee, hint))
        }
    }
}

fn literal_key_code(node: &Value, ctx: &mut Context, at: &Value) -> Option<String> {
    if ty(node) != "String" {
        return ctx.error(at, "키 이름은 \"space\" 처럼 문자열로 직접 적어야 합니다.");
    }
    let name = s(node, "value");
    if let Some(code) = key_code(&name) {
        return Some(code);
    }
    let names = key_names();
    let hint = did_you_mean(&name, names.iter().map(|x| x.as_str()));
    ctx.error(at, format!("알 수 없는 키 이름 \"{}\" 입니다.{}", name, hint))
}

/// Key names in the order the reference table lists them (used for suggestions).
pub fn key_names() -> Vec<String> {
    let mut v: Vec<String> = (0..=9).map(|i| i.to_string()).collect();
    v.extend(["backspace", "tab", "enter", "shift", "ctrl", "alt", "esc", "escape", "space", "left", "up", "right", "down",
        "capslock", "pageup", "pagedown", "end", "home", "insert", "delete"].iter().map(|s| s.to_string()));
    for i in 1..=12 { v.push(format!("f{}", i)); }
    for i in 0..=9 { v.push(format!("numpad{}", i)); }
    for c in 'a'..='z' { v.push(c.to_string()); }
    for k in [";", "=", ",", "-", ".", "/", "~", "[", "backslash", "\\", "]", "'"] { v.push(k.to_string()); }
    v
}

#[derive(Default, Clone, Copy)]
pub struct TargetOptions {
    pub wall: bool,
    pub self_: bool,
    pub all: bool,
    pub mouse: bool,
}

pub fn resolve_target(node: &Value, ctx: &mut Context, options: TargetOptions) -> Option<String> {
    let _ = options.mouse;
    if node.is_null() || ty(node) != "String" {
        return ctx.error(node, "오브젝트 이름은 \"player\" 처럼 문자열로 직접 적어야 합니다.");
    }
    let name = s(node, "value");
    if name == "mouse" {
        return Some("mouse".into());
    }
    if options.wall && name.starts_with("wall") {
        return Some(name);
    }
    if options.self_ && (name == "self" || name == "this") {
        return Some("self".into());
    }
    if options.all && name == "all" {
        return Some("all".into());
    }
    if let Some(id) = ctx.object_id(&name) {
        return Some(id);
    }
    if let Some(rest) = name.strip_prefix("_missing_object_") {
        if !rest.is_empty() {
            ctx.notice(node, format!("'{}' 오브젝트가 작품에 없습니다. 엔트리처럼 아이디만 남깁니다.", rest));
            return Some(rest.to_string());
        }
    }
    let keys: Vec<String> = ctx.object_by_name.keys().map(|k| k.to_string()).collect();
    let hint = did_you_mean(&name, keys.iter().map(|x| x.as_str()));
    ctx.error(node, format!("'{}' 이라는 오브젝트가 없습니다.{}", name, hint))
}
