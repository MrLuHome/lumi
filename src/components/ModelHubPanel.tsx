import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, FilePlus2, HardDrive, Loader2, RefreshCw, ShieldAlert, Trash2 } from "lucide-react";
import type { InstalledModel, ProviderDescriptor } from "../types";
import {
  importFaceModel,
  listFaceModels,
  listFaceProviders,
  pickFaceModelFile,
  removeFaceModel,
  revalidateFaceModel,
  selectFaceProvider,
} from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { useEditorStore } from "../state/editorStore";

const NORMALIZATIONS = [
  ["zeroToOne", "0..1"],
  ["minusOneToOne", "-1..1"],
  ["imagenet", "ImageNet 均值/方差"],
  ["raw255", "0..255"],
] as const;

export function ModelHubPanel() {
  const notify = useEditorStore((state) => state.notify);
  const clearFaceAnalyses = useEditorStore((state) => state.clearFaceAnalyses);
  const desktop = isDesktopRuntime();
  const [models, setModels] = useState<InstalledModel[]>([]);
  const [providers, setProviders] = useState<ProviderDescriptor[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState("");
  const [name, setName] = useState("用户关键点模型");
  const [licenseName, setLicenseName] = useState("UNKNOWN");
  const [licenseUrl, setLicenseUrl] = useState("");
  const [normalization, setNormalization] = useState<string>("zeroToOne");
  const [outputCoordinate, setOutputCoordinate] = useState<string>("normalizedCrop");
  const [cropMargin, setCropMargin] = useState(0.15);

  const refresh = useCallback(async () => {
    if (!desktop) return;
    try {
      const [list, providerList] = await Promise.all([listFaceModels(), listFaceProviders()]);
      setModels(list);
      setProviders(providerList);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [desktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pick = async () => {
    try {
      const path = await pickFaceModelFile();
      if (path) setSource(path);
    } catch (cause) {
      notify("error", `选择文件失败：${cause instanceof Error ? cause.message : String(cause)}`);
    }
  };

  const importModel = async () => {
    if (!source) {
      notify("error", "请先选择 .onnx 文件");
      return;
    }
    setBusy("import");
    setError(null);
    try {
      const installed = await importFaceModel({
        source,
        name,
        version: "1.0",
        author: "",
        description: "",
        sourceUrl: licenseUrl || null,
        licenseName,
        licenseUrl: licenseUrl || null,
        licenseCommercialClaimed: false,
        licenseRedistributionClaimed: false,
        licenseNote: "用户自备模型；LUMI 未核实许可证，不随安装包分发",
        normalization: normalization as "zeroToOne" | "minusOneToOne" | "imagenet" | "raw255",
        outputCoordinate: outputCoordinate as "normalizedCrop" | "pixelCrop",
        cropMargin,
      });
      notify("success", `已导入 ${installed.manifest.name}（SHA256 ${installed.manifest.sha256.slice(0, 12)}…）`);
      setSource("");
      await refresh();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      notify("error", `导入失败：${message}`);
    } finally {
      setBusy(null);
    }
  };

  const revalidate = async (id: string) => {
    setBusy(`revalidate:${id}`);
    try {
      const installed = await revalidateFaceModel(id);
      notify(
        installed.validationOk ? "success" : "error",
        installed.validationOk ? "重新验证通过" : `重新验证失败：${installed.error ?? "未知原因"}`,
      );
      await refresh();
    } catch (cause) {
      notify("error", `重新验证失败：${cause instanceof Error ? cause.message : String(cause)}`);
    } finally {
      setBusy(null);
    }
  };

  const remove = async (id: string) => {
    setBusy(`remove:${id}`);
    try {
      await removeFaceModel(id);
      notify("success", "模型已移除");
      await refresh();
    } catch (cause) {
      notify("error", `移除失败：${cause instanceof Error ? cause.message : String(cause)}`);
    } finally {
      setBusy(null);
    }
  };

  const use = async (id: string) => {
    const providerId = `onnx:${id}`;
    if (!providers.some((provider) => provider.id === providerId && provider.available)) {
      notify("error", "该模型当前不可用，请先重新验证");
      return;
    }
    setBusy(`use:${id}`);
    try {
      const report = await selectFaceProvider(providerId);
      if (report.requiresReview) {
        for (const photoId of Object.keys(useEditorStore.getState().faces)) {
          clearFaceAnalyses(photoId);
        }
      }
      notify("success", report.message);
    } catch (cause) {
      notify("error", `切换失败：${cause instanceof Error ? cause.message : String(cause)}`);
    } finally {
      setBusy(null);
    }
  };

  if (!desktop) {
    return (
      <aside className="panel model-hub">
        <p className="param-hint">模型库需要桌面运行时。</p>
      </aside>
    );
  }

  return (
    <aside className="panel model-hub">
      <div className="panel-head">
        <div>
          <h2>模型库</h2>
          <p className="panel-sub">用户自备 ONNX · 本机校验 · 不随安装包分发</p>
        </div>
        <button type="button" className="mini-btn" onClick={() => void refresh()} disabled={busy !== null}>
          <RefreshCw size={12} />
          刷新
        </button>
      </div>

      <div className="reshape-warning is-info">
        <ShieldAlert size={12} />
        <span>
          不打包任何第三方权重。导入的模型仅保存在本机应用数据目录，许可证由用户自行确认；
          LUMI 不把它包装成商业可用。
        </span>
      </div>

      {error ? (
        <div className="reshape-warning">
          <AlertTriangle size={12} />
          <span>{error}</span>
        </div>
      ) : null}

      <div className="ai-section-title">导入模型</div>
      <label className="raw-field">
        <span>ONNX 文件</span>
        <div className="model-file-row">
          <input value={source} onChange={(event) => setSource(event.target.value)} placeholder="选择或粘贴 .onnx 路径" />
          <button type="button" className="mini-btn" onClick={() => void pick()}>
            <FilePlus2 size={12} />
            选择
          </button>
        </div>
      </label>
      <label className="raw-field">
        <span>名称</span>
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <div className="model-form-grid">
        <label className="raw-field">
          <span>归一化</span>
          <select value={normalization} onChange={(event) => setNormalization(event.target.value)}>
            {NORMALIZATIONS.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="raw-field">
          <span>输出坐标</span>
          <select
            value={outputCoordinate}
            onChange={(event) => setOutputCoordinate(event.target.value)}
          >
            <option value="normalizedCrop">裁剪内 0..1</option>
            <option value="pixelCrop">裁剪内像素</option>
          </select>
        </label>
        <label className="raw-field">
          <span>裁剪边距</span>
          <input
            type="number"
            min={0}
            max={0.5}
            step={0.01}
            value={cropMargin}
            onChange={(event) => setCropMargin(Number(event.target.value))}
          />
        </label>
        <label className="raw-field">
          <span>许可证（用户声明）</span>
          <input value={licenseName} onChange={(event) => setLicenseName(event.target.value)} />
        </label>
      </div>
      <label className="raw-field">
        <span>许可证链接（可选）</span>
        <input value={licenseUrl} onChange={(event) => setLicenseUrl(event.target.value)} />
      </label>
      <button
        type="button"
        className="primary-btn"
        onClick={() => void importModel()}
        disabled={busy !== null}
      >
        {busy === "import" ? <Loader2 size={13} className="spin" /> : <HardDrive size={13} />}
        校验并导入
      </button>
      <p className="param-hint">
        导入要求：单文件 .onnx、≤512 MB、静态输入 32..1024、输出 68 点、无 external_data；
        校验在独立子进程中运行，失败会给出具体原因。
      </p>

      <div className="ai-section-title">已安装（{models.length}）</div>
      <div className="model-list">
        {models.map((model) => {
          const providerId = `onnx:${model.manifest.id}`;
          const provider = providers.find((item) => item.id === providerId);
          const ok = model.present && model.sha256Matches && model.validationOk;
          return (
            <div className="model-card" key={model.manifest.id}>
              <div className="model-card-head">
                <span className="ai-provider-name">{model.manifest.name}</span>
                <span className={`ai-badge ${ok ? "is-ready" : "is-failed"}`}>
                  {ok ? "可用" : model.error ?? "不可用"}
                </span>
              </div>
              <dl className="perf-list">
                <div>
                  <dt>ID</dt>
                  <dd>{model.manifest.id}</dd>
                </div>
                <div>
                  <dt>SHA256</dt>
                  <dd title={model.manifest.sha256}>{model.manifest.sha256.slice(0, 16)}…</dd>
                </div>
                <div>
                  <dt>输入</dt>
                  <dd>
                    {model.manifest.input.width}×{model.manifest.input.height} · {model.manifest.input.layout}
                  </dd>
                </div>
                <div>
                  <dt>输出</dt>
                  <dd>
                    {model.manifest.output.layout === "n2"
                      ? "[1,N,2]"
                      : model.manifest.output.layout === "n3"
                        ? "[1,N,3]"
                        : model.manifest.output.layout === "c2n"
                          ? "[1,2,N]"
                          : "[1,3,N]"}{" "}
                    · {model.manifest.output.coordinate}
                  </dd>
                </div>
                <div>
                  <dt>拓扑</dt>
                  <dd>{model.manifest.topologyId}</dd>
                </div>
                <div>
                  <dt>许可证</dt>
                  <dd>
                    {model.manifest.license.name}
                    {model.manifest.license.commercialUseClaimed ? "（声明可商用）" : ""} · 未核实
                  </dd>
                </div>
              </dl>
              <div className="model-card-actions">
                <button
                  type="button"
                  className="mini-btn"
                  disabled={!ok || busy !== null}
                  onClick={() => void use(model.manifest.id)}
                >
                  使用
                </button>
                <button
                  type="button"
                  className="mini-btn"
                  disabled={busy !== null}
                  onClick={() => void revalidate(model.manifest.id)}
                >
                  {busy === `revalidate:${model.manifest.id}` ? (
                    <Loader2 size={11} className="spin" />
                  ) : null}
                  重新验证
                </button>
                <button
                  type="button"
                  className="mini-btn"
                  disabled={busy !== null}
                  onClick={() => void remove(model.manifest.id)}
                >
                  {busy === `remove:${model.manifest.id}` ? (
                    <Loader2 size={11} className="spin" />
                  ) : (
                    <Trash2 size={11} />
                  )}
                  移除
                </button>
                {provider ? (
                  <span className="ai-provider-id">{provider.available ? "Provider 已注册" : "Provider 不可用"}</span>
                ) : null}
              </div>
            </div>
          );
        })}
        {models.length === 0 ? <div className="panel-empty compact">尚未安装用户模型。</div> : null}
      </div>
    </aside>
  );
}
