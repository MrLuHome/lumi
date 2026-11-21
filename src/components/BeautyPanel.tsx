import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Brush,
  ChevronDown,
  Copy,
  Eraser,
  Eye,
  EyeOff,
  Loader2,
  RotateCcw,
  ScanFace,
  ShieldCheck,
  Sparkles,
  Target,
  Undo2,
} from "lucide-react";
import type { BeautyParams, PhotoAsset } from "../types";
import { defaultBeauty, findFaceBeauty } from "../state/params";
import {
  selectActiveBeautyAnalysis,
  selectActiveFace,
  selectActiveFaceBeauty,
  selectActiveFaces,
  selectActiveParams,
  useEditorStore,
} from "../state/editorStore";
import { analyzeBeauty, detectFaces, previewSkinMask, skinModelStatus } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { Slider } from "./ui/Slider";

interface BeautyPanelProps {
  photo: PhotoAsset | null;
}

const PRESETS: { id: string; label: string; beauty: BeautyParams }[] = [
  { id: "light", label: "轻度", beauty: { ...defaultBeauty(), skinSmooth: 22, texture: 55, detail: 25, blemish: 20, toneEven: 12 } },
  { id: "natural", label: "自然", beauty: { ...defaultBeauty(), skinSmooth: 32, texture: 50, detail: 25, blemish: 30, spots: 15, toneEven: 18, darkCircles: 12 } },
  { id: "strong", label: "强力", beauty: { ...defaultBeauty(), skinSmooth: 48, texture: 40, detail: 30, blemish: 45, spots: 25, toneEven: 30, whitening: 12, darkCircles: 20 } },
];

function faceLetter(index: number): string {
  return String.fromCharCode(65 + (index % 26));
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** AI 美肤面板：所有按钮都调用真实后端，不产生“演示用固定效果”。 */
export function BeautyPanel({ photo }: BeautyPanelProps) {
  const desktop = isDesktopRuntime();
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const faceResult = useEditorStore(selectActiveFaces);
  const activeFaceIndex = useEditorStore((state) => state.activeFace);
  const activeFace = useEditorStore(selectActiveFace);
  const activeFaceBeauty = useEditorStore(selectActiveFaceBeauty);
  const analysis = useEditorStore(selectActiveBeautyAnalysis);
  const beautyStatus = useEditorStore((state) => state.beautyStatus);
  const beautyProgress = useEditorStore((state) => state.beautyProgress);
  const beautyError = useEditorStore((state) => state.beautyError);
  const skinModel = useEditorStore((state) => state.skinModel);
  const faceStatus = useEditorStore((state) => state.faceStatus);
  const faceError = useEditorStore((state) => state.faceError);
  const faceProgress = useEditorStore((state) => state.faceProgress);

  const maskVisible = useEditorStore((state) => state.maskVisible);
  const maskPreview = useEditorStore((state) => state.maskPreview);
  const maskStatus = useEditorStore((state) => state.maskStatus);
  const maskError = useEditorStore((state) => state.maskError);
  const brushMode = useEditorStore((state) => state.brushMode);
  const brushRadius = useEditorStore((state) => state.brushRadius);

  const setBeautyStatus = useEditorStore((state) => state.setBeautyStatus);
  const setBeautyError = useEditorStore((state) => state.setBeautyError);
  const setBeautyAnalysis = useEditorStore((state) => state.setBeautyAnalysis);
  const setSkinModel = useEditorStore((state) => state.setSkinModel);
  const updateFaceBeauty = useEditorStore((state) => state.updateFaceBeauty);
  const setFaceEnabled = useEditorStore((state) => state.setFaceEnabled);
  const resetFaceBeauty = useEditorStore((state) => state.resetFaceBeauty);
  const applyBeautySuggestion = useEditorStore((state) => state.applyBeautySuggestion);
  const copyFaceParamsToOthers = useEditorStore((state) => state.copyFaceParamsToOthers);
  const updateParams = useEditorStore((state) => state.updateParams);
  const updateManualMask = useEditorStore((state) => state.updateManualMask);
  const undoMaskStroke = useEditorStore((state) => state.undoMaskStroke);
  const clearMaskStrokes = useEditorStore((state) => state.clearMaskStrokes);
  const undoRepairPoint = useEditorStore((state) => state.undoRepairPoint);
  const clearRepairPoints = useEditorStore((state) => state.clearRepairPoints);
  const setMaskVisible = useEditorStore((state) => state.setMaskVisible);
  const setMaskPreview = useEditorStore((state) => state.setMaskPreview);
  const setMaskStatus = useEditorStore((state) => state.setMaskStatus);
  const setMaskError = useEditorStore((state) => state.setMaskError);
  const setBrushMode = useEditorStore((state) => state.setBrushMode);
  const setBrushRadius = useEditorStore((state) => state.setBrushRadius);
  const setActiveFace = useEditorStore((state) => state.setActiveFace);
  const notify = useEditorStore((state) => state.notify);

  const params = useEditorStore(selectActiveParams);
  const manualKey = useMemo(() => JSON.stringify(params.manualMask), [params.manualMask]);

  const isOpen = (id: string, fallback = true) =>
    collapsed[id] === undefined ? fallback : !collapsed[id];
  const toggle = (id: string) =>
    setCollapsed((current) => {
      const open = current[id] === undefined ? true : !current[id];
      return { ...current, [id]: open };
    });

  // 皮肤分割能力：如实展示当前生效的方法，不把几何蒙版说成神经网络。
  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    skinModelStatus()
      .then((status) => {
        if (!disposed) setSkinModel(status);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [desktop, setSkinModel]);

  // 打开含人脸的照片时自动执行一次真实检测，让滑块与蒙版有可用几何。
  useEffect(() => {
    if (!desktop || !photo || faceResult || faceStatus === "loading") return;
    const state = useEditorStore.getState();
    let cancelled = false;
    state.setFaceStatus("loading");
    detectFaces(photo.path)
      .then((result) => {
        if (!cancelled) useEditorStore.getState().setFaces(photo.id, result);
      })
      .catch((error) => {
        if (!cancelled) useEditorStore.getState().setFaceError(toMessage(error));
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, photo, faceResult, faceStatus]);

  // 蒙版显示：只在“显示蒙版”打开时请求，且只在手动蒙版/照片变化时重算。
  useEffect(() => {
    if (!desktop || !photo || !maskVisible || !faceResult || faceResult.faces.length === 0) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      setMaskStatus("loading");
      previewSkinMask(photo.path, selectActiveParams(useEditorStore.getState()))
        .then((preview) => {
          if (!cancelled) setMaskPreview(preview);
        })
        .catch((error) => {
          if (!cancelled) setMaskError(toMessage(error));
        });
    }, 220);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [desktop, photo, maskVisible, manualKey, faceResult, setMaskPreview, setMaskStatus, setMaskError]);

  const runAnalysis = async (apply: boolean) => {
    if (!photo || !desktop) return;
    setBeautyStatus("analyzing");
    try {
      if (!faceResult) {
        const detection = await detectFaces(photo.path);
        useEditorStore.getState().setFaces(photo.id, detection);
      }
      const result = await analyzeBeauty(photo.path);
      setBeautyAnalysis(photo.id, result);
      if (result.faces.length === 0) {
        notify("info", "没有可分析的皮肤区域（可能未检测到人脸或遮挡严重）");
        return;
      }
      if (apply) {
        const suggested = new Map(result.faces.map((face) => [face.faceId, face.suggested]));
        updateParams(
          (current) => ({
            ...current,
            faces: current.faces.map((face) =>
              suggested.has(face.faceId)
                ? { ...face, enabled: true, beauty: { ...suggested.get(face.faceId)! } }
                : face,
            ),
          }),
          "一键美肤",
        );
        notify(
          "success",
          `已应用 ${result.faces.length} 张脸的建议参数 · 可逐项调整 · ${result.elapsedMs} ms`,
        );
      } else {
        notify("success", `皮肤分析完成 · ${result.faces.length} 张脸 · ${result.elapsedMs} ms`);
      }
    } catch (error) {
      const message = toMessage(error);
      setBeautyError(message);
      notify("error", `美肤分析失败：${message}`);
    }
  };

  const activeNotes = analysis?.faces.find((face) => face.faceId === activeFace?.id)?.notes ?? [];
  const activeMetrics = analysis?.faces.find((face) => face.faceId === activeFace?.id)?.metrics;
  const faces = faceResult?.faces ?? [];
  const hasFace = Boolean(activeFace && activeFaceBeauty);
  const busy = beautyStatus === "analyzing" || faceStatus === "loading";
  const progress = beautyStatus === "analyzing" ? beautyProgress : faceProgress;

  const update = (key: keyof BeautyParams, value: number) => {
    if (!activeFace) return;
    updateFaceBeauty(activeFace.id, key, value);
  };

  const applyPreset = (preset: (typeof PRESETS)[number]) => {
    if (!activeFace) return;
    applyBeautySuggestion(activeFace.id, preset.beauty, `应用${preset.label}档`);
  };

  const beauty = activeFaceBeauty?.beauty ?? defaultBeauty();
  const disabled = !photo || !hasFace || !desktop;

  const renderSlider = (key: keyof BeautyParams, label: string, hint?: string) => (
    <div className="beauty-slider" key={key}>
      <Slider
        label={label}
        value={beauty[key]}
        min={0}
        max={100}
        step={1}
        bipolar={false}
        defaultValue={0}
        onChange={(value) => update(key, value)}
      />
      {hint ? <span className="beauty-slider-hint">{hint}</span> : null}
    </div>
  );

  return (
    <div className="beauty-panel">
      <section className={`param-group ${isOpen("ai") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("ai")}>
            <ChevronDown size={12} className="chev" />
            <span>美肤精修</span>
          </button>
          <span className={`beauty-badge ${analysis ? "is-ready" : ""}`}>
            {analysis ? "已分析" : "未分析"}
          </span>
        </header>
        {isOpen("ai") && (
          <div className="param-group-body">
            <div className="beauty-action-row">
              <button
                type="button"
                className="btn btn-accent beauty-primary"
                disabled={!photo || !desktop || busy}
                onClick={() => void runAnalysis(true)}
              >
                {beautyStatus === "analyzing" ? (
                  <Loader2 size={13} className="spin" />
                ) : (
                  <Sparkles size={13} />
                )}
                一键美肤
              </button>
              <button
                type="button"
                className="btn beauty-secondary"
                disabled={!photo || !desktop || busy}
                onClick={() => void runAnalysis(false)}
              >
                {faceStatus === "loading" ? (
                  <Loader2 size={13} className="spin" />
                ) : (
                  <ScanFace size={13} />
                )}
                自动皮肤分析
              </button>
            </div>

            {busy && progress ? (
              <div className="face-progress">
                <div className="progress-bar">
                  <span style={{ width: `${Math.round(progress.progress * 100)}%` }} />
                </div>
                <div className="progress-text">
                  <span>{progress.stage}</span>
                  <span>{Math.round(progress.progress * 100)}%</span>
                </div>
              </div>
            ) : null}

            {beautyStatus === "error" && beautyError ? (
              <p className="face-error">
                <AlertTriangle size={12} />
                {beautyError}
              </p>
            ) : null}
            {faceStatus === "error" && faceError ? (
              <p className="face-error">
                <AlertTriangle size={12} />
                {faceError}
              </p>
            ) : null}

            <div className="beauty-model">
              <div className="beauty-model-line">
                <span className={`face-badge ${skinModel?.available ? "is-ok" : "is-warn"}`}>
                  {skinModel?.available ? "ONNX 分割" : "几何蒙版"}
                </span>
                <span className="beauty-model-name">{skinModel?.name ?? "关键点皮肤区域"}</span>
              </div>
              <div className="beauty-model-meta">
                {skinModel ? `${skinModel.license} · v${skinModel.version}` : "读取中…"}
              </div>
              {skinModel && !skinModel.available ? (
                <div className="beauty-model-reason" title={skinModel.reason}>
                  {skinModel.reason}
                </div>
              ) : null}
            </div>

            {analysis ? (
              <div className="beauty-analysis">
                <div className="beauty-analysis-head">
                  {activeFace ? `人脸 ${faceLetter(activeFaceIndex)} 分析` : "分析结果"} ·{" "}
                  {analysis.elapsedMs} ms
                </div>
                {activeMetrics ? (
                  <div className="beauty-metrics">
                    <span>皮肤占比 {(activeMetrics.skinRatio * 100).toFixed(0)}%</span>
                    <span>纹理 {(activeMetrics.texture * 100).toFixed(1)}</span>
                    <span>瑕疵 {(activeMetrics.blemishDensity * 100).toFixed(1)}%</span>
                    <span>泛红 {activeMetrics.redness.toFixed(1)}</span>
                    <span>高光 {(activeMetrics.highlightRatio * 100).toFixed(0)}%</span>
                  </div>
                ) : null}
                {activeNotes.length > 0 ? (
                  <ul className="beauty-notes">
                    {activeNotes.slice(0, 5).map((note) => (
                      <li key={note}>{note}</li>
                    ))}
                  </ul>
                ) : null}
                <div className="beauty-action-row">
                  <button
                    type="button"
                    className="mini-btn"
                    disabled={disabled || !activeFace}
                    onClick={() => {
                      const suggestion = analysis.faces.find(
                        (face) => face.faceId === activeFace?.id,
                      );
                      if (suggestion) {
                        applyBeautySuggestion(suggestion.faceId, suggestion.suggested);
                        notify("success", "已应用本脸建议，可继续微调");
                      }
                    }}
                  >
                    应用本脸建议
                  </button>
                  {analysis.warnings.length > 0 ? (
                    <span className="beauty-warning" title={analysis.warnings.join("\n")}>
                      {analysis.warnings.length} 条提示
                    </span>
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("texture") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("texture")}>
            <ChevronDown size={12} className="chev" />
            <span>皮肤质感</span>
          </button>
          <button
            type="button"
            className="param-group-reset"
            disabled={disabled}
            onClick={() => activeFace && resetFaceBeauty(activeFace.id)}
          >
            <RotateCcw size={11} />
          </button>
        </header>
        {isOpen("texture") && (
          <div className="param-group-body">
            <div className="beauty-presets">
              {PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className="mini-btn"
                  disabled={disabled}
                  onClick={() => applyPreset(preset)}
                >
                  {preset.label}
                </button>
              ))}
            </div>
            {renderSlider("skinSmooth", "磨皮", "0–100 · 默认自然档")}
            {renderSlider("texture", "纹理保护", "保留毛孔与颗粒")}
            {renderSlider("detail", "细节恢复", "补回真实高频细节")}
            <p className="param-hint">
              <ShieldCheck size={11} />
              多尺度低频平滑 + 高频回注，不使用整脸模糊。
            </p>
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("blemish") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("blemish")}>
            <ChevronDown size={12} className="chev" />
            <span>瑕疵修复</span>
          </button>
          <span className="beauty-badge">保守</span>
        </header>
        {isOpen("blemish") && (
          <div className="param-group-body">
            {renderSlider("blemish", "自动祛痘")}
            {renderSlider("spots", "祛斑")}
            {renderSlider("blackheads", "黑头")}
            <div className="beauty-brush-row">
              <button
                type="button"
                className={`mini-btn ${brushMode === "paint" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  setBrushMode(brushMode === "paint" ? "off" : "paint");
                  setMaskVisible(true);
                }}
              >
                <Brush size={11} />
                修复画笔
              </button>
              <button
                type="button"
                className={`mini-btn ${brushMode === "erase" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  setBrushMode(brushMode === "erase" ? "off" : "erase");
                  setMaskVisible(true);
                }}
              >
                <Eraser size={11} />
                橡皮擦
              </button>
              <button
                type="button"
                className={`mini-btn ${brushMode === "repair" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  if (brushMode !== "repair" && beauty.blemish < 60) {
                    update("blemish", 60);
                  }
                  setBrushMode(brushMode === "repair" ? "off" : "repair");
                }}
              >
                <Target size={11} />
                手动点选
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={!desktop || params.repairPoints.length === 0}
                onClick={undoRepairPoint}
              >
                <Undo2 size={11} />
                撤销修复点
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={!desktop || params.repairPoints.length === 0}
                onClick={clearRepairPoints}
              >
                清除点
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={!desktop || !params.manualMask.strokes.length}
                onClick={undoMaskStroke}
              >
                <Undo2 size={11} />
                撤销笔触
              </button>
            </div>
            {brushMode === "repair" ? (
              <p className="param-hint">
                在画布上点击瑕疵即可定点修复；已有 {params.repairPoints.length} 个修复点，可逐个撤销。
              </p>
            ) : null}
            {brushMode !== "off" ? (
              <Slider
                label="画笔半径"
                value={Math.round(brushRadius * 1000)}
                min={4}
                max={200}
                step={1}
                bipolar={false}
                defaultValue={30}
                onChange={(value) => setBrushRadius(value / 1000)}
              />
            ) : null}
            <p className="param-hint">
              画笔只在皮肤蒙版上扩展修复范围，不直接涂抹像素；痣与天然雀斑由残差保护。
            </p>
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("tone") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("tone")}>
            <ChevronDown size={12} className="chev" />
            <span>肤色</span>
          </button>
        </header>
        {isOpen("tone") && (
          <div className="param-group-body">
            {renderSlider("toneEven", "肤色均匀")}
            {renderSlider("whitening", "自然美白")}
            {renderSlider("redness", "泛红改善")}
            {renderSlider("dullness", "暗沉改善")}
            <p className="param-hint">Lab 感知空间处理，保护高光与阴影，逐项独立强度。</p>
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("eyes") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("eyes")}>
            <ChevronDown size={12} className="chev" />
            <span>眼周</span>
          </button>
        </header>
        {isOpen("eyes") && (
          <div className="param-group-body">
            {renderSlider("darkCircles", "黑眼圈淡化")}
            {renderSlider("eyeLines", "眼下细纹")}
            {renderSlider("foreheadLines", "额头细纹")}
            {renderSlider("nasolabial", "法令纹")}
            <p className="param-hint">眼、睫毛、眉毛始终排除；需先有真实检测结果。</p>
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("people") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("people")}>
            <ChevronDown size={12} className="chev" />
            <span>人物</span>
          </button>
          <span className="beauty-badge">{faces.length} 张脸</span>
        </header>
        {isOpen("people") && (
          <div className="param-group-body">
            {faces.length === 0 ? (
              <p className="param-hint">
                {faceStatus === "loading" ? "正在检测人脸…" : "未检测到人脸，或模型不可用。"}
              </p>
            ) : (
              <div className="beauty-face-list">
                {faces.map((face, index) => {
                  const entry = findFaceBeauty(params, face.id);
                  const active = index === activeFaceIndex;
                  return (
                    <div
                      key={face.id}
                      className={`beauty-face-item ${active ? "is-active" : ""}`}
                    >
                      <button
                        type="button"
                        className="beauty-face-select"
                        onClick={() => setActiveFace(index)}
                        title={`选择人物 ${faceLetter(index)}（ID ${face.id}）`}
                      >
                        <span className="beauty-face-letter">{faceLetter(index)}</span>
                        <span className="beauty-face-meta">
                          ID {face.id} · {(face.score * 100).toFixed(0)}%
                        </span>
                      </button>
                      <button
                        type="button"
                        className={`icon-toggle ${entry?.enabled !== false ? "is-on" : ""}`}
                        title={entry?.enabled !== false ? "停用该人脸美肤" : "启用该人脸美肤"}
                        aria-label={entry?.enabled !== false ? "停用该人脸美肤" : "启用该人脸美肤"}
                        onClick={() => setFaceEnabled(face.id, entry?.enabled === false)}
                      >
                        {entry?.enabled !== false ? <Eye size={13} /> : <EyeOff size={13} />}
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
            <div className="beauty-brush-row">
              <button
                type="button"
                className="mini-btn"
                disabled={!activeFace || faces.length < 2}
                onClick={() => activeFace && copyFaceParamsToOthers(activeFace.id)}
              >
                <Copy size={11} />
                复制到其他人物
              </button>
              <button
                type="button"
                className={`mini-btn ${maskVisible ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => setMaskVisible(!maskVisible)}
              >
                {maskVisible ? <Eye size={11} /> : <EyeOff size={11} />}
                显示蒙版
              </button>
            </div>
            {activeFace ? (
              <p className="param-hint">
                当前：人物 {faceLetter(activeFaceIndex)} · 稳定 ID {activeFace.id}；每张脸参数独立存储，重叠区域按最近人脸归属。
              </p>
            ) : null}
          </div>
        )}
      </section>

      <section className={`param-group ${isOpen("mask") ? "is-open" : ""}`}>
        <header className="param-group-head">
          <button type="button" className="param-group-toggle" onClick={() => toggle("mask")}>
            <ChevronDown size={12} className="chev" />
            <span>手动蒙版</span>
          </button>
          {params.manualMask.strokes.length > 0 ? (
            <button
              type="button"
              className="param-group-reset"
              onClick={clearMaskStrokes}
              title="恢复自动蒙版"
            >
              <RotateCcw size={11} />
            </button>
          ) : null}
        </header>
        {isOpen("mask") && (
          <div className="param-group-body">
            <div className="beauty-toggle-row">
              <button
                type="button"
                className={`mini-btn ${params.manualMask.enabled ? "is-active" : ""}`}
                disabled={!desktop}
                onClick={() =>
                  updateManualMask(
                    (mask) => ({ ...mask, enabled: !mask.enabled }),
                    "切换手动蒙版",
                  )
                }
              >
                手动蒙版 {params.manualMask.enabled ? "开" : "关"}
              </button>
              <button
                type="button"
                className={`mini-btn ${params.manualMask.invert ? "is-active" : ""}`}
                disabled={!desktop}
                onClick={() =>
                  updateManualMask((mask) => ({ ...mask, invert: !mask.invert }), "反转蒙版")
                }
              >
                反转
              </button>
              <button
                type="button"
                className={`mini-btn ${brushMode !== "off" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  setBrushMode(brushMode === "off" ? "paint" : "off");
                  setMaskVisible(true);
                }}
              >
                <Brush size={11} />
                {brushMode === "off" ? "画笔" : "结束绘制"}
              </button>
            </div>
            <Slider
              label="羽化"
              value={params.manualMask.feather}
              min={0}
              max={100}
              step={1}
              bipolar={false}
              defaultValue={40}
              onChange={(value) =>
                updateManualMask((mask) => ({ ...mask, feather: value }), "羽化", "mask:feather")
              }
            />
            <Slider
              label="蒙版强度"
              value={params.manualMask.strength}
              min={0}
              max={100}
              step={1}
              bipolar={false}
              defaultValue={100}
              onChange={(value) =>
                updateManualMask((mask) => ({ ...mask, strength: value }), "蒙版强度", "mask:strength")
              }
            />
            <div className="beauty-brush-row">
              <button
                type="button"
                className={`mini-btn ${brushMode === "paint" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  setBrushMode("paint");
                  setMaskVisible(true);
                }}
              >
                <Brush size={11} />
                画笔
              </button>
              <button
                type="button"
                className={`mini-btn ${brushMode === "erase" ? "is-active" : ""}`}
                disabled={!desktop || !hasFace}
                onClick={() => {
                  setBrushMode("erase");
                  setMaskVisible(true);
                }}
              >
                <Eraser size={11} />
                橡皮擦
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={!params.manualMask.strokes.length}
                onClick={undoMaskStroke}
              >
                <Undo2 size={11} />
                撤销笔触
              </button>
              <button
                type="button"
                className="mini-btn"
                disabled={!params.manualMask.strokes.length}
                onClick={clearMaskStrokes}
              >
                恢复自动
              </button>
            </div>
            <Slider
              label="画笔半径"
              value={Math.round(brushRadius * 1000)}
              min={4}
              max={200}
              step={1}
              bipolar={false}
              defaultValue={30}
              onChange={(value) => setBrushRadius(value / 1000)}
            />
            <p className="param-hint">
              笔触按图像归一化坐标保存，缩放/平移后仍然准确；每笔都可单独撤销。
            </p>
            {maskStatus === "loading" ? (
              <p className="param-hint">
                <Loader2 size={11} className="spin" />
                正在生成蒙版预览…
              </p>
            ) : null}
            {maskStatus === "ready" && maskPreview ? (
              <p className="param-hint">
                皮肤占比 {(maskPreview.skinRatio * 100).toFixed(1)}% · {maskPreview.faces} 张脸 ·{" "}
                {maskPreview.method}
              </p>
            ) : null}
            {maskStatus === "error" && maskError ? (
              <p className="face-error">
                <AlertTriangle size={12} />
                {maskError}
              </p>
            ) : null}
          </div>
        )}
      </section>
    </div>
  );
}
