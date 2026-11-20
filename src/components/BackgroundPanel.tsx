import { useState } from "react";
import { AlertTriangle, ImagePlus, Loader2, RotateCcw } from "lucide-react";
import type { BackgroundFit, BackgroundMode, PhotoAsset } from "../types";
import { pickBackgroundImage } from "../lib/backend";
import { isDesktopRuntime } from "../lib/platform";
import { selectActiveParams, useEditorStore } from "../state/editorStore";
import { Slider } from "./ui/Slider";

interface BackgroundPanelProps {
  photo: PhotoAsset | null;
}

const MODES: { mode: BackgroundMode; label: string }[] = [
  { mode: "original", label: "原始背景" },
  { mode: "blur", label: "背景虚化" },
  { mode: "solidColor", label: "纯色" },
  { mode: "gradient", label: "渐变" },
  { mode: "image", label: "图片" },
];

export function BackgroundPanel({ photo }: BackgroundPanelProps) {
  const params = useEditorStore(selectActiveParams);
  const bg = params.background;
  const updateBackground = useEditorStore((state) => state.updateBackground);
  const resetBackground = useEditorStore((state) => state.resetBackground);
  const setBackgroundImage = useEditorStore((state) => state.setBackgroundImage);
  const notify = useEditorStore((state) => state.notify);
  const [picking, setPicking] = useState(false);
  const desktop = isDesktopRuntime();

  if (!photo) return null;

  const maskReady = params.subjectMask.enabled && params.subjectMask.strokes.length > 0;

  const chooseImage = async () => {
    setPicking(true);
    try {
      const info = await pickBackgroundImage();
      if (!info) return;
      setBackgroundImage(info);
      notify("success", `背景图片：${info.name}`);
    } catch (error) {
      notify("error", `选择背景图片失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setPicking(false);
    }
  };

  return (
    <div className="reshape-panel">
      <div className="reshape-notice">
        背景处理只作用在人物蒙版之外；人物像素不会被背景颜色、模糊或替换影响。
      </div>

      {!maskReady ? (
        <div className="reshape-warning is-info">
          <AlertTriangle size={12} />
          <span>尚未绘制并启用手动人物蒙版，背景处理暂不会改变像素。</span>
        </div>
      ) : null}

      <div className="warp-actions">
        <button
          type="button"
          className={`mini-btn ${bg.enabled ? "is-active" : ""}`}
          onClick={() =>
            updateBackground((current) => ({ ...current, enabled: !current.enabled }), "背景开关")
          }
        >
          {bg.enabled ? "背景处理已启用" : "启用背景处理"}
        </button>
        <button type="button" className="mini-btn" onClick={resetBackground}>
          <RotateCcw size={11} />
          重置背景
        </button>
      </div>

      <div className="reshape-section-title">背景模式</div>
      <div className="warp-tools">
        {MODES.map((item) => (
          <button
            key={item.mode}
            type="button"
            className={`warp-tool ${bg.mode === item.mode ? "is-active" : ""}`}
            onClick={() =>
              updateBackground(
                (current) => ({
                  ...current,
                  enabled: item.mode !== "original" ? true : current.enabled,
                  mode: item.mode,
                }),
                `背景模式 · ${item.label}`,
              )
            }
          >
            {item.label}
          </button>
        ))}
      </div>

      {bg.mode === "blur" ? (
        <>
          <Slider
            label="虚化强度"
            value={bg.blur}
            min={0}
            max={100}
            bipolar={false}
            defaultValue={45}
            suffix="%"
            onChange={(value) =>
              updateBackground((current) => ({ ...current, blur: value }), "背景虚化", "bg:blur")
            }
          />
          <Slider
            label="景深模拟"
            value={bg.depth}
            min={0}
            max={100}
            bipolar={false}
            defaultValue={30}
            suffix="%"
            onChange={(value) =>
              updateBackground((current) => ({ ...current, depth: value }), "景深模拟", "bg:depth")
            }
          />
          <p className="param-hint">景深是按纵向位置模拟的近似效果，不是 3D 深度重建。</p>
        </>
      ) : null}

      {bg.mode === "solidColor" ? (
        <div className="export-row">
          <label className="export-label" htmlFor="bg-solid-color">
            背景颜色
          </label>
          <input
            id="bg-solid-color"
            type="color"
            className="color-input"
            value={bg.color}
            onChange={(event) =>
              updateBackground((current) => ({ ...current, color: event.target.value }), "背景颜色")
            }
          />
        </div>
      ) : null}

      {bg.mode === "gradient" ? (
        <>
          <Slider
            label="渐变角度"
            value={bg.gradient.angle}
            min={0}
            max={360}
            bipolar={false}
            defaultValue={0}
            suffix="°"
            onChange={(value) =>
              updateBackground(
                (current) => ({ ...current, gradient: { ...current.gradient, angle: value } }),
                "渐变角度",
                "bg:angle",
              )
            }
          />
          <div className="export-row">
            <label className="export-label" htmlFor="bg-grad-a">
              起点
            </label>
            <input
              id="bg-grad-a"
              type="color"
              className="color-input"
              value={bg.gradient.stops[0]?.color ?? "#1C2733"}
              onChange={(event) =>
                updateBackground(
                  (current) => ({
                    ...current,
                    gradient: {
                      ...current.gradient,
                      stops: current.gradient.stops.map((stop, index) =>
                        index === 0 ? { ...stop, color: event.target.value } : stop,
                      ),
                    },
                  }),
                  "渐变起点颜色",
                )
              }
            />
            <label className="export-label" htmlFor="bg-grad-b">
              终点
            </label>
            <input
              id="bg-grad-b"
              type="color"
              className="color-input"
              value={bg.gradient.stops[bg.gradient.stops.length - 1]?.color ?? "#4A6076"}
              onChange={(event) =>
                updateBackground(
                  (current) => ({
                    ...current,
                    gradient: {
                      ...current.gradient,
                      stops: current.gradient.stops.map((stop, index) =>
                        index === current.gradient.stops.length - 1
                          ? { ...stop, color: event.target.value }
                          : stop,
                      ),
                    },
                  }),
                  "渐变终点颜色",
                )
              }
            />
          </div>
        </>
      ) : null}

      {bg.mode === "image" ? (
        <>
          <button type="button" className="mini-btn" disabled={!desktop || picking} onClick={() => void chooseImage()}>
            {picking ? <Loader2 size={11} className="spin" /> : <ImagePlus size={11} />}
            {bg.imagePath ? "更换本地图片…" : "选择本地图片…"}
          </button>
          {bg.imagePath ? (
            <p className="param-hint" title={bg.imagePath}>
              {bg.imagePath.split(/[\\/]/).pop()}
            </p>
          ) : null}
          <div className="export-row">
            <label className="export-label" htmlFor="bg-fit">
              铺满方式
            </label>
            <select
              id="bg-fit"
              className="export-select"
              value={bg.imageFit}
              onChange={(event) =>
                updateBackground(
                  (current) => ({ ...current, imageFit: event.target.value as BackgroundFit }),
                  "背景铺满方式",
                )
              }
            >
              <option value="cover">覆盖</option>
              <option value="contain">完整显示</option>
              <option value="stretch">拉伸</option>
            </select>
          </div>
          <Slider
            label="图片缩放"
            value={bg.imageScale}
            min={50}
            max={200}
            bipolar={false}
            defaultValue={100}
            suffix="%"
            onChange={(value) =>
              updateBackground((current) => ({ ...current, imageScale: value }), "背景缩放", "bg:scale")
            }
          />
          <Slider
            label="水平偏移"
            value={bg.imageOffsetX}
            min={-100}
            max={100}
            defaultValue={0}
            suffix="%"
            onChange={(value) =>
              updateBackground(
                (current) => ({ ...current, imageOffsetX: value }),
                "背景水平偏移",
                "bg:offsetX",
              )
            }
          />
          <Slider
            label="垂直偏移"
            value={bg.imageOffsetY}
            min={-100}
            max={100}
            defaultValue={0}
            suffix="%"
            onChange={(value) =>
              updateBackground(
                (current) => ({ ...current, imageOffsetY: value }),
                "背景垂直偏移",
                "bg:offsetY",
              )
            }
          />
          <Slider
            label="图片模糊"
            value={bg.imageBlur}
            min={0}
            max={100}
            bipolar={false}
            defaultValue={0}
            suffix="%"
            onChange={(value) =>
              updateBackground(
                (current) => ({ ...current, imageBlur: value }),
                "背景图片模糊",
                "bg:imageBlur",
              )
            }
          />
        </>
      ) : null}

      <div className="reshape-section-title">背景色彩</div>
      <Slider
        label="背景亮度"
        value={bg.brightness}
        min={-100}
        max={100}
        defaultValue={0}
        onChange={(value) =>
          updateBackground((current) => ({ ...current, brightness: value }), "背景亮度", "bg:brightness")
        }
      />
      <Slider
        label="背景色温"
        value={bg.temperature}
        min={-100}
        max={100}
        defaultValue={0}
        onChange={(value) =>
          updateBackground(
            (current) => ({ ...current, temperature: value }),
            "背景色温",
            "bg:temperature",
          )
        }
      />
      <Slider
        label="背景饱和度"
        value={bg.saturation}
        min={-100}
        max={100}
        defaultValue={0}
        onChange={(value) =>
          updateBackground(
            (current) => ({ ...current, saturation: value }),
            "背景饱和度",
            "bg:saturation",
          )
        }
      />
      <Slider
        label="边缘去污染"
        value={bg.decontaminate}
        min={0}
        max={100}
        bipolar={false}
        defaultValue={60}
        suffix="%"
        onChange={(value) =>
          updateBackground(
            (current) => ({ ...current, decontaminate: value }),
            "背景去污染",
            "bg:decontaminate",
          )
        }
      />
    </div>
  );
}
