import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ImagePlus } from "lucide-react";
import type { MeshPreview, PhotoAsset } from "../types";
import { previewSubjectMask, reshapeMesh } from "../lib/backend";
import { selectActiveBodyAdjust, selectActiveFaces, selectActiveParams, useEditorStore } from "../state/editorStore";
import { isDesktopRuntime } from "../lib/platform";

interface CanvasStageProps {
  photo: PhotoAsset | null;
  effectUrl: string | null;
  onImport: () => void;
}

interface DraftStroke {
  points: { x: number; y: number }[];
}

function faceLetter(index: number): string {
  return String.fromCharCode(65 + (index % 26));
}

export function CanvasStage({ photo, effectUrl, onImport }: CanvasStageProps) {
  const zoom = useEditorStore((state) => state.zoom);
  const pan = useEditorStore((state) => state.pan);
  const viewport = useEditorStore((state) => state.viewport);
  const compareMode = useEditorStore((state) => state.compareMode);
  const compareSplit = useEditorStore((state) => state.compareSplit);
  const holdOriginal = useEditorStore((state) => state.holdOriginal);
  const dragActive = useEditorStore((state) => state.dragActive);
  const rendering = useEditorStore((state) => state.renderStatus === "rendering");
  const renderError = useEditorStore((state) => state.renderError);
  const renderProgress = useEditorStore((state) => state.renderProgress);
  const faceResult = useEditorStore(selectActiveFaces);
  const activeFace = useEditorStore((state) => state.activeFace);
  const showFaceOverlay = useEditorStore((state) => state.showFaceOverlay);
  const maskVisible = useEditorStore((state) => state.maskVisible);
  const maskPreview = useEditorStore((state) => state.maskPreview);
  const brushMode = useEditorStore((state) => state.brushMode);
  const brushRadius = useEditorStore((state) => state.brushRadius);
  const warpTool = useEditorStore((state) => state.warpTool);
  const warpRadius = useEditorStore((state) => state.warpRadius);
  const warpStrength = useEditorStore((state) => state.warpStrength);
  const warpFeather = useEditorStore((state) => state.warpFeather);
  const showMesh = useEditorStore((state) => state.showMesh);
  const subjectBrush = useEditorStore((state) => state.subjectBrush);
  const subjectBrushRadius = useEditorStore((state) => state.subjectBrushRadius);
  const subjectBrushHardness = useEditorStore((state) => state.subjectBrushHardness);
  const subjectBrushStrength = useEditorStore((state) => state.subjectBrushStrength);
  const subjectMaskView = useEditorStore((state) => state.subjectMaskView);
  const showSubjectMask = useEditorStore((state) => state.showSubjectMask);
  const subjectMaskPreview = useEditorStore((state) => state.subjectMaskPreview);
  const bodyTool = useEditorStore((state) => state.bodyTool);
  const bodyRadius = useEditorStore((state) => state.bodyRadius);
  const bodyStrength = useEditorStore((state) => state.bodyStrength);
  const bodyFeather = useEditorStore((state) => state.bodyFeather);
  const showBodyAnchors = useEditorStore((state) => state.showBodyAnchors);
  const bodyAdjust = useEditorStore(selectActiveBodyAdjust);
  const params = useEditorStore(selectActiveParams);
  const repairPoints = params.repairPoints;

  const containerRef = useRef<HTMLDivElement>(null);
  const [panning, setPanning] = useState(false);
  const [draft, setDraft] = useState<DraftStroke | null>(null);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const [mesh, setMesh] = useState<MeshPreview | null>(null);
  const panRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    panX: number;
    panY: number;
  } | null>(null);
  const splitPointer = useRef<number | null>(null);
  const anchorDrag = useRef<{
    index: number;
    startX: number;
    startY: number;
    originX: number;
    originY: number;
    originalDx: number;
    originalDy: number;
  } | null>(null);
  const activeFaceEntry = faceResult?.faces[activeFace] ?? null;

  // 网格显示：使用后端真实控制网格（原图像素坐标），不是装饰性线条。
  useEffect(() => {
    if (!showMesh || !photo || !activeFaceEntry || !isDesktopRuntime()) {
      setMesh(null);
      return;
    }
    let disposed = false;
    const timer = window.setTimeout(() => {
      reshapeMesh(photo.path, activeFaceEntry.id, params)
        .then((preview) => {
          if (!disposed) setMesh(preview);
        })
        .catch(() => {
          if (!disposed) setMesh(null);
        });
    }, 160);
    return () => {
      disposed = true;
      window.clearTimeout(timer);
    };
  }, [showMesh, photo, activeFaceEntry, params]);

  // 手动人物蒙版预览：与渲染共用同一套本地算法，仅用于编辑辅助。
  useEffect(() => {
    if (!photo || !isDesktopRuntime()) return;
    if (!showSubjectMask && subjectBrush === "off") {
      return;
    }
    const mask = params.subjectMask;
    let disposed = false;
    const state = useEditorStore.getState();
    if (mask.strokes.length === 0) {
      state.setSubjectMaskPreview(null);
      return;
    }
    state.setSubjectMaskStatus("loading");
    const timer = window.setTimeout(() => {
      previewSubjectMask(photo.path, mask, subjectMaskView)
        .then((preview) => {
          if (disposed) return;
          useEditorStore.getState().setSubjectMaskPreview(preview);
        })
        .catch((error) => {
          if (disposed) return;
          useEditorStore
            .getState()
            .setSubjectMaskError(error instanceof Error ? error.message : String(error));
        });
    }, 180);
    return () => {
      disposed = true;
      window.clearTimeout(timer);
    };
  }, [photo, params.subjectMask, subjectMaskView, showSubjectMask, subjectBrush]);

  // 视口尺寸与"适应窗口"比例
  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => {
      const rect = element.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return;
      const state = useEditorStore.getState();
      state.setViewport({ width: rect.width, height: rect.height });
      if (photo) {
        const padding = 96;
        const scale = Math.min(
          (rect.width - padding) / photo.width,
          (rect.height - padding) / photo.height,
        );
        state.setFitZoom(scale);
      }
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [photo]);

  // 滚轮缩放：以指针为锚点，符合专业修图软件的操作直觉
  useEffect(() => {
    const element = containerRef.current;
    if (!element || !photo) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      const anchor = {
        x: event.clientX - rect.left - rect.width / 2,
        y: event.clientY - rect.top - rect.height / 2,
      };
      const state = useEditorStore.getState();
      state.setZoom(state.zoom * Math.exp(-event.deltaY * 0.002), anchor);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [photo]);

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!photo || event.button !== 0) return;
    if ((event.target as HTMLElement).closest(".split-handle")) return;
    // 锚点拖拽优先：锚点坐标保存为归一化比例，缩放/平移后仍能命中。
    if (showBodyAnchors && bodyAdjust && bodyAdjust.anchors.length > 0) {
      const point = toImagePoint(event.clientX, event.clientY);
      if (point) {
        const state = useEditorStore.getState();
        const threshold = 14 / state.zoom;
        const index = bodyAdjust.anchors.findIndex(
          (anchor) =>
            Math.hypot(anchor.x * photo.width - point.x, anchor.y * photo.height - point.y) <=
            threshold,
        );
        const anchor = index >= 0 ? bodyAdjust.anchors[index] : undefined;
        if (anchor && !anchor.locked) {
          anchorDrag.current = {
            index,
            startX: event.clientX,
            startY: event.clientY,
            originX: anchor.x,
            originY: anchor.y,
            originalDx: anchor.dx,
            originalDy: anchor.dy,
          };
          event.currentTarget.setPointerCapture(event.pointerId);
          return;
        }
      }
    }
    if (subjectBrush !== "off") {
      const point = toImagePoint(event.clientX, event.clientY);
      if (!point) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      setDraft({ points: [point] });
      return;
    }
    if (bodyTool !== "off") {
      if (!params.subjectMask.strokes.length && !bodyAdjust?.maskStrokes.length) {
        useEditorStore.getState().notify("info", "请先绘制人物蒙版，身材调整只作用在人物区域");
        return;
      }
      const point = toImagePoint(event.clientX, event.clientY);
      if (!point) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      setDraft({ points: [point] });
      return;
    }
    if (warpTool !== "off") {
      if (!activeFaceEntry) {
        useEditorStore.getState().notify("info", "请先在「人脸信息」中检测并选择一张人脸");
        return;
      }
      const point = toImagePoint(event.clientX, event.clientY);
      if (!point) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      setDraft({ points: [point] });
      return;
    }
    if (brushMode === "repair") {
      const point = toImagePoint(event.clientX, event.clientY);
      if (!point) return;
      useEditorStore.getState().addRepairPoint(
        point.x / photo.width,
        point.y / photo.height,
        brushRadius,
      );
      return;
    }
    if (brushMode !== "off") {
      const point = toImagePoint(event.clientX, event.clientY);
      if (!point) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      setDraft({ points: [point] });
      return;
    }
    panRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      panX: pan.x,
      panY: pan.y,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    setPanning(true);
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const anchor = anchorDrag.current;
    if (anchor && photo) {
      const state = useEditorStore.getState();
      const offsetX = (event.clientX - anchor.startX) / state.zoom / photo.width;
      const offsetY = (event.clientY - anchor.startY) / state.zoom / photo.height;
      const dx = Math.min(0.15, Math.max(-0.15, anchor.originalDx + offsetX));
      const dy = Math.min(0.15, Math.max(-0.15, anchor.originalDy + offsetY));
      const x = Math.min(1, Math.max(0, anchor.originX + offsetX));
      const y = Math.min(1, Math.max(0, anchor.originY + offsetY));
      state.updateBody(
        (entry) => ({
          ...entry,
          anchors: entry.anchors.map((item, index) =>
            index === anchor.index ? { ...item, x, y, dx, dy } : item,
          ),
        }),
        "拖动锚点",
        `body:anchor:${anchor.index}`,
      );
      return;
    }
    if (subjectBrush !== "off" || bodyTool !== "off") {
      const point = toImagePoint(event.clientX, event.clientY);
      setCursor(point);
      if (draft && point) {
        const last = draft.points[draft.points.length - 1];
        if (!last || Math.hypot(point.x - last.x, point.y - last.y) >= 1.5) {
          setDraft({ points: [...draft.points, point] });
        }
      }
      return;
    }
    if (warpTool !== "off") {
      const point = toImagePoint(event.clientX, event.clientY);
      setCursor(point);
      if (draft && point) {
        const last = draft.points[draft.points.length - 1];
        if (!last || Math.hypot(point.x - last.x, point.y - last.y) >= 1.5) {
          setDraft({ points: [...draft.points, point] });
        }
      }
      return;
    }
    if (brushMode !== "off") {
      const point = toImagePoint(event.clientX, event.clientY);
      setCursor(point);
      if (draft && point) {
        const last = draft.points[draft.points.length - 1];
        if (!last || Math.hypot(point.x - last.x, point.y - last.y) >= 1.5) {
          setDraft({ points: [...draft.points, point] });
        }
      }
      return;
    }
    const drag = panRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    useEditorStore.getState().setPan({
      x: drag.panX + event.clientX - drag.startX,
      y: drag.panY + event.clientY - drag.startY,
    });
  };

  const endPan = (event: React.PointerEvent<HTMLDivElement>) => {
    if (anchorDrag.current) {
      anchorDrag.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      return;
    }
    if (subjectBrush !== "off" && draft && photo) {
      const points = draft.points.map((point) => ({
        x: point.x / photo.width,
        y: point.y / photo.height,
      }));
      if (points.length > 0) {
        useEditorStore.getState().appendSubjectMaskStroke({
          kind: subjectBrush,
          radius: subjectBrushRadius,
          hardness: subjectBrushHardness,
          strength: subjectBrushStrength,
          points,
        });
      }
      setDraft(null);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      return;
    }
    if (bodyTool !== "off" && draft && photo) {
      const points = draft.points.map((point) => ({
        x: point.x / photo.width,
        y: point.y / photo.height,
      }));
      if (points.length > 0) {
        useEditorStore.getState().appendBodyStroke({
          mode: bodyTool,
          radius: bodyRadius,
          strength: bodyStrength,
          feather: bodyFeather,
          points,
        });
      }
      setDraft(null);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      return;
    }
    if (warpTool !== "off" && draft && photo && activeFaceEntry) {
      const points = draft.points.map((point) => ({
        x: point.x / photo.width,
        y: point.y / photo.height,
      }));
      if (points.length > 0) {
        useEditorStore.getState().appendWarpStroke(activeFaceEntry.id, {
          mode: warpTool,
          radius: warpRadius,
          strength: warpStrength,
          feather: warpFeather,
          points,
        });
      }
      setDraft(null);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      return;
    }
    if (draft && photo) {
      const points = draft.points.map((point) => ({
        x: point.x / photo.width,
        y: point.y / photo.height,
      }));
      if (points.length > 0) {
        useEditorStore.getState().appendMaskStroke({
          erase: brushMode === "erase",
          radius: brushRadius,
          points,
        });
      }
      setDraft(null);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      return;
    }
    if (panRef.current?.pointerId !== event.pointerId) return;
    panRef.current = null;
    setPanning(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  /** 屏幕坐标 → 图像像素坐标（考虑缩放与平移，与渲染器一致）。 */
  const toImagePoint = (clientX: number, clientY: number) => {
    const element = containerRef.current;
    if (!element || !photo) return null;
    const rect = element.getBoundingClientRect();
    const state = useEditorStore.getState();
    const left = rect.left + rect.width / 2 + state.pan.x - (photo.width * state.zoom) / 2;
    const top = rect.top + rect.height / 2 + state.pan.y - (photo.height * state.zoom) / 2;
    const x = (clientX - left) / state.zoom;
    const y = (clientY - top) / state.zoom;
    if (x < 0 || y < 0 || x > photo.width || y > photo.height) return null;
    return { x, y };
  };

  const updateSplit = (clientX: number) => {
    const element = containerRef.current;
    if (!element || !photo) return;
    const rect = element.getBoundingClientRect();
    const state = useEditorStore.getState();
    const imageLeft = rect.width / 2 + state.pan.x - (photo.width * state.zoom) / 2;
    const ratio = (clientX - rect.left - imageLeft) / (photo.width * state.zoom);
    state.setCompareSplit(ratio);
  };

  const dividerX = photo
    ? viewport.width / 2 + pan.x - (photo.width * zoom) / 2 + compareSplit * photo.width * zoom
    : 0;

  const progressText = renderProgress
    ? `${renderProgress.stage} ${Math.round(renderProgress.progress * 100)}%`
    : "渲染中";

  return (
    <main
      className={`stage ${panning ? "is-panning" : ""} ${photo ? "has-photo" : ""} ${
        warpTool !== "off"
          ? "is-brushing is-warping"
          : subjectBrush !== "off" || bodyTool !== "off"
            ? "is-brushing"
            : brushMode !== "off"
              ? `is-brushing is-${brushMode}`
              : ""
      }`}
      ref={containerRef}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endPan}
      onPointerCancel={endPan}
      onPointerLeave={() => setCursor(null)}
      onDoubleClick={() => {
        const state = useEditorStore.getState();
        if (state.zoom >= 0.999) state.fitToWindow();
        else state.actualPixels();
      }}
    >
      {photo ? (
        <>
          <div
            className="stage-frame"
            style={{
              width: photo.width,
              height: photo.height,
              transform: `translate(-50%, -50%) translate3d(${pan.x}px, ${pan.y}px, 0) scale(${zoom})`,
            }}
          >
            <img
              className="stage-image"
              src={photo.previewUrl}
              alt={`${photo.fileName} 原图`}
              draggable={false}
            />
            {effectUrl && !holdOriginal && (
              <img
                className="stage-image stage-effect"
                src={effectUrl}
                alt={`${photo.fileName} 效果图`}
                draggable={false}
                style={{
                  clipPath:
                    compareMode === "split" ? `inset(0 0 0 ${compareSplit * 100}%)` : undefined,
                }}
              />
            )}

            {maskVisible && maskPreview && (
              <img
                className="stage-image stage-mask"
                src={maskPreview.url}
                alt="皮肤蒙版"
                draggable={false}
              />
            )}

            {(showSubjectMask || subjectBrush !== "off") && subjectMaskPreview && (
              <img
                className="stage-image stage-subject-mask"
                src={subjectMaskPreview.url}
                alt="手动人物蒙版（非 AI 分割）"
                draggable={false}
              />
            )}

            {showBodyAnchors && bodyAdjust && bodyAdjust.anchors.length > 0 && (
              <svg
                className="body-anchor-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="手动身体锚点"
              >
                {bodyAdjust.anchors.map((anchor, index) => (
                  <g
                    key={`${anchor.part}-${index}`}
                    className={`body-anchor ${anchor.locked ? "is-locked" : ""}`}
                  >
                    <circle
                      cx={anchor.x * photo.width}
                      cy={anchor.y * photo.height}
                      r={anchor.radius * photo.width}
                      className="body-anchor-range"
                      vectorEffect="non-scaling-stroke"
                    />
                    <circle
                      cx={(anchor.x + anchor.dx) * photo.width}
                      cy={(anchor.y + anchor.dy) * photo.height}
                      r={Math.max(5, photo.width * 0.008)}
                      className="body-anchor-point"
                      vectorEffect="non-scaling-stroke"
                    />
                    <line
                      x1={anchor.x * photo.width}
                      y1={anchor.y * photo.height}
                      x2={(anchor.x + anchor.dx) * photo.width}
                      y2={(anchor.y + anchor.dy) * photo.height}
                      className="body-anchor-link"
                      vectorEffect="non-scaling-stroke"
                    />
                  </g>
                ))}
              </svg>
            )}

            {showFaceOverlay && faceResult && faceResult.faces.length > 0 && (
              <svg
                className="face-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="人脸检测结果"
              >
                {faceResult.faces.map((face, index) => (
                  <g
                    key={face.id}
                    className={`face-group ${index === activeFace ? "is-active" : ""}`}
                  >
                    <rect
                      x={face.x}
                      y={face.y}
                      width={face.width}
                      height={face.height}
                      rx={Math.max(8, face.width * 0.03)}
                      vectorEffect="non-scaling-stroke"
                    />
                    {face.landmarks.map((point, pointIndex) => (
                      <circle
                        key={pointIndex}
                        cx={point.x}
                        cy={point.y}
                        r={Math.max(4, face.width * 0.012)}
                        vectorEffect="non-scaling-stroke"
                      />
                    ))}
                    <text
                      className="face-label"
                      x={face.x + face.width * 0.03}
                      y={face.y + face.width * 0.07}
                      fontSize={Math.max(14, face.width * 0.09)}
                    >
                      {faceLetter(index)} · ID {face.id}
                    </text>
                  </g>
                ))}
              </svg>
            )}

            {showMesh && mesh && mesh.points.length > 0 && (
              <svg
                className="mesh-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="面部控制网格"
              >
                {mesh.triangles.map((triangle, index) => (
                  <polygon
                    key={`t-${index}`}
                    points={triangle
                      .map((pointIndex) => mesh.points[pointIndex]!.join(","))
                      .join(" ")}
                    vectorEffect="non-scaling-stroke"
                  />
                ))}
                {mesh.points.map(([x, y], index) => (
                  <circle
                    key={`p-${index}`}
                    cx={x}
                    cy={y}
                    r={Math.max(2, photo.width * 0.002)}
                    vectorEffect="non-scaling-stroke"
                  />
                ))}
              </svg>
            )}

            {(subjectBrush !== "off" || bodyTool !== "off") && (cursor || draft) && (
              <svg
                className="brush-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label={subjectBrush !== "off" ? "手动人物蒙版画笔" : "身材推拉笔刷"}
              >
                {draft && draft.points.length > 0 ? (
                  draft.points.length === 1 ? (
                    <circle
                      cx={draft.points[0]!.x}
                      cy={draft.points[0]!.y}
                      r={
                        (subjectBrush !== "off" ? subjectBrushRadius : bodyRadius) * photo.width
                      }
                      className={
                        subjectBrush === "erase" || subjectBrush === "background"
                          ? "brush-stroke is-erase"
                          : "brush-stroke"
                      }
                    />
                  ) : (
                    <polyline
                      points={draft.points.map((point) => `${point.x},${point.y}`).join(" ")}
                      fill="none"
                      strokeWidth={
                        (subjectBrush !== "off" ? subjectBrushRadius : bodyRadius) *
                        photo.width *
                        2
                      }
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      className={
                        subjectBrush === "erase" || subjectBrush === "background"
                          ? "brush-stroke is-erase"
                          : "brush-stroke"
                      }
                    />
                  )
                ) : null}
                {cursor && !draft ? (
                  <circle
                    cx={cursor.x}
                    cy={cursor.y}
                    r={
                      (subjectBrush !== "off" ? subjectBrushRadius : bodyRadius) * photo.width
                    }
                    className={
                      subjectBrush === "erase" || subjectBrush === "background"
                        ? "brush-cursor is-erase"
                        : "brush-cursor"
                    }
                  />
                ) : null}
              </svg>
            )}

            {brushMode !== "off" && (cursor || draft) && (
              <svg
                className="brush-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="手动蒙版画笔"
              >
                {draft && draft.points.length > 0 ? (
                  draft.points.length === 1 ? (
                    <circle
                      cx={draft.points[0]!.x}
                      cy={draft.points[0]!.y}
                      r={brushRadius * photo.width}
                      className={`brush-stroke ${brushMode === "erase" ? "is-erase" : ""}`}
                    />
                  ) : (
                    <polyline
                      points={draft.points.map((point) => `${point.x},${point.y}`).join(" ")}
                      fill="none"
                      strokeWidth={brushRadius * photo.width * 2}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      className={`brush-stroke ${brushMode === "erase" ? "is-erase" : ""}`}
                    />
                  )
                ) : null}
                {cursor && !draft ? (
                  <circle
                    cx={cursor.x}
                    cy={cursor.y}
                    r={brushRadius * photo.width}
                    className={`brush-cursor ${brushMode === "erase" ? "is-erase" : ""}`}
                  />
                ) : null}
              </svg>
            )}

            {warpTool !== "off" && (cursor || draft) && (
              <svg
                className="brush-overlay warp-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="局部变形笔刷"
              >
                {draft && draft.points.length > 0 ? (
                  draft.points.length === 1 ? (
                    <circle
                      cx={draft.points[0]!.x}
                      cy={draft.points[0]!.y}
                      r={warpRadius * photo.width}
                      className="warp-stroke"
                    />
                  ) : (
                    <polyline
                      points={draft.points.map((point) => `${point.x},${point.y}`).join(" ")}
                      fill="none"
                      strokeWidth={warpRadius * photo.width * 2}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      className="warp-stroke"
                    />
                  )
                ) : null}
                {cursor && !draft ? (
                  <circle
                    cx={cursor.x}
                    cy={cursor.y}
                    r={warpRadius * photo.width}
                    className="warp-cursor"
                  />
                ) : null}
              </svg>
            )}

            {repairPoints.length > 0 && (
              <svg
                className="brush-overlay"
                viewBox={`0 0 ${photo.width} ${photo.height}`}
                preserveAspectRatio="none"
                aria-label="手动修复点"
              >
                {repairPoints.map((point, index) => (
                  <g key={`${index}-${point.x}-${point.y}`}>
                    <circle
                      cx={point.x * photo.width}
                      cy={point.y * photo.height}
                      r={point.radius * photo.width}
                      className="repair-ring"
                    />
                    <circle
                      cx={point.x * photo.width}
                      cy={point.y * photo.height}
                      r={Math.max(3, point.radius * photo.width * 0.1)}
                      className="repair-dot"
                    />
                  </g>
                ))}
              </svg>
            )}
          </div>

          {compareMode === "split" && !holdOriginal && (
            <>
              <div
                className="split-handle"
                style={{ left: dividerX }}
                onPointerDown={(event) => {
                  event.stopPropagation();
                  splitPointer.current = event.pointerId;
                  event.currentTarget.setPointerCapture(event.pointerId);
                  updateSplit(event.clientX);
                }}
                onPointerMove={(event) => {
                  if (splitPointer.current === event.pointerId) updateSplit(event.clientX);
                }}
                onPointerUp={(event) => {
                  splitPointer.current = null;
                  event.currentTarget.releasePointerCapture(event.pointerId);
                }}
              >
                <span className="split-grip" />
              </div>
              <span className="compare-tag tag-left">原图</span>
              <span className="compare-tag tag-right">效果</span>
            </>
          )}

          {holdOriginal && <span className="compare-tag tag-left">原图（按住 \）</span>}

          {rendering && (
            <div className="stage-busy">
              <span className="spinner" />
              {progressText}
            </div>
          )}

          {renderError && (
            <div className="stage-error" role="alert">
              <AlertTriangle size={13} />
              <span>{renderError}</span>
            </div>
          )}
        </>
      ) : (
        <div className="stage-empty">
          <div className="stage-empty-icon">
            <ImagePlus size={26} />
          </div>
          <h1>导入照片开始修图</h1>
          <p>
            拖入文件，或从本地选择 JPG / PNG / WebP
            {!isDesktopRuntime() && " · 当前为浏览器预览模式"}
          </p>
          <button type="button" className="btn btn-accent" onClick={onImport}>
            选择照片
          </button>
          <div className="stage-empty-meta">
            <span>
              <kbd>⌘O</kbd> 导入
            </span>
            <span>
              <kbd>F</kbd> 适应窗口
            </span>
            <span>
              <kbd>1</kbd> 实际像素
            </span>
            <span>
              <kbd>\</kbd> 按住看原图
            </span>
          </div>
        </div>
      )}

      {dragActive && (
        <div className="stage-drop">
          <div className="stage-drop-inner">
            <ImagePlus size={22} />
            <span>松开以导入照片</span>
          </div>
        </div>
      )}
    </main>
  );
}
