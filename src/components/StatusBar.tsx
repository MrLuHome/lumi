import { Minus, Plus } from "lucide-react";
import type { PhotoAsset } from "../types";
import { useEditorStore } from "../state/editorStore";
import { formatFileSize, formatMegapixels, formatZoom } from "../lib/format";
import { isDesktopRuntime } from "../lib/platform";

interface StatusBarProps {
  photo: PhotoAsset | null;
}

/** 底部状态条：只说人话，不暴露算法名、耗时与内部配置。 */
export function StatusBar({ photo }: StatusBarProps) {
  const zoom = useEditorStore((state) => state.zoom);
  const viewport = useEditorStore((state) => state.viewport);
  const fitZoom = useEditorStore((state) => state.fitZoom);
  const status = useEditorStore((state) => state.renderStatus);
  const error = useEditorStore((state) => state.renderError);
  const progress = useEditorStore((state) => state.renderProgress);
  const zoomStep = useEditorStore((state) => state.zoomStep);
  const setZoom = useEditorStore((state) => state.setZoom);
  const fitToWindow = useEditorStore((state) => state.fitToWindow);
  const actualPixels = useEditorStore((state) => state.actualPixels);
  const loadSession = useEditorStore((state) => state.loadSession);

  return (
    <footer className="statusbar">
      <div className="status-left">
        {photo ? (
          <>
            <span className="status-file" title={photo.path}>
              {photo.fileName}
            </span>
            <span className="status-sep" />
            <span>
              {photo.width} × {photo.height}
            </span>
            <span className="status-muted">{formatMegapixels(photo.width, photo.height)}</span>
            <span className="status-badge">{photo.format}</span>
            {photo.bitDepth > 8 ? <span className="status-badge">16 位</span> : null}
            <span className="status-muted">{formatFileSize(photo.fileSize)}</span>
          </>
        ) : (
          <span className="status-muted">还没有打开照片</span>
        )}
        {loadSession.phase === "loading" ? (
          <span className="status-badge">项目读取中…</span>
        ) : loadSession.phase === "restoring" ? (
          <span className="status-badge">正在恢复人脸参数…</span>
        ) : loadSession.phase === "failed" ? (
          <span className="status-badge" title={loadSession.error?.message}>
            项目打开失败
          </span>
        ) : null}
      </div>

      <div className="status-center">
        <button
          type="button"
          className="status-icon"
          onClick={() => zoomStep(-1)}
          disabled={!photo}
          title="缩小"
        >
          <Minus size={12} />
        </button>
        <span className="zoom-value">{formatZoom(zoom)}</span>
        <button
          type="button"
          className="status-icon"
          onClick={() => zoomStep(1)}
          disabled={!photo}
          title="放大"
        >
          <Plus size={12} />
        </button>
        <span className="status-sep" />
        <button
          type="button"
          className={`status-action ${Math.abs(zoom - fitZoom) < 0.001 ? "is-active" : ""}`}
          onClick={fitToWindow}
          disabled={!photo}
          title="适应窗口 (F)"
        >
          适应
        </button>
        <button
          type="button"
          className={`status-action ${Math.abs(zoom - 1) < 0.001 ? "is-active" : ""}`}
          onClick={actualPixels}
          disabled={!photo}
          title="实际像素 (1)"
        >
          1:1
        </button>
        <button
          type="button"
          className={`status-action ${Math.abs(zoom - 2) < 0.001 ? "is-active" : ""}`}
          onClick={() => setZoom(2)}
          disabled={!photo}
          title="放大到 200% (2)"
        >
          2:1
        </button>
        <span className="status-muted status-viewport">
          {Math.round(viewport.width)} × {Math.round(viewport.height)}
        </span>
      </div>

      <div className="status-right">
        {!isDesktopRuntime() && <span className="status-note">浏览器预览模式</span>}
        <span className={`proc proc-${error ? "error" : status}`} title={error ?? undefined}>
          {error ? (
            <>预览没出来，参数还在（记得保存）</>
          ) : status === "rendering" ? (
            <>
              <span className="spinner" />
              {progress ? `正在生成预览 ${Math.round(progress.progress * 100)}%` : "正在生成预览"}
            </>
          ) : status === "ready" ? (
            <>
              <span className="proc-dot" />
              预览已更新
            </>
          ) : (
            <>
              <span className="proc-dot" />
              待命
            </>
          )}
        </span>
      </div>
    </footer>
  );
}
