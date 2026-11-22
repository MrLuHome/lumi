import { useEffect, useRef, useState } from "react";

interface SliderProps {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  bipolar?: boolean;
  /** 重置目标值，默认 0。 */
  defaultValue?: number;
  onChange: (value: number) => void;
  /** 百分比类参数显示 % 后缀。 */
  suffix?: string;
  /** 禁用（例如这一项还没有可用的分析结论）。 */
  disabled?: boolean;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function Slider({
  label,
  value,
  min = -100,
  max = 100,
  step = 1,
  bipolar = true,
  defaultValue = 0,
  onChange,
  suffix,
  disabled = false,
}: SliderProps) {
  const ratio = (value - min) / (max - min);
  const zeroRatio = (0 - min) / (max - min);
  const fillLeft = bipolar ? Math.min(ratio, zeroRatio) : 0;
  const fillWidth = bipolar ? Math.abs(ratio - zeroRatio) : ratio;
  const active = Math.abs(value - defaultValue) >= 0.5;

  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(String(Math.round(value)));
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!editing) setText(String(Math.round(value)));
  }, [value, editing]);

  const commit = () => {
    const numeric = Number.parseFloat(text);
    if (Number.isFinite(numeric)) onChange(clamp(numeric, min, max));
    setEditing(false);
    setText(String(Math.round(Number.isFinite(numeric) ? clamp(numeric, min, max) : value)));
  };

  return (
    <div
      className={`slider ${active ? "is-active" : ""} ${disabled ? "is-disabled" : ""}`}
      onDoubleClick={() => {
        if (!disabled) onChange(defaultValue);
      }}
      title={disabled ? "这一项暂时不可调" : "双击归零"}
    >
      <div className="slider-head">
        <span className="slider-label">{label}</span>
        <div className="slider-numbers">
          <input
            ref={inputRef}
            className="slider-input"
            type="number"
            min={min}
            max={max}
            step={step}
            value={text}
            disabled={disabled}
            aria-label={`${label}数值`}
            onFocus={() => setEditing(true)}
            onChange={(event) => {
              setText(event.target.value);
              const numeric = Number.parseFloat(event.target.value);
              if (Number.isFinite(numeric) && !disabled) onChange(clamp(numeric, min, max));
            }}
            onBlur={commit}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                inputRef.current?.blur();
              }
            }}
          />
          {suffix ? <span className="slider-unit">{suffix}</span> : null}
          <button
            type="button"
            className="slider-reset"
            aria-label={`重置${label}`}
            title={`重置${label}`}
            disabled={!active || disabled}
            onClick={() => onChange(defaultValue)}
          >
            ↺
          </button>
        </div>
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
          disabled={disabled}
          aria-label={label}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </div>
    </div>
  );
}
