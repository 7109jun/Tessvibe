//! Tess statements to Entry blocks.
use crate::core::{did_you_mean, or_hint};
use crate::ctx::*;
use crate::expression::*;
use crate::jsnum::num_to_string;
use crate::runtime::{require_save_function, require_scale_setter};
use serde_json::{json, Value};

static NULLV: Value = Value::Null;
const NULL: Value = Value::Null;

const STOP_TARGETS: &[(&str, &str)] = &[
    ("this", "thisThread"),
    ("other", "otherThread"),
    ("me", "thisOnly"),
    ("object", "thisObject"),
    ("them", "other_objects"),
    ("all", "all"),
];

const EFFECTS: &[(&str, &str)] = &[
    ("effect_color", "color"),
    ("effect_brightness", "brightness"),
    ("effect_alpha", "transparency"),
];

const TEXT_EFFECTS: &[(&str, &str)] = &[
    ("text_bold", "fontBold"),
    ("text_italic", "fontItalic"),
    ("text_underline", "underLine"),
    ("text_strikethrough", "strike"),
];

const TTS_SPEAKERS: &[(&str, &str)] = &[
    ("kyuri", "kyuri"),
    ("female", "kyuri"),
    ("jinho", "jinho"),
    ("male", "jinho"),
    ("hana", "hana"),
    ("kind", "hana"),
    ("dinna", "dinna"),
    ("sweet", "dinna"),
    ("brown", "brown"),
    ("echo", "brown"),
    ("minions", "minions"),
    ("mischievous", "minions"),
    ("sally", "sally"),
    ("dainty", "sally"),
    ("nsabina", "nsabina"),
    ("nmammon", "nmammon"),
    ("nmeow", "nmeow"),
    ("kitty", "nmeow"),
    ("nwoof", "nwoof"),
    ("doggy", "nwoof"),
];
const TTS_SPEAKERS_KNOWN: &str = "kyuri, jinho, hana, dinna, brown, minions, sally, nsabina, nmammon, nmeow, nwoof";

const TTS_LEVELS: &[(&str, &str)] = &[
    ("veryslow", "5"),
    ("verylow", "5"),
    ("slow", "3"),
    ("low", "3"),
    ("normal", "0"),
    ("fast", "-3"),
    ("high", "-3"),
    ("veryfast", "-5"),
    ("veryhigh", "-5"),
    ("5", "5"),
    ("3", "3"),
    ("0", "0"),
    ("-3", "-3"),
    ("-5", "-5"),
];
// Object.values order: integer-like keys ascending, then insertion order.
const TTS_LEVELS_KNOWN: &str = "0, 3, 5, -3, -5";

const READABLE_PROPERTIES: &[(&str, &str)] = &[("x", "x"), ("y", "y"), ("angle", "rotation"), ("way", "direction"), ("size", "size")];

fn lookup(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub fn g<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&NULLV)
}

/// Present and not null/undefined.
pub fn nn<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v.get(k) {
        Some(x) if !x.is_null() => Some(x),
        _ => None,
    }
}

fn one(b: Option<Value>) -> Vec<Value> {
    b.into_iter().collect()
}

fn list_of(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default()
}

pub fn compile_statements(statements: &[Value], ctx: &mut Context) -> Vec<Value> {
    let mut blocks = Vec::new();
    for st in statements {
        blocks.extend(compile_statement(st, ctx));
    }
    blocks
}

pub fn compile_statement(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let mut blocks = compile(node, ctx);
    if let Some(first) = blocks.first_mut() {
        ctx.apply_comment(node, first);
    }
    blocks
}

fn compile(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let kind = ty(node).to_string();
    match kind.as_str() {
        "If" => {
            let Some(test) = compile_boolean(g(node, "test"), ctx) else { return vec![] };
            let cons = compile_statements(&list_of(node, "consequent"), ctx);
            match nn(node, "alternate") {
                None => vec![ctx.block_with("_if", vec![test, NULL], vec![cons])],
                Some(alt) => {
                    let alt = compile_statements(alt.as_array().map(|a| a.as_slice()).unwrap_or(&[]), ctx);
                    vec![ctx.block_with("if_else", vec![test, NULL, NULL], vec![cons, alt])]
                }
            }
        }
        "Repeat" => {
            let Some(count) = compile_value(g(node, "count"), ctx) else { return vec![] };
            let body = compile_statements(&list_of(node, "body"), ctx);
            vec![ctx.block_with("repeat_basic", vec![count, NULL], vec![body])]
        }
        "Forever" => {
            let body = compile_statements(&list_of(node, "body"), ctx);
            vec![ctx.block_with("repeat_inf", vec![NULL, NULL], vec![body])]
        }
        "While" | "Until" => {
            let test_node = g(node, "test");
            if kind == "While" && ty(test_node) == "Boolean" && test_node.get("value") == Some(&Value::Bool(true)) {
                let body = compile_statements(&list_of(node, "body"), ctx);
                return vec![ctx.block_with("repeat_inf", vec![NULL, NULL], vec![body])];
            }
            let Some(test) = compile_boolean(test_node, ctx) else { return vec![] };
            let mode = if kind == "While" { "while" } else { "until" };
            let body = compile_statements(&list_of(node, "body"), ctx);
            vec![ctx.block_with("repeat_while_true", vec![test, json!(mode), NULL], vec![body])]
        }
        "Wait" => {
            let Some(value) = compile_any_value(g(node, "value"), ctx) else { return vec![] };
            if is_boolean_block(&value) {
                vec![ctx.block("wait_until_true", vec![value, NULL])]
            } else {
                vec![ctx.block("wait_second", vec![value, NULL])]
            }
        }
        "Break" => vec![ctx.block("stop_repeat", vec![NULL])],
        "Continue" => vec![ctx.block("continue_repeat", vec![NULL])],
        "Skip" => {
            let cont = ctx.block("continue_repeat", vec![NULL]);
            let not = ctx.block("boolean_not", vec![NULL, cont, NULL]);
            vec![ctx.block("wait_until_true", vec![not, NULL])]
        }
        "Restart" => vec![ctx.block("restart_project", vec![NULL])],
        "Stop" => {
            let target = lookup(STOP_TARGETS, &s(node, "target")).map(|t| json!(t)).unwrap_or(NULL);
            vec![ctx.block("stop_object", vec![target, NULL])]
        }
        "StopProject" => vec![ctx.block("stop_run", vec![NULL])],
        "Return" => {
            let _: Option<()> = ctx.error(node, "return 은 함수의 마지막 문장에서만 쓸 수 있습니다.");
            vec![]
        }
        "Send" => {
            let signal = g(node, "signal");
            if ty(signal) != "String" {
                let _: Option<()> = ctx.error(node, "신호 이름은 \"게임 시작\" 처럼 문자열로 직접 적어야 합니다.");
                return vec![];
            }
            let id = ctx.message_id(&s(signal, "value"));
            let wait = g(node, "wait").as_bool().unwrap_or(false);
            vec![ctx.block(if wait { "message_cast_wait" } else { "message_cast" }, vec![json!(id), NULL])]
        }
        "Clone" => {
            let target = match nn(node, "target") {
                None => Some("self".to_string()),
                Some(t) => resolve_target(t, ctx, TargetOptions { self_: true, ..Default::default() }),
            };
            match target {
                None => vec![],
                Some(t) => vec![ctx.block("create_clone", vec![json!(t), NULL])],
            }
        }
        "DeleteClone" => vec![ctx.block("delete_clone", vec![NULL])],
        "DeleteClones" => vec![ctx.block("remove_all_clones", vec![NULL])],
        "Jump" => {
            let target = g(node, "target");
            if target.as_str() == Some("next") {
                return vec![ctx.block("start_neighbor_scene", vec![json!("next"), NULL])];
            }
            if target.as_str() == Some("back") {
                return vec![ctx.block("start_neighbor_scene", vec![json!("prev"), NULL])];
            }
            if target.is_string() || ty(target) != "String" {
                let _: Option<()> = ctx.error(node, "장면 이름은 문자열로 직접 적어야 합니다.");
                return vec![];
            }
            let name = s(target, "value");
            match ctx.scene_by_name.get(&name).cloned() {
                None => {
                    let keys: Vec<String> = ctx.scene_by_name.keys().map(|k| k.to_string()).collect();
                    let hint = did_you_mean(&name, keys.iter().map(|k| k.as_str()));
                    let _: Option<()> = ctx.error(node, format!("'{}' 이라는 장면이 없습니다.{}", name, hint));
                    vec![]
                }
                Some(scene) => vec![ctx.block("start_scene", vec![json!(scene.0), NULL])],
            }
        }
        "Forward" => {
            let Some(distance) = compile_value(g(node, "distance"), ctx) else { return vec![] };
            match nn(node, "angle") {
                None => vec![ctx.block("move_direction", vec![distance, NULL])],
                Some(a) => match compile_angle(a, ctx) {
                    None => vec![],
                    Some(angle) => vec![ctx.block("move_to_angle", vec![angle, distance, NULL])],
                },
            }
        }
        "Bounce" => vec![ctx.block("bounce_wall", vec![NULL])],
        "Move" => {
            let x = compile_value(g(node, "x"), ctx);
            let y = compile_value(g(node, "y"), ctx);
            let (Some(x), Some(y)) = (x, y) else { return vec![] };
            if let Some(d) = nn(node, "duration") {
                return match compile_value(d, ctx) {
                    None => vec![],
                    Some(dur) => vec![ctx.block("move_xy_time", vec![dur, x, y, NULL])],
                };
            }
            let a = ctx.block("move_x", vec![x, NULL]);
            let b = ctx.block("move_y", vec![y, NULL]);
            vec![a, b]
        }
        "Go" => {
            if let Some(t) = nn(node, "target") {
                let Some(target) = resolve_target(t, ctx, TargetOptions { self_: true, ..Default::default() }) else { return vec![] };
                let Some(d) = nn(node, "duration") else {
                    return vec![ctx.block("locate", vec![json!(target), NULL])];
                };
                return match compile_value(d, ctx) {
                    None => vec![],
                    Some(dur) => vec![ctx.block("locate_object_time", vec![dur, json!(target), NULL])],
                };
            }
            let x = compile_value(g(node, "x"), ctx);
            let y = compile_value(g(node, "y"), ctx);
            let (Some(x), Some(y)) = (x, y) else { return vec![] };
            let Some(d) = nn(node, "duration") else {
                return vec![ctx.block("locate_xy", vec![x, y, NULL])];
            };
            match compile_value(d, ctx) {
                None => vec![],
                Some(dur) => vec![ctx.block("locate_xy_time", vec![dur, x, y, NULL])],
            }
        }
        "Turn" | "Steer" => {
            let Some(angle) = compile_angle(g(node, "angle"), ctx) else { return vec![] };
            let is_shape = kind == "Turn";
            let Some(d) = nn(node, "duration") else {
                return vec![ctx.block(if is_shape { "rotate_relative" } else { "direction_relative" }, vec![angle, NULL])];
            };
            let Some(dur) = compile_value(d, ctx) else { return vec![] };
            vec![ctx.block(if is_shape { "rotate_by_time" } else { "direction_relative_duration" }, vec![dur, angle, NULL])]
        }
        "Look" => match resolve_target(g(node, "target"), ctx, TargetOptions::default()) {
            None => vec![],
            Some(t) => vec![ctx.block("see_angle_object", vec![json!(t), NULL])],
        },
        "Show" | "Hide" => compile_visibility(node, ctx),
        "CostumeStep" => vec![ctx.block("change_to_next_shape", vec![g(node, "direction").clone(), NULL])],
        "Say" | "Think" => {
            let Some(message) = compile_value(g(node, "message"), ctx) else { return vec![] };
            let mode = if kind == "Say" { "speak" } else { "think" };
            let Some(d) = nn(node, "duration") else {
                return vec![ctx.block("dialog", vec![message, json!(mode), NULL])];
            };
            match compile_value(d, ctx) {
                None => vec![],
                Some(dur) => vec![ctx.block("dialog_time", vec![message, dur, json!(mode), NULL])],
            }
        }
        "Flip" => vec![ctx.block(if s(node, "axis") == "x" { "flip_y" } else { "flip_x" }, vec![NULL])],
        "Order" => {
            let to = match s(node, "to").as_str() {
                "first" => json!("FRONT"),
                "front" => json!("FORWARD"),
                "back" => json!("BACKWARD"),
                "last" => json!("BACK"),
                _ => NULL,
            };
            vec![ctx.block("change_object_index", vec![to, NULL])]
        }
        "ResetSize" => vec![ctx.block("reset_scale_size", vec![NULL])],
        "Clear" => {
            let t = match s(node, "target").as_str() {
                "effects" => "erase_all_effects",
                "bubble" => "remove_dialog",
                "draw" => "brush_erase_all",
                _ => "text_flush",
            };
            vec![ctx.block(t, vec![NULL])]
        }
        "TextWrite" => {
            let Some(value) = compile_value(g(node, "value"), ctx) else { return vec![] };
            let t = match s(node, "mode").as_str() {
                "write" => "text_write",
                "append" => "text_append",
                _ => "text_prepend",
            };
            vec![ctx.block(t, vec![value, NULL])]
        }
        "StartDraw" => vec![ctx.block("start_drawing", vec![NULL])],
        "StopDraw" => vec![ctx.block("stop_drawing", vec![NULL])],
        "StartFill" => vec![ctx.block("start_fill", vec![NULL])],
        "StopFill" => vec![ctx.block("stop_fill", vec![NULL])],
        "Stamp" => vec![ctx.block("brush_stamp", vec![NULL])],
        "StartTimer" => vec![timer_action("START", ctx)],
        "StopTimer" => vec![timer_action("STOP", ctx)],
        "ResetTimer" => vec![timer_action("RESET", ctx)],
        "PlaySound" => compile_play_sound(node, ctx),
        "PlayBgm" => match resolve_sound(g(node, "name"), ctx) {
            None => vec![],
            Some(sound) => vec![ctx.block("play_bgm", vec![sound, NULL])],
        },
        "StopBgm" => vec![ctx.block("stop_bgm", vec![NULL])],
        "StopSound" => {
            let t = if s(node, "target") == "this" { "thisOnly" } else { "all" };
            vec![ctx.block("sound_silent_all", vec![json!(t), NULL])]
        }
        "Read" => {
            let Some(message) = compile_value(g(node, "value"), ctx) else { return vec![] };
            ctx.add_expansion("tts", true);
            let wait = g(node, "wait").as_bool().unwrap_or(false);
            vec![ctx.block(if wait { "read_text_wait_with_block" } else { "read_text" }, vec![message, NULL])]
        }
        "TtsSetting" => {
            let speaker = tts_option(TTS_SPEAKERS, TTS_SPEAKERS_KNOWN, g(node, "voice"), "목소리", ctx);
            let speed = tts_option(TTS_LEVELS, TTS_LEVELS_KNOWN, g(node, "speed"), "속도", ctx);
            let pitch = tts_option(TTS_LEVELS, TTS_LEVELS_KNOWN, g(node, "pitch"), "음높이", ctx);
            let (Some(a), Some(b), Some(c)) = (speaker, speed, pitch) else { return vec![] };
            ctx.add_expansion("tts", true);
            vec![ctx.block("set_tts_property", vec![json!(a), json!(b), json!(c), NULL])]
        }
        "Ask" => match compile_value(g(node, "question"), ctx) {
            None => vec![],
            Some(q) => vec![ctx.block("ask_and_wait", vec![q, NULL])],
        },
        "ListAdd" => {
            let list = require_list(g(node, "list"), ctx);
            let value = compile_value(g(node, "value"), ctx);
            match (list, value) {
                (Some(l), Some(v)) => vec![ctx.block("add_value_to_list", vec![v, json!(l.id), NULL])],
                _ => vec![],
            }
        }
        "ListInsert" => {
            let list = require_list(g(node, "list"), ctx);
            let value = compile_value(g(node, "value"), ctx);
            let index = compile_value(g(node, "index"), ctx);
            match (list, value, index) {
                (Some(l), Some(v), Some(i)) => vec![ctx.block("insert_value_to_list", vec![v, json!(l.id), i, NULL])],
                _ => vec![],
            }
        }
        "ListRemove" => {
            let list = require_list(g(node, "list"), ctx);
            let index = compile_value(g(node, "index"), ctx);
            match (list, index) {
                (Some(l), Some(i)) => vec![ctx.block("remove_value_from_list", vec![i, json!(l.id), NULL])],
                _ => vec![],
            }
        }
        "TableAddLine" | "TableInsertLine" | "TableRemoveLine" => compile_table_line(node, ctx),
        "TableSave" => match require_table(g(node, "table"), ctx) {
            None => vec![],
            Some(t) => vec![ctx.block("save_current_table", vec![json!(t.id), NULL])],
        },
        "StoreSave" => {
            let is_async = g(node, "async").as_bool().unwrap_or(false);
            let id = require_save_function(is_async, ctx);
            vec![ctx.block(&format!("func_{}", id), vec![NULL])]
        }
        "VarDecl" | "ListDecl" => compile_declaration(node, ctx),
        "Assign" => compile_assign(node, ctx),
        "ExpressionStatement" => compile_call_statement(node, ctx),
        _ => {
            let _: Option<()> = ctx.error(node, format!("'{}' 문장은 아직 엔트리 블록으로 바꿀 수 없습니다.", kind));
            vec![]
        }
    }
}

fn timer_action(action: &str, ctx: &mut Context) -> Value {
    ctx.block("choose_project_timer_action", vec![NULL, json!(action), NULL, NULL])
}

fn compile_angle(node: &Value, ctx: &mut Context) -> Option<Value> {
    if ty(node) == "Number" {
        return Some(ctx.angle(g(node, "value").as_f64().unwrap_or(0.0)));
    }
    if ty(node) == "Unary" && s(node, "operator") == "-" && ty(g(node, "argument")) == "Number" {
        return Some(ctx.angle(-g(g(node, "argument"), "value").as_f64().unwrap_or(0.0)));
    }
    compile_value(node, ctx)
}

fn compile_visibility(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let showing = ty(node) == "Show";
    let target = nn(node, "target");
    if let Some(t) = target {
        if s(t, "name") == "chart" && ctx.lookup_variable("chart").is_none() {
            if showing {
                let _: Option<()> = ctx.error(node, "show chart 는 없습니다. 'show 테이블 chart 1' 처럼 어떤 테이블의 차트인지 적으세요.");
                return vec![];
            }
            return vec![ctx.block("close_table_chart", vec![NULL])];
        }
    }
    if let Some(t) = target {
        if let Some(table) = ctx.table_by_name.get(&s(t, "name")).cloned() {
            return compile_table_window(node, &table, showing, ctx);
        }
    }
    let Some(t) = target else {
        return vec![ctx.block(if showing { "show" } else { "hide" }, vec![NULL])];
    };
    let name = s(t, "name");
    if name == "timer" {
        return vec![ctx.block("set_visible_project_timer", vec![NULL, json!(if showing { "SHOW" } else { "HIDE" }), NULL, NULL])];
    }
    if name == "answer" {
        return vec![ctx.block("set_visible_answer", vec![json!(if showing { "SHOW" } else { "HIDE" }), NULL])];
    }
    match ctx.lookup_variable(&name) {
        Some(VarRef::Ambiguous { name, owners }) => {
            let _: Option<()> = ambiguous_local_error(node, &name, &owners, ctx);
            vec![]
        }
        Some(VarRef::Variable { entry }) => {
            let is_list = entry.variable_type == "list";
            let t = match (is_list, showing) {
                (true, true) => "show_list",
                (true, false) => "hide_list",
                (false, true) => "show_variable",
                (false, false) => "hide_variable",
            };
            vec![ctx.block(t, vec![json!(entry.id), NULL])]
        }
        _ => {
            let _: Option<()> = ctx.error(node, format!("'{}' 은(는) 무대에 표시할 수 있는 변수나 리스트가 아닙니다.", name));
            vec![]
        }
    }
}

fn compile_table_window(node: &Value, table: &TableInfo, showing: bool, ctx: &mut Context) -> Vec<Value> {
    if !showing {
        return vec![ctx.block("close_table_chart", vec![NULL])];
    }
    if let Some(sec) = nn(node, "seconds") {
        return match compile_value(sec, ctx) {
            None => vec![],
            Some(v) => vec![ctx.block("open_table_wait", vec![json!(table.id), v, NULL])],
        };
    }
    if let Some(chart) = nn(node, "chart") {
        return match literal_index(chart, ctx) {
            None => vec![],
            Some(c) => vec![ctx.block("open_table_chart", vec![json!(table.id), json!(c), NULL])],
        };
    }
    vec![ctx.block("open_table", vec![json!(table.id), json!("0"), NULL])]
}

fn literal_index(node: &Value, ctx: &mut Context) -> Option<String> {
    if ty(node) == "Number" {
        return Some(num_to_string(g(node, "value").as_f64().unwrap_or(0.0) - 1.0));
    }
    if ty(node) == "String" {
        return Some(s(node, "value"));
    }
    ctx.error(node, "차트 번호는 숫자로 직접 적어야 합니다.")
}

fn require_table(node: &Value, ctx: &mut Context) -> Option<TableInfo> {
    let name = s(node, "name");
    if let Some(t) = ctx.table_by_name.get(&name) {
        return Some(t.clone());
    }
    let keys: Vec<String> = ctx.table_by_name.keys().map(|k| k.to_string()).collect();
    let hint = did_you_mean(&name, keys.iter().map(|k| k.as_str()));
    ctx.error(node, format!("'{}' 은(는) 테이블이 아닙니다.{}", name, hint))
}

fn compile_table_line(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let Some(table) = require_table(g(node, "table"), ctx) else { return vec![] };
    let property = if s(node, "line") == "row" { "ROW" } else { "COL" };
    if ty(node) == "TableAddLine" {
        return vec![ctx.block("append_row_to_table", vec![json!(table.id), json!(property), NULL])];
    }
    let Some(index) = compile_value(g(node, "index"), ctx) else { return vec![] };
    let t = if ty(node) == "TableInsertLine" { "insert_row_to_table" } else { "delete_row_from_table" };
    vec![ctx.block(t, vec![json!(table.id), index, json!(property), NULL])]
}

fn compile_table_assign(node: &Value, table: &TableInfo, ctx: &mut Context) -> Vec<Value> {
    let target = g(node, "target");
    let Some(value) = compile_value(g(node, "value"), ctx) else { return vec![] };
    match nn(target, "column") {
        None => match compile_value(g(target, "index"), ctx) {
            None => vec![],
            Some(cell) => vec![ctx.block("set_value_from_cell", vec![json!(table.id), cell, value, NULL])],
        },
        Some(col) => {
            let row = compile_value(g(target, "index"), ctx);
            let column = compile_value(col, ctx);
            match (row, column) {
                (Some(r), Some(c)) => vec![ctx.block("set_value_from_table", vec![json!(table.id), r, c, value, NULL])],
                _ => vec![],
            }
        }
    }
}

fn compile_play_sound(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let Some(sound) = resolve_sound(g(node, "name"), ctx) else { return vec![] };
    let wait = g(node, "wait").as_bool().unwrap_or(false);
    if let Some(from_node) = nn(node, "from") {
        let from = compile_value(from_node, ctx);
        let to = compile_value(g(node, "to"), ctx);
        let (Some(from), Some(to)) = (from, to) else { return vec![] };
        return vec![ctx.block(if wait { "sound_from_to_and_wait" } else { "sound_from_to" }, vec![sound, from, to, NULL])];
    }
    if let Some(d) = nn(node, "duration") {
        let Some(dur) = compile_value(d, ctx) else { return vec![] };
        let t = if wait { "sound_something_second_wait_with_block" } else { "sound_something_second_with_block" };
        return vec![ctx.block(t, vec![sound, dur, NULL])];
    }
    let t = if wait { "sound_something_wait_with_block" } else { "sound_something_with_block" };
    vec![ctx.block(t, vec![sound, NULL])]
}

/// Falls back to a runtime lookup by name; `ambiguous` suppresses the notice.
fn by_name_at_runtime(node: &Value, ctx: &mut Context, ambiguous: bool, pictures: bool) -> Option<Value> {
    if ambiguous {
        return compile_value(node, ctx);
    }
    let name = s(node, "value");
    let keys: Vec<String> = match ctx.cur_object() {
        Some(o) => (if pictures { &o.pictures } else { &o.sounds }).keys().map(|k| k.to_string()).collect(),
        None => vec![],
    };
    let label = if pictures { "모양을" } else { "소리를" };
    let declare = if pictures { "costume" } else { "sound" };
    let place = if ctx.object.is_some() { "이 오브젝트에서" } else { "어느 오브젝트에서도" };
    let hint = or_hint(&name, keys.iter().map(|k| k.as_str()), &format!("{} {} \"파일명\" 으로 먼저 등록하세요.", declare, name));
    ctx.notice(node, format!("'{}' {} {} 찾지 못했습니다. 실행할 때 이름으로 찾습니다.{}", name, label, place, hint));
    compile_value(node, ctx)
}

fn resolve_resource(node: &Value, ctx: &mut Context, pictures: bool) -> Option<Value> {
    if ty(node) != "String" {
        return compile_value(node, ctx);
    }
    let name = s(node, "value");
    let own = ctx.cur_object().and_then(|o| (if pictures { &o.pictures } else { &o.sounds }).get(&name).cloned());
    if let Some(a) = own {
        return Some(ctx.block(if pictures { "get_pictures" } else { "get_sounds" }, vec![json!(s(&a, "id"))]));
    }
    if ctx.forced_resource_ids.contains(&name) {
        return Some(json!(name));
    }
    let found = if ctx.object.is_some() { None } else { ctx.lookup_object_resource(!pictures, &name) };
    match found {
        Some(ResourceRef::Found(a)) => Some(ctx.block(if pictures { "get_pictures" } else { "get_sounds" }, vec![json!(s(&a, "id"))])),
        Some(ResourceRef::Ambiguous(_)) => by_name_at_runtime(node, ctx, true, pictures),
        None => by_name_at_runtime(node, ctx, false, pictures),
    }
}

fn resolve_sound(node: &Value, ctx: &mut Context) -> Option<Value> {
    resolve_resource(node, ctx, false)
}

fn resolve_picture(node: &Value, ctx: &mut Context) -> Option<Value> {
    resolve_resource(node, ctx, true)
}

fn require_list(node: &Value, ctx: &mut Context) -> Option<VarInfo> {
    if let Some(l) = resolve_list(node, ctx) {
        return Some(l);
    }
    let label = if ty(node) == "Identifier" { s(node, "name") } else { String::new() };
    ctx.error(node, format!("'{}' 은(는) 리스트가 아닙니다.", label))
}

fn tts_option(table: &[(&'static str, &'static str)], known: &str, node: &Value, label: &str, ctx: &mut Context) -> Option<String> {
    if ty(node) != "String" {
        return ctx.error(node, format!("tts {}은(는) 문자열로 적어야 합니다.", label));
    }
    let raw = s(node, "value");
    let key = raw.trim().to_lowercase();
    match lookup(table, &key) {
        Some(v) => Some(v.to_string()),
        None => ctx.error(node, format!("tts {} '{}' 을(를) 모릅니다. 쓸 수 있는 값: {}", label, raw, known)),
    }
}

fn compile_declaration(node: &Value, ctx: &mut Context) -> Vec<Value> {
    if ty(node) == "ListDecl" {
        return vec![];
    }
    let loc = g(node, "loc").clone();
    let assign = json!({
        "type": "Assign",
        "operator": "=",
        "target": {"type": "Identifier", "name": g(node, "name"), "loc": loc},
        "value": g(node, "value"),
        "loc": loc,
    });
    compile_assign(&assign, ctx)
}

fn compile_assign(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let target = g(node, "target");
    if ty(target) == "Index" {
        return compile_list_element_assign(node, ctx);
    }
    let name = s(target, "name");
    match ctx.lookup_variable(&name) {
        Some(found) => compile_variable_assign(node, found, ctx),
        None => compile_property_assign(node, &name, ctx),
    }
}

fn compile_variable_assign(node: &Value, found: VarRef, ctx: &mut Context) -> Vec<Value> {
    let operator = s(node, "operator");
    match &found {
        VarRef::Ambiguous { name, owners } => {
            let _: Option<()> = ambiguous_local_error(node, name, owners, ctx);
            return vec![];
        }
        VarRef::Param(name) => {
            let _: Option<()> = ctx.error(node, format!("함수 매개변수 '{}' 에는 값을 대입할 수 없습니다.", name));
            return vec![];
        }
        _ => {}
    }
    let read = |ctx: &mut Context| match &found {
        VarRef::FuncLocal { id } => ctx.block("get_func_variable", vec![json!(id), NULL]),
        VarRef::Variable { entry } => ctx.block("get_variable", vec![json!(entry.id), NULL]),
        _ => unreachable!(),
    };
    let write = |ctx: &mut Context, value: Value| match &found {
        VarRef::FuncLocal { id } => ctx.block("set_func_variable", vec![json!(id), value, NULL]),
        VarRef::Variable { entry } => ctx.block("set_variable", vec![json!(entry.id), value, NULL]),
        _ => unreachable!(),
    };

    if operator == "=" {
        return match compile_value(g(node, "value"), ctx) {
            None => vec![],
            Some(v) => vec![write(ctx, v)],
        };
    }
    if operator == "+=" {
        if let VarRef::Variable { entry } = &found {
            return match compile_value(g(node, "value"), ctx) {
                None => vec![],
                Some(v) => vec![ctx.block("change_variable", vec![json!(entry.id), v, NULL])],
            };
        }
    }
    let current = read(ctx);
    match combine(current, &operator, g(node, "value"), ctx) {
        None => vec![],
        Some(c) => vec![write(ctx, c)],
    }
}

fn compile_list_element_assign(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let target = g(node, "target");
    let operator = s(node, "operator");
    let inner = g(target, "target");
    if ty(inner) == "Identifier" {
        if let Some(table) = ctx.table_by_name.get(&s(inner, "name")).cloned() {
            if operator != "=" {
                let _: Option<()> = ctx.error(node, "테이블 칸에는 '=' 로만 값을 넣을 수 있습니다.");
                return vec![];
            }
            return compile_table_assign(node, &table, ctx);
        }
    }
    if nn(target, "column").is_some() {
        let label = if ty(inner) == "Identifier" { s(inner, "name") } else { String::new() };
        let _: Option<()> = ctx.error(node, format!("'{}' 은(는) 테이블이 아니라서 [행, 열] 로 쓸 수 없습니다.", label));
        return vec![];
    }
    let list = require_list(inner, ctx);
    let index = compile_value(g(target, "index"), ctx);
    let (Some(list), Some(index)) = (list, index) else { return vec![] };

    if operator == "=" {
        return match compile_value(g(node, "value"), ctx) {
            None => vec![],
            Some(v) => vec![ctx.block("change_value_list_index", vec![json!(list.id), index, v, NULL])],
        };
    }
    let idx2 = compile_value(g(target, "index"), ctx).unwrap_or(NULL);
    let current = ctx.block("value_of_index_from_list", vec![NULL, json!(list.id), NULL, idx2, NULL]);
    match combine(current, &operator, g(node, "value"), ctx) {
        None => vec![],
        Some(c) => vec![ctx.block("change_value_list_index", vec![json!(list.id), index, c, NULL])],
    }
}

fn combine(current: Value, operator: &str, value_node: &Value, ctx: &mut Context) -> Option<Value> {
    let value = compile_value(value_node, ctx)?;
    let arithmetic = match operator {
        "+=" => Some("PLUS"),
        "-=" => Some("MINUS"),
        "*=" => Some("MULTI"),
        "/=" => Some("DIVIDE"),
        _ => None,
    };
    if let Some(a) = arithmetic {
        return Some(ctx.block("calc_basic", vec![current, json!(a), value]));
    }
    if operator == "%=" {
        return Some(ctx.block("quotient_and_mod", vec![NULL, current, NULL, value, NULL, json!("MOD")]));
    }
    if operator == "**=" {
        let is_num = ty(value_node) == "Number";
        let v = g(value_node, "value").as_f64();
        if is_num && v == Some(2.0) {
            return Some(ctx.block("calc_operation", vec![NULL, current, NULL, json!("square")]));
        }
        if is_num && v == Some(0.5) {
            return Some(ctx.block("calc_operation", vec![NULL, current, NULL, json!("root")]));
        }
        return ctx.error(value_node, "엔트리에서 **= 은 2(제곱) 또는 0.5(제곱근)만 쓸 수 있습니다.");
    }
    ctx.error(value_node, format!("연산자 '{}' 를 엔트리 블록으로 바꿀 수 없습니다.", operator))
}

fn property_value(node: &Value, negate: bool, ctx: &mut Context) -> Option<Value> {
    let compiled = compile_value(g(node, "value"), ctx)?;
    if !negate {
        return Some(compiled);
    }
    let v = g(node, "value");
    if ty(v) == "Number" {
        return Some(ctx.number(-v.get("value").and_then(|x| x.as_f64()).unwrap_or(0.0)));
    }
    let zero = ctx.number(0.0);
    Some(ctx.block("calc_basic", vec![zero, json!("MINUS"), compiled]))
}

fn property_angle(node: &Value, negate: bool, ctx: &mut Context) -> Option<Value> {
    let v = g(node, "value");
    if !negate {
        return compile_angle(v, ctx);
    }
    if ty(v) == "Number" {
        return Some(ctx.angle(-v.get("value").and_then(|x| x.as_f64()).unwrap_or(0.0)));
    }
    let compiled = compile_value(v, ctx)?;
    let zero = ctx.number(0.0);
    Some(ctx.block("calc_basic", vec![zero, json!("MINUS"), compiled]))
}

fn simple(node: &Value, name: &str, set_type: &str, add_type: &str, angle: bool, ctx: &mut Context) -> Vec<Value> {
    let operator = s(node, "operator");
    let relative = operator == "+=" || operator == "-=";
    let negate = operator == "-=";
    let make = |ctx: &mut Context| if angle { property_angle(node, negate, ctx) } else { property_value(node, negate, ctx) };
    if operator == "=" {
        return match make(ctx) {
            None => vec![],
            Some(c) => vec![ctx.block(set_type, vec![c, NULL])],
        };
    }
    if relative && !add_type.is_empty() {
        return match make(ctx) {
            None => vec![],
            Some(c) => vec![ctx.block(add_type, vec![c, NULL])],
        };
    }
    read_modify_write(name, set_type, node, ctx)
}

fn origin_scale(name: &str, ctx: &Context) -> f64 {
    let read = |key: &str| -> Option<f64> {
        let v = ctx.cur_object()?.properties.get(key)?;
        if ty(v) == "Number" {
            v.get("value").and_then(|x| x.as_f64())
        } else {
            None
        }
    };
    read(name).or_else(|| read("size")).unwrap_or(100.0) / 100.0
}

fn compile_color(node: &Value, ctx: &mut Context) -> Option<Value> {
    match ty(node) {
        "Color" => Some(g(node, "value").clone()),
        "Transparent" => Some(json!("transparent")),
        "String" => Some(g(node, "value").clone()),
        _ => compile_value(node, ctx),
    }
}

fn is_transparent(v: &Option<Value>) -> bool {
    matches!(v, Some(Value::String(t)) if t == "transparent")
}

fn reject_runtime_transparent(name: &str, node: &Value, ctx: &mut Context) {
    let _: Option<()> = ctx.error(
        node,
        format!(
            "실행 중에는 {n} 을(를) transparent 로 정할 수 없습니다. 엔트리의 색 블록은 '#' 로 시작하지 \
             않는 값을 받으면 강제로 '#' 를 붙이는데, 그러면 transparent 가 잘못된 색이 되어 오히려 검은색으로 \
             보입니다(엔트리 자체의 한계로, 이 프로젝트가 만들 수 있는 블록으로는 피할 방법이 없습니다). \
             오브젝트/글상자 선언 맨 위에서 {n} = transparent 로 정적으로만 쓸 수 있습니다.",
            n = name
        ),
    );
}

fn compile_property_assign(node: &Value, name: &str, ctx: &mut Context) -> Vec<Value> {
    let operator = s(node, "operator");
    let relative = operator == "+=" || operator == "-=";
    let negate = operator == "-=";
    let value_node = g(node, "value");

    match name {
        "x" => simple(node, name, "locate_x", "move_x", false, ctx),
        "y" => simple(node, name, "locate_y", "move_y", false, ctx),
        "size" => simple(node, name, "set_scale_size", "change_scale_size", false, ctx),
        "angle" => simple(node, name, "rotate_absolute", "rotate_relative", true, ctx),
        "way" => simple(node, name, "direction_absolute", "direction_relative", true, ctx),
        "scale_x" | "scale_y" => {
            if relative {
                let compiled = property_value(node, negate, ctx);
                let dimension = if name == "scale_x" { "WIDTH" } else { "HEIGHT" };
                return match compiled {
                    None => vec![],
                    Some(c) => vec![ctx.block("stretch_scale_size", vec![json!(dimension), c, NULL])],
                };
            }
            if operator != "=" {
                let _: Option<()> = ctx.error(node, format!("{} 에는 = 과 +=, -= 만 쓸 수 있습니다.", name));
                return vec![];
            }
            if ctx.func_scope.is_some() {
                let _: Option<()> = ctx.error(
                    node,
                    format!("{} 을(를) 정하는 일은 함수 안에서 할 수 없습니다. 오브젝트마다 시작 배율이 다르기 때문입니다.", name),
                );
                return vec![];
            }
            let Some(compiled) = compile_value(value_node, ctx) else { return vec![] };
            let setter = require_scale_setter(name, ctx);
            let setter_id = ctx.functions[setter].id.clone();
            let origin = origin_scale(name, ctx);
            let origin_block = ctx.number(origin);
            vec![ctx.block(&format!("func_{}", setter_id), vec![compiled, origin_block, NULL])]
        }
        "costume" => {
            if operator != "=" {
                let _: Option<()> = ctx.error(node, "costume 은 = 로만 바꿀 수 있습니다.");
                return vec![];
            }
            match resolve_picture(value_node, ctx) {
                None => vec![],
                Some(p) => vec![ctx.block("change_to_some_shape", vec![p, NULL])],
            }
        }
        "effect_color" | "effect_brightness" | "effect_alpha" => {
            let effect = lookup(EFFECTS, name).unwrap();
            let Some(compiled) = property_value(node, negate, ctx) else { return vec![] };
            if operator == "=" {
                return vec![ctx.block("change_effect_amount", vec![json!(effect), compiled, NULL])];
            }
            if relative {
                return vec![ctx.block("add_effect_amount", vec![json!(effect), compiled, NULL])];
            }
            let _: Option<()> = ctx.error(node, "효과 값에는 = 과 +=, -= 만 쓸 수 있습니다.");
            vec![]
        }
        "draw_color" | "fill_color" => {
            if operator != "=" {
                let _: Option<()> = ctx.error(node, format!("{} 은(는) = 로만 정할 수 있습니다.", name));
                return vec![];
            }
            if ty(value_node) == "Call" && s(value_node, "callee") == "random_color" {
                if name == "fill_color" {
                    let _: Option<()> = ctx.error(node, "엔트리에는 무작위 채우기 색 블록이 없습니다.");
                    return vec![];
                }
                return vec![ctx.block("set_random_color", vec![NULL])];
            }
            let compiled = compile_color(value_node, ctx);
            if is_transparent(&compiled) {
                reject_runtime_transparent(name, node, ctx);
                return vec![];
            }
            let t = if name == "draw_color" { "set_color" } else { "set_fill_color" };
            match compiled {
                None => vec![],
                Some(c) => vec![ctx.block(t, vec![c, NULL])],
            }
        }
        "draw_width" => simple(node, name, "set_thickness", "change_thickness", false, ctx),
        "draw_alpha" => simple(node, name, "set_brush_tranparency", "change_brush_transparency", false, ctx),
        "sound_volume" => simple(node, name, "sound_volume_set", "sound_volume_change", false, ctx),
        "sound_speed" => simple(node, name, "sound_speed_set", "sound_speed_change", false, ctx),
        "text_content" => {
            if operator != "=" {
                let _: Option<()> = ctx.error(node, "text_content 는 = 로만 바꿀 수 있습니다. 이어 붙이려면 append 를 쓰세요.");
                return vec![];
            }
            match property_value(node, negate, ctx) {
                None => vec![],
                Some(c) => vec![ctx.block("text_write", vec![c, NULL])],
            }
        }
        "font" => {
            if ty(value_node) != "String" {
                let _: Option<()> = ctx.error(node, "글씨체는 문자열로 적어야 합니다.");
                return vec![];
            }
            vec![ctx.block("text_change_font", vec![g(value_node, "value").clone(), NULL])]
        }
        "font_color" | "bg_color" => {
            let compiled = compile_color(value_node, ctx);
            if is_transparent(&compiled) {
                reject_runtime_transparent(name, node, ctx);
                return vec![];
            }
            let t = if name == "font_color" { "text_change_font_color" } else { "text_change_bg_color" };
            match compiled {
                None => vec![],
                Some(c) => vec![ctx.block(t, vec![c, NULL])],
            }
        }
        "text_bold" | "text_italic" | "text_underline" | "text_strikethrough" => {
            if ty(value_node) != "Boolean" {
                let _: Option<()> = ctx.error(node, format!("{} 에는 true 또는 false 만 쓸 수 있습니다.", name));
                return vec![];
            }
            let mode = if g(value_node, "value").as_bool().unwrap_or(false) { "on" } else { "off" };
            vec![ctx.block("text_change_effect", vec![json!(lookup(TEXT_EFFECTS, name).unwrap()), json!(mode), NULL])]
        }
        _ => {
            let known = ctx.known_names();
            let hint = or_hint(name, known.iter().map(|k| k.as_str()), "var 로 먼저 선언하세요.");
            let _: Option<()> = ctx.error(node, format!("선언되지 않은 이름 '{}' 에 값을 대입했습니다.{}", name, hint));
            vec![]
        }
    }
}

fn read_modify_write(name: &str, set_type: &str, node: &Value, ctx: &mut Context) -> Vec<Value> {
    let Some(coordinate) = lookup(READABLE_PROPERTIES, name) else {
        let _: Option<()> = ctx.error(node, format!("'{}' 은(는) {} 연산을 지원하지 않습니다.", name, s(node, "operator")));
        return vec![];
    };
    let current = ctx.block("coordinate_object", vec![NULL, json!("self"), NULL, json!(coordinate)]);
    match combine(current, &s(node, "operator"), g(node, "value"), ctx) {
        None => vec![],
        Some(c) => vec![ctx.block(set_type, vec![c, NULL])],
    }
}

fn compile_call_statement(node: &Value, ctx: &mut Context) -> Vec<Value> {
    let call = g(node, "expression");
    let callee = s(call, "callee");
    let Some(idx) = ctx.function_by_name.get(&callee).copied() else {
        let _: Option<()> = ctx.error(node, format!("'{}' 함수를 찾을 수 없습니다. 내장 함수는 문장으로 쓸 수 없습니다.", callee));
        return vec![];
    };
    let (fid, is_value, params, booleans) = {
        let f = &ctx.functions[idx];
        (f.id.clone(), f.is_value, f.params.clone(), f.boolean_params.clone())
    };
    if is_value {
        let Some(value) = compile_value(call, ctx) else { return vec![] };
        let empty = ctx.text("");
        let cmp = ctx.block("boolean_basic_operator", vec![value, json!("EQUAL"), empty]);
        return vec![ctx.block_with("_if", vec![cmp, NULL], vec![vec![]])];
    }
    let args = list_of(call, "arguments");
    if args.len() != params.len() {
        let _: Option<()> = ctx.error(node, format!("함수 '{}' 는 인자가 {}개여야 합니다.", callee, params.len()));
        return vec![];
    }
    let flags: Vec<bool> = params.iter().map(|p| booleans.contains(p)).collect();
    let compiled = compile_call_arguments(&flags, &args, ctx);
    if compiled.iter().any(|p| p.is_none()) {
        return vec![];
    }
    let mut ps: Vec<Value> = compiled.into_iter().map(|p| p.unwrap()).collect();
    ps.push(NULL);
    vec![ctx.block(&format!("func_{}", fid), ps)]
}
