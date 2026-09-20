import { invoke } from "@tauri-apps/api/core";
import type { PlanResult, RevertResult, ScanResult, WriteResult } from "./types";

export function browseFolder(): Promise<string | null> {
  return invoke("browse_folder");
}

export function scanFolder(folder: string): Promise<ScanResult> {
  return invoke("scan_folder", { folder });
}

export interface RampSettings {
  smoothing: number;
  deflickerEnabled: boolean;
  deflickerMaxStops: number;
  deflickerWindow: number;
}

export function computePlan(folder: string, keyframeIndices: number[], settings: RampSettings): Promise<PlanResult> {
  return invoke("compute_plan", {
    folder,
    keyframeIndices,
    smoothing: settings.smoothing,
    deflickerEnabled: settings.deflickerEnabled,
    deflickerMaxStops: settings.deflickerMaxStops,
    deflickerWindow: settings.deflickerWindow,
  });
}

export function writePlan(
  folder: string,
  keyframeIndices: number[],
  settings: RampSettings,
  backup: boolean,
): Promise<WriteResult> {
  return invoke("write_plan", {
    folder,
    keyframeIndices,
    smoothing: settings.smoothing,
    deflickerEnabled: settings.deflickerEnabled,
    deflickerMaxStops: settings.deflickerMaxStops,
    deflickerWindow: settings.deflickerWindow,
    backup,
  });
}

export function revertFolder(folder: string): Promise<RevertResult> {
  return invoke("revert_folder", { folder });
}
