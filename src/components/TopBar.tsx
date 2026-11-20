import {
  Columns2,
  Download,
  FolderOpen,
  Redo2,
  RotateCcw,
  Ruler,
  Save,
  Undo2,
} from "lucide-react";
import type { PhotoAsset, Workspace } from "../types";
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
  canPaste: boolean;
  compareMode: CompareMode;
  workspace: Workspace;
  onWorkspace: (workspace: Workspace) => void;
  importing: boolean;
  exporting: boolean;
  onImport: () => void;
  onExport: () => void;
  onSaveProject: () => void;
  onOpenProject: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onCopyParams: () => void;
  onPasteParams: () => void;
  onToggleCompare: () => void;
  onResetAll: () => void;
  onFit: () => void;
  onActualPixels: () => void;
  onZoom200: () => void;
}

export function TopBar({
  photo,
  canUndo,
  canRedo,
  canPaste,
  compareMode,
  workspace,
  onWorkspace,
  importing,
  exporting,
  onImport,
  onExport,
  onSaveProject,
  onOpenProject,
  onUndo,
  onRedo,
  onCopyParams,
  onPasteParams,
  onToggleCompare,
  onResetAll,
  onFit,
  onActualPixels,
  onZoom200,
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
            { label: "打开项目…", shortcut: "⇧⌘O", onSelect: onOpenProject },
            {
              label: "保存项目…",
              shortcut: "⌘S",
              disabled: !photo,
              onSelect: onSaveProject,
            },
            "separator",
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
            { label: "复制参数", shortcut: "⌥⌘C", disabled: !photo, onSelect: onCopyParams },
            { label: "粘贴参数", shortcut: "⌥⌘V", disabled: !photo || !canPaste, onSelect: onPasteParams },
            "separator",
            { label: "重置全部参数", disabled: !photo, onSelect: onResetAll },
          ]}
        />
        <Menu
          label="视图"
          items={[
            { label: "适应窗口", shortcut: "F", disabled: !photo, onSelect: onFit },
            { label: "实际像素", shortcut: "1", disabled: !photo, onSelect: onActualPixels },
            { label: "放大到 200%", shortcut: "2", disabled: !photo, onSelect: onZoom200 },
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

      <nav className="workspace-switch" aria-label="工作区">
        {(
          [
            ["edit", "编辑"],
            ["smart", "智能精修"],
            ["presets", "预设库"],
            ["batch", "批量处理"],
          ] as [Workspace, string][]
        ).map(([value, label]) => (
          <button
            key={value}
            type="button"
            className={`workspace-tab ${workspace === value ? "active" : ""}`}
            onClick={() => onWorkspace(value)}
          >
            {label}
          </button>
        ))}
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
          title="保存项目 (⌘S)"
          onClick={onSaveProject}
          disabled={!photo}
        >
          <Save size={14} />
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
