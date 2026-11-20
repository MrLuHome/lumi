import { useState } from "react";
import { ChevronDown, ClipboardPaste, Copy, Loader2, RotateCcw, Sparkles, Wand2, X } from "lucide-react";
import type { ExportFormat, PhotoAsset } from "../types";
import type { EditParams } from "../types";
import { PARAM_GROUPS, activeParamCount, isNeutral } from "../state/params";
import { useEditorStore } from "../state/editorStore";
import { autoWhiteBalance, pickLut } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { Slider } from "./ui/Slider";
import { HslPanel } from "./HslPanel";
import { CurveEditor } from "./CurveEditor";
import { FacePanel } from "./FacePanel";

interface InspectorProps {
  photo: PhotoAsset | null;
  params: EditParams;
}

export function Inspector({ photo, params }: InspectorProps) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [wbBusy, setWbBusy] = useState(false);
  const updateParam = useEditorStore((state) => state.updateParam);
  const updateParams = useEditorStore((state) => state.updateParams);
  const resetGroup = useEditorStore((state) => state.resetGroup);
  const resetAll = useEditorStore((state) => state.resetAll);
  const copyParams = useEditorStore((state) => state.copyParams);
  const pasteParams = useEditorStore((state) => state.pasteParams);
  const clipboard = useEditorStore((state) => state.clipboard);
  const setLut = useEditorStore((state) => state.setLut);
  const exportSettings = useEditorStore((state) => state.exportSettings);
  const setExportSettings = useEditorStore((state) => state.setExportSettings);
  const notify = useEditorStore((state) => state.notify);
  const activeCount = activeParamCount(params);
  const desktop = isDesktopRuntime();

  const isOpen = (id: string, fallback = true) => (collapsed[id] === undefined ? fallback : !collapsed[id]);
  const toggle = (id: string) =>
    setCollapsed((current) => {
      const open = current[id] === undefined ? true : !current[id];
      return { ...current, [id]: open };
    });

  const chooseLut = async () => {
    try {
      const picked = await pickLut();
      if (!picked) return;
      setLut({ path: picked.path, name: picked.name, strength: 100 });
      notify("success", `已应用 LUT：${picked.name}`);
    } catch (error) {
      notify("error", `LUT 加载失败：${error instanceof Error ? error.message : String(error)}`);
    }
  };

  const runAutoWhiteBalance = async () => {
    if (!photo) return;
    setWbBusy(true);
    try {
      const result = await autoWhiteBalance(photo.path);
      updateParams(
        (current) => ({ ...current, temperature: result.temperature, tint: result.tint }),
        "自动白平衡",
      );
      notify("success", `自动白平衡：色温 ${Math.round(result.temperature)} · 色调 ${Math.round(result.tint)}`);
    } catch (error) {
      notify("error", `自动白平衡失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setWbBusy(false);
    }
  };

  return (
    <aside className="inspector">
      <div className="inspector-head">
        <div>
          <span className="eyebrow">调整</span>
          <span className="inspector-count">
            {activeCount > 0 ? `${activeCount} 项已调整` : photo ? "未调整" : "等待照片"}
          </span>
        </div>
        <div className="inspector-head-actions">
          <button
            type="button"
            className="icon-btn"
            title="复制当前参数"
            aria-label="复制当前参数"
            disabled={!photo}
            onClick={copyParams}
          >
            <Copy size={12} />
          </button>
          <button
            type="button"
            className="icon-btn"
            title="粘贴参数"
            aria-label="粘贴参数"
            disabled={!photo || !clipboard}
            onClick={pasteParams}
          >
            <ClipboardPaste size={12} />
          </button>
          <button
            type="button"
            className="ghost-btn"
            onClick={resetAll}
            disabled={!photo || isNeutral(params)}
            title="重置全部参数"
          >
            <RotateCcw size={12} />
            重置全部
          </button>
        </div>
      </div>

      <div className={`inspector-body ${photo ? "" : "is-disabled"}`}>
        {PARAM_GROUPS.map((group) => {
          const open = isOpen(group.id);
          const groupDirty = group.params.some((definition) => params[definition.key] !== 0);
          return (
            <section className={`param-group ${open ? "is-open" : ""}`} key={group.id}>
              <header className="param-group-head">
                <button type="button" className="param-group-toggle" onClick={() => toggle(group.id)}>
                  <ChevronDown size={12} className="chev" />
                  <span>{group.label}</span>
                </button>
                <button
                  type="button"
                  className="param-group-reset"
                  onClick={() => resetGroup(group.id)}
                  disabled={!groupDirty}
                >
                  重置
                </button>
              </header>

              {open && (
                <div className="param-group-body">
                  {group.params.map((definition) => (
                    <Slider
                      key={definition.key}
                      label={definition.label}
                      value={params[definition.key]}
                      min={definition.min}
                      max={definition.max}
                      step={definition.step}
                      bipolar={definition.bipolar}
                      onChange={(value) => updateParam(definition.key, value)}
                    />
                  ))}
                  {group.id === "color" && (
                    <button
                      type="button"
                      className="mini-btn"
                      disabled={!photo || wbBusy || !desktop}
                      onClick={runAutoWhiteBalance}
                    >
                      {wbBusy ? <Loader2 size={11} className="spin" /> : <Wand2 size={11} />}
                      {wbBusy ? "计算白平衡…" : "自动白平衡"}
                    </button>
                  )}
                  {group.hint && (
                    <p className="param-hint">
                      <Sparkles size={11} />
                      {group.hint}
                    </p>
                  )}
                </div>
              )}
            </section>
          );
        })}

        <section className={`param-group ${isOpen("hsl") ? "is-open" : ""}`}>
          <header className="param-group-head">
            <button type="button" className="param-group-toggle" onClick={() => toggle("hsl")}>
              <ChevronDown size={12} className="chev" />
              <span>HSL</span>
            </button>
          </header>
          {isOpen("hsl") && (
            <div className="param-group-body">
              <HslPanel />
            </div>
          )}
        </section>

        <section className={`param-group ${isOpen("curves") ? "is-open" : ""}`}>
          <header className="param-group-head">
            <button type="button" className="param-group-toggle" onClick={() => toggle("curves")}>
              <ChevronDown size={12} className="chev" />
              <span>曲线</span>
            </button>
          </header>
          {isOpen("curves") && (
            <div className="param-group-body">
              <CurveEditor />
            </div>
          )}
        </section>

        <section className={`param-group ${isOpen("lut") ? "is-open" : ""}`}>
          <header className="param-group-head">
            <button type="button" className="param-group-toggle" onClick={() => toggle("lut")}>
              <ChevronDown size={12} className="chev" />
              <span>LUT</span>
            </button>
            {params.lut ? (
              <button
                type="button"
                className="param-group-reset"
                aria-label="移除 LUT"
                title="移除 LUT"
                onClick={() => setLut(null)}
              >
                <X size={11} />
              </button>
            ) : null}
          </header>
          {isOpen("lut") && (
            <div className="param-group-body">
              {params.lut ? (
                <>
                  <div className="lut-name" title={params.lut.path}>
                    {params.lut.name}
                  </div>
                  <Slider
                    label="LUT 强度"
                    value={params.lut.strength}
                    min={0}
                    max={100}
                    bipolar={false}
                    defaultValue={100}
                    suffix="%"
                    onChange={(strength) => setLut({ ...params.lut!, strength })}
                  />
                </>
              ) : (
                <p className="param-hint">支持 3D `.cube` LUT；1D LUT 会明确报错。</p>
              )}
              <button type="button" className="mini-btn" disabled={!desktop} onClick={chooseLut}>
                选择 .cube 文件…
              </button>
            </div>
          )}
        </section>

        <section className={`param-group ${isOpen("faces") ? "is-open" : ""}`}>
          <header className="param-group-head">
            <button type="button" className="param-group-toggle" onClick={() => toggle("faces")}>
              <ChevronDown size={12} className="chev" />
              <span>人脸信息</span>
            </button>
          </header>
          {isOpen("faces") && (
            <div className="param-group-body">
              <FacePanel photo={photo} />
            </div>
          )}
        </section>

        <section className={`param-group ${isOpen("export") ? "is-open" : ""}`}>
          <header className="param-group-head">
            <button type="button" className="param-group-toggle" onClick={() => toggle("export")}>
              <ChevronDown size={12} className="chev" />
              <span>导出设置</span>
            </button>
          </header>
          {isOpen("export") && (
            <div className="param-group-body">
              <div className="export-row">
                <label className="export-label" htmlFor="export-format">
                  格式
                </label>
                <select
                  id="export-format"
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
              <p className="param-hint">导出始终使用原始分辨率，原图文件不会被覆盖。</p>
            </div>
          )}
        </section>
      </div>

      <div className="inspector-foot">
        <span>{desktop ? "Rust 本地渲染" : "浏览器预览模式"}</span>
        <span>
          {photo
            ? `预览 ${photo.previewWidth} × ${photo.previewHeight} · 导出原图 ${photo.width} × ${photo.height}`
            : "尚未载入照片"}
        </span>
      </div>
    </aside>
  );
}
