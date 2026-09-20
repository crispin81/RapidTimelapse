use crate::{deflicker, interpolate, params, preview, rrdata, sequence};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize, Clone)]
pub struct ScanProgress {
    pub done: usize,
    pub total: usize,
}

const THUMB_MAX_DIM: u32 = 220;
const DEFAULT_RATING_THRESHOLD: i64 = 5;

#[derive(Serialize)]
pub struct FrameInfo {
    pub index: usize,
    pub file: String,
    pub rating: i64,
    #[serde(rename = "hasSidecar")]
    pub has_sidecar: bool,
    #[serde(rename = "hasAdjustments")]
    pub has_adjustments: bool,
    pub thumbnail: Option<String>,
    pub params: BTreeMap<String, f64>,
}

#[derive(Serialize)]
pub struct ScanResult {
    pub frames: Vec<FrameInfo>,
    #[serde(rename = "suggestedKeyframes")]
    pub suggested_keyframes: Vec<usize>,
    #[serde(rename = "rampParams")]
    pub ramp_params: Vec<String>,
}

#[tauri::command]
pub async fn browse_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    // `FilePath` can come back as a `file://` URL rather than a plain path
    // (seen via some portal/GTK file-chooser configurations) — `into_path`
    // normalizes both cases to a real filesystem path.
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn scan_folder(app: tauri::AppHandle, folder: String) -> Result<ScanResult, String> {
    let frames = sequence::scan(&folder)?;
    let total = frames.len();

    // Thumbnail generation (read + decode + resize + re-encode a preview
    // per frame) dominates scan time on real sequences, so spread it across
    // threads rather than doing it one frame at a time, and report progress
    // as frames complete so a slow scan on a large sequence doesn't look
    // like it's frozen.
    let thread_count = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(1);
    let chunk_size = frames.len().div_ceil(thread_count).max(1);
    let mut thumbnails: Vec<Option<String>> = vec![None; frames.len()];
    let progress = Arc::new(AtomicUsize::new(0));
    let _ = app.emit("scan-progress", ScanProgress { done: 0, total });

    std::thread::scope(|scope| {
        let handles: Vec<_> = frames
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk_idx, chunk)| {
                let offset = chunk_idx * chunk_size;
                let app = app.clone();
                let progress = progress.clone();
                scope.spawn(move || {
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let thumb = preview::thumbnail_data_url(&f.path, THUMB_MAX_DIM);
                            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                            let _ = app.emit("scan-progress", ScanProgress { done, total });
                            (offset + i, thumb)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            for (idx, thumb) in handle.join().expect("thumbnail worker thread panicked") {
                thumbnails[idx] = thumb;
            }
        }
    });

    let mut ramp_param_set: BTreeSet<String> = BTreeSet::new();
    let mut suggested_keyframes = Vec::new();
    let mut out_frames = Vec::with_capacity(frames.len());

    for f in &frames {
        let rating = f.doc.as_ref().map(|d| d.rating).unwrap_or(0);
        if rating >= DEFAULT_RATING_THRESHOLD {
            suggested_keyframes.push(f.index);
        }

        let mut params_map = BTreeMap::new();
        if let Some(adj) = f.doc.as_ref().and_then(|d| d.adjustments.as_ref()) {
            params::flatten_ramp_leaves(adj, "", &mut params_map);
            for k in params_map.keys() {
                ramp_param_set.insert(k.clone());
            }
        }

        out_frames.push(FrameInfo {
            index: f.index,
            file: f.file.clone(),
            rating,
            has_sidecar: f.doc.is_some(),
            has_adjustments: f.doc.as_ref().map(|d| d.adjustments.is_some()).unwrap_or(false),
            thumbnail: thumbnails[f.index].take(),
            params: params_map,
        });
    }

    Ok(ScanResult {
        frames: out_frames,
        suggested_keyframes,
        ramp_params: params::sort_params_rapidraw_order(ramp_param_set.into_iter().collect()),
    })
}

#[derive(Serialize)]
pub struct PlannedFrameOut {
    pub index: usize,
    #[serde(rename = "isKeyframe")]
    pub is_keyframe: bool,
    pub params: Option<BTreeMap<String, f64>>,
    #[serde(rename = "deflickerStops")]
    pub deflicker_stops: Option<f64>,
}

#[derive(Serialize)]
pub struct DeflickerOut {
    pub measured: BTreeMap<usize, f64>,
    pub target: BTreeMap<usize, f64>,
}

#[derive(Serialize)]
pub struct PlanResult {
    pub frames: Vec<PlannedFrameOut>,
    pub deflicker: Option<DeflickerOut>,
}

#[allow(clippy::too_many_arguments)]
fn run_plan(
    folder: &str,
    keyframe_indices: &[usize],
    smoothing: f64,
    deflicker_enabled: bool,
    deflicker_max_stops: f64,
    deflicker_window: usize,
) -> Result<(Vec<sequence::Frame>, Vec<interpolate::PlannedFrame>, Option<deflicker::DeflickerResult>), String> {
    let frames = sequence::scan(folder)?;
    let keyframe_set: BTreeSet<usize> = keyframe_indices.iter().copied().collect();
    if keyframe_set.is_empty() {
        return Err("No keyframes selected. Rate at least two frames 5 stars first.".into());
    }

    let dfl = if deflicker_enabled {
        Some(deflicker::analyze(&frames, &keyframe_set, deflicker_window, deflicker_max_stops))
    } else {
        None
    };

    let plan = interpolate::build_plan(&frames, &keyframe_set, smoothing, dfl.as_ref());
    Ok((frames, plan, dfl))
}

fn plan_to_output(plan: &[interpolate::PlannedFrame], dfl: Option<deflicker::DeflickerResult>) -> PlanResult {
    let frames = plan
        .iter()
        .map(|pf| {
            let mut m = BTreeMap::new();
            if let Some(adj) = &pf.adjustments {
                params::flatten_ramp_leaves(adj, "", &mut m);
            }
            PlannedFrameOut {
                index: pf.index,
                is_keyframe: pf.is_keyframe,
                params: if pf.adjustments.is_some() { Some(m) } else { None },
                deflicker_stops: pf.deflicker_stops,
            }
        })
        .collect();

    PlanResult {
        frames,
        deflicker: dfl.map(|d| DeflickerOut { measured: d.measured, target: d.target }),
    }
}

#[tauri::command]
pub async fn compute_plan(
    folder: String,
    keyframe_indices: Vec<usize>,
    smoothing: f64,
    deflicker_enabled: bool,
    deflicker_max_stops: f64,
    deflicker_window: usize,
) -> Result<PlanResult, String> {
    let (_frames, plan, dfl) = run_plan(
        &folder,
        &keyframe_indices,
        smoothing,
        deflicker_enabled,
        deflicker_max_stops,
        deflicker_window,
    )?;
    Ok(plan_to_output(&plan, dfl))
}

#[derive(Serialize)]
pub struct WriteResult {
    #[serde(rename = "writtenCount")]
    pub written_count: usize,
    #[serde(rename = "backedUpCount")]
    pub backed_up_count: usize,
    #[serde(rename = "keyframesRatedCount")]
    pub keyframes_rated_count: usize,
}

#[tauri::command]
pub async fn write_plan(
    folder: String,
    keyframe_indices: Vec<usize>,
    smoothing: f64,
    deflicker_enabled: bool,
    deflicker_max_stops: f64,
    deflicker_window: usize,
    backup: bool,
) -> Result<WriteResult, String> {
    let (frames, plan, _dfl) = run_plan(
        &folder,
        &keyframe_indices,
        smoothing,
        deflicker_enabled,
        deflicker_max_stops,
        deflicker_window,
    )?;

    let by_index: BTreeMap<usize, &sequence::Frame> = frames.iter().map(|f| (f.index, f)).collect();
    let mut written_count = 0;
    let mut backed_up_count = 0;
    let mut keyframes_rated_count = 0;

    for pf in &plan {
        let Some(frame) = by_index.get(&pf.index) else { continue };

        if pf.is_keyframe {
            // Never touch a keyframe's own edit -- but if it was marked a
            // keyframe by hand in this app (not already 5-star on disk),
            // stamp the rating so RapidRAW and a future scan agree it's a
            // keyframe too, without altering its adjustments at all.
            let current_rating = frame.doc.as_ref().map(|d| d.rating).unwrap_or(0);
            if current_rating != 5 {
                if backup && !rrdata::backup_path(&frame.rrdata_path).exists() && frame.rrdata_path.exists() {
                    rrdata::backup_if_absent(&frame.rrdata_path)?;
                    backed_up_count += 1;
                }
                let mut doc = frame.doc.clone().unwrap_or_else(default_doc);
                doc.rating = 5;
                rrdata::write(&frame.rrdata_path, &doc)?;
                keyframes_rated_count += 1;
            }
            continue;
        }

        let Some(adjustments) = &pf.adjustments else { continue };

        if backup && !rrdata::backup_path(&frame.rrdata_path).exists() && frame.rrdata_path.exists() {
            rrdata::backup_if_absent(&frame.rrdata_path)?;
            backed_up_count += 1;
        }

        let mut doc = frame.doc.clone().unwrap_or_else(default_doc);
        doc.adjustments = Some(adjustments.clone());
        rrdata::write(&frame.rrdata_path, &doc)?;
        written_count += 1;
    }

    Ok(WriteResult { written_count, backed_up_count, keyframes_rated_count })
}

fn default_doc() -> rrdata::RrDoc {
    rrdata::RrDoc {
        version: 1,
        rating: 0,
        adjustments: None,
        tags: Value::Null,
        extra: Map::new(),
    }
}

#[derive(Serialize)]
pub struct RevertResult {
    #[serde(rename = "restoredCount")]
    pub restored_count: usize,
}

#[tauri::command]
pub async fn revert_folder(folder: String) -> Result<RevertResult, String> {
    let frames = sequence::scan(&folder)?;
    let mut restored_count = 0;
    for f in &frames {
        let bak = rrdata::backup_path(&f.rrdata_path);
        if bak.exists() {
            std::fs::copy(&bak, &f.rrdata_path).map_err(|e| e.to_string())?;
            restored_count += 1;
        }
    }
    Ok(RevertResult { restored_count })
}
