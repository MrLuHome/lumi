import { useCallback, useEffect } from "react";
import { TopBar } from "./components/TopBar";
import { LibraryRail } from "./components/LibraryRail";
import { CanvasStage } from "./components/CanvasStage";
import { Inspector } from "./components/Inspector";
import { SmartPanel } from "./components/SmartPanel";
import { PresetPanel } from "./components/PresetPanel";
import { BatchWorkspace } from "./components/BatchWorkspace";
import { RawPanel } from "./components/RawPanel";
import { ColorPanel } from "./components/ColorPanel";
import { PerformancePanel } from "./components/PerformancePanel";
import { AiPortraitPanel } from "./components/AiPortraitPanel";
import { ModelHubPanel } from "./components/ModelHubPanel";
import { StatusBar } from "./components/StatusBar";
import { Toast } from "./components/ui/Toast";
import { usePreviewRenderer } from "./hooks/usePreviewRenderer";
import {
  detectFaces,
  exportImage,
  importPaths,
  openProject,
  pickImages,
  restoreFaces,
  saveProject,
  subscribeBeautyProgress,
  subscribeFaceProgress,
  subscribeToFileDrop,
} from "./lib/backend";
import { formatDuration, formatFileSize } from "./lib/format";
import { platformClass } from "./lib/platform";
import { devLog, safeName } from "./lib/diagnostics";
import {
  selectActiveParams,
  selectActivePhoto,
  selectCanRedo,
  selectCanUndo,
  selectEffectUrl,
  useEditorStore,
} from "./state/editorStore";
import { buildProjectDoc, buildProjectSnapshot, computeProjectId } from "./state/project";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败，请重试";
}

function isCancelled(message: string): boolean {
  return message.includes("取消");
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
  const canPaste = useEditorStore((state) => Boolean(state.clipboard));
  const compareMode = useEditorStore((state) => state.compareMode);
  const importing = useEditorStore((state) => state.importing);
  const exporting = useEditorStore((state) => state.exporting);
  const projectPath = useEditorStore((state) => state.projectPath);
  const workspace = useEditorStore((state) => state.workspace);
  const setWorkspace = useEditorStore((state) => state.setWorkspace);

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
      const result = await exportImage(
        active.path,
        selectActiveParams(state),
        state.exportSettings,
        undefined,
        state.rawByPhoto[active.id] ?? null,
      );
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
      if (!isCancelled(message)) useEditorStore.getState().notify("error", message);
    } finally {
      useEditorStore.getState().setExporting(false);
    }
  }, []);

  const handleSaveProject = useCallback(async () => {
    const state = useEditorStore.getState();
    if (state.photos.length === 0) return;
    if (state.loadSession.phase !== "ready") {
      state.notify("info", "项目正在加载或恢复，暂不能保存");
      return;
    }
    const started = performance.now();
    devLog("save_started", { projectId: state.loadSession.projectId, photos: state.photos.length });
    try {
      const doc = buildProjectDoc(state);
      const saved = await saveProject(doc, state.projectPath ?? undefined);
      useEditorStore.getState().setProjectPath(saved.path);
      devLog("save_completed", {
        projectId: state.loadSession.projectId,
        bytes: saved.bytes,
        elapsedMs: Math.round(performance.now() - started),
      });
      useEditorStore
        .getState()
        .notify(
          "success",
          `项目已保存：${baseName(saved.path)} · ${formatFileSize(saved.bytes)}`,
        );
    } catch (error) {
      const message = toMessage(error);
      if (!isCancelled(message)) useEditorStore.getState().notify("error", message);
    }
  }, []);

  const handleOpenProject = useCallback(async () => {
    const started = performance.now();
    try {
      const opened = await openProject();
      const requestId = useEditorStore.getState().beginProjectLoad(opened.path);
      devLog("load_started", {
        requestId,
        project: safeName(opened.path),
        photos: opened.project.photos.length,
      });

      const items = await importPaths(opened.project.photos.map((entry) => entry.path));
      if (useEditorStore.getState().loadSession.requestId !== requestId) return;
      devLog("database_loaded", {
        requestId,
        restoredItems: items.length,
        elapsedMs: Math.round(performance.now() - started),
      });
      if (items.length === 0) {
        useEditorStore
          .getState()
          .failProjectLoad(requestId, "photos-unavailable", "项目中的照片文件都不存在");
        useEditorStore.getState().notify("error", "项目中的照片文件都不存在，无法恢复");
        return;
      }

      const projectId = await computeProjectId(opened.path, opened.project);
      const snapshot = buildProjectSnapshot(items, opened.project, projectId);
      const committed = useEditorStore.getState().commitProjectSnapshot(requestId, snapshot);
      if (!committed) {
        if (snapshot.fatal) useEditorStore.getState().notify("error", snapshot.fatal);
        devLog("load_failed", { requestId, code: "project-invalid" });
        return;
      }
      useEditorStore.getState().setProjectPath(opened.path);
      const missing = snapshot.missing.length;
      useEditorStore.getState().notify(
        missing > 0 ? "info" : "success",
        missing > 0
          ? `已恢复项目（${items.length}/${opened.project.photos.length} 张照片，${missing} 张文件缺失）`
          : `已打开项目：${baseName(opened.path)}`,
      );
      if (snapshot.warnings.length > 0) {
        devLog("load_warnings", { requestId, count: snapshot.warnings.length });
      }

      // 恢复到 ready 之前只更新检测结果与参数快照，不允许默认参数写回。
      useEditorStore.getState().beginFaceRestore(requestId);
      let pending = 0;
      let missingFaces = 0;
      let changed = 0;
      let discarded = 0;
      for (const item of items) {
        if (useEditorStore.getState().loadSession.requestId !== requestId) return;
        const records = useEditorStore.getState().projectFaceRecords[item.id] ?? [];
        if (records.length === 0) continue;
        try {
          const detection = await detectFaces(item.path);
          if (useEditorStore.getState().loadSession.requestId !== requestId) return;
          useEditorStore.getState().setDetectedFaces(item.id, detection, requestId);
          const restored = await restoreFaces(
            item.path,
            records,
            useEditorStore.getState().projectSourceHashes[item.id],
          );
          if (useEditorStore.getState().loadSession.requestId !== requestId) return;
          useEditorStore.getState().applyFaceRestore(item.id, restored, requestId);
          pending += restored.pending.length;
          missingFaces += restored.missing.length;
          discarded += restored.discardedDense;
          if (restored.sourceChanged) changed += 1;
        } catch (error) {
          devLog("face_restore_failed", { requestId, photo: safeName(item.path), error: toMessage(error) });
        }
      }
      if (useEditorStore.getState().loadSession.requestId !== requestId) return;
      useEditorStore.getState().finishProjectLoad(requestId);
      devLog("ui_ready", { requestId, elapsedMs: Math.round(performance.now() - started) });

      if (changed > 0) {
        useEditorStore
          .getState()
          .notify("info", `${changed} 张照片的原图文件已变化，人脸参数需人工确认`);
      } else if (discarded > 0) {
        useEditorStore
          .getState()
          .notify(
            "info",
            `${discarded} 张人脸的旧 Provider/模型版本稠密结论与当前环境不一致，已锁定 LEVEL 3，需重新分析`,
          );
      } else if (pending > 0) {
        useEditorStore
          .getState()
          .notify("info", `${pending} 张人脸匹配置信度不足，请在人脸面板确认`);
      } else if (missingFaces > 0) {
        useEditorStore
          .getState()
          .notify("info", `${missingFaces} 张人脸的编辑记录已保留（本次未检测到）`);
      }
    } catch (error) {
      const message = toMessage(error);
      devLog("load_failed", { error: message });
      if (!isCancelled(message)) useEditorStore.getState().notify("error", message);
    }
  }, []);

  const handleUndo = useCallback(() => useEditorStore.getState().undo(), []);
  const handleRedo = useCallback(() => useEditorStore.getState().redo(), []);
  const handleToggleCompare = useCallback(() => useEditorStore.getState().toggleCompare(), []);
  const handleResetAll = useCallback(() => useEditorStore.getState().resetAll(), []);
  const handleFit = useCallback(() => useEditorStore.getState().fitToWindow(), []);
  const handleActualPixels = useCallback(() => useEditorStore.getState().actualPixels(), []);
  const handleZoom200 = useCallback(() => {
    const state = useEditorStore.getState();
    state.setZoom(2);
  }, []);
  const handleCopyParams = useCallback(() => useEditorStore.getState().copyParams(), []);
  const handlePasteParams = useCallback(() => useEditorStore.getState().pasteParams(), []);

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

  // 人脸推理进度（真实后端事件）。
  useEffect(() => {
    let alive = true;
    let unsubscribe: (() => void) | undefined;
    subscribeFaceProgress((progress) => {
      useEditorStore.getState().setFaceProgress(progress);
    }).then((cleanup) => {
      if (alive) unsubscribe = cleanup;
      else cleanup();
    });
    return () => {
      alive = false;
      unsubscribe?.();
    };
  }, []);

  // AI 美肤分析进度（检测 → 蒙版 → 指标）。
  useEffect(() => {
    let alive = true;
    let unsubscribe: (() => void) | undefined;
    subscribeBeautyProgress((progress) => {
      useEditorStore.getState().setBeautyProgress(progress);
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
      if (mod && key === "s") {
        event.preventDefault();
        void handleSaveProject();
        return;
      }
      if (mod && event.altKey && key === "c") {
        event.preventDefault();
        state.copyParams();
        return;
      }
      if (mod && event.altKey && key === "v") {
        event.preventDefault();
        state.pasteParams();
        return;
      }
      if (mod && key === "o") {
        event.preventDefault();
        if (event.shiftKey) void handleOpenProject();
        else void handleImport();
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
        case "2":
          state.setZoom(2);
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
  }, [handleImport, handleExport, handleSaveProject, handleOpenProject]);

  return (
    <div className={`app ${platformClass()}`}>
      <TopBar
        photo={photo}
        canUndo={canUndo}
        canRedo={canRedo}
        canPaste={canPaste}
        compareMode={compareMode}
        workspace={workspace}
        onWorkspace={setWorkspace}
        importing={importing}
        exporting={exporting}
        onImport={handleImport}
        onExport={handleExport}
        onSaveProject={handleSaveProject}
        onOpenProject={handleOpenProject}
        onUndo={handleUndo}
        onRedo={handleRedo}
        onCopyParams={handleCopyParams}
        onPasteParams={handlePasteParams}
        onToggleCompare={handleToggleCompare}
        onResetAll={handleResetAll}
        onFit={handleFit}
        onActualPixels={handleActualPixels}
        onZoom200={handleZoom200}
      />

      <div className="workspace">
        <LibraryRail photos={photos} activeId={activeId} onImport={handleImport} />
        {workspace === "batch" ? (
          <BatchWorkspace />
        ) : (
          <>
            <CanvasStage photo={photo} effectUrl={effectUrl} onImport={handleImport} />
            {workspace === "smart" ? (
              <SmartPanel />
            ) : workspace === "presets" ? (
              <PresetPanel />
            ) : workspace === "raw" ? (
              <RawPanel />
            ) : workspace === "color" ? (
              <ColorPanel />
            ) : workspace === "performance" ? (
              <PerformancePanel />
            ) : workspace === "ai" ? (
              <AiPortraitPanel photo={photo} />
            ) : workspace === "models" ? (
              <ModelHubPanel />
            ) : (
              <Inspector photo={photo} params={params} />
            )}
          </>
        )}
      </div>

      <StatusBar photo={photo} />
      <Toast />
      {projectPath ? (
        <span className="project-indicator" title={projectPath}>
          项目：{baseName(projectPath)}
        </span>
      ) : null}
    </div>
  );
}
