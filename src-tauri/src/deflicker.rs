//! Measures actual per-frame brightness (from each frame's embedded
//! preview) and computes a small per-frame exposure correction that
//! flattens frame-to-frame flicker while leaving the intentional
//! keyframe-driven exposure ramp alone. This is on top of, not instead of,
//! the ramped `exposure` value — the ramp handles the deliberate trend
//! (e.g. day to night), deflicker handles the noise on top of it (a cloud
//! passing, a shutter/aperture micro-variation).

use crate::sequence::Frame;
use std::collections::{BTreeMap, BTreeSet};

pub struct DeflickerResult {
    /// Measured luma (0..1) per frame index, for frames where a preview
    /// could be extracted.
    pub measured: BTreeMap<usize, f64>,
    /// Smoothed brightness trend each frame is corrected toward.
    pub target: BTreeMap<usize, f64>,
    /// EV correction to add to the ramped exposure, per non-keyframe frame.
    pub stops: BTreeMap<usize, f64>,
}

pub fn analyze(
    frames: &[Frame],
    keyframe_indices: &BTreeSet<usize>,
    window: usize,
    max_stops: f64,
) -> DeflickerResult {
    let mut measured = BTreeMap::new();
    for f in frames {
        if let Some(luma) = crate::preview::measure_luma(&f.path) {
            measured.insert(f.index, luma);
        }
    }

    let indices: Vec<usize> = measured.keys().copied().collect();
    let half = window.max(1) / 2;
    let mut target = BTreeMap::new();
    for (pos, &idx) in indices.iter().enumerate() {
        let lo = pos.saturating_sub(half);
        let hi = (pos + half).min(indices.len().saturating_sub(1));
        let slice = &indices[lo..=hi];
        let avg = slice.iter().map(|i| measured[i]).sum::<f64>() / slice.len() as f64;
        target.insert(idx, avg);
    }

    let mut stops = BTreeMap::new();
    for (&idx, &m) in &measured {
        if keyframe_indices.contains(&idx) {
            continue; // keyframes are never modified
        }
        if m <= 0.0001 {
            continue; // avoid log(0) on a near-black measurement
        }
        let t = target[&idx];
        let stop = (t / m).log2().clamp(-max_stops, max_stops);
        stops.insert(idx, stop);
    }

    DeflickerResult { measured, target, stops }
}
