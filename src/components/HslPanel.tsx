import { useState } from "react";
import type { HslKey } from "../types";
import { HSL_CHANNELS } from "../state/params";
import { useEditorStore } from "../state/editorStore";
import { Slider } from "./ui/Slider";

/** HSL 八色通道：色相 / 饱和度 / 明度独立调节。 */
export function HslPanel() {
  const params = useEditorStore((state) => state.params[state.activeId ?? ""]);
  const updateHsl = useEditorStore((state) => state.updateHsl);
  const updateParams = useEditorStore((state) => state.updateParams);
  const [channel, setChannel] = useState<HslKey>("red");

  if (!params) return null;
  const active = HSL_CHANNELS.find((item) => item.key === channel) ?? HSL_CHANNELS[0]!;
  const value = params.hsl[channel];
  const dirty = value.hue !== 0 || value.saturation !== 0 || value.luminance !== 0;

  return (
    <div className="hsl-panel">
      <div className="hsl-tabs" role="tablist" aria-label="HSL 色相通道">
        {HSL_CHANNELS.map((item) => (
          <button
            key={item.key}
            type="button"
            role="tab"
            aria-selected={channel === item.key}
            className={`hsl-tab ${channel === item.key ? "is-active" : ""}`}
            style={{ ["--tab-color" as string]: item.color }}
            title={item.label}
            onClick={() => setChannel(item.key)}
          >
            <span className="hsl-dot" style={{ background: item.color }} />
            {item.label}
          </button>
        ))}
      </div>

      <Slider
        label={`${active.label}色相`}
        value={value.hue}
        onChange={(next) => updateHsl(channel, "hue", next)}
      />
      <Slider
        label={`${active.label}饱和度`}
        value={value.saturation}
        onChange={(next) => updateHsl(channel, "saturation", next)}
      />
      <Slider
        label={`${active.label}明度`}
        value={value.luminance}
        onChange={(next) => updateHsl(channel, "luminance", next)}
      />

      <button
        type="button"
        className="mini-btn"
        disabled={!dirty}
        onClick={() =>
          updateParams(
            (current) => ({
              ...current,
              hsl: { ...current.hsl, [channel]: { hue: 0, saturation: 0, luminance: 0 } },
            }),
            "重置 HSL 通道",
          )
        }
      >
        重置 {active.label} 通道
      </button>
    </div>
  );
}
