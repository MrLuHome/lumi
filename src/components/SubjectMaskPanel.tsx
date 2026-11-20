import { useEffect } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Eraser,
  Eye,
  EyeOff,
  Loader2,
  MousePointer2,
  Paintbrush,
  RotateCcw,
  Undo2,
} from "lucide-react";
import type { PhotoAsset } from "../types";
import { segmentationCapability } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import type { SubjectMaskStrokeKind } from "../state/params";
import { selectActiveParams, useEditorStore } from "../state/editorStore";
import { Slider } from "./ui/Slider";

interface SubjectMaskPanelProps {
  photo: PhotoAsset | null;
}

const BRUSH_TOOLS: { kind: SubjectMaskStrokeKind; label: string }[] = [
  { kind: "paint", label: "添加前景" },
  { kind: "erase", label: "删除前景" },
  { kind: "foreground", label: "前景标记" },
  { kind: "background", label: "背景标记" },
];

export function SubjectMaskPanel({ photo }: SubjectMaskPanelProps) {
  const params = useEditorStore(selectActiveParams);
  const mask = params.subjectMask;
  const capability = useEditorStore((state) => state.segmentationCapability);
  const subjectBrush = useEditorStore((state) => state.subjectBrush);
  const brushRadius = useEditorStore((state) => state.subjectBrushRadius);
  const hardness = useEditorStore((state) => state.subjectBrushHardness);
  const strength = useEditorStore((state) => state.subjectBrushStrength);
  const maskView = useEditorStore((state) => state.subjectMaskView);
  const showMask = useEditorStore((state) => state.showSubjectMask);
  const maskTarget = useEditorStore((state) => state.subjectMaskTarget);
  const preview = useEditorStore((state) => state.subjectMaskPreview);
  const status = useEditorStore((state) => state.subjectMaskStatus);
  const error = useEditorStore((state) => state.subjectMaskError);
  const setSubjectBrush = useEditorStore((state) => state.setSubjectBrush);
  const setBrushRadius = useEditorStore((state) => state.setSubjectBrushRadius);
  const setHardness = useEditorStore((state) => state.setSubjectBrushHardness);
  const setStrength = useEditorStore((state) => state.setSubjectBrushStrength);
  const setMaskView = useEditorStore((state) => state.setSubjectMaskView);
  const setShowMask = useEditorStore((state) => state.setShowSubjectMask);
  const setMaskTarget = useEditorStore((state) => state.setSubjectMaskTarget);
  const updateSubjectMask = useEditorStore((state) => state.updateSubjectMask);
  const undoStroke = useEditorStore((state) => state.undoSubjectMaskStroke);
  const clearMask = useEditorStore((state) => state.clearSubjectMask);
  const setMaskEnabled = useEditorStore((state) => state.setSubjectMaskEnabled);
  const setCapability = useEditorStore((state) => state.setSegmentationCapability);
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    segmentationCapability()
      .then((value) => {
        if (!disposed) setCapability(value);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [desktop, setCapability]);

  if (!photo) return null;

  const disabledProviders = (capability?.providers ?? []).filter((provider) => !provider.available);

  return (
    <div className="reshape-panel">
      <div className="reshape-notice">
        手动人物蒙版：所有 Alpha 均由本地画笔与确定性算法生成，不是 AI 自动分割或智能抠图。
      </div>

      {capability ? (
        <p className="param-hint">
          {capability.aiAvailable
            ? "已启用经过授权校验的自动分割 Provider。"
            : "AI 自动分割未启用。手动笔触是当前唯一可用的蒙版路径。"}
        </p>
      ) : null}

      <div className="reshape-section-title">画笔</div>
      <div className="warp-tools">
        <button
          type="button"
          className={`warp-tool ${subjectBrush === "off" ? "is-active" : ""}`}
          onClick={() => setSubjectBrush("off")}
          title="关闭画笔，恢复画布平移"
        >
          <MousePointer2 size={12} />
          关闭
        </button>
        {BRUSH_TOOLS.map((tool) => (
          <button
            key={tool.kind}
            type="button"
            className={`warp-tool ${subjectBrush === tool.kind ? "is-active" : ""}`}
            onClick={() => {
              setSubjectBrush(tool.kind);
              setShowMask(true);
            }}
          >
            {tool.kind === "erase" || tool.kind === "background" ? (
              <Eraser size={12} />
            ) : (
              <Paintbrush size={12} />
            )}
            {tool.label}
          </button>
        ))}
      </div>
      <p className="param-hint">
        前景标记优先于普通笔触，背景标记会从蒙版中扣除。画布坐标按原图换算，缩放、平移后仍然准确。
      </p>

      <Slider
        label="画笔半径"
        value={Math.round(brushRadius * 100)}
        min={1}
        max={40}
        bipolar={false}
        defaultValue={4}
        suffix="%"
        onChange={(value) => setBrushRadius(value / 100)}
      />
      <Slider
        label="画笔硬度"
        value={hardness}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={60}
        suffix="%"
        onChange={setHardness}
      />
      <Slider
        label="笔触强度"
        value={strength}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={100}
        suffix="%"
        onChange={setStrength}
      />

      <div className="reshape-section-title">Alpha 处理</div>
      <Slider
        label="蒙版羽化"
        value={mask.feather}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={40}
        suffix="%"
        onChange={(value) =>
          updateSubjectMask(
            (current) => ({ ...current, feather: value }),
            "蒙版羽化",
            "subject:feather",
          )
        }
      />
      <Slider
        label="扩张 / 收缩"
        value={mask.expand}
        min={-100}
        max={100}
        defaultValue={0}
        onChange={(value) =>
          updateSubjectMask(
            (current) => ({ ...current, expand: value }),
            "蒙版扩张收缩",
            "subject:expand",
          )
        }
      />
      <Slider
        label="边缘平滑"
        value={mask.smooth}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={15}
        suffix="%"
        onChange={(value) =>
          updateSubjectMask(
            (current) => ({ ...current, smooth: value }),
            "边缘平滑",
            "subject:smooth",
          )
        }
      />
      <Slider
        label="发丝边缘辅助"
        value={mask.hairProtect}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={0}
        suffix="%"
        onChange={(value) =>
          updateSubjectMask(
            (current) => ({ ...current, hairProtect: value }),
            "发丝边缘辅助",
            "subject:hair",
          )
        }
      />
      <Slider
        label="边缘去污染"
        value={mask.decontaminate}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={40}
        suffix="%"
        onChange={(value) =>
          updateSubjectMask(
            (current) => ({ ...current, decontaminate: value }),
            "边缘去污染",
            "subject:decontaminate",
          )
        }
      />
      <p className="param-hint">
        去污染用邻近可信前景颜色估计边缘颜色，不会把边缘涂成固定色。复杂发丝无法可靠恢复时保留手动笔刷入口。
      </p>

      <div className="warp-actions">
        <button
          type="button"
          className={`mini-btn ${mask.enabled ? "is-active" : ""}`}
          onClick={() => setMaskEnabled(!mask.enabled)}
        >
          <CheckCircle2 size={11} />
          {mask.enabled ? "蒙版已启用" : "启用蒙版"}
        </button>
        <button
          type="button"
          className={`mini-btn ${mask.invert ? "is-active" : ""}`}
          onClick={() =>
            updateSubjectMask((current) => ({ ...current, invert: !current.invert }), "反转蒙版")
          }
        >
          反转
        </button>
        <button
          type="button"
          className="mini-btn"
          disabled={mask.strokes.length === 0}
          onClick={undoStroke}
        >
          <Undo2 size={11} />
          撤销单笔
        </button>
        <button
          type="button"
          className="mini-btn"
          disabled={mask.strokes.length === 0}
          onClick={clearMask}
        >
          <RotateCcw size={11} />
          清除蒙版
        </button>
      </div>

      <div className="warp-actions">
        <button
          type="button"
          className={`mini-btn ${showMask ? "is-active" : ""}`}
          onClick={() => setShowMask(!showMask)}
        >
          {showMask ? <Eye size={11} /> : <EyeOff size={11} />}
          {showMask ? "显示蒙版" : "隐藏蒙版"}
        </button>
        <button
          type="button"
          className={`mini-btn ${maskView === "overlay" ? "is-active" : ""}`}
          onClick={() => setMaskView("overlay")}
        >
          红色遮罩
        </button>
        <button
          type="button"
          className={`mini-btn ${maskView === "alpha" ? "is-active" : ""}`}
          onClick={() => setMaskView("alpha")}
        >
          黑白 Alpha
        </button>
      </div>

      <div className="reshape-section-title">多人照片</div>
      <div className="warp-actions">
        <button
          type="button"
          className={`mini-btn ${maskTarget === "global" ? "is-active" : ""}`}
          onClick={() => setMaskTarget("global")}
        >
          公共蒙版
        </button>
        <button
          type="button"
          className={`mini-btn ${maskTarget === "subject" ? "is-active" : ""}`}
          onClick={() => setMaskTarget("subject")}
        >
          当前人物独立蒙版
        </button>
      </div>

      {status === "loading" ? (
        <p className="param-hint">
          <Loader2 size={11} className="spin" /> 正在重建 Alpha…
        </p>
      ) : null}
      {status === "error" && error ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <span>{error}</span>
        </div>
      ) : null}
      {preview ? (
        <p className="param-hint">
          覆盖 {Math.round(preview.coverage * 100)}% · 边缘像素{" "}
          {Math.round(preview.edgeRatio * 100)}% · {preview.strokes} 笔
        </p>
      ) : null}

      {disabledProviders.length > 0 ? (
        <>
          <div className="reshape-section-title is-disabled">自动分割模型（未启用）</div>
          <div className="reshape-disabled">
            {disabledProviders.map((provider) => (
              <div className="reshape-disabled-row" key={provider.id}>
                <span>
                  {provider.id} {provider.version}
                </span>
                <span className="reshape-disabled-reason">{provider.reason}</span>
              </div>
            ))}
          </div>
        </>
      ) : null}
    </div>
  );
}
