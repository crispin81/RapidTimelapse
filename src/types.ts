export interface FrameInfo {
  index: number;
  file: string;
  rating: number;
  hasSidecar: boolean;
  hasAdjustments: boolean;
  thumbnail: string | null;
  params: Record<string, number>;
}

export interface ScanResult {
  frames: FrameInfo[];
  suggestedKeyframes: number[];
  rampParams: string[];
}

export interface PlannedFrameOut {
  index: number;
  isKeyframe: boolean;
  params: Record<string, number> | null;
  deflickerStops: number | null;
}

export interface DeflickerOut {
  measured: Record<string, number>;
  target: Record<string, number>;
}

export interface PlanResult {
  frames: PlannedFrameOut[];
  deflicker: DeflickerOut | null;
}

export interface WriteResult {
  writtenCount: number;
  backedUpCount: number;
  keyframesRatedCount: number;
}

export interface RevertResult {
  restoredCount: number;
}
