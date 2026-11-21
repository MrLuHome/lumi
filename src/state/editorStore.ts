import { create } from "zustand";
import type {
  AnalysisReport,
  BackgroundImageInfo,
  BackgroundReport,
  BatchSnapshot,
  BeautyAnalysis,
  BeautyParams,
  BeautyProgress,
  BodyCapability,
  BodyReport,
  ColorOutputSettings,
  ColorStatus,
  ComputeBackend,
  ConflictPolicy,
  EditParams,
  ExportSettings,
  FaceDetectionResult,
  FaceMatchEntry,
  FaceModelStatus,
  FaceProgress,
  FaceReshapeReport,
  FaceRestoreResult,
  GpuStatus,
  MaskPreview,
  MaskReport,
  MaskStroke,
  PhotoAsset,
  ProjectDoc,
  ProjectFaceRecord,
  RawDevelopParams,
  RenderProgress,
  RetouchMode,
  RetouchPreset,
  SegmentationCapability,
  SkinModelStatus,
  SubjectMaskPreview,
  Workspace,
} from "../types";
import {
  DEFAULT_COLOR_SETTINGS,
  DEFAULT_RAW_PARAMS,
} from "../types";
import {
  BEAUTY_LABELS,
  DEFAULT_EXPORT,
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_LABELS,
  type BeautyKey,
  type BodyAnchor,
  type BodyShapeKey,
  type CurveChannel,
  type CurvePoint,
  type HslKey,
  type ManualMask,
  type FaceShapeKey,
  type SubjectMaskStroke,
  type SubjectMaskStrokeKind,
  type WarpMode,
  type WarpStroke,
  clampParams,
  cloneParams,
  defaultBackground,
  defaultBeauty,
  defaultBodyAnchor,
  defaultFaceBeauty,
  defaultFaceReshape,
  findBodyAdjust,
  findFaceBeauty,
  findFaceReshape,
  isNeutral,
  paramsEqual,
  withBodyAdjust,
  withFaceBeauty,
  withFaceReshape,
} from "./params";
import {
  createHistory,
  pushHistory,
  redoHistory,
  undoHistory,
  type HistoryStack,
} from "./history";

export const COALESCE_WINDOW_MS = 600;
export const ZOOM_MIN = 0.02;
export const ZOOM_MAX = 4;

/** 缩放超过该比例时请求 3200px 高清预览。 */
export const DETAIL_ZOOM_THRESHOLD = 1.05;

export type RenderStatus = "idle" | "rendering" | "ready" | "error";
export type CompareMode = "off" | "split";
export type FaceStatus = "idle" | "loading" | "ready" | "error";
export type BeautyStatus = "idle" | "analyzing" | "ready" | "error";
export type MaskStatus = "idle" | "loading" | "ready" | "error";
export type BrushMode = "off" | "paint" | "erase" | "repair";

export interface Notice {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export interface RenderedPreview {
  url: string;
  key: string;
  elapsedMs: number;
}

type Point = { x: number; y: number };
type Size = { width: number; height: number };
type ParamUpdater = (current: EditParams) => EditParams;

export interface EditorState {
  photos: PhotoAsset[];
  activeId: string | null;
  params: Record<string, EditParams>;
  history: Record<string, HistoryStack<EditParams>>;
  lastEdit: { photoId: string; key: string; at: number } | null;
  clipboard: EditParams | null;
  exportSettings: ExportSettings;
  projectPath: string | null;
  /** 项目载入时保存的原始人脸记录与源图哈希，用于重开匹配。 */
  projectFaceRecords: Record<string, ProjectFaceRecord[]>;
  projectSourceHashes: Record<string, string>;

  renderedPreviews: Record<string, RenderedPreview>;
  compareMode: CompareMode;
  compareSplit: number;
  holdOriginal: boolean;

  zoom: number;
  fitZoom: number;
  autoFit: boolean;
  pan: Point;
  viewport: Size;

  renderStatus: RenderStatus;
  renderElapsedMs: number | null;
  renderError: string | null;
  renderProgress: RenderProgress | null;

  faces: Record<string, FaceDetectionResult>;
  faceStatus: FaceStatus;
  faceProgress: FaceProgress | null;
  faceError: string | null;
  activeFace: number;
  showFaceOverlay: boolean;
  modelStatus: FaceModelStatus | null;
  /** 项目重开时需要用户确认的低置信人脸匹配。 */
  pendingFaceMatches: Record<string, FaceMatchEntry[]>;
  /** 本次检测中消失、但项目里仍保留的人脸记录。 */
  missingFaceRecords: Record<string, ProjectFaceRecord[]>;

  /** 手动变形工具；off 时画布恢复普通平移/画笔行为。 */
  warpTool: WarpMode | "off";
  warpRadius: number;
  warpStrength: number;
  warpFeather: number;
  showMesh: boolean;
  /** 当前渲染的塑形质量报告与警告。 */
  reshapeReports: FaceReshapeReport[];
  renderWarnings: string[];

  beautyAnalysis: Record<string, BeautyAnalysis>;
  beautyStatus: BeautyStatus;
  beautyProgress: BeautyProgress | null;
  beautyError: string | null;
  skinModel: SkinModelStatus | null;

  maskVisible: boolean;
  maskPreview: MaskPreview | null;
  maskStatus: MaskStatus;
  maskError: string | null;
  brushMode: BrushMode;
  /** 画笔半径，按图像宽度归一化。 */
  brushRadius: number;

  /** 第五轮：手动人物蒙版画笔；off 时画布恢复普通行为。 */
  subjectBrush: SubjectMaskStrokeKind | "off";
  subjectBrushRadius: number;
  subjectBrushHardness: number;
  subjectBrushStrength: number;
  /** 蒙版显示模式：红色遮罩 / 黑白 Alpha。 */
  subjectMaskView: "overlay" | "alpha";
  /** 是否在画布显示手动蒙版预览。 */
  showSubjectMask: boolean;
  /** 笔触写入全局蒙版还是当前人物独立蒙版。 */
  subjectMaskTarget: "global" | "subject";
  subjectMaskPreview: SubjectMaskPreview | null;
  subjectMaskStatus: MaskStatus;
  subjectMaskError: string | null;
  segmentationCapability: SegmentationCapability | null;

  /** 第五轮：身材调整工具与参数。 */
  bodyTool: WarpMode | "off";
  bodyRadius: number;
  bodyStrength: number;
  bodyFeather: number;
  bodyAnchorRadius: number;
  bodySubjectId: string;
  bodyCapability: BodyCapability | null;
  showBodyAnchors: boolean;
  bodyReports: BodyReport[];
  backgroundReport: BackgroundReport | null;
  maskReport: MaskReport | null;

  importing: boolean;
  exporting: boolean;
  dragActive: boolean;
  notice: Notice | null;

  /** 第六轮：工作区切换。 */
  workspace: Workspace;
  setWorkspace: (workspace: Workspace) => void;

  /** 第七轮：Raster/RAW 来源、RAW 显影参数、色彩输出与计算后端。 */
  rawByPhoto: Record<string, RawDevelopParams>;
  rawStatus: Record<string, MaskStatus>;
  rawError: string | null;
  rawDirty: boolean;
  setRawParams: (photoId: string, patch: Partial<RawDevelopParams>) => void;
  setRawStatus: (photoId: string, status: MaskStatus) => void;
  setRawError: (message: string | null) => void;
  resetRawParams: (photoId: string) => void;
  colorSettings: ColorOutputSettings;
  colorStatus: ColorStatus | null;
  setColorSettings: (settings: ColorOutputSettings) => void;
  setColorStatus: (status: ColorStatus | null) => void;
  computePreference: ComputeBackend;
  setComputePreference: (preference: ComputeBackend) => void;
  gpuStatus: GpuStatus | null;
  setGpuStatus: (status: GpuStatus | null) => void;

  /** 智能精修分析：按照片保存最近一次真实报告。 */
  smartReports: Record<string, AnalysisReport>;
  smartStatus: MaskStatus;
  smartError: string | null;
  smartElapsedMs: number | null;
  smartMode: RetouchMode;
  setSmartMode: (mode: RetouchMode) => void;
  setSmartReport: (photoId: string, report: AnalysisReport, elapsedMs: number) => void;
  setSmartStatus: (status: MaskStatus) => void;
  setSmartError: (message: string | null) => void;

  /** 预设库。缩略图来自当前照片的真实渲染，不用占位图。 */
  presets: RetouchPreset[];
  presetStatus: MaskStatus;
  presetError: string | null;
  presetStrength: number;
  presetPreviews: Record<string, string>;
  presetPreviewStatus: MaskStatus;
  selectedPresetId: string | null;
  setPresets: (presets: RetouchPreset[]) => void;
  setPresetStatus: (status: MaskStatus) => void;
  setPresetError: (message: string | null) => void;
  setPresetStrength: (value: number) => void;
  setPresetPreviews: (previews: Record<string, string>) => void;
  setPresetPreviewStatus: (status: MaskStatus) => void;
  setSelectedPreset: (presetId: string | null) => void;

  /** 批量处理：选择、输出设置与任务快照。 */
  batchSelection: string[];
  batchSnapshot: BatchSnapshot | null;
  batchOutputDir: string;
  batchNaming: string;
  batchStartIndex: number;
  batchConflict: ConflictPolicy;
  batchSettings: ExportSettings;
  toggleBatchSelection: (photoId: string) => void;
  setBatchSelection: (ids: string[]) => void;
  setBatchSnapshot: (snapshot: BatchSnapshot | null) => void;
  setBatchOutputDir: (dir: string) => void;
  setBatchNaming: (naming: string) => void;
  setBatchStartIndex: (index: number) => void;
  setBatchConflict: (policy: ConflictPolicy) => void;
  setBatchSettings: (settings: ExportSettings) => void;

  /** 把后端返回的完整参数写回指定照片，并保留可撤销历史。 */
  applyExternalParams: (photoId: string, next: EditParams, label: string) => void;

  addPhotos: (items: PhotoAsset[]) => void;
  removePhoto: (id: string) => void;
  setActive: (id: string) => void;
  loadProject: (items: PhotoAsset[], project: ProjectDoc) => void;
  setProjectPath: (path: string | null) => void;

  updateParam: (key: keyof typeof PARAM_LABELS, value: number) => void;
  updateParams: (updater: ParamUpdater, label: string, coalesceKey?: string) => void;
  updateHsl: (channel: HslKey, field: "hue" | "saturation" | "luminance", value: number) => void;
  setCurve: (channel: CurveChannel, points: CurvePoint[]) => void;
  resetCurve: (channel: CurveChannel) => void;
  setLut: (lut: EditParams["lut"]) => void;
  setExportSettings: (settings: ExportSettings) => void;
  resetGroup: (groupId: string) => void;
  resetAll: () => void;
  copyParams: () => void;
  pasteParams: () => void;
  undo: () => void;
  redo: () => void;

  setCompareMode: (mode: CompareMode) => void;
  toggleCompare: () => void;
  setCompareSplit: (value: number) => void;
  setHoldOriginal: (value: boolean) => void;

  setViewport: (size: Size) => void;
  setFitZoom: (value: number) => void;
  setZoom: (value: number, anchor?: Point) => void;
  zoomStep: (direction: 1 | -1) => void;
  setPan: (pan: Point) => void;
  fitToWindow: () => void;
  actualPixels: () => void;

  markRendered: (
    photoId: string,
    url: string,
    key: string,
    elapsedMs: number,
    extra?: {
      warnings?: string[];
      reshape?: FaceReshapeReport[];
      body?: BodyReport[];
      background?: BackgroundReport | null;
      mask?: MaskReport | null;
    },
  ) => void;
  setRenderStatus: (status: RenderStatus) => void;
  setRenderError: (message: string) => void;
  setRenderProgress: (progress: RenderProgress | null) => void;

  setFaces: (photoId: string, result: FaceDetectionResult) => void;
  setFaceStatus: (status: FaceStatus) => void;
  setFaceProgress: (progress: FaceProgress | null) => void;
  setFaceError: (message: string) => void;
  setActiveFace: (index: number) => void;
  setShowFaceOverlay: (value: boolean) => void;
  setModelStatus: (status: FaceModelStatus) => void;
  /** 项目重开后应用后端恢复结果：只按 UUID 恢复高置信人脸参数。 */
  applyFaceRestore: (photoId: string, result: FaceRestoreResult) => void;
  /** 人工确认后，用后端返回的新人脸列表重新绑定 UUID。 */
  applyConfirmedFaces: (photoId: string, faces: FaceDetectionResult["faces"]) => void;

  setWarpTool: (tool: WarpMode | "off") => void;
  setWarpRadius: (radius: number) => void;
  setWarpStrength: (value: number) => void;
  setWarpFeather: (value: number) => void;
  setShowMesh: (value: boolean) => void;
  updateFaceShape: (faceId: number, key: FaceShapeKey, value: number) => void;
  appendWarpStroke: (faceId: number, stroke: WarpStroke) => void;
  undoWarpStroke: (faceId: number) => void;
  clearWarpStrokes: (faceId: number) => void;
  resetFaceReshape: (faceId: number) => void;
  resetAllReshape: () => void;
  copyFaceReshapeToOthers: (faceId: number) => void;

  setBeautyStatus: (status: BeautyStatus) => void;
  setBeautyProgress: (progress: BeautyProgress | null) => void;
  setBeautyError: (message: string) => void;
  setBeautyAnalysis: (photoId: string, analysis: BeautyAnalysis) => void;
  setSkinModel: (status: SkinModelStatus) => void;
  updateFaceBeauty: (faceId: number, key: BeautyKey, value: number) => void;
  setFaceEnabled: (faceId: number, enabled: boolean) => void;
  resetFaceBeauty: (faceId: number) => void;
  applyBeautySuggestion: (faceId: number, beauty: BeautyParams, label?: string) => void;
  copyFaceParamsToOthers: (faceId: number) => void;
  updateManualMask: (
    updater: (mask: ManualMask) => ManualMask,
    label: string,
    coalesceKey?: string,
  ) => void;
  appendMaskStroke: (stroke: MaskStroke) => void;
  undoMaskStroke: () => void;
  clearMaskStrokes: () => void;
  addRepairPoint: (x: number, y: number, radius: number) => void;
  undoRepairPoint: () => void;
  clearRepairPoints: () => void;
  setMaskVisible: (visible: boolean) => void;
  setMaskPreview: (preview: MaskPreview | null) => void;
  setMaskStatus: (status: MaskStatus) => void;
  setMaskError: (message: string) => void;
  setBrushMode: (mode: BrushMode) => void;
  setBrushRadius: (radius: number) => void;

  setSubjectBrush: (kind: SubjectMaskStrokeKind | "off") => void;
  setSubjectBrushRadius: (radius: number) => void;
  setSubjectBrushHardness: (value: number) => void;
  setSubjectBrushStrength: (value: number) => void;
  setSubjectMaskView: (view: "overlay" | "alpha") => void;
  setShowSubjectMask: (value: boolean) => void;
  setSubjectMaskTarget: (target: "global" | "subject") => void;
  updateSubjectMask: (
    updater: (mask: EditParams["subjectMask"]) => EditParams["subjectMask"],
    label: string,
    coalesceKey?: string,
  ) => void;
  appendSubjectMaskStroke: (stroke: SubjectMaskStroke) => void;
  undoSubjectMaskStroke: () => void;
  clearSubjectMask: () => void;
  setSubjectMaskEnabled: (enabled: boolean) => void;
  setSubjectMaskPreview: (preview: SubjectMaskPreview | null) => void;
  setSubjectMaskStatus: (status: MaskStatus) => void;
  setSubjectMaskError: (message: string) => void;
  setSegmentationCapability: (capability: SegmentationCapability) => void;

  setBodyTool: (tool: WarpMode | "off") => void;
  setBodyRadius: (radius: number) => void;
  setBodyStrength: (value: number) => void;
  setBodyFeather: (value: number) => void;
  setBodyAnchorRadius: (radius: number) => void;
  setBodySubjectId: (subjectId: string) => void;
  setBodyCapability: (capability: BodyCapability) => void;
  setShowBodyAnchors: (value: boolean) => void;
  updateBodyShape: (key: BodyShapeKey, value: number) => void;
  updateBody: (
    updater: (entry: EditParams["body"][number]) => EditParams["body"][number],
    label: string,
    coalesceKey?: string,
  ) => void;
  addBodyAnchor: (anchor: BodyAnchor) => void;
  addBodyAnchorPresets: (presets: { part: BodyAnchor["part"]; x: number; y: number }[]) => void;
  updateBodyAnchor: (index: number, patch: Partial<BodyAnchor>) => void;
  removeBodyAnchor: (index: number) => void;
  lockBodyAnchor: (index: number, locked: boolean) => void;
  clearBodyAnchors: () => void;
  appendBodyStroke: (stroke: WarpStroke) => void;
  undoBodyStroke: () => void;
  clearBodyStrokes: () => void;
  resetBodyAdjust: () => void;
  resetBackground: () => void;
  setBackgroundImage: (info: BackgroundImageInfo | null) => void;
  updateBackground: (
    updater: (background: EditParams["background"]) => EditParams["background"],
    label: string,
    coalesceKey?: string,
  ) => void;

  setImporting: (value: boolean) => void;
  setExporting: (value: boolean) => void;
  setDragActive: (value: boolean) => void;
  notify: (kind: Notice["kind"], text: string) => void;
  dismissNotice: (id: number) => void;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function clampPan(pan: Point, photo: PhotoAsset | undefined, zoom: number, viewport: Size): Point {
  if (!photo) return { x: 0, y: 0 };
  const slackX = Math.max(0, (photo.width * zoom - viewport.width) / 2 + 32);
  const slackY = Math.max(0, (photo.height * zoom - viewport.height) / 2 + 32);
  return {
    x: clamp(pan.x, -slackX, slackX),
    y: clamp(pan.y, -slackY, slackY),
  };
}

let noticeSeed = 0;

export function createEditorStore() {
  return create<EditorState>((set, get) => {
    const activePhoto = (): PhotoAsset | undefined => {
      const { photos, activeId } = get();
      return photos.find((photo) => photo.id === activeId);
    };

    const commit = (label: string, next: EditParams) => {
      const state = get();
      const photoId = state.activeId;
      if (!photoId) return;
      const current = state.params[photoId] ?? DEFAULT_PARAMS;
      if (paramsEqual(current, next)) return;
      set({
        params: { ...state.params, [photoId]: next },
        history: {
          ...state.history,
          [photoId]: pushHistory(state.history[photoId], current, label),
        },
        lastEdit: null,
      });
    };

    const apply = (updater: ParamUpdater, label: string, coalesceKey?: string) => {
      const state = get();
      const photoId = state.activeId;
      if (!photoId) return;
      const current = state.params[photoId] ?? DEFAULT_PARAMS;
      const next = clampParams(updater(cloneParams(current)));
      if (paramsEqual(current, next)) return;

      const now = Date.now();
      const last = state.lastEdit;
      const shouldStartEntry =
        !coalesceKey ||
        !last ||
        last.photoId !== photoId ||
        last.key !== coalesceKey ||
        now - last.at > COALESCE_WINDOW_MS;

      set({
        params: { ...state.params, [photoId]: next },
        history: shouldStartEntry
          ? {
              ...state.history,
              [photoId]: pushHistory(state.history[photoId], current, label, now),
            }
          : state.history,
        lastEdit: coalesceKey ? { photoId, key: coalesceKey, at: now } : null,
      });
    };

    return {
      photos: [],
      activeId: null,
      params: {},
      history: {},
      lastEdit: null,
      clipboard: null,
      exportSettings: DEFAULT_EXPORT,
      projectPath: null,
      projectFaceRecords: {},
      projectSourceHashes: {},

      renderedPreviews: {},
      compareMode: "off",
      compareSplit: 0.5,
      holdOriginal: false,

      zoom: 0.6,
      fitZoom: 0.6,
      autoFit: true,
      pan: { x: 0, y: 0 },
      viewport: { width: 1200, height: 800 },

      renderStatus: "idle",
      renderElapsedMs: null,
      renderError: null,
      renderProgress: null,

      faces: {},
      faceStatus: "idle",
      faceProgress: null,
      faceError: null,
      activeFace: 0,
      showFaceOverlay: true,
      modelStatus: null,
      pendingFaceMatches: {},
      missingFaceRecords: {},

      warpTool: "off",
      warpRadius: 0.06,
      warpStrength: 60,
      warpFeather: 50,
      showMesh: false,
      reshapeReports: [],
      renderWarnings: [],

      beautyAnalysis: {},
      beautyStatus: "idle",
      beautyProgress: null,
      beautyError: null,
      skinModel: null,

      maskVisible: false,
      maskPreview: null,
      maskStatus: "idle",
      maskError: null,
      brushMode: "off",
      brushRadius: 0.03,

      subjectBrush: "off",
      subjectBrushRadius: 0.04,
      subjectBrushHardness: 60,
      subjectBrushStrength: 100,
      subjectMaskView: "overlay",
      showSubjectMask: false,
      subjectMaskTarget: "global",
      subjectMaskPreview: null,
      subjectMaskStatus: "idle",
      subjectMaskError: null,
      segmentationCapability: null,

      bodyTool: "off",
      bodyRadius: 0.08,
      bodyStrength: 60,
      bodyFeather: 50,
      bodyAnchorRadius: 0.12,
      bodySubjectId: "subject-1",
      bodyCapability: null,
      showBodyAnchors: true,
      bodyReports: [],
      backgroundReport: null,
      maskReport: null,

      importing: false,
      exporting: false,
      dragActive: false,
      notice: null,

      workspace: "edit",
      setWorkspace: (workspace) => set({ workspace }),

      rawByPhoto: {},
      rawStatus: {},
      rawError: null,
      rawDirty: false,
      setRawParams: (photoId, patch) =>
        set((state) => ({
          rawByPhoto: {
            ...state.rawByPhoto,
            [photoId]: { ...(state.rawByPhoto[photoId] ?? DEFAULT_RAW_PARAMS), ...patch },
          },
          rawDirty: true,
        })),
      setRawStatus: (photoId, status) =>
        set((state) => ({ rawStatus: { ...state.rawStatus, [photoId]: status } })),
      setRawError: (rawError) => set({ rawError }),
      resetRawParams: (photoId) =>
        set((state) => {
          const next = { ...state.rawByPhoto };
          delete next[photoId];
          return { rawByPhoto: next, rawDirty: false, rawError: null };
        }),
      colorSettings: DEFAULT_COLOR_SETTINGS,
      colorStatus: null,
      setColorSettings: (colorSettings) => set({ colorSettings }),
      setColorStatus: (colorStatus) => set({ colorStatus }),
      computePreference: "auto",
      setComputePreference: (computePreference) => set({ computePreference }),
      gpuStatus: null,
      setGpuStatus: (gpuStatus) => set({ gpuStatus }),

      smartReports: {},
      smartStatus: "idle",
      smartError: null,
      smartElapsedMs: null,
      smartMode: "natural",
      setSmartMode: (smartMode) => set({ smartMode }),
      setSmartReport: (photoId, report, elapsedMs) =>
        set((state) => ({
          smartReports: { ...state.smartReports, [photoId]: report },
          smartElapsedMs: elapsedMs,
          smartStatus: "ready",
          smartError: null,
        })),
      setSmartStatus: (smartStatus) => set({ smartStatus }),
      setSmartError: (smartError) => set({ smartError, smartStatus: smartError ? "error" : "idle" }),

      presets: [],
      presetStatus: "idle",
      presetError: null,
      presetStrength: 100,
      presetPreviews: {},
      presetPreviewStatus: "idle",
      selectedPresetId: null,
      setPresets: (presets) => set({ presets, presetStatus: "ready", presetError: null }),
      setPresetStatus: (presetStatus) => set({ presetStatus }),
      setPresetError: (presetError) =>
        set({ presetError, presetStatus: presetError ? "error" : "idle" }),
      setPresetStrength: (presetStrength) =>
        set({ presetStrength: Math.min(100, Math.max(0, presetStrength)) }),
      setPresetPreviews: (presetPreviews) => set({ presetPreviews }),
      setPresetPreviewStatus: (presetPreviewStatus) => set({ presetPreviewStatus }),
      setSelectedPreset: (selectedPresetId) => set({ selectedPresetId }),

      batchSelection: [],
      batchSnapshot: null,
      batchOutputDir: "",
      batchNaming: "{name}-lumi",
      batchStartIndex: 1,
      batchConflict: "skip",
      batchSettings: DEFAULT_EXPORT,
      toggleBatchSelection: (photoId) =>
        set((state) => ({
          batchSelection: state.batchSelection.includes(photoId)
            ? state.batchSelection.filter((id) => id !== photoId)
            : [...state.batchSelection, photoId],
        })),
      setBatchSelection: (batchSelection) => set({ batchSelection }),
      setBatchSnapshot: (batchSnapshot) => set({ batchSnapshot }),
      setBatchOutputDir: (batchOutputDir) => set({ batchOutputDir }),
      setBatchNaming: (batchNaming) => set({ batchNaming }),
      setBatchStartIndex: (batchStartIndex) =>
        set({ batchStartIndex: Math.max(1, Math.round(batchStartIndex || 1)) }),
      setBatchConflict: (batchConflict) => set({ batchConflict }),
      setBatchSettings: (batchSettings) => set({ batchSettings }),
      applyExternalParams: (photoId, next, label) => {
        const state = get();
        const current = state.params[photoId];
        if (!current) return;
        const clamped = clampParams(cloneParams(next));
        if (paramsEqual(current, clamped)) return;
        set({
          params: { ...state.params, [photoId]: clamped },
          history: {
            ...state.history,
            [photoId]: pushHistory(state.history[photoId], current, label, Date.now()),
          },
          lastEdit: null,
        });
      },

      addPhotos: (items) => {
        if (items.length === 0) return;
        const state = get();
        const known = new Set(state.photos.map((photo) => photo.id));
        const fresh = items.filter((item) => !known.has(item.id));
        const firstFresh = fresh[0];
        if (!firstFresh) return;

        const params = { ...state.params };
        const history = { ...state.history };
        for (const photo of fresh) {
          params[photo.id] = DEFAULT_PARAMS;
          history[photo.id] = createHistory<EditParams>();
        }

        const hadActive = Boolean(state.activeId);
        set({
          photos: [...state.photos, ...fresh],
          params,
          history,
          activeId: state.activeId ?? firstFresh.id,
          autoFit: hadActive ? state.autoFit : true,
          pan: hadActive ? state.pan : { x: 0, y: 0 },
          faceStatus: "idle",
          beautyStatus: "idle",
          beautyError: null,
          maskPreview: null,
          maskVisible: false,
          brushMode: "off",
        });
      },

      removePhoto: (id) => {
        const state = get();
        const index = state.photos.findIndex((photo) => photo.id === id);
        if (index === -1) return;
        const photos = state.photos.filter((photo) => photo.id !== id);
        const params = { ...state.params };
        const history = { ...state.history };
        const renderedPreviews = { ...state.renderedPreviews };
        const faces = { ...state.faces };
        const beautyAnalysis = { ...state.beautyAnalysis };
        delete params[id];
        delete history[id];
        delete renderedPreviews[id];
        delete faces[id];
        delete beautyAnalysis[id];

        let activeId = state.activeId;
        if (activeId === id) {
          const fallback = photos[index] ?? photos[index - 1] ?? null;
          activeId = fallback ? fallback.id : null;
        }

        // 浏览器模式导入的照片用 blob: 预览地址，移除时必须释放，否则整页生命周期内泄漏。
        const removed = state.photos[index];
        for (const url of [removed?.thumbUrl, removed?.previewUrl]) {
          if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
        }

        set({
          photos,
          params,
          history,
          renderedPreviews,
          faces,
          beautyAnalysis,
          activeId,
          autoFit: true,
          pan: { x: 0, y: 0 },
          lastEdit: null,
        });
      },

      setActive: (id) => {
        const state = get();
        if (state.activeId === id || !state.photos.some((photo) => photo.id === id)) return;
        set({
          activeId: id,
          autoFit: true,
          pan: { x: 0, y: 0 },
          lastEdit: null,
          renderError: null,
          faceStatus: state.faces[id] ? "ready" : "idle",
          faceError: null,
          activeFace: 0,
          maskPreview: null,
          maskError: null,
          maskVisible: false,
          brushMode: "off",
          beautyStatus: state.beautyAnalysis[id] ? "ready" : "idle",
          beautyError: null,
          beautyProgress: null,
        });
      },

      loadProject: (items, project) => {
        if (items.length === 0) return;
        const byPath = new Map(items.map((item) => [item.path, item]));
        const params: Record<string, EditParams> = {};
        const history: Record<string, HistoryStack<EditParams>> = {};
        const projectFaceRecords: Record<string, ProjectFaceRecord[]> = {};
        const projectSourceHashes: Record<string, string> = {};
        const rawByPhoto: Record<string, RawDevelopParams> = {};
        let loadedColor: ColorOutputSettings | null = null;
        for (const item of items) {
          params[item.id] = DEFAULT_PARAMS;
          history[item.id] = createHistory<EditParams>();
        }
        for (const entry of project.photos) {
          const item = byPath.get(entry.path);
          if (!item) continue;
          params[item.id] = clampParams(entry.params);
          projectFaceRecords[item.id] = entry.faces ?? [];
          projectSourceHashes[item.id] = entry.sourceHash ?? "";
          if (entry.raw?.params) rawByPhoto[item.id] = { ...DEFAULT_RAW_PARAMS, ...entry.raw.params };
          if (!loadedColor && entry.color?.output) loadedColor = entry.color.output;
        }
        const active = items[clamp(project.activeIndex ?? 0, 0, items.length - 1)] ?? items[0]!;
        set({
          photos: items,
          params,
          history,
          activeId: active.id,
          exportSettings: project.export ?? DEFAULT_EXPORT,
          rawByPhoto,
          rawStatus: {},
          rawError: null,
          rawDirty: false,
          ...(loadedColor ? { colorSettings: loadedColor } : {}),
          computePreference: project.compute?.preference ?? "auto",
          projectFaceRecords,
          projectSourceHashes,
          renderedPreviews: {},
          faces: {},
          faceStatus: "idle",
          faceError: null,
          pendingFaceMatches: {},
          missingFaceRecords: {},
          renderError: null,
          autoFit: true,
          pan: { x: 0, y: 0 },
          lastEdit: null,
          beautyAnalysis: {},
          beautyStatus: "idle",
          beautyError: null,
          beautyProgress: null,
          maskVisible: false,
          maskPreview: null,
          maskStatus: "idle",
          maskError: null,
          brushMode: "off",
        });
      },

      setProjectPath: (path) => set({ projectPath: path }),

      updateParam: (key, value) => {
        apply((current) => ({ ...current, [key]: value }), PARAM_LABELS[key], key);
      },

      updateParams: (updater, label, coalesceKey) => apply(updater, label, coalesceKey),

      updateHsl: (channel, field, value) => {
        apply(
          (current) => ({
            ...current,
            hsl: {
              ...current.hsl,
              [channel]: { ...current.hsl[channel], [field]: clamp(value, -100, 100) },
            },
          }),
          "HSL",
          `hsl:${channel}:${field}`,
        );
      },

      setCurve: (channel, points) => {
        apply(
          (current) => ({ ...current, curves: { ...current.curves, [channel]: points } }),
          "曲线",
          `curve:${channel}`,
        );
      },

      resetCurve: (channel) => {
        apply(
          (current) => ({ ...current, curves: { ...current.curves, [channel]: [] } }),
          "重置曲线",
        );
      },

      setLut: (lut) => {
        apply((current) => ({ ...current, lut }), lut ? "应用 LUT" : "移除 LUT");
      },

      setExportSettings: (settings) => {
        set({ exportSettings: { ...settings } });
      },

      resetGroup: (groupId) => {
        const group = PARAM_GROUPS.find((item) => item.id === groupId);
        if (!group) return;
        const current = selectActiveParams(get());
        const next = cloneParams(current);
        for (const definition of group.params) {
          next[definition.key] = DEFAULT_PARAMS[definition.key];
        }
        commit(`重置${group.label}`, next);
      },

      resetAll: () => {
        commit("重置全部参数", DEFAULT_PARAMS);
      },

      copyParams: () => {
        const state = get();
        if (!state.activeId) return;
        set({ clipboard: cloneParams(selectActiveParams(state)) });
        get().notify("success", "已复制当前照片参数");
      },

      pasteParams: () => {
        const clipboard = get().clipboard;
        if (!clipboard) {
          get().notify("info", "剪贴板中没有可粘贴的参数");
          return;
        }
        apply(() => cloneParams(clipboard), "粘贴参数");
        get().notify("success", "已粘贴参数");
      },

      undo: () => {
        const state = get();
        const photoId = state.activeId;
        if (!photoId) return;
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const step = undoHistory(state.history[photoId], current);
        if (!step) return;
        set({
          params: { ...state.params, [photoId]: step.state },
          history: { ...state.history, [photoId]: step.history },
          lastEdit: null,
        });
      },

      redo: () => {
        const state = get();
        const photoId = state.activeId;
        if (!photoId) return;
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const step = redoHistory(state.history[photoId], current);
        if (!step) return;
        set({
          params: { ...state.params, [photoId]: step.state },
          history: { ...state.history, [photoId]: step.history },
          lastEdit: null,
        });
      },

      setCompareMode: (mode) => set({ compareMode: mode }),
      toggleCompare: () => set({ compareMode: get().compareMode === "split" ? "off" : "split" }),
      setCompareSplit: (value) => set({ compareSplit: clamp(value, 0.02, 0.98) }),
      setHoldOriginal: (value) => set({ holdOriginal: value }),

      setViewport: (size) => {
        const state = get();
        set({
          viewport: size,
          pan: clampPan(state.pan, activePhoto(), state.zoom, size),
        });
      },

      setFitZoom: (value) => {
        const safe = clamp(value, ZOOM_MIN, ZOOM_MAX);
        if (get().autoFit) {
          set({ fitZoom: safe, zoom: safe, pan: { x: 0, y: 0 } });
        } else {
          set({ fitZoom: safe });
        }
      },

      setZoom: (value, anchor) => {
        const state = get();
        const next = clamp(value, ZOOM_MIN, ZOOM_MAX);
        if (Math.abs(next - state.zoom) < 0.0001) return;
        let pan = state.pan;
        if (anchor) {
          const ratio = next / state.zoom;
          pan = {
            x: anchor.x - (anchor.x - pan.x) * ratio,
            y: anchor.y - (anchor.y - pan.y) * ratio,
          };
        }
        set({
          zoom: next,
          pan: clampPan(pan, activePhoto(), next, state.viewport),
          autoFit: false,
        });
      },

      zoomStep: (direction) => {
        const state = get();
        state.setZoom(state.zoom * (direction > 0 ? 1.3 : 1 / 1.3));
      },

      setPan: (pan) => {
        const state = get();
        set({ pan: clampPan(pan, activePhoto(), state.zoom, state.viewport) });
      },

      fitToWindow: () => set({ zoom: get().fitZoom, pan: { x: 0, y: 0 }, autoFit: true }),
      actualPixels: () => {
        set({ autoFit: false });
        get().setZoom(1);
      },

      markRendered: (photoId, url, key, elapsedMs, extra) =>
        set((state) => ({
          renderedPreviews: { ...state.renderedPreviews, [photoId]: { url, key, elapsedMs } },
          renderStatus: "ready",
          renderElapsedMs: elapsedMs,
          renderError: null,
          renderProgress: null,
          renderWarnings: extra?.warnings ?? [],
          reshapeReports: extra?.reshape ?? [],
          bodyReports: extra?.body ?? [],
          backgroundReport: extra?.background ?? null,
          maskReport: extra?.mask ?? null,
        })),

      setRenderStatus: (status) => set({ renderStatus: status }),
      setRenderError: (message) =>
        set({ renderStatus: "error", renderError: message, renderElapsedMs: null, renderProgress: null }),
      setRenderProgress: (progress) => set({ renderProgress: progress }),

      setFaces: (photoId, result) => {
        const state = get();
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        // 检测结果与参数按稳定 UUID 对齐；UUID 不可用时回退旧 faceId。
        const beauty = result.faces
          .map((face) => {
            const matched =
              current.faces.find((entry) => entry.faceUuid && entry.faceUuid === face.uuid) ??
              current.faces.find((entry) => entry.faceId === face.id) ??
              defaultFaceBeauty(face.id, face.uuid);
            return { ...matched, faceId: face.id, faceUuid: face.uuid || matched.faceUuid };
          })
          .sort((a, b) => a.faceId - b.faceId);
        const reshape = result.faces
          .map((face) => {
            const matched =
              current.reshape.find((entry) => entry.faceUuid && entry.faceUuid === face.uuid) ??
              current.reshape.find((entry) => entry.faceId === face.id) ??
              defaultFaceReshape(face.id, face.uuid);
            return { ...matched, faceId: face.id, faceUuid: face.uuid || matched.faceUuid };
          })
          .sort((a, b) => a.faceId - b.faceId);
        set({
          faces: { ...state.faces, [photoId]: result },
          params: { ...state.params, [photoId]: { ...current, faces: beauty, reshape } },
          faceStatus: "ready",
          faceError: null,
          faceProgress: null,
          activeFace: 0,
          showFaceOverlay: true,
          maskPreview: null,
          maskError: null,
          brushMode: "off",
        });
      },

      applyFaceRestore: (photoId, result) => {
        const state = get();
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const restored = new Map(result.restored.map((entry) => [entry.faceId, entry.faceUuid]));
        const beauty = result.faces
          .map((face) => {
            const uuid = restored.get(face.id);
            const matched = uuid
              ? current.faces.find((entry) => entry.faceUuid === uuid)
              : undefined;
            return {
              ...(matched ?? defaultFaceBeauty(face.id, face.uuid)),
              faceId: face.id,
              faceUuid: uuid ?? face.uuid,
            };
          })
          .sort((a, b) => a.faceId - b.faceId);
        const reshape = result.faces
          .map((face) => {
            const uuid = restored.get(face.id);
            const matched = uuid
              ? current.reshape.find((entry) => entry.faceUuid === uuid)
              : undefined;
            return {
              ...(matched ?? defaultFaceReshape(face.id, face.uuid)),
              faceId: face.id,
              faceUuid: uuid ?? face.uuid,
            };
          })
          .sort((a, b) => a.faceId - b.faceId);
        const previous = state.faces[photoId];
        if (!previous) return;
        set({
          faces: {
            ...state.faces,
            [photoId]: { ...previous, faces: result.faces },
          },
          params: { ...state.params, [photoId]: { ...current, faces: beauty, reshape } },
          pendingFaceMatches: { ...state.pendingFaceMatches, [photoId]: result.pending },
          missingFaceRecords: { ...state.missingFaceRecords, [photoId]: result.missing },
          faceStatus: "ready",
          faceProgress: null,
          faceError: null,
          activeFace: 0,
        });
      },

      applyConfirmedFaces: (photoId, faces) => {
        const state = get();
        const result = state.faces[photoId];
        if (!result) return;
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const byUuid = new Map(faces.map((face) => [face.uuid, face.id]));
        set({
          faces: { ...state.faces, [photoId]: { ...result, faces } },
          params: {
            ...state.params,
            [photoId]: {
              ...current,
              faces: current.faces.map((entry) => ({
                ...entry,
                faceId: byUuid.get(entry.faceUuid) ?? entry.faceId,
              })),
              reshape: current.reshape.map((entry) => ({
                ...entry,
                faceId: byUuid.get(entry.faceUuid) ?? entry.faceId,
              })),
            },
          },
          pendingFaceMatches: { ...state.pendingFaceMatches, [photoId]: [] },
        });
      },

      setWarpTool: (tool) =>
        set({
          warpTool: tool,
          brushMode: tool === "off" ? get().brushMode : "off",
          bodyTool: tool === "off" ? get().bodyTool : "off",
          subjectBrush: tool === "off" ? get().subjectBrush : "off",
        }),
      setWarpRadius: (radius) => set({ warpRadius: clamp(radius, 0.004, 0.25) }),
      setWarpStrength: (value) => set({ warpStrength: clamp(value, 0, 100) }),
      setWarpFeather: (value) => set({ warpFeather: clamp(value, 0, 100) }),
      setShowMesh: (value) => set({ showMesh: value }),

      updateFaceShape: (faceId, key, value) => {
        const state = get();
        const face = selectActiveFace(state);
        const uuid = face?.uuid ?? "";
        apply(
          (current) =>
            withFaceReshape(current, faceId, uuid, (entry) => ({
              ...entry,
              face: { ...entry.face, [key]: value },
            })),
          `面部塑形 · ${key}`,
          `reshape:${faceId}:${key}`,
        );
      },

      appendWarpStroke: (faceId, stroke) => {
        const state = get();
        const face = selectActiveFace(state);
        const uuid = face?.uuid ?? "";
        apply(
          (current) =>
            withFaceReshape(current, faceId, uuid, (entry) => ({
              ...entry,
              manual: {
                enabled: true,
                strokes: [...entry.manual.strokes, stroke].slice(-400),
              },
            })),
          "手动塑形笔触",
        );
      },

      undoWarpStroke: (faceId) =>
        apply(
          (current) =>
            withFaceReshape(current, faceId, "", (entry) => ({
              ...entry,
              manual: { ...entry.manual, strokes: entry.manual.strokes.slice(0, -1) },
            })),
          "撤销塑形笔触",
        ),

      clearWarpStrokes: (faceId) =>
        apply(
          (current) =>
            withFaceReshape(current, faceId, "", (entry) => ({
              ...entry,
              manual: { ...entry.manual, strokes: [] },
            })),
          "清除塑形笔触",
        ),

      resetFaceReshape: (faceId) =>
        apply(
          (current) => ({
            ...current,
            reshape: current.reshape.filter((entry) => entry.faceId !== faceId),
          }),
          "重置该人脸塑形",
        ),

      resetAllReshape: () =>
        apply((current) => ({ ...current, reshape: [] }), "重置全部塑形"),

      copyFaceReshapeToOthers: (faceId) => {
        const state = get();
        const params = selectActiveParams(state);
        const source = findFaceReshape(params, faceId);
        if (!source) return;
        const result = selectActiveFaces(state);
        const targets = (result?.faces ?? []).filter((face) => face.id !== faceId);
        if (targets.length === 0) {
          get().notify("info", "当前照片没有其他可复制的人脸");
          return;
        }
        apply(
          (current) => ({
            ...current,
            reshape: current.reshape.map((entry) => {
              const target = targets.find((face) => face.id === entry.faceId);
              if (!target) return entry;
              return {
                ...entry,
                face: { ...source.face },
                manual: {
                  enabled: source.manual.enabled,
                  strokes: source.manual.strokes.map((stroke) => ({
                    ...stroke,
                    points: stroke.points.map((point) => ({ ...point })),
                  })),
                },
              };
            }),
          }),
          "复制人脸塑形",
        );
        get().notify("success", "已把当前人物塑形参数复制到其他人");
      },
      setFaceStatus: (status) => set({ faceStatus: status }),
      setFaceProgress: (progress) => set({ faceProgress: progress }),
      setFaceError: (message) =>
        set({ faceStatus: "error", faceError: message, faceProgress: null }),
      setActiveFace: (index) => set({ activeFace: Math.max(0, index) }),
      setShowFaceOverlay: (value) => set({ showFaceOverlay: value }),
      setModelStatus: (status) => set({ modelStatus: status }),

      setBeautyStatus: (status) => set({ beautyStatus: status }),
      setBeautyProgress: (progress) => set({ beautyProgress: progress }),
      setBeautyError: (message) =>
        set({ beautyStatus: "error", beautyError: message, beautyProgress: null }),
      setBeautyAnalysis: (photoId, analysis) =>
        set((state) => ({
          beautyAnalysis: { ...state.beautyAnalysis, [photoId]: analysis },
          beautyStatus: "ready",
          beautyError: null,
          beautyProgress: null,
        })),
      setSkinModel: (status) => set({ skinModel: status }),

      updateFaceBeauty: (faceId, key, value) => {
        const uuid = selectActiveFace(get())?.uuid ?? "";
        apply(
          (current) =>
            withFaceBeauty(
              current,
              faceId,
              (face) => ({
                ...face,
                beauty: { ...face.beauty, [key]: value },
              }),
              uuid,
            ),
          BEAUTY_LABELS[key],
          `beauty:${faceId}:${key}`,
        );
      },

      setFaceEnabled: (faceId, enabled) => {
        const uuid = selectActiveFace(get())?.uuid ?? "";
        apply(
          (current) =>
            withFaceBeauty(current, faceId, (face) => ({ ...face, enabled }), uuid),
          enabled ? "启用该人脸" : "停用该人脸",
        );
      },

      resetFaceBeauty: (faceId) => {
        const uuid = selectActiveFace(get())?.uuid ?? "";
        apply(
          (current) =>
            withFaceBeauty(
              current,
              faceId,
              (face) => ({
                ...face,
                beauty: defaultBeauty(),
              }),
              uuid,
            ),
          "重置该人脸",
        );
      },

      applyBeautySuggestion: (faceId, beauty, label) => {
        const uuid = selectActiveFace(get())?.uuid ?? "";
        apply(
          (current) =>
            withFaceBeauty(
              current,
              faceId,
              (face) => ({
                ...face,
                enabled: true,
                beauty: { ...beauty },
              }),
              uuid,
            ),
          label ?? "应用智能建议",
        );
      },

      copyFaceParamsToOthers: (faceId) => {
        const state = get();
        const params = selectActiveParams(state);
        const source = findFaceBeauty(params, faceId);
        if (!source) return;
        const result = selectActiveFaces(state);
        const targetIds = (result?.faces ?? []).map((face) => face.id);
        if (targetIds.length <= 1) {
          get().notify("info", "当前照片只有一张人脸");
          return;
        }
        apply(
          (current) => ({
            ...current,
            faces: current.faces.map((face) =>
              targetIds.includes(face.faceId)
                ? { ...face, beauty: { ...source.beauty } }
                : face,
            ),
          }),
          "复制人脸参数",
        );
        get().notify("success", "已把当前人脸参数复制到其他人物");
      },

      updateManualMask: (updater, label, coalesceKey) =>
        apply(
          (current) => ({ ...current, manualMask: updater(current.manualMask) }),
          label,
          coalesceKey,
        ),

      appendMaskStroke: (stroke) =>
        apply(
          (current) => ({
            ...current,
            manualMask: {
              ...current.manualMask,
              enabled: true,
              strokes: [...current.manualMask.strokes, stroke],
            },
          }),
          "手动蒙版笔触",
        ),

      undoMaskStroke: () =>
        apply(
          (current) => ({
            ...current,
            manualMask: {
              ...current.manualMask,
              strokes: current.manualMask.strokes.slice(0, -1),
            },
          }),
          "撤销笔触",
        ),

      clearMaskStrokes: () =>
        apply(
          (current) => ({
            ...current,
            manualMask: { ...current.manualMask, strokes: [] },
          }),
          "恢复自动蒙版",
        ),

      addRepairPoint: (x, y, radius) =>
        apply(
          (current) => ({
            ...current,
            repairPoints: [...current.repairPoints, { x, y, radius }].slice(-200),
          }),
          "添加修复点",
        ),

      undoRepairPoint: () =>
        apply(
          (current) => ({
            ...current,
            repairPoints: current.repairPoints.slice(0, -1),
          }),
          "撤销修复点",
        ),

      clearRepairPoints: () =>
        apply(
          (current) => ({ ...current, repairPoints: [] }),
          "清除修复点",
        ),

      setMaskVisible: (visible) => set({ maskVisible: visible }),
      setMaskPreview: (preview) =>
        set({ maskPreview: preview, maskStatus: preview ? "ready" : "idle" }),
      setMaskStatus: (status) => set({ maskStatus: status }),
      setMaskError: (message) => set({ maskStatus: "error", maskError: message }),
      setBrushMode: (mode) => set({ brushMode: mode }),
      setBrushRadius: (radius) => set({ brushRadius: clamp(radius, 0.004, 0.2) }),

      setSubjectBrush: (kind) =>
        set({
          subjectBrush: kind,
          warpTool: "off",
          bodyTool: "off",
          brushMode: "off",
        }),
      setSubjectBrushRadius: (radius) => set({ subjectBrushRadius: clamp(radius, 0.002, 0.5) }),
      setSubjectBrushHardness: (value) => set({ subjectBrushHardness: clamp(value, 0, 100) }),
      setSubjectBrushStrength: (value) => set({ subjectBrushStrength: clamp(value, 0, 100) }),
      setSubjectMaskView: (view) => set({ subjectMaskView: view }),
      setShowSubjectMask: (value) => set({ showSubjectMask: value }),
      setSubjectMaskTarget: (target) => set({ subjectMaskTarget: target }),
      updateSubjectMask: (updater, label, coalesceKey) =>
        apply(
          (current) => ({ ...current, subjectMask: updater(current.subjectMask) }),
          label,
          coalesceKey,
        ),
      appendSubjectMaskStroke: (stroke) => {
        const state = get();
        if (state.subjectMaskTarget === "subject") {
          apply(
            (current) =>
              withBodyAdjust(current, state.bodySubjectId, (entry) => ({
                ...entry,
                maskStrokes: [...entry.maskStrokes, stroke].slice(-400),
              })),
            "人物独立蒙版笔触",
          );
          return;
        }
        apply(
          (current) => ({
            ...current,
            subjectMask: {
              ...current.subjectMask,
              enabled: true,
              strokes: [...current.subjectMask.strokes, stroke].slice(-400),
            },
          }),
          "人物蒙版笔触",
        );
      },
      undoSubjectMaskStroke: () => {
        const state = get();
        if (state.subjectMaskTarget === "subject") {
          apply(
            (current) =>
              withBodyAdjust(current, state.bodySubjectId, (entry) => ({
                ...entry,
                maskStrokes: entry.maskStrokes.slice(0, -1),
              })),
            "撤销人物独立蒙版笔触",
          );
          return;
        }
        apply(
          (current) => ({
            ...current,
            subjectMask: { ...current.subjectMask, strokes: current.subjectMask.strokes.slice(0, -1) },
          }),
          "撤销人物蒙版笔触",
        );
      },
      clearSubjectMask: () =>
        apply(
          (current) => ({
            ...current,
            subjectMask: {
              ...current.subjectMask,
              enabled: false,
              strokes: [],
              invert: false,
            },
          }),
          "清除人物蒙版",
        ),
      setSubjectMaskEnabled: (enabled) =>
        apply(
          (current) => ({ ...current, subjectMask: { ...current.subjectMask, enabled } }),
          enabled ? "启用人物蒙版" : "停用人物蒙版",
        ),
      setSubjectMaskPreview: (preview) =>
        set({ subjectMaskPreview: preview, subjectMaskStatus: preview ? "ready" : "idle" }),
      setSubjectMaskStatus: (status) => set({ subjectMaskStatus: status }),
      setSubjectMaskError: (message) =>
        set({ subjectMaskStatus: "error", subjectMaskError: message }),
      setSegmentationCapability: (capability) =>
        set({ segmentationCapability: capability }),

      setBodyTool: (tool) =>
        set({
          bodyTool: tool,
          warpTool: "off",
          subjectBrush: "off",
          brushMode: "off",
        }),
      setBodyRadius: (radius) => set({ bodyRadius: clamp(radius, 0.004, 0.25) }),
      setBodyStrength: (value) => set({ bodyStrength: clamp(value, 0, 100) }),
      setBodyFeather: (value) => set({ bodyFeather: clamp(value, 0, 100) }),
      setBodyAnchorRadius: (radius) => set({ bodyAnchorRadius: clamp(radius, 0.02, 0.5) }),
      setBodySubjectId: (subjectId) =>
        set({ bodySubjectId: subjectId.trim() ? subjectId : "subject-1" }),
      setBodyCapability: (capability) => set({ bodyCapability: capability }),
      setShowBodyAnchors: (value) => set({ showBodyAnchors: value }),
      updateBodyShape: (key, value) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              shape: { ...entry.shape, [key]: value },
            })),
          `身材 · ${key}`,
          `body:${state.bodySubjectId}:${key}`,
        );
      },
      updateBody: (updater, label, coalesceKey) => {
        const state = get();
        apply(
          (current) => withBodyAdjust(current, state.bodySubjectId, updater),
          label,
          coalesceKey,
        );
      },
      addBodyAnchor: (anchor) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              anchors: [...entry.anchors, anchor].slice(-200),
            })),
          `添加锚点 · ${anchor.part}`,
        );
      },
      addBodyAnchorPresets: (presets) => {
        const state = get();
        if (presets.length === 0) return;
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => {
              const anchors = [...entry.anchors];
              for (const preset of presets) {
                if (anchors.some((anchor) => anchor.part === preset.part)) continue;
                anchors.push({
                  ...defaultBodyAnchor(preset.part),
                  x: clamp(preset.x, 0, 1),
                  y: clamp(preset.y, 0, 1),
                  radius: state.bodyAnchorRadius,
                });
              }
              return { ...entry, anchors: anchors.slice(-200) };
            }),
          "添加锚点建议",
        );
      },
      updateBodyAnchor: (index, patch) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              anchors: entry.anchors.map((anchor, i) =>
                i === index ? { ...anchor, ...patch } : anchor,
              ),
            })),
          "编辑锚点",
        );
      },
      removeBodyAnchor: (index) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              anchors: entry.anchors.filter((_, i) => i !== index),
            })),
          "删除锚点",
        );
      },
      lockBodyAnchor: (index, locked) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              anchors: entry.anchors.map((anchor, i) => (i === index ? { ...anchor, locked } : anchor)),
            })),
          locked ? "锁定锚点" : "解锁锚点",
        );
      },
      clearBodyAnchors: () => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({ ...entry, anchors: [] })),
          "清除锚点",
        );
      },
      appendBodyStroke: (stroke) => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              strokes: [...entry.strokes, stroke].slice(-400),
            })),
          "身材推拉笔触",
        );
      },
      undoBodyStroke: () => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({
              ...entry,
              strokes: entry.strokes.slice(0, -1),
            })),
          "撤销身材笔触",
        );
      },
      clearBodyStrokes: () => {
        const state = get();
        apply(
          (current) =>
            withBodyAdjust(current, state.bodySubjectId, (entry) => ({ ...entry, strokes: [] })),
          "清除身材笔触",
        );
      },
      resetBodyAdjust: () => {
        const state = get();
        apply(
          (current) => ({
            ...current,
            body: current.body.filter((entry) => entry.subjectId !== state.bodySubjectId),
          }),
          "重置身材",
        );
      },
      resetBackground: () =>
        apply(
          (current) => ({ ...current, background: defaultBackground() }),
          "重置背景",
        ),
      setBackgroundImage: (info) =>
        apply(
          (current) => ({
            ...current,
            background: info
              ? {
                  ...current.background,
                  enabled: true,
                  mode: "image",
                  imagePath: info.path,
                }
              : { ...current.background, mode: current.background.imagePath ? "image" : "original" },
          }),
          info ? "选择背景图片" : "清除背景图片",
        ),
      updateBackground: (updater, label, coalesceKey) =>
        apply(
          (current) => ({ ...current, background: updater(current.background) }),
          label,
          coalesceKey,
        ),

      setImporting: (value) => set({ importing: value }),
      setExporting: (value) => set({ exporting: value }),
      setDragActive: (value) => set({ dragActive: value }),
      notify: (kind, text) => {
        noticeSeed += 1;
        set({ notice: { id: noticeSeed, kind, text } });
      },
      dismissNotice: (id) => {
        if (get().notice?.id === id) set({ notice: null });
      },
    };
  });
}

export const useEditorStore = createEditorStore();

export const selectActivePhoto = (state: EditorState): PhotoAsset | null =>
  state.photos.find((photo) => photo.id === state.activeId) ?? null;

export const selectActiveParams = (state: EditorState): EditParams =>
  (state.activeId ? state.params[state.activeId] : undefined) ?? DEFAULT_PARAMS;

export const selectCanUndo = (state: EditorState): boolean =>
  Boolean(state.activeId && (state.history[state.activeId]?.past.length ?? 0) > 0);

export const selectCanRedo = (state: EditorState): boolean =>
  Boolean(state.activeId && (state.history[state.activeId]?.future.length ?? 0) > 0);

export const selectActiveFaces = (state: EditorState) =>
  (state.activeId ? state.faces[state.activeId] : undefined) ?? null;

export const selectActiveFace = (state: EditorState) => {
  const result = selectActiveFaces(state);
  if (!result || result.faces.length === 0) return null;
  return result.faces[Math.min(state.activeFace, result.faces.length - 1)] ?? result.faces[0];
};

export const selectActiveBeautyAnalysis = (state: EditorState): BeautyAnalysis | null =>
  (state.activeId ? state.beautyAnalysis[state.activeId] : undefined) ?? null;

/** 当前选中人脸的参数条目；尚未同步时返回 null。 */
export const selectActiveFaceBeauty = (state: EditorState) => {
  const face = selectActiveFace(state);
  if (!face) return null;
  return findFaceBeauty(selectActiveParams(state), face.id) ?? null;
};

/** 当前编辑人物的身材参数条目；没有时返回 null。 */
export const selectActiveBodyAdjust = (state: EditorState) =>
  findBodyAdjust(selectActiveParams(state), state.bodySubjectId) ?? null;

/** 效果图 URL：参数归零时直接复用原图，省掉一次后端渲染。 */
export function selectEffectUrl(state: EditorState): string | null {
  const photo = selectActivePhoto(state);
  if (!photo) return null;
  const params = selectActiveParams(state);
  if (isNeutral(params)) return photo.previewUrl;
  return state.renderedPreviews[photo.id]?.url ?? photo.previewUrl;
}
