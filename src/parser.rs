//! Lexer and recursive-descent parser producing the Tess AST as JSON values.
use crate::core::{color_literal_length, normalize_color};
use serde_json::{json, Map, Value};

pub const KEYWORDS: &[&str] = &[
    "add", "all", "and", "append", "as", "ask", "async", "at", "back", "bgm", "bounce", "break",
    "bubble", "call", "center", "clear", "click", "clone", "cloned", "clones", "chart", "column",
    "columns", "continue", "costume", "default", "del", "description", "do", "draw", "effects",
    "else", "end", "false", "fill", "flip", "for", "force", "forever", "forward", "fps", "free",
    "front", "first", "function", "go", "hide", "id", "if", "in", "insert", "jump", "key", "kill",
    "last", "list", "lock", "look", "me", "move", "name", "next", "none", "not", "object", "or",
    "order", "other", "pitch", "play", "prepend", "prev", "project", "read", "realtime", "remove",
    "repeat", "reset", "restart", "return", "rotation", "row", "save", "say", "scene", "send",
    "from", "series", "shared", "signal", "show", "size", "skip", "sound", "speed", "stage", "stamp",
    "start", "steer", "stop", "store", "table", "text", "them", "then", "think", "this", "timer",
    "title", "to", "transparent", "true", "tts", "turn", "until", "up", "use", "useobject",
    "usetext", "var", "vertical", "visible", "voice", "wait", "when", "while", "write", "x", "y",
];

const RESERVED: &[&str] = &["and", "or", "not", "true", "false", "end", "then", "do", "in", "wait"];
const STANDALONE: &[&str] = &[
    "break", "continue", "skip", "restart", "stop", "bounce", "stamp", "show", "hide", "clone", "kill",
];
const STATEMENT_KEYWORDS: &[&str] = &[
    "if", "repeat", "while", "until", "forever", "wait", "return", "break", "continue", "skip",
    "restart", "stop", "start", "reset", "clear", "send", "call", "clone", "del", "kill", "jump",
    "forward", "bounce", "move", "go", "turn", "steer", "look", "show", "hide", "next", "prev",
    "say", "think", "flip", "order", "write", "append", "prepend", "stamp", "play", "read", "tts",
    "in", "remove", "ask", "var", "list", "save", "store",
];
const PROPERTY_NAMES: &[&str] = &[
    "scale_x", "scale_y", "size", "text_content", "text_bold", "text_italic", "text_underline",
    "text_strikethrough", "text_align", "font_color", "font_size", "font", "bg_color",
    "line_break", "draw_color", "draw_width", "draw_alpha", "fill_color", "angle", "way", "x", "y",
];
const ASSIGN_OPS: &[&str] = &["+=", "-=", "**=", "*=", "/=", "%=", "="];

const ID: &str = "#ID";
const NUM: &str = "#NUM";
const STR: &str = "#STR";
const COL: &str = "#COL";
const EOF: &str = "#EOF";

#[derive(Clone, Debug)]
pub struct Tok {
    pub ty: &'static str,
    pub image: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct PErr {
    pub offset: usize,
    pub message: String,
}

pub struct Failure {
    pub offset: usize,
    pub message: String,
    pub detail: String,
}

const OPERATORS: &[&str] = &[
    "**=", "**", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<=", ">=", "//", "+", "-", "*", "/",
    "%", "<", ">", "=", "(", ")", "[", "]", ",", ":", "?",
];

fn is_js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// Lexes `chars`; offsets are UTF-16 code unit offsets. Err carries (offset, char).
pub fn tokenize(chars: &[char]) -> Result<Vec<Tok>, (usize, Option<char>)> {
    let mut toks = Vec::new();
    let mut i = 0usize;
    let mut off = 0usize;
    let mut line = 1usize;
    let n = chars.len();
    let adv = |c: char, off: &mut usize, line: &mut usize, prev: Option<char>, next: Option<char>| {
        *off += c.len_utf16();
        if c == '\n' || (c == '\r' && next != Some('\n')) {
            *line += 1;
        }
        let _ = prev;
    };
    while i < n {
        let c = chars[i];
        if is_js_space(c) {
            adv(c, &mut off, &mut line, None, chars.get(i + 1).copied());
            i += 1;
            continue;
        }
        if c == '#' {
            let len = color_literal_length(chars, i);
            if len > 0 {
                let image: String = chars[i..i + len].iter().collect();
                let w: usize = image.chars().map(|c| c.len_utf16()).sum();
                toks.push(Tok { ty: COL, image, start: off, end: off + w, line, end_line: line });
                off += w;
                i += len;
            } else {
                while i < n && !matches!(chars[i], '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    off += chars[i].len_utf16();
                    i += 1;
                }
            }
            continue;
        }
        if c == '"' {
            let start = i;
            let (start_off, start_line) = (off, line);
            let mut j = i + 1;
            let mut w = 1usize;
            let mut ln = line;
            let mut ok = false;
            while j < n {
                let d = chars[j];
                if d == '"' {
                    w += 1;
                    j += 1;
                    ok = true;
                    break;
                }
                if matches!(d, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    break;
                }
                if d == '\\' {
                    if j + 1 >= n {
                        break;
                    }
                    let e = chars[j + 1];
                    w += 1 + e.len_utf16();
                    if e == '\n' || (e == '\r' && chars.get(j + 2) != Some(&'\n')) {
                        ln += 1;
                    }
                    j += 2;
                    continue;
                }
                w += d.len_utf16();
                j += 1;
            }
            if !ok {
                return Err((off, Some('"')));
            }
            let image: String = chars[start..j].iter().collect();
            toks.push(Tok { ty: STR, image, start: start_off, end: start_off + w, line: start_line, end_line: ln });
            off += w;
            line = ln;
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j + 1 < n && chars[j] == '.' && chars[j + 1].is_ascii_digit() {
                j += 1;
                while j < n && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let image: String = chars[i..j].iter().collect();
            let w = j - i;
            toks.push(Tok { ty: NUM, image, start: off, end: off + w, line, end_line: line });
            off += w;
            i = j;
            continue;
        }
        let mut matched = None;
        for op in OPERATORS {
            let oc: Vec<char> = op.chars().collect();
            if i + oc.len() <= n && chars[i..i + oc.len()] == oc[..] {
                matched = Some(*op);
                break;
            }
        }
        if let Some(op) = matched {
            toks.push(Tok { ty: op, image: op.to_string(), start: off, end: off + op.len(), line, end_line: line });
            off += op.len();
            i += op.len();
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            let mut w = 0;
            while j < n && (chars[j].is_alphabetic() || chars[j].is_ascii_digit() || chars[j] == '_') {
                w += chars[j].len_utf16();
                j += 1;
            }
            let image: String = chars[i..j].iter().collect();
            let ty = KEYWORDS.iter().find(|k| **k == image).copied().unwrap_or(ID);
            toks.push(Tok { ty, image, start: off, end: off + w, line, end_line: line });
            off += w;
            i = j;
            continue;
        }
        return Err((off, Some(c)));
    }
    toks.push(Tok { ty: EOF, image: String::new(), start: usize::MAX, end: usize::MAX, line, end_line: line });
    Ok(toks)
}

pub fn decode_string(image: &str) -> String {
    let chars: Vec<char> = image.chars().collect();
    let raw = &chars[1..chars.len() - 1];
    let mut out = String::new();
    let mut units: Vec<u16> = Vec::new();
    let flush = |units: &mut Vec<u16>, out: &mut String| {
        if !units.is_empty() {
            out.push_str(&String::from_utf16_lossy(units));
            units.clear();
        }
    };
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != '\\' {
            flush(&mut units, &mut out);
            out.push(raw[i]);
            i += 1;
            continue;
        }
        let next = raw.get(i + 1).copied();
        let hex: String = raw.iter().skip(i + 2).take(4).collect();
        if next == Some('u') && hex.chars().count() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            units.push(u16::from_str_radix(&hex, 16).unwrap());
            i += 6;
            continue;
        }
        flush(&mut units, &mut out);
        match next {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('v') => out.push('\u{b}'),
            Some('0') => out.push('\0'),
            Some(c) => out.push(c),
            None => {}
        }
        i += 2;
    }
    flush(&mut units, &mut out);
    out
}

fn is_ident_like_ty(ty: &str) -> bool {
    ty == ID || (KEYWORDS.contains(&ty) && !RESERVED.contains(&ty))
}

fn label(ty: &str) -> String {
    match ty {
        ID => "이름".into(),
        NUM => "숫자".into(),
        STR => "문자열".into(),
        COL => "색상".into(),
        t => format!("'{}'", t),
    }
}

/// Span-carrying expression result: node plus first/last token offsets of its rule.
pub struct E {
    pub v: Value,
    pub s: usize,
    pub e: usize,
}

type R<T> = Result<T, PErr>;

pub struct Parser {
    pub toks: Vec<Tok>,
    pub pos: usize,
    pub src_len: usize,
}

fn loc(s: usize, e: usize) -> Value {
    json!({"start": s, "end": e})
}

fn num_value(image: &str) -> f64 {
    image.parse::<f64>().unwrap_or(0.0)
}

fn num(v: f64) -> Value {
    json!(v)
}

impl Parser {
    pub fn new(toks: Vec<Tok>, src_len: usize) -> Self {
        Parser { toks, pos: 0, src_len }
    }

    fn la(&self, n: usize) -> &Tok {
        let i = self.pos + n - 1;
        self.toks.get(i).unwrap_or_else(|| self.toks.last().unwrap())
    }

    fn ty(&self, n: usize) -> &'static str {
        self.la(n).ty
    }

    fn prev_end(&self) -> usize {
        self.toks[self.pos - 1].end
    }

    fn cur_start(&self) -> usize {
        self.la(1).start
    }

    fn found(&self, t: &Tok) -> String {
        if t.ty == EOF {
            "입력이 끝났습니다".into()
        } else {
            format!("'{}' 이(가) 있습니다", t.image)
        }
    }

    fn off_of(&self, t: &Tok) -> usize {
        if t.start == usize::MAX { self.src_len } else { t.start }
    }

    fn err_mismatch(&self, expected: &str) -> PErr {
        let t = self.la(1);
        PErr {
            offset: self.off_of(t),
            message: format!("{} 이(가) 와야 하는데 {}.", label(expected), self.found(t)),
        }
    }

    fn err_no_alt(&self) -> PErr {
        let t = self.la(1);
        PErr {
            offset: self.off_of(t),
            message: format!("여기서 시작할 수 있는 문법이 없습니다 — {}.", self.found(t)),
        }
    }

    fn consume(&mut self, ty: &str) -> R<Tok> {
        if self.ty(1) == ty {
            self.pos += 1;
            Ok(self.toks[self.pos - 1].clone())
        } else {
            Err(self.err_mismatch(ty))
        }
    }

    fn consume_ident(&mut self) -> R<Tok> {
        if is_ident_like_ty(self.ty(1)) {
            self.pos += 1;
            Ok(self.toks[self.pos - 1].clone())
        } else {
            Err(self.err_mismatch(ID))
        }
    }

    fn same_line(&self) -> bool {
        self.pos > 0 && self.la(1).line == self.toks[self.pos - 1].end_line
    }

    fn is_ident_like(&self, n: usize) -> bool {
        is_ident_like_ty(self.ty(n))
    }

    fn starts_expr_at(&self, n: usize) -> bool {
        let t = self.ty(n);
        matches!(t, "(" | "-" | "not" | NUM | STR | "true" | "false" | COL) || is_ident_like_ty(t)
    }

    fn leads(&self, ty: &str) -> bool {
        if self.ty(1) != ty {
            return false;
        }
        if STANDALONE.contains(&ty) {
            return true;
        }
        let next = self.ty(2);
        !(ASSIGN_OPS.contains(&next) || next == "[")
    }

    fn leads_any(&self, tys: &[&str]) -> bool {
        tys.iter().any(|t| self.leads(t))
    }

    fn leads_decl(&self, ty: &str) -> bool {
        if self.leads(ty) {
            return true;
        }
        matches!(self.ty(1), "shared" | "realtime" | "store") && self.ty(2) == ty
    }

    fn at_block_end(&self) -> bool {
        matches!(self.ty(1), "end" | EOF)
    }

    fn starts_top_level(&self) -> bool {
        matches!(
            self.ty(1),
            "project" | "scene" | "object" | "text" | "function" | "useobject" | "usetext" | "use"
                | "var" | "list" | "shared" | "realtime" | "store" | "table"
        )
    }

    fn starts_statement(&self) -> bool {
        let t = self.ty(1);
        if t == EOF {
            return false;
        }
        if STATEMENT_KEYWORDS.contains(&t) {
            return true;
        }
        if !is_ident_like_ty(t) {
            return false;
        }
        let next = self.ty(2);
        ASSIGN_OPS.contains(&next) || next == "[" || next == "("
    }

    // ---------------------------------------------------------------- program
    pub fn program(&mut self) -> R<Value> {
        let mut body = Vec::new();
        while self.starts_top_level() {
            body.push(self.top_level_item()?);
        }
        let start = if self.toks.len() > 1 { self.toks[0].start } else { self.src_len };
        Ok(json!({"type": "Program", "body": body, "loc": {"start": start, "end": self.src_len}}))
    }

    fn fragment(&mut self, scene: bool) -> R<Value> {
        let mut members = Vec::new();
        while !self.at_block_end() {
            members.push(if scene { self.scene_member()? } else { self.object_member()? });
        }
        Ok(Value::Array(members))
    }

    fn top_level_item(&mut self) -> R<Value> {
        match self.ty(1) {
            "project" => self.project_decl(),
            "scene" => self.scene_decl(),
            "object" | "text" => self.object_decl(),
            "function" => self.function_decl(),
            "useobject" | "usetext" => self.use_object_decl(),
            "use" => self.use_decl(),
            "var" | "shared" | "realtime" | "store" | "list" => self.var_or_list_decl(),
            "table" => self.table_decl(),
            _ => Err(self.err_no_alt()),
        }
    }

    fn var_or_list_decl(&mut self) -> R<Value> {
        let t = self.ty(1);
        if t == "var" || t == "list" {
            return if t == "var" { self.var_decl() } else { self.list_decl() };
        }
        match self.ty(2) {
            "var" => self.var_decl(),
            "list" => self.list_decl(),
            _ => Err(self.err_no_alt()),
        }
    }

    fn table_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("table")?;
        let name = self.identifier()?;
        let display = if self.ty(1) == "as" { Some(self.display_name()?) } else { None };
        self.consume(":")?;
        self.consume("columns")?;
        let columns = self.table_cells()?;
        let mut rows = Vec::new();
        while self.ty(1) == "row" {
            self.consume("row")?;
            rows.push(Value::Array(self.table_cells()?));
        }
        let mut charts = Vec::new();
        while self.ty(1) == "chart" {
            charts.push(self.table_chart()?);
        }
        self.consume("end")?;
        Ok(json!({
            "type": "TableDecl", "name": name["name"], "displayName": display,
            "columns": columns, "rows": rows, "charts": charts, "loc": loc(s, self.prev_end())
        }))
    }

    fn table_chart(&mut self) -> R<Value> {
        self.consume("chart")?;
        let kind = self.identifier()?;
        let title = if self.ty(1) == STR { Some(Value::String(decode_string(&self.consume(STR)?.image))) } else { None };
        let x = if self.ty(1) == "x" { self.consume("x")?; Some(self.unary_expr()?.v) } else { None };
        let y = if self.ty(1) == "y" { self.consume("y")?; Some(self.unary_expr()?.v) } else { None };
        let mut series = Vec::new();
        if self.ty(1) == "series" {
            self.consume("series")?;
            series.push(self.unary_expr()?.v);
            while self.ty(1) == "," {
                self.consume(",")?;
                series.push(self.unary_expr()?.v);
            }
        }
        Ok(json!({"kind": kind["name"], "title": title, "x": x, "y": y, "series": series}))
    }

    fn table_cells(&mut self) -> R<Vec<Value>> {
        let mut cells = vec![self.expr()?.v];
        while self.ty(1) == "," {
            self.consume(",")?;
            cells.push(self.expr()?.v);
        }
        Ok(cells)
    }

    fn use_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("use")?;
        let p = self.consume(STR)?;
        Ok(json!({"type": "Use", "path": decode_string(&p.image), "loc": loc(s, self.prev_end())}))
    }

    fn use_object_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        let k = self.la(1).image.clone();
        self.pos += 1;
        let p = self.consume(STR)?;
        Ok(json!({
            "type": "UseObject", "kind": if k == "useobject" { "object" } else { "text" },
            "path": decode_string(&p.image), "loc": loc(s, self.prev_end())
        }))
    }

    fn string_node(t: &Tok) -> Value {
        json!({"type": "String", "value": decode_string(&t.image), "loc": loc(t.start, t.end)})
    }

    fn project_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("project")?;
        self.block_open()?;
        let mut fields = Vec::new();
        while !self.at_block_end() {
            let fs = self.cur_start();
            let field = match self.ty(1) {
                "title" | "description" => {
                    let f = self.la(1).image.clone();
                    self.pos += 1;
                    let t = self.consume(STR)?;
                    (f, Self::string_node(&t))
                }
                "fps" => {
                    self.pos += 1;
                    let t = self.consume(NUM)?;
                    ("fps".to_string(), json!({"type": "Number", "value": num_value(&t.image), "loc": loc(t.start, t.end)}))
                }
                _ => return Err(self.err_no_alt()),
            };
            fields.push(json!({"type": "ProjectField", "field": field.0, "value": field.1, "loc": loc(fs, self.prev_end())}));
        }
        self.consume("end")?;
        Ok(json!({"type": "Project", "fields": fields, "loc": loc(s, self.prev_end())}))
    }

    fn scene_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("scene")?;
        let name = self.consume(STR)?;
        self.block_open()?;
        let mut body = Vec::new();
        while !self.at_block_end() {
            body.push(self.scene_member()?);
        }
        self.consume("end")?;
        Ok(json!({"type": "Scene", "name": decode_string(&name.image), "body": body, "loc": loc(s, self.prev_end())}))
    }

    fn scene_member(&mut self) -> R<Value> {
        match self.ty(1) {
            "object" | "text" => self.object_decl(),
            "useobject" | "usetext" => self.use_object_decl(),
            "use" => self.use_decl(),
            "name" => {
                let s = self.cur_start();
                self.consume("name")?;
                let t = self.consume(STR)?;
                Ok(json!({"type": "Property", "name": "name", "value": Self::string_node(&t), "loc": loc(s, self.prev_end())}))
            }
            _ => Err(self.err_no_alt()),
        }
    }

    fn object_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        let kind = self.la(1).image.clone();
        self.pos += 1;
        let name = self.consume(STR)?;
        self.block_open()?;
        let mut body = Vec::new();
        while !self.at_block_end() {
            body.push(self.object_member()?);
        }
        self.consume("end")?;
        Ok(json!({"type": "Object", "kind": kind, "name": decode_string(&name.image), "body": body, "loc": loc(s, self.prev_end())}))
    }

    fn object_member(&mut self) -> R<Value> {
        match self.ty(1) {
            "var" | "shared" | "realtime" | "store" | "list" => self.var_or_list_decl(),
            "function" => self.function_decl(),
            "when" => self.event_handler(),
            "use" => self.use_decl(),
            _ => self.property_decl(),
        }
    }

    fn property_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        match self.ty(1) {
            "default" | "costume" => self.costume_property(),
            "sound" => self.sound_property(),
            "name" if self.ty(2) == STR => {
                self.consume("name")?;
                let t = self.consume(STR)?;
                Ok(json!({"type": "Property", "name": "name", "value": Self::string_node(&t), "loc": loc(s, self.prev_end())}))
            }
            "visible" | "lock" => {
                let flag = self.la(1).image.clone();
                self.pos += 1;
                let v = self.boolean_literal()?;
                Ok(json!({"type": "Property", "name": flag, "value": v, "loc": loc(s, self.prev_end())}))
            }
            "rotation" => {
                self.consume("rotation")?;
                let m = match self.ty(1) {
                    "free" | "vertical" | "none" => self.la(1).image.clone(),
                    _ => return Err(self.err_no_alt()),
                };
                self.pos += 1;
                Ok(json!({"type": "Property", "name": "rotation", "value": {"type": "Keyword", "name": m}, "loc": loc(s, self.prev_end())}))
            }
            "size" if self.ty(2) == NUM => {
                self.consume("size")?;
                let w = self.consume(NUM)?;
                let h = self.consume(NUM)?;
                Ok(json!({"type": "BoxSize", "width": num_value(&w.image), "height": num_value(&h.image), "loc": loc(s, self.prev_end())}))
            }
            "center" => {
                self.consume("center")?;
                let x = self.signed_number()?;
                let y = self.signed_number()?;
                Ok(json!({"type": "Center", "x": x["value"], "y": y["value"], "loc": loc(s, self.prev_end())}))
            }
            t if t == "size" || t == "x" || t == "y" || (t == ID && PROPERTY_NAMES.contains(&self.la(1).image.as_str())) => {
                let name = self.la(1).image.clone();
                self.pos += 1;
                self.consume("=")?;
                let v = self.expr()?.v;
                Ok(json!({"type": "Property", "name": name, "value": v, "loc": loc(s, self.prev_end())}))
            }
            _ => Err(self.err_no_alt()),
        }
    }

    fn signed_number(&mut self) -> R<Value> {
        let s = self.cur_start();
        let mut sign = false;
        if self.ty(1) == "-" && self.ty(2) == NUM && self.la(2).start == self.la(1).end {
            self.pos += 1;
            sign = true;
        }
        let n = self.consume(NUM)?;
        let v = num_value(&n.image) * if sign { -1.0 } else { 1.0 };
        Ok(json!({"type": "Number", "value": v, "loc": loc(s, self.prev_end())}))
    }

    fn display_name(&mut self) -> R<String> {
        self.consume("as")?;
        Ok(decode_string(&self.consume(STR)?.image))
    }

    fn force_id(&mut self) -> R<String> {
        self.consume("force")?;
        self.consume("id")?;
        Ok(decode_string(&self.consume(STR)?.image))
    }

    fn costume_property(&mut self) -> R<Value> {
        let s = self.cur_start();
        let is_default = if self.ty(1) == "default" { self.pos += 1; true } else { false };
        self.consume("costume")?;
        let id = self.identifier()?;
        let file = self.consume(STR)?;
        let (mut w, mut h) = (Value::Null, Value::Null);
        if self.ty(1) == "size" {
            self.pos += 1;
            w = num(num_value(&self.consume(NUM)?.image));
            h = num(num_value(&self.consume(NUM)?.image));
        }
        let display = if self.ty(1) == "as" { Value::String(self.display_name()?) } else { Value::Null };
        let force = if self.ty(1) == "force" { Value::String(self.force_id()?) } else { Value::Null };
        Ok(json!({
            "type": "Costume", "id": id["name"], "displayName": display, "file": decode_string(&file.image),
            "isDefault": is_default, "width": w, "height": h, "forceId": force, "loc": loc(s, self.prev_end())
        }))
    }

    fn sound_property(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("sound")?;
        let id = self.identifier()?;
        let file = self.consume(STR)?;
        let mut duration = Value::Null;
        if self.ty(1) == "for" {
            self.pos += 1;
            duration = num(num_value(&self.consume(NUM)?.image));
        }
        let display = if self.ty(1) == "as" { Value::String(self.display_name()?) } else { Value::Null };
        let force = if self.ty(1) == "force" { Value::String(self.force_id()?) } else { Value::Null };
        Ok(json!({
            "type": "Sound", "id": id["name"], "displayName": display, "file": decode_string(&file.image),
            "duration": duration, "forceId": force, "loc": loc(s, self.prev_end())
        }))
    }

    fn function_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("function")?;
        let name = self.identifier()?;
        self.consume("(")?;
        let mut params: Vec<String> = Vec::new();
        let mut bools: Vec<String> = Vec::new();
        if self.is_ident_like(1) {
            loop {
                let p = self.identifier()?;
                let pname = p["name"].as_str().unwrap().to_string();
                if self.ty(1) == "?" {
                    self.pos += 1;
                    bools.push(pname.clone());
                }
                params.push(pname);
                if self.ty(1) == "," {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        self.consume(")")?;
        self.block_open()?;
        let body = self.block()?;
        self.consume("end")?;
        Ok(json!({
            "type": "FunctionDecl", "name": name["name"], "params": params, "booleanParams": bools,
            "body": body, "loc": loc(s, self.prev_end())
        }))
    }

    fn storage_scope(&mut self) -> Option<&'static str> {
        let r = match self.ty(1) {
            "shared" => Some("shared"),
            "realtime" => Some("realtime"),
            "store" => Some("store"),
            _ => None,
        };
        if r.is_some() {
            self.pos += 1;
        }
        r
    }

    fn at_pair(&mut self) -> R<Value> {
        self.consume("at")?;
        let x = self.unary_expr()?.v;
        let y = self.unary_expr()?.v;
        Ok(json!({"x": x, "y": y}))
    }

    fn var_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        let scope = self.storage_scope();
        self.consume("var")?;
        let name = self.identifier()?;
        let display = if self.ty(1) == "as" { Value::String(self.display_name()?) } else { Value::Null };
        self.consume("=")?;
        let value = self.expr()?.v;
        let mut range = Value::Null;
        if self.ty(1) == "from" {
            self.pos += 1;
            let min = self.expr()?.v;
            self.consume("to")?;
            let max = self.expr()?.v;
            range = json!({"min": min, "max": max});
        }
        let shown = if self.ty(1) == "visible" { self.pos += 1; true } else { false };
        let at = if self.ty(1) == "at" { self.at_pair()? } else { Value::Null };
        Ok(json!({
            "type": "VarDecl", "name": name["name"], "displayName": display, "scope": scope,
            "value": value, "range": range, "shown": shown, "at": at, "loc": loc(s, self.prev_end())
        }))
    }

    fn list_decl(&mut self) -> R<Value> {
        let s = self.cur_start();
        let scope = self.storage_scope();
        self.consume("list")?;
        let name = self.identifier()?;
        let display = if self.ty(1) == "as" { Value::String(self.display_name()?) } else { Value::Null };
        self.consume("=")?;
        let value = self.list_literal()?;
        let shown = if self.ty(1) == "visible" { self.pos += 1; true } else { false };
        let at = if self.ty(1) == "at" { self.at_pair()? } else { Value::Null };
        let mut size = Value::Null;
        if self.ty(1) == "size" {
            self.pos += 1;
            let w = self.unary_expr()?.v;
            let h = self.unary_expr()?.v;
            size = json!({"width": w, "height": h});
        }
        Ok(json!({
            "type": "ListDecl", "name": name["name"], "displayName": display, "scope": scope,
            "value": value, "shown": shown, "at": at, "size": size, "loc": loc(s, self.prev_end())
        }))
    }

    fn event_handler(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("when")?;
        let mut key: Option<String> = None;
        let mut signal: Option<String> = None;
        let event = match self.ty(1) {
            "scene" => {
                self.pos += 1;
                self.consume("start")?;
                "scene_start"
            }
            "start" => { self.pos += 1; "start" }
            "key" => {
                self.pos += 1;
                key = Some(decode_string(&self.consume(STR)?.image));
                if self.ty(1) == "up" { self.pos += 1; "key_up" } else { "key" }
            }
            "stage" => {
                self.pos += 1;
                self.consume("click")?;
                if self.ty(1) == "up" { self.pos += 1; "stage_click_up" } else { "stage_click" }
            }
            "click" => {
                self.pos += 1;
                if self.ty(1) == "up" { self.pos += 1; "click_up" } else { "click" }
            }
            "signal" => {
                self.pos += 1;
                signal = Some(decode_string(&self.consume(STR)?.image));
                "signal"
            }
            "cloned" => { self.pos += 1; "cloned" }
            _ => return Err(self.err_no_alt()),
        };
        self.block_open()?;
        let body = self.block()?;
        self.consume("end")?;
        let l = loc(s, self.prev_end());
        let mut m = Map::new();
        m.insert("type".into(), json!("Event"));
        m.insert("event".into(), json!(event));
        if let Some(k) = key { m.insert("key".into(), json!(k)); }
        if let Some(k) = signal { m.insert("signal".into(), json!(k)); }
        m.insert("body".into(), Value::Array(body));
        m.insert("loc".into(), l);
        Ok(Value::Object(m))
    }

    // ------------------------------------------------------------- statements
    fn block_open(&mut self) -> R<()> {
        match self.ty(1) {
            ":" | "then" | "do" => { self.pos += 1; Ok(()) }
            _ => Err(self.err_no_alt()),
        }
    }

    fn block(&mut self) -> R<Vec<Value>> {
        let mut out = Vec::new();
        while self.starts_statement() {
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn statement(&mut self) -> R<Value> {
        let s = self.cur_start();
        let t = self.ty(1);
        macro_rules! lead { ($($k:literal),+) => { self.leads_any(&[$($k),+]) }; }
        let v = if self.leads("if") { self.if_statement(s)? }
        else if self.leads("repeat") { self.loop_statement("Repeat", "repeat", "count", s)? }
        else if self.leads("while") { self.loop_statement("While", "while", "test", s)? }
        else if self.leads("until") { self.loop_statement("Until", "until", "test", s)? }
        else if self.leads("forever") {
            self.pos += 1;
            self.block_open()?;
            let body = self.block()?;
            self.consume("end")?;
            json!({"type": "Forever", "body": body, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("wait") { self.pos += 1; let v = self.expr()?.v; json!({"type": "Wait", "value": v, "loc": loc(s, self.prev_end())}) }
        else if self.leads("return") { self.pos += 1; let v = self.expr()?.v; json!({"type": "Return", "value": v, "loc": loc(s, self.prev_end())}) }
        else if lead!("break", "continue", "skip", "restart") {
            let ty = match t { "break" => "Break", "continue" => "Continue", "skip" => "Skip", _ => "Restart" };
            self.pos += 1;
            json!({"type": ty, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("stop") { self.stop_statement(s)? }
        else if self.leads("start") {
            self.pos += 1;
            let ty = match self.ty(1) { "draw" => "StartDraw", "fill" => "StartFill", "timer" => "StartTimer", _ => return Err(self.err_no_alt()) };
            self.pos += 1;
            json!({"type": ty, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("reset") {
            self.pos += 1;
            let ty = match self.ty(1) { "size" => "ResetSize", "timer" => "ResetTimer", _ => return Err(self.err_no_alt()) };
            self.pos += 1;
            json!({"type": ty, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("clear") {
            self.pos += 1;
            let w = match self.ty(1) { "effects" | "bubble" | "draw" | "text" => self.la(1).image.clone(), _ => return Err(self.err_no_alt()) };
            self.pos += 1;
            json!({"type": "Clear", "target": w, "loc": loc(s, self.prev_end())})
        }
        else if lead!("send", "call") {
            let wait = t == "call";
            self.pos += 1;
            let sig = self.expr()?.v;
            json!({"type": "Send", "signal": sig, "wait": wait, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("clone") {
            self.pos += 1;
            let target = if self.same_line() && self.starts_expr_at(1) { self.expr()?.v } else { Value::Null };
            json!({"type": "Clone", "target": target, "loc": loc(s, self.prev_end())})
        }
        else if lead!("del", "kill") {
            self.pos += 1;
            let ty = if t == "del" {
                match self.ty(1) { "clones" => "DeleteClones", "clone" => "DeleteClone", _ => return Err(self.err_no_alt()) }
            } else { "DeleteClone" };
            if t == "del" { self.pos += 1; }
            json!({"type": ty, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("jump") {
            self.pos += 1;
            let target = if matches!(self.ty(1), "next" | "back") {
                let w = self.la(1).image.clone(); self.pos += 1; Value::String(w)
            } else { self.expr()?.v };
            json!({"type": "Jump", "target": target, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("forward") {
            self.pos += 1;
            let d = self.expr()?.v;
            let a = if self.ty(1) == "at" { self.pos += 1; self.expr()?.v } else { Value::Null };
            json!({"type": "Forward", "distance": d, "angle": a, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("bounce") { self.pos += 1; json!({"type": "Bounce", "loc": loc(s, self.prev_end())}) }
        else if self.leads("move") {
            self.pos += 1;
            let (x, y, d) = self.point_args()?;
            json!({"type": "Move", "x": x, "y": y, "duration": d, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("go") { self.go_statement(s)? }
        else if lead!("turn", "steer") {
            let ty = if t == "turn" { "Turn" } else { "Steer" };
            self.pos += 1;
            let a = self.expr()?.v;
            let d = if self.same_line() && self.ty(1) == "in" { self.pos += 1; self.expr()?.v } else { Value::Null };
            json!({"type": ty, "angle": a, "duration": d, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("look") { self.pos += 1; let v = self.expr()?.v; json!({"type": "Look", "target": v, "loc": loc(s, self.prev_end())}) }
        else if lead!("show", "hide") { self.show_hide(s)? }
        else if lead!("next", "prev") {
            let d = t.to_string();
            self.pos += 1;
            self.consume("costume")?;
            json!({"type": "CostumeStep", "direction": d, "loc": loc(s, self.prev_end())})
        }
        else if lead!("say", "think") {
            let ty = if t == "say" { "Say" } else { "Think" };
            self.pos += 1;
            let m = self.expr()?.v;
            let d = if self.ty(1) == "for" { self.pos += 1; self.expr()?.v } else { Value::Null };
            json!({"type": ty, "message": m, "duration": d, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("flip") {
            self.pos += 1;
            let a = match self.ty(1) { "x" | "y" => self.la(1).image.clone(), _ => return Err(self.err_no_alt()) };
            self.pos += 1;
            json!({"type": "Flip", "axis": a, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("order") {
            self.pos += 1;
            let a = match self.ty(1) { "front" | "back" | "first" | "last" => self.la(1).image.clone(), _ => return Err(self.err_no_alt()) };
            self.pos += 1;
            json!({"type": "Order", "to": a, "loc": loc(s, self.prev_end())})
        }
        else if lead!("write", "append", "prepend") {
            let mode = t.to_string();
            self.pos += 1;
            let v = self.expr()?.v;
            json!({"type": "TextWrite", "mode": mode, "value": v, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("stamp") { self.pos += 1; json!({"type": "Stamp", "loc": loc(s, self.prev_end())}) }
        else if self.leads("play") { self.play_statement(s)? }
        else if self.leads("read") {
            self.pos += 1;
            let v = self.expr()?.v;
            let mut wait = false;
            if self.ty(1) == "and" { self.pos += 1; self.consume("wait")?; wait = true; }
            json!({"type": "Read", "value": v, "wait": wait, "loc": loc(s, self.prev_end())})
        }
        else if self.leads("tts") {
            self.pos += 1;
            self.consume("voice")?;
            let v = self.consume(STR)?;
            self.consume("speed")?;
            let sp = self.consume(STR)?;
            self.consume("pitch")?;
            let p = self.consume(STR)?;
            json!({"type": "TtsSetting", "voice": Self::string_node(&v), "speed": Self::string_node(&sp), "pitch": Self::string_node(&p), "loc": loc(s, self.prev_end())})
        }
        else if self.leads("in") { self.list_add(s)? }
        else if self.leads("remove") { self.list_remove(s)? }
        else if self.leads("ask") { self.pos += 1; let q = self.expr()?.v; json!({"type": "Ask", "question": q, "loc": loc(s, self.prev_end())}) }
        else if self.leads("save") {
            self.pos += 1;
            if self.same_line() && self.is_ident_like(1) {
                if self.ty(1) == "async" {
                    self.pos += 1;
                    json!({"type": "StoreSave", "async": true, "loc": loc(s, self.prev_end())})
                } else {
                    let id = self.identifier()?;
                    json!({"type": "TableSave", "table": id, "loc": loc(s, self.prev_end())})
                }
            } else {
                json!({"type": "StoreSave", "async": false, "loc": loc(s, self.prev_end())})
            }
        }
        else if self.leads_decl("var") { self.var_decl()? }
        else if self.leads_decl("list") { self.list_decl()? }
        else { self.assign_or_call(s)? };
        Ok(v)
    }

    fn if_statement(&mut self, s: usize) -> R<Value> {
        self.consume("if")?;
        let test = self.expr()?.v;
        self.block_open()?;
        let cons = self.block()?;
        let mut alt = Value::Null;
        if self.ty(1) == "else" {
            self.pos += 1;
            self.block_open()?;
            alt = Value::Array(self.block()?);
        }
        self.consume("end")?;
        Ok(json!({"type": "If", "test": test, "consequent": cons, "alternate": alt, "loc": loc(s, self.prev_end())}))
    }

    fn loop_statement(&mut self, ty: &str, kw: &str, key: &str, s: usize) -> R<Value> {
        self.consume(kw)?;
        let test = self.expr()?.v;
        self.block_open()?;
        let body = self.block()?;
        self.consume("end")?;
        let mut m = Map::new();
        m.insert("type".into(), json!(ty));
        m.insert(key.into(), test);
        m.insert("body".into(), Value::Array(body));
        m.insert("loc".into(), loc(s, self.prev_end()));
        Ok(Value::Object(m))
    }

    fn stop_statement(&mut self, s: usize) -> R<Value> {
        self.consume("stop")?;
        let ty: Value = match self.ty(1) {
            "sound" => {
                self.pos += 1;
                let t = match self.ty(1) { "this" | "all" => self.la(1).image.clone(), _ => return Err(self.err_no_alt()) };
                self.pos += 1;
                json!({"type": "StopSound", "target": t, "loc": loc(s, self.prev_end())})
            }
            w @ ("draw" | "fill" | "bgm" | "timer" | "project") => {
                self.pos += 1;
                let t = match w { "draw" => "StopDraw", "fill" => "StopFill", "bgm" => "StopBgm", "timer" => "StopTimer", _ => "StopProject" };
                json!({"type": t, "loc": loc(s, self.prev_end())})
            }
            "other" | "me" | "object" | "them" | "all" => {
                let t = self.la(1).image.clone();
                self.pos += 1;
                json!({"type": "Stop", "target": t, "loc": loc(s, self.prev_end())})
            }
            _ => json!({"type": "Stop", "target": "this", "loc": loc(s, self.prev_end())}),
        };
        Ok(ty)
    }

    fn point_args(&mut self) -> R<(Value, Value, Value)> {
        let x = self.unary_expr()?.v;
        if !self.same_line() {
            return Err(self.err_no_alt());
        }
        let y = self.unary_expr()?.v;
        let d = if self.same_line() && self.ty(1) == "in" { self.pos += 1; self.expr()?.v } else { Value::Null };
        Ok((x, y, d))
    }

    fn go_statement(&mut self, s: usize) -> R<Value> {
        self.consume("go")?;
        let save = self.pos;
        match self.point_args() {
            Ok((x, y, d)) => Ok(json!({"type": "Go", "x": x, "y": y, "target": null, "duration": d, "loc": loc(s, self.prev_end())})),
            Err(_) => {
                self.pos = save;
                let t = self.expr()?.v;
                let d = if self.same_line() && self.ty(1) == "in" { self.pos += 1; self.expr()?.v } else { Value::Null };
                Ok(json!({"type": "Go", "x": null, "y": null, "target": t, "duration": d, "loc": loc(s, self.prev_end())}))
            }
        }
    }

    fn show_hide(&mut self, s: usize) -> R<Value> {
        let ty = if self.ty(1) == "show" { "Show" } else { "Hide" };
        self.pos += 1;
        let target = if self.same_line() && self.is_ident_like(1) { self.identifier()? } else { Value::Null };
        let (mut seconds, mut chart) = (Value::Null, Value::Null);
        if self.same_line() {
            match self.ty(1) {
                "for" => { self.pos += 1; seconds = self.expr()?.v; }
                "chart" => { self.pos += 1; chart = self.expr()?.v; }
                _ => {}
            }
        }
        Ok(json!({"type": ty, "target": target, "seconds": seconds, "chart": chart, "loc": loc(s, self.prev_end())}))
    }

    fn play_statement(&mut self, s: usize) -> R<Value> {
        self.consume("play")?;
        match self.ty(1) {
            "sound" => {
                self.pos += 1;
                let name = self.expr()?.v;
                let (mut dur, mut from, mut to) = (Value::Null, Value::Null, Value::Null);
                if self.ty(1) == "for" {
                    self.pos += 1;
                    dur = self.expr()?.v;
                } else if self.ty(1) == "from" {
                    self.pos += 1;
                    from = self.expr()?.v;
                    self.consume("to")?;
                    to = self.expr()?.v;
                }
                let mut wait = false;
                if self.ty(1) == "and" { self.pos += 1; self.consume("wait")?; wait = true; }
                Ok(json!({"type": "PlaySound", "name": name, "duration": dur, "from": from, "to": to, "wait": wait, "loc": loc(s, self.prev_end())}))
            }
            "bgm" => {
                self.pos += 1;
                let name = self.expr()?.v;
                Ok(json!({"type": "PlayBgm", "name": name, "loc": loc(s, self.prev_end())}))
            }
            _ => Err(self.err_no_alt()),
        }
    }

    fn table_line(&mut self) -> R<&'static str> {
        match self.ty(1) {
            "row" => { self.pos += 1; Ok("row") }
            "column" => { self.pos += 1; Ok("column") }
            _ => Err(self.err_no_alt()),
        }
    }

    fn list_add(&mut self, s: usize) -> R<Value> {
        self.consume("in")?;
        let list = self.identifier()?;
        let (t2, t) = (self.ty(2), self.ty(1));
        if self.leads("add") && t2 != "row" && t2 != "column" {
            self.pos += 1;
            let v = self.expr()?.v;
            return Ok(json!({"type": "ListAdd", "list": list, "value": v, "loc": loc(s, self.prev_end())}));
        }
        if t == "insert" && t2 != "row" && t2 != "column" {
            self.pos += 1;
            let v = self.expr()?.v;
            self.consume("at")?;
            let i = self.expr()?.v;
            return Ok(json!({"type": "ListInsert", "list": list, "value": v, "index": i, "loc": loc(s, self.prev_end())}));
        }
        if t == "add" || t == "insert" {
            self.pos += 1;
            let line = self.table_line()?;
            let index = if self.ty(1) == "at" { self.pos += 1; self.expr()?.v } else { Value::Null };
            return Ok(json!({
                "type": if t == "add" { "TableAddLine" } else { "TableInsertLine" },
                "table": list, "line": line, "index": index, "loc": loc(s, self.prev_end())
            }));
        }
        Err(self.err_no_alt())
    }

    fn list_remove(&mut self, s: usize) -> R<Value> {
        self.consume("remove")?;
        let list = self.identifier()?;
        if self.ty(1) == "[" {
            self.pos += 1;
            let i = self.expr()?.v;
            self.consume("]")?;
            return Ok(json!({"type": "ListRemove", "list": list, "index": i, "loc": loc(s, self.prev_end())}));
        }
        let line = self.table_line()?;
        let i = self.expr()?.v;
        Ok(json!({"type": "TableRemoveLine", "table": list, "line": line, "index": i, "loc": loc(s, self.prev_end())}))
    }

    fn assign_or_call(&mut self, s: usize) -> R<Value> {
        if self.is_ident_like(1) && self.ty(2) == "(" {
            let call = self.call_expr()?.v;
            return Ok(json!({"type": "ExpressionStatement", "expression": call, "loc": loc(s, self.prev_end())}));
        }
        if !(self.is_ident_like(1) && (ASSIGN_OPS.contains(&self.ty(2)) || self.ty(2) == "[")) {
            return Err(self.err_no_alt());
        }
        let ls = self.cur_start();
        let name = self.identifier()?;
        let target = if self.ty(1) == "[" {
            self.pos += 1;
            let index = self.expr()?.v;
            let column = if self.ty(1) == "," { self.pos += 1; self.expr()?.v } else { Value::Null };
            self.consume("]")?;
            json!({"type": "Index", "target": name, "index": index, "column": column, "loc": loc(ls, self.prev_end())})
        } else {
            name
        };
        let op = self.la(1).image.clone();
        if !ASSIGN_OPS.contains(&self.ty(1)) {
            return Err(self.err_no_alt());
        }
        self.pos += 1;
        let value = self.expr()?.v;
        Ok(json!({"type": "Assign", "operator": op, "target": target, "value": value, "loc": loc(s, self.prev_end())}))
    }

    // ------------------------------------------------------------ expressions
    pub fn expr(&mut self) -> R<E> {
        self.or_expr()
    }

    fn fold(&self, operands: Vec<E>, ops: Vec<String>) -> E {
        let s = operands[0].s;
        let e = operands.last().unwrap().e;
        let mut it = operands.into_iter();
        let mut node = it.next().unwrap().v;
        for (op, right) in ops.into_iter().zip(it) {
            node = json!({
                "type": "Binary", "operator": op, "left": node, "right": right.v,
                "loc": {"start": s, "end": right.e}
            });
        }
        E { v: node, s, e }
    }

    fn or_expr(&mut self) -> R<E> {
        let mut operands = vec![self.and_expr()?];
        let mut ops = Vec::new();
        while self.ty(1) == "or" {
            self.pos += 1;
            ops.push("or".to_string());
            operands.push(self.and_expr()?);
        }
        Ok(self.fold(operands, ops))
    }

    fn and_expr(&mut self) -> R<E> {
        let mut operands = vec![self.not_expr()?];
        let mut ops = Vec::new();
        while self.ty(1) == "and" && self.starts_expr_at(2) {
            self.pos += 1;
            ops.push("and".to_string());
            operands.push(self.not_expr()?);
        }
        Ok(self.fold(operands, ops))
    }

    fn prefix(&mut self, ops: Vec<usize>, operand: E, name: &str, s: usize) -> E {
        let end = operand.e;
        let mut node = operand.v;
        for st in ops.into_iter().rev() {
            node = json!({"type": "Unary", "operator": name, "argument": node, "loc": {"start": st, "end": end}});
        }
        E { v: node, s, e: end }
    }

    fn not_expr(&mut self) -> R<E> {
        let s = self.cur_start();
        let mut ops = Vec::new();
        while self.ty(1) == "not" {
            ops.push(self.la(1).start);
            self.pos += 1;
        }
        let operand = self.compare_expr()?;
        Ok(self.prefix(ops, operand, "not", s))
    }

    fn compare_expr(&mut self) -> R<E> {
        let mut operands = vec![self.add_expr()?];
        let mut ops = Vec::new();
        while matches!(self.ty(1), "==" | "!=" | "<=" | ">=" | "<" | ">") {
            ops.push(self.la(1).image.clone());
            self.pos += 1;
            operands.push(self.add_expr()?);
        }
        Ok(self.fold(operands, ops))
    }

    fn add_expr(&mut self) -> R<E> {
        let mut operands = vec![self.mul_expr()?];
        let mut ops = Vec::new();
        while matches!(self.ty(1), "+" | "-") {
            ops.push(self.la(1).image.clone());
            self.pos += 1;
            operands.push(self.mul_expr()?);
        }
        Ok(self.fold(operands, ops))
    }

    fn mul_expr(&mut self) -> R<E> {
        let mut operands = vec![self.pow_expr()?];
        let mut ops = Vec::new();
        while matches!(self.ty(1), "//" | "*" | "/" | "%") {
            ops.push(self.la(1).image.clone());
            self.pos += 1;
            operands.push(self.pow_expr()?);
        }
        Ok(self.fold(operands, ops))
    }

    fn pow_expr(&mut self) -> R<E> {
        let base = self.unary_expr()?;
        if self.ty(1) == "**" {
            self.pos += 1;
            let exp = self.pow_expr()?;
            let (s, e) = (base.s, exp.e);
            let v = json!({"type": "Binary", "operator": "**", "left": base.v, "right": exp.v, "loc": {"start": s, "end": e}});
            return Ok(E { v, s, e });
        }
        Ok(base)
    }

    pub fn unary_expr(&mut self) -> R<E> {
        let s = self.cur_start();
        let mut ops = Vec::new();
        while self.ty(1) == "-" {
            ops.push(self.la(1).start);
            self.pos += 1;
        }
        let operand = self.primary_expr()?;
        Ok(self.prefix(ops, operand, "-", s))
    }

    fn primary_expr(&mut self) -> R<E> {
        let s = self.cur_start();
        let t = self.ty(1);
        let wrap = |p: &Parser, v: Value| E { v, s, e: p.prev_end() };
        if t == "(" {
            self.pos += 1;
            let inner = self.expr()?;
            self.consume(")")?;
            return Ok(wrap(self, inner.v));
        }
        if is_ident_like_ty(t) && self.ty(2) == "(" {
            return self.call_expr();
        }
        if is_ident_like_ty(t) && self.ty(2) == "[" {
            let target = self.identifier()?;
            self.consume("[")?;
            let index = self.expr()?.v;
            let column = if self.ty(1) == "," { self.pos += 1; self.expr()?.v } else { Value::Null };
            self.consume("]")?;
            let v = json!({"type": "Index", "target": target, "index": index, "column": column, "loc": loc(s, self.prev_end())});
            return Ok(wrap(self, v));
        }
        match t {
            NUM => {
                let tk = self.consume(NUM)?;
                let v = json!({"type": "Number", "value": num_value(&tk.image), "loc": loc(tk.start, tk.end)});
                Ok(wrap(self, v))
            }
            STR => {
                let tk = self.consume(STR)?;
                Ok(wrap(self, Self::string_node(&tk)))
            }
            "true" | "false" => {
                let v = self.boolean_literal()?;
                Ok(wrap(self, v))
            }
            COL => {
                let tk = self.consume(COL)?;
                let value = normalize_color(&tk.image).unwrap_or_else(|| tk.image.to_lowercase());
                let v = json!({"type": "Color", "value": value, "loc": loc(tk.start, tk.end)});
                Ok(wrap(self, v))
            }
            "transparent" => {
                self.pos += 1;
                let v = json!({"type": "Transparent", "loc": loc(s, self.prev_end())});
                Ok(wrap(self, v))
            }
            _ if is_ident_like_ty(t) => {
                let v = self.identifier()?;
                Ok(wrap(self, v))
            }
            _ => Err(self.err_no_alt()),
        }
    }

    fn call_expr(&mut self) -> R<E> {
        let s = self.cur_start();
        let callee = self.identifier()?;
        self.consume("(")?;
        let mut args = Vec::new();
        if self.starts_expr_at(1) {
            loop {
                args.push(self.expr()?.v);
                if self.ty(1) == "," { self.pos += 1; } else { break; }
            }
        }
        self.consume(")")?;
        let e = self.prev_end();
        Ok(E { v: json!({"type": "Call", "callee": callee["name"], "arguments": args, "loc": loc(s, e)}), s, e })
    }

    fn list_literal(&mut self) -> R<Value> {
        let s = self.cur_start();
        self.consume("[")?;
        let mut els = Vec::new();
        if self.starts_expr_at(1) {
            loop {
                els.push(self.expr()?.v);
                if self.ty(1) == "," { self.pos += 1; } else { break; }
            }
        }
        self.consume("]")?;
        Ok(json!({"type": "ListLiteral", "elements": els, "loc": loc(s, self.prev_end())}))
    }

    fn identifier(&mut self) -> R<Value> {
        let t = self.consume_ident()?;
        Ok(json!({"type": "Identifier", "name": t.image, "loc": loc(t.start, t.end)}))
    }

    fn boolean_literal(&mut self) -> R<Value> {
        match self.ty(1) {
            "true" | "false" => {
                let t = self.la(1).clone();
                self.pos += 1;
                Ok(json!({"type": "Boolean", "value": t.image == "true", "loc": loc(t.start, t.end)}))
            }
            _ => Err(self.err_no_alt()),
        }
    }
}

pub fn code_frame(source: &str, line: usize, column: usize, message: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let first = line.saturating_sub(2).max(1);
    let last = lines.len().min(line + 2);
    let gutter = last.to_string().len();
    let mut out = Vec::new();
    for at in first..=last {
        out.push(format!("{} {:>w$} | {}", if at == line { '>' } else { ' ' }, at, lines.get(at - 1).unwrap_or(&""), w = gutter));
        if at != line {
            continue;
        }
        let caret = format!("{}^", " ".repeat(column.saturating_sub(1)));
        let msg = if message.is_empty() { String::new() } else { format!(" {}", message) };
        out.push(format!("  {} | {}{}", " ".repeat(gutter), caret, msg));
    }
    out.join("\n")
}

/// Line and column (UTF-16 based, 1-indexed) of a UTF-16 offset.
pub fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column, mut i) = (1usize, 1usize, 0usize);
    for c in source.chars() {
        if i >= offset {
            break;
        }
        let w = c.len_utf16();
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += w;
        }
        i += w;
    }
    (line, column)
}

fn make_failure(source: &str, offset: usize, message: String) -> Failure {
    let (line, column) = line_and_column(source, offset);
    let detail = code_frame(source, line, column, &message);
    Failure { offset, message, detail }
}

/// Parses a whole program. Err holds the single reported failure.
#[derive(Clone, Copy, PartialEq)]
pub enum StartRule {
    Program,
    SceneFragment,
    ObjectFragment,
}

pub fn parse_source(source: &str) -> Result<Value, Failure> {
    parse_with(source, StartRule::Program)
}

/// Parses with the given start rule; fragments yield a JSON array of members.
pub fn parse_with(source: &str, rule: StartRule) -> Result<Value, Failure> {
    let chars: Vec<char> = source.chars().collect();
    let src_len: usize = chars.iter().map(|c| c.len_utf16()).sum();
    let toks = match tokenize(&chars) {
        Ok(t) => t,
        Err((off, c)) => {
            let ch = c.map(|c| c.to_string()).unwrap_or_default();
            let quoted = serde_json::to_string(&ch).unwrap();
            return Err(make_failure(source, off, format!("읽을 수 없는 글자입니다: {}", quoted)));
        }
    };
    let mut p = Parser::new(toks, src_len);
    let parsed = match rule {
        StartRule::Program => p.program(),
        StartRule::SceneFragment => p.fragment(true),
        StartRule::ObjectFragment => p.fragment(false),
    };
    match parsed {
        Err(e) => Err(make_failure(source, e.offset, e.message)),
        Ok(ast) => {
            if p.ty(1) != EOF {
                let t = p.la(1).clone();
                let message = format!("입력의 끝이 와야 하는데 '{}' 이(가) 있습니다.", t.image);
                return Err(make_failure(source, p.off_of(&t), message));
            }
            Ok(ast)
        }
    }
}
