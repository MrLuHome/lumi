import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, Cpu, Loader2, RefreshCw, ScanFace, ShieldCheck, X } from "lucide-react";
import type {
  FaceIntelligenceStatus,
  PhotoAsset,
  ProviderDescriptor,
  QualityReport,
} from "../types";
import {
  analyzePhotoFaces,
  faceIntelligenceStatus,
  probeFaceQuality,
  selectFaceProvider,
} from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { useEditorStore } from "../state/editorStore";

interface AiPortraitPanelProps {
  photo: PhotoAsset | null;
}

const LICENSE_LABELS: Record<string, string> = {
  verifiedCommercial: "可商用（已核实）",
  verifiedPersonal: "仅个人（已核实）",
  systemFramework: "系统框架",
  researchOnly: "仅研究用途",
  userUnverified: "用户自备 · 许可未核实",
  prohibited: "禁止使用",
  incompatible: "许可证不兼容",
  removed: "已移除",
};

const STATE_LABELS: Record<string, string> = {
  ready: "就绪",
  idle: "空闲",
  analyzing: "分析中",
  missing: "缺少模型",
  unsupported: "不支持",
  failed: "失败",
  lowQuality: "质量不足",
  licenseUnverified: "许可未核实",
};

const EVIDENCE_LABELS: Record<string, string> = {
  verified: "已验证",
  declared: "仅声明",
  unverified: "未验证",
  unsupported: "不支持",
};

function capabilityChips(provider: ProviderDescriptor) {
  const verified = provider.verifiedCapabilities;
  const declared = provider.capabilities;
  const chips: { label: string; ok: boolean; declared: boolean }[] = [
    { label: "稠密关键点", ok: verified.denseLandmarks, declared: declared.denseLandmarks },
    { label: "眼睑", ok: verified.eyelidLandmarks, declared: declared.eyelidLandmarks },
    { label: "鼻部", ok: verified.noseLandmarks, declared: declared.noseLandmarks },
    { label: "唇部", ok: verified.lipLandmarks, declared: declared.lipLandmarks },
    { label: "分割", ok: verified.faceParsing, declared: declared.faceParsing },
  ];
  return chips;
}

export function AiPortraitPanel({ photo }: AiPortraitPanelProps) {
  const notify = useEditorStore((state) => state.notify);
  const setFaces = useEditorStore((state) => state.setFaces);
  const clearFaceAnalyses = useEditorStore((state) => state.clearFaceAnalyses);
  const desktop = isDesktopRuntime();
  const [status, setStatus] = useState<FaceIntelligenceStatus | null>(null);
  const [busy, setBusy] = useState<"analyze" | "probe" | "switch" | "refresh" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [probe, setProbe] = useState<QualityReport | null>(null);
  const [switchNote, setSwitchNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!desktop) return;
    setBusy("refresh");
    setError(null);
    try {
      setStatus(await faceIntelligenceStatus());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [desktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const gate = useMemo(() => {
    const record = status?.lastAnalysis;
    if (!record) return null;
    return record;
  }, [status]);

  const activate = async (providerId: string) => {
    setBusy("switch");
    setError(null);
    try {
      const report = await selectFaceProvider(providerId);
      setSwitchNote(report.message);
      if (report.requiresReview && photo) {
        // 旧 Provider 的 dense/level3 不能继续解锁五官。
        clearFaceAnalyses(photo.id);
        // 旧 Provider 的稳定性探针结论同样作废。
        setProbe(null);
      }
      notify("success", report.message);
      await refresh();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      notify("error", `切换 Provider 失败：${message}`);
    } finally {
      setBusy(null);
    }
  };

  const analyze = async () => {
    if (!photo) {
      notify("error", "请先选择一张照片");
      return;
    }
    setBusy("analyze");
    setError(null);
    try {
      const report = await analyzePhotoFaces(photo.path);
      setFaces(photo.id, {
        faces: report.faces,
        imageWidth: photo.width,
        imageHeight: photo.height,
        elapsedMs: report.processingTimeMs,
        inferenceMs: report.processingTimeMs,
        model: report.providerId,
      });
      notify(
        report.denseFaceCount > 0 ? "success" : "info",
        `${report.providerName}：${report.faceCount} 张人脸，${report.totalLandmarks} 个关键点，${report.gateSummary}`,
      );
      if (report.warnings.length > 0) {
        notify("info", report.warnings.join("；"));
      }
      await refresh();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      notify("error", `分析失败：${message}`);
    } finally {
      setBusy(null);
    }
  };

  const probeQuality = async () => {
    if (!photo) {
      notify("error", "请先选择一张照片");
      return;
    }
    setBusy("probe");
    setError(null);
    try {
      const report = await probeFaceQuality(photo.path, 2);
      setProbe(report);
      notify(
        report.verified ? "success" : "info",
        report.stability.runs === 0
          ? `质量验证：${report.note}`
          : `质量验证：稳定性 ${(report.stability.ratio * 100).toFixed(3)}%，${report.verified ? "能力已升级为已验证" : "未达门槛，LEVEL 3 保持锁定"}`,
      );
      if (report.verified) {
        // 能力升级后立即重跑分析，让人脸缓存与门控同步，不需要用户再点一次。
        const analysis = await analyzePhotoFaces(photo.path);
        setFaces(photo.id, {
          faces: analysis.faces,
          imageWidth: photo.width,
          imageHeight: photo.height,
          elapsedMs: analysis.processingTimeMs,
          inferenceMs: analysis.processingTimeMs,
          model: analysis.providerId,
        });
      }
      await refresh();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      notify("error", `质量验证失败：${message}`);
    } finally {
      setBusy(null);
    }
  };

  if (!desktop) {
    return (
      <aside className="panel ai-panel">
        <p className="param-hint">人脸智能需要桌面运行时。</p>
      </aside>
    );
  }

  const providers = status?.providers ?? [];
  const activeId = status?.activeProvider ?? null;
  const active = providers.find((provider) => provider.id === activeId) ?? null;
  const stability = probe?.stability ?? gate?.stability ?? null;

  return (
    <aside className="panel ai-panel">
      <div className="panel-head">
        <div>
          <h2>AI 人像</h2>
          <p className="panel-sub">
            Provider 能力解耦 · 当前 {active?.displayName ?? "未选择"}
            {active ? ` · ${active.topologyId ?? "无拓扑"}` : ""}
          </p>
        </div>
        <button
          type="button"
          className="mini-btn"
          onClick={() => void refresh()}
          disabled={busy !== null}
        >
          {busy === "refresh" ? <Loader2 size={12} className="spin" /> : <RefreshCw size={12} />}
          刷新
        </button>
      </div>

      {error ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <span>{error}</span>
        </div>
      ) : null}
      {switchNote ? <p className="param-hint">{switchNote}</p> : null}

      <div className="ai-actions">
        <button
          type="button"
          className="primary-btn"
          onClick={() => void analyze()}
          disabled={busy !== null || !photo}
        >
          {busy === "analyze" ? <Loader2 size={13} className="spin" /> : <ScanFace size={13} />}
          分析当前照片
        </button>
        <button
          type="button"
          className="ghost-btn"
          onClick={() => void probeQuality()}
          disabled={busy !== null || !photo}
          title="同一张照片重复推理 2 次，测量关键点稳定性；通过后才解锁 LEVEL 3"
        >
          {busy === "probe" ? <Loader2 size={13} className="spin" /> : <ShieldCheck size={13} />}
          质量验证
        </button>
      </div>

      {status ? (
        <p className="param-hint">
          LEVEL 3：{status.level3Summary} · 稳定性门槛{" "}
          {(status.thresholds.maxStabilityRatio * 100).toFixed(1)}% 眼距
        </p>
      ) : null}

      <div className="ai-section-title">Provider</div>
      <div className="ai-providers">
        {providers.map((provider) => (
          <div
            key={provider.id}
            className={`ai-provider ${provider.id === activeId ? "is-active" : ""} ${
              provider.available ? "" : "is-unavailable"
            }`}
          >
            <div className="ai-provider-head">
              <span className="ai-provider-name">{provider.displayName}</span>
              <span className={`ai-badge is-${provider.state}`}>
                {STATE_LABELS[provider.state] ?? provider.state}
              </span>
            </div>
            <div className="ai-provider-meta">
              <span>{LICENSE_LABELS[provider.license.state] ?? provider.license.state}</span>
              <span>{provider.version}</span>
              <span>{provider.kind}</span>
            </div>
            <div className="ai-chips">
              {capabilityChips(provider).map((chip) => (
                <span
                  key={chip.label}
                  className={`ai-chip ${chip.ok ? "is-verified" : chip.declared ? "is-declared" : "is-off"}`}
                  title={
                    chip.ok ? "本机实测通过" : chip.declared ? "仅 Provider 声明，未通过本机验证" : "不支持"
                  }
                >
                  {chip.ok ? <Check size={10} /> : <X size={10} />}
                  {chip.label}
                </span>
              ))}
            </div>
            <p className="ai-provider-note">{provider.note}</p>
            <div className="ai-provider-foot">
              <button
                type="button"
                className="mini-btn"
                disabled={!provider.available || provider.id === activeId || busy !== null}
                onClick={() => void activate(provider.id)}
              >
                {provider.id === activeId ? "当前使用" : busy === "switch" ? "切换中…" : "切换"}
              </button>
              <span className="ai-provider-id">{provider.id}</span>
            </div>
            <details className="ai-evidence">
              <summary>能力证据（{provider.evidence.length}）</summary>
              <table className="ai-table">
                <thead>
                  <tr>
                    <th>能力</th>
                    <th>等级</th>
                    <th>测量 / 说明</th>
                  </tr>
                </thead>
                <tbody>
                  {provider.evidence.map((item) => (
                    <tr key={item.capability}>
                      <td>{item.capability}</td>
                      <td>{EVIDENCE_LABELS[item.level] ?? item.level}</td>
                      <td>{item.measurement || item.note}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </details>
          </div>
        ))}
        {providers.length === 0 ? <div className="panel-empty compact">正在读取 Provider…</div> : null}
      </div>

      {gate ? (
        <>
          <div className="ai-section-title">
            最近分析
            <span className="section-meta">
              {gate.providerName} · {gate.processingTimeMs} ms · {gate.totalLandmarks} 点
            </span>
          </div>
          <p className="param-hint">
            {gate.faces.length} 张人脸 · 图像 {gate.imageWidth}×{gate.imageHeight} · 拓扑{" "}
            {gate.topologyId || "—"}
          </p>
          {gate.warnings.length > 0
            ? gate.warnings.map((warning) => (
                <div className="reshape-warning" key={warning}>
                  <AlertTriangle size={12} />
                  <span>{warning}</span>
                </div>
              ))
            : null}
          <table className="ai-table">
            <thead>
              <tr>
                <th>人脸</th>
                <th>置信度</th>
                <th>稠密点</th>
                <th>眼 / 鼻 / 唇 / 下巴</th>
              </tr>
            </thead>
            <tbody>
              {gate.faces.map((face) => (
                <tr key={face.faceUuid}>
                  <td title={face.faceUuid}>{face.faceUuid.slice(0, 8)}</td>
                  <td>{Math.round(face.confidence * 100)}%</td>
                  <td>{face.densePoints}</td>
                  <td className="ai-gate-cell">
                    {[
                      ["眼", face.gate.eyes],
                      ["鼻", face.gate.nose],
                      ["唇", face.gate.mouth],
                      ["下巴", face.gate.chin],
                    ].map(([label, feature]) => {
                      const value = feature as { enabled: boolean; reason: string };
                      return (
                        <span
                          key={label as string}
                          className={`ai-gate ${value.enabled ? "is-on" : "is-off"}`}
                          title={value.reason || (value.enabled ? "已通过质量验证" : "未解锁")}
                        >
                          {value.enabled ? <Check size={10} /> : <X size={10} />}
                          {label as string}
                        </span>
                      );
                    })}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {gate.faces.some((face) => !face.gate.eyes.enabled) ? (
            <div className="ai-gate-reasons">
              {gate.faces
                .filter((face) => !face.gate.eyes.enabled)
                .slice(0, 2)
                .map((face) => (
                  <p key={face.faceUuid} className="param-hint">
                    #{face.faceUuid.slice(0, 8)}：{face.gate.summary}
                  </p>
                ))}
            </div>
          ) : null}
        </>
      ) : (
        <div className="panel-empty compact">
          还没有分析记录。点击「分析当前照片」；5 点 Provider 不会解锁 LEVEL 3。
        </div>
      )}

      {active?.id === "apple-vision" && gate ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <span>
            稳定性与结构检查已通过，定位精度尚未经人工标注验证。重复运行一致只说明结果稳定，不代表位置准确。
          </span>
        </div>
      ) : null}

      <div className="ai-section-title">精度证据</div>
      <dl className="perf-list">
        <div>
          <dt>稳定性</dt>
          <dd>
            {stability && stability.runs > 0
              ? `${stability.runs} 次重复分析，眼距占比 ${(stability.ratio * 100).toFixed(3)}%，${
                  stability.passed ? "通过" : "未通过"
                }`
              : "未运行重复分析"}
          </dd>
        </div>
        <div>
          <dt>结构验证</dt>
          <dd>
            {gate
              ? gate.faces
                  .map((face) => {
                    const enabled = [
                      face.gate.eyes.enabled,
                      face.gate.nose.enabled,
                      face.gate.mouth.enabled,
                      face.gate.chin.enabled,
                    ].filter(Boolean).length;
                    return `${face.faceUuid.slice(0, 6)} ${enabled}/4 区域`;
                  })
                  .join(" · ")
              : "未分析"}
          </dd>
        </div>
        <div>
          <dt>定位精度</dt>
          <dd>
            未验证：尚缺 50 张以上人工标注样本与 NME 评测；检测级 confidence 不等于逐点精度。
          </dd>
        </div>
      </dl>

      {stability && stability.runs > 0 ? (
        <>
          <div className="ai-section-title">
            稳定性实测
            <span className="section-meta">
              {stability.runs} 次 · {stability.elapsedMs.join(" / ")} ms
            </span>
          </div>
          <dl className="perf-list">
            <div>
              <dt>最大偏差</dt>
              <dd>{stability.maxDeviation.toFixed(5)} px</dd>
            </div>
            <div>
              <dt>眼距占比</dt>
              <dd>{(stability.ratio * 100).toFixed(3)}%</dd>
            </div>
            <div>
              <dt>结论</dt>
              <dd>{stability.passed ? "通过" : "未通过"}</dd>
            </div>
            <div>
              <dt>说明</dt>
              <dd>{stability.note}</dd>
            </div>
          </dl>
        </>
      ) : null}

      {probe ? (
        <div className="ai-probe">
          <div
            className={`ai-probe-result ${
              probe.stability.runs === 0 ? "is-fail" : probe.verified ? "is-pass" : "is-fail"
            }`}
          >
            {probe.verified ? <ShieldCheck size={13} /> : <AlertTriangle size={13} />}
            <span>
              {probe.stability.runs === 0
                ? "该 Provider 没有稠密关键点能力，探针不适用"
                : probe.verified
                  ? "稠密关键点与五官能力已升级为「已验证」"
                  : "未达到质量门槛，LEVEL 3 保持禁用"}
            </span>
          </div>
          <p className="param-hint">{probe.note}</p>
        </div>
      ) : null}

      <p className="hint-text">
        <Cpu size={12} /> 质量验证只衡量重复运行的一致性与结构完整性，不代表定位精度；未通过时不会产生任何
        LEVEL 3 变形，也不会用声明代替实测。
      </p>
    </aside>
  );
}
