import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Check,
  Copy,
  Grid3X3,
  Loader2,
  MousePointer2,
  RotateCcw,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import type { FaceMatchEntry, PhotoAsset, ProjectFaceRecord, ReshapeCapability } from "../types";
import { confirmFaceMatch, reshapeCapability } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import type { FaceShapeKey, WarpMode } from "../state/params";
import { defaultFaceReshape, findFaceReshape } from "../state/params";
import {
  selectActiveFace,
  selectActiveFaces,
  selectActiveParams,
  useEditorStore,
} from "../state/editorStore";
import { Slider } from "./ui/Slider";

interface ReshapePanelProps {
  photo: PhotoAsset | null;
}

const SHAPE_FIELDS: { key: FaceShapeKey; label: string }[] = [
  { key: "slim", label: "整体瘦脸" },
  { key: "width", label: "面部宽度" },
  { key: "jaw", label: "下颌线" },
  { key: "cheekbone", label: "颧骨宽度" },
  { key: "length", label: "面部长度" },
  { key: "left", label: "左侧轮廓" },
  { key: "right", label: "右侧轮廓" },
  { key: "balance", label: "左右平衡" },
];

const WARP_TOOLS: { mode: WarpMode; label: string }[] = [
  { mode: "push", label: "推动" },
  { mode: "pull", label: "拉动" },
  { mode: "zoomIn", label: "局部放大" },
  { mode: "zoomOut", label: "局部缩小" },
  { mode: "protect", label: "保护" },
  { mode: "restore", label: "恢复" },
];

// Zustand 快照必须保持引用稳定：选择器里每次返回新的 `[]` 会让 React 判定快照持续变化，
// 触发 “getSnapshot should be cached” 并最终抛 maximum update depth（窗口空白）。
const EMPTY_MATCHES: FaceMatchEntry[] = [];
const EMPTY_RECORDS: ProjectFaceRecord[] = [];

export function ReshapePanel({ photo }: ReshapePanelProps) {
  const params = useEditorStore(selectActiveParams);
  const face = useEditorStore(selectActiveFace);
  const faces = useEditorStore(selectActiveFaces)?.faces ?? [];
  const activeIndex = useEditorStore((state) => state.activeFace);
  const warpTool = useEditorStore((state) => state.warpTool);
  const warpRadius = useEditorStore((state) => state.warpRadius);
  const warpStrength = useEditorStore((state) => state.warpStrength);
  const warpFeather = useEditorStore((state) => state.warpFeather);
  const showMesh = useEditorStore((state) => state.showMesh);
  const renderStatus = useEditorStore((state) => state.renderStatus);
  const renderWarnings = useEditorStore((state) => state.renderWarnings);
  const reshapeReports = useEditorStore((state) => state.reshapeReports);
  const pending = useEditorStore(
    (state) => (photo && state.pendingFaceMatches[photo.id]) || EMPTY_MATCHES,
  );
  const missing = useEditorStore(
    (state) => (photo && state.missingFaceRecords[photo.id]) || EMPTY_RECORDS,
  );
  const setWarpTool = useEditorStore((state) => state.setWarpTool);
  const setWarpRadius = useEditorStore((state) => state.setWarpRadius);
  const setWarpStrength = useEditorStore((state) => state.setWarpStrength);
  const setWarpFeather = useEditorStore((state) => state.setWarpFeather);
  const setShowMesh = useEditorStore((state) => state.setShowMesh);
  const updateFaceShape = useEditorStore((state) => state.updateFaceShape);
  const updateFaceFeature = useEditorStore((state) => state.updateFaceFeature);
  const undoWarpStroke = useEditorStore((state) => state.undoWarpStroke);
  const clearWarpStrokes = useEditorStore((state) => state.clearWarpStrokes);
  const resetFaceReshape = useEditorStore((state) => state.resetFaceReshape);
  const resetAllReshape = useEditorStore((state) => state.resetAllReshape);
  const copyFaceReshapeToOthers = useEditorStore((state) => state.copyFaceReshapeToOthers);
  const applyConfirmedFaces = useEditorStore((state) => state.applyConfirmedFaces);
  const notify = useEditorStore((state) => state.notify);
  const [capability, setCapability] = useState<ReshapeCapability | null>(null);
  const [confirming, setConfirming] = useState<number | null>(null);
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    reshapeCapability()
      .then((status) => {
        if (!disposed) setCapability(status);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [desktop]);

  const entry = face ? findFaceReshape(params, face.id) ?? defaultFaceReshape(face.id, face.uuid) : null;
  const strokeCount = entry?.manual.strokes.length ?? 0;
  const level3 = faces.find((item) => item.id === face?.id)?.level3 ?? null;
  const level3Unlocked =
    !!level3 && (level3.eyes.enabled || level3.nose.enabled || level3.mouth.enabled || level3.chin.enabled);
  const level3Groups = [
    {
      key: "eyes" as const,
      label: "眼部",
      gate: level3?.eyes,
      fields: [
        ["size", "整体大小"],
        ["width", "眼形宽度"],
        ["height", "眼形高度"],
        ["distance", "眼距"],
        ["corner", "眼角"],
      ] as const,
    },
    {
      key: "nose" as const,
      label: "鼻部",
      gate: level3?.nose,
      fields: [
        ["bridge", "鼻梁宽度"],
        ["alar", "鼻翼宽度"],
        ["tip", "鼻头大小"],
        ["balance", "鼻部平衡"],
      ] as const,
    },
    {
      key: "mouth" as const,
      label: "唇部",
      gate: level3?.mouth,
      fields: [
        ["width", "唇宽"],
        ["upper", "上唇厚度"],
        ["lower", "下唇厚度"],
        ["corner", "嘴角"],
        ["size", "唇部大小"],
      ] as const,
    },
    {
      key: "chin" as const,
      label: "下巴",
      gate: level3?.chin,
      fields: [
        ["length", "下巴长度"],
        ["width", "下巴宽度"],
        ["jawline", "下颌线"],
      ] as const,
    },
  ];

  if (!photo) return null;

  if (!face) {
    return (
      <div className="reshape-panel">
        <p className="param-hint">
          请先在「人脸信息」中检测人脸。面部塑形需要真实检测框；没有检测结果时不会输出任何变形。
        </p>
      </div>
    );
  }

  const confirm = async (faceId: number, faceUuid: string) => {
    setConfirming(faceId);
    try {
      const result = await confirmFaceMatch(photo.path, faceId, faceUuid);
      applyConfirmedFaces(photo.id, result.faces);
      notify("success", "已确认人脸匹配，参数已恢复到对应的人");
    } catch (error) {
      notify("error", `确认失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setConfirming(null);
    }
  };

  return (
    <div className="reshape-panel">
      <div className="reshape-face-row">
        <span className="reshape-face-label">
          当前人物 #{Math.min(activeIndex + 1, faces.length)} / {faces.length}
        </span>
        <button
          type="button"
          className="mini-btn"
          disabled={faces.length < 2}
          title="把当前人物塑形参数复制到其他人物"
          onClick={() => copyFaceReshapeToOthers(face.id)}
        >
          <Copy size={11} />
          复制到其他
        </button>
      </div>

      {pending.length > 0 ? (
        <div className="reshape-warning is-info">
          <AlertTriangle size={12} />
          <span>{pending.length} 张人脸匹配置信度不足，确认前不会套用旧参数。</span>
        </div>
      ) : null}
      {pending.map((match) => (
        <div className="reshape-confirm" key={`${match.faceId}-${match.faceUuid}`}>
          <span>
            检测 #{faces.findIndex((item) => item.id === match.faceId) + 1} ↔ 记录{" "}
            {match.faceUuid.slice(0, 8)}（{Math.round(match.score * 100)}%）
          </span>
          <button
            type="button"
            className="mini-btn"
            disabled={confirming === match.faceId}
            onClick={() => void confirm(match.faceId, match.faceUuid)}
          >
            {confirming === match.faceId ? (
              <Loader2 size={11} className="spin" />
            ) : (
              <Check size={11} />
            )}
            确认
          </button>
        </div>
      ))}
      {missing.length > 0 ? (
        <p className="param-hint">
          本次未检测到 {missing.length} 张已保存的人脸；编辑记录仍保留在项目中，不会被删除。
        </p>
      ) : null}

      <div className="reshape-notice">
        轮廓级近似调整，不代表精确面部关键点识别。仅调整脸缘轮廓与整体比例。
      </div>

      <div className="reshape-section-title">轮廓级塑形</div>
      {SHAPE_FIELDS.map((field) => (
        <Slider
          key={field.key}
          label={field.label}
          value={entry?.face[field.key] ?? 0}
          min={-100}
          max={100}
          defaultValue={0}
          onChange={(value) => updateFaceShape(face.id, field.key, value)}
        />
      ))}

      <div className="reshape-section-title">局部手动变形</div>
      <div className="warp-tools">
        <button
          type="button"
          className={`warp-tool ${warpTool === "off" ? "is-active" : ""}`}
          onClick={() => setWarpTool("off")}
          title="关闭手动变形，恢复画布平移"
        >
          <MousePointer2 size={12} />
          关闭
        </button>
        {WARP_TOOLS.map((tool) => (
          <button
            key={tool.mode}
            type="button"
            className={`warp-tool ${warpTool === tool.mode ? "is-active" : ""}`}
            onClick={() => setWarpTool(tool.mode)}
          >
            {tool.mode === "zoomIn" ? <ZoomIn size={12} /> : null}
            {tool.mode === "zoomOut" ? <ZoomOut size={12} /> : null}
            {tool.label}
          </button>
        ))}
      </div>
      <p className="param-hint">
        在画布上按住拖动。坐标按原图计算，缩放后依然准确；每一笔都会记录为可撤销的变形。
      </p>
      <Slider
        label="画笔半径"
        value={Math.round(warpRadius * 100)}
        min={1}
        max={25}
        bipolar={false}
        defaultValue={6}
        suffix="%"
        onChange={(value) => setWarpRadius(value / 100)}
      />
      <Slider
        label="变形强度"
        value={warpStrength}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={60}
        suffix="%"
        onChange={setWarpStrength}
      />
      <Slider
        label="羽化"
        value={warpFeather}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={50}
        suffix="%"
        onChange={setWarpFeather}
      />
      <div className="warp-actions">
        <button
          type="button"
          className="mini-btn"
          disabled={strokeCount === 0}
          onClick={() => undoWarpStroke(face.id)}
        >
          <Undo2 size={11} />
          撤销单笔
        </button>
        <button
          type="button"
          className="mini-btn"
          disabled={strokeCount === 0}
          onClick={() => clearWarpStrokes(face.id)}
        >
          <RotateCcw size={11} />
          清除笔触
        </button>
        <button
          type="button"
          className={`mini-btn ${showMesh ? "is-active" : ""}`}
          onClick={() => setShowMesh(!showMesh)}
          title="显示真实控制网格"
        >
          <Grid3X3 size={11} />
          网格
        </button>
      </div>
      <div className="reshape-actions">
        <button type="button" className="mini-btn" onClick={() => resetFaceReshape(face.id)}>
          重置该人脸
        </button>
        <button
          type="button"
          className="mini-btn"
          disabled={params.reshape.length === 0}
          onClick={resetAllReshape}
        >
          重置全部塑形
        </button>
      </div>

      {renderStatus === "rendering" ? (
        <p className="param-hint">
          <Loader2 size={11} className="spin" /> 正在重算变形…
        </p>
      ) : null}
      {renderWarnings.length > 0 ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <div>
            {renderWarnings.map((warning) => (
              <div key={warning}>{warning}</div>
            ))}
          </div>
        </div>
      ) : null}
      {reshapeReports.some((report) => report.degraded) ? (
        <p className="param-hint">
          检测到网格折叠或位移超限，已自动降低强度以保护画面。请减少参数或撤销笔触。
        </p>
      ) : null}

      <div className={`reshape-section-title ${level3Unlocked ? "" : "is-disabled"}`}>
        五官轮廓精修（LEVEL 3 · 结构门控{level3Unlocked ? " · 已解锁" : " · 未解锁"}）
      </div>
      {level3Unlocked ? (
        <>
          <p className="param-hint">
            当前人脸使用 {level3?.providerId ?? "AI Provider"}{" "}
            {level3?.providerVersion ?? ""} 的稠密轮廓（{level3?.topologyId ?? "—"} ·{" "}
            {level3?.modelVersion || "模型版本未记录"}）。参数只作用于实测通过的部位。
          </p>
          <p className="param-hint">
            稳定性、结构验证已通过；定位精度尚未经人工标注验证，这里的“通过”不代表五官位置准确。
          </p>
          {level3Groups.map((group) =>
            group.gate?.enabled ? (
              <div key={group.key}>
                <div className="reshape-section-title is-sub">{group.label}</div>
                {group.fields.map(([field, label]) => (
                  <Slider
                    key={field}
                    label={label}
                    value={
                      entry
                        ? ((entry[group.key] as unknown as Record<string, number>)[field] ?? 0)
                        : 0
                    }
                    min={-100}
                    max={100}
                    defaultValue={0}
                    onChange={(value) => updateFaceFeature(face.id, group.key, { [field]: value })}
                  />
                ))}
              </div>
            ) : (
              <div className="reshape-disabled-row" key={group.key}>
                <span>{group.label}</span>
                <span className="reshape-disabled-reason">
                  {group.gate?.reason ?? "该部位未通过质量验证"}
                </span>
              </div>
            ),
          )}
        </>
      ) : (
        <div className="reshape-disabled">
          <p>
            {level3?.summary ??
              capability?.note ??
              "精确眼睑、鼻翼、唇线、眼角需要先通过质量验证的稠密关键点。当前人脸保持锁定，不会产生任何 LEVEL 3 变形。"}
          </p>
          {level3Groups.map((group) => (
            <div className="reshape-disabled-row" key={group.key}>
              <span>{group.label}</span>
              <span className="reshape-disabled-reason">
                {group.gate?.reason ?? "缺少稠密关键点"}
              </span>
            </div>
          ))}
          {!level3 ? (
            <p className="param-hint">
              在「AI 人像」工作区选择 Provider 并点击「分析当前照片」，或先运行「质量验证」。
            </p>
          ) : null}
        </div>
      )}
    </div>
  );
}
