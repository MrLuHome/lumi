export type {
  AdjustmentKey,
  CurveChannel,
  CurvePoint,
  CurveSet,
  EditParams,
  ExportFormat,
  ExportSettings,
  HslChannel,
  HslKey,
  HslParams,
  LutSettings,
  ParamDefinition,
  ParamGroup,
} from "./state/params";

/** 与 Rust `ImageItem` 对应。 */
export interface PhotoAsset {
  id: string;
  path: string;
  fileName: string;
  format: string;
  bitDepth: number;
  width: number;
  height: number;
  fileSize: number;
  hasAlpha: boolean;
  thumbUrl: string;
  previewUrl: string;
  previewWidth: number;
  previewHeight: number;
}

/** 与 Rust `RenderResult` 对应。 */
export interface RenderResult {
  requestId: number;
  previewUrl: string;
  width: number;
  height: number;
  elapsedMs: number;
  computed: boolean;
}

export interface RenderProgress {
  requestId: number;
  stage: string;
  progress: number;
}

/** 与 Rust `FaceModelStatus` 对应。 */
export interface FaceModelStatus {
  available: boolean;
  loaded: boolean;
  name: string;
  version: string;
  license: string;
  source: string;
  sha256: string;
  input: string;
  outputs: string;
  path: string | null;
  error: string | null;
}

export interface FacePoint {
  x: number;
  y: number;
}

/** 与 Rust `DetectedFace` 对应，坐标为原图像素。 */
export interface DetectedFace {
  id: number;
  score: number;
  x: number;
  y: number;
  width: number;
  height: number;
  landmarks: FacePoint[];
}

export interface FaceDetectionResult {
  faces: DetectedFace[];
  imageWidth: number;
  imageHeight: number;
  elapsedMs: number;
  inferenceMs: number;
  model: string;
}

export interface FaceProgress {
  stage: string;
  progress: number;
}

/** 与 Rust `ProjectDoc` / `ProjectPhoto` 对应。 */
export interface ProjectPhoto {
  path: string;
  params: import("./state/params").EditParams;
  masks: unknown[];
  export: import("./state/params").ExportSettings | null;
}

export interface ProjectDoc {
  version: number;
  createdAt: string;
  activeIndex: number;
  export: import("./state/params").ExportSettings;
  photos: ProjectPhoto[];
}

export interface ProjectOpenResult {
  path: string;
  project: ProjectDoc;
}

export interface ExportResult {
  path: string;
  bytes: number;
  width: number;
  height: number;
  format: string;
  elapsedMs: number;
}
