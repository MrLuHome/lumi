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
  /** 只有通过质量验证的 Provider 才会填充；5 点结果永远没有该字段。 */
  dense?: DenseFace | null;
  /** 每张脸的 LEVEL 3 门控结论与原因。 */
  level3?: Level3Gate | null;
}

export interface DenseLandmark {
  x: number;
  y: number;
  confidence: number;
  semanticId: string;
}

export interface DenseRegion {
  kind:
    | "faceContour"
    | "leftEye"
    | "rightEye"
    | "leftEyebrow"
    | "rightEyebrow"
    | "nose"
    | "noseCrest"
    | "medianLine"
    | "outerLips"
    | "innerLips"
    | "iris"
    | "faceMesh"
    | "other";
  points: DenseLandmark[];
  closed: boolean;
}

export interface DenseFace {
  topologyId: string;
  providerId: string;
  providerVersion: string;
  imageWidth: number;
  imageHeight: number;
  boundingBox: { x: number; y: number; width: number; height: number };
  confidence: number;
  regions: DenseRegion[];
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
  /** R11 新增：被跳过的脸与原因（后端为空时会省略该字段）。 */
  warnings?: string[];
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
  /** `apply_identities` 之后由项目 UUID 推导的最终人脸 ID。 */
  finalFaceId?: number;
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
  /** 因 Provider/拓扑/模型版本/源图不一致而丢弃的旧稠密结论数量。 */
  discardedDense: number;
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
  /** 第七轮：来源、RAW 显影参数与色彩输出设置（旧项目可缺省）。 */
  source?: ProjectSource | null;
  raw?: { params: RawDevelopParams; decoderVersion: string } | null;
  color?: { output: ColorOutputSettings } | null;
  /** 未识别字段原样保留，保存时不得丢弃。 */
  [key: string]: unknown;
}

export interface ProjectSource {
  sourceType: string;
  path: string;
  hash: string;
  metadata: unknown | null;
}

export interface ProjectDoc {
  version: number;
  createdAt: string;
  activeIndex: number;
  export: import("./state/params").ExportSettings;
  /** 第七轮：计算后端偏好。 */
  compute?: { preference: ComputeBackend } | null;
  photos: ProjectPhoto[];
  /** 项目身份：防止 A 项目保存结果覆盖 B 项目。 */
  projectUuid?: string;
  /** 保存代数：旧快照不得覆盖更新的磁盘内容。 */
  revision?: number;
  /** 最近一次成功保存时间（ISO 8601）。 */
  updatedAt?: string;
  /** 未识别字段原样保留，保存时不得丢弃。 */
  [key: string]: unknown;
}

export interface ProjectOpenResult {
  path: string;
  project: ProjectDoc;
  /** 旧版本项目的迁移说明；空/缺省表示无需迁移。 */
  migrationLog?: string[];
}

export interface ProjectSaved {
  path: string;
  bytes: number;
  projectUuid: string;
  revision: number;
  /** ChangeSet 的 requestId 之前已执行过，本次没有再次应用。 */
  duplicate?: boolean;
}

/** ROUND 10：类型化保存协议。 */
export interface ChangeSet {
  projectId: string;
  baseRevision: number;
  requestId: string;
  operations: ChangeOperation[];
  createdAt: string;
  /** 冲突时重新读取磁盘最新状态并重新合并增量（不使用旧快照覆盖）。 */
  rebaseOnConflict?: boolean;
}

export type ChangeOperation =
  | { type: "updateDocMeta"; patch: Record<string, unknown> }
  | { type: "updatePhotoParams"; photo: string; patch: Record<string, unknown> }
  | {
      type: "updateFaceParams";
      photo: string;
      faceUuid: string;
      patch: Record<string, unknown>;
    }
  | {
      type: "updateReshapeParams";
      photo: string;
      faceUuid: string;
      patch: Record<string, unknown>;
    }
  | { type: "updateSubjectMask"; photo: string; patch: Record<string, unknown> }
  | { type: "updateManualMask"; photo: string; patch: Record<string, unknown> }
  | { type: "updateBackground"; photo: string; patch: Record<string, unknown> }
  | { type: "updateRawParams"; photo: string; patch: Record<string, unknown> }
  | { type: "updateColorParams"; photo: string; patch: Record<string, unknown> }
  | {
      type: "updateExportParams";
      photo?: string | null;
      patch: Record<string, unknown>;
    }
  | { type: "addFace"; photo: string; face: Record<string, unknown> }
  | { type: "removeFace"; photo: string; faceUuid: string }
  | { type: "addMask"; photo: string; mask: Record<string, unknown> }
  | {
      type: "updateMask";
      photo: string;
      maskId: string;
      patch: Record<string, unknown>;
    }
  | { type: "removeMask"; photo: string; maskId: string }
  | { type: "upsertFaceRecord"; photo: string; record: Record<string, unknown> }
  | { type: "removeFaceRecord"; photo: string; faceUuid: string }
  | { type: "addPhoto"; photo: ProjectPhoto }
  | { type: "updateSource"; photo: string; patch: Record<string, unknown> }
  | { type: "updateBatchParams"; photos: string[]; patch: Record<string, unknown> };

/** ChangeSet 失败响应：kind + message + 磁盘最新 revision。 */
export interface ChangeSetFailure {
  kind:
    | "PROJECT_MISMATCH"
    | "REVISION_CONFLICT"
    | "INVALID_OPERATION"
    | "TARGET_NOT_FOUND"
    | "STORAGE_ERROR"
    | "TASK_ERROR";
  message: string;
  currentRevision?: number | null;
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

export type Workspace =
  | "edit"
  | "smart"
  | "presets"
  | "batch"
  | "raw"
  | "color"
  | "performance"
  | "ai"
  | "models";

/** 第七轮：RAW 显影参数，与 Rust `RawDevelopParams` 对应。 */
export type WbMode = "camera" | "auto" | "custom";
export type DemosaicMode = "mhc" | "bilinear";

export interface RawDevelopParams {
  exposureEv: number;
  highlights: number;
  shadows: number;
  wbMode: WbMode;
  temperature: number;
  tint: number;
  demosaic: DemosaicMode;
  maxEdge: number | null;
}

export const DEFAULT_RAW_PARAMS: RawDevelopParams = {
  exposureEv: 0,
  highlights: 0,
  shadows: 0,
  wbMode: "camera",
  temperature: 6500,
  tint: 0,
  demosaic: "mhc",
  maxEdge: null,
};

export interface CfaInfo {
  pattern: string;
  filters: number;
  colors: number;
  xtrans: string[] | null;
}

export interface RawInfo {
  path: string;
  fileSize: number;
  width: number;
  height: number;
  rawWidth: number;
  rawHeight: number;
  topMargin: number;
  leftMargin: number;
  cfa: CfaInfo;
  blackLevel: { black: number; cblack: number[] };
  whiteBalance: { camMul: number[]; rawCamMul: number[]; preMul: number[]; source: string };
  cameraMatrix: number[][];
  make: string;
  model: string;
  exif: { iso: number; shutter: number; aperture: number; focalLength: number; timestamp: number };
  whiteLevel: number;
  hasEmbeddedPreview: boolean;
  preview: { format: string; width: number; height: number; bytes: number } | null;
}

export interface RawCapabilities {
  decoder: string;
  supported: string[];
  unsupported: string[];
  demosaic: string[];
  notes: string[];
}

export interface RawDevelopResult {
  url: string;
  width: number;
  height: number;
  gains: number[];
  wbSource: string;
  demosaic: string;
}

/** 第七轮：输出 Profile 与色彩管理状态。 */
export type ProfileKind = "srgb" | "adobeRgb" | "displayP3" | "proPhoto" | "custom";
export type RenderingIntent = "perceptual" | "relative" | "saturation" | "absolute";

export interface ColorOutputSettings {
  profile: ProfileKind;
  customPath: string | null;
  intent: RenderingIntent;
  blackPointCompensation: boolean;
  embed: boolean;
}

export const DEFAULT_COLOR_SETTINGS: ColorOutputSettings = {
  profile: "srgb",
  customPath: null,
  intent: "relative",
  blackPointCompensation: true,
  embed: true,
};

export interface ColorProfileOption {
  kind: ProfileKind;
  label: string;
  builtin: boolean;
}

export interface DisplayStatus {
  source: string;
  description: string;
  available: boolean;
  reason: string;
}

export interface ColorStatus {
  workingSpace: string;
  output: ColorOutputSettings;
  outputLabel: string;
  display: DisplayStatus;
  displayTransform: boolean;
  cms: string;
  profiles: ColorProfileOption[];
  notes: string[];
}

export interface ProfileSummary {
  path: string;
  description: string;
  colorSpace: string;
  channels: number;
  version: number;
}

/** 软打样预览：真实 sRGB → 输出 Profile → sRGB 往返统计。 */
export interface SoftProofResult {
  url: string;
  width: number;
  height: number;
  outOfGamutPixels: number;
  totalPixels: number;
  ratio: number;
  maxDeltaE00: number;
  intent: string;
  profile: string;
}

/** 显示器色彩预览：sRGB → ColorSync 主显示器 Profile，PNG 内嵌显示 ICC。 */
export interface DisplayPreviewResult {
  url: string;
  width: number;
  height: number;
  source: string;
  description: string;
  reason: string;
}

/** 第七轮：计算后端。 */
export type ComputeBackend = "auto" | "gpu" | "cpu";

export interface GpuCapabilities {
  deviceName: string;
  backend: string;
  driver: string;
  deviceType: string;
  maxTextureDimension2d: number;
  maxStorageBufferBindingSize: number;
  maxBufferSize: number;
}

export interface GpuStatus {
  preference: ComputeBackend;
  available: boolean;
  enabled: boolean;
  backend: string;
  device: string;
  reason: string;
  verifiedSpeedup: number | null;
  capabilities: GpuCapabilities | null;
}

export interface GpuBenchRow {
  width: number;
  height: number;
  megapixels: number;
  cpuMs: number;
  gpuUploadMs: number;
  gpuComputeMs: number;
  gpuTotalMs: number;
  speedup: number;
  uploadBytes: number;
}

export interface GpuBenchmarkResult {
  rows: GpuBenchRow[];
  verifiedSpeedup: number | null;
  autoEnabled: boolean;
  status: GpuStatus;
}

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
  photos: {
    path: string;
    params: import("./state/params").EditParams;
    rawParams?: RawDevelopParams | null;
  }[];
  settings: import("./state/params").ExportSettings;
  outputDir: string;
  naming: string;
  startIndex: number;
  conflict: ConflictPolicy;
  overwriteConfirmed: boolean;
}

/** 第八轮：人脸智能 Provider 统一抽象（与 Rust `face_intelligence` 对应）。 */
export type EvidenceLevel = "verified" | "declared" | "unverified" | "unsupported";

export interface ProviderCapabilities {
  faceDetection: boolean;
  sparseLandmarks: boolean;
  denseLandmarks: boolean;
  eyeLandmarks: boolean;
  eyelidLandmarks: boolean;
  noseLandmarks: boolean;
  lipLandmarks: boolean;
  faceMesh: boolean;
  faceParsing: boolean;
  skinSegmentation: boolean;
  hairSegmentation: boolean;
  faceMatting: boolean;
  multiFace: boolean;
  offlineInference: boolean;
  staticImage: boolean;
  maxInputResolution: [number, number] | null;
}

export interface CapabilityEvidence {
  capability: string;
  level: EvidenceLevel;
  note: string;
  measurement?: string | null;
}

export type LicenseState =
  | "verifiedCommercial"
  | "verifiedPersonal"
  | "systemFramework"
  | "researchOnly"
  | "userUnverified"
  | "prohibited"
  | "incompatible"
  | "removed";

export interface LicenseInfo {
  name: string;
  url: string | null;
  state: LicenseState;
  note: string;
}

export type ProviderKind = "builtin" | "system" | "userModel" | "commercialPoc";

export type ProviderState =
  | "ready"
  | "idle"
  | "analyzing"
  | "missing"
  | "unsupported"
  | "failed"
  | "lowQuality"
  | "licenseUnverified";

export interface ProviderDescriptor {
  id: string;
  displayName: string;
  version: string;
  kind: ProviderKind;
  state: ProviderState;
  available: boolean;
  enabled: boolean;
  license: LicenseInfo;
  capabilities: ProviderCapabilities;
  verifiedCapabilities: ProviderCapabilities;
  evidence: CapabilityEvidence[];
  topologyId: string | null;
  note: string;
}

export interface RegionQuality {
  region: string;
  points: number;
  passed: boolean;
  reason: string;
}

export interface FaceQuality {
  passed: boolean;
  densePoints: number;
  confidence: number;
  eyeDistance: number;
  regions: RegionQuality[];
  issues: string[];
}

export interface FeatureGate {
  enabled: boolean;
  reason: string;
}

export interface Level3Gate {
  providerId: string;
  providerVersion: string;
  topologyId: string;
  modelVersion: string;
  faceUuid: string;
  denseLandmarks: boolean;
  eyes: FeatureGate;
  nose: FeatureGate;
  mouth: FeatureGate;
  chin: FeatureGate;
  summary: string;
  issues: string[];
}

export interface AnalyzedFace {
  faceUuid: string;
  boundingBox: { x: number; y: number; width: number; height: number };
  confidence: number;
  densePoints: number;
  denseFingerprint: string | null;
  quality: FaceQuality;
  gate: Level3Gate;
}

export interface StabilityReport {
  providerId: string;
  topologyId: string;
  runs: number;
  facesPerRun: number[];
  eyeDistance: number;
  maxDeviation: number;
  ratio: number;
  passed: boolean;
  elapsedMs: number[];
  note: string;
}

export interface FaceAnalysisRecord {
  photoPath: string;
  providerId: string;
  providerName: string;
  topologyId: string;
  processingTimeMs: number;
  imageWidth: number;
  imageHeight: number;
  faces: AnalyzedFace[];
  stability: StabilityReport | null;
  warnings: string[];
  totalLandmarks: number;
  landmarkCounts: Record<string, number>;
  createdAtUnix: number;
}

export interface QualityThresholds {
  minConfidence: number;
  maxStabilityRatio: number;
  minEyeWidth: number;
  minMouthWidth: number;
  minDensePoints: number;
}

export interface FaceIntelligenceStatus {
  activeProvider: string;
  providers: ProviderDescriptor[];
  lastAnalysis: FaceAnalysisRecord | null;
  quality: QualityReport[];
  level3Summary: string;
  modelsRoot: string;
  thresholds: QualityThresholds;
}

export interface QualityReport {
  providerId: string;
  providerName: string;
  stability: StabilityReport;
  qualitySample: FaceQuality[];
  verified: boolean;
  note: string;
  measuredAtUnix: number;
}

export interface AnalyzeFacesReport {
  providerId: string;
  providerName: string;
  processingTimeMs: number;
  faceCount: number;
  denseFaceCount: number;
  totalLandmarks: number;
  gateSummary: string;
  warnings: string[];
  faces: DetectedFace[];
  record: FaceAnalysisRecord;
}

export interface ProviderSwitchReport {
  from: string;
  to: string;
  previousGateSummary: string | null;
  requiresReview: boolean;
  clearedFaces: number;
  message: string;
}

export interface InstalledModel {
  manifest: {
    schemaVersion: number;
    id: string;
    name: string;
    version: string;
    author: string;
    description: string;
    task: string;
    topologyId: string;
    input: {
      width: number;
      height: number;
      layout: string;
      normalization: string;
      cropMargin: number;
    };
    output: { layout: string; coordinate: string };
    license: {
      name: string;
      url: string | null;
      commercialUseClaimed: boolean;
      redistributionClaimed: boolean;
      note: string;
    };
    sha256: string;
    fileName: string;
    sourceUrl: string | null;
    sourcePath: string;
    importedAt: string;
    validation: {
      ok: boolean;
      sha256: string;
      fileSize: number;
      inputShape: (number | null)[];
      outputShape: (number | null)[];
      operatorCount: number;
      elapsedMs: number;
      warnings: string[];
      errors: string[];
    } | null;
  };
  directory: string;
  modelPath: string;
  present: boolean;
  sha256Matches: boolean;
  validationOk: boolean;
  error: string | null;
}

export interface ImportFaceModelInput {
  source: string;
  name: string;
  version: string;
  author: string;
  description: string;
  sourceUrl?: string | null;
  licenseName: string;
  licenseUrl?: string | null;
  licenseCommercialClaimed: boolean;
  licenseRedistributionClaimed: boolean;
  licenseNote: string;
  normalization: "zeroToOne" | "minusOneToOne" | "imagenet" | "raw255";
  outputCoordinate: "normalizedCrop" | "pixelCrop";
  cropMargin: number;
}

// ───────────────────────────── R11 · 模型注册与推理契约 ─────────────────────────────
//
// 与 Rust `ai_engine::inference::InferenceError::code()` 一一对应。
// 新增错误码必须同时改两侧（`src-tauri/src/ai_engine/inference/error.rs` 的
// `ERROR_CODES` 与下面的联合类型），否则前端 `switch` 会漏分支。
export type ModelErrorCode =
  | "E_MODEL_LICENSE"
  | "E_MODEL_MISSING"
  | "E_MODEL_CORRUPT"
  | "E_MODEL_VERSION"
  | "E_MODEL_LOAD"
  | "E_MODEL_CONTRACT"
  | "E_INPUT"
  | "E_INFER"
  | "E_TIMEOUT"
  | "E_CANCELLED"
  | "E_BACKEND";

/** 与 Rust `ModelRuntimeState::as_str()` 一一对应。 */
export type ModelState =
  | "verified"
  | "ready"
  | "licenseBlocked"
  | "missing"
  | "corrupt"
  | "versionMismatch"
  | "loadFailed";

/** 与 Rust `ModelFailure` 对应。可选字段由 Rust 侧 `skip_serializing_if` 省略。 */
export interface ModelFailure {
  kind: Exclude<ModelState, "verified" | "ready">;
  modelId: string;
  message: string;
  detail?: string;
  searched?: string[];
  expected?: string;
  actual?: string;
}

/** 与 Rust `LicenseState`（face_intelligence::capability）对应。 */
export type ModelLicenseState =
  | "verifiedCommercial"
  | "verifiedPersonal"
  | "systemFramework"
  | "researchOnly"
  | "userUnverified"
  | "prohibited"
  | "incompatible"
  | "removed";

/** 三态判定：许可字段的取值。 */
export type LicenseVerdict = "allowed" | "notAllowed" | "unverified";

export interface ModelLicenseEvidence {
  kind: "officialPage" | "bundledLicenseFile" | "weightCard";
  url: string;
  /** 原文摘句，便于法务复核。 */
  quote: string;
  accessedAt: string;
}

/** 许可四字段**独立**：不得把 trainingData 合并进 commercialUse。 */
export interface ModelLicenseView {
  state: ModelLicenseState;
  spdx: string | null;
  weights: LicenseVerdict;
  commercialUse: LicenseVerdict;
  redistribution: LicenseVerdict;
  trainingData: LicenseVerdict;
  /** 未核实的字段名，例如 ["trainingData"]。 */
  unverifiedFields: string[];
  evidence: ModelLicenseEvidence[];
  note: string;
}

/** 结构化张量契约（不是自由字符串）。 */
export interface TensorSpecView {
  dtype: "f32";
  layout: "nchw";
  shape: number[];
  valueRange: "zeroOne" | "minusOneToOne" | "raw255" | "imagenet";
  channelOrder: "bgr" | "rgb";
}

export interface ModelOutputHeadView {
  name: string;
  shape: number[];
  /** 该 head 的语义说明（可选展示）。 */
  semantic?: string;
}

export interface ModelEntryView {
  id: string;
  displayName: string;
  version: string;
  task: "faceDetect" | "faceLandmark" | "skinParse" | "faceParse" | "subjectSegment" | "bodyPose";
  origin: "builtin" | "userImported";
  /** 是否属于正式注册清单。用户自备模型恒为 false，不参与默认美肤/默认精修。 */
  official: boolean;
  format: "onnx";
  file: string;
  sizeBytes: number;
  sizeOnDiskBytes: number;
  sha256: string;
  input: TensorSpecView;
  outputs: ModelOutputHeadView[];
  license: ModelLicenseView;
  state: ModelState;
  loaded: boolean;
  path: string | null;
  failure: ModelFailure | null;
  /** 全部发现（含被 failure 掩盖的），用于运维诊断。 */
  findings: ModelFailure[];
  checkedAtUnix: number;
}

export interface RegistrySnapshot {
  schemaVersion: number;
  generatedAtUnix: number;
  models: ModelEntryView[];
  summary: {
    total: number;
    official: number;
    ready: number;
    verified: number;
    blocked: number;
    userImported: number;
  };
}

export interface ModelRuntimeStatus {
  modelId: string;
  state: ModelState;
  loaded: boolean;
  elapsedMs: number;
  failure: ModelFailure | null;
  backend: "cpu-tract" | "gpu-reserved";
}
