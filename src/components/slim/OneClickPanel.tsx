import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Columns2, RefreshCw, Sparkles, Undo2 } from "lucide-react";
import type { PhotoAsset } from "../../types";
import {
  selectCanUndo,
  useEditorStore,
} from "../../state/editorStore";
import {
  ONE_CLICK_MODES,
  applyOneClickPlan,
  buildOneClickPlan,
  itemsFromParams,
  toAppliedPlan,
  tonalAnchors,
  tonalDeltas,
  type OneClickMode,
  type OneClickPlan,
} from "../../state/slim";
import { Slider } from "../ui/Slider";
import { ensureBeauty, ensureFaces, ensureGlobalReport, plainMessage } from "./analysis";

/**
 * 一键精修面板。
 *
 * 真实链路（零新增 Rust 命令）：
 *   detect_faces（预热，必须有）→ analyze_beauty（①②③④）→ analyze_retouch(standard)（⑤⑥）
 *   → buildOneClickPlan（TS 合成）→ applyExternalParams（一条撤销记录）
 *
 * 三档与整体强度只在这里乘一次，参数仍然是普通可编辑参数，⌘Z 可撤销。
 */
export function OneClickPanel({ photo }: { photo: PhotoAsset | null }) {
  const oneClick = useEditorStore((state) => state.oneClick);
  const detection = useEditorStore((state) => (photo ? state.faces[photo.id] : undefined));
  const slimReport = useEditorStore((state) => (photo ? state.slimReports[photo.id] : undefined));
  const faceStatus = useEditorStore((state) => state.faceStatus);
  const faceProgress = useEditorStore((state) => state.faceProgress);
  const beautyProgress = useEditorStore((state) => state.beautyProgress);
  const beautyError = useEditorStore((state) => state.beautyError);
  const faceError = useEditorStore((state) => state.faceError);
  const canUndo = useEditorStore(selectCanUndo);  const setOneClickMode = useEditorStore((state) => state.setOneClickMode);
  const setOneClickStrength = useEditorStore((state) => state.setOneClickStrength);
  const setOneClickStatus = useEditorStore((state) => state.setOneClickStatus);
  const setOneClickError = useEditorStore((state) => state.setOneClickError);
  const setOneClickApplied = useEditorStore((state) => state.setOneClickApplied);
  const setSlimItems = useEditorStore((state) => state.setSlimItems);
  const applyExternalParams = useEditorStore((state) => state.applyExternalParams);
  const toggleCompare = useEditorStore((state) => state.toggleCompare);
  const compareMode = useEditorStore((state) => state.compareMode);
  const undo = useEditorStore((state) => state.undo);
  const notify = useEditorStore((state) => state.notify);

  const [pendingPlan, setPendingPlan] = useState<OneClickPlan | null>(null);
  const [appliedNote, setAppliedNote] = useState<string | null>(null);

  // 进入入口即后台预热检测：没有 faces 就没有美肤，六项会全变假按钮。
  useEffect(() => {
    if (!photo) return;
    if (detection || faceStatus === "loading") return;
    void ensureFaces(photo).catch(() => undefined);
  }, [photo, detection, faceStatus]);

  const faceCount = detection?.faces.length ?? 0;
  const busy = oneClick.status === "analyzing";
  const hasReport = Boolean(slimReport?.report);
  const applied = oneClick.applied && oneClick.applied.photoId === photo?.id ? oneClick.applied : null;

  const label = useMemo(
    () => ONE_CLICK_MODES.find((item) => item.id === oneClick.mode)?.label ?? "自然",
    [oneClick.mode],
  );
  const hint = useMemo(
    () => ONE_CLICK_MODES.find((item) => item.id === oneClick.mode)?.hint ?? "",
    [oneClick.mode],
  );

  const commit = useCallback(
    (plan: OneClickPlan, mode: OneClickMode, overall: number) => {
      if (!photo) return;
      const state = useEditorStore.getState();
      const current = state.params[photo.id];
      if (!current) return;
      const next = applyOneClickPlan(current, plan);
      applyExternalParams(photo.id, next, `一键精修·${label}`);
      setOneClickApplied(toAppliedPlan(plan, photo.id, mode, overall));
      const anchors = state.slimReports[photo.id]?.anchors ?? {};
      const deltas = tonalDeltas(state.slimReports[photo.id]?.report ?? null);
      setSlimItems(itemsFromParams(next, { anchors, deltas }));
      setPendingPlan(null);
      setAppliedNote(
        plan.kept > 0
          ? `修好了，保留了 ${plan.kept} 项你的手动修改。可以对比看看，不满意就点「撤销」或「重新精修」。`
          : "修好了，可以左右对比看看。不满意可以点「重新精修」或「撤销」。",
      );
      notify("success", `已应用「${label}」档精修，⌘Z 可撤销`);
      setOneClickStatus("ready");
      setOneClickError(null);
    },
    [
      photo,
      label,
      applyExternalParams,
      setOneClickApplied,
      setSlimItems,
      setOneClickStatus,
      setOneClickError,
      notify,
    ],
  );

  const run = useCallback(
    async (force: boolean) => {
      if (!photo || busy) return;
      setPendingPlan(null);
      setAppliedNote(null);
      setOneClickError(null);
      setOneClickStatus("analyzing");
      try {
        // 1) 检测（不跑这一步，后面的美肤参数会被引擎静默跳过）
        const faces = await ensureFaces(photo);
        // 2) 逐人脸真实指标与建议
        const analysis = await ensureBeauty(photo, force);
        // 3) 整图光影 / 色调（mode 固定 standard）
        const report = await ensureGlobalReport(photo, force);
        const state = useEditorStore.getState();
        const current = state.params[photo.id];
        if (!current) return;
        const anchors = state.slimReports[photo.id]?.anchors ?? tonalAnchors(report);
        // `applied` 只对同一张照片有意义：换照片后应按「首次精修」处理（先确认）。
        const applied = state.oneClick.applied?.photoId === photo.id ? state.oneClick.applied : null;
        const plan = buildOneClickPlan({
          photoId: photo.id,
          mode: state.oneClick.mode,
          overall: state.oneClick.strength,
          beauty: analysis,
          report,
          current,
          applied,
          anchors,
          detection: faces,
        });
        if (plan.fields === 0) {
          setOneClickStatus("ready");
          setAppliedNote(
            plan.kept > 0
              ? "已经是最新效果，你的手动修改都还在。"
              : "这张照片状态不错，没有需要修的地方。",
          );
          return;
        }
        if (plan.needsConfirm) {
          setPendingPlan(plan);
          setOneClickStatus("ready");
          return;
        }
        commit(plan, state.oneClick.mode, state.oneClick.strength);
      } catch (error) {
        setOneClickStatus("error");
        setOneClickError(plainMessage(error));
      }
    },
    [photo, busy, setOneClickError, setOneClickStatus, commit],
  );

  const handleUndo = useCallback(() => {
    undo();
    // 撤销后参数已经不等于记录值，清掉记录以便下一次精修先确认而不是静默覆盖。
    setOneClickApplied(null);
    setAppliedNote("已撤销上一步。");
  }, [undo, setOneClickApplied]);

  const errorText = oneClick.error ?? beautyError ?? faceError;
  const progress = beautyProgress ?? faceProgress;

  if (!photo) {
    return (
      <aside className="panel slim-panel">
        <div className="slim-head">
          <h2>一键精修</h2>
          <p className="slim-sub">照片只在这台电脑上处理，不会上传</p>
        </div>
        <div className="panel-empty">先拖一张照片进来，或者点上面的「导入」。</div>
      </aside>
    );
  }

  const noFace = detection !== undefined && faceCount === 0;

  return (
    <aside className="panel slim-panel">
      <div className="slim-head">
        <h2>一键精修</h2>
        <p className="slim-sub">照片只在这台电脑上处理，不会上传</p>
      </div>

      <section className="slim-section">
        <div className="section-title">力度</div>
        <div className="segmented slim-modes">
          {ONE_CLICK_MODES.map((item) => (
            <button
              key={item.id}
              type="button"
              className={oneClick.mode === item.id ? "active" : ""}
              title={item.hint}
              disabled={busy}
              onClick={() => setOneClickMode(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <p className="hint-text">{hint}</p>
      </section>

      <section className="slim-section">
        <div className="section-title">
          整体强度
          <span className="section-meta">{Math.round(oneClick.strength)}</span>
        </div>
        <Slider
          label="整体强度"
          value={oneClick.strength}
          min={0}
          max={100}
          bipolar={false}
          defaultValue={100}
          disabled={busy}
          onChange={setOneClickStrength}
        />
        <p className="hint-text">往左是更轻，往右是更重。0 等于不开精修。</p>
      </section>

      {busy ? (
        <section className="slim-section slim-progress">
          <div className="slim-progress-text">
            {faceStatus === "loading" || !detection
              ? "正在找照片里的人脸…"
              : "正在看这张照片需要修哪里… 一般几秒钟"}
          </div>
          {progress ? (
            <div className="slim-bar">
              <div
                className="slim-bar-fill"
                style={{ width: `${Math.round(Math.max(0, Math.min(1, progress.progress)) * 100)}%` }}
              />
            </div>
          ) : (
            <div className="slim-bar is-indeterminate">
              <div className="slim-bar-fill" />
            </div>
          )}
        </section>
      ) : null}

      {noFace ? (
        <div className="slim-notice">
          这张照片里没找到人脸，一键精修只会调整光影和色调。换一张正面、清楚的照片可以修皮肤。
        </div>
      ) : null}

      {errorText && !busy ? (
        <div className="slim-error">
          <AlertTriangle size={13} />
          <span>刚才没成功：{plainMessage(errorText)}。点「重试」再来一次。</span>
        </div>
      ) : null}

      {pendingPlan ? (
        <section className="slim-section slim-confirm">
          <p>
            重新精修会覆盖你已有的调整（{pendingPlan.conflicts} 处），继续吗？想保住的话先去「手动调整」里看一眼。
          </p>
          <div className="slim-actions">
            <button
              type="button"
              className="btn btn-accent"
              onClick={() => commit(pendingPlan, oneClick.mode, oneClick.strength)}
            >
              继续覆盖
            </button>
            <button type="button" className="btn" onClick={() => setPendingPlan(null)}>
              取消
            </button>
          </div>
        </section>
      ) : null}

      {appliedNote ? <div className="slim-note">{appliedNote}</div> : null}

      <div className="slim-actions">
        <button
          type="button"
          className="btn btn-accent slim-primary"
          disabled={busy}
          onClick={() => void run(false)}
        >
          <Sparkles size={15} />
          {busy ? "正在修…" : applied ? "应用当前设置" : "一键精修"}
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => void run(true)}
          title="重新读一遍照片，按新的设置再修一次"
        >
          <RefreshCw size={14} />
          重新精修
        </button>
      </div>

      <div className="slim-actions">
        <button
          type="button"
          className={`btn ${compareMode === "split" ? "btn-active" : ""}`}
          onClick={toggleCompare}
          title="左右分屏看原图与效果"
        >
          <Columns2 size={14} />
          对比
        </button>
        <button type="button" className="btn" disabled={!canUndo} onClick={handleUndo}>
          <Undo2 size={14} />
          撤销
        </button>
      </div>

      <div className="slim-foot">
        <p className="hint-text">
          应用到全部 {faceCount > 0 ? `${faceCount} 张` : ""}人脸与整张照片；想单独调某一项，切到「手动调整」。
        </p>
        {hasReport ? (
          <p className="hint-text">已经看过这张照片的光影与色调，换档位会立刻按新力度重算。</p>
        ) : null}
      </div>
    </aside>
  );
}
