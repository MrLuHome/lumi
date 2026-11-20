import type { ParamDefinition } from "../../state/params";
import { DEFAULT_PARAMS, formatParamValue } from "../../state/params";

interface SliderProps {
  definition: ParamDefinition;
  value: number;
  onChange: (value: number) => void;
}

export function Slider({ definition, value, onChange }: SliderProps) {
  const { min, max, step, bipolar } = definition;
  const ratio = (value - min) / (max - min);
  const zeroRatio = (0 - min) / (max - min);
  const fillLeft = bipolar ? Math.min(ratio, zeroRatio) : 0;
  const fillWidth = bipolar ? Math.abs(ratio - zeroRatio) : ratio;
  const active = Math.abs(value) >= 0.5;

  return (
    <div
      className={`slider ${active ? "is-active" : ""}`}
      onDoubleClick={() => onChange(DEFAULT_PARAMS[definition.key])}
      title="双击归零"
    >
      <div className="slider-head">
        <span className="slider-label">{definition.label}</span>
        <span className="slider-value">{formatParamValue(definition, value)}</span>
      </div>
      <div className="slider-track">
        <div className="slider-rail" />
        <div
          className="slider-fill"
          style={{ left: `${fillLeft * 100}%`, width: `${Math.max(fillWidth * 100, 0)}%` }}
        />
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          aria-label={definition.label}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </div>
    </div>
  );
}
