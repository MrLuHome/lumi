export type {
  AdjustmentKey,
  BackgroundFit,
  BackgroundGradient,
  BackgroundMode,
  BackgroundParams,
  BeautyKey,
  BeautyParams,
  BodyAdjust,
  BodyAnchor,
  BodyAnchorPart,
  BodyShape,
  BodyShapeKey,
  CurveChannel,
  CurvePoint,
  CurveSet,
  EditParams,
  ExportFormat,
  ExportSettings,
  FaceBeauty,
  FaceReshape,
  FaceShape,
  FaceShapeKey,
  GradientStop,
  ManualWarp,
  WarpMode,
  WarpStroke,
  HslChannel,
  HslKey,
  HslParams,
  LutSettings,
  ManualMask,
  MaskPoint,
  MaskStroke,
  ParamDefinition,
  ParamGroup,
  RepairPoint,
  SubjectMaskParams,
  SubjectMaskStroke,
  SubjectMaskStrokeKind,
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
  reshape: FaceReshapeReport[];
  /** 第五轮身材处理报告。 */
  body: BodyReport[];
  /** 第五轮背景处理报告。 */
  background: BackgroundReport | null;
  /** 第五轮手动蒙版报告。 */
  mask: MaskReport | null;
  warnings: string[];
}

/** 与 Rust `BodyReport` 对应。 */
export interface BodyReport {
  subjectId: string;
  displacementMaxRatio: number;
  foldedCells: number;
  affectedPixels: number;
  degraded: boolean;
  elapsedMs: number;
  notes: string[];
}

/** 与 Rust `BackgroundReport` 对应。 */
export interface BackgroundReport {
  mode: string;
  maskedPixels: number;
  elapsedMs: number;
  degraded: boolean;
  notes: string[];
}

/** 与 Rust `MaskReport` 对应。 */
export interface MaskReport {
  source: string;
  modelVersion: string;
  coverage: number;
  edgeRatio: number;
  strokes: number;
  elapsedMs: number;
}

/** 与 Rust `SegmentationCapability` 对应：AI 分割是否可用及原因。 */
export interface SegmentationCapability {
  aiAvailable: boolean;
  manualAvailable: boolean;
  providers: MaskProviderInfo[];
  note: string;
}

export interface MaskProviderInfo {
  id: string;
  version: string;
  license: string;
  available: boolean;
  reason: string;
}

/** 与 Rust `SubjectMaskPreview` 对应。 */
export interface SubjectMaskPreview {
  url: string;
  width: number;
  height: number;
  coverage: number;
  edgeRatio: number;
  source: string;
  modelVersion: string;
  strokes: number;
  note: string;
}

/** 与 Rust `PoseCapability` 对应：姿态/自动身材是否可用及原因。 */
export interface BodyCapability {
  poseAvailable: boolean;
  provider: string;
  license: string;
  reason: string;
  note: string;
}

/** 与 Rust `BodyAnchorPreset` 对应：从手动蒙版轮廓得到的锚点建议位置。 */
export interface BodyAnchorPreset {
  part: import("./state/params").BodyAnchorPart;
  x: number;
  y: number;
}

/** 与 Rust `BackgroundImageInfo` 对应。 */
export interface BackgroundImageInfo {
  path: string;
  name: string;
  width: number;
  height: number;
}

/** 与 Rust `FaceReshapeReport` 对应：塑形质量与自动降强度结果。 */
export interface FaceReshapeReport {
  faceId: number;
  source: string;
  denseLandmarks: boolean;
  confidence: number;
  autoScale: number;
  controlPoints: number;
  triangles: number;
  flippedTriangles: number;
  minAreaRatio: number;
  maxAreaRatio: number;
  maxDisplacementRatio: number;
  appliedScale: number;
  degraded: boolean;
  meshVersion: number;
  warpedPixels: number;
  elapsedMs: number;
  notes: string[];
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
  /** 稳定人脸 UUID，由跟踪器或项目恢复分配。 */
  uuid: string;
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

/** 与 Rust `SkinModelStatus` 对应：当前是否加载了可商用皮肤分割模型。 */
export interface SkinModelStatus {
  available: boolean;
  method: string;
  name: string;
  version: string;
  license: string;
  commercialUse: boolean;
  sha256: string;
  input: string;
  outputs: string;
  path: string | null;
  reason: string;
  registeredCount: number;
}

/** 与 Rust `FaceMetrics` 对应。 */
export interface FaceMetrics {
  faceId: number;
  skinRatio: number;
  meanLuma: number;
  texture: number;
  blemishDensity: number;
  chromaVariation: number;
  redness: number;
  darkRatio: number;
  highlightRatio: number;
  darkCircleDelta: number;
  eyeTexture: number;
  foreheadTexture: number;
  nasolabialTexture: number;
}

/** 与 Rust `FaceSuggestion` 对应。 */
export interface FaceSuggestion {
  faceId: number;
  metrics: FaceMetrics;
  suggested: import("./state/params").BeautyParams;
  notes: string[];
}

/** 与 Rust `BeautyAnalysis` 对应。 */
export interface BeautyAnalysis {
  faces: FaceSuggestion[];
  method: string;
  model: string;
  elapsedMs: number;
  skinModel: SkinModelStatus;
  warnings: string[];
}

/** 与 Rust `MaskPreview` 对应。 */
export interface MaskPreview {
  url: string;
  width: number;
  height: number;
  skinRatio: number;
  faces: number;
  method: string;
}

export interface BeautyProgress {
  stage: string;
  progress: number;
}

/** 与 Rust `ProjectFace` 对应：项目持久化的人脸身份记录。 */
export interface ProjectFaceRecord {
  uuid: string;
  bbox: { x: number; y: number; width: number; height: number };
  landmarks: FacePoint[];
  score: number;
  model: string;
  confidence: number;
  maskRef: string | null;
}

export interface FaceMatchEntry {
  recordIndex: number;
  faceId: number;
  faceUuid: string;
  score: number;
  confidence: "high" | "low";
}

/** 与 Rust `FaceRestoreResult` 对应。 */
export interface FaceRestoreResult {
  restored: FaceMatchEntry[];
  pending: FaceMatchEntry[];
  missing: ProjectFaceRecord[];
  sourceChanged: boolean;
  faces: DetectedFace[];
}

/** 与 Rust `ReshapeCapability` 对应。 */
export interface ReshapeCapability {
  source: string;
  denseLandmarks: boolean;
  supported: string[];
  disabled: { key: string; reason: string }[];
  note: string;
}

/** 与 Rust `MeshPreview` 对应，点坐标为原图像素，用于画布网格显示。 */
export interface MeshPreview {
  points: [number, number][];
  triangles: [number, number, number][];
  denseLandmarks: boolean;
  source: string;
  version: number;
}

/** 与 Rust `ProjectSubject` 对应：第五轮人物身材记录。 */
export interface ProjectSubject {
  subjectId: string;
  maskSource: string;
  maskModelVersion: string;
  maskStrokes: number;
  body: import("./state/params").BodyShape;
  anchors: number;
}

/** 与 Rust `MaskMeta` 对应：蒙版来源与版本，不保存 Alpha 像素。 */
export interface MaskMeta {
  source: string;
  modelId: string;
  modelVersion: string;
  coverage: number;
  strokeHash: string;
  revision: number;
}

/** 与 Rust `ProjectDoc` / `ProjectPhoto` 对应。 */
export interface ProjectPhoto {
  path: string;
  params: import("./state/params").EditParams;
  masks: unknown[];
  export: import("./state/params").ExportSettings | null;
  sourceHash: string;
  faces: ProjectFaceRecord[];
  subjects: ProjectSubject[];
  maskMeta: MaskMeta | null;
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

/* ------------------------- 第六轮：智能精修 / 预设 / 批量 ------------------------- */

export type Workspace = "edit" | "smart" | "presets" | "batch";

export type RetouchMode = "natural" | "light" | "standard" | "strong";

export type MetricStatus = "ok" | "lowConfidence" | "noData" | "unsupported" | "failed";

/** 与 Rust `AnalysisMetric` 对应。value 为 null 表示没有可靠数据。 */
export interface AnalysisMetric {
  key: string;
  label: string;
  value: number | null;
  unit: string;
  status: MetricStatus;
  detail: string;
}

/** 与 Rust `RetouchSuggestion` 对应，target 区分普通参数与按人脸美肤参数。 */
export interface RetouchSuggestion {
  id: string;
  group: string;
  label: string;
  target: "param" | "faceBeauty";
  field: string;
  current: number;
  suggested: number;
  reason: string;
  confidence: number;
  applicable: boolean;
}

export interface AnalysisCapabilities {
  faceDetection: boolean;
  beautyAnalysis: boolean;
  manualMask: boolean;
  background: boolean;
}

export interface AnalysisReport {
  imageHash: string;
  algorithmVersion: string;
  resultVersion: number;
  analyzedAtMs: number;
  mode: RetouchMode;
  faces: number;
  metrics: AnalysisMetric[];
  suggestions: RetouchSuggestion[];
  warnings: string[];
  capabilities: AnalysisCapabilities;
}

export interface AnalysisResponse {
  report: AnalysisReport;
  cached: boolean;
  /** true 表示输入或算法版本已变化，本次重新计算，旧结果可能过期。 */
  stale: boolean;
  elapsedMs: number;
}

export interface AppliedSuggestions {
  params: import("./state/params").EditParams;
  warnings: string[];
}

export type PresetCategory = "portraitNatural" | "skinTone" | "color" | "mood" | "custom";

export type PresetSource = "builtin" | "custom";

/** 预设参数子集：只含调色与美肤模板，不含任何对象级数据。 */
export interface PresetParams {
  exposure?: number;
  contrast?: number;
  highlights?: number;
  shadows?: number;
  whites?: number;
  blacks?: number;
  temperature?: number;
  tint?: number;
  saturation?: number;
  vibrance?: number;
  clarity?: number;
  sharpness?: number;
  grain?: number;
  vignette?: number;
  hsl: import("./state/params").HslParams;
  curves: import("./state/params").CurveSet;
  lutName?: string;
  lutStrength?: number;
  beauty: Record<string, number>;
}

export interface RetouchPreset {
  id: string;
  name: string;
  category: PresetCategory;
  description: string;
  version: string;
  source: PresetSource;
  params: PresetParams;
  suitableFor: string;
  avoid: string;
  preview?: string;
}

export interface PresetList {
  presets: RetouchPreset[];
  schemaVersion: number;
}

export interface PresetPreviews {
  previews: { presetId: string; url: string; warnings: string[] }[];
  failed: string[];
}

export interface PresetSaved {
  preset: RetouchPreset;
}

export type BatchStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "skipped";

export interface BatchTask {
  jobId: string;
  path: string;
  name: string;
  number: number;
  status: BatchStatus;
  stage: string;
  progress: number;
  outputPath: string | null;
  error: string | null;
  startedAtMs: number;
  finishedAtMs: number;
  bytes: number;
  width: number;
  height: number;
}

export interface BatchCurrent {
  jobId: string;
  name: string;
  index: number;
  stage: string;
  progress: number;
}

export interface BatchSnapshot {
  tasks: BatchTask[];
  running: boolean;
  paused: boolean;
  total: number;
  completed: number;
  failed: number;
  skipped: number;
  cancelled: number;
  pending: number;
  exported: number;
  current: BatchCurrent | null;
  overall: number;
  elapsedMs: number;
  etaMs: number | null;
}

export type ConflictPolicy = "skip" | "rename" | "overwrite";

export interface BatchExportRequest {
  photos: { path: string; params: import("./state/params").EditParams }[];
  settings: import("./state/params").ExportSettings;
  outputDir: string;
  naming: string;
  startIndex: number;
  conflict: ConflictPolicy;
  overwriteConfirmed: boolean;
}
