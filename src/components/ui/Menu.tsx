import { useEffect, useRef, useState } from "react";

export interface MenuItemDef {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onSelect: () => void;
}

interface MenuProps {
  label: string;
  items: Array<MenuItemDef | "separator">;
}

export function Menu({ label, items }: MenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className={`menu ${open ? "is-open" : ""}`} ref={rootRef}>
      <button
        type="button"
        className="menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {label}
      </button>
      {open && (
        <div className="menu-popover" role="menu">
          {items.map((item, index) =>
            item === "separator" ? (
              <div className="menu-separator" key={`separator-${index}`} />
            ) : (
              <button
                type="button"
                className="menu-item"
                key={item.label}
                role="menuitem"
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
              >
                <span>{item.label}</span>
                {item.shortcut && <span className="menu-shortcut">{item.shortcut}</span>}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}
