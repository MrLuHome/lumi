export type { AdjustmentKey, EditParams, ParamDefinition, ParamGroup } from "./state/params";

/** 与 Rust `ImageItem` 对应。 */
export interface PhotoAsset {
  id: string;
  path: string;
  fileName: string;
  format: string;
  width: number;
  height: number;
  fileSize: number;
  hasAlpha: boolean;
  thumbUrl: string;
  previewUrl: string;
  previewWidth: number;
  previewHeight: number;
}
