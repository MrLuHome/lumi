import { useState } from "react";
import { ChevronDown, Eraser, Paintbrush, Sparkles, Trash2, Undo2, Wrench } from "lucide-react";
import type { ExportFormat, FaceDetectionResult, PhotoAsset } from "../../types";
import type { EditParams } from "../../state/params";
import { findFaceBeauty } from "../../state/params";
import { useEditorStore } from "../../state/editorStore";
import { auditLegacyAdjustments } from "../../state/slim";
import { Slider } from "../ui/Slider";
import { Switch } from "../ui/Switch";

interface AdvancedSettingsProps {
  photo: PhotoAsset;
  params: EditParams;
  detection: FaceDetectionResult | null;
}

/**
 * 高级设置（默认收起）。
 *
 * 这里只放「不能用六项表达、但确实需要出口」的东西：
 * 逐人脸开关、修复画笔、旧调整清理、导出设置。
 * 旧调整清理是本轮兼容性的关键：塑形/身材/背景/HSL/曲线/滤镜仍然参与渲染，
 * 新界面看不到它们，必须在这里能被发现并清除。
 */
export function AdvancedSettings({ photo, params, detection }: AdvancedSettingsProps) {
  const [open, setOpen] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const brushMode = useEditorStore((state) => state.brushMode);
  const brushRadius = useEditorStore((state) => state.brushRadius);
  const setBrushMode = useEditorStore((state) => state.setBrushMode);
  const setBrushRadius = useEditorStore((state) => state.setBrushRadius);
  const setFaceEnabled = useEditorStore((state) => state.setFaceEnabled);
  const undoMaskStroke = useEditorStore((state) => state.undoMaskStroke);
  const clearMaskStrokes = useEditorStore((state) => state.clearMaskStrokes);
  const undoRepairPoint = useEditorStore((state) => state.undoRepairPoint);
  const clearRepairPoints = useEditorStore((state) => state.clearRepairPoints);
  const exportSettings = useEditorStore((state) => state.exportSettings);
  const setExportSettings = useEditorStore((state) => state.setExportSettings);
  const clearLegacy = useEditorStore((state) => state.clearLegacyAdjustments);
  const notify = useEditorStore((state) => state.notify);

  const legacy = auditLegacyAdjustments(params);
  const faces = detection?.faces ?? [];
  const legacyTotal = legacy.reduce((sum, item) => sum + item.count, 0);

  return (
    <section className={`advanced ${open ? "is-open" : ""}`}>
      <button
        type="button"
        className="advanced-toggle"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronDown size={12} className="chev" />
        <span>高级设置</span>
        <span className="advanced-meta">
          {legacy.length > 0 ? `${legacy.length} 项旧调整在生效` : "一般用不到"}
        </span>
      </button>

      {open ? (
        <div className="advanced-body">
          {faces.length >= 2 ? (
            <div className="advanced-block">
              <div className="section-title">分别调整每个人</div>
              <p className="hint-text">关掉某个人，六项改动就不会落在他身上。</p>
              {faces.map((face, index) => {
                const entry = findFaceBeauty(params, face.id);
                return (
                  <div className="advanced-row" key={face.uuid || face.id}>
                    <Switch
                      checked={entry ? entry.enabled : true}
                      label={`人物 ${index + 1}`}
                      onChange={(enabled) => setFaceEnabled(face.id, enabled)}
                    />
                    <span className="advanced-label">人物 {index + 1}</span>
                  </div>
                );
              })}
            </div>
          ) : null}

          <div className="advanced-block">
            <div className="section-title">点掉小瑕疵</div>
            <p className="hint-text">画笔涂过的位置会额外处理一遍；点选只修你点的那个点。</p>
            <div className="advanced-buttons">
              <button
                type="button"
                className={`btn ${brushMode === "paint" ? "btn-active" : ""}`}
                onClick={() => setBrushMode(brushMode === "paint" ? "off" : "paint")}
              >
                <Paintbrush size={13} />
                画笔
              </button>
              <button
                type="button"
                className={`btn ${brushMode === "erase" ? "btn-active" : ""}`}
                onClick={() => setBrushMode(brushMode === "erase" ? "off" : "erase")}
              >
                <Eraser size={13} />
                橡皮
              </button>
              <button
                type="button"
                className={`btn ${brushMode === "repair" ? "btn-active" : ""}`}
                onClick={() => setBrushMode(brushMode === "repair" ? "off" : "repair")}
              >
                <Wrench size={13} />
                点选修复
              </button>
            </div>
            <Slider
              label="笔刷大小"
              value={Math.round(brushRadius * 1000) / 10}
              min={0.4}
              max={20}
              step={0.1}
              bipolar={false}
              defaultValue={3}
              suffix="%"
              onChange={(value) => setBrushRadius(value / 100)}
            />
            <div className="advanced-buttons">
              <button
                type="button"
                className="mini-btn"
                disabled={params.manualMask.strokes.length === 0}
                onClick={() => undoMaskStroke()}
              >
                <Undo2 size={11} />
                撤销一笔
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={params.manualMask.strokes.length === 0}
                onClick={() => clearMaskStrokes()}
              >
                <Trash2 size={11} />
                清掉笔触（{params.manualMask.strokes.length}）
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={params.repairPoints.length === 0}
                onClick={() => undoRepairPoint()}
              >
                撤销一个点
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={params.repairPoints.length === 0}
                onClick={() => clearRepairPoints()}
              >
                清掉点选（{params.repairPoints.length}）
              </button>
            </div>
          </div>

          <div className="advanced-block">
            <div className="section-title">旧调整清理</div>
            {legacy.length === 0 ? (
              <p className="hint-text">没有旧调整在生效，画面就是你现在看到的设置。</p>
            ) : (
              <>
                <p className="hint-text">
                  还有 {legacy.length} 项旧调整在生效（共 {legacyTotal} 处），它们在这一版界面里看不到，
                  但依然会影响照片：
                </p>
                <ul className="advanced-legacy">
                  {legacy.map((item) => (
                    <li key={item.id}>
                      <span>{item.label}</span>
                      <em>{item.count} 处</em>
                    </li>
                  ))}
                </ul>
                {confirmClear ? (
                  <div className="advanced-buttons">
                    <button
                      type="button"
                      className="btn btn-accent"
                      onClick={() => {
                        clearLegacy();
                        setConfirmClear(false);
                        notify("success", "旧调整已全部清除，⌘Z 可撤销");
                      }}
                    >
                      确认清除
                    </button>
                    <button type="button" className="btn" onClick={() => setConfirmClear(false)}>
                      再想想
                    </button>
                  </div>
                ) : (
                  <button
                    type="button"
                    className="btn"
                    onClick={() => setConfirmClear(true)}
                  >
                    <Sparkles size={13} />
                    全部清除
                  </button>
                )}
              </>
            )}
          </div>

          <div className="advanced-block">
            <div className="section-title">导出</div>
            <div className="export-row">
              <label className="export-label" htmlFor="slim-export-format">
                格式
              </label>
              <select
                id="slim-export-format"
                className="export-select"
                value={exportSettings.format}
                onChange={(event) =>
                  setExportSettings({
                    ...exportSettings,
                    format: event.target.value as ExportFormat,
                  })
                }
              >
                <option value="jpeg">JPEG</option>
                <option value="png">PNG</option>
                <option value="webp">WebP</option>
              </select>
            </div>
            {exportSettings.format !== "png" ? (
              <Slider
                label="质量"
                value={exportSettings.quality}
                min={60}
                max={100}
                bipolar={false}
                defaultValue={92}
                suffix="%"
                onChange={(quality) => setExportSettings({ ...exportSettings, quality })}
              />
            ) : (
              <p className="param-hint">PNG 无压缩损失；16-bit 原图会保留高位深。</p>
            )}
            <p className="param-hint">
              {photo.fileName} · 导出始终用原始分辨率，原图文件不会被覆盖。
            </p>
          </div>
        </div>
      ) : null}
    </section>
  );
}
