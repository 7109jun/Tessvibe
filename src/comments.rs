//! Maps `#` comments to the statements they annotate.
use crate::core::color_literal_length;
use serde_json::Value;
use std::collections::HashMap;

const ATTACHABLE: &[&str] = &[
    "Event", "If", "Repeat", "While", "Until", "Forever", "Wait", "Break", "Continue", "Skip", "Restart", "Return",
    "Stop", "StopSound", "StopBgm", "StopDraw", "StopFill", "StopTimer", "StartDraw", "StartFill", "StartTimer",
    "ResetSize", "ResetTimer", "Clear", "Send", "Clone", "DeleteClone", "DeleteClones", "Jump", "Forward", "Bounce",
    "Move", "Go", "Turn", "Steer", "Look", "Show", "Hide", "CostumeStep", "Say", "Think", "Flip", "Order", "TextWrite",
    "Stamp", "PlaySound", "PlayBgm", "ListAdd", "ListInsert", "ListRemove", "Ask", "VarDecl", "ListDecl", "Assign",
    "ExpressionStatement",
];

struct Scanned {
    start: usize,
    end: usize,
    text: String,
}

fn units_to_string(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// Finds `#` comments outside strings and colour literals (UTF-16 offsets).
fn scan_comments(units: &[u16]) -> Vec<Scanned> {
    let chars: Vec<char> = units.iter().map(|u| char::from_u32(*u as u32).unwrap_or('\u{fffd}')).collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < units.len() {
        let ch = units[i];
        if ch == b'"' as u16 {
            i += 1;
            while i < units.len() && units[i] != b'"' as u16 {
                if units[i] == b'\\' as u16 {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if ch != b'#' as u16 {
            i += 1;
            continue;
        }
        let color = color_literal_length(&chars, i);
        if color > 0 {
            i += color;
            continue;
        }
        let mut end = i;
        while end < units.len() && units[end] != b'\n' as u16 && units[end] != b'\r' as u16 {
            end += 1;
        }
        let text = units_to_string(&units[i + 1..end]).trim().to_string();
        out.push(Scanned { start: i, end, text });
        i = end + 1;
    }
    out
}

struct Anchor {
    start: usize,
    key: String,
}

fn collect_nodes(node: &Value, out: &mut HashMap<String, Vec<Anchor>>) {
    match node {
        Value::Array(a) => a.iter().for_each(|c| collect_nodes(c, out)),
        Value::Object(m) => {
            let file = m.get("loc").and_then(|l| l.get("file")).and_then(|f| f.as_str()).filter(|f| !f.is_empty());
            if let (Some(t), Some(file)) = (m.get("type").and_then(|t| t.as_str()), file) {
                if ATTACHABLE.contains(&t) {
                    let start = m.get("loc").and_then(|l| l.get("start")).and_then(|s| s.as_u64()).unwrap_or(0) as usize;
                    out.entry(file.to_string()).or_default().push(Anchor { start, key: crate::ctx::comment_key(node) });
                }
            }
            for (k, v) in m {
                if k != "loc" {
                    collect_nodes(v, out);
                }
            }
        }
        _ => {}
    }
}

pub fn build_comment_map(ast: &Value, sources: &HashMap<String, String>) -> HashMap<String, String> {
    let mut by_file: HashMap<String, Vec<Anchor>> = HashMap::new();
    collect_nodes(ast, &mut by_file);
    let mut map = HashMap::new();
    for (file, nodes) in by_file {
        let Some(source) = sources.get(&file) else { continue };
        if source.is_empty() {
            continue;
        }
        attach_in_file(source, nodes, &mut map);
    }
    map
}

fn append(map: &mut HashMap<String, String>, key: &str, text: &str) {
    match map.get_mut(key) {
        Some(existing) => {
            existing.push('\n');
            existing.push_str(text);
        }
        None => {
            map.insert(key.to_string(), text.to_string());
        }
    }
}

/// Index of the first anchor whose start is greater than `offset` (`offset` may be -1).
fn first_after(list: &[Anchor], offset: i64) -> usize {
    let (mut low, mut high) = (0usize, list.len());
    while low < high {
        let mid = (low + high) >> 1;
        if list[mid].start as i64 > offset {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    low
}

fn only_space_between(units: &[u16], from: usize, to: usize) -> bool {
    if to < from {
        return false;
    }
    let to = to.min(units.len());
    let from = from.min(to);
    units_to_string(&units[from..to]).chars().all(|c| c.is_whitespace() || c == '\u{feff}')
}

fn attach_in_file(source: &str, mut nodes: Vec<Anchor>, map: &mut HashMap<String, String>) {
    let units: Vec<u16> = source.encode_utf16().collect();
    let comments = scan_comments(&units);
    if comments.is_empty() {
        return;
    }
    let mut starts = vec![0usize];
    for (i, u) in units.iter().enumerate() {
        if *u == b'\n' as u16 {
            starts.push(i + 1);
        }
    }
    let line_of = |offset: usize| -> usize {
        let (mut low, mut high) = (0usize, starts.len() - 1);
        while low < high {
            let mid = (low + high + 1) / 2;
            if starts[mid] <= offset {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        low + 1
    };
    nodes.sort_by_key(|n| n.start);
    let mut remaining: Vec<&Scanned> = Vec::new();

    for comment in &comments {
        let idx = first_after(&nodes, comment.start as i64 - 1);
        let owner = if idx > 0 {
            let before = &nodes[idx - 1];
            if line_of(before.start) == line_of(comment.start) { Some(before) } else { None }
        } else {
            None
        };
        match owner {
            Some(o) => append(map, &o.key, &comment.text),
            None => remaining.push(comment),
        }
    }

    let mut i = 0usize;
    while i < remaining.len() {
        let mut group = vec![remaining[i]];
        while i + 1 < remaining.len() && only_space_between(&units, group.last().unwrap().end, remaining[i + 1].start) {
            group.push(remaining[i + 1]);
            i += 1;
        }
        let last = group.last().unwrap();
        let idx = first_after(&nodes, last.end as i64);
        if let Some(owner) = nodes.get(idx) {
            if only_space_between(&units, last.end, owner.start) {
                let text = group.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\n");
                append(map, &owner.key, &text);
            }
        }
        i += 1;
    }
}
