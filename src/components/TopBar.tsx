import {
  Columns2,
  Download,
  FolderOpen,
  Redo2,
  RotateCcw,
  Ruler,
  Undo2,
} from "lucide-react";
import type { PhotoAsset } from "../types";
import { Menu } from "./ui/Menu";
import type { CompareMode } from "../state/editorStore";
import { isDesktopRuntime } from "../lib/platform";

async function closeWindow() {
  if (!isDesktopRuntime()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

interface TopBarProps {
  photo: PhotoAsset | null;
  canUndo: boolean;
  canRedo: boolean;
  compareMode: CompareMode;
  importing: boolean;
  exporting: boolean;
  onImport: () => void;
  onExport: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onToggleCompare: () => void;
  onResetAll: () => void;
  onFit: () => void;
  onActualPixels: () => void;
}

export function TopBar({
  photo,
  canUndo,
  canRedo,
  compareMode,
  importing,
  exporting,
  onImport,
  onExport,
  onUndo,
  onRedo,
  onToggleCompare,
  onResetAll,
  onFit,
  onActualPixels,
}: TopBarProps) {
  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <span className="brand-name">LUMI</span>
        <span className="brand-sub">RETOUCH</span>
      </div>

      <nav className="menubar">
        <Menu
          label="文件"
          items={[
            { label: "导入照片…", shortcut: "⌘O", onSelect: onImport },
            { label: "导出当前照片…", shortcut: "⌘E", disabled: !photo, onSelect: onExport },
            "separator",
            ...(isDesktopRuntime()
              ? [{ label: "退出 LUMI RETOUCH", onSelect: () => void closeWindow() }]
              : []),
          ]}
        />
        <Menu
          label="编辑"
          items={[
            { label: "撤销", shortcut: "⌘Z", disabled: !canUndo, onSelect: onUndo },
            { label: "重做", shortcut: "⇧⌘Z", disabled: !canRedo, onSelect: onRedo },
            "separator",
            { label: "重置全部参数", disabled: !photo, onSelect: onResetAll },
          ]}
        />
        <Menu
          label="视图"
          items={[
            { label: "适应窗口", shortcut: "F", disabled: !photo, onSelect: onFit },
            { label: "实际像素", shortcut: "1", disabled: !photo, onSelect: onActualPixels },
            "separator",
            {
              label: compareMode === "split" ? "关闭对比" : "分屏对比",
              shortcut: "⌘\\",
              disabled: !photo,
              onSelect: onToggleCompare,
            },
          ]}
        />
      </nav>

      <div className="topbar-actions">
        <button type="button" className="btn btn-accent" onClick={onImport} disabled={importing}>
          <FolderOpen size={14} />
          {importing ? "导入中…" : "导入"}
        </button>
        <div className="topbar-divider" />
        <button
          type="button"
          className="icon-btn"
          title="撤销 (⌘Z)"
          onClick={onUndo}
          disabled={!canUndo}
        >
          <Undo2 size={15} />
        </button>
        <button
          type="button"
          className="icon-btn"
          title="重做 (⇧⌘Z)"
          onClick={onRedo}
          disabled={!canRedo}
        >
          <Redo2 size={15} />
        </button>
        <div className="topbar-divider" />
        <button
          type="button"
          className={`btn ${compareMode === "split" ? "btn-active" : ""}`}
          onClick={onToggleCompare}
          disabled={!photo}
          title="分屏对比 (⌘\\)"
        >
          <Columns2 size={14} />
          对比
        </button>
        <button type="button" className="btn" onClick={onExport} disabled={!photo || exporting}>
          <Download size={14} />
          {exporting ? "导出中…" : "导出"}
        </button>
        <button
          type="button"
          className="icon-btn"
          title="重置参数"
          onClick={onResetAll}
          disabled={!photo}
        >
          <RotateCcw size={14} />
        </button>
      </div>

      <div className="topbar-spacer" data-tauri-drag-region />

      <div className="topbar-meta" data-tauri-drag-region>
        <Ruler size={12} />
        <span>{photo ? `${photo.width} × ${photo.height}` : "工程底座 v0.1"}</span>
      </div>
    </header>
  );
}
