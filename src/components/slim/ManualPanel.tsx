import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle } from "lucide-react";
import type { PhotoAsset } from "../../types";
import {
  DEFAULT_ITEM_STRENGTH,
  selectActiveParams,
  useEditorStore,
} from "../../state/editorStore";
import {
  SIX_ITEM_ORDER,
  directionHint,
  invertItemStrength,
  itemNeedsAnalysis,
  needsGlobalAnalysis,
  tonalDeltas,
  type SixItemId,
} from "../../state/slim";
import { AdvancedSettings } from "./AdvancedSettings";
import { SixItemRow } from "./SixItemRow";
import { ensureFaces, ensureGlobalReport, plainMessage } from "./analysis";

/** 手动入口顶部的状态提示。**纯 props 驱动**，不读 store，便于直接渲染断言。 */
export interface ManualNoticeProps {
  faceStatus: "idle" | "loading" | "ready" | "error";
  hasDetection: boolean;
  noFace: boolean;
  faceError: string | null;
  faceProgress: { progress: number } | null;
  faceCount: number;
  reportStatus: "idle" | "loading" | "ready" | "error";
  reportError: string | null;
  reportReady: boolean;
  retrying: boolean;
  onRetry: () => void;
  /** 人脸检测失败时的重试（与整图分析失败是两条独立路径）。 */
  onRetryFace: () => void;
}

export function ManualNotice({
  faceStatus,
  hasDetection,
  noFace,
  faceError,
  faceProgress,
  faceCount,
  reportStatus,
  reportError,
  reportReady,
  retrying,
  onRetry,
  onRetryFace,
}: ManualNoticeProps) {
  if (faceStatus === "loading" && !hasDetection) {
    return (
      <div className="slim-notice">
        <span className="slim-dot" />
        正在准备，几秒后就能调
        {faceProgress ? ` · ${Math.round(faceProgress.progress * 100)}%` : ""}
      </div>
    );
  }
  if (noFace) {
    return (
      <div className="slim-notice is-warning">
        这张照片里没找到人脸，手动调整用不了。换一张正面、清楚的照片试试。
      </div>
    );
  }
  if (faceError) {
    return (
      <div className="slim-error">
        <AlertTriangle size={13} />
        <span>刚才没成功：{plainMessage(faceError)}。点「重试」再来一次。</span>
        <button type="button" className="mini-btn" onClick={onRetryFace}>
          重试
        </button>
      </div>
    );
  }
  if (reportStatus === "loading" || retrying) {
    return (
      <div className="slim-notice">
        <span className="slim-dot" />
        正在看这张照片的光影和色调… 一般几秒钟
      </div>
    );
  }
  if (reportError) {
    return (
      <div className="slim-error">
        <AlertTriangle size={13} />
        <span>
          「光影调整」「照片色调」这次没看成：{plainMessage(reportError)}。
          其余四项不受影响，可以照常调整。
        </span>
        <button type="button" className="mini-btn" onClick={onRetry}>
          重试
        </button>
      </div>
    );
  }
  if (!reportReady && hasDetection) {
    return (
      <p className="hint-text slim-tip">
        拖动「光影调整」或「照片色调」时会先看一遍这张照片，方向由分析决定。
      </p>
    );
  }
  if (faceCount >= 2) {
    return <div className="slim-notice">这张照片有 {faceCount} 张人脸，调整会同时应用到全部。</div>;
  }
  return null;
}

/**
 * 手动调整面板：只六项，每项「开关 + 0-100 强度 + 单项重置」，高级设置折叠。
 *
 * 滑块值一律从 `params` 反解（`invertItemStrength`），不另存一份影子状态，
 * 否则在两个入口之间切换一次就丢编辑。
 */
export function ManualPanel({ photo }: { photo: PhotoAsset | null }) {
  const params = useEditorStore(selectActiveParams);
  const detection = useEditorStore((state) => (photo ? state.faces[photo.id] : undefined));
  const slimReport = useEditorStore((state) => (photo ? state.slimReports[photo.id] : undefined));
  const faceStatus = useEditorStore((state) => state.faceStatus);
  const faceError = useEditorStore((state) => state.faceError);
  const faceProgress = useEditorStore((state) => state.faceProgress);
  const reportStatus = useEditorStore(
    (state) => (photo ? state.slimReports[photo.id]?.status : "idle") ?? "idle",
  );
  const remembered = useEditorStore((state) => state.slimItems);
  const applySlimItem = useEditorStore((state) => state.applySlimItem);
  const setSlimItemEnabled = useEditorStore((state) => state.setSlimItemEnabled);
  const resetSlimItem = useEditorStore((state) => state.resetSlimItem);
  const notify = useEditorStore((state) => state.notify);

  const [analyzing, setAnalyzing] = useState<SixItemId | null>(null);
  const [retrying, setRetrying] = useState(false);

  // 预热检测：`pipeline.rs` 在 faces 为空时会静默跳过美肤，没这一步六项全是假按钮。
  useEffect(() => {
    if (!photo) return;
    if (detection || faceStatus === "loading") return;
    void ensureFaces(photo).catch(() => undefined);
  }, [photo, detection, faceStatus]);

  const anchors = slimReport?.anchors ?? {};
  const deltas = useMemo(() => tonalDeltas(slimReport?.report ?? null), [slimReport]);
  /** 只有真实拿到分析结果才算「已分析」；分析中/失败时 report 为 null（D1）。 */
  const reportReady = Boolean(slimReport?.report);
  const reportError = slimReport?.status === "error" ? slimReport.error : null;

  const inverted = useMemo(() => {
    const out = {} as Record<SixItemId, { strength: number; manual: boolean }>;
    for (const id of SIX_ITEM_ORDER) {
      out[id] = invertItemStrength(params, id, { anchors, deltas });
    }
    return out;
  }, [params, anchors, deltas]);

  const faceCount = detection?.faces.length ?? 0;
  const noFace = detection !== undefined && faceCount === 0;

  /**
   * 真实跑一次整图分析，返回是否成功。
   * `force` 用于「重试」与「上次失败」：不吃缓存，重新读一遍照片。
   */
  const runGlobalAnalysis = useCallback(
    async (force: boolean): Promise<boolean> => {
      if (!photo) return false;
      setRetrying(true);
      try {
        await ensureGlobalReport(photo, force);
        return true;
      } catch (error) {
        notify("error", `刚才没成功：${plainMessage(error)}。点「重试」再来一次。`);
        return false;
      } finally {
        setRetrying(false);
      }
    },
    [photo, notify],
  );

  const handleChange = useCallback(
    async (id: SixItemId, value: number) => {
      if (!photo) return;
      if (!needsGlobalAnalysis(id, slimReport ?? null)) {
        applySlimItem(id, value);
        return;
      }
      // ⑤⑥：还没有分析结论时（含上次失败），这一次拖动就真的去分析。
      // 失败时**不写入**，避免产生「滑块动了但参数没动」的假写入。
      setAnalyzing(id);
      try {
        const ok = await runGlobalAnalysis(slimReport?.status === "error");
        if (ok) useEditorStore.getState().applySlimItem(id, value);
      } finally {
        setAnalyzing(null);
      }
    },
    [photo, reportReady, slimReport, applySlimItem, runGlobalAnalysis],
  );

  const handleRetry = useCallback(async () => {
    await runGlobalAnalysis(true);
  }, [runGlobalAnalysis]);

  // 检测失败同样是死路，必须给一个真实的重试入口（D1 同类问题）。
  const handleRetryFace = useCallback(async () => {
    if (!photo) return;
    try {
      await ensureFaces(photo);
    } catch (error) {
      notify("error", `刚才没成功：${plainMessage(error)}。点「重试」再来一次。`);
    }
  }, [photo, notify]);

  if (!photo) {
    return (
      <aside className="panel slim-panel">
        <div className="slim-head">
          <h2>手动调整</h2>
          <p className="slim-sub">照片只在这台电脑上处理，不会上传</p>
        </div>
        <div className="panel-empty">先拖一张照片进来，或者点上面的「导入」。</div>
      </aside>
    );
  }

  return (
    <aside className="panel slim-panel">
      <div className="slim-head">
        <h2>手动调整</h2>
        <p className="slim-sub">六个开关，调到你觉得自然就行</p>
      </div>

      <ManualNotice
        faceStatus={faceStatus}
        hasDetection={Boolean(detection)}
        noFace={noFace}
        faceError={faceError}
        faceProgress={faceProgress ?? null}
        faceCount={faceCount}
        reportStatus={reportStatus}
        reportError={reportError}
        reportReady={reportReady}
        retrying={retrying}
        onRetry={() => void handleRetry()}
        onRetryFace={() => void handleRetryFace()}
      />

      <div className="six-list">
        {SIX_ITEM_ORDER.map((id) => {
          const state = inverted[id];
          const needsAnalysis = itemNeedsAnalysis(id);
          const unavailable = noFace || !detection;
          // ⑤⑥ 在分析还没成功时，开关也必须走「先分析再写」，否则又是一个假开关。
          const waiting = analyzing === id || (needsAnalysis && retrying);
          return (
            <SixItemRow
              key={id}
              id={id}
              strength={state.strength}
              manual={state.manual}
              disabled={unavailable || waiting}
              direction={needsAnalysis ? directionHint(id, deltas) : null}
              onToggle={(enabled) => {
                if (!enabled) {
                  setSlimItemEnabled(id, false);
                  return;
                }
                const target =
                  remembered[id].strength > 0 ? remembered[id].strength : DEFAULT_ITEM_STRENGTH;
                void handleChange(id, target);
              }}
              onChange={(value) => void handleChange(id, value)}
              onReset={() => resetSlimItem(id)}
            />
          );
        })}
      </div>

      <AdvancedSettings photo={photo} params={params} detection={detection ?? null} />
    </aside>
  );
}
