import { useEffect } from "react";
import { AlertTriangle, Eye, EyeOff, Loader2, ScanFace } from "lucide-react";
import type { PhotoAsset } from "../types";
import { detectFaces, faceModelStatus } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { selectActiveFaces, useEditorStore } from "../state/editorStore";
import { DEBUG_OVERLAYS_ENABLED } from "./slim/featureFlags";

interface FacePanelProps {
  photo: PhotoAsset | null;
}

/** 人脸信息面板：模型状态来自后端，检测结果全部来自本地 YuNet 推理。 */
export function FacePanel({ photo }: FacePanelProps) {
  const modelStatus = useEditorStore((state) => state.modelStatus);
  const faceStatus = useEditorStore((state) => state.faceStatus);
  const faceProgress = useEditorStore((state) => state.faceProgress);
  const faceError = useEditorStore((state) => state.faceError);
  const activeFace = useEditorStore((state) => state.activeFace);
  const showOverlay = useEditorStore((state) => state.showFaceOverlay);
  const result = useEditorStore(selectActiveFaces);
  const setModelStatus = useEditorStore((state) => state.setModelStatus);
  const setFaceStatus = useEditorStore((state) => state.setFaceStatus);
  const setFaceProgress = useEditorStore((state) => state.setFaceProgress);
  const setFaceError = useEditorStore((state) => state.setFaceError);
  const setFaces = useEditorStore((state) => state.setFaces);
  const setActiveFace = useEditorStore((state) => state.setActiveFace);
  const setShowOverlay = useEditorStore((state) => state.setShowFaceOverlay);
  const notify = useEditorStore((state) => state.notify);
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    faceModelStatus()
      .then((status) => {
        if (!disposed) setModelStatus(status);
      })
      .catch((error) => {
        if (!disposed) {
          notify("error", `人脸模型状态读取失败：${error instanceof Error ? error.message : String(error)}`);
        }
      });
    return () => {
      disposed = true;
    };
  }, [desktop, notify, setModelStatus]);

  const runDetection = async () => {
    if (!photo) return;
    setFaceStatus("loading");
    setFaceProgress({ stage: "准备模型", progress: 0 });
    try {
      const detection = await detectFaces(photo.path);
      setFaces(photo.id, detection);
      if (detection.faces.length === 0) {
        notify("info", "未检测到人脸（这是真实推理结果，不是错误）");
      } else {
        notify("success", `检测到 ${detection.faces.length} 张人脸 · ${detection.inferenceMs} ms`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setFaceError(message);
      notify("error", `人脸检测失败：${message}`);
    }
  };

  const busy = faceStatus === "loading";
  const faces = result?.faces ?? [];

  return (
    <div className="face-panel">
      <div className="face-model">
        <div className="face-model-line">
          <span className="face-model-name">{modelStatus?.name ?? "YuNet"}</span>
          <span className={`face-badge ${modelStatus?.available ? "is-ok" : "is-warn"}`}>
            {!desktop
              ? "仅桌面端"
              : modelStatus?.available
                ? modelStatus.loaded
                  ? "已加载"
                  : "模型就绪"
                : "模型缺失"}
          </span>
        </div>
        <div className="face-meta">
          <span>{modelStatus?.version ?? "2023mar"}</span>
          <span>{modelStatus?.license ?? "MIT"}</span>
          <span>{modelStatus?.outputs ?? "bbox · kps · score"}</span>
        </div>
        {modelStatus?.sha256 ? (
          <div className="face-hash" title={modelStatus.sha256}>
            SHA256 {modelStatus.sha256.slice(0, 16)}…
          </div>
        ) : null}
        {modelStatus?.source ? (
          <div className="face-source" title={modelStatus.source}>
            来源：{modelStatus.source}
          </div>
        ) : null}
      </div>

      <div className="face-actions">
        <button
          type="button"
          className="face-detect-btn"
          disabled={!photo || busy || !desktop}
          onClick={runDetection}
        >
          {busy ? <Loader2 size={13} className="spin" /> : <ScanFace size={13} />}
          {busy ? "检测中…" : "检测人脸"}
        </button>
        <button
          type="button"
          className={`icon-toggle ${showOverlay ? "is-on" : ""}`}
          title={
            DEBUG_OVERLAYS_ENABLED
              ? showOverlay
                ? "隐藏检测框与关键点"
                : "显示检测框与关键点"
              : "调试预览（需开发者开关）"
          }
          aria-label={showOverlay ? "隐藏检测框与关键点" : "显示检测框与关键点"}
          onClick={() => setShowOverlay(!showOverlay)}
          disabled={!result || !DEBUG_OVERLAYS_ENABLED}
        >
          {showOverlay ? <Eye size={14} /> : <EyeOff size={14} />}
        </button>
      </div>

      {busy && faceProgress ? (
        <div className="face-progress">
          <div className="progress-bar">
            <span style={{ width: `${Math.round(faceProgress.progress * 100)}%` }} />
          </div>
          <div className="progress-text">
            <span>{faceProgress.stage}</span>
            <span>{Math.round(faceProgress.progress * 100)}%</span>
          </div>
        </div>
      ) : null}

      {faceStatus === "error" && faceError ? (
        <p className="face-error">
          <AlertTriangle size={12} />
          {faceError}
        </p>
      ) : null}

      {result ? (
        <div className="face-results">
          <div className="face-count">
            检测到 <strong>{result.faces.length}</strong> 张人脸 · 推理 {result.inferenceMs} ms
          </div>
          {faces.map((face, index) => (
            <button
              key={face.id}
              type="button"
              className={`face-item ${index === activeFace ? "is-active" : ""}`}
              onClick={() => setActiveFace(index)}
            >
              <span className="face-index">#{index + 1}</span>
              <span className="face-score">{(face.score * 100).toFixed(1)}%</span>
              <span className="face-box-size">
                {Math.round(face.width)}×{Math.round(face.height)}
              </span>
              <span className="face-landmarks">{face.landmarks.length} 关键点</span>
            </button>
          ))}
          {faces.length === 0 ? <p className="param-hint">这张照片中没有检测到人脸。</p> : null}
        </div>
      ) : (
        <p className="param-hint">
          本地 CPU 推理 · 不会上传照片 · 未授权模型不会被打包
        </p>
      )}
    </div>
  );
}
