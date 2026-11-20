import { create } from "zustand";
import type {
  EditParams,
  ExportSettings,
  FaceDetectionResult,
  FaceModelStatus,
  FaceProgress,
  PhotoAsset,
  ProjectDoc,
  RenderProgress,
} from "../types";
import {
  DEFAULT_EXPORT,
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_LABELS,
  type CurveChannel,
  type CurvePoint,
  type HslKey,
  clampParams,
  cloneParams,
  isNeutral,
  paramsEqual,
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
        delete params[id];
        delete history[id];
        delete renderedPreviews[id];
        delete faces[id];

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

      setFaces: (photoId, result) =>
        set({
          faces: { ...get().faces, [photoId]: result },
          faceStatus: "ready",
          faceError: null,
          faceProgress: null,
          activeFace: 0,
          showFaceOverlay: true,
        }),
      setFaceStatus: (status) => set({ faceStatus: status }),
      setFaceProgress: (progress) => set({ faceProgress: progress }),
      setFaceError: (message) =>
        set({ faceStatus: "error", faceError: message, faceProgress: null }),
      setActiveFace: (index) => set({ activeFace: Math.max(0, index) }),
      setShowFaceOverlay: (value) => set({ showFaceOverlay: value }),
      setModelStatus: (status) => set({ modelStatus: status }),

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

/** 效果图 URL：参数归零时直接复用原图，省掉一次后端渲染。 */
export function selectEffectUrl(state: EditorState): string | null {
  const photo = selectActivePhoto(state);
  if (!photo) return null;
  const params = selectActiveParams(state);
  if (isNeutral(params)) return photo.previewUrl;
  return state.renderedPreviews[photo.id]?.url ?? photo.previewUrl;
}
