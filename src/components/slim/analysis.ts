import type { AnalysisReport, BeautyAnalysis, FaceDetectionResult, PhotoAsset } from "../../types";
import { analyzeBeauty, analyzeRetouch, detectFaces } from "../../lib/backend";
import { DEFAULT_PARAMS } from "../../state/params";
import { useEditorStore } from "../../state/editorStore";
import { tonalAnchors, withAnchors } from "../../state/slim";

/**
 * 精简版三个真实后端动作的统一入口。
 *
 * 关键约束（会影响「点一下像素必须变」这条验收）：`pipeline.rs:187-197` 在 `faces`
 * 为空时会**静默跳过**所有美肤，而 `faces` 只由 `detect_faces` / `analyze_beauty`
 * 写入 Rust 的人脸仓库。所以进入面板或点「一键精修」之前，必须先真的跑一次检测，
 * 否则六项全是假按钮。
 */
export async function ensureFaces(photo: PhotoAsset): Promise<FaceDetectionResult> {
  const existing = useEditorStore.getState().faces[photo.id];
  if (existing) return existing;
  useEditorStore.getState().setFaceStatus("loading");
  try {
    const result = await detectFaces(photo.path);
    // setFaces 会按稳定 UUID 把检测结果与参数条目对齐。
    useEditorStore.getState().setFaces(photo.id, result);
    return result;
  } catch (error) {
    useEditorStore.getState().setFaceError(toMessage(error));
    throw error;
  }
}

/** 逐脸真实指标（①②③④ 的建议值来源）。 */
export async function ensureBeauty(
  photo: PhotoAsset,
  force = false,
): Promise<BeautyAnalysis> {
  const cached = useEditorStore.getState().beautyAnalysis[photo.id];
  if (!force && cached) return cached;
  useEditorStore.getState().setBeautyStatus("analyzing");
  try {
    const analysis = await analyzeBeauty(photo.path);
    useEditorStore.getState().setBeautyAnalysis(photo.id, analysis);
    return analysis;
  } catch (error) {
    useEditorStore.getState().setBeautyError(toMessage(error));
    throw error;
  }
}

/**
 * 整图光影 / 色调建议（⑤⑥ 的方向与幅度来源）。
 *
 * `mode` 固定传 `standard`（factor = 1.0）：三档系数只在 TS 侧乘一次，
 * 否则自然档会变成 0.55 × 0.55 ≈ 0.30，用户会觉得「点了没反应」。
 */
export async function ensureGlobalReport(
  photo: PhotoAsset,
  force = false,
): Promise<AnalysisReport> {
  const state = useEditorStore.getState();
  const cached = state.slimReports[photo.id];
  if (!force && cached?.report) return cached.report;

  const anchors = cached?.anchors ?? {};
  const current = state.params[photo.id] ?? DEFAULT_PARAMS;
  // ⑤⑥ 先恢复到锚点再分析：已经采纳的建议不该把下一次的建议吞掉。
  const baseline = withAnchors(current, anchors);
  state.setSlimReportStatus(photo.id, "loading");
  try {
    const response = await analyzeRetouch(photo.path, baseline, "standard");
    useEditorStore
      .getState()
      .setSlimReport(photo.id, response.report, tonalAnchors(response.report), response.elapsedMs);
    return response.report;
  } catch (error) {
    useEditorStore.getState().setSlimReportStatus(photo.id, "error", toMessage(error));
    throw error;
  }
}

export function toMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "操作失败，请重试";
}

/**
 * 后端错误直接展示会把工程术语漏到界面上（模型名、Provider、蒙版、关键点…）。
 * 这里只做**词替换**，不隐藏真实原因：用户仍然看得到「哪一步没成功」。
 */
const PLAIN_WORDS: [RegExp, string][] = [
  [/语义分割|分割模型|蒙版算法|蒙版/g, "选区"],
  [/稠密关键点|关键点|稠密点/g, "脸部位置"],
  [/置信度/g, "判断把握"],
  [/ONNX Runtime|ONNX|Tensor|tract/gi, "计算组件"],
  [/\bSHA256\b/gi, "校验值"],
  [/\bLEVEL\s*[0-9]?\b/gi, "级别"],
  [/Provider/g, "组件"],
  [/模型库|模型版本|模型/g, "组件"],
  [/推理/g, "处理"],
  [/算法/g, "方法"],
  [/\bRust\b/g, "本机"],
  [/\d+\s*ms\b/g, ""],
  [/LICEN[CS]E|许可证/g, ""],
];

export function plainMessage(error: unknown): string {
  let text = toMessage(error);
  for (const [pattern, replacement] of PLAIN_WORDS) text = text.replace(pattern, replacement);
  return text.replace(/\s{2,}/g, " ").trim();
}
