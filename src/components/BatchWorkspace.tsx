import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowDownAZ,
  CheckSquare,
  Copy,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  Square,
  Trash2,
  XCircle,
} from "lucide-react";
import type { BatchSnapshot, ConflictPolicy, ExportFormat } from "../types";
import {
  applyPresetToPhoto,
  batchCancelAll,
  batchCancelCurrent,
  batchClearCompleted,
  batchPause,
  batchResume,
  batchRetryFailed,
  batchStart,
  batchStatus,
  pickBatchOutputDir,
  subscribeBatchProgress,
} from "../lib/backend";
import {
  COPY_GROUPS,
  DEFAULT_COPY_GROUPS,
  OBJECT_LEVEL_GROUPS,
  applyCopy,
  describeGroups,
  extractCopy,
  type CopyGroupId,
  type ParamCopy,
} from "../state/paramGroups";
import { useEditorStore } from "../state/editorStore";
import { formatDuration, formatFileSize } from "../lib/format";

function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败，请重试";
}

const STATUS_LABELS: Record<string, string> = {
  pending: "等待",
  running: "进行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
  skipped: "已跳过",
};

export function BatchWorkspace() {
  const photos = useEditorStore((state) => state.photos);
  const params = useEditorStore((state) => state.params);
  const activeId = useEditorStore((state) => state.activeId);
  const setActive = useEditorStore((state) => state.setActive);
  const selection = useEditorStore((state) => state.batchSelection);
  const toggleSelection = useEditorStore((state) => state.toggleBatchSelection);
  const setSelection = useEditorStore((state) => state.setBatchSelection);
  const snapshot = useEditorStore((state) => state.batchSnapshot);
  const setSnapshot = useEditorStore((state) => state.setBatchSnapshot);
  const outputDir = useEditorStore((state) => state.batchOutputDir);
  const setOutputDir = useEditorStore((state) => state.setBatchOutputDir);
  const naming = useEditorStore((state) => state.batchNaming);
  const setNaming = useEditorStore((state) => state.setBatchNaming);
  const startIndex = useEditorStore((state) => state.batchStartIndex);
  const setStartIndex = useEditorStore((state) => state.setBatchStartIndex);
  const conflict = useEditorStore((state) => state.batchConflict);
  const setConflict = useEditorStore((state) => state.setBatchConflict);
  const settings = useEditorStore((state) => state.batchSettings);
  const setSettings = useEditorStore((state) => state.setBatchSettings);
  const applyExternalParams = useEditorStore((state) => state.applyExternalParams);
  const notify = useEditorStore((state) => state.notify);
  const presets = useEditorStore((state) => state.presets);
  const rawByPhoto = useEditorStore((state) => state.rawByPhoto);

  const [sortAsc, setSortAsc] = useState(true);
  const [formatFilter, setFormatFilter] = useState<"all" | string>("all");
  const [copyGroups, setCopyGroups] = useState<CopyGroupId[]>(DEFAULT_COPY_GROUPS);
  const [copied, setCopied] = useState<ParamCopy | null>(null);
  const [overwriteConfirmed, setOverwriteConfirmed] = useState(false);
  const [presetId, setPresetId] = useState("");
  const [presetStrength, setPresetStrength] = useState(100);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    let unsubscribe: (() => void) | undefined;
    subscribeBatchProgress((next) => {
      if (alive) setSnapshot(next);
    }).then((cleanup) => {
      if (alive) unsubscribe = cleanup;
      else cleanup();
    });
    batchStatus()
      .then((next) => {
        if (alive) setSnapshot(next);
      })
      .catch(() => {});
    return () => {
      alive = false;
      unsubscribe?.();
    };
  }, [setSnapshot]);

  const formats = useMemo(
    () => Array.from(new Set(photos.map((photo) => photo.format.toLowerCase()))).sort(),
    [photos],
  );

  const visiblePhotos = useMemo(() => {
    const list = photos.filter(
      (photo) =>
        formatFilter === "all" || photo.format.toLowerCase() === formatFilter.toLowerCase(),
    );
    return [...list].sort((left, right) =>
      sortAsc
        ? left.fileName.localeCompare(right.fileName, "zh")
        : right.fileName.localeCompare(left.fileName, "zh"),
    );
  }, [photos, formatFilter, sortAsc]);

  const selectedPhotos = useMemo(
    () => photos.filter((photo) => selection.includes(photo.id)),
    [photos, selection],
  );

  // 首次进入或删除照片后，保证选择集合合法；为空且有待处理照片时默认全选。
  useEffect(() => {
    const existing = new Set(photos.map((photo) => photo.id));
    const next = selection.filter((id) => existing.has(id));
    if (next.length !== selection.length) {
      setSelection(next);
    } else if (next.length === 0 && photos.length > 0) {
      setSelection(photos.map((photo) => photo.id));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photos]);

  const handleCopyFromActive = useCallback(() => {
    const sourceParams = activeId ? params[activeId] : undefined;
    if (!sourceParams) {
      notify("error", "请先在照片列表中选择当前照片");
      return;
    }
    setCopied(extractCopy(sourceParams, copyGroups));
    notify(
      "info",
      `已复制：${describeGroups(copyGroups).join("、") || "无"}；对象级参数始终不复制`,
    );
  }, [activeId, params, copyGroups, notify]);

  const handleApplyCopy = useCallback(() => {
    if (!copied) {
      notify("error", "请先复制当前照片的参数");
      return;
    }
    if (selectedPhotos.length === 0) {
      notify("error", "请先选择目标照片");
      return;
    }
    const warnings: string[] = [];
    for (const photo of selectedPhotos) {
      const target = params[photo.id];
      if (!target) continue;
      const result = applyCopy(target, copied);
      applyExternalParams(photo.id, result.params, "批量同步参数");
      warnings.push(...result.warnings);
    }
    if (warnings.length > 0) notify("info", Array.from(new Set(warnings)).join("；"));
    notify("success", `已把参数同步到 ${selectedPhotos.length} 张照片，各自保留独立历史`);
  }, [copied, selectedPhotos, params, applyExternalParams, notify]);

  const handleApplyPreset = useCallback(async () => {
    if (!presetId) {
      notify("error", "请选择预设");
      return;
    }
    if (selectedPhotos.length === 0) {
      notify("error", "请先选择目标照片");
      return;
    }
    setBusy(true);
    const warnings: string[] = [];
    let applied = 0;
    try {
      for (const photo of selectedPhotos) {
        const target = params[photo.id];
        if (!target) continue;
        const result = await applyPresetToPhoto(
          photo.path,
          target,
          presetId,
          presetStrength,
        );
        applyExternalParams(photo.id, result.params, "批量套用预设");
        warnings.push(...result.warnings);
        applied += 1;
      }
      if (warnings.length > 0) notify("info", Array.from(new Set(warnings)).join("；"));
      notify("success", `已对 ${applied} 张照片套用预设，参数仍然独立可调`);
    } catch (failure) {
      notify("error", toMessage(failure));
    } finally {
      setBusy(false);
    }
  }, [presetId, presetStrength, selectedPhotos, params, applyExternalParams, notify]);

  const handlePickDir = useCallback(async () => {
    try {
      const dir = await pickBatchOutputDir();
      if (dir) setOutputDir(dir);
    } catch (failure) {
      notify("error", toMessage(failure));
    }
  }, [setOutputDir, notify]);

  const handleStart = useCallback(async () => {
    if (!outputDir) {
      notify("error", "请先选择输出目录");
      return;
    }
    if (selectedPhotos.length === 0) {
      notify("error", "请选择要导出的照片");
      return;
    }
    if (conflict === "overwrite" && !overwriteConfirmed) {
      notify("error", "覆盖已有文件需要先勾选确认");
      return;
    }
    setBusy(true);
    try {
      const started = await batchStart({
        photos: selectedPhotos.flatMap((photo) => {
          const photoParams = params[photo.id];
          return photoParams
            ? [{ path: photo.path, params: photoParams, rawParams: rawByPhoto[photo.id] ?? null }]
            : [];
        }),
        settings,
        outputDir,
        naming,
        startIndex,
        conflict,
        overwriteConfirmed,
      });
      setSnapshot(started);
      notify("info", `批量任务已开始，共 ${started.total} 张`);
    } catch (failure) {
      notify("error", toMessage(failure));
    } finally {
      setBusy(false);
    }
  }, [
    outputDir,
    selectedPhotos,
    params,
    settings,
    naming,
    startIndex,
    conflict,
    overwriteConfirmed,
    setSnapshot,
    notify,
  ]);

  const runControl = useCallback(
    async (action: () => Promise<BatchSnapshot>) => {
      try {
        const next = await action();
        setSnapshot(next);
      } catch (failure) {
        notify("error", toMessage(failure));
      }
    },
    [setSnapshot, notify],
  );

  const running = snapshot?.running ?? false;
  const failureList = snapshot?.tasks.filter((task) => task.status === "failed") ?? [];

  return (
    <section className="batch-workspace">
      <div className="batch-columns">
        <div className="batch-col photos-col">
          <div className="section-title">
            照片
            <span className="section-meta">已选 {selection.length}/{photos.length}</span>
          </div>
          <div className="batch-toolbar">
            <button
              type="button"
              className="btn"
              onClick={() => setSelection(photos.map((photo) => photo.id))}
            >
              <CheckSquare size={13} /> 全选
            </button>
            <button type="button" className="btn" onClick={() => setSelection([])}>
              <Square size={13} /> 取消全选
            </button>
            <button
              type="button"
              className="btn"
              onClick={() =>
                setSelection(
                  photos
                    .filter((photo) => !selection.includes(photo.id))
                    .map((photo) => photo.id),
                )
              }
            >
              反选
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => setSortAsc((value) => !value)}
              title="按名称排序"
            >
              <ArrowDownAZ size={13} /> {sortAsc ? "名称升序" : "名称降序"}
            </button>
            <select
              value={formatFilter}
              onChange={(event) => setFormatFilter(event.target.value)}
              aria-label="按格式筛选"
            >
              <option value="all">全部格式</option>
              {formats.map((format) => (
                <option key={format} value={format}>
                  {format.toUpperCase()}
                </option>
              ))}
            </select>
          </div>

          <div className="batch-photo-list">
            {visiblePhotos.map((photo) => {
              const checked = selection.includes(photo.id);
              return (
                <div
                  key={photo.id}
                  className={`batch-photo ${checked ? "checked" : ""} ${
                    activeId === photo.id ? "active" : ""
                  }`}
                  onClick={() => toggleSelection(photo.id)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") toggleSelection(photo.id);
                  }}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    readOnly
                    tabIndex={-1}
                    aria-label={`选择 ${photo.fileName}`}
                  />
                  <img src={photo.thumbUrl} alt="" />
                  <span className="batch-photo-meta">
                    <strong>{photo.fileName}</strong>
                    <em>
                      {photo.width}×{photo.height} · {photo.format.toUpperCase()} ·{" "}
                      {formatFileSize(photo.fileSize)}
                    </em>
                  </span>
                  <button
                    type="button"
                    className="link-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      setActive(photo.id);
                    }}
                  >
                    设为当前
                  </button>
                </div>
              );
            })}
            {visiblePhotos.length === 0 ? (
              <div className="panel-empty compact">没有符合筛选条件的照片</div>
            ) : null}
          </div>
        </div>

        <div className="batch-col settings-col">
          <div className="section-title">参数同步</div>
          <div className="copy-groups">
            {COPY_GROUPS.map((group) => (
              <label key={group.id} className="check-row">
                <input
                  type="checkbox"
                  checked={copyGroups.includes(group.id)}
                  onChange={() =>
                    setCopyGroups((current) =>
                      current.includes(group.id)
                        ? current.filter((id) => id !== group.id)
                        : [...current, group.id],
                    )
                  }
                />
                {group.label}
              </label>
            ))}
          </div>
          <details className="object-level">
            <summary>对象级参数（已排除，不可批量复制）</summary>
            <ul>
              {OBJECT_LEVEL_GROUPS.map((item) => (
                <li key={item.label}>
                  <strong>{item.label}</strong>：{item.reason}
                </li>
              ))}
            </ul>
          </details>
          <p className="hint-text">
            将覆盖：{describeGroups(copyGroups).join("、") || "无"}
          </p>
          <div className="batch-row">
            <button type="button" className="btn" onClick={handleCopyFromActive}>
              <Copy size={13} /> 复制当前照片
            </button>
            <button
              type="button"
              className="btn btn-accent"
              onClick={handleApplyCopy}
              disabled={!copied || selectedPhotos.length === 0}
            >
              应用到所选（{selectedPhotos.length}）
            </button>
          </div>
          {copied ? (
            <p className="hint-text">
              已复制 {copied.groups.length} 组参数，目标照片各自保留独立历史
            </p>
          ) : null}

          <div className="section-title">批量套用预设</div>
          <select value={presetId} onChange={(event) => setPresetId(event.target.value)}>
            <option value="">选择预设…</option>
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
          <div className="section-title">
            强度
            <span className="section-meta">{presetStrength}%</span>
          </div>
          <input
            type="range"
            min={0}
            max={100}
            value={presetStrength}
            onChange={(event) => setPresetStrength(Number(event.target.value))}
          />
          <button
            type="button"
            className="btn"
            onClick={() => void handleApplyPreset()}
            disabled={busy || !presetId || selectedPhotos.length === 0}
          >
            对所选 {selectedPhotos.length} 张应用预设
          </button>

          <div className="section-title">导出设置</div>
          <div className="batch-form">
            <label>
              格式
              <select
                value={settings.format}
                onChange={(event) =>
                  setSettings({ ...settings, format: event.target.value as ExportFormat })
                }
              >
                <option value="jpeg">JPEG</option>
                <option value="png">PNG</option>
                <option value="webp">WebP</option>
              </select>
            </label>
            <label>
              质量
              <input
                type="range"
                min={60}
                max={100}
                value={settings.quality}
                onChange={(event) =>
                  setSettings({ ...settings, quality: Number(event.target.value) })
                }
              />
              <span>{settings.quality}</span>
            </label>
            <label>
              尺寸
              <span className="readonly-value">原图尺寸（走正式全尺寸管线）</span>
            </label>
            <label>
              输出目录
              <span className="dir-row">
                <input
                  value={outputDir}
                  onChange={(event) => setOutputDir(event.target.value)}
                  placeholder="选择或输入目录"
                />
                <button type="button" className="btn" onClick={() => void handlePickDir()}>
                  <FolderOpen size={13} />
                </button>
              </span>
            </label>
            <label>
              命名规则
              <input
                value={naming}
                onChange={(event) => setNaming(event.target.value)}
                placeholder="{name}-lumi"
              />
            </label>
            <p className="hint-text">支持 {"{name}"} / {"{index}"} / {"{format}"}</p>
            <label>
              起始编号
              <input
                type="number"
                min={1}
                value={startIndex}
                onChange={(event) => setStartIndex(Number(event.target.value))}
              />
            </label>
            <label>
              同名冲突
              <select
                value={conflict}
                onChange={(event) => {
                  setConflict(event.target.value as ConflictPolicy);
                  if (event.target.value !== "overwrite") setOverwriteConfirmed(false);
                }}
              >
                <option value="skip">跳过已有文件</option>
                <option value="rename">自动重命名</option>
                <option value="overwrite">覆盖（需确认）</option>
              </select>
            </label>
            {conflict === "overwrite" ? (
              <label className="warn-row">
                <input
                  type="checkbox"
                  checked={overwriteConfirmed}
                  onChange={(event) => setOverwriteConfirmed(event.target.checked)}
                />
                我确认允许覆盖输出目录中的同名文件（原图永远不会被覆盖）
              </label>
            ) : null}
          </div>
          <button
            type="button"
            className="btn btn-accent batch-start"
            onClick={() => void handleStart()}
            disabled={busy || running || selectedPhotos.length === 0}
          >
            <Play size={14} /> 开始导出 {selectedPhotos.length} 张
          </button>
        </div>

        <div className="batch-col queue-col">
          <div className="section-title">
            任务队列
            <span className="section-meta">
              {snapshot
                ? `${snapshot.completed}/${snapshot.total} 完成 · ${snapshot.failed} 失败 · ${
                    snapshot.skipped
                  } 跳过 · ${snapshot.cancelled} 取消`
                : "尚未开始"}
            </span>
          </div>
          {snapshot ? (
            <>
              <div className="batch-progress-summary">
                <div className="progress-track">
                  <span style={{ width: `${Math.round(snapshot.overall * 100)}%` }} />
                </div>
                <div className="progress-meta">
                  <span>{Math.round(snapshot.overall * 100)}%</span>
                  <span>已导出 {snapshot.exported} 张</span>
                  <span>耗时 {formatDuration(snapshot.elapsedMs)}</span>
                  <span>
                    {snapshot.etaMs !== null
                      ? `预计剩余 ${formatDuration(snapshot.etaMs)}（按实际耗时估算）`
                      : "剩余时间：样本不足"}
                  </span>
                </div>
                {snapshot.current ? (
                  <p className="hint-text">
                    当前：{snapshot.current.name} · {snapshot.current.stage}
                  </p>
                ) : snapshot.paused ? (
                  <p className="hint-text">已暂停，当前任务完成阶段后不再启动新任务</p>
                ) : null}
              </div>
              <div className="batch-controls">
                <button
                  type="button"
                  className="btn"
                  disabled={!snapshot.running || snapshot.paused}
                  onClick={() => void runControl(batchPause)}
                >
                  <Pause size={13} /> 暂停
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={!snapshot.running || !snapshot.paused}
                  onClick={() => void runControl(batchResume)}
                >
                  <Play size={13} /> 继续
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={!snapshot.running}
                  onClick={() => void runControl(batchCancelCurrent)}
                >
                  <XCircle size={13} /> 取消当前
                </button>
                <button
                  type="button"
                  className="btn btn-danger"
                  disabled={!snapshot.running && snapshot.pending === 0}
                  onClick={() => void runControl(batchCancelAll)}
                >
                  取消全部
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={snapshot.running || snapshot.failed === 0}
                  onClick={() => void runControl(batchRetryFailed)}
                >
                  <RotateCcw size={13} /> 重试失败
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={snapshot.running}
                  onClick={() => void runControl(batchClearCompleted)}
                >
                  <Trash2 size={13} /> 清理完成
                </button>
              </div>
              <div className="batch-task-list">
                {snapshot.tasks.map((task) => (
                  <div key={task.jobId} className={`batch-task status-${task.status}`}>
                    <span className="task-name" title={task.path}>
                      {task.number}. {task.name}
                    </span>
                    <span className="task-status">{STATUS_LABELS[task.status] ?? task.status}</span>
                    <span className="task-stage">{task.stage || "—"}</span>
                    <span className="task-progress">
                      <span style={{ width: `${Math.round(task.progress * 100)}%` }} />
                    </span>
                    {task.outputPath ? (
                      <span className="task-output" title={task.outputPath}>
                        {task.outputPath.split(/[\\/]/).pop()}
                      </span>
                    ) : (
                      <span className="task-output muted">—</span>
                    )}
                    {task.error ? (
                      <span className="task-error">
                        <AlertTriangle size={12} /> {task.error}
                      </span>
                    ) : null}
                  </div>
                ))}
              </div>
              {failureList.length > 0 ? (
                <div className="batch-failures">
                  <strong>失败原因</strong>
                  <ul>
                    {failureList.map((task) => (
                      <li key={task.jobId}>
                        {task.name}：{task.error ?? "未知错误"}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </>
          ) : (
            <div className="panel-empty">按「开始导出」后，这里会显示每张照片的真实进度</div>
          )}
        </div>
      </div>
    </section>
  );
}
