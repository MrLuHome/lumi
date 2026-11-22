import { useEffect, useState } from "react";
import { CheckCircle2, Monitor, RefreshCw, ScanEye } from "lucide-react";
import type {
  ColorOutputSettings,
  DisplayPreviewResult,
  ProfileSummary,
  RenderingIntent,
  SoftProofResult,
} from "../types";
import { DEFAULT_COLOR_SETTINGS } from "../types";
import {
  colorCheckProfile,
  colorSetOutput,
  colorStatus,
  displayPreview,
  displayProfileRefresh,
  softProof,
} from "../lib/backend";
import { isRawPath } from "../state/project";
import { selectActiveParams, selectActivePhoto, useEditorStore } from "../state/editorStore";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "色彩管理操作失败";
}

const INTENTS: { value: RenderingIntent; label: string }[] = [
  { value: "perceptual", label: "感知" },
  { value: "relative", label: "相对比色" },
  { value: "saturation", label: "饱和度" },
  { value: "absolute", label: "绝对比色" },
];

export function ColorPanel() {
  const photo = useEditorStore(selectActivePhoto);
  const params = useEditorStore(selectActiveParams);
  const settings = useEditorStore((state) => state.colorSettings);
  const status = useEditorStore((state) => state.colorStatus);
  const setSettings = useEditorStore((state) => state.setColorSettings);
  const setStatus = useEditorStore((state) => state.setColorStatus);
  const notify = useEditorStore((state) => state.notify);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [customPath, setCustomPath] = useState(settings.customPath ?? "");
  const [customProfile, setCustomProfile] = useState<ProfileSummary | null>(null);
  const [proof, setProof] = useState<SoftProofResult | null>(null);
  const [proofBusy, setProofBusy] = useState(false);
  const [proofError, setProofError] = useState<string | null>(null);
  const [displayProof, setDisplayProof] = useState<DisplayPreviewResult | null>(null);
  const [displayBusy, setDisplayBusy] = useState(false);
  const raw = photo ? isRawPath(photo.path) : false;

  const apply = async (next: ColorOutputSettings) => {
    setSettings(next);
    setError(null);
    try {
      const value = await colorSetOutput(next);
      setStatus(value);
    } catch (cause) {
      setError(toMessage(cause));
    }
  };

  const refresh = async () => {
    setLoading(true);
    try {
      const value = await colorStatus();
      setStatus(value);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    // 只在挂载时读取一次，后续由用户操作驱动。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 项目里保存的输出设置是事实来源；后端 status 只用于能力/警告展示，
  // 否则重开项目时会把用户保存的配置显示成引擎默认值。
  const current = settings ?? status?.output ?? DEFAULT_COLOR_SETTINGS;

  const handleCustomCheck = async () => {
    if (!customPath.trim()) return;
    setError(null);
    try {
      const summary = await colorCheckProfile(customPath.trim());
      setCustomProfile(summary);
      notify("info", `已读取 ICC：${summary.description || summary.colorSpace}`);
    } catch (cause) {
      setCustomProfile(null);
      setError(toMessage(cause));
    }
  };

  const handleSoftProof = async () => {
    if (!photo) return;
    setProofBusy(true);
    setProofError(null);
    try {
      const result = await softProof(photo.path, params, true);
      setProof(result);
    } catch (cause) {
      setProof(null);
      setProofError(toMessage(cause));
    } finally {
      setProofBusy(false);
    }
  };

  const handleDisplayPreview = async () => {
    if (!photo) return;
    setDisplayBusy(true);
    setError(null);
    try {
      setDisplayProof(await displayPreview(photo.path, params));
    } catch (cause) {
      setDisplayProof(null);
      setError(toMessage(cause));
    } finally {
      setDisplayBusy(false);
    }
  };

  return (
    <aside className="panel color-panel">
      <div className="panel-head">
        <h2>色彩管理</h2>
        <p className="panel-sub">16-bit ICC 真实转换，输出嵌入 Profile；软打样与 CMYK 读取在能力内标注</p>
        <div className="preset-head-actions">
          <button type="button" className="btn" onClick={() => void refresh()} disabled={loading}>
            <RefreshCw size={13} />
            {loading ? "读取中…" : "刷新"}
          </button>
        </div>
      </div>

      {error ? <div className="smart-error">{error}</div> : null}

      <section className="color-section">
        <div className="section-title">
          输出 Profile
          <span className="section-meta">{status?.outputLabel ?? current.profile}</span>
        </div>
        <label className="raw-field">
          <span>目标空间</span>
          <select
            value={current.profile}
            onChange={(event) => {
              const profile = event.target.value as ColorOutputSettings["profile"];
              void apply({
                ...current,
                profile,
                customPath: profile === "custom" ? customPath.trim() || null : null,
              });
            }}
          >
            <option value="srgb">sRGB IEC61966-2.1</option>
            <option value="adobeRgb">Adobe RGB (1998)</option>
            <option value="displayP3">Display P3</option>
            <option value="proPhoto">ProPhoto RGB</option>
            <option value="custom">自定义 ICC…</option>
          </select>
        </label>

        {current.profile === "custom" ? (
          <div className="color-custom">
            <input
              type="text"
              value={customPath}
              placeholder="/Users/you/Library/ColorSync/Profiles/custom.icc"
              onChange={(event) => setCustomPath(event.target.value)}
            />
            <button type="button" className="btn" onClick={() => void handleCustomCheck()}>
              校验
            </button>
          </div>
        ) : null}

        {customProfile ? (
          <p className="hint-text">
            <CheckCircle2 size={12} /> {customProfile.description || "未命名 Profile"} ·{" "}
            {customProfile.colorSpace} · {customProfile.channels} 通道
          </p>
        ) : null}

        <label className="raw-field">
          <span>渲染意图</span>
          <select
            value={current.intent}
            onChange={(event) =>
              void apply({ ...current, intent: event.target.value as RenderingIntent })
            }
          >
            {INTENTS.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </label>

        <label className="color-toggle">
          <input
            type="checkbox"
            checked={current.blackPointCompensation}
            onChange={(event) =>
              void apply({ ...current, blackPointCompensation: event.target.checked })
            }
          />
          黑场补偿
        </label>
        <label className="color-toggle">
          <input
            type="checkbox"
            checked={current.embed}
            onChange={(event) => void apply({ ...current, embed: event.target.checked })}
          />
          导出时嵌入 ICC（JPEG APP2 / PNG iCCP / TIFF 34675）
        </label>
        <p className="hint-text">
          关闭嵌入只会省略 Profile 标记，像素仍按目标空间真实转换；单张与批量导出共用同一设置。
        </p>
      </section>

      <section className="color-section">
        <div className="section-title">
          软打样与色域警告
          <span className="section-meta">
            {proof ? `超色域 ${(proof.ratio * 100).toFixed(2)}%` : "sRGB → 输出 → sRGB"}
          </span>
        </div>
        <button
          type="button"
          className="btn color-proof-btn"
          disabled={!photo || raw || proofBusy}
          onClick={() => void handleSoftProof()}
        >
          <ScanEye size={13} />
          {proofBusy ? "转换中…" : "软打样当前照片"}
        </button>
        {!photo ? <p className="hint-text">打开照片后可运行软打样。</p> : null}
        {raw ? (
          <p className="hint-text">
            RAW 软打样需要先显影为栅格；当前版本仅对栅格输入启用（未验证项）。
          </p>
        ) : null}
        {proofError ? <div className="smart-error">{proofError}</div> : null}
        {proof ? (
          <>
            <img className="soft-proof-preview" src={proof.url} alt="软打样预览（洋红为超色域）" />
            <p className="hint-text">
              {proof.profile} · {proof.intent} · 超色域 {proof.outOfGamutPixels}/{proof.totalPixels}
              （{(proof.ratio * 100).toFixed(2)}%），最大 ΔE00 {proof.maxDeltaE00.toFixed(2)}
            </p>
          </>
        ) : null}
      </section>

      <section className="color-section">
        <div className="section-title">
          显示器色彩
          <span className="section-meta">{status?.display.source ?? "—"}</span>
        </div>
        {status ? (
          <>
            <p className="color-display">
              <Monitor size={13} /> {status.display.description || "未读取到显示器 Profile"}
            </p>
            {status.display.reason ? <p className="hint-text">{status.display.reason}</p> : null}
            <button
              type="button"
              className="btn"
              onClick={() =>
                void displayProfileRefresh()
                  .then(() => refresh())
                  .catch((cause) => setError(toMessage(cause)))
              }
            >
              重新读取 ColorSync
            </button>
            <button
              type="button"
              className="btn"
              disabled={!photo || raw || displayBusy}
              onClick={() => void handleDisplayPreview()}
            >
              <Monitor size={13} />
              {displayBusy ? "转换中…" : "显示器 Profile 预览"}
            </button>
            {displayProof ? (
              <>
                <p className="hint-text">
                  {displayProof.source} · {displayProof.description}
                  {displayProof.reason ? `（${displayProof.reason}）` : ""}
                </p>
                <img className="soft-proof-preview" src={displayProof.url} alt="显示器色彩预览" />
              </>
            ) : null}
          </>
        ) : (
          <div className="panel-empty compact">正在读取显示器色彩配置…</div>
        )}
        {status?.notes.length ? (
          <ul className="raw-notes">
            {status.notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        ) : null}
      </section>
    </aside>
  );
}
