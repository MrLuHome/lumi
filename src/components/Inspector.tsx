import { useState } from "react";
import { ChevronDown, RotateCcw, Sparkles } from "lucide-react";
import type { PhotoAsset } from "../types";
import type { EditParams } from "../types";
import { PARAM_GROUPS, activeParamCount, isNeutral } from "../state/params";
import { useEditorStore } from "../state/editorStore";
import { isDesktopRuntime } from "../lib/platform";
import { Slider } from "./ui/Slider";

interface InspectorProps {
  photo: PhotoAsset | null;
  params: EditParams;
}

export function Inspector({ photo, params }: InspectorProps) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const updateParam = useEditorStore((state) => state.updateParam);
  const resetGroup = useEditorStore((state) => state.resetGroup);
  const resetAll = useEditorStore((state) => state.resetAll);
  const activeCount = activeParamCount(params);

  return (
    <aside className="inspector">
      <div className="inspector-head">
        <div>
          <span className="eyebrow">调整</span>
          <span className="inspector-count">
            {activeCount > 0 ? `${activeCount} 项已调整` : photo ? "未调整" : "等待照片"}
          </span>
        </div>
        <button
          type="button"
          className="ghost-btn"
          onClick={resetAll}
          disabled={!photo || isNeutral(params)}
          title="重置全部参数"
        >
          <RotateCcw size={12} />
          重置全部
        </button>
      </div>

      <div className={`inspector-body ${photo ? "" : "is-disabled"}`}>
        {PARAM_GROUPS.map((group) => {
          const open = !collapsed[group.id];
          const groupDirty = group.params.some((definition) => params[definition.key] !== 0);
          return (
            <section className={`param-group ${open ? "is-open" : ""}`} key={group.id}>
              <header className="param-group-head">
                <button
                  type="button"
                  className="param-group-toggle"
                  onClick={() =>
                    setCollapsed((current) => ({ ...current, [group.id]: !current[group.id] }))
                  }
                >
                  <ChevronDown size={12} className="chev" />
                  <span>{group.label}</span>
                </button>
                <button
                  type="button"
                  className="param-group-reset"
                  onClick={() => resetGroup(group.id)}
                  disabled={!groupDirty}
                >
                  重置
                </button>
              </header>

              {open && (
                <div className="param-group-body">
                  {group.params.map((definition) => (
                    <Slider
                      key={definition.key}
                      definition={definition}
                      value={params[definition.key]}
                      onChange={(value) => updateParam(definition.key, value)}
                    />
                  ))}
                  {group.hint && (
                    <p className="param-hint">
                      <Sparkles size={11} />
                      {group.hint}
                    </p>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </div>

      <div className="inspector-foot">
        <span>{isDesktopRuntime() ? "Rust 本地渲染" : "浏览器预览模式"}</span>
        <span>
          {photo
            ? `预览 ${photo.previewWidth} × ${photo.previewHeight} · 导出原图`
            : "尚未载入照片"}
        </span>
      </div>
    </aside>
  );
}
