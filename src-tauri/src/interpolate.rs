//! The ramp engine: turns a set of keyframes into a full per-frame plan.
//!
//! For every non-keyframe frame we start from a clone of the *nearest*
//! keyframe's adjustments (so masks, crop, LUT, lens correction etc. all
//! come along correctly with no extra bookkeeping), then overwrite the
//! rampable numeric leaves with a value eased between the two bracketing
//! keyframes. A frame before the first keyframe or after the last one has
//! only one bracket, so it just inherits that keyframe's settings outright
//! (flat extrapolation) — matches the LRTimelapse convention that the
//! first and last frame of a sequence should themselves be keyframes.

use crate::params;
use crate::sequence::Frame;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub struct PlannedFrame {
    pub index: usize,
    pub is_keyframe: bool,
    /// `None` only when there are no keyframes with real adjustments to
    /// draw from at all.
    pub adjustments: Option<Map<String, Value>>,
    pub deflicker_stops: Option<f64>,
}

/// `smoothing` in [0,1]: 0 is a straight linear ramp between keyframes,
/// 1 eases in and out of every keyframe (smoothstep), values between blend
/// the two — mirrors LRTimelapse's "smoothing" control on visualized
/// keyframes.
pub fn build_plan(
    frames: &[Frame],
    keyframe_indices: &BTreeSet<usize>,
    smoothing: f64,
    deflicker: Option<&crate::deflicker::DeflickerResult>,
) -> Vec<PlannedFrame> {
    let by_index: BTreeMap<usize, &Frame> = frames.iter().map(|f| (f.index, f)).collect();
    let kf_sorted: Vec<usize> = keyframe_indices.iter().copied().collect();
    let smoothing = smoothing.clamp(0.0, 1.0);

    let mut out = Vec::with_capacity(frames.len());
    for f in frames {
        if keyframe_indices.contains(&f.index) {
            out.push(PlannedFrame {
                index: f.index,
                is_keyframe: true,
                adjustments: None,
                deflicker_stops: None,
            });
            continue;
        }

        let before = kf_sorted.iter().rev().find(|&&k| k < f.index).copied();
        let after = kf_sorted.iter().find(|&&k| k > f.index).copied();

        let mut adjustments = match (before, after) {
            (Some(a), Some(b)) => {
                let adj_a = by_index.get(&a).and_then(|fr| fr.doc.as_ref()).and_then(|d| d.adjustments.as_ref());
                let adj_b = by_index.get(&b).and_then(|fr| fr.doc.as_ref()).and_then(|d| d.adjustments.as_ref());
                let nearest = if f.index - a <= b - f.index { a } else { b };
                let nearest_adj = by_index
                    .get(&nearest)
                    .and_then(|fr| fr.doc.as_ref())
                    .and_then(|d| d.adjustments.as_ref());

                match (adj_a, adj_b, nearest_adj) {
                    (Some(aa), Some(bb), Some(nn)) => {
                        let t = (f.index - a) as f64 / (b - a) as f64;
                        let eased = ease(t, smoothing);
                        Some(ramp_between(aa, bb, nn, eased))
                    }
                    _ => nearest_adj.cloned(),
                }
            }
            (Some(a), None) => by_index.get(&a).and_then(|fr| fr.doc.as_ref()).and_then(|d| d.adjustments.clone()),
            (None, Some(b)) => by_index.get(&b).and_then(|fr| fr.doc.as_ref()).and_then(|d| d.adjustments.clone()),
            (None, None) => None,
        };

        let mut deflicker_stops = None;
        if let (Some(adj), Some(dfl)) = (adjustments.as_mut(), deflicker) {
            if let Some(&stop) = dfl.stops.get(&f.index) {
                let current = adj.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0);
                adj.insert("exposure".into(), serde_json::json!(current + stop));
                deflicker_stops = Some(stop);
            }
        }

        out.push(PlannedFrame {
            index: f.index,
            is_keyframe: false,
            adjustments,
            deflicker_stops,
        });
    }
    out
}

fn ease(t: f64, smoothing: f64) -> f64 {
    let smoothstep = t * t * (3.0 - 2.0 * t);
    t + (smoothstep - t) * smoothing
}

fn ramp_between(a: &Map<String, Value>, b: &Map<String, Value>, nearest: &Map<String, Value>, t: f64) -> Map<String, Value> {
    let mut out = nearest.clone();

    let mut leaves_a = BTreeMap::new();
    params::flatten_ramp_leaves(a, "", &mut leaves_a);
    let mut leaves_b = BTreeMap::new();
    params::flatten_ramp_leaves(b, "", &mut leaves_b);

    for (path, &va) in &leaves_a {
        if let Some(&vb) = leaves_b.get(path) {
            params::set_by_path(&mut out, path, va + (vb - va) * t);
        }
    }

    for key in params::CURVE_KEYS {
        if let (Some(Value::Object(ca)), Some(Value::Object(cb))) = (a.get(*key), b.get(*key)) {
            if let Some(interp) = params::interpolate_curve_set(ca, cb, t) {
                out.insert((*key).to_string(), Value::Object(interp));
            }
        }
    }

    if let (Some(Value::Array(ma)), Some(Value::Array(mb)), Some(Value::Array(mn))) =
        (a.get("masks"), b.get("masks"), out.get("masks").cloned())
    {
        out.insert("masks".to_string(), Value::Array(params::interpolate_masks(ma, mb, &mn, t)));
    }

    out
}
