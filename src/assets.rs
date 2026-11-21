//! Costume and sound assets: file lookup, size probing, deterministic names.
use crate::audio::{audio_duration, be16_pub, be32_pub, le16_pub};
use serde_json::{json, Value};

pub const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

pub fn seed_from(text: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for u in text.encode_utf16() {
        hash ^= u as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

pub fn image_type(ext: &str) -> Option<&'static str> {
    match ext {
        ".png" => Some("png"), ".jpg" | ".jpeg" => Some("jpg"), ".gif" => Some("gif"),
        ".svg" => Some("svg"), ".bmp" => Some("bmp"), _ => None,
    }
}

pub fn is_sound_ext(ext: &str) -> bool {
    matches!(ext, ".mp3" | ".wav" | ".ogg" | ".m4a")
}

fn data_ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => ".png", "image/jpeg" | "image/jpg" => ".jpg", "image/gif" => ".gif",
        "image/svg+xml" => ".svg", "image/bmp" => ".bmp",
        "audio/mpeg" | "audio/mp3" => ".mp3", "audio/wav" | "audio/x-wav" | "audio/wave" => ".wav",
        "audio/ogg" => ".ogg", "audio/mp4" | "audio/x-m4a" => ".m4a",
        _ => "",
    }
}

pub fn basename(file: &str) -> &str {
    file.rsplit(|c| c == '/' || c == '\\').next().unwrap_or("")
}

pub fn dirname(file: &str) -> String {
    match file.rfind(|c| c == '/' || c == '\\') {
        None => ".".into(),
        Some(cut) => if cut == 0 { "/".into() } else { file[..cut].to_string() },
    }
}

pub fn extname(file: &str) -> String {
    let name = basename(file);
    match name.rfind('.') {
        Some(d) if d > 0 => name[d..].to_string(),
        _ => String::new(),
    }
}

pub fn asset_ext(file: &str) -> String {
    if let Some(rest) = file.strip_prefix("data:").or_else(|| {
        match file.get(..5) {
            Some(head) if head.eq_ignore_ascii_case("data:") => Some(&file[5..]),
            _ => None,
        }
    }) {
        let mime: String = rest.chars().take_while(|c| *c != ';' && *c != ',').collect();
        if !mime.is_empty() {
            return data_ext(&mime.to_lowercase()).to_string();
        }
    }
    extname(file).to_lowercase()
}

pub fn asset_filename(key: &str) -> String {
    let mut state = seed_from(key);
    if state == 0 {
        state = 1;
    }
    let mut name = String::new();
    for _ in 0..32 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        name.push(ALPHABET[(state as usize) % ALPHABET.len()] as char);
    }
    name
}

pub fn file_url_for(kind: &str, filename: &str, ext: &str) -> String {
    format!("temp/{}/{}/{}/{}{}", &filename[0..2], &filename[2..4], kind, filename, ext)
}

pub fn image_size(b: &[u8]) -> Option<(f64, f64)> {
    if b.len() >= 24 && be32_pub(b, 0) == Some(0x89504e47) {
        return Some((be32_pub(b, 16)? as f64, be32_pub(b, 20)? as f64));
    }
    if b.len() >= 10 && b[0] == 0x47 && b[1] == 0x49 && b[2] == 0x46 {
        return Some((le16_pub(b, 6)? as f64, le16_pub(b, 8)? as f64));
    }
    if b.len() >= 4 && b[0] == 0xff && b[1] == 0xd8 {
        let mut offset = 2usize;
        while offset + 9 < b.len() {
            if b[offset] != 0xff {
                offset += 1;
                continue;
            }
            let marker = b[offset + 1];
            let length = be16_pub(b, offset + 2)? as usize;
            if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
                return Some((be16_pub(b, offset + 7)? as f64, be16_pub(b, offset + 5)? as f64));
            }
            offset += 2 + length;
        }
    }
    let head: String = b.iter().map(|c| *c as char).collect();
    if head.contains("<svg") {
        return svg_size(b);
    }
    None
}

fn svg_size(b: &[u8]) -> Option<(f64, f64)> {
    let text = String::from_utf8_lossy(b).to_string();
    let lower = text.to_lowercase();
    let start = find_svg_tag(&lower)?;
    let end = text[start..].find('>')? + start + 1;
    let tag = &text[start..end];
    let length = |attr: &str| -> Option<f64> {
        let raw = attr_value(tag, attr)?;
        let raw = raw.trim();
        let num = raw.strip_suffix("px").or_else(|| raw.strip_suffix("PX")).unwrap_or(raw);
        if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        let v: f64 = num.parse().ok()?;
        if v.is_finite() && v > 0.0 { Some(v) } else { None }
    };
    if let (Some(w), Some(h)) = (length("width"), length("height")) {
        return Some((w, h));
    }
    if let Some(vb) = attr_value(tag, "viewBox") {
        let parts: Vec<f64> = vb.trim().split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).map(|s| s.parse::<f64>().unwrap_or(f64::NAN)).collect();
        if parts.len() == 4 && parts.iter().all(|p| p.is_finite()) && parts[2] > 0.0 && parts[3] > 0.0 {
            return Some((parts[2], parts[3]));
        }
    }
    None
}

fn find_svg_tag(lower: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = lower[from..].find("<svg") {
        let at = from + i;
        match lower[at + 4..].chars().next() {
            Some(c) if c.is_alphanumeric() || c == '_' => from = at + 4,
            _ => return Some(at),
        }
    }
    None
}

fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let key = attr.to_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&key) {
        let at = from + i;
        let before_ok = at == 0 || !lower[..at].chars().last().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
        let mut rest = tag[at + key.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            rest = rest[1..].trim_start();
            let q = rest.chars().next()?;
            if q == '"' || q == '\'' {
                let inner = &rest[1..];
                let close = inner.find(q)?;
                return Some(inner[..close].to_string());
            }
        }
        from = at + key.len();
    }
    None
}

pub struct AssetDecl<'a> {
    pub id: String,
    pub file: &'a str,
    pub name: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub duration: Option<f64>,
}

pub struct AssetFile {
    pub source: String,
    pub target: String,
}

pub struct Probe<'a> {
    pub asset_dirs: &'a [String],
    pub asset_urls: bool,
}

fn resolve(base: &str, target: &str) -> String {
    let p = std::path::Path::new(target);
    if p.is_absolute() {
        return normalize(p);
    }
    normalize(&std::path::Path::new(base).join(target))
}

pub fn normalize(p: &std::path::Path) -> String {
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    let mut out = std::path::PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::ParentDir => { out.pop(); }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().to_string()
}

pub fn host_resolve(base: &str, target: &str) -> String {
    resolve(base, target)
}

pub fn is_file(p: &str) -> bool {
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

/// Builds an asset JSON entry. Warnings are returned as messages for the caller to report.
pub fn make_asset(kind: &str, d: AssetDecl, probe: &Probe, files: &mut Vec<AssetFile>, warnings: &mut Vec<String>) -> Value {
    let ext = asset_ext(d.file);
    let is_image = kind == "image";
    if is_image && image_type(&ext).is_none() {
        warnings.push(format!("'{}' 은(는) 엔트리가 아는 이미지 형식이 아닙니다.", d.file));
    }
    if !is_image && !is_sound_ext(&ext) {
        warnings.push(format!("'{}' 은(는) 엔트리가 아는 소리 형식이 아닙니다.", d.file));
    }
    let name = d.name.clone().unwrap_or_else(|| basename(d.file).to_string());

    if probe.asset_urls {
        let filename = asset_filename(&format!("{}:{}:0", kind, d.file));
        if is_image {
            let w = d.width.filter(|v| *v != 0.0).unwrap_or(100.0);
            let h = d.height.filter(|v| *v != 0.0).unwrap_or(100.0);
            let mut m = json!({"id": d.id, "name": name, "filename": filename, "fileurl": d.file, "imageType": image_type(&ext).unwrap_or("png"), "dimension": {"width": w, "height": h}});
            if ext == ".svg" {
                let base = &d.file[..d.file.len().saturating_sub(4)];
                m["pngurl"] = json!(format!("{}.png", base));
            }
            return m;
        }
        return json!({"id": d.id, "name": name, "filename": filename, "fileurl": d.file, "ext": ext, "duration": d.duration.unwrap_or(1.0)});
    }

    let mut resolved: Option<String> = None;
    for dir in probe.asset_dirs {
        let candidate = resolve(dir, d.file);
        if is_file(&candidate) {
            resolved = Some(candidate);
            break;
        }
    }
    let declared = if is_image { d.width.filter(|v| *v != 0.0).is_some() && d.height.filter(|v| *v != 0.0).is_some() } else { d.duration.is_some() };
    let mut size = match (d.width.filter(|v| *v != 0.0), d.height.filter(|v| *v != 0.0)) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };
    let mut seconds = d.duration;
    let mut bytes: Option<Vec<u8>> = None;
    if let Some(r) = &resolved {
        bytes = std::fs::read(r).ok();
        if let Some(b) = &bytes {
            if is_image && size.is_none() {
                size = image_size(b);
            }
            if !is_image && seconds.is_none() {
                seconds = audio_duration(b, &ext);
            }
        }
    } else if !declared {
        let hint = if is_image { "size 가로 세로" } else { "for 초" };
        warnings.push(format!("리소스 파일 '{}' 을(를) 찾지 못했습니다. 경로만 기록합니다. ({} 를 적어 두면 이 알림이 사라집니다)", d.file, hint));
    }
    if is_image && size.is_none() {
        size = Some((100.0, 100.0));
    }
    let filename = asset_filename(&format!("{}:{}:{}", kind, d.file, bytes.as_ref().map(|b| b.len()).unwrap_or(0)));
    let fileurl = file_url_for(kind, &filename, &ext);
    let mut m = serde_json::Map::new();
    m.insert("id".into(), json!(d.id));
    m.insert("name".into(), json!(name));
    m.insert("filename".into(), json!(filename));
    m.insert("fileurl".into(), json!(fileurl));
    if is_image {
        let (w, h) = size.unwrap();
        m.insert("imageType".into(), json!(image_type(&ext).unwrap_or("png")));
        m.insert("dimension".into(), json!({"width": w, "height": h}));
    } else {
        m.insert("ext".into(), json!(ext));
        m.insert("duration".into(), json!(seconds.unwrap_or(1.0)));
    }
    if let Some(r) = &resolved {
        files.push(AssetFile { source: r.clone(), target: fileurl.clone() });
        if is_image && ext == ".svg" {
            let twin = format!("{}.png", &r[..r.len() - 4]);
            if is_file(&twin) {
                let target = file_url_for(kind, &filename, ".png");
                m.insert("pngurl".into(), json!(target));
                files.push(AssetFile { source: twin, target });
            }
        }
    }
    Value::Object(m)
}
