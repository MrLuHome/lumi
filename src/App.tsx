import { useCallback, useEffect } from "react";
import { TopBar } from "./components/TopBar";
import { LibraryRail } from "./components/LibraryRail";
import { CanvasStage } from "./components/CanvasStage";
import { Inspector } from "./components/Inspector";
import { StatusBar } from "./components/StatusBar";
import { Toast } from "./components/ui/Toast";
import { usePreviewRenderer } from "./hooks/usePreviewRenderer";
import { exportImage, importPaths, pickImages, subscribeToFileDrop } from "./lib/backend";
import { formatDuration, formatFileSize } from "./lib/format";
import { platformClass } from "./lib/platform";
import {
  selectActiveParams,
  selectActivePhoto,
  selectCanRedo,
  selectCanUndo,
  selectEffectUrl,
  useEditorStore,
} from "./state/editorStore";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败，请重试";
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export default function App() {
  usePreviewRenderer();

  const photos = useEditorStore((state) => state.photos);
  const activeId = useEditorStore((state) => state.activeId);
  const photo = useEditorStore(selectActivePhoto);
  const params = useEditorStore(selectActiveParams);
  const effectUrl = useEditorStore(selectEffectUrl);
  const canUndo = useEditorStore(selectCanUndo);
  const canRedo = useEditorStore(selectCanRedo);
  const compareMode = useEditorStore((state) => state.compareMode);
  const importing = useEditorStore((state) => state.importing);
  const exporting = useEditorStore((state) => state.exporting);

  const handleImport = useCallback(async () => {
    const state = useEditorStore.getState();
    if (state.importing) return;
    state.setImporting(true);
    try {
      const items = await pickImages();
      if (items.length > 0) {
        useEditorStore.getState().addPhotos(items);
        useEditorStore.getState().notify("success", `已导入 ${items.length} 张照片`);
      }
    } catch (error) {
      useEditorStore.getState().notify("error", toMessage(error));
    } finally {
      useEditorStore.getState().setImporting(false);
    }
  }, []);

  const handleExport = useCallback(async () => {
    const state = useEditorStore.getState();
    const active = selectActivePhoto(state);
    if (!active || state.exporting) return;
    state.setExporting(true);
    try {
      const result = await exportImage(active.path, selectActiveParams(state));
      useEditorStore
        .getState()
        .notify(
          "success",
          `已导出 ${baseName(result.path)} · ${formatFileSize(result.bytes)} · ${formatDuration(
            result.elapsedMs,
          )}`,
        );
    } catch (error) {
      const message = toMessage(error);
      // 用户在保存对话框点取消，不弹错误。
      if (!message.includes("取消")) useEditorStore.getState().notify("error", message);
    } finally {
      useEditorStore.getState().setExporting(false);
    }
  }, []);

  const handleUndo = useCallback(() => useEditorStore.getState().undo(), []);
  const handleRedo = useCallback(() => useEditorStore.getState().redo(), []);
  const handleToggleCompare = useCallback(() => useEditorStore.getState().toggleCompare(), []);
  const handleResetAll = useCallback(() => useEditorStore.getState().resetAll(), []);
  const handleFit = useCallback(() => useEditorStore.getState().fitToWindow(), []);
  const handleActualPixels = useCallback(() => useEditorStore.getState().actualPixels(), []);

  // 桌面端文件拖拽导入
  useEffect(() => {
    let alive = true;
    let unsubscribe: (() => void) | undefined;
    void subscribeToFileDrop({
      onEnter: () => useEditorStore.getState().setDragActive(true),
      onLeave: () => useEditorStore.getState().setDragActive(false),
      onDrop: async (paths) => {
        const state = useEditorStore.getState();
        state.setDragActive(false);
        try {
          const items = await importPaths(paths);
          if (items.length === 0) {
            state.notify("error", "没有找到可导入的图片（支持 JPG / PNG / WebP）");
            return;
          }
          useEditorStore.getState().addPhotos(items);
          useEditorStore.getState().notify("success", `已导入 ${items.length} 张照片`);
        } catch (error) {
          useEditorStore.getState().notify("error", toMessage(error));
        }
      },
    }).then((cleanup) => {
      if (alive) unsubscribe = cleanup;
      else cleanup();
    });
    return () => {
      alive = false;
      unsubscribe?.();
    };
  }, []);

  // 全局快捷键
  useEffect(() => {
    const isTyping = (target: EventTarget | null) =>
      target instanceof HTMLElement &&
      (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);

    const onKeyDown = (event: KeyboardEvent) => {
      const state = useEditorStore.getState();
      const mod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();

      if (mod && key === "z") {
        event.preventDefault();
        if (event.shiftKey) state.redo();
        else state.undo();
        return;
      }
      if (mod && key === "o") {
        event.preventDefault();
        void handleImport();
        return;
      }
      if (mod && key === "e") {
        event.preventDefault();
        void handleExport();
        return;
      }
      if (isTyping(event.target)) return;

      switch (event.key) {
        case "\\":
          event.preventDefault();
          state.setHoldOriginal(true);
          break;
        case "f":
        case "0":
          state.fitToWindow();
          break;
        case "1":
          state.actualPixels();
          break;
        case "=":
        case "+":
          state.zoomStep(1);
          break;
        case "-":
          state.zoomStep(-1);
          break;
        case "Escape":
          state.setCompareMode("off");
          break;
        default:
          break;
      }
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === "\\") useEditorStore.getState().setHoldOriginal(false);
    };
    const onBlur = () => useEditorStore.getState().setHoldOriginal(false);

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [handleImport, handleExport]);

  return (
    <div className={`app ${platformClass()}`}>
      <TopBar
        photo={photo}
        canUndo={canUndo}
        canRedo={canRedo}
        compareMode={compareMode}
        importing={importing}
        exporting={exporting}
        onImport={handleImport}
        onExport={handleExport}
        onUndo={handleUndo}
        onRedo={handleRedo}
        onToggleCompare={handleToggleCompare}
        onResetAll={handleResetAll}
        onFit={handleFit}
        onActualPixels={handleActualPixels}
      />

      <div className="workspace">
        <LibraryRail photos={photos} activeId={activeId} onImport={handleImport} />
        <CanvasStage photo={photo} effectUrl={effectUrl} onImport={handleImport} />
        <Inspector photo={photo} params={params} />
      </div>

      <StatusBar photo={photo} />
      <Toast />
    </div>
  );
}
