//! Read/write RapidRAW's `.rrdata` sidecar files.
//!
//! Only `version`, `rating`, `adjustments` and `tags` are modeled explicitly;
//! everything else RapidRAW puts at the top level (e.g. `exif`) is captured
//! by `extra` via `#[serde(flatten)]` and round-tripped untouched. Within
//! `adjustments` we keep the raw `serde_json::Map` rather than a typed
//! struct, so fields this tool doesn't know about (future RapidRAW versions,
//! odd/undocumented keys) survive a scan -> ramp -> write cycle unharmed.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RrDoc {
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default)]
    pub rating: i64,
    #[serde(default)]
    pub adjustments: Option<Map<String, Value>>,
    #[serde(default)]
    pub tags: Value,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_version() -> i64 {
    1
}

pub fn read(path: &Path) -> Result<RrDoc, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn write(path: &Path, doc: &RrDoc) -> Result<(), String> {
    let data = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn backup_path(rrdata_path: &Path) -> PathBuf {
    let mut s = rrdata_path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

/// Copy the current sidecar to `<file>.rrdata.bak` if no backup already
/// exists. We never overwrite an existing backup, so the very first backup
/// taken (the user's real, pre-ramp state) is the one `revert` restores,
/// no matter how many times ramp/write is re-run afterward.
pub fn backup_if_absent(rrdata_path: &Path) -> Result<(), String> {
    if !rrdata_path.exists() {
        return Ok(());
    }
    let bak = backup_path(rrdata_path);
    if bak.exists() {
        return Ok(());
    }
    fs::copy(rrdata_path, &bak)
        .map(|_| ())
        .map_err(|e| format!("backing up {}: {e}", rrdata_path.display()))
}
