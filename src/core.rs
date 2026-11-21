//! Shared tables: builtin names, expansion blocks, key codes, colors, typo suggestions.
use std::collections::HashSet;

pub const STATE_VALUES: &[&str] = &[
    "mouse_down", "clicked", "boost_mode", "touchable", "device", "user_id", "nickname", "timer",
    "answer", "block_count", "costume_number", "can_save",
];

pub const OPTION_KEYWORDS: &[&str] = &["red", "green", "blue"];

pub const TEXT_ONLY_PROPERTIES: &[&str] = &[
    "text_content", "font", "font_color", "bg_color", "font_size", "text_bold", "text_italic",
    "text_underline", "text_strikethrough", "text_align", "line_break",
];

pub const OBJECT_PROPERTIES: &[&str] = &[
    "x", "y", "size", "scale_x", "scale_y", "angle", "way", "costume", "rotation", "effect_color",
    "effect_brightness", "effect_alpha", "draw_color", "draw_width", "draw_alpha", "fill_color",
    "sound_volume", "sound_speed",
];

const BASE_BUILTIN_FUNCTIONS: &[&str] = &[
    "key_down", "touching", "type", "sin", "cos", "tan", "asin", "acos", "atan", "log2", "ln",
    "log10", "floor", "ceil", "round", "abs", "factorial", "random", "root", "x", "y", "angle",
    "way", "size", "costume", "costume_number", "distance", "block_count", "text_content",
    "length", "slice", "count", "join", "index_of", "replace", "reverse", "uppercase",
    "lowercase", "contains", "sound_duration", "now", "to_hex", "from_hex", "random_color",
    "row_count", "column_count", "last_row", "correlation", "lookup", "sum", "average",
    "maximum", "minimum", "stdev", "median",
];

/// Expansion block: (name, module, kind, slot kinds `f`=field `v`=value, ai-group).
pub struct Expansion {
    pub name: &'static str,
    pub module: &'static str,
    pub boolean: bool,
    pub slots: &'static str,
    pub ai: bool,
}

macro_rules! ex {
    ($n:expr, $m:expr, $k:expr, $s:expr) => {
        Expansion { name: $n, module: $m, boolean: $k, slots: $s, ai: false }
    };
    ($n:expr, $m:expr, $k:expr, $s:expr, ai) => {
        Expansion { name: $n, module: $m, boolean: $k, slots: $s, ai: true }
    };
}

pub const EXPANSION_BLOCKS: &[Expansion] = &[
    ex!("count_disaster_behavior", "behaviorConductDisaster", false, "ff"),
    ex!("get_disaster_behavior", "behaviorConductDisaster", false, "ffv"),
    ex!("count_lifeSafety_behavior", "behaviorConductLifeSafety", false, "ff"),
    ex!("get_lifeSafety_behavior", "behaviorConductLifeSafety", false, "ffv"),
    ex!("count_disaster_alert", "disasterAlert", false, "f"),
    ex!("get_disaster_alert", "disasterAlert", false, "fvf"),
    ex!("check_disaster_alert", "disasterAlert", true, "f"),
    ex!("count_disaster_guideline", "emergencyActionGuidelines", false, "ff"),
    ex!("get_disaster_guideline", "emergencyActionGuidelines", false, "ffv"),
    ex!("count_social_disaster_guideline", "emergencyActionGuidelines", false, "ff"),
    ex!("get_social_disaster_guideline", "emergencyActionGuidelines", false, "ffv"),
    ex!("count_safety_accident_guideline", "emergencyActionGuidelines", false, "ff"),
    ex!("get_safety_accident_guideline", "emergencyActionGuidelines", false, "ffv"),
    ex!("count_festival", "festival", false, "ff"),
    ex!("get_festival_info", "festival", false, "ffvf"),
    ex!("check_city_weather", "weather", true, "ffff"),
    ex!("check_city_finedust", "weather", true, "fff"),
    ex!("get_city_weather_data", "weather", false, "ffff"),
    ex!("get_current_city_weather_data", "weather", false, "fff"),
    ex!("get_today_city_temperature", "weather", false, "fff"),
    ex!("check_weather", "weather", true, "fff"),
    ex!("check_finedust", "weather", true, "ff"),
    ex!("get_weather_data", "weather", false, "fff"),
    ex!("get_current_weather_data", "weather", false, "ff"),
    ex!("get_today_temperature", "weather", false, "ff"),
    ex!("get_cur_weather", "weather", false, "v"),
    ex!("get_cur_wind", "weather", false, "v"),
    ex!("get_cur_weather_data", "weather", false, "vf"),
    ex!("check_cur_weather", "weather", true, "vf"),
    ex!("check_cur_finddust", "weather", true, "vf"),
    ex!("get_day_weather", "weather", false, "fv"),
    ex!("get_day_weather_data", "weather", false, "fvf"),
    ex!("check_day_weather", "weather", true, "fvf"),
    ex!("get_time_weather", "weather", false, "vf"),
    ex!("get_time_weather_data", "weather", false, "vff"),
    ex!("check_time_weather", "weather", true, "vff"),
    ex!("get_korea_area_code", "weather", false, "ff"),
    ex!("get_translated_string", "translate", false, "fvf", ai),
    ex!("check_language", "translate", false, "v", ai),
];

pub fn expansion_block(name: &str) -> Option<&'static Expansion> {
    EXPANSION_BLOCKS.iter().find(|e| e.name == name)
}

pub fn set_of(list: &[&'static str]) -> HashSet<&'static str> {
    list.iter().copied().collect()
}

pub fn is_builtin_function(name: &str) -> bool {
    BASE_BUILTIN_FUNCTIONS.contains(&name) || expansion_block(name).is_some()
}

pub fn builtin_function_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = BASE_BUILTIN_FUNCTIONS.to_vec();
    v.extend(EXPANSION_BLOCKS.iter().map(|e| e.name));
    v
}

pub fn is_builtin_name(name: &str) -> bool {
    STATE_VALUES.contains(&name)
        || is_builtin_function(name)
        || OPTION_KEYWORDS.contains(&name)
        || TEXT_ONLY_PROPERTIES.contains(&name)
        || OBJECT_PROPERTIES.contains(&name)
}

/// Automatic parameter name for a zero-based index (`a`, `b`, ... skipping builtin letters).
pub fn auto_param_name(index: usize) -> String {
    let letters: Vec<char> = ('a'..='z').filter(|c| !is_builtin_name(&c.to_string())).collect();
    if index < letters.len() {
        letters[index].to_string()
    } else {
        format!("a{}", index - letters.len() + 1)
    }
}

pub fn key_code(name: &str) -> Option<String> {
    let key = name.trim().to_lowercase();
    let code: Option<u32> = match key.as_str() {
        "backspace" => Some(8), "tab" => Some(9), "enter" => Some(13), "shift" => Some(16),
        "ctrl" => Some(17), "alt" => Some(18), "esc" | "escape" => Some(27), "space" => Some(32),
        "left" => Some(37), "up" => Some(38), "right" => Some(39), "down" => Some(40),
        "capslock" => Some(20), "pageup" => Some(33), "pagedown" => Some(34), "end" => Some(35),
        "home" => Some(36), "insert" => Some(45), "delete" => Some(46),
        ";" => Some(186), "=" => Some(187), "," => Some(188), "-" => Some(189), "." => Some(190),
        "/" => Some(191), "~" => Some(192), "[" => Some(219), "backslash" | "\\" => Some(220),
        "]" => Some(221), "'" => Some(222),
        _ => None,
    };
    if let Some(c) = code {
        return Some(c.to_string());
    }
    let b = key.as_bytes();
    if let Some(n) = key.strip_prefix('f') {
        if let Ok(i) = n.parse::<u32>() {
            if (1..=12).contains(&i) && n == i.to_string() {
                return Some((111 + i).to_string());
            }
        }
    }
    if let Some(n) = key.strip_prefix("numpad") {
        if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() {
            return Some((96 + (n.as_bytes()[0] - b'0') as u32).to_string());
        }
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Some((48 + (b[0] - b'0') as u32).to_string());
    }
    if b.len() == 1 && b[0].is_ascii_lowercase() {
        return Some((65 + (b[0] - b'a') as u32).to_string());
    }
    if !b.is_empty() && b.len() <= 3 && b.iter().all(|c| c.is_ascii_digit()) {
        return Some(key);
    }
    None
}

pub fn named_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "검정" | "검정색" | "검은색" | "흑색" | "black" => "#000000",
        "하양" | "하얀색" | "흰색" | "백색" | "white" => "#ffffff",
        "빨강" | "빨간색" | "적색" | "red" => "#ff0000",
        "주황" | "주황색" => "#ff7f00",
        "노랑" | "노란색" | "황색" | "yellow" => "#ffff00",
        "연두" | "연두색" => "#7fff00",
        "초록" | "초록색" | "녹색" | "lime" => "#00ff00",
        "청록" | "청록색" | "cyan" | "aqua" => "#00ffff",
        "하늘" | "하늘색" => "#00bfff",
        "파랑" | "파란색" | "청색" | "blue" => "#0000ff",
        "남색" | "navy" => "#000080",
        "보라" | "보라색" => "#7f00ff",
        "자주색" => "#8b008b",
        "분홍" | "분홍색" | "pink" => "#ffc0cb",
        "자홍" | "자홍색" | "magenta" | "fuchsia" => "#ff00ff",
        "갈색" | "brown" => "#a52a2a",
        "밤색" => "#8b4513",
        "회색" | "gray" | "grey" => "#808080",
        "은색" | "silver" => "#c0c0c0",
        "금색" | "gold" => "#ffd700",
        "green" => "#008000", "maroon" => "#800000", "olive" => "#808000", "purple" => "#800080",
        "teal" => "#008080", "orange" => "#ffa500", "beige" => "#f5f5dc", "ivory" => "#fffff0",
        "khaki" => "#f0e68c", "indigo" => "#4b0082", "violet" => "#ee82ee", "crimson" => "#dc143c",
        "salmon" => "#fa8072", "coral" => "#ff7f50", "tomato" => "#ff6347",
        "turquoise" => "#40e0d0", "skyblue" => "#87ceeb", "lavender" => "#e6e6fa",
        "tan" => "#d2b48c", "plum" => "#dda0dd", "orchid" => "#da70d6", "wheat" => "#f5deb3",
        "snow" => "#fffafa", "mint" => "#98ff98", "chocolate" => "#d2691e",
        _ => return None,
    })
}

fn is_color_body(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit() || c == '_'
}

fn hex_to_rgb(hex: &str) -> String {
    let lower = hex.to_lowercase();
    let n = lower.chars().count();
    if n == 3 || n == 4 {
        let mut s = String::from("#");
        for c in lower.chars() {
            s.push(c);
            s.push(c);
        }
        return s;
    }
    let width = if n > 6 { 8 } else { 6 };
    let mut s: String = lower.chars().collect();
    while s.chars().count() < width {
        s.push('0');
    }
    format!("#{}", s.chars().take(width).collect::<String>())
}

/// `#RRGGBB` (or `#RRGGBBAA`) for a colour literal, `raw` with or without `#`.
pub fn normalize_color(raw: &str) -> Option<String> {
    let body = raw.strip_prefix('#').unwrap_or(raw);
    if body.is_empty() || !body.chars().all(is_color_body) {
        return None;
    }
    if body.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(hex_to_rgb(body));
    }
    named_color(&body.to_lowercase()).map(|s| s.to_string())
}

/// Length in chars of the colour literal at `at` (the `#`), or 0 when it is a comment.
pub fn color_literal_length(chars: &[char], at: usize) -> usize {
    if chars.get(at) != Some(&'#') {
        return 0;
    }
    let mut end = at + 1;
    while end < chars.len() && is_color_body(chars[end]) {
        end += 1;
    }
    let body: String = chars[at + 1..end].iter().collect();
    if !body.is_empty() && normalize_color(&body).is_some() {
        end - at
    } else {
        0
    }
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (rows, cols) = (a.len() + 1, b.len() + 1);
    let mut g = vec![vec![0usize; cols]; rows];
    for (i, row) in g.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..cols {
        g[0][j] = j;
    }
    for i in 1..rows {
        for j in 1..cols {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            g[i][j] = (g[i - 1][j] + 1).min(g[i][j - 1] + 1).min(g[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                g[i][j] = g[i][j].min(g[i - 2][j - 2] + 1);
            }
        }
    }
    g[a.len()][b.len()]
}

pub fn nearest_name<'a, I: IntoIterator<Item = &'a str>>(name: &str, candidates: I) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let target = name.to_lowercase();
    let n = target.chars().count();
    let limit = if n <= 3 { 1 } else if n <= 8 { 2 } else { 3 };
    let mut best: Option<String> = None;
    let mut best_score = usize::MAX;
    for c in candidates {
        if c == name {
            continue;
        }
        let score = edit_distance(&target, &c.to_lowercase());
        if score < best_score {
            best_score = score;
            best = Some(c.to_string());
        }
    }
    if best_score <= limit { best } else { None }
}

pub fn did_you_mean<'a, I: IntoIterator<Item = &'a str>>(name: &str, candidates: I) -> String {
    match nearest_name(name, candidates) {
        Some(f) => format!(" 혹시 '{}' 인가요?", f),
        None => String::new(),
    }
}

pub fn or_hint<'a, I: IntoIterator<Item = &'a str>>(name: &str, candidates: I, hint: &str) -> String {
    match nearest_name(name, candidates) {
        Some(f) => format!(" 혹시 '{}' 인가요?", f),
        None => format!(" {}", hint),
    }
}
