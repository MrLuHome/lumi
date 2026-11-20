import { useEffect } from "react";
import { renderPreview } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { isNeutral, paramsKey } from "../state/params";
import { selectActiveParams, useEditorStore } from "../state/editorStore";

/**
 * 参数变化 → 防抖调用 Rust 管线 → 写回渲染结果。
 *
 * - 参数归零时直接复用原图，不触发后端；
 * - 只接受与当前参数一致的响应，避免快速拖动时旧结果覆盖新结果；
 * - 浏览器预览模式下不发请求（界面仍可完整操作）。
 */
export function usePreviewRenderer(): void {
  const photo = useEditorStore((state) =>
    state.photos.find((item) => item.id === state.activeId),
  );
  const params = useEditorStore(selectActiveParams);
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!photo) return;
    const store = useEditorStore.getState();
    const key = paramsKey(params);

    if (!desktop) {
      store.setRenderStatus("idle");
      return;
    }

    if (isNeutral(params)) {
      store.markRendered(photo.id, photo.previewUrl, key, 0);
      return;
    }

    const cached = store.renderedPreviews[photo.id];
    if (cached?.key === key) return;

    let cancelled = false;
    store.setRenderStatus("rendering");

    const timer = window.setTimeout(async () => {
      try {
        const result = await renderPreview(photo.path, params);
        if (cancelled) return;
        const current = useEditorStore.getState();
        if (current.activeId !== photo.id) return;
        const liveParams = current.params[photo.id];
        if (!liveParams || paramsKey(liveParams) !== key) return;
        current.markRendered(photo.id, result.previewUrl, key, result.elapsedMs);
      } catch (error) {
        if (cancelled) return;
        const current = useEditorStore.getState();
        if (current.activeId !== photo.id) return;
        current.setRenderError(error instanceof Error ? error.message : String(error));
      }
    }, 70);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [photo, params, desktop]);
}
