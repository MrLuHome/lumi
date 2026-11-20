import { useCallback, useEffect, useMemo, useState } from "react";
import { Download, FolderOpen, RefreshCw, Save, Trash2, Upload } from "lucide-react";
import type { PresetCategory, RetouchPreset } from "../types";
import {
  applyPresetToPhoto,
  deleteCustomPreset,
  duplicateCustomPreset,
  exportPreset,
  importPreset,
  listPresets,
  presetParamsFromEdit,
  presetPreview,
  presetPreviews,
  renameCustomPreset,
  saveCustomPreset,
} from "../lib/backend";
import {
  selectActiveParams,
  selectActivePhoto,
  useEditorStore,
} from "../state/editorStore";

const CATEGORIES: { value: "all" | PresetCategory; label: string }[] = [
  { value: "all", label: "全部" },
  { value: "portraitNatural", label: "人像自然" },
  { value: "skinTone", label: "肤色" },
  { value: "color", label: "色彩" },
  { value: "mood", label: "氛围" },
  { value: "custom", label: "我的预设" },
];

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败，请重试";
}

export function PresetPanel() {
  const photo = useEditorStore(selectActivePhoto);
  const params = useEditorStore(selectActiveParams);
  const presets = useEditorStore((state) => state.presets);
  const presetStatus = useEditorStore((state) => state.presetStatus);
  const presetError = useEditorStore((state) => state.presetError);
  const strength = useEditorStore((state) => state.presetStrength);
  const previews = useEditorStore((state) => state.presetPreviews);
  const previewStatus = useEditorStore((state) => state.presetPreviewStatus);
  const selectedId = useEditorStore((state) => state.selectedPresetId);
  const setPresets = useEditorStore((state) => state.setPresets);
  const setPresetStatus = useEditorStore((state) => state.setPresetStatus);
  const setPresetError = useEditorStore((state) => state.setPresetError);
  const setStrength = useEditorStore((state) => state.setPresetStrength);
  const setPreviews = useEditorStore((state) => state.setPresetPreviews);
  const setPreviewStatus = useEditorStore((state) => state.setPresetPreviewStatus);
  const setSelected = useEditorStore((state) => state.setSelectedPreset);
  const applyExternalParams = useEditorStore((state) => state.applyExternalParams);
  const undo = useEditorStore((state) => state.undo);
  const notify = useEditorStore((state) => state.notify);

  const [category, setCategory] = useState<"all" | PresetCategory>("all");
  const [livePreview, setLivePreview] = useState<string | null>(null);
  const [liveWarnings, setLiveWarnings] = useState<string[]>([]);
  const [liveBusy, setLiveBusy] = useState(false);
  const [applying, setApplying] = useState(false);
  const [saveOpen, setSaveOpen] = useState(false);
  const [saveName, setSaveName] = useState("");
  const [saveDescription, setSaveDescription] = useState("");
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  const selected = useMemo(
    () => presets.find((preset) => preset.id === selectedId) ?? null,
    [presets, selectedId],
  );

  const loadPresets = useCallback(async () => {
    setPresetStatus("loading");
    try {
      const result = await listPresets();
      setPresets(result.presets);
    } catch (failure) {
      setPresetError(toMessage(failure));
    }
  }, [setPresetStatus, setPresets, setPresetError]);

  useEffect(() => {
    if (presets.length === 0) void loadPresets();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshPreviews = useCallback(async () => {
    if (!photo) return;
    setPreviewStatus("loading");
    try {
      const result = await presetPreviews(photo.path, params, 240);
      const map: Record<string, string> = {};
      for (const item of result.previews) map[item.presetId] = item.url;
      setPreviews(map);
      setPreviewStatus(result.failed.length > 0 && result.previews.length === 0 ? "error" : "ready");
      if (result.failed.length > 0) {
        notify("info", `部分预设缩略图生成失败：${result.failed.slice(0, 3).join("；")}`);
      }
    } catch (failure) {
      setPreviewStatus("error");
      notify("error", toMessage(failure));
    }
  }, [photo, params, setPreviews, setPreviewStatus, notify]);

  // 只在切换照片或手动刷新时生成整套缩略图，拖动参数不会重复请求。
  useEffect(() => {
    if (!photo) return;
    void refreshPreviews();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photo?.id]);

  // 选中预设 + 强度变化时，只在小尺寸上生成实时预览（150ms 防抖）。
  useEffect(() => {
    if (!photo || !selectedId) {
      setLivePreview(null);
      setLiveWarnings([]);
      return;
    }
    let alive = true;
    const timer = window.setTimeout(() => {
      setLiveBusy(true);
      presetPreview(photo.path, params, selectedId, strength, 360)
        .then((result) => {
          if (alive) {
            setLivePreview(result.url);
            // 例如缺少同名 LUT 时，在这里明确提示，而不是静默跳过。
            setLiveWarnings(result.warnings);
          }
        })
        .catch(() => {
          if (alive) {
            setLivePreview(null);
            setLiveWarnings([]);
          }
        })
        .finally(() => {
          if (alive) setLiveBusy(false);
        });
    }, 150);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photo?.id, selectedId, strength]);

  const handleApply = useCallback(async () => {
    if (!photo || !selected) return;
    setApplying(true);
    try {
      const result = await applyPresetToPhoto(photo.path, params, selected.id, strength);
      applyExternalParams(photo.id, result.params, `应用预设：${selected.name}`);
      if (result.warnings.length > 0) notify("info", result.warnings.join("；"));
      notify("success", `已应用预设「${selected.name}」· 强度 ${strength}%`);
    } catch (failure) {
      notify("error", toMessage(failure));
    } finally {
      setApplying(false);
    }
  }, [photo, params, selected, strength, applyExternalParams, notify]);

  const handleSaveCurrent = useCallback(async () => {
    if (!photo || saveName.trim().length === 0) {
      notify("error", "请填写预设名称");
      return;
    }
    try {
      const subset = await presetParamsFromEdit(params);
      const saved = await saveCustomPreset(null, saveName.trim(), saveDescription.trim(), subset);
      await loadPresets();
      setSelected(saved.preset.id);
      setSaveOpen(false);
      setSaveName("");
      setSaveDescription("");
      notify("success", `自定义预设「${saved.preset.name}」已保存`);
    } catch (failure) {
      notify("error", toMessage(failure));
    }
  }, [photo, params, saveName, saveDescription, loadPresets, setSelected, notify]);

  const handleImport = useCallback(async () => {
    try {
      const saved = await importPreset();
      await loadPresets();
      setSelected(saved.preset.id);
      notify("success", `已导入预设「${saved.preset.name}」`);
    } catch (failure) {
      const message = toMessage(failure);
      if (!message.includes("取消")) notify("error", message);
    }
  }, [loadPresets, setSelected, notify]);

  const handleExport = useCallback(
    async (preset: RetouchPreset) => {
      try {
        const result = await exportPreset(preset.id);
        notify("success", `预设已导出：${result.path.split(/[\\/]/).pop()}`);
      } catch (failure) {
        const message = toMessage(failure);
        if (!message.includes("取消")) notify("error", message);
      }
    },
    [notify],
  );

  const handleRename = useCallback(async () => {
    if (!renameTarget || renameValue.trim().length === 0) return;
    try {
      await renameCustomPreset(renameTarget, renameValue.trim());
      await loadPresets();
      setRenameTarget(null);
      notify("success", "预设已重命名");
    } catch (failure) {
      notify("error", toMessage(failure));
    }
  }, [renameTarget, renameValue, loadPresets, notify]);

  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      await deleteCustomPreset(deleteTarget);
      await loadPresets();
      if (selectedId === deleteTarget) setSelected(null);
      setDeleteTarget(null);
      notify("success", "自定义预设已删除");
    } catch (failure) {
      notify("error", toMessage(failure));
    }
  }, [deleteTarget, selectedId, loadPresets, setSelected, notify]);

  const handleDuplicate = useCallback(
    async (preset: RetouchPreset) => {
      try {
        const saved = await duplicateCustomPreset(preset.id, `${preset.name} 副本`);
        await loadPresets();
        setSelected(saved.preset.id);
        notify("success", "已复制为自定义预设");
      } catch (failure) {
        notify("error", toMessage(failure));
      }
    },
    [loadPresets, setSelected, notify],
  );

  const visible = useMemo(
    () => presets.filter((preset) => category === "all" || preset.category === category),
    [presets, category],
  );

  return (
    <aside className="panel preset-panel">
      <div className="panel-head">
        <div>
          <h2>预设库</h2>
          <p className="panel-sub">缩略图来自当前照片的真实渲染，强度按参数语义混合</p>
        </div>
        <div className="preset-head-actions">
          <button type="button" className="btn" onClick={() => void loadPresets()}>
            <RefreshCw size={13} />
          </button>
          <button type="button" className="btn" onClick={() => void handleImport()}>
            <Upload size={13} /> 导入
          </button>
        </div>
      </div>

      <div className="preset-tabs">
        {CATEGORIES.map((item) => (
          <button
            key={item.value}
            type="button"
            className={category === item.value ? "active" : ""}
            onClick={() => setCategory(item.value)}
          >
            {item.label}
            <em>
              {item.value === "all"
                ? presets.length
                : presets.filter((preset) => preset.category === item.value).length}
            </em>
          </button>
        ))}
      </div>

      {presetStatus === "error" ? (
        <div className="smart-error">{presetError ?? "预设读取失败"}</div>
      ) : null}

      <div className="preset-grid">
        {visible.map((preset) => {
          const url = previews[preset.id];
          return (
            <button
              key={preset.id}
              type="button"
              className={`preset-card ${selectedId === preset.id ? "active" : ""}`}
              onClick={() => setSelected(preset.id)}
            >
              <span className="preset-thumb">
                {url ? (
                  <img src={url} alt={`${preset.name} 在当前照片上的真实效果`} />
                ) : (
                  <span className="preset-thumb-pending">
                    {previewStatus === "loading" ? "生成中…" : "暂无预览"}
                  </span>
                )}
                {preset.source === "custom" ? <em className="preset-tag">自定义</em> : null}
              </span>
              <span className="preset-name">{preset.name}</span>
              <span className="preset-desc">{preset.description}</span>
            </button>
          );
        })}
        {visible.length === 0 && presetStatus !== "loading" ? (
          <div className="panel-empty compact">这个分类还没有预设</div>
        ) : null}
      </div>

      {selected ? (
        <section className="preset-detail">
          <div className="section-title">
            {selected.name}
            <span className="section-meta">
              {selected.source === "builtin" ? "内置" : "自定义"} · v{selected.version}
            </span>
          </div>
          {livePreview ? (
            <div className="preset-live">
              <img src={livePreview} alt={`${selected.name} 实时预览`} />
              {liveBusy ? <span className="preset-live-badge">更新中…</span> : null}
            </div>
          ) : null}
          {liveWarnings.length > 0 ? (
            <ul className="warning-list">
              {liveWarnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          ) : null}
          <p className="preset-description">{selected.description}</p>
          <p className="hint-text">适用：{selected.suitableFor || "未标注"}</p>
          <p className="hint-text">不适用：{selected.avoid || "未标注"}</p>

          <div className="section-title">
            强度
            <span className="section-meta">{strength}%</span>
          </div>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={strength}
            onChange={(event) => setStrength(Number(event.target.value))}
          />

          <div className="preset-actions">
            <button
              type="button"
              className="btn btn-accent"
              onClick={() => void handleApply()}
              disabled={applying}
            >
              <Save size={13} /> 应用
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => {
                undo();
                notify("info", "已撤销上一次修改");
              }}
            >
              取消 / 撤销
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setSaveName(selected.name.startsWith("我的") ? "" : `${selected.name} 调整`);
                setSaveOpen(true);
              }}
            >
              <Download size={13} /> 另存为自定义
            </button>
            <button type="button" className="btn" onClick={() => void handleExport(selected)}>
              导出
            </button>
            {selected.source === "custom" ? (
              <>
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    setRenameTarget(selected.id);
                    setRenameValue(selected.name);
                  }}
                >
                  重命名
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => setDeleteTarget(selected.id)}
                >
                  <Trash2 size={13} /> 删除
                </button>
              </>
            ) : (
              <button type="button" className="btn" onClick={() => void handleDuplicate(selected)}>
                复制为自定义
              </button>
            )}
          </div>

          {renameTarget ? (
            <div className="inline-form">
              <input
                value={renameValue}
                onChange={(event) => setRenameValue(event.target.value)}
                placeholder="新的预设名称"
              />
              <button type="button" className="btn" onClick={() => void handleRename()}>
                确认
              </button>
              <button type="button" className="btn" onClick={() => setRenameTarget(null)}>
                取消
              </button>
            </div>
          ) : null}

          {deleteTarget ? (
            <div className="inline-confirm">
              <span>确认删除这个自定义预设？内置预设不受影响。</span>
              <button type="button" className="btn btn-danger" onClick={() => void handleDelete()}>
                确认删除
              </button>
              <button type="button" className="btn" onClick={() => setDeleteTarget(null)}>
                取消
              </button>
            </div>
          ) : null}
        </section>
      ) : null}

      <section className="preset-save">
        <button type="button" className="btn" onClick={() => setSaveOpen((value) => !value)}>
          <FolderOpen size={13} /> 从当前照片保存预设
        </button>
        {saveOpen ? (
          <div className="inline-form column">
            <input
              value={saveName}
              onChange={(event) => setSaveName(event.target.value)}
              placeholder="预设名称"
            />
            <input
              value={saveDescription}
              onChange={(event) => setSaveDescription(event.target.value)}
              placeholder="说明（可选）"
            />
            <button type="button" className="btn btn-accent" onClick={() => void handleSaveCurrent()}>
              保存为我的预设
            </button>
            <p className="hint-text">
              只保存曝光、色彩、HSL、曲线、颗粒、暗角与美肤模板，不包含人脸 UUID、蒙版笔触、
              变形锚点或背景图片路径。
            </p>
          </div>
        ) : null}
      </section>
    </aside>
  );
}
