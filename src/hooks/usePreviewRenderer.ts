import { useEffect, useRef } from "react";
import { renderPreview, cancelRender, subscribeRenderProgress } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { isNeutral, paramsKey } from "../state/params";
import {
  DETAIL_ZOOM_THRESHOLD,
  selectActiveParams,
  useEditorStore,
} from "../state/editorStore";

/** 高清预览请求的最大边：屏幕放大到 100% 以上时使用。 */
const DETAIL_MAX_EDGE = 3200;

/**
 * 参数变化 → 防抖调用 Rust 管线 → 写回渲染结果。
 *
 * - 参数归零且适应窗口时直接复用原图，不触发后端；
 * - 缩放超过 100% 时请求 3200px 高清预览；
 * - 每次请求带唯一 requestId，只接受最新结果，旧请求在后端取消；
 * - 浏览器预览模式下不发请求（界面仍可完整操作）。
 */
export function usePreviewRenderer(): void {
  const photo = useEditorStore((state) =>
    state.photos.find((item) => item.id === state.activeId),
  );
  const params = useEditorStore(selectActiveParams);
  const zoom = useEditorStore((state) => state.zoom);
  const desktop = isDesktopRuntime();
  const requestSeq = useRef(0);
  const activeRequest = useRef(0);

  useEffect(() => {
    if (!desktop) return;
    let dispose: (() => void) | undefined;
    subscribeRenderProgress((progress) => {
      if (progress.requestId === activeRequest.current) {
        useEditorStore.getState().setRenderProgress(progress);
      }
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, [desktop]);

  useEffect(() => {
    if (!photo) return;
    const store = useEditorStore.getState();
    const key = paramsKey(params);

    if (!desktop) {
      store.setRenderStatus("idle");
      return;
    }

    const detail = zoom >= DETAIL_ZOOM_THRESHOLD;
    const maxEdge = detail ? DETAIL_MAX_EDGE : undefined;
    const cacheKey = `${key}|${maxEdge ?? "fit"}`;

    if (isNeutral(params) && !detail) {
      store.markRendered(photo.id, photo.previewUrl, cacheKey, 0);
      return;
    }

    const cached = store.renderedPreviews[photo.id];
    if (cached?.key === cacheKey) return;

    requestSeq.current += 1;
    const requestId = requestSeq.current;
    activeRequest.current = requestId;
    let disposed = false;
    store.setRenderStatus("rendering");
    store.setRenderProgress({ requestId, stage: "排队", progress: 0 });

    const timer = window.setTimeout(async () => {
      try {
        const result = await renderPreview(photo.path, params, requestId, maxEdge);
        if (disposed || requestId !== activeRequest.current) return;
        const current = useEditorStore.getState();
        if (current.activeId !== photo.id) return;
        const liveParams = current.params[photo.id];
        if (!liveParams || paramsKey(liveParams) !== key) return;
        current.markRendered(photo.id, result.previewUrl, cacheKey, result.elapsedMs, {
          warnings: result.warnings,
          reshape: result.reshape,
          body: result.body,
          background: result.background,
          mask: result.mask,
        });
      } catch (error) {
        if (disposed || requestId !== activeRequest.current) return;
        const current = useEditorStore.getState();
        if (current.activeId !== photo.id) return;
        current.setRenderError(error instanceof Error ? error.message : String(error));
      }
    }, 70);

    return () => {
      disposed = true;
      window.clearTimeout(timer);
      // 后端只会保留最新任务，这里显式取消可避免排队中的旧任务浪费 CPU。
      void cancelRender(requestId);
    };
  }, [photo, params, zoom, desktop]);
}
