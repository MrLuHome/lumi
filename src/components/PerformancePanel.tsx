import { useCallback, useEffect, useState } from "react";
import { Cpu, Gauge, RefreshCw, Zap } from "lucide-react";
import type { ComputeBackend, GpuBenchmarkResult, GpuStatus } from "../types";
import { gpuBenchmark, gpuStatus, setComputeBackend } from "../lib/backend";
import { useEditorStore } from "../state/editorStore";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "计算后端操作失败";
}

const PREFERENCES: { value: ComputeBackend; label: string }[] = [
  { value: "auto", label: "自动（基准 ≥1.25× 才启用）" },
  { value: "gpu", label: "强制 GPU" },
  { value: "cpu", label: "仅 CPU" },
];

export function PerformancePanel() {
  const preference = useEditorStore((state) => state.computePreference);
  const setPreference = useEditorStore((state) => state.setComputePreference);
  const setGpuStatus = useEditorStore((state) => state.setGpuStatus);
  const [status, setStatus] = useState<GpuStatus | null>(null);
  const [bench, setBench] = useState<GpuBenchmarkResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const value = await gpuStatus();
      setStatus(value);
      setGpuStatus(value);
      setPreference(value.preference);
      setError(null);
    } catch (cause) {
      setError(toMessage(cause));
    }
  }, [setPreference, setGpuStatus]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const changePreference = async (next: ComputeBackend) => {
    setPreference(next);
    try {
      const value = await setComputeBackend(next);
      setStatus(value);
      setGpuStatus(value);
    } catch (cause) {
      setError(toMessage(cause));
    }
  };

  const runBenchmark = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await gpuBenchmark();
      setBench(result);
      setStatus(result.status);
      setGpuStatus(result.status);
      setPreference(result.status.preference);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="panel performance-panel">
      <div className="panel-head">
        <h2>性能与计算后端</h2>
        <p className="panel-sub">GPU 仅加速已验证的曝光/曲线内核，任何失败都会回退 CPU</p>
        <div className="preset-head-actions">
          <button type="button" className="btn" onClick={() => void refresh()}>
            <RefreshCw size={13} /> 刷新
          </button>
          <button type="button" className="btn" onClick={() => void runBenchmark()} disabled={busy}>
            <Gauge size={13} /> {busy ? "基准运行中…" : "运行基准"}
          </button>
        </div>
      </div>

      {error ? <div className="smart-error">{error}</div> : null}

      <section className="perf-section">
        <div className="section-title">
          当前后端
          <span className="section-meta">
            {status?.enabled ? "GPU 已启用" : status?.available ? "GPU 可用（未启用）" : "CPU"}
          </span>
        </div>
        <label className="raw-field">
          <span>策略</span>
          <select
            value={preference}
            onChange={(event) => void changePreference(event.target.value as ComputeBackend)}
          >
            {PREFERENCES.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>

        {status ? (
          <dl className="perf-list">
            <div>
              <dt>设备</dt>
              <dd>{status.device || "—"}</dd>
            </div>
            <div>
              <dt>后端</dt>
              <dd>
                {status.backend}
                {status.capabilities ? ` · ${status.capabilities.deviceType}` : ""}
              </dd>
            </div>
            <div>
              <dt>实测倍率</dt>
              <dd>{status.verifiedSpeedup ? `${status.verifiedSpeedup.toFixed(2)}×` : "未验证"}</dd>
            </div>
            <div>
              <dt>说明</dt>
              <dd>{status.reason || "—"}</dd>
            </div>
          </dl>
        ) : (
          <div className="panel-empty compact">正在读取计算后端状态…</div>
        )}
        <p className="hint-text">
          <Cpu size={12} /> Auto 在未完成基准前保持 CPU；显式选择 GPU 也会在初始化/绑定/读回失败时回退，
          回退原因显示在上方。
        </p>
      </section>

      {bench ? (
        <section className="perf-section">
          <div className="section-title">
            基准结果
            <span className="section-meta">
              {bench.autoEnabled ? <Zap size={12} /> : null}
              {bench.autoEnabled ? "Auto 已启用 GPU" : "Auto 保持 CPU"}
            </span>
          </div>
          <table className="perf-table">
            <thead>
              <tr>
                <th>尺寸</th>
                <th>MP</th>
                <th>CPU</th>
                <th>上传</th>
                <th>GPU 运算</th>
                <th>GPU 总</th>
                <th>倍率</th>
              </tr>
            </thead>
            <tbody>
              {bench.rows.map((row) => (
                <tr key={`${row.width}x${row.height}`}>
                  <td>
                    {row.width}×{row.height}
                  </td>
                  <td>{row.megapixels.toFixed(1)}</td>
                  <td>{row.cpuMs.toFixed(1)} ms</td>
                  <td>{row.gpuUploadMs.toFixed(1)} ms</td>
                  <td>{row.gpuComputeMs.toFixed(1)} ms</td>
                  <td>{row.gpuTotalMs.toFixed(1)} ms</td>
                  <td>{row.speedup.toFixed(2)}×</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="hint-text">
            倍率按端到端耗时（含上传/读回）计算，只有全部尺寸都不低于 1.25× 才会在 Auto 下启用。
          </p>
        </section>
      ) : null}
    </aside>
  );
}
