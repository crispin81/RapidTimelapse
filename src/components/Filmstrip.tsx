import type { FrameInfo, PlanResult } from "../types";

interface Props {
  frames: FrameInfo[];
  keyframeIndices: Set<number>;
  plan: PlanResult | null;
  selectedIndex: number | null;
  onSelect: (index: number) => void;
  onToggleKeyframe: (index: number) => void;
}

export default function Filmstrip({ frames, keyframeIndices, plan, selectedIndex, onSelect, onToggleKeyframe }: Props) {
  return (
    <div id="frameList">
      {frames.map((f) => {
        const isKf = keyframeIndices.has(f.index);
        const planned = plan?.frames.find((pf) => pf.index === f.index);
        const willWrite = planned && !planned.isKeyframe && planned.params;
        return (
          <div
            key={f.index}
            className={`frame-row ${isKf ? "is-keyframe" : ""} ${selectedIndex === f.index ? "is-selected" : ""}`}
            onClick={() => onSelect(f.index)}
          >
            <div className="frame-thumb">
              {f.thumbnail ? <img src={f.thumbnail} alt={f.file} /> : <div className="frame-thumb-placeholder" />}
              {willWrite && <span className="ramp-badge" title="Will be written by the next ramp" />}
            </div>
            <div className="frame-meta">
              <span className="frame-name">{f.file}</span>
              <span className="frame-sub">
                {!f.hasSidecar && "no sidecar"}
                {f.hasSidecar && !f.hasAdjustments && "imported, no edits"}
                {f.hasAdjustments && `rating ${f.rating}`}
              </span>
            </div>
            <button
              type="button"
              className={`star-toggle ${isKf ? "active" : ""}`}
              title={isKf ? "Remove as keyframe" : "Mark as keyframe"}
              onClick={(e) => {
                e.stopPropagation();
                onToggleKeyframe(f.index);
              }}
            >
              {isKf ? "★" : "☆"}
            </button>
          </div>
        );
      })}
    </div>
  );
}
