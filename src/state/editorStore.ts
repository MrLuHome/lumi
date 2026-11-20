import { create } from "zustand";
import type { EditParams, PhotoAsset } from "../types";
import {
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_LABELS,
  clampParams,
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

/** 同一滑杆连续拖动产生的更新合并为一条历史记录。 */
export const COALESCE_WINDOW_MS = 600;
export const ZOOM_MIN = 0.02;
export const ZOOM_MAX = 4;

export type RenderStatus = "idle" | "rendering" | "ready" | "error";
export type CompareMode = "off" | "split";

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

export interface EditorState {
  photos: PhotoAsset[];
  activeId: string | null;
  params: Record<string, EditParams>;
  history: Record<string, HistoryStack<EditParams>>;
  lastEdit: { photoId: string; key: string; at: number } | null;

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

  importing: boolean;
  exporting: boolean;
  dragActive: boolean;
  notice: Notice | null;

  addPhotos: (items: PhotoAsset[]) => void;
  removePhoto: (id: string) => void;
  setActive: (id: string) => void;

  updateParam: (key: keyof EditParams, value: number) => void;
  resetGroup: (groupId: string) => void;
  resetAll: () => void;
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

    return {
      photos: [],
      activeId: null,
      params: {},
      history: {},
      lastEdit: null,

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
        delete params[id];
        delete history[id];
        delete renderedPreviews[id];

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
        });
      },

      updateParam: (key, value) => {
        const state = get();
        const photoId = state.activeId;
        if (!photoId) return;
        const current = state.params[photoId] ?? DEFAULT_PARAMS;
        const next = clampParams({ ...current, [key]: value });
        if (paramsEqual(current, next)) return;

        const now = Date.now();
        const last = state.lastEdit;
        const shouldStartEntry =
          !last || last.photoId !== photoId || last.key !== key || now - last.at > COALESCE_WINDOW_MS;

        set({
          params: { ...state.params, [photoId]: next },
          history: shouldStartEntry
            ? {
                ...state.history,
                [photoId]: pushHistory(state.history[photoId], current, PARAM_LABELS[key], now),
              }
            : state.history,
          lastEdit: { photoId, key, at: now },
        });
      },

      resetGroup: (groupId) => {
        const group = PARAM_GROUPS.find((item) => item.id === groupId);
        if (!group) return;
        const state = get();
        const current = state.activeId ? state.params[state.activeId] ?? DEFAULT_PARAMS : DEFAULT_PARAMS;
        const next = { ...current };
        for (const definition of group.params) {
          next[definition.key] = DEFAULT_PARAMS[definition.key];
        }
        commit(`重置${group.label}`, next);
      },

      resetAll: () => {
        commit("重置全部参数", DEFAULT_PARAMS);
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
        })),

      setRenderStatus: (status) => set({ renderStatus: status }),
      setRenderError: (message) =>
        set({ renderStatus: "error", renderError: message, renderElapsedMs: null }),

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

/** 效果图 URL：参数归零时直接复用原图，省掉一次后端渲染。 */
export function selectEffectUrl(state: EditorState): string | null {
  const photo = selectActivePhoto(state);
  if (!photo) return null;
  const params = selectActiveParams(state);
  if (isNeutral(params)) return photo.previewUrl;
  return state.renderedPreviews[photo.id]?.url ?? photo.previewUrl;
}
