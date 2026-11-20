import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2, RefreshCw, Sparkles } from "lucide-react";
import type { RetouchMode, RetouchSuggestion } from "../types";
import { analyzeRetouch, applyRetouchSuggestions } from "../lib/backend";
import {
  selectActiveParams,
  selectActivePhoto,
  useEditorStore,
} from "../state/editorStore";

const MODES: { value: RetouchMode; label: string; hint: string }[] = [
  { value: "natural", label: "自然", hint: "只做轻微修正，优先保留原始质感" },
  { value: "light", label: "轻度", hint: "适度修正曝光与色彩" },
  { value: "standard", label: "标准", hint: "完整的本地分析建议" },
  { value: "strong", label: "强化", hint: "更积极的修正，仍受上下限约束" },
];

const STATUS_TEXT: Record<string, string> = {
  ok: "正常",
  lowConfidence: "低置信度",
  noData: "无数据",
  unsupported: "不支持",
  failed: "检测失败",
};

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "分析失败，请重试";
}

function formatMetricValue(value: number | null, unit: string): string {
  if (value === null) return "—";
  if (unit === "%") return `${(value * 100).toFixed(1)}%`;
  if (unit === "°") return `${value.toFixed(0)}°`;
  return `${value.toFixed(2)}${unit}`;
}

export function SmartPanel() {
  const photo = useEditorStore(selectActivePhoto);
  const params = useEditorStore(selectActiveParams);
  const reports = useEditorStore((state) => state.smartReports);
  const status = useEditorStore((state) => state.smartStatus);
  const error = useEditorStore((state) => state.smartError);
  const mode = useEditorStore((state) => state.smartMode);
  const elapsed = useEditorStore((state) => state.smartElapsedMs);
  const setMode = useEditorStore((state) => state.setSmartMode);
  const setStatus = useEditorStore((state) => state.setSmartStatus);
  const setSmartError = useEditorStore((state) => state.setSmartError);
  const setReport = useEditorStore((state) => state.setSmartReport);
  const applyExternalParams = useEditorStore((state) => state.applyExternalParams);
  const notify = useEditorStore((state) => state.notify);
  const setWorkspace = useEditorStore((state) => state.setWorkspace);
  const setCompareMode = useEditorStore((state) => state.setCompareMode);

  const [selected, setSelected] = useState<string[]>([]);
  const [strength, setStrength] = useState(100);
  const [applying, setApplying] = useState(false);

  const report = photo ? reports[photo.id] : undefined;
  const modeMismatch = Boolean(report && report.mode !== mode);

  const runAnalysis = useCallback(async () => {
    if (!photo) return;
    setStatus("loading");
    setSmartError(null);
    try {
      const response = await analyzeRetouch(photo.path, params, mode);
      setReport(photo.id, response.report, response.elapsedMs);
      setSelected(
        response.report.suggestions.filter((item) => item.applicable).map((item) => item.id),
      );
      if (response.stale) {
        notify("info", "原图、参数或分析版本已变化，本次为重新分析");
      }
    } catch (failure) {
      setSmartError(toMessage(failure));
    }
  }, [photo, params, mode, setReport, setSmartError, setStatus, notify]);

  // 切换照片后如果没有报告，自动做一次本地分析；有报告则直接展示。
  useEffect(() => {
    if (!photo) return;
    const existing = reports[photo.id];
    if (existing) {
      setSelected(
        existing.suggestions.filter((item) => item.applicable).map((item) => item.id),
      );
      return;
    }
    void runAnalysis();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photo?.id]);

  const chosen = useMemo<RetouchSuggestion[]>(() => {
    if (!report) return [];
    return report.suggestions.filter((item) => selected.includes(item.id));
  }, [report, selected]);

  const handleApply = useCallback(
    async (items: RetouchSuggestion[]) => {
      if (!photo || items.length === 0) return;
      setApplying(true);
      try {
        // 强度只按参数语义插值，不会把整份参数乘以比例；
        // 后端仍然返回普通可编辑参数，并写入撤销历史。
        const scaled = items.map((item) => ({
          ...item,
          suggested: item.current + (item.suggested - item.current) * (strength / 100),
        }));
        const result = await applyRetouchSuggestions(photo.path, params, scaled);
        applyExternalParams(photo.id, result.params, "应用智能精修建议");
        if (result.warnings.length > 0) notify("info", result.warnings.join("；"));
        notify("success", `已应用 ${items.length} 条建议，可继续编辑或撤销`);
      } catch (failure) {
        notify("error", toMessage(failure));
      } finally {
        setApplying(false);
      }
    },
    [photo, params, strength, applyExternalParams, notify],
  );

  if (!photo) {
    return (
      <aside className="panel smart-panel">
        <div className="panel-empty">先导入并选择一张照片，才能进行本地分析</div>
      </aside>
    );
  }

  return (
    <aside className="panel smart-panel">
      <div className="panel-head">
        <div>
          <h2>智能精修建议</h2>
          <p className="panel-sub">本地照片分析 · 不上传照片 · 建议可编辑可撤销</p>
        </div>
        <button
          type="button"
          className="btn btn-accent"
          onClick={() => void runAnalysis()}
          disabled={status === "loading"}
        >
          <RefreshCw size={14} />
          {status === "loading" ? "分析中…" : "重新分析"}
        </button>
      </div>

      <section className="smart-section">
        <div className="section-title">修图模式</div>
        <div className="segmented">
          {MODES.map((item) => (
            <button
              key={item.value}
              type="button"
              className={mode === item.value ? "active" : ""}
              title={item.hint}
              onClick={() => setMode(item.value)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <p className="hint-text">
          {MODES.find((item) => item.value === mode)?.hint}
          {modeMismatch ? " · 模式已更改，请重新分析" : ""}
        </p>
      </section>

      {status === "error" ? (
        <div className="smart-error">
          <AlertTriangle size={14} />
          <span>{error ?? "分析失败"}</span>
        </div>
      ) : null}

      {!report ? (
        <div className="panel-empty">
          {status === "loading" ? "正在读取像素并统计…" : "点击「重新分析」生成真实建议"}
        </div>
      ) : (
        <>
          <section className="smart-section">
            <div className="section-title">
              检测结果
              <span className="section-meta">
                {report.faces} 张人脸 · 算法 {report.algorithmVersion} · 结果 v
                {report.resultVersion}
                {elapsed !== null ? ` · ${elapsed} ms` : ""}
              </span>
            </div>
            <div className="metric-list">
              {report.metrics.map((metric) => (
                <div key={metric.key} className={`metric-row status-${metric.status}`}>
                  <span className="metric-name">{metric.label}</span>
                  <span className="metric-value">
                    {formatMetricValue(metric.value, metric.unit)}
                  </span>
                  <span className="metric-status">{STATUS_TEXT[metric.status] ?? metric.status}</span>
                  <span className="metric-detail">{metric.detail}</span>
                </div>
              ))}
            </div>
            {report.warnings.length > 0 ? (
              <ul className="warning-list">
                {report.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            ) : null}
          </section>

          <section className="smart-section">
            <div className="section-title">
              建议参数
              <span className="section-meta">
                已选 {chosen.length}/{report.suggestions.length}
              </span>
            </div>
            {report.suggestions.length === 0 ? (
              <div className="panel-empty compact">
                当前参数已在合理范围，没有需要调整的指标
              </div>
            ) : (
              <div className="suggestion-list">
                {report.suggestions.map((item) => (
                  <label
                    key={item.id}
                    className={`suggestion-row ${selected.includes(item.id) ? "checked" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={selected.includes(item.id)}
                      onChange={() =>
                        setSelected((current) =>
                          current.includes(item.id)
                            ? current.filter((value) => value !== item.id)
                            : [...current, item.id],
                        )
                      }
                    />
                    <span className="suggestion-main">
                      <span className="suggestion-title">
                        {item.label}
                        <em>{item.group}</em>
                      </span>
                      <span className="suggestion-values">
                        {item.current.toFixed(0)} → {item.suggested.toFixed(0)}
                        <span className="confidence">
                          置信度 {Math.round(item.confidence * 100)}%
                        </span>
                      </span>
                      <span className="suggestion-reason">{item.reason}</span>
                    </span>
                  </label>
                ))}
              </div>
            )}
          </section>

          <section className="smart-section">
            <div className="section-title">
              建议强度
              <span className="section-meta">{strength}%</span>
            </div>
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={strength}
              onChange={(event) => setStrength(Number(event.target.value))}
            />
          </section>

          <div className="smart-actions">
            <button
              type="button"
              className="btn btn-accent"
              disabled={applying || chosen.length === 0}
              onClick={() => void handleApply(chosen)}
            >
              <Sparkles size={14} />
              应用所选（{chosen.length}）
            </button>
            <button
              type="button"
              className="btn"
              disabled={applying || report.suggestions.length === 0}
              onClick={() =>
                void handleApply(report.suggestions.filter((item) => item.applicable))
              }
            >
              应用全部
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => setSelected([])}
              disabled={chosen.length === 0}
            >
              全部拒绝
            </button>
          </div>

          <div className="smart-foot">
            <button
              type="button"
              className="link-btn"
              onClick={() => {
                setWorkspace("edit");
                setCompareMode("split");
              }}
            >
              <CheckCircle2 size={13} />
              到编辑工作区前后对比 / 手动微调
            </button>
            <p className="hint-text">
              建议只合并勾选字段，不会覆盖未选参数；应用后即为普通参数，⌘Z 可撤销。
            </p>
          </div>
        </>
      )}
    </aside>
  );
}
