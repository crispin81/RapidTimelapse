//! Knows which leaves of a RapidRAW `adjustments` object are safe to ramp
//! (smoothly interpolated between keyframes) versus which must be carried
//! over verbatim from the nearest keyframe (geometry, masks, LUT choice,
//! lens-correction toggles...) or preserved as the target frame's own
//! UI state rather than touched at all.
//!
//! Deliberately generic: rather than hand-listing every one of RapidRAW's
//! ~90 adjustment fields, we walk the JSON recursively and ramp every
//! number we find, except under a short blacklist of top-level keys. That
//! keeps this correct across RapidRAW versions that add new sliders.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Not ramped: copied wholesale from the nearest keyframe. Geometry, crop,
/// LUT choice, lens-correction profile — things where "halfway between two
/// settings" isn't a meaningful value. `masks` is listed here too but gets
/// its own smarter handling (see `interpolate_masks`): a mask is only ever
/// carried like this when it *can't* be matched to the same mask on the
/// other bracketing keyframe.
pub const CARRY_KEYS: &[&str] = &[
    "aiPatches",
    "aspectRatio",
    "crop",
    "curveMode",
    "flipHorizontal",
    "flipVertical",
    "guidedPerspective",
    "lensBlurDepthMap",
    "lensBlurEnabled",
    "lensBlurShape",
    "lensCorrectionMode",
    "lensDistortionAmount",
    "lensDistortionEnabled",
    "lensMaker",
    "lensModel",
    "lensTcaAmount",
    "lensTcaEnabled",
    "lensVignetteAmount",
    "lensVignetteEnabled",
    "lutData",
    "lutIntensity",
    "lutIsSceneReferred",
    "lutName",
    "lutPath",
    "lutSize",
    "masks",
    "orientationSteps",
    "rotation",
    "toneMapper",
    "transformAspect",
    "transformDistortion",
    "transformHorizontal",
    "transformRotate",
    "transformScale",
    "transformXOffset",
    "transformYOffset",
    // Undocumented/unknown-purpose field observed in real sidecars; left
    // alone out of caution rather than guessed at.
    "centr\u{e9}",
];

/// Per-frame UI/app state, not part of the "look" of the edit. Left as
/// whatever the target frame's own file already had (see rrdata_writer).
pub const UI_STATE_KEYS: &[&str] = &["sectionVisibility", "showClipping", "rating"];

/// Point-curve arrays: ramped specially (see `interpolate_curve_set`)
/// because they're arrays of {x,y} points, not plain numbers.
pub const CURVE_KEYS: &[&str] = &["curves", "pointCurves"];

/// Flatten every rampable numeric leaf of `obj` into dotted paths, e.g.
/// `hsl.reds.hue` -> -12.0. Skips CARRY/UI_STATE/CURVE top-level keys.
pub fn flatten_ramp_leaves(obj: &Map<String, Value>, prefix: &str, out: &mut BTreeMap<String, f64>) {
    for (k, v) in obj {
        if prefix.is_empty()
            && (CARRY_KEYS.contains(&k.as_str())
                || UI_STATE_KEYS.contains(&k.as_str())
                || CURVE_KEYS.contains(&k.as_str()))
        {
            continue;
        }
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    out.insert(path, f);
                }
            }
            Value::Object(sub) => flatten_ramp_leaves(sub, &path, out),
            _ => {}
        }
    }
}

/// Write a single dotted-path leaf value back into a nested adjustments map.
/// Assumes the parent objects along the path already exist (true whenever
/// `obj` started life as a clone of a real keyframe's adjustments).
pub fn set_by_path(obj: &mut Map<String, Value>, path: &str, value: f64) {
    let parts: Vec<&str> = path.split('.').collect();
    set_by_parts(obj, &parts, value);
}

fn set_by_parts(obj: &mut Map<String, Value>, parts: &[&str], value: f64) {
    if parts.len() == 1 {
        obj.insert(parts[0].to_string(), serde_json::json!(value));
        return;
    }
    let entry = obj
        .entry(parts[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(sub) = entry {
        set_by_parts(sub, &parts[1..], value);
    }
}

/// Interpolate two curve-set objects (`curves` or `pointCurves`: a map of
/// channel name -> array of `{x, y}` points) point-by-point. Returns `None`
/// if the two keyframes disagree on channel names, point counts, or x
/// positions for any channel — in that case the caller falls back to
/// carrying the curve over from the nearest keyframe instead of guessing
/// at a blend between differently-shaped curves.
pub fn interpolate_curve_set(a: &Map<String, Value>, b: &Map<String, Value>, t: f64) -> Option<Map<String, Value>> {
    let mut out = Map::new();
    for (channel, va) in a {
        let vb = b.get(channel)?;
        let pa = va.as_array()?;
        let pb = vb.as_array()?;
        if pa.len() != pb.len() {
            return None;
        }
        let mut points = Vec::with_capacity(pa.len());
        for (pta, ptb) in pa.iter().zip(pb.iter()) {
            let xa = pta.get("x")?.as_f64()?;
            let xb = ptb.get("x")?.as_f64()?;
            if (xa - xb).abs() > 0.001 {
                return None;
            }
            let ya = pta.get("y")?.as_f64()?;
            let yb = ptb.get("y")?.as_f64()?;
            points.push(serde_json::json!({ "x": xa, "y": ya + (yb - ya) * t }));
        }
        out.insert(channel.clone(), Value::Array(points));
    }
    Some(out)
}

/// Numeric geometry fields for RapidRAW's two "trackable" mask shapes.
/// Other mask types (brush strokes, AI subject/sky selections, quick
/// eraser...) describe their shape as pixel data or point lists with no
/// simple center/size/rotation to ramp, so those are always carried from
/// the nearest keyframe rather than guessed at.
const RADIAL_MASK_PARAMS: &[&str] = &["centerX", "centerY", "radiusX", "radiusY", "rotation", "feather"];
const LINEAR_MASK_PARAMS: &[&str] = &["startX", "startY", "endX", "endY", "range"];

fn find_mask_by_id<'a>(list: &'a [Value], id: &str) -> Option<&'a Map<String, Value>> {
    list.iter()
        .find_map(|v| v.as_object().filter(|o| o.get("id").and_then(|v| v.as_str()) == Some(id)))
}

/// Ramp a sequence's `masks` array between two keyframes. Each mask in
/// `nearest` (a clone of the bracketing keyframe closest to this frame) is
/// looked up by `id` in both `a` and `b`: found in both -> its geometry,
/// opacity, and local (in-mask) adjustments are interpolated the same way
/// the top-level adjustments are; found in only one, or not a trackable
/// shape -> left exactly as `nearest` has it. This is what lets a radial
/// gradient (e.g. isolating the Milky Way) tracked and rotated by hand on
/// each keyframe smoothly follow through the frames in between, while any
/// mask that isn't part of that workflow is untouched.
pub fn interpolate_masks(a: &[Value], b: &[Value], nearest: &[Value], t: f64) -> Vec<Value> {
    nearest.iter().map(|m| interpolate_mask_container(m, a, b, t)).collect()
}

fn interpolate_mask_container(nearest_val: &Value, a: &[Value], b: &[Value], t: f64) -> Value {
    let Some(nearest_obj) = nearest_val.as_object() else {
        return nearest_val.clone();
    };
    let Some(id) = nearest_obj.get("id").and_then(|v| v.as_str()) else {
        return nearest_val.clone();
    };
    let (Some(a_obj), Some(b_obj)) = (find_mask_by_id(a, id), find_mask_by_id(b, id)) else {
        return nearest_val.clone();
    };

    let mut out = nearest_obj.clone();

    if let (Some(oa), Some(ob)) = (
        a_obj.get("opacity").and_then(|v| v.as_f64()),
        b_obj.get("opacity").and_then(|v| v.as_f64()),
    ) {
        out.insert("opacity".into(), serde_json::json!(oa + (ob - oa) * t));
    }

    if let (Some(Value::Object(aa)), Some(Value::Object(ba))) = (a_obj.get("adjustments"), b_obj.get("adjustments")) {
        let mut leaves_a = BTreeMap::new();
        flatten_ramp_leaves(aa, "", &mut leaves_a);
        let mut leaves_b = BTreeMap::new();
        flatten_ramp_leaves(ba, "", &mut leaves_b);
        if let Some(Value::Object(out_adj)) = out.get_mut("adjustments") {
            for (path, &va) in &leaves_a {
                if let Some(&vb) = leaves_b.get(path) {
                    set_by_path(out_adj, path, va + (vb - va) * t);
                }
            }
            for key in CURVE_KEYS {
                if let (Some(Value::Object(ca)), Some(Value::Object(cb))) = (aa.get(*key), ba.get(*key)) {
                    if let Some(interp) = interpolate_curve_set(ca, cb, t) {
                        out_adj.insert((*key).to_string(), Value::Object(interp));
                    }
                }
            }
        }
    }

    if let (Some(Value::Array(a_subs)), Some(Value::Array(b_subs)), Some(Value::Array(nearest_subs))) =
        (a_obj.get("subMasks"), b_obj.get("subMasks"), nearest_obj.get("subMasks"))
    {
        let interpolated: Vec<Value> = nearest_subs
            .iter()
            .map(|sub| interpolate_submask(sub, a_subs, b_subs, t))
            .collect();
        out.insert("subMasks".to_string(), Value::Array(interpolated));
    }

    Value::Object(out)
}

fn interpolate_submask(nearest_val: &Value, a_subs: &[Value], b_subs: &[Value], t: f64) -> Value {
    let Some(nearest_obj) = nearest_val.as_object() else {
        return nearest_val.clone();
    };
    let Some(id) = nearest_obj.get("id").and_then(|v| v.as_str()) else {
        return nearest_val.clone();
    };
    let sub_type = nearest_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let tracked_params: &[&str] = match sub_type {
        "radial" => RADIAL_MASK_PARAMS,
        "linear" => LINEAR_MASK_PARAMS,
        _ => return nearest_val.clone(),
    };

    let (Some(a_obj), Some(b_obj)) = (find_mask_by_id(a_subs, id), find_mask_by_id(b_subs, id)) else {
        return nearest_val.clone();
    };
    if a_obj.get("type").and_then(|v| v.as_str()) != Some(sub_type)
        || b_obj.get("type").and_then(|v| v.as_str()) != Some(sub_type)
    {
        return nearest_val.clone(); // shape changed between keyframes -- don't guess at a blend
    }

    let mut out = nearest_obj.clone();
    if let (Some(Value::Object(pa)), Some(Value::Object(pb))) = (a_obj.get("parameters"), b_obj.get("parameters")) {
        if let Some(Value::Object(out_params)) = out.get_mut("parameters") {
            for &key in tracked_params {
                if let (Some(va), Some(vb)) = (
                    pa.get(key).and_then(|v| v.as_f64()),
                    pb.get(key).and_then(|v| v.as_f64()),
                ) {
                    out_params.insert(key.to_string(), serde_json::json!(va + (vb - va) * t));
                }
            }
        }
    }
    if let (Some(oa), Some(ob)) = (
        a_obj.get("opacity").and_then(|v| v.as_f64()),
        b_obj.get("opacity").and_then(|v| v.as_f64()),
    ) {
        out.insert("opacity".into(), serde_json::json!(oa + (ob - oa) * t));
    }
    Value::Object(out)
}

/// The order RapidRAW itself lays these sliders out in, panel by panel
/// (Basic, Curves, Color, Details, Effects), read directly from its
/// `src/components/adjustments/*.tsx` source rather than guessed at. Used
/// to sort the parameter list shown in the UI (curve dropdown, per-frame
/// table) so it reads the same way RapidRAW's own editor does instead of
/// alphabetically.
const RAPIDRAW_PARAM_ORDER: &[&str] = &[
    // Basic
    "exposure",
    "contrast",
    "highlights",
    "shadows",
    "whites",
    "blacks",
    "brightness",
    // Curves (parametric curve, channel by channel: luma, red, green, blue)
    "parametricCurve.luma.whiteLevel",
    "parametricCurve.luma.highlights",
    "parametricCurve.luma.lights",
    "parametricCurve.luma.darks",
    "parametricCurve.luma.shadows",
    "parametricCurve.luma.blackLevel",
    "parametricCurve.luma.split1",
    "parametricCurve.luma.split2",
    "parametricCurve.luma.split3",
    "parametricCurve.red.whiteLevel",
    "parametricCurve.red.highlights",
    "parametricCurve.red.lights",
    "parametricCurve.red.darks",
    "parametricCurve.red.shadows",
    "parametricCurve.red.blackLevel",
    "parametricCurve.red.split1",
    "parametricCurve.red.split2",
    "parametricCurve.red.split3",
    "parametricCurve.green.whiteLevel",
    "parametricCurve.green.highlights",
    "parametricCurve.green.lights",
    "parametricCurve.green.darks",
    "parametricCurve.green.shadows",
    "parametricCurve.green.blackLevel",
    "parametricCurve.green.split1",
    "parametricCurve.green.split2",
    "parametricCurve.green.split3",
    "parametricCurve.blue.whiteLevel",
    "parametricCurve.blue.highlights",
    "parametricCurve.blue.lights",
    "parametricCurve.blue.darks",
    "parametricCurve.blue.shadows",
    "parametricCurve.blue.blackLevel",
    "parametricCurve.blue.split1",
    "parametricCurve.blue.split2",
    "parametricCurve.blue.split3",
    // Color
    "temperature",
    "tint",
    "vibrance",
    "saturation",
    "hue",
    "colorGrading.global.hue",
    "colorGrading.global.saturation",
    "colorGrading.global.luminance",
    "colorGrading.shadows.hue",
    "colorGrading.shadows.saturation",
    "colorGrading.shadows.luminance",
    "colorGrading.midtones.hue",
    "colorGrading.midtones.saturation",
    "colorGrading.midtones.luminance",
    "colorGrading.highlights.hue",
    "colorGrading.highlights.saturation",
    "colorGrading.highlights.luminance",
    "colorGrading.blending",
    "colorGrading.balance",
    "hsl.reds.hue",
    "hsl.reds.saturation",
    "hsl.reds.luminance",
    "hsl.oranges.hue",
    "hsl.oranges.saturation",
    "hsl.oranges.luminance",
    "hsl.yellows.hue",
    "hsl.yellows.saturation",
    "hsl.yellows.luminance",
    "hsl.greens.hue",
    "hsl.greens.saturation",
    "hsl.greens.luminance",
    "hsl.aquas.hue",
    "hsl.aquas.saturation",
    "hsl.aquas.luminance",
    "hsl.blues.hue",
    "hsl.blues.saturation",
    "hsl.blues.luminance",
    "hsl.purples.hue",
    "hsl.purples.saturation",
    "hsl.purples.luminance",
    "hsl.magentas.hue",
    "hsl.magentas.saturation",
    "hsl.magentas.luminance",
    "colorCalibration.redHue",
    "colorCalibration.redSaturation",
    "colorCalibration.greenHue",
    "colorCalibration.greenSaturation",
    "colorCalibration.blueHue",
    "colorCalibration.blueSaturation",
    "colorCalibration.shadowsTint",
    // Details
    "sharpness",
    "sharpnessThreshold",
    "clarity",
    "dehaze",
    "structure",
    "lumaNoiseReduction",
    "colorNoiseReduction",
    "chromaticAberrationRedCyan",
    "chromaticAberrationBlueYellow",
    // Effects
    "glowAmount",
    "halationAmount",
    "flareAmount",
    "lensBlurAmount",
    "lensBlurDiffusion",
    "lensBlurMinDepth",
    "lensBlurMaxDepth",
    "lensBlurMinFade",
    "lensBlurMaxFade",
    "vignetteAmount",
    "vignetteMidpoint",
    "vignetteRoundness",
    "vignetteFeather",
    "grainAmount",
    "grainSize",
    "grainRoughness",
];

/// Sort a set of dotted ramp-parameter paths into RapidRAW's own panel
/// order. Anything not in `RAPIDRAW_PARAM_ORDER` (a future RapidRAW field
/// this tool doesn't know about yet) sorts alphabetically after everything
/// that is, rather than being dropped.
pub fn sort_params_rapidraw_order(mut params: Vec<String>) -> Vec<String> {
    let rank = |p: &str| RAPIDRAW_PARAM_ORDER.iter().position(|&k| k == p).unwrap_or(usize::MAX);
    params.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    params
}
