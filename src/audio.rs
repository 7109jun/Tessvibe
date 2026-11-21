//! Audio length probing for wav, ogg, mp4/m4a and mp3 (header based).

fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?) as u32)
}
fn le16(b: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as u32)
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn be64(b: &[u8], at: usize) -> Option<f64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?) as f64)
}
fn le64(b: &[u8], at: usize) -> Option<f64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?) as f64)
}
fn latin1(b: &[u8], s: usize, e: usize) -> String {
    b[s.min(b.len())..e.min(b.len())].iter().map(|c| *c as char).collect()
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn audio_duration(bytes: &[u8], ext: &str) -> Option<f64> {
    let seconds = measure(bytes, ext)?;
    if seconds.is_finite() && seconds > 0.0 {
        Some(js_round(seconds * 10.0) / 10.0)
    } else {
        None
    }
}

fn measure(b: &[u8], ext: &str) -> Option<f64> {
    match ext {
        ".wav" => return wav(b),
        ".ogg" => return ogg(b),
        ".m4a" | ".mp4" => return mp4(b, 0, b.len()),
        ".mp3" => return mp3(b),
        _ => {}
    }
    if b.len() > 12 && latin1(b, 0, 4) == "RIFF" {
        return wav(b);
    }
    if b.len() > 4 && latin1(b, 0, 4) == "OggS" {
        return ogg(b);
    }
    mp3(b)
}

fn wav(b: &[u8]) -> Option<f64> {
    if b.len() < 12 || latin1(b, 8, 12) != "WAVE" {
        return None;
    }
    let mut byte_rate = 0u32;
    let mut offset = 12usize;
    while offset + 8 <= b.len() {
        let id = latin1(b, offset, offset + 4);
        let size = le32(b, offset + 4)? as usize;
        let body = offset + 8;
        if id == "fmt " && body + 16 <= b.len() {
            byte_rate = le32(b, body + 8)?;
        }
        if id == "data" {
            if byte_rate == 0 {
                return None;
            }
            let data_size = if size > 0 && body + size <= b.len() { size } else { b.len() - body };
            return Some(data_size as f64 / byte_rate as f64);
        }
        offset = body + size + (size % 2);
    }
    None
}

fn find(hay: &str, needle: &str) -> Option<usize> {
    hay.find(needle)
}

fn ogg(b: &[u8]) -> Option<f64> {
    if latin1(b, 0, 4) != "OggS" {
        return None;
    }
    let head = latin1(b, 0, b.len().min(4096));
    let (rate, pre_skip);
    if let Some(opus) = find(&head, "OpusHead") {
        rate = 48000.0;
        pre_skip = le16(b, opus + 10)? as f64;
    } else {
        let vorbis = find(&head, "\u{1}vorbis")?;
        rate = le32(b, vorbis + 12)? as f64;
        pre_skip = 0.0;
    }
    if rate == 0.0 {
        return None;
    }
    if b.len() < 14 {
        return None;
    }
    let mut offset = b.len() as isize - 14;
    while offset >= 0 {
        let o = offset as usize;
        if latin1(b, o, o + 4) == "OggS" {
            let granule = le64(b, o + 6)?;
            if granule > 0.0 {
                return Some((granule - pre_skip).max(0.0) / rate);
            }
        }
        offset -= 1;
    }
    None
}

fn mp4(b: &[u8], start: usize, end: usize) -> Option<f64> {
    let mut offset = start;
    while offset + 8 <= end {
        let mut size = be32(b, offset)? as f64;
        let ty = latin1(b, offset + 4, offset + 8);
        let mut body = offset + 8;
        if size == 1.0 {
            if body + 8 > end {
                return None;
            }
            size = be64(b, body)?;
            body += 8;
        }
        if size == 0.0 {
            size = (end - offset) as f64;
        }
        if ty == "moov" {
            let e2 = (end as f64).min(offset as f64 + size) as usize;
            return mp4(b, body, e2);
        }
        if ty == "mvhd" {
            let version = *b.get(body)?;
            let at = if version == 1 { body + 20 } else { body + 12 };
            let timescale = be32(b, at)? as f64;
            let duration = if version == 1 { be64(b, at + 4)? } else { be32(b, at + 4)? as f64 };
            return if timescale != 0.0 { Some(duration / timescale) } else { None };
        }
        if size < 8.0 {
            return None;
        }
        offset += size as usize;
    }
    None
}

struct Frame {
    version: f64,
    layer: u32,
    bitrate: u32,
    sample_rate: u32,
    samples: u32,
    mono: bool,
}

const BR1: [[u32; 15]; 3] = [
    [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
    [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
    [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
];
const BR2: [[u32; 15]; 3] = [
    [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
];

fn skip_id3(b: &[u8]) -> usize {
    if b.len() < 10 || latin1(b, 0, 3) != "ID3" {
        return 0;
    }
    let size = (((b[6] & 0x7f) as usize) << 21) | (((b[7] & 0x7f) as usize) << 14) | (((b[8] & 0x7f) as usize) << 7) | (b[9] & 0x7f) as usize;
    10 + size + if b[5] & 0x10 != 0 { 10 } else { 0 }
}

fn frame_header(b: &[u8], o: usize) -> Option<Frame> {
    if o + 4 > b.len() {
        return None;
    }
    if b[o] != 0xff || (b[o + 1] & 0xe0) != 0xe0 {
        return None;
    }
    let vb = (b[o + 1] >> 3) & 3;
    let lb = (b[o + 1] >> 1) & 3;
    if vb == 1 || lb == 0 {
        return None;
    }
    let version = match vb { 0 => 2.5, 2 => 2.0, 3 => 1.0, _ => return None };
    let layer = match lb { 1 => 3, 2 => 2, 3 => 1, _ => return None };
    let table = if version == 1.0 { &BR1 } else { &BR2 };
    let bitrate = table[(layer - 1) as usize].get(((b[o + 2] >> 4) & 0x0f) as usize).copied().unwrap_or(0);
    let rates: [u32; 3] = if version == 1.0 { [44100, 48000, 32000] } else if version == 2.0 { [22050, 24000, 16000] } else { [11025, 12000, 8000] };
    let sample_rate = rates.get(((b[o + 2] >> 2) & 3) as usize).copied().unwrap_or(0);
    if bitrate == 0 || sample_rate == 0 {
        return None;
    }
    let mono = ((b[o + 3] >> 6) & 3) == 3;
    let samples = if layer == 1 { 384 } else if layer == 2 || version == 1.0 { 1152 } else { 576 };
    Some(Frame { version, layer, bitrate, sample_rate, samples, mono })
}

fn mp3(b: &[u8]) -> Option<f64> {
    let start = skip_id3(b);
    let mut frame = None;
    let mut offset = start;
    let limit = b.len().min(start + 65536);
    while offset < limit {
        if let Some(f) = frame_header(b, offset) {
            frame = Some(f);
            break;
        }
        offset += 1;
    }
    let frame = frame?;
    let vbr = xing(b, offset, &frame).or_else(|| vbri(b, offset));
    if let Some(n) = vbr {
        if n != 0 {
            return Some(n as f64 * frame.samples as f64 / frame.sample_rate as f64);
        }
    }
    let has_v1 = b.len() >= 128 && latin1(b, b.len() - 128, b.len() - 125) == "TAG";
    let audio = (b.len() - if has_v1 { 128 } else { 0 }) as f64 - offset as f64;
    let _ = frame.layer;
    Some(audio * 8.0 / (frame.bitrate as f64 * 1000.0))
}

fn xing(b: &[u8], start: usize, f: &Frame) -> Option<u32> {
    let skip = if f.version == 1.0 { if f.mono { 21 } else { 36 } } else if f.mono { 13 } else { 21 };
    let at = start + skip;
    if at + 12 > b.len() {
        return None;
    }
    let tag = latin1(b, at, at + 4);
    if tag != "Xing" && tag != "Info" {
        return None;
    }
    let flags = be32(b, at + 4)?;
    if flags & 1 != 0 { be32(b, at + 8) } else { None }
}

fn vbri(b: &[u8], start: usize) -> Option<u32> {
    let at = start + 36;
    if at + 18 > b.len() {
        return None;
    }
    if latin1(b, at, at + 4) != "VBRI" {
        return None;
    }
    be32(b, at + 14)
}

pub fn be16_pub(b: &[u8], at: usize) -> Option<u32> { be16(b, at) }
pub fn le16_pub(b: &[u8], at: usize) -> Option<u32> { le16(b, at) }
pub fn be32_pub(b: &[u8], at: usize) -> Option<u32> { be32(b, at) }
