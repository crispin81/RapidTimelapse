import { useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";
import { browseFolder, computePlan, revertFolder, scanFolder, writePlan, type RampSettings } from "./api";
import type { FrameInfo, PlanResult } from "./types";
import Filmstrip from "./components/Filmstrip";
import CurveChart from "./components/CurveChart";
import DataTable from "./components/DataTable";

const DEFAULT_PARAMS = ["exposure", "contrast", "highlights", "shadows", "whites", "blacks", "temperature", "tint", "vibrance", "saturation", "clarity", "dehaze"];

export default function App() {
  const [folder, setFolder] = useState<string | null>(null);
  const [frames, setFrames] = useState<FrameInfo[]>([]);
  const [rampParams, setRampParams] = useState<string[]>([]);
  const [keyframeIndices, setKeyframeIndices] = useState<Set<number>>(new Set());
  const [selectedParam, setSelectedParam] = useState("exposure");
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

  const [smoothing, setSmoothing] = useState(0.35);
  const [deflickerEnabled, setDeflickerEnabled] = useState(false);
  const [deflickerMaxStops, setDeflickerMaxStops] = useState(2);
  const [deflickerWindow, setDeflickerWindow] = useState(15);

  const [plan, setPlan] = useState<PlanResult | null>(null);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanProgress, setScanProgress] = useState<{ done: number; total: number } | null>(null);
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then((v) => {
      const [major, minor] = v.split(".");
      setVersion(`v${major}.${minor ?? 0}`);
    });
  }, []);

  const settings: RampSettings = useMemo(
    () => ({ smoothing, deflickerEnabled, deflickerMaxStops, deflickerWindow }),
    [smoothing, deflickerEnabled, deflickerMaxStops, deflickerWindow],
  );

  // Once scanned, rampParams (already ordered to match RapidRAW's own panel
  // layout by the backend) is authoritative; DEFAULT_PARAMS is just a
  // placeholder for the dropdown before a folder's been scanned.
  const paramOptions = rampParams.length > 0 ? rampParams : DEFAULT_PARAMS;

  async function doBrowse() {
    try {
      const picked = await browseFolder();
      if (picked) {
        setFolder(picked);
        await doScan(picked);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function doScan(target?: string) {
    const f = target ?? folder;
    if (!f) return;
    setBusy(true);
    setError(null);
    setScanProgress({ done: 0, total: 0 });
    const unlisten = await listen<{ done: number; total: number }>("scan-progress", (e) => {
      setScanProgress(e.payload);
    });
    try {
      const result = await scanFolder(f);
      setFrames(result.frames);
      setRampParams(result.rampParams);
      setKeyframeIndices(new Set(result.suggestedKeyframes));
      setStatus(`${result.frames.length} frames, ${result.suggestedKeyframes.length} keyframes (5★)`);
    } catch (e) {
      setError(String(e));
    } finally {
      unlisten();
      setBusy(false);
      setScanProgress(null);
    }
  }

  // Live-recompute the ramp whenever inputs change, debounced.
  useEffect(() => {
    if (!folder || frames.length === 0 || keyframeIndices.size === 0) {
      setPlan(null);
      return;
    }
    const handle = setTimeout(async () => {
      try {
        const result = await computePlan(folder, Array.from(keyframeIndices), settings);
        setPlan(result);
      } catch (e) {
        setError(String(e));
      }
    }, 250);
    return () => clearTimeout(handle);
  }, [folder, frames.length, keyframeIndices, settings]);

  function toggleKeyframe(index: number) {
    setKeyframeIndices((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  async function doWrite() {
    if (!folder) return;
    setBusy(true);
    setError(null);
    try {
      const result = await writePlan(folder, Array.from(keyframeIndices), settings, true);
      const kfNote = result.keyframesRatedCount > 0 ? `, rated ${result.keyframesRatedCount} new keyframe(s) 5★` : "";
      setStatus(`Wrote ${result.writtenCount} sidecars (${result.backedUpCount} backed up)${kfNote}. Refresh the folder in RapidRAW.`);
      await doScan();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function doRevert() {
    if (!folder) return;
    if (!confirm("Restore every frame's .rrdata.bak, undoing all ramp writes in this folder?")) return;
    setBusy(true);
    setError(null);
    try {
      const result = await revertFolder(folder);
      setStatus(`Restored ${result.restoredCount} frames from backup.`);
      await doScan();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const selectedFrame = frames.find((f) => f.index === selectedIndex) ?? null;
  const keyframesWithoutEdits = frames.filter((f) => keyframeIndices.has(f.index) && !f.hasAdjustments);

  return (
    <div id="shell">
      <header id="toolbar">
        <div className="brand">
          Rapid<span>TIMELAPSE</span> <span className="brand-version">{version}</span>{" "}
          <span className="brand-beta">beta</span>
        </div>
        <input
          className="folder-input"
          type="text"
          value={folder ?? ""}
          placeholder="/path/to/sequence/folder"
          onChange={(e) => setFolder(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && doScan()}
        />
        <button type="button" onClick={doBrowse} disabled={busy}>
          Browse…
        </button>
        <button type="button" onClick={() => doScan()} disabled={busy || !folder}>
          Scan Folder
        </button>

        <label className="slider-label">
          Smoothing
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={smoothing}
            onChange={(e) => setSmoothing(Number(e.currentTarget.value))}
          />
          <span>{smoothing.toFixed(2)}</span>
        </label>

        <label className="checkbox-label">
          <input type="checkbox" checked={deflickerEnabled} onChange={(e) => setDeflickerEnabled(e.currentTarget.checked)} />
          Deflicker
        </label>
        {deflickerEnabled && (
          <>
            <label className="slider-label">
              Max EV
              <input
                type="range"
                min={0.2}
                max={4}
                step={0.1}
                value={deflickerMaxStops}
                onChange={(e) => setDeflickerMaxStops(Number(e.currentTarget.value))}
              />
              <span>{deflickerMaxStops.toFixed(1)}</span>
            </label>
            <label className="slider-label">
              Window
              <input
                type="range"
                min={3}
                max={61}
                step={2}
                value={deflickerWindow}
                onChange={(e) => setDeflickerWindow(Number(e.currentTarget.value))}
              />
              <span>{deflickerWindow}</span>
            </label>
          </>
        )}

        <button type="button" className="primary" onClick={doWrite} disabled={busy || !plan}>
          Write Sidecars
        </button>
        <button type="button" onClick={doRevert} disabled={busy || !folder}>
          Revert
        </button>
      </header>

      {scanProgress && (
        <div className="banner banner-progress">
          <span>
            {scanProgress.total > 0
              ? `Scanning frames… ${scanProgress.done} / ${scanProgress.total}`
              : "Scanning folder…"}
          </span>
          <div className="progress-track">
            <div
              className="progress-fill"
              style={{
                width: scanProgress.total > 0 ? `${(100 * scanProgress.done) / scanProgress.total}%` : "8%",
              }}
            />
          </div>
        </div>
      )}
      {error && <div className="banner banner-error">{error}</div>}
      {!error && keyframesWithoutEdits.length > 0 && (
        <div className="banner banner-warn">
          {keyframesWithoutEdits.length} keyframe(s) have no edits yet in RapidRAW ({keyframesWithoutEdits.map((f) => f.file).join(", ")}) —
          nudge a slider (even 0.01 and back) so RapidRAW writes real adjustment data for them.
        </div>
      )}
      {!error && status && <div className="banner banner-status">{status}</div>}

      <div id="main">
        <aside id="filmstripPanel">
          <a
            className="video-link"
            href="https://youtu.be/GPtzFxWJlyk"
            onClick={(e) => {
              e.preventDefault();
              openUrl("https://youtu.be/GPtzFxWJlyk");
            }}
            title="Watch the tutorial video on YouTube"
          >
            ▶ New user? Watch this first!
          </a>
          <button type="button" className="holy-grail-btn" disabled title="Day-to-night / night-to-day auto exposure blending — not implemented yet">
            ✨ Holy Grail Mode — coming soon
          </button>
          <div className="panel-title">
            Filmstrip <span className="panel-title-sub">{frames.length} frames</span>
          </div>
          <Filmstrip
            frames={frames}
            keyframeIndices={keyframeIndices}
            plan={plan}
            selectedIndex={selectedIndex}
            onSelect={setSelectedIndex}
            onToggleKeyframe={toggleKeyframe}
          />
        </aside>

        <section id="curvePanel">
          <div className="panel-title">
            Keyframe Curve
            <select value={selectedParam} onChange={(e) => setSelectedParam(e.currentTarget.value)}>
              {paramOptions.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </div>
          <CurveChart frames={frames} plan={plan} keyframeIndices={keyframeIndices} selectedParam={selectedParam} />
          <div className="hint">
            Click ☆ in the filmstrip to mark/unmark a keyframe. The ramp recomputes live as you adjust smoothing or keyframes.
            {selectedFrame && ` Selected: ${selectedFrame.file}`}
          </div>
        </section>

        <section id="dataPanel">
          <div className="panel-title">Per-frame Values</div>
          <DataTable frames={frames} keyframeIndices={keyframeIndices} plan={plan} selectedIndex={selectedIndex} columns={rampParams} />
        </section>
      </div>

      <footer id="statusBar">
        <span className="oss-note">RapidTIMELAPSE is free and open-source software, licensed AGPL-3.0. Developed and maintained by Chris Cork Photography.</span>
      </footer>
    </div>
  );
}
