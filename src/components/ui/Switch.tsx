interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** 无障碍名称；开关本身不显示文字。 */
  label: string;
  disabled?: boolean;
}

/** 极简开关：一个按钮 + 一个滑块，不引入任何第三方依赖。 */
export function Switch({ checked, onChange, label, disabled = false }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      className={`switch ${checked ? "is-on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-knob" />
    </button>
  );
}
