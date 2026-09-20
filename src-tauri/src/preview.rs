//! Best-effort embedded-preview extraction for filmstrip thumbnails and
//! deflicker brightness measurement.
//!
//! RapidRAW does the real RAW demosaic on its own GPU pipeline; we don't
//! reimplement that. Instead we lean on the fact that virtually every RAW
//! format (NEF/RAF/RW2/ARW/CR2/CR3/DNG...) embeds a full-size JPEG preview
//! near the start of the file as part of its TIFF-derived container. Rather
//! than writing a parser per manufacturer, we scan the first few MB of the
//! file for JPEG SOI/EOI markers and keep the largest span that actually
//! decodes — format-agnostic, and cheap enough to run on every frame.

use base64::Engine;
use image::{GenericImageView, ImageEncoder};
use std::fs;
use std::path::Path;

/// Only look this far into the file. TIFF-based RAW containers keep their
/// preview IFD near the front; the multi-megabyte sensor payload comes
/// after. Keeps cost bounded even on 80MB+ RAW files.
const SCAN_CAP: usize = 16 * 1024 * 1024;

pub struct Preview {
    pub bytes: Vec<u8>,
}

pub fn get_preview(path: &Path) -> Option<Preview> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            let bytes = fs::read(path).ok()?;
            image::load_from_memory(&bytes).ok()?;
            Some(Preview { bytes })
        }
        "png" => {
            let bytes = fs::read(path).ok()?;
            image::load_from_memory(&bytes).ok()?;
            Some(Preview { bytes })
        }
        _ => extract_embedded_jpeg(path),
    }
}

fn extract_embedded_jpeg(path: &Path) -> Option<Preview> {
    // Read only the first SCAN_CAP bytes rather than the whole file -- RAW
    // frames can be 30-100+MB, and we only ever look near the front.
    use std::io::Read;
    let file = fs::File::open(path).ok()?;
    let mut region = Vec::with_capacity(SCAN_CAP);
    file.take(SCAN_CAP as u64).read_to_end(&mut region).ok()?;

    let mut best: Option<(usize, usize)> = None;
    let mut i = 0usize;
    while i + 1 < region.len() {
        if region[i] == 0xFF && region[i + 1] == 0xD8 {
            if let Some(end) = find_eoi(&region, i + 2) {
                let better = match best {
                    Some((s, e)) => (end - i) > (e - s),
                    None => true,
                };
                if better {
                    // Only keep it if it actually decodes -- a stray FFD8
                    // byte pair inside sensor data almost never does.
                    if image::load_from_memory(&region[i..end]).is_ok() {
                        best = Some((i, end));
                    }
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }

    let (start, end) = best?;
    Some(Preview {
        bytes: region[start..end].to_vec(),
    })
}

fn find_eoi(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

/// A small base64 JPEG data URL for the filmstrip, downscaled so the
/// Tauri IPC payload for a few hundred frames stays reasonable.
pub fn thumbnail_data_url(path: &Path, max_dim: u32) -> Option<String> {
    let preview = get_preview(path)?;
    let img = image::load_from_memory(&preview.bytes).ok()?;
    let (w, h) = img.dimensions();
    let longest = w.max(h) as f64;
    let resized = if longest > max_dim as f64 {
        let scale = max_dim as f64 / longest;
        img.resize(
            ((w as f64 * scale) as u32).max(1),
            ((h as f64 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };

    let rgb = resized.to_rgb8();
    let mut buf = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut buf);
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 72);
        encoder
            .write_image(&rgb, rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
            .ok()?;
    }
    Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&buf)
    ))
}

/// Mean relative luminance (0..1) of a small downsample of the frame's
/// preview, for deflicker brightness measurement.
pub fn measure_luma(path: &Path) -> Option<f64> {
    let preview = get_preview(path)?;
    let img = image::load_from_memory(&preview.bytes).ok()?;
    let small = img
        .resize(48, 48, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let mut sum = 0f64;
    let mut count = 0u32;
    for px in small.pixels() {
        let [r, g, b] = px.0;
        sum += 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(sum / count as f64 / 255.0)
}
