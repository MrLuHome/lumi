import { create } from "zustand";
import type {
  BeautyAnalysis,
  BeautyParams,
  BeautyProgress,
  EditParams,
  ExportSettings,
  FaceDetectionResult,
  FaceModelStatus,
  FaceProgress,
  MaskPreview,
  MaskStroke,
  PhotoAsset,
  ProjectDoc,
  RenderProgress,
  SkinModelStatus,
} from "../types";
import {
  BEAUTY_LABELS,
  DEFAULT_EXPORT,
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_LABELS,
  type BeautyKey,
  type CurveChannel,
  type CurvePoint,
  type HslKey,
  type ManualMask,
  clampParams,
  cloneParams,
  defaultBeauty,
  defaultFaceBeauty,
  findFaceBeauty,
  isNeutral,
  paramsEqual,
  withFaceBeauty,
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

  importing: boolean;
  exporting: boolean;
  dragActive: boolean;
  notice: Notice | null;

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

  markRendered: (photoId: string, url: string, key: string, elapsedMs: number) => void;
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

      importing: false,
      exporting: false,
      dragActive: false,
      notice: null,

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
        for (const item of items) {
          params[item.id] = DEFAULT_PARAMS;
          history[item.id] = createHistory<EditParams>();
        }
        for (const entry of project.photos) {
          const item = byPath.get(entry.path);
          if (!item) continue;
          params[item.id] = clampParams(entry.params);
        }
        const active = items[clamp(project.activeIndex ?? 0, 0, items.length - 1)] ?? items[0]!;
        set({
          photos: items,
          params,
          history,
          activeId: active.id,
          exportSettings: project.export ?? DEFAULT_EXPORT,
          renderedPreviews: {},
          faces: {},
          faceStatus: "idle",
          faceError: null,
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

      markRendered: (photoId, url, key, elapsedMs) =>
        set((state) => ({
          renderedPreviews: { ...state.renderedPreviews, [photoId]: { url, key, elapsedMs } },
          renderStatus: "ready",
          renderElapsedMs: elapsedMs,
          renderError: null,
          renderProgress: null,
        })),

      setRenderStatus: (status) => set({ renderStatus: status }),
      setRenderError: (message) =>
        set({ renderStatus: "error", renderError: message, renderElapsedMs: null, renderProgress: null }),
      setRenderProgress: (progress) => set({ renderProgress: progress }),

      setFaces: (photoId, result) => {
        const state = get();
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const existing = new Map(current.faces.map((face) => [face.faceId, face]));
        // 检测结果与参数按稳定 ID 对齐：保留已有参数，新增人脸补默认项。
        const faces = result.faces
          .map((face) => existing.get(face.id) ?? defaultFaceBeauty(face.id))
          .sort((a, b) => a.faceId - b.faceId);
        set({
          faces: { ...state.faces, [photoId]: result },
          params: { ...state.params, [photoId]: { ...current, faces } },
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

      updateFaceBeauty: (faceId, key, value) =>
        apply(
          (current) =>
            withFaceBeauty(current, faceId, (face) => ({
              ...face,
              beauty: { ...face.beauty, [key]: value },
            })),
          BEAUTY_LABELS[key],
          `beauty:${faceId}:${key}`,
        ),

      setFaceEnabled: (faceId, enabled) =>
        apply(
          (current) =>
            withFaceBeauty(current, faceId, (face) => ({ ...face, enabled })),
          enabled ? "启用该人脸" : "停用该人脸",
        ),

      resetFaceBeauty: (faceId) =>
        apply(
          (current) =>
            withFaceBeauty(current, faceId, (face) => ({
              ...face,
              beauty: defaultBeauty(),
            })),
          "重置该人脸",
        ),

      applyBeautySuggestion: (faceId, beauty, label) =>
        apply(
          (current) =>
            withFaceBeauty(current, faceId, (face) => ({
              ...face,
              enabled: true,
              beauty: { ...beauty },
            })),
          label ?? "应用 AI 建议",
        ),

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

/** 效果图 URL：参数归零时直接复用原图，省掉一次后端渲染。 */
export function selectEffectUrl(state: EditorState): string | null {
  const photo = selectActivePhoto(state);
  if (!photo) return null;
  const params = selectActiveParams(state);
  if (isNeutral(params)) return photo.previewUrl;
  return state.renderedPreviews[photo.id]?.url ?? photo.previewUrl;
}
