import { useEffect, useRef } from "react";
import type { FrameInfo, PlanResult } from "../types";

interface Props {
  frames: FrameInfo[];
  plan: PlanResult | null;
  keyframeIndices: Set<number>;
  selectedParam: string;
}

interface Point {
  index: number;
  value: number;
  isKeyframe: boolean;
}

export default function CurveChart({ frames, plan, keyframeIndices, selectedParam }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const draw = () => {
      drawChart(canvas, frames, plan, keyframeIndices, selectedParam);
    };

    draw();

    const observer = new ResizeObserver(draw);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [frames, plan, keyframeIndices, selectedParam]);

  return <canvas ref={canvasRef} className="curve-canvas" />;
}

function drawChart(
  canvas: HTMLCanvasElement,
  frames: FrameInfo[],
  plan: PlanResult | null,
  keyframeIndices: Set<number>,
  selectedParam: string,
) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const w = rect.width;
  const h = rect.height;

  const style = getComputedStyle(document.documentElement);
  const bg = style.getPropertyValue("--panel-bg").trim() || "#1b1e22";
  const grid = style.getPropertyValue("--grid-line").trim() || "#2c3036";
  const line = style.getPropertyValue("--accent").trim() || "#e8a33d";
  const kfDot = style.getPropertyValue("--kf-dot").trim() || "#ffffff";
  const text = style.getPropertyValue("--text-dim").trim() || "#8a8f98";

  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, w, h);

  if (frames.length === 0 || !selectedParam) {
    ctx.fillStyle = text;
    ctx.font = "13px system-ui, sans-serif";
    ctx.fillText("Scan a folder to see the keyframe curve", 16, h / 2);
    return;
  }

  const points: Point[] = [];
  for (const f of frames) {
    const isKf = keyframeIndices.has(f.index);
    let value: number | undefined;
    if (isKf) {
      value = f.params[selectedParam];
    } else {
      const planned = plan?.frames.find((pf) => pf.index === f.index);
      value = planned?.params?.[selectedParam] ?? undefined;
    }
    if (value !== undefined) points.push({ index: f.index, value, isKeyframe: isKf });
  }

  const padding = { left: 48, right: 16, top: 16, bottom: 24 };
  const plotW = Math.max(1, w - padding.left - padding.right);
  const plotH = Math.max(1, h - padding.top - padding.bottom);
  const maxIndex = Math.max(1, frames.length - 1);

  if (points.length === 0) {
    ctx.fillStyle = text;
    ctx.font = "13px system-ui, sans-serif";
    ctx.fillText(`No values yet for "${selectedParam}" — rate keyframes and compute a ramp.`, 16, h / 2);
    return;
  }

  let minV = Math.min(...points.map((p) => p.value));
  let maxV = Math.max(...points.map((p) => p.value));
  if (minV === maxV) {
    minV -= 1;
    maxV += 1;
  }
  const vSpan = maxV - minV;
  const vPad = vSpan * 0.12;
  minV -= vPad;
  maxV += vPad;

  const xAt = (index: number) => padding.left + (index / maxIndex) * plotW;
  const yAt = (value: number) => padding.top + plotH - ((value - minV) / (maxV - minV)) * plotH;

  // grid
  ctx.strokeStyle = grid;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let g = 0; g <= 4; g++) {
    const y = padding.top + (plotH * g) / 4;
    ctx.moveTo(padding.left, y);
    ctx.lineTo(padding.left + plotW, y);
  }
  ctx.stroke();

  ctx.fillStyle = text;
  ctx.font = "11px system-ui, sans-serif";
  ctx.textAlign = "right";
  for (let g = 0; g <= 4; g++) {
    const v = maxV - (vSpan + 2 * vPad) * (g / 4);
    const y = padding.top + (plotH * g) / 4;
    ctx.fillText(v.toFixed(2), padding.left - 8, y + 3);
  }
  ctx.textAlign = "left";

  // line through all known points, in frame order
  ctx.strokeStyle = line;
  ctx.lineWidth = 2;
  ctx.beginPath();
  points.forEach((p, i) => {
    const x = xAt(p.index);
    const y = yAt(p.value);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();

  // keyframe markers
  for (const p of points) {
    const x = xAt(p.index);
    const y = yAt(p.value);
    if (p.isKeyframe) {
      ctx.beginPath();
      ctx.arc(x, y, 5, 0, Math.PI * 2);
      ctx.fillStyle = kfDot;
      ctx.fill();
      ctx.strokeStyle = line;
      ctx.lineWidth = 2;
      ctx.stroke();
    } else {
      ctx.beginPath();
      ctx.arc(x, y, 2, 0, Math.PI * 2);
      ctx.fillStyle = line;
      ctx.fill();
    }
  }
}
