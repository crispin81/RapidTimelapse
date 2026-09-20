import type { FrameInfo, PlanResult } from "../types";

interface Props {
  frames: FrameInfo[];
  keyframeIndices: Set<number>;
  plan: PlanResult | null;
  selectedIndex: number | null;
  columns: string[];
}

function fmt(v: number | undefined): string {
  if (v === undefined || v === null || Number.isNaN(v)) return "—";
  return v.toFixed(2);
}

export default function DataTable({ frames, keyframeIndices, plan, selectedIndex, columns }: Props) {
  const rows = frames.map((f) => {
    const isKf = keyframeIndices.has(f.index);
    const planned = plan?.frames.find((pf) => pf.index === f.index);
    return { frame: f, isKf, planned, values: isKf ? f.params : planned?.params ?? {} };
  });

  // A column with the exact same (displayed) value on every frame, keyframes
  // included, was never actually touched -- hide it rather than clutter the
  // table with dozens of untouched sliders sitting at their default.
  const visibleColumns = columns.filter((c) => {
    const seen = new Set(rows.map((r) => fmt(r.values[c])));
    return seen.size > 1;
  });
  const hiddenCount = columns.length - visibleColumns.length;

  return (
    <>
      {hiddenCount > 0 && (
        <div className="hidden-columns-note">
          {hiddenCount} untouched column{hiddenCount === 1 ? "" : "s"} hidden
        </div>
      )}
      <div className="data-table-scroll">
        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>File</th>
              <th>★</th>
              {visibleColumns.map((c) => (
                <th key={c}>{c}</th>
              ))}
              <th>deflicker</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ frame: f, isKf, planned, values }) => (
              <tr key={f.index} className={`${isKf ? "is-keyframe" : ""} ${selectedIndex === f.index ? "is-selected" : ""}`}>
                <td>{f.index}</td>
                <td className="file-cell">{f.file}</td>
                <td>{isKf ? "★" : ""}</td>
                {visibleColumns.map((c) => (
                  <td key={c}>{fmt(values[c])}</td>
                ))}
                <td>{planned?.deflickerStops !== null && planned?.deflickerStops !== undefined ? `${planned.deflickerStops >= 0 ? "+" : ""}${planned.deflickerStops.toFixed(2)} EV` : ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
