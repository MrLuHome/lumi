import { RotateCcw } from "lucide-react";
import { SHAPE_ITEM_META, type ShapeItemId } from "../../state/slim";
import { Slider } from "../ui/Slider";
import { Switch } from "../ui/Switch";

interface ShapeItemRowProps {
  id: ShapeItemId;
  strength: number;
  disabled: boolean;
  onToggle: (enabled: boolean) => void;
  onChange: (value: number) => void;
  onReset: () => void;
}

/** G15：瘦脸 / 瘦身单项——开关 + 0–100 + 重置（普通模式只暴露滑块）。 */
export function ShapeItemRow({
  id,
  strength,
  disabled,
  onToggle,
  onChange,
  onReset,
}: ShapeItemRowProps) {
  const meta = SHAPE_ITEM_META[id];
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
    </section>
  );
}
