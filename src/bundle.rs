//! `.ent` container: a ustar archive holding `temp/project.json` and assets.
use crate::assets::AssetFile;
use image::imageops::FilterType;
use image::ImageFormat;
use std::collections::HashSet;
use std::io::Cursor;

const THUMB_BOX: u32 = 96;

pub struct TarEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn octal(value: u64, width: usize) -> Vec<u8> {
    let mut s = format!("{:0w$o}", value, w = width - 1).into_bytes();
    s.push(0);
    s
}

fn put(block: &mut [u8; 512], at: usize, bytes: &[u8]) {
    block[at..at + bytes.len()].copy_from_slice(bytes);
}

fn split_name(name: &str) -> (String, String) {
    if name.len() <= 100 {
        return (String::new(), name.to_string());
    }
    for (i, c) in name.char_indices().rev() {
        if c == '/' && i <= 155 && name.len() - i - 1 <= 100 {
            return (name[..i].to_string(), name[i + 1..].to_string());
        }
    }
    (String::new(), name.chars().take(100).collect())
}

fn header(name: &str, size: usize, mtime: u64) -> [u8; 512] {
    let mut b = [0u8; 512];
    let (prefix, base) = split_name(name);
    put(&mut b, 0, base.as_bytes());
    put(&mut b, 100, &octal(0o644, 8));
    put(&mut b, 108, &octal(0, 8));
    put(&mut b, 116, &octal(0, 8));
    put(&mut b, 124, &octal(size as u64, 12));
    put(&mut b, 136, &octal(mtime, 12));
    put(&mut b, 148, b"        ");
    b[156] = b'0';
    put(&mut b, 257, b"ustar\0");
    put(&mut b, 263, b"00");
    put(&mut b, 345, prefix.as_bytes());
    let sum: u32 = b.iter().map(|x| *x as u32).sum();
    let mut chk = format!("{:06o}", sum).into_bytes();
    chk.push(0);
    chk.push(b' ');
    put(&mut b, 148, &chk);
    b
}

pub fn make_tar(entries: &[TarEntry]) -> Vec<u8> {
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut out = Vec::new();
    for e in entries {
        out.extend_from_slice(&header(&e.name, e.data.len(), mtime));
        out.extend_from_slice(&e.data);
        let rem = e.data.len() % 512;
        if rem != 0 {
            out.extend(std::iter::repeat(0u8).take(512 - rem));
        }
    }
    out.extend(std::iter::repeat(0u8).take(1024));
    out
}

/// 96px "inside" PNG preview; None for formats that cannot be decoded (e.g. SVG).
pub fn make_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (THUMB_BOX as f64 / w as f64).min(THUMB_BOX as f64 / h as f64).min(1.0);
    let img = if scale < 1.0 {
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        img.resize_exact(nw, nh, FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

pub fn make_entry_bundle(project_json: &str, assets: &[AssetFile]) -> Result<Vec<u8>, String> {
    let mut entries = vec![TarEntry { name: "temp/project.json".into(), data: project_json.as_bytes().to_vec() }];
    let mut packed: HashSet<String> = HashSet::new();
    for asset in assets {
        if !packed.insert(asset.target.clone()) {
            continue;
        }
        let data = std::fs::read(&asset.source).map_err(|e| format!("{}: {}", asset.source, e))?;
        let thumb_name = asset.target.replacen("/image/", "/thumb/", 1);
        let thumb = if thumb_name != asset.target && packed.insert(thumb_name.clone()) {
            make_thumbnail(&data).map(|t| TarEntry { name: thumb_name, data: t })
        } else {
            None
        };
        entries.push(TarEntry { name: asset.target.clone(), data });
        if let Some(t) = thumb {
            entries.push(t);
        }
    }
    Ok(make_tar(&entries))
}
