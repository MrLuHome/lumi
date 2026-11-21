import { Minus, Plus } from "lucide-react";
import type { PhotoAsset } from "../types";
import { useEditorStore } from "../state/editorStore";
import { formatDuration, formatFileSize, formatMegapixels, formatZoom } from "../lib/format";
import { isDesktopRuntime } from "../lib/platform";

interface StatusBarProps {
  photo: PhotoAsset | null;
}

export function StatusBar({ photo }: StatusBarProps) {
  const zoom = useEditorStore((state) => state.zoom);
  const viewport = useEditorStore((state) => state.viewport);
  const fitZoom = useEditorStore((state) => state.fitZoom);
  const status = useEditorStore((state) => state.renderStatus);
  const elapsed = useEditorStore((state) => state.renderElapsedMs);
  const error = useEditorStore((state) => state.renderError);
  const progress = useEditorStore((state) => state.renderProgress);
  const zoomStep = useEditorStore((state) => state.zoomStep);
  const setZoom = useEditorStore((state) => state.setZoom);
  const fitToWindow = useEditorStore((state) => state.fitToWindow);
  const actualPixels = useEditorStore((state) => state.actualPixels);
  const colorStatus = useEditorStore((state) => state.colorStatus);
  const gpuStatus = useEditorStore((state) => state.gpuStatus);
  const faceResult = useEditorStore((state) => (photo ? state.faces[photo.id] : undefined));

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
            {photo.bitDepth > 8 ? <span className="status-badge">16-bit</span> : null}
            <span className="status-muted">工作区 sRGB</span>
            {colorStatus ? <span className="status-badge">{colorStatus.outputLabel}</span> : null}
            <span className="status-muted">{formatFileSize(photo.fileSize)}</span>
          </>
        ) : (
          <span className="status-muted">未打开照片</span>
        )}
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
        {gpuStatus ? (
          <span className="status-muted" title={gpuStatus.reason}>
            {gpuStatus.enabled ? `GPU · ${gpuStatus.device || "Metal"}` : "CPU 计算"}
          </span>
        ) : null}
        {faceResult ? (
          <span className="status-muted" title="当前照片使用的人脸 Provider">
            人脸 · {faceResult.model}
          </span>
        ) : null}
        <span className={`proc proc-${error ? "error" : status}`} title={error ?? undefined}>
          {error ? (
            <>处理失败</>
          ) : status === "rendering" ? (
            <>
              <span className="spinner" />
              {progress
                ? `${progress.stage} ${Math.round(progress.progress * 100)}%`
                : "处理中"}
            </>
          ) : status === "ready" ? (
            <>
              <span className="proc-dot" />
              就绪{elapsed !== null && elapsed > 0 ? ` · ${formatDuration(elapsed)}` : ""}
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
