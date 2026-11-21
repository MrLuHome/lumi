import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, Camera, RefreshCw } from "lucide-react";
import type { RawCapabilities, RawInfo } from "../types";
import { DEFAULT_RAW_PARAMS } from "../types";
import { rawCapabilities, rawInfo } from "../lib/backend";
import { isRawPath } from "../state/project";
import { selectActivePhoto, useEditorStore } from "../state/editorStore";
import { Slider } from "./ui/Slider";
import { isDesktopRuntime } from "../lib/platform";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "RAW 读取失败";
}

function formatNumber(value: number, digits = 2): string {
  return Number.isFinite(value) ? value.toFixed(digits) : "—";
}

export function RawPanel() {
  const photo = useEditorStore(selectActivePhoto);
  const rawParams = useEditorStore(
    (state) => state.rawByPhoto[state.activeId ?? ""] ?? DEFAULT_RAW_PARAMS,
  );
  const rawError = useEditorStore((state) => state.rawError);
  const setRawParams = useEditorStore((state) => state.setRawParams);
  const setRawError = useEditorStore((state) => state.setRawError);
  const resetRawParams = useEditorStore((state) => state.resetRawParams);
  const activeId = useEditorStore((state) => state.activeId);

  const [info, setInfo] = useState<RawInfo | null>(null);
  const [capabilities, setCapabilities] = useState<RawCapabilities | null>(null);
  const [loading, setLoading] = useState(false);
  const isRaw = Boolean(photo && isRawPath(photo.path));
  const desktop = isDesktopRuntime();

  useEffect(() => {
    let cancelled = false;
    void rawCapabilities()
      .then((value) => {
        if (!cancelled) setCapabilities(value);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const loadInfo = useCallback(async () => {
    if (!photo || !isRaw) {
      setInfo(null);
      return;
    }
    setLoading(true);
    setRawError(null);
    try {
      const value = await rawInfo(photo.path);
      setInfo(value);
    } catch (error) {
      setInfo(null);
      setRawError(toMessage(error));
    } finally {
      setLoading(false);
    }
  }, [photo, isRaw, setRawError]);

  useEffect(() => {
    void loadInfo();
  }, [loadInfo]);

  if (!photo) {
    return (
      <aside className="panel raw-panel">
        <div className="panel-head">
          <h2>RAW 显影</h2>
          <p className="panel-sub">导入 RAW 后可查看传感器数据并调整显影参数</p>
        </div>
        <div className="panel-empty">尚未打开照片</div>
      </aside>
    );
  }

  if (!isRaw) {
    return (
      <aside className="panel raw-panel">
        <div className="panel-head">
          <h2>RAW 显影</h2>
          <p className="panel-sub">当前照片是普通图像，走常规解码与编辑管线</p>
        </div>
        <div className="panel-empty compact">
          {photo.fileName} 不是 RAW 文件。支持 CR2/CR3/NEF/ARW/RW2/ORF/RAF/DNG 等。
        </div>
      </aside>
    );
  }

  return (
    <aside className="panel raw-panel">
      <div className="panel-head">
        <h2>RAW 显影</h2>
        <p className="panel-sub">LibRaw 解包 + 线性去马赛克 + 相机矩阵，原图只读</p>
        <div className="preset-head-actions">
          <button type="button" className="btn" onClick={() => void loadInfo()} disabled={loading}>
            <RefreshCw size={13} />
            {loading ? "读取中…" : "刷新元数据"}
          </button>
        </div>
      </div>

      {rawError ? <div className="smart-error">{rawError}</div> : null}
      {!desktop ? <div className="panel-empty compact">浏览器预览模式不解析 RAW</div> : null}

      {info ? (
        <>
          <div className="raw-meta">
            <div className="raw-meta-row">
              <Camera size={13} />
              <strong>
                {info.make || "未知厂商"} {info.model}
              </strong>
            </div>
            <dl>
              <div>
                <dt>传感器</dt>
                <dd>
                  {info.width} × {info.height}（{info.rawWidth} × {info.rawHeight}）
                </dd>
              </div>
              <div>
                <dt>CFA</dt>
                <dd>
                  {info.cfa.pattern}
                  {info.cfa.xtrans ? " · X-Trans 6×6" : " · Bayer 2×2"}
                </dd>
              </div>
              <div>
                <dt>黑电平 / 白点</dt>
                <dd>
                  {formatNumber(info.blackLevel.black, 0)} / {info.whiteLevel.toFixed(0)}
                </dd>
              </div>
              <div>
                <dt>相机白平衡</dt>
                <dd>
                  {info.whiteBalance.camMul.map((value) => value.toFixed(3)).join(" / ")}
                  {info.whiteBalance.source ? ` · ${info.whiteBalance.source}` : ""}
                </dd>
              </div>
              <div>
                <dt>内嵌预览</dt>
                <dd>
                  {info.preview ? `${info.preview.width} × ${info.preview.height}` : "无"}
                </dd>
              </div>
              <div>
                <dt>EXIF</dt>
                <dd>
                  ISO {info.exif.iso || "—"} · 1/{info.exif.shutter || "—"}s · f/
                  {info.exif.aperture || "—"}
                </dd>
              </div>
            </dl>
          </div>

          <div className="raw-matrix">
            <span className="section-meta">相机 → XYZ(D65) 矩阵（行归一化后参与渲染）</span>
            <code>
              {info.cameraMatrix
                .map((row) => row.map((value) => value.toFixed(4)).join(", "))
                .join("\n")}
            </code>
          </div>

          <section className="raw-controls">
            <div className="section-title">显影参数</div>
            <Slider
              label="曝光"
              value={rawParams.exposureEv}
              min={-3}
              max={3}
              step={0.05}
              onChange={(value) => activeId && setRawParams(activeId, { exposureEv: value })}
              suffix=" EV"
            />
            <Slider
              label="高光"
              value={rawParams.highlights}
              min={-100}
              max={100}
              onChange={(value) => activeId && setRawParams(activeId, { highlights: value })}
            />
            <Slider
              label="阴影"
              value={rawParams.shadows}
              min={-100}
              max={100}
              onChange={(value) => activeId && setRawParams(activeId, { shadows: value })}
            />

            <label className="raw-field">
              <span>白平衡</span>
              <select
                value={rawParams.wbMode}
                onChange={(event) =>
                  activeId &&
                  setRawParams(activeId, { wbMode: event.target.value as typeof rawParams.wbMode })
                }
              >
                <option value="camera">相机（cam_mul）</option>
                <option value="auto">自动（灰度世界）</option>
                <option value="custom">自定义色温</option>
              </select>
            </label>

            {rawParams.wbMode === "custom" ? (
              <>
                <Slider
                  label="色温"
                  value={rawParams.temperature}
                  min={2000}
                  max={12000}
                  step={50}
                  bipolar={false}
                  defaultValue={6500}
                  onChange={(value) => activeId && setRawParams(activeId, { temperature: value })}
                  suffix=" K"
                />
                <Slider
                  label="色调"
                  value={rawParams.tint}
                  min={-100}
                  max={100}
                  onChange={(value) => activeId && setRawParams(activeId, { tint: value })}
                />
              </>
            ) : null}

            <label className="raw-field">
              <span>去马赛克</span>
              <select
                value={rawParams.demosaic}
                onChange={(event) =>
                  activeId &&
                  setRawParams(activeId, {
                    demosaic: event.target.value as typeof rawParams.demosaic,
                  })
                }
              >
                <option value="mhc">Malvar-He-Cutler 5×5</option>
                <option value="bilinear">双线性（对比用）</option>
              </select>
            </label>

            <div className="preset-actions">
              <button
                type="button"
                className="btn"
                onClick={() => activeId && resetRawParams(activeId)}
              >
                重置显影参数
              </button>
            </div>
            <p className="hint-text">
              参数变化会通过预览管线真实重新显影；导出使用同一 develop 函数的全尺寸结果。
            </p>
          </section>
        </>
      ) : (
        <div className="panel-empty compact">{loading ? "正在读取 RAW 元数据…" : "暂无元数据"}</div>
      )}

      {capabilities ? (
        <section className="raw-capabilities">
          <div className="section-title">
            能力矩阵
            <span className="section-meta">{capabilities.decoder}</span>
          </div>
          {capabilities.unsupported.length > 0 ? (
            <ul className="warning-list">
              {capabilities.unsupported.map((item) => (
                <li key={item}>
                  <AlertTriangle size={12} /> {item}
                </li>
              ))}
            </ul>
          ) : null}
          <ul className="raw-notes">
            {capabilities.notes.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </section>
      ) : null}
    </aside>
  );
}
