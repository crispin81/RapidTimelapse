# RapidTimelapse - Companion for RapidRAW

An LRTimelapse-style keyframe ramping companion for
[RapidRAW](https://github.com/CyberTimon/RapidRAW). A native desktop app
(Tauri: Rust backend, React/TypeScript UI) — no separate runtime to install.

Edit a handful of turning-point frames in RapidRAW, rate each one 5 stars,
then use this app to smoothly interpolate every setting across the frames in
between and write the result straight to their `.rrdata` sidecars. Refresh
the folder in RapidRAW and export as normal.

Free and open-source, licensed [AGPL-3.0](LICENSE).

## Installing

Grab the latest build for your OS from
[Releases](https://github.com/crispin81/RapidTimelapse/releases).

**These builds aren't code-signed** (that needs a paid developer
certificate this project doesn't have yet), so your OS will warn that the
publisher is unverified on first launch. That's expected for unsigned
beta software, not a sign anything's wrong:

- **macOS**: [Direct download (.dmg, Apple Silicon + Intel)](https://github.com/crispin81/RapidTimelapse/releases/download/v1.0.0-beta/RapidTimelapse_1.0.0_universal.dmg).
  Open the `.dmg`, drag `RapidTimelapse.app` into Applications.
  Gatekeeper will refuse to open it the first time — either right-click
  (Control-click) the app and choose **Open**, then confirm, or run this
  once in Terminal:
  ```
  xattr -d com.apple.quarantine /Applications/RapidTimelapse.app
  ```
- **Windows**: [Direct download (.msi installer)](https://github.com/crispin81/RapidTimelapse/releases/download/v1.0.0-beta/RapidTimelapse_1.0.0_x64_en-US.msi).
  Run the `.msi` or `.exe`. SmartScreen will show "Windows
  protected your PC" — click **More info**, then **Run anyway**.
- **Linux**: [Direct download (.AppImage)](https://github.com/crispin81/RapidTimelapse/releases/download/v1.0.0-beta/RapidTimelapse_1.0.0_amd64.AppImage).
  `chmod +x` the `.AppImage` and run it directly (works on
  Debian, Arch, Fedora and most others), or install the `.deb`/`.rpm` for
  your distro. If the window opens blank, see
  [Linux: blank/gray window](#linux-blankgray-window) below.

## Workflow

1. **Import the whole sequence into RapidRAW**, select all, press <kbd>0</kbd>
   (or make any tiny edit) so every frame gets a `.rrdata` sidecar.
2. **Edit your turning points** — first frame, last frame, and anywhere the
   light changes direction — and **rate each one 5 stars**. A keyframe needs
   a real edit, not just a rating: RapidRAW only writes adjustment data once
   a slider actually moves (confirmed on 1.6.4 — a bare rating with nothing
   else touched can still leave `adjustments: null`). The app will warn you
   in the toolbar if a 5-star frame has no edits yet.
3. **Open this app, Browse… to the folder, Scan Folder.** 5-star frames are
   auto-selected as keyframes (toggle any frame's ☆ in the filmstrip to
   add/remove one). Adding a star here doesn't touch the file yet — see step 5.
4. **Adjust Smoothing** and watch the keyframe curve update live. Optionally
   enable **Deflicker** to flatten frame-to-frame brightness noise on top of
   the ramp (measured from each frame's own embedded preview).
5. **Write Sidecars.** Every non-keyframe frame's `.rrdata` is regenerated
   from the ramp; a keyframe's own `adjustments` (its real edit) is never
   touched. If you starred a frame in this app's filmstrip that wasn't
   already 5-star on disk, its `rating` is stamped to 5 at write time too
   (its edit still untouched) — so RapidRAW and the next scan agree it's a
   keyframe without you re-clicking it every session. A `.rrdata.bak` is
   kept per frame the first time it's touched, so **Revert** always restores
   your real pre-ramp state, however many times you re-run the ramp in
   between.

## What's interpolated vs. carried over

Interpolated: exposure, contrast, highlights/shadows/whites/blacks,
temperature/tint, vibrance/saturation, clarity/dehaze/structure, sharpness +
noise reduction, vignette, grain, glow/halation/flare/lens-blur effect,
the full 8-band HSL mixer, color grading wheels, color calibration, the
parametric tone curve, and point/curve tone curves (when both bracketing
keyframes have the same curve shape).

Carried from the nearest keyframe rather than blended — geometry and choices
don't have a meaningful "halfway" value: crop, transform/rotation/flip,
lens-correction profile and toggles, LUT choice, tone mapper, orientation.

**Masks track when copied, not when redrawn.** A mask is only ever carried
wholesale like the above, *unless* it can be matched to a mask with the same
ID on both bracketing keyframes — which happens when you create it on one
keyframe and then copy/duplicate it (not redraw it from scratch) onto the
others. A matched radial or linear gradient has its position, size and
rotation smoothly interpolated across the frames in between (its own local
exposure/contrast/etc. and opacity ramp too), so e.g. a radial gradient
isolating the Milky Way, repositioned and rotated by hand on each keyframe
to track its movement across the sky, follows a smooth path through every
frame rather than jumping. Non-geometric mask shapes (brush, AI subject/sky,
quick eraser) can't be tracked this way and are always carried as-is.

See `src-tauri/src/params.rs` for the exact field lists — the engine walks
`adjustments` generically (ramp every number it finds except a short
blacklist) so it stays correct if RapidRAW adds new sliders later.

## Building / running

Requires Node 18+ and a Rust toolchain (see [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/)).

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce a native installer for the current OS
```

`tauri build` produces a `.AppImage`/`.deb` on Linux, `.dmg`/`.app` on macOS,
and `.msi`/`.exe` on Windows — run it on each target OS (or via CI) to get
that platform's installer; Tauri doesn't cross-compile installers from one
OS to another.

### Linux: blank/gray window

If the window opens but renders blank, WebKitGTK's GPU compositing path is
failing on your graphics stack (seen as `Failed to create GBM buffer...` in
the terminal). Force software rendering:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 npm run tauri dev
```

For a built `.AppImage`/binary, set the same two environment variables
before launching it, or bake them into a wrapper script / the `.desktop`
launcher's `Exec` line.

## Safety

- A keyframe's `adjustments` (its actual edit) are never modified by this
  app. The one exception: if you marked it a keyframe by hand and it wasn't
  already 5-star on disk, Write Sidecars stamps its `rating` to 5 so it's
  remembered — nothing else in the file changes.
- Every sidecar this app writes is backed up to `<file>.rrdata.bak` the
  first time it's touched (never overwritten on later runs), so **Revert**
  always gets you back to your real, pre-ramp edits.
- RAW files are never touched — only `.rrdata` sidecars, exactly like
  RapidRAW's own non-destructive workflow.

## Known limits

- **Frame order is filename order** (natural/numeric-aware sort), which
  matches in-camera numbering almost always but isn't capture-time-aware.
- **Tone curves with differing point counts or x-positions between the two
  bracketing keyframes** fall back to carrying the nearest keyframe's curve
  rather than guessing at a blend.
- **No RAW demosaic** — filmstrip thumbnails and deflicker brightness both
  come from each RAW file's embedded JPEG preview (found by scanning for
  JPEG markers, format-agnostic), not RapidRAW's real GPU pipeline. Good
  enough to browse and measure exposure by; not a substitute for RapidRAW's
  own preview.

## Layout

```
src-tauri/src/
  rrdata.rs       read/write .rrdata sidecars; unknown fields pass through untouched
  sequence.rs     scan a folder, natural-sort frames, pair with sidecars
  params.rs       which adjustment fields ramp vs. carry over; the JSON leaf walker
  interpolate.rs  the ramp engine: keyframes -> full per-frame plan
  deflicker.rs    measured brightness -> per-frame exposure correction
  preview.rs      embedded-JPEG extraction for thumbnails + brightness measurement
  commands.rs     Tauri commands exposed to the UI
src/               React/TypeScript UI (filmstrip, keyframe curve, data table)
```
