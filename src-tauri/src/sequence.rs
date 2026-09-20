//! Scan a working directory for a timelapse frame sequence and pair each
//! frame with its `.rrdata` sidecar, in natural (numeric-aware) filename
//! order — matching RapidRAW/Lightroom-style in-camera numbering.

use crate::rrdata::{self, RrDoc};
use std::fs;
use std::path::{Path, PathBuf};

const IMAGE_EXTS: &[&str] = &[
    "nef", "raf", "rw2", "arw", "cr2", "cr3", "dng", "orf", "pef", "srw", "raw", "x3f", "3fr",
    "erf", "kdc", "mrw", "nrw", "rwl", "iiq", "jpg", "jpeg", "png", "tif", "tiff",
];

pub struct Frame {
    pub index: usize,
    pub path: PathBuf,
    pub file: String,
    pub rrdata_path: PathBuf,
    pub doc: Option<RrDoc>,
}

pub fn is_image_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

pub fn scan(folder: &str) -> Result<Vec<Frame>, String> {
    let dir = Path::new(folder);
    if !dir.is_dir() {
        return Err(format!("Not a folder: {folder}"));
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Can't read {folder}: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image_file(p))
        .collect();

    paths.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)));

    let mut frames = Vec::with_capacity(paths.len());
    for (index, path) in paths.into_iter().enumerate() {
        let rrdata_path = sidecar_path(&path);
        let doc = if rrdata_path.exists() {
            rrdata::read(&rrdata_path).ok()
        } else {
            None
        };
        frames.push(Frame {
            index,
            file: file_name(&path),
            path,
            rrdata_path,
            doc,
        });
    }
    Ok(frames)
}

pub fn sidecar_path(image_path: &Path) -> PathBuf {
    let mut s = image_path.as_os_str().to_os_string();
    s.push(".rrdata");
    PathBuf::from(s)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Compare two filenames treating runs of ASCII digits as numbers, so
/// `frame_9` sorts before `frame_10`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        let (ac, bc) = match (ai.peek(), bi.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(&ac), Some(&bc)) => (ac, bc),
        };
        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let an: String = take_digits(&mut ai);
            let bn: String = take_digits(&mut bi);
            let av: u64 = an.parse().unwrap_or(0);
            let bv: u64 = bn.parse().unwrap_or(0);
            match av.cmp(&bv) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        } else {
            ai.next();
            bi.next();
            match ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase()) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if c.is_ascii_digit() {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    s
}
