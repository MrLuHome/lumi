import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle } from "lucide-react";
import type { PhotoAsset } from "../../types";
import { selectActiveParams, useEditorStore } from "../../state/editorStore";
import {
  SIX_ITEM_ORDER,
  directionHint,
  invertItemStrength,
  itemNeedsAnalysis,
  tonalDeltas,
  type SixItemId,
} from "../../state/slim";
import { AdvancedSettings } from "./AdvancedSettings";
import { SixItemRow } from "./SixItemRow";
import { ensureFaces, ensureGlobalReport, plainMessage } from "./analysis";

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
  const reportStatus = useEditorStore((state) => (photo ? state.slimReports[photo.id]?.status : "idle"));
  const applySlimItem = useEditorStore((state) => state.applySlimItem);
  const setSlimItemEnabled = useEditorStore((state) => state.setSlimItemEnabled);
  const resetSlimItem = useEditorStore((state) => state.resetSlimItem);
  const notify = useEditorStore((state) => state.notify);

  const [analyzing, setAnalyzing] = useState<SixItemId | null>(null);

  // 预热检测：`pipeline.rs` 在 faces 为空时会静默跳过美肤，没这一步六项全是假按钮。
  useEffect(() => {
    if (!photo) return;
    if (detection || faceStatus === "loading") return;
    void ensureFaces(photo).catch(() => undefined);
  }, [photo, detection, faceStatus]);

  const anchors = slimReport?.anchors ?? {};
  const deltas = useMemo(() => tonalDeltas(slimReport?.report ?? null), [slimReport]);

  const inverted = useMemo(() => {
    const out = {} as Record<SixItemId, { strength: number; manual: boolean }>;
    for (const id of SIX_ITEM_ORDER) {
      out[id] = invertItemStrength(params, id, { anchors, deltas });
    }
    return out;
  }, [params, anchors, deltas]);

  const faceCount = detection?.faces.length ?? 0;
  const noFace = detection !== undefined && faceCount === 0;

  const handleChange = useCallback(
    async (id: SixItemId, value: number) => {
      if (!photo) return;
      if (!itemNeedsAnalysis(id) || slimReport?.report) {
        applySlimItem(id, value);
        return;
      }
      // ⑤⑥：还没有分析结论时，第一次拖动就真的去分析（不是假装有方向）。
      setAnalyzing(id);
      try {
        await ensureGlobalReport(photo);
        useEditorStore.getState().applySlimItem(id, value);
      } catch (error) {
        notify("error", `刚才没成功：${plainMessage(error)}。点一下再试。`);
      } finally {
        setAnalyzing(null);
      }
    },
    [photo, slimReport, applySlimItem, notify],
  );

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

      {faceStatus === "loading" && !detection ? (
        <div className="slim-notice">
          <span className="slim-dot" />
          正在准备，几秒后就能调
          {faceProgress ? ` · ${Math.round(faceProgress.progress * 100)}%` : ""}
        </div>
      ) : noFace ? (
        <div className="slim-notice is-warning">
          这张照片里没找到人脸，手动调整用不了。换一张正面、清楚的照片试试。
        </div>
      ) : faceError ? (
        <div className="slim-error">
          <AlertTriangle size={13} />
          <span>刚才没成功：{plainMessage(faceError)}。点一下再试。</span>
        </div>
      ) : faceCount >= 2 ? (
        <div className="slim-notice">
          这张照片有 {faceCount} 张人脸，调整会同时应用到全部。
        </div>
      ) : null}

      {reportStatus === "loading" ? (
        <div className="slim-notice">正在看这张照片的光影和色调… 一般几秒钟</div>
      ) : null}

      <div className="six-list">
        {SIX_ITEM_ORDER.map((id) => {
          const state = inverted[id];
          const needsAnalysis = itemNeedsAnalysis(id);
          const unavailable = noFace || !detection;
          return (
            <SixItemRow
              key={id}
              id={id}
              strength={state.strength}
              manual={state.manual}
              disabled={unavailable || analyzing === id}
              direction={needsAnalysis ? directionHint(id, deltas) : null}
              onToggle={(enabled) => setSlimItemEnabled(id, enabled)}
              onChange={(value) => void handleChange(id, value)}
              onReset={() => resetSlimItem(id)}
            />
          );
        })}
      </div>

      <p className="hint-text slim-tip">
        拖动「光影调整」「照片色调」时会先看一遍这张照片，方向由分析决定，不用自己猜。
      </p>

      <AdvancedSettings photo={photo} params={params} detection={detection ?? null} />
    </aside>
  );
}
