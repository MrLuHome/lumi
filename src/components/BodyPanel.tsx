import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Anchor,
  Loader2,
  Lock,
  MousePointer2,
  Plus,
  RotateCcw,
  Trash2,
  Undo2,
  Unlock,
} from "lucide-react";
import type { BodyCapability, PhotoAsset } from "../types";
import { bodyAnchorPresets, bodyCapability as fetchBodyCapability } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { BODY_ANCHOR_LABELS, type BodyShapeKey, type WarpMode } from "../state/params";
import { selectActiveBodyAdjust, selectActiveParams, useEditorStore } from "../state/editorStore";
import { Slider } from "./ui/Slider";

interface BodyPanelProps {
  photo: PhotoAsset | null;
}

const SHAPE_FIELDS: { key: BodyShapeKey; label: string }[] = [
  { key: "waist", label: "收腰 / 放宽腰部" },
  { key: "shoulders", label: "肩宽" },
  { key: "hips", label: "胯部" },
  { key: "thighs", label: "大腿" },
  { key: "calves", label: "小腿" },
  { key: "legs", label: "腿部比例（受控）" },
  { key: "arms", label: "手臂" },
];

const BODY_TOOLS: { mode: WarpMode; label: string }[] = [
  { mode: "push", label: "推动" },
  { mode: "pull", label: "拉动" },
  { mode: "zoomIn", label: "局部放大" },
  { mode: "zoomOut", label: "局部缩小" },
  { mode: "protect", label: "保护" },
  { mode: "restore", label: "恢复" },
];

export function BodyPanel({ photo }: BodyPanelProps) {
  const params = useEditorStore(selectActiveParams);
  const adjust = useEditorStore(selectActiveBodyAdjust);
  const subjectId = useEditorStore((state) => state.bodySubjectId);
  const capability = useEditorStore((state) => state.bodyCapability);
  const tool = useEditorStore((state) => state.bodyTool);
  const radius = useEditorStore((state) => state.bodyRadius);
  const strength = useEditorStore((state) => state.bodyStrength);
  const feather = useEditorStore((state) => state.bodyFeather);
  const anchorRadius = useEditorStore((state) => state.bodyAnchorRadius);
  const showAnchors = useEditorStore((state) => state.showBodyAnchors);
  const reports = useEditorStore((state) => state.bodyReports);
  const setBodySubjectId = useEditorStore((state) => state.setBodySubjectId);
  const setCapability = useEditorStore((state) => state.setBodyCapability);
  const setTool = useEditorStore((state) => state.setBodyTool);
  const setRadius = useEditorStore((state) => state.setBodyRadius);
  const setStrength = useEditorStore((state) => state.setBodyStrength);
  const setFeather = useEditorStore((state) => state.setBodyFeather);
  const setAnchorRadius = useEditorStore((state) => state.setBodyAnchorRadius);
  const setShowAnchors = useEditorStore((state) => state.setShowBodyAnchors);
  const updateShape = useEditorStore((state) => state.updateBodyShape);
  const addAnchorPresets = useEditorStore((state) => state.addBodyAnchorPresets);
  const addAnchor = useEditorStore((state) => state.addBodyAnchor);
  const updateAnchor = useEditorStore((state) => state.updateBodyAnchor);
  const removeAnchor = useEditorStore((state) => state.removeBodyAnchor);
  const lockAnchor = useEditorStore((state) => state.lockBodyAnchor);
  const clearAnchors = useEditorStore((state) => state.clearBodyAnchors);
  const undoStroke = useEditorStore((state) => state.undoBodyStroke);
  const clearStrokes = useEditorStore((state) => state.clearBodyStrokes);
  const resetBody = useEditorStore((state) => state.resetBodyAdjust);
  const notify = useEditorStore((state) => state.notify);
  const [loading, setLoading] = useState(false);
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    fetchBodyCapability()
      .then((value: BodyCapability) => {
        if (!disposed) setCapability(value);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [desktop, setCapability]);

  if (!photo) return null;

  const subjectIds = Array.from(
    new Set([subjectId, ...params.body.map((entry) => entry.subjectId)]),
  ).sort((a, b) => a.localeCompare(b));
  const anchors = adjust?.anchors ?? [];
  const strokeCount = adjust?.strokes.length ?? 0;
  const maskReady = params.subjectMask.strokes.length > 0;
  const nextSubjectId = (() => {
    let index = subjectIds.length + 1;
    while (subjectIds.includes(`subject-${index}`)) index += 1;
    return `subject-${index}`;
  })();

  const suggestAnchors = async () => {
    if (!maskReady) {
      notify("info", "请先绘制手动人物蒙版，再生成锚点建议");
      return;
    }
    setLoading(true);
    try {
      const presets = await bodyAnchorPresets(photo.path, params.subjectMask);
      addAnchorPresets(presets);
      notify("success", `已添加 ${presets.length} 个锚点建议，可继续拖动或删除`);
    } catch (error) {
      notify("error", `锚点建议失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="reshape-panel">
      <div className="reshape-notice">
        身材调整使用手动蒙版轮廓 + 用户确认的锚点 + 局部网格变形，不依赖 AI 姿态识别，不做整图拉伸。
      </div>

      <p className="param-hint">
        {capability?.poseAvailable
          ? "已启用经过授权校验的姿态 Provider。"
          : capability?.reason ??
            "人体姿态模型未启用；身材调整只作用在手动蒙版内的人物区域。"}
      </p>

      <div className="reshape-section-title">当前人物</div>
      <div className="warp-actions">
        <select
          className="export-select"
          value={subjectId}
          aria-label="选择身材对象"
          onChange={(event) => setBodySubjectId(event.target.value)}
        >
          {subjectIds.map((id) => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
        <button type="button" className="mini-btn" onClick={() => setBodySubjectId(nextSubjectId)}>
          <Plus size={11} />
          新建人物
        </button>
      </div>
      <p className="param-hint">
        多人照片可分别为每个人建立对象；每个对象的位移只在自己蒙版内生效，互不影响。
      </p>

      <div className="reshape-section-title">身材参数</div>
      {!maskReady ? (
        <div className="reshape-warning is-info">
          <AlertTriangle size={12} />
          <span>尚未绘制人物蒙版：现在调整参数不会改变任何像素。</span>
        </div>
      ) : null}
      {SHAPE_FIELDS.map((field) => (
        <Slider
          key={field.key}
          label={field.label}
          value={adjust?.shape[field.key] ?? 0}
          min={-100}
          max={100}
          defaultValue={0}
          onChange={(value) => updateShape(field.key, value)}
        />
      ))}

      <div className="reshape-section-title">手动锚点</div>
      <div className="warp-actions">
        <button type="button" className="mini-btn" disabled={loading} onClick={() => void suggestAnchors()}>
          {loading ? <Loader2 size={11} className="spin" /> : <Anchor size={11} />}
          从轮廓建议锚点
        </button>
        <button
          type="button"
          className="mini-btn"
          onClick={() =>
            addAnchor({
              part: "custom",
              x: 0.5,
              y: 0.5,
              dx: 0,
              dy: 0,
              radius: anchorRadius,
              locked: false,
            })
          }
        >
          <Plus size={11} />
          添加锚点
        </button>
        <button
          type="button"
          className={`mini-btn ${showAnchors ? "is-active" : ""}`}
          onClick={() => setShowAnchors(!showAnchors)}
        >
          显示锚点
        </button>
      </div>
      <Slider
        label="锚点影响半径"
        value={Math.round(anchorRadius * 100)}
        min={2}
        max={50}
        bipolar={false}
        defaultValue={12}
        suffix="%"
        onChange={(value) => setAnchorRadius(value / 100)}
      />

      {anchors.length === 0 ? (
        <p className="param-hint">还没有锚点。可先生成建议，再在画布上拖动确认位置。</p>
      ) : (
        <div className="reshape-disabled">
          {anchors.map((anchor, index) => (
            <div className="reshape-disabled-row" key={`${anchor.part}-${index}`}>
              <span>{BODY_ANCHOR_LABELS[anchor.part]}</span>
              <span className="reshape-disabled-reason">
                {Math.round(anchor.x * 100)},{Math.round(anchor.y * 100)}
              </span>
              <button
                type="button"
                className="icon-btn"
                title={anchor.locked ? "解锁" : "锁定"}
                aria-label={anchor.locked ? "解锁锚点" : "锁定锚点"}
                onClick={() => lockAnchor(index, !anchor.locked)}
              >
                {anchor.locked ? <Lock size={11} /> : <Unlock size={11} />}
              </button>
              <button
                type="button"
                className="icon-btn"
                title="复位锚点位移"
                aria-label="复位锚点位移"
                onClick={() => updateAnchor(index, { dx: 0, dy: 0 })}
              >
                <RotateCcw size={11} />
              </button>
              <button
                type="button"
                className="icon-btn"
                title="删除锚点"
                aria-label="删除锚点"
                onClick={() => removeAnchor(index)}
              >
                <Trash2 size={11} />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="reshape-section-title">局部推拉</div>
      <div className="warp-tools">
        <button
          type="button"
          className={`warp-tool ${tool === "off" ? "is-active" : ""}`}
          onClick={() => setTool("off")}
        >
          <MousePointer2 size={12} />
          关闭
        </button>
        {BODY_TOOLS.map((item) => (
          <button
            key={item.mode}
            type="button"
            className={`warp-tool ${tool === item.mode ? "is-active" : ""}`}
            onClick={() => setTool(item.mode)}
          >
            {item.label}
          </button>
        ))}
      </div>
      <p className="param-hint">
        推拉只作用在人物蒙版内；保护笔触可保住背景与不需变形的区域。位移上限 12%，检测到网格折叠会自动降强度。
      </p>
      <Slider
        label="画笔半径"
        value={Math.round(radius * 100)}
        min={1}
        max={25}
        bipolar={false}
        defaultValue={8}
        suffix="%"
        onChange={(value) => setRadius(value / 100)}
      />
      <Slider
        label="推拉强度"
        value={strength}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={60}
        suffix="%"
        onChange={setStrength}
      />
      <Slider
        label="羽化"
        value={feather}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={50}
        suffix="%"
        onChange={setFeather}
      />

      <div className="warp-actions">
        <button
          type="button"
          className="mini-btn"
          disabled={strokeCount === 0}
          onClick={undoStroke}
        >
          <Undo2 size={11} />
          撤销单笔
        </button>
        <button
          type="button"
          className="mini-btn"
          disabled={strokeCount === 0}
          onClick={clearStrokes}
        >
          <RotateCcw size={11} />
          清除推拉
        </button>
        <button type="button" className="mini-btn" disabled={anchors.length === 0} onClick={clearAnchors}>
          清除锚点
        </button>
      </div>
      <div className="reshape-actions">
        <button type="button" className="mini-btn" onClick={resetBody}>
          重置该人物身材
        </button>
      </div>

      {reports.some((report) => report.degraded) ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <span>检测到网格折叠或位移超限，已自动降低强度以保护画面。</span>
        </div>
      ) : null}
    </div>
  );
}
