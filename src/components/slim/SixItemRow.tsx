import { RotateCcw } from "lucide-react";
import { ITEM_META, type SixItemId } from "../../state/slim";
import { Slider } from "../ui/Slider";
import { Switch } from "../ui/Switch";

interface SixItemRowProps {
  id: SixItemId;
  /** 从当前参数反解出来的强度（唯一事实来源）。 */
  strength: number;
  /** 当前值不是某个强度的一次写入（例如老项目留下的手工值）。 */
  manual: boolean;
  disabled: boolean;
  /** ⑤⑥ 的白话方向说明；①②③④ 为 null。 */
  direction: string | null;
  onToggle: (enabled: boolean) => void;
  onChange: (value: number) => void;
  onReset: () => void;
}

/** 六项之一：开关 + 0-100 强度 + 单项重置。 */
export function SixItemRow({
  id,
  strength,
  manual,
  disabled,
  direction,
  onToggle,
  onChange,
  onReset,
}: SixItemRowProps) {
  const meta = ITEM_META[id];
  const enabled = strength > 0;
  return (
    <section className={`six-item ${enabled ? "is-on" : ""} ${disabled ? "is-disabled" : ""}`}>
      <header className="six-item-head">
        <Switch
          checked={enabled}
          label={meta.label}
          disabled={disabled}
          onChange={onToggle}
        />
        <span className="six-item-label">{meta.label}</span>
        {manual ? <span className="six-item-tag">微调过</span> : null}
        <span className="six-item-value">{Math.round(strength)}</span>
        <button
          type="button"
          className="six-item-reset"
          title={`恢复默认（${meta.label}）`}
          disabled={disabled || strength <= 0}
          onClick={onReset}
        >
          <RotateCcw size={11} />
          恢复默认
        </button>
      </header>
      <Slider
        label={meta.label}
        value={strength}
        min={0}
        max={100}
        step={1}
        bipolar={false}
        defaultValue={0}
        disabled={disabled}
        onChange={onChange}
      />
      <p className="six-item-hint">{meta.hint}</p>
      {direction ? <p className="six-item-direction">{direction}</p> : null}
    </section>
  );
}
