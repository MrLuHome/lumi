/**
 * 双入口精简版的数据层：六项映射、三档系数、一键计划与「不静默覆盖」判定。
 *
 * 这里的函数全部是**纯函数**（不读 store、不发请求、不改入参），方便单测锁定数值契约。
 * 六项 ↔ 参数映射（任务书 §4；改这里之前先读 docs/SLIMMING_AUDIT_AND_DESIGN.md）：
 *
 *   ① 磨皮美肤 = skinSmooth + texture + detail
 *   ② 祛痘祛斑 = blemish + spots(必须 ≤ blemish) + blackheads
 *   ③ 眼睛美化 = darkCircles + eyeLines            （绝不碰 eyeShape / reshape：大眼暂缓）
 *   ④ 肤色提亮 = whitening + toneEven + dullness + redness
 *   ⑤ 光影调整 = exposure + contrast + highlights + shadows
 *   ⑥ 照片色调 = temperature + tint + saturation + vibrance
 *
 * 两条硬约束：
 * 1. `blemish.rs:45` 取 `amount = max(blemish, spots)`，所以映射层必须保证 `spots ≤ blemish`，
 *    否则滑块会「越拉越不单调」。
 * 2. 额头细纹 / 法令纹不进入任何一项（主理人裁决 Q1）：一键计划不写这两个字段，
 *    老项目遗留值交给「高级设置 → 旧调整清理」暴露并一键清除。
 */

import type {
  AdjustmentKey,
  AnalysisReport,
  BeautyAnalysis,
  BeautyKey,
  BeautyParams,
  EditParams,
  FaceDetectionResult,
} from "../types";
import {
  BEAUTY_KEYS,
  backgroundIsActive,
  beautyIsActive,
  clampParams,
  defaultBeauty,
  faceReshapeIsActive,
  bodyAdjustIsActive,
  subjectMaskHasContent,
} from "./params";

// ────────────────────────────── 基础类型 ──────────────────────────────

/** 手动调整的六项。顺序即 UI 顺序。 */
export type SixItemId = "skin" | "blemish" | "eyes" | "tone" | "light" | "color";

export const SIX_ITEM_ORDER: SixItemId[] = ["skin", "blemish", "eyes", "tone", "light", "color"];

/** 一键精修的三档。`light`（轻度）档在本 UI 不再出现，但后端枚举保留。 */
export type OneClickMode = "natural" | "refined" | "advanced";

export const ONE_CLICK_MODES: { id: OneClickMode; label: string; hint: string }[] = [
  { id: "natural", label: "自然", hint: "只做轻微修正，优先保住原来的质感" },
  { id: "refined", label: "精致", hint: "AI 建议的七到八成，日常出片够用" },
  { id: "advanced", label: "高级", hint: "用满 AI 的建议量，力度更大" },
];

export const DEFAULT_ONE_CLICK_MODE: OneClickMode = "natural";

/** 单张照片的滑块状态；`strength` 是「记住的强度」，关掉开关时不清零，方便再打开。 */
export interface SixItemState {
  enabled: boolean;
  strength: number;
}

export function defaultSixItems(): Record<SixItemId, SixItemState> {
  return {
    skin: { enabled: false, strength: 0 },
    blemish: { enabled: false, strength: 0 },
    eyes: { enabled: false, strength: 0 },
    tone: { enabled: false, strength: 0 },
    light: { enabled: false, strength: 0 },
    color: { enabled: false, strength: 0 },
  };
}

/** 一键精修的界面状态。`applied` 只活在内存里，不进项目文件（不改 `.lumi` 结构）。 */
export type OneClickStatus = "idle" | "analyzing" | "applying" | "ready" | "error";

export interface AppliedPlan {
  photoId: string;
  mode: OneClickMode;
  itemFactor: Record<SixItemId, number>;
  overall: number;
  /** key = 人脸 UUID（没有 UUID 时用 `id:<faceId>`），value = 字段 → 当时写入的值。 */
  faceWrites: Record<string, Partial<Record<BeautyKey, number>>>;
  paramWrites: Partial<Record<AdjustmentKey, number>>;
  at: number;
}

export interface OneClickState {
  mode: OneClickMode;
  strength: number;
  status: OneClickStatus;
  error: string | null;
  applied: AppliedPlan | null;
}

export function defaultOneClick(): OneClickState {
  return {
    mode: DEFAULT_ONE_CLICK_MODE,
    strength: 100,
    status: "idle",
    error: null,
    applied: null,
  };
}

// ────────────────────────────── 六项元数据 ──────────────────────────────

export interface SixItemMeta {
  id: SixItemId;
  /** 开关名（§6.5）。 */
  label: string;
  /** 滑块旁的白话说明（§6.5）。 */
  hint: string;
  /** 该组写入的人脸字段。 */
  beauty: BeautyKey[];
  /** 该组写入的全局字段（只有 ⑤⑥ 有）。 */
  adjustment: AdjustmentKey[];
  /** 是否需要先做整图分析才知道方向（⑤⑥）。 */
  needsAnalysis: boolean;
}

export const ITEM_META: Record<SixItemId, SixItemMeta> = {
  skin: {
    id: "skin",
    label: "磨皮美肤",
    hint: "让皮肤更细腻，同时尽量留住毛孔",
    beauty: ["skinSmooth", "texture", "detail"],
    adjustment: [],
    needsAnalysis: false,
  },
  blemish: {
    id: "blemish",
    label: "祛痘祛斑",
    hint: "淡掉小痘印和小斑点，痣会保留",
    beauty: ["blemish", "spots", "blackheads"],
    adjustment: [],
    needsAnalysis: false,
  },
  eyes: {
    id: "eyes",
    label: "眼睛美化",
    hint: "淡一点黑眼圈和眼下细纹",
    beauty: ["darkCircles", "eyeLines"],
    adjustment: [],
    needsAnalysis: false,
  },
  tone: {
    id: "tone",
    label: "肤色提亮",
    hint: "让肤色更均匀、更透亮",
    beauty: ["whitening", "toneEven", "dullness", "redness"],
    adjustment: [],
    needsAnalysis: false,
  },
  light: {
    id: "light",
    label: "光影调整",
    hint: "自动把偏暗/偏亮的地方拉回来",
    beauty: [],
    adjustment: ["exposure", "contrast", "highlights", "shadows"],
    needsAnalysis: true,
  },
  color: {
    id: "color",
    label: "照片色调",
    hint: "自动纠正偏黄、偏蓝的颜色",
    beauty: [],
    adjustment: ["temperature", "tint", "saturation", "vibrance"],
    needsAnalysis: true,
  },
};

/**
 * 强度 100 时的剖面 = 引擎自身认可的安全上限（§4.1）。
 * 任意强度 S 的取值 = 剖面 × S / 100（线性），所以 S=0 一定回到全 0，
 * 参数才能回到中性、预览才能字节级复用原图。
 */
export const ITEM_PROFILE: Record<SixItemId, Partial<Record<BeautyKey, number>>> = {
  skin: { skinSmooth: 70, texture: 55, detail: 28 },
  blemish: { blemish: 90, spots: 60, blackheads: 35 },
  eyes: { darkCircles: 90, eyeLines: 70 },
  tone: { whitening: 50, toneEven: 65, dullness: 45, redness: 40 },
  light: {},
  color: {},
};

/** 写入前的安全上限（§4.3）：保证 UI 显示的值就是引擎真正用的值，避免二义。 */
export const BEAUTY_CAP: Partial<Record<BeautyKey, number>> = {
  skinSmooth: 85,
  texture: 80,
  detail: 45,
  blemish: 95,
  spots: 70,
  blackheads: 45,
  darkCircles: 95,
  eyeLines: 70,
  whitening: 60,
  toneEven: 75,
  dullness: 55,
  redness: 50,
};

export const ADJUST_CAP: Partial<Record<AdjustmentKey, [number, number]>> = {
  exposure: [-30, 30],
  contrast: [-30, 30],
  highlights: [-45, 10],
  shadows: [-10, 45],
  temperature: [-25, 25],
  tint: [-15, 15],
  saturation: [-30, 30],
  vibrance: [-25, 25],
};

/** 三档系数（§5.2）：含义是「把 AI 建议做到百分之多少」。高级档也不超调。 */
export const MODE_FACTOR: Record<OneClickMode, Record<SixItemId, number>> = {
  natural: { skin: 0.55, blemish: 0.5, eyes: 0.4, tone: 0.55, light: 0.6, color: 0.5 },
  refined: { skin: 0.8, blemish: 0.75, eyes: 0.7, tone: 0.8, light: 0.85, color: 0.75 },
  advanced: { skin: 1, blemish: 1, eyes: 1, tone: 1, light: 1, color: 1 },
};

/** 覆盖判定的容差（§5.5）。 */
export const WRITE_TOLERANCE = 0.5;

// ────────────────────────────── 小工具 ──────────────────────────────

function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

/** 保留一位小数：既避免 0.30000000000000004 这类噪声，也不丢滑块分辨率。 */
function round1(value: number): number {
  return Math.round(value * 10) / 10;
}

function capBeauty(key: BeautyKey, value: number): number {
  return round1(clamp(value, 0, BEAUTY_CAP[key] ?? 100));
}

function capAdjust(key: AdjustmentKey, value: number): number {
  const range = ADJUST_CAP[key];
  if (!range) return round1(value);
  return round1(clamp(value, range[0], range[1]));
}

/** 人脸条目的稳定键：UUID 优先，旧数据回退到 faceId。 */
export function faceKey(face: { faceId: number; faceUuid: string }): string {
  return face.faceUuid ? face.faceUuid : `id:${face.faceId}`;
}

/** 只有「光影」「色调」两项需要先分析。 */
export function itemNeedsAnalysis(id: SixItemId): boolean {
  return ITEM_META[id].needsAnalysis;
}

// ────────────────────────────── 手动六项：正向写入 ──────────────────────────────

/** 单项强度 → 人脸字段写入值（①②③④）。S=0 一律返回空对象。 */
export function buildSixItemWrite(
  id: SixItemId,
  strength: number,
): Partial<Record<BeautyKey, number>> {
  const profile = ITEM_PROFILE[id];
  const ratio = clamp(strength, 0, 100) / 100;
  const write: Partial<Record<BeautyKey, number>> = {};
  for (const key of BEAUTY_KEYS) {
    const peak = profile[key];
    if (peak === undefined || peak === 0) continue;
    const value = capBeauty(key, peak * ratio);
    if (value <= 0) continue;
    write[key] = value;
  }
  // `spots` 必须 ≤ `blemish`（引擎取 max），否则滑块会不单调。
  if (write.spots !== undefined) {
    const limit = write.blemish ?? 0;
    if (write.spots > limit) {
      if (limit > 0) write.spots = limit;
      else delete write.spots;
    }
  }
  return write;
}

/**
 * ⑤⑥ 的写入：`值 = 锚点 + 建议增量 × S/100`。
 * 方向由 AI 分析决定，滑块只缩放幅度（主理人裁决 Q4）。
 */
export function buildAdjustWrite(
  id: SixItemId,
  strength: number,
  anchors: Partial<Record<AdjustmentKey, number>>,
  deltas: Partial<Record<AdjustmentKey, number>>,
): Partial<Record<AdjustmentKey, number>> {
  const meta = ITEM_META[id];
  const ratio = clamp(strength, 0, 100) / 100;
  const write: Partial<Record<AdjustmentKey, number>> = {};
  for (const key of meta.adjustment) {
    const anchor = anchors[key] ?? 0;
    const delta = deltas[key];
    if (delta === undefined || Math.abs(delta) < 0.5) continue;
    write[key] = capAdjust(key, anchor + delta * ratio);
  }
  return write;
}

/** 六项之一的完整写入（含 ⑤⑥ 的锚点语义）。 */
export function writeSixItem(
  params: EditParams,
  detection: FaceDetectionResult | null,
  id: SixItemId,
  strength: number,
  ctx: {
    anchors?: Partial<Record<AdjustmentKey, number>>;
    deltas?: Partial<Record<AdjustmentKey, number>>;
  } = {},
): EditParams {
  const meta = ITEM_META[id];
  if (meta.adjustment.length > 0) {
    const write = buildAdjustWrite(id, strength, ctx.anchors ?? {}, ctx.deltas ?? {});
    if (Object.keys(write).length === 0) return params;
    return { ...params, ...write };
  }

  const write = buildSixItemWrite(id, strength);
  const next: EditParams = { ...params, faces: params.faces.map((face) => ({ ...face })) };
  // ① 接管磨皮：旧的全图 `skinSmooth` 与新参数同源（`smoothing.rs:20` 取 max），
  // 这里一起归位，避免「滑块是 0、皮肤却还在磨」的错位。
  if (id === "skin") next.skinSmooth = 0;

  const emitEmpty = Object.keys(write).length === 0;
  if (emitEmpty) {
    // 强度 0：只把已有条目归零，不新建条目（否则会凭空多出一条全 0 的人脸记录）。
    next.faces = next.faces.map((face) => {
      const beauty: BeautyParams = { ...face.beauty };
      for (const key of meta.beauty) beauty[key] = 0;
      return { ...face, beauty };
    });
    return next;
  }

  const known = new Set(next.faces.map((face) => faceKey(face)));
  for (const face of next.faces) {
    const beauty: BeautyParams = { ...face.beauty };
    for (const key of meta.beauty) {
      const value = write[key];
      if (value !== undefined) beauty[key] = value;
    }
    face.beauty = beauty;
  }
  // 已检测到、但还没有参数条目的人脸：补齐条目，保证写入对所有脸生效（§4.4）。
  for (const face of detection?.faces ?? []) {
    const key = face.uuid ? face.uuid : `id:${face.id}`;
    if (known.has(key)) continue;
    known.add(key);
    next.faces.push({
      faceId: face.id,
      faceUuid: face.uuid ?? "",
      enabled: true,
      beauty: { ...defaultBeauty(), ...write },
    });
  }
  next.faces.sort((a, b) => a.faceId - b.faceId);
  return next;
}

// ────────────────────────────── 手动六项：反解强度 ──────────────────────────────

export interface ItemStrength {
  /** 0..100，四舍五入到整数。 */
  strength: number;
  /** true 表示当前参数不是「某一个强度的一次写入」，例如老项目留下的手工值。 */
  manual: boolean;
}

/**
 * 最小二乘投影：求最接近当前参数的强度 r（0..1）。
 * `actual / peak` 允许为负（例如 ⑤⑥ 里的 highlights 建议是负向、shadows 是正向），
 * 所以直接用带符号的乘积求和，而不是先取绝对值。
 */
function projectStrength(pairs: { actual: number; peak: number }[]): { ratio: number; manual: boolean } {
  let numerator = 0;
  let denominator = 0;
  for (const { actual, peak } of pairs) {
    if (Math.abs(peak) < 0.5) continue;
    numerator += actual * peak;
    denominator += peak * peak;
  }
  if (denominator <= 0) return { ratio: 0, manual: false };
  const ratio = clamp(numerator / denominator, 0, 1);
  let manual = false;
  for (const { actual, peak } of pairs) {
    if (Math.abs(peak) < 0.5) continue;
    if (Math.abs(actual - peak * ratio) > 1) manual = true;
  }
  return { ratio, manual };
}

/**
 * 从当前参数反解某一项的强度。
 *
 * 要点：两个入口**不能各存一份滑块状态**，否则用户切换一次入口就丢编辑。
 * 反解用「最小二乘投影」而不是取平均值：老项目里只有 `skinSmooth=70`、`texture=0`
 * 时也能落到一个最接近的强度，同时标记 `manual` 让 UI 显示「微调过」。
 */
export function invertItemStrength(
  params: EditParams,
  id: SixItemId,
  ctx: {
    anchors?: Partial<Record<AdjustmentKey, number>>;
    deltas?: Partial<Record<AdjustmentKey, number>>;
  } = {},
): ItemStrength {
  const meta = ITEM_META[id];
  if (meta.adjustment.length > 0) {
    const pairs: { actual: number; peak: number }[] = [];
    const anchors = ctx.anchors ?? {};
    const deltas = ctx.deltas ?? {};
    for (const key of meta.adjustment) {
      const delta = deltas[key];
      if (delta === undefined || Math.abs(delta) < 0.5) continue;
      pairs.push({ actual: params[key] - (anchors[key] ?? 0), peak: delta });
    }
    if (pairs.length === 0) {
      const touched = meta.adjustment.some(
        (key) => Math.abs(params[key] - (anchors[key] ?? 0)) >= WRITE_TOLERANCE,
      );
      return { strength: 0, manual: touched };
    }
    // 同一项里正负建议混在一起（例如 highlights 负、shadows 正）也要能反解：
    // 直接对「带符号的增量」做最小二乘投影。
    const { ratio, manual } = projectStrength(pairs);
    return { strength: Math.round(clamp(ratio, 0, 1) * 100), manual };
  }

  // ①②③④：人脸字段。所有人脸共用同一个强度，取「最偏离」的那张脸的结论。
  const faces = params.faces.length > 0 ? params.faces : [];
  if (faces.length === 0) {
    const legacy = id === "skin" ? params.skinSmooth : 0;
    const peak = ITEM_PROFILE.skin.skinSmooth ?? 0;
    if (legacy > 0 && peak > 0) {
      return { strength: Math.round(clamp(legacy / peak, 0, 1) * 100), manual: true };
    }
    return { strength: 0, manual: false };
  }

  let best: ItemStrength = { strength: 0, manual: false };
  for (const face of faces) {
    const pairs: { actual: number; peak: number }[] = [];
    for (const key of meta.beauty) {
      const peak = ITEM_PROFILE[id][key];
      if (peak === undefined || peak <= 0) continue;
      const actual =
        key === "skinSmooth" ? Math.max(face.beauty.skinSmooth, params.skinSmooth) : face.beauty[key];
      pairs.push({ actual, peak });
    }
    const { ratio, manual } = projectStrength(pairs);
    const strength = Math.round(clamp(ratio, 0, 1) * 100);
    if (strength > best.strength || (strength === best.strength && manual && !best.manual)) {
      best = { strength, manual };
    } else if (manual && !best.manual) {
      best = { strength: best.strength, manual: true };
    }
  }
  return best;
}

// ────────────────────────────── ⑤⑥ 的分析结果 ──────────────────────────────

const TONAL_KEYS: AdjustmentKey[] = [
  "exposure",
  "contrast",
  "highlights",
  "shadows",
  "temperature",
  "tint",
  "saturation",
  "vibrance",
];

/** 从整图分析报告里取出建议增量（`suggested - current`）。 */
export function tonalDeltas(report: AnalysisReport | null): Partial<Record<AdjustmentKey, number>> {
  const out: Partial<Record<AdjustmentKey, number>> = {};
  if (!report) return out;
  for (const item of report.suggestions) {
    if (item.target !== "param") continue;
    const key = item.field as AdjustmentKey;
    if (!TONAL_KEYS.includes(key)) continue;
    const delta = item.suggested - item.current;
    if (Math.abs(delta) < 0.5) continue;
    out[key] = delta;
  }
  return out;
}

/** 从整图分析报告里取出锚点（分析当时的参数值）。 */
export function tonalAnchors(report: AnalysisReport | null): Partial<Record<AdjustmentKey, number>> {
  const out: Partial<Record<AdjustmentKey, number>> = {};
  if (!report) return out;
  for (const item of report.suggestions) {
    if (item.target !== "param") continue;
    const key = item.field as AdjustmentKey;
    if (!TONAL_KEYS.includes(key)) continue;
    if (out[key] === undefined) out[key] = item.current;
  }
  return out;
}

/** 分析时把 ⑤⑥ 恢复到锚点，避免「已经采纳的值」把下一次分析的建议吞掉。 */
export function withAnchors(
  params: EditParams,
  anchors: Partial<Record<AdjustmentKey, number>>,
): EditParams {
  const next: EditParams = { ...params };
  for (const key of TONAL_KEYS) {
    const anchor = anchors[key];
    if (anchor !== undefined) next[key] = anchor;
  }
  return next;
}

/** 只覆盖六项里会出现的 8 个字段；其它字段没有建议，也就没有方向说明。 */
const DIRECTION_TEXT: Partial<Record<AdjustmentKey, { up: string; down: string }>> = {
  exposure: { up: "AI 判断这张照片偏暗，向右会提亮", down: "AI 判断这张照片偏亮，向右会压暗" },
  contrast: {
    up: "AI 判断明暗反差偏小，向右会更通透",
    down: "AI 判断明暗反差偏大，向右会更柔和",
  },
  highlights: {
    up: "AI 判断亮部偏灰，向右会把亮部提回来",
    down: "AI 判断亮部有点过，向右会压回亮部",
  },
  shadows: { up: "AI 判断暗部太黑，向右会提回细节", down: "AI 判断暗部发灰，向右会压深一点" },
  temperature: { up: "AI 判断照片偏冷，向右会加暖", down: "AI 判断照片偏黄，向右会调冷" },
  tint: { up: "AI 判断照片偏绿，向右会补一点洋红", down: "AI 判断照片偏洋红，向右会调回自然" },
  saturation: { up: "AI 判断颜色偏淡，向右会更浓", down: "AI 判断颜色偏艳，向右会收一点" },
  vibrance: { up: "AI 判断颜色发灰，向右会更鲜活", down: "AI 判断颜色太杂，向右会压一点" },
};

/**
 * ⑤⑥ 的白话方向说明：取该项里幅度最大的那条建议，说清楚「向右会怎样」。
 * 方向不可手动反转（已知限制，见轮次报告）。
 */
export function directionHint(
  id: SixItemId,
  deltas: Partial<Record<AdjustmentKey, number>>,
): string | null {
  const meta = ITEM_META[id];
  if (meta.adjustment.length === 0) return null;
  let pick: AdjustmentKey | null = null;
  let magnitude = 0;
  for (const key of meta.adjustment) {
    const delta = deltas[key];
    if (delta === undefined) continue;
    if (Math.abs(delta) > magnitude) {
      magnitude = Math.abs(delta);
      pick = key;
    }
  }
  if (!pick) return null;
  const text = DIRECTION_TEXT[pick];
  if (!text) return null;
  const delta = deltas[pick] ?? 0;
  return delta > 0 ? text.up : text.down;
}

// ────────────────────────────── 一键精修：生成计划 ──────────────────────────────

export interface OneClickFaceWrite {
  faceId: number;
  faceUuid: string;
  beauty: Partial<Record<BeautyKey, number>>;
}

export interface OneClickPlan {
  faces: OneClickFaceWrite[];
  params: Partial<Record<AdjustmentKey, number>>;
  /** 真正会改变像素的字段数（与当前值差 ≥ 0.5）。 */
  fields: number;
  /** 因为用户手改过而保留的字段数。 */
  kept: number;
  /** 首次精修（没有 applied 记录）且目标字段当前非 0 时为 true，必须先确认。 */
  needsConfirm: boolean;
  /** 需要确认的字段个数，用于文案。 */
  conflicts: number;
}

export interface OneClickInput {
  photoId: string;
  mode: OneClickMode;
  /** 整体强度 K，0..100。 */
  overall: number;
  beauty: BeautyAnalysis | null;
  report: AnalysisReport | null;
  current: EditParams;
  applied: AppliedPlan | null;
  anchors: Partial<Record<AdjustmentKey, number>>;
  /** 当前照片的检测结果：分析结果里只有 faceId，UUID 要从检测结果补齐。 */
  detection?: FaceDetectionResult | null;
}

/** 找到某个人脸条目的当前值：UUID 优先，回退 faceId。 */
function currentFaceBeauty(
  params: EditParams,
  faceId: number,
  faceUuid: string,
): Partial<Record<BeautyKey, number>> {
  const byUuid = faceUuid ? params.faces.find((face) => face.faceUuid === faceUuid) : undefined;
  const entry = byUuid ?? params.faces.find((face) => face.faceId === faceId);
  if (!entry) return {};
  const out: Partial<Record<BeautyKey, number>> = {};
  for (const key of BEAUTY_KEYS) out[key] = entry.beauty[key];
  return out;
}

/**
 * 把「双分析结果 + 档位 + 整体强度 + 上次写入记录」合成为一次写入计划（§5.4 / §5.5）。
 *
 * 逐字段规则：
 * - 当前值 ≈ 上次写入值 → AI 拥有这个字段，正常覆盖；
 * - 当前值 ≠ 上次写入值 → 用户手改过，**保留用户值**，计入 `kept`；
 * - 没有 applied 记录 → 目标字段当前非 0 且差值 ≥5 时置 `needsConfirm`，绝不静默覆盖。
 *
 * 注意：额头细纹 / 法令纹永远不写（Q1）。
 */
export function buildOneClickPlan(input: OneClickInput): OneClickPlan {
  const factor = MODE_FACTOR[input.mode];
  const overall = clamp(input.overall, 0, 100) / 100;
  const { current, applied } = input;
  let kept = 0;
  let conflicts = 0;
  let fields = 0;

  const faces: OneClickFaceWrite[] = [];
  for (const suggestion of input.beauty?.faces ?? []) {
    const faceUuid = findFaceUuid(current, suggestion.faceId, input.detection ?? null);
    const before = currentFaceBeauty(current, suggestion.faceId, faceUuid);
    const wrote = applied?.faceWrites[faceUuid || `id:${suggestion.faceId}`] ?? null;
    const beauty: Partial<Record<BeautyKey, number>> = {};
    for (const id of SIX_ITEM_ORDER) {
      for (const key of ITEM_META[id].beauty) {
        const peak = suggestion.suggested[key];
        if (!Number.isFinite(peak) || peak <= 0) continue;
        const target = capBeauty(key, peak * factor[id] * overall);
        const existing = before[key] ?? 0;
        const owned = wrote?.[key];
        if (owned !== undefined && Math.abs(existing - owned) > WRITE_TOLERANCE) {
          // 用户手改过：保留，不算冲突。
          kept += 1;
          continue;
        }
        if (owned === undefined && applied === null && Math.abs(existing) > 0) {
          if (Math.abs(target - existing) >= 5) conflicts += 1;
        }
        if (Math.abs(target - existing) < WRITE_TOLERANCE) continue;
        beauty[key] = target;
        fields += 1;
      }
    }
    if (Object.keys(beauty).length > 0) {
      faces.push({ faceId: suggestion.faceId, faceUuid, beauty });
    }
  }

  const params: Partial<Record<AdjustmentKey, number>> = {};
  const deltas = tonalDeltas(input.report);
  const anchorSource = Object.keys(input.anchors).length > 0 ? input.anchors : tonalAnchors(input.report);
  for (const id of SIX_ITEM_ORDER) {
    for (const key of ITEM_META[id].adjustment) {
      const delta = deltas[key];
      if (delta === undefined) continue;
      const anchor = anchorSource[key] ?? current[key];
      const target = capAdjust(key, anchor + delta * factor[id] * overall);
      const existing = current[key];
      const owned = applied?.paramWrites[key];
      if (owned !== undefined && Math.abs(existing - owned) > WRITE_TOLERANCE) {
        kept += 1;
        continue;
      }
      if (owned === undefined && applied === null && Math.abs(existing) > 0) {
        if (Math.abs(target - existing) >= 5) conflicts += 1;
      }
      if (Math.abs(target - existing) < WRITE_TOLERANCE) continue;
      params[key] = target;
      fields += 1;
    }
  }

  return {
    faces,
    params,
    fields,
    kept,
    conflicts,
    needsConfirm: applied === null && conflicts > 0,
  };
}

/** 分析结果只给 faceId；UUID 依次从参数条目、检测结果补齐。 */
function findFaceUuid(
  params: EditParams,
  faceId: number,
  detection: FaceDetectionResult | null,
): string {
  const fromParams = params.faces.find((face) => face.faceId === faceId && face.faceUuid);
  if (fromParams?.faceUuid) return fromParams.faceUuid;
  const detected = detection?.faces.find((face) => face.id === faceId);
  return detected?.uuid ?? "";
}

/** 把计划写成参数：缺失的人脸条目自动创建，其它参数原样保留。 */
export function applyOneClickPlan(current: EditParams, plan: OneClickPlan): EditParams {
  const next: EditParams = { ...current, faces: current.faces.map((face) => ({ ...face })) };
  for (const write of plan.faces) {
    const index = next.faces.findIndex((face) =>
      write.faceUuid ? face.faceUuid === write.faceUuid : face.faceId === write.faceId,
    );
    if (index >= 0) {
      const entry = next.faces[index]!;
      next.faces[index] = { ...entry, beauty: { ...entry.beauty, ...write.beauty } };
      continue;
    }
    next.faces.push({
      faceId: write.faceId,
      faceUuid: write.faceUuid,
      enabled: true,
      beauty: { ...defaultBeauty(), ...write.beauty },
    });
  }
  next.faces.sort((a, b) => a.faceId - b.faceId);
  for (const key of TONAL_KEYS) {
    const value = plan.params[key];
    if (value !== undefined) next[key] = value;
  }
  return next;
}

/** 计划 → 内存里的写入记录，用于下一次「重新精修」的逐字段比对。 */
export function toAppliedPlan(
  plan: OneClickPlan,
  photoId: string,
  mode: OneClickMode,
  overall: number,
): AppliedPlan {
  const faceWrites: Record<string, Partial<Record<BeautyKey, number>>> = {};
  for (const write of plan.faces) {
    faceWrites[write.faceUuid || `id:${write.faceId}`] = { ...write.beauty };
  }
  return {
    photoId,
    mode,
    itemFactor: { ...MODE_FACTOR[mode] },
    overall,
    faceWrites,
    paramWrites: { ...plan.params },
    at: Date.now(),
  };
}

/** 一键精修写完后，把六项滑块状态同步成「实际生效的强度」，避免两处数字打架。 */
export function itemsFromParams(
  params: EditParams,
  ctx: {
    anchors?: Partial<Record<AdjustmentKey, number>>;
    deltas?: Partial<Record<AdjustmentKey, number>>;
  } = {},
): Record<SixItemId, SixItemState> {
  const out = defaultSixItems();
  for (const id of SIX_ITEM_ORDER) {
    const { strength } = invertItemStrength(params, id, ctx);
    out[id] = { enabled: strength > 0, strength };
  }
  return out;
}

// ────────────────────────────── 旧调整清理 ──────────────────────────────

export interface LegacyAdjustment {
  id: string;
  label: string;
  /** 该分组里还有几处改动在生效，用于「还有 N 项」文案。 */
  count: number;
}

function hslIsActive(hsl: EditParams["hsl"]): boolean {
  return Object.values(hsl).some(
    (channel) =>
      Math.abs(channel.hue) >= 0.001 ||
      Math.abs(channel.saturation) >= 0.001 ||
      Math.abs(channel.luminance) >= 0.001,
  );
}

function curvesIsActive(curves: EditParams["curves"]): boolean {
  return Object.values(curves).some((points) => points.length > 0);
}

function legacyFaceLines(params: EditParams): number {
  let count = 0;
  for (const face of params.faces) {
    if (face.beauty.foreheadLines > 0) count += 1;
    if (face.beauty.nasolabial > 0) count += 1;
  }
  return count;
}

/**
 * 盘点「新界面看不到、但仍在改像素」的旧调整（§7 兼容性）。
 * 这些参数仍参与渲染，所以必须给用户一个出口，否则等于关不掉。
 */
export function auditLegacyAdjustments(params: EditParams): LegacyAdjustment[] {
  const out: LegacyAdjustment[] = [];
  const reshapeCount = params.reshape.filter((entry) => faceReshapeIsActive(entry)).length;
  if (reshapeCount > 0) out.push({ id: "reshape", label: "面部塑形", count: reshapeCount });
  const bodyCount = params.body.filter((entry) => bodyAdjustIsActive(entry)).length;
  if (bodyCount > 0) out.push({ id: "body", label: "身材调整", count: bodyCount });
  if (backgroundIsActive(params.background)) {
    out.push({ id: "background", label: "背景处理", count: 1 });
  }
  if (subjectMaskHasContent(params.subjectMask)) {
    out.push({ id: "subjectMask", label: "手动人物选区", count: params.subjectMask.strokes.length });
  }
  if (params.manualMask.strokes.length > 0) {
    out.push({ id: "manualMask", label: "皮肤选区笔触", count: params.manualMask.strokes.length });
  }
  if (params.repairPoints.length > 0) {
    out.push({ id: "repairPoints", label: "点选修复", count: params.repairPoints.length });
  }
  if (hslIsActive(params.hsl)) out.push({ id: "hsl", label: "分色调整", count: 1 });
  if (curvesIsActive(params.curves)) out.push({ id: "curves", label: "曲线", count: 1 });
  if (params.lut) out.push({ id: "lut", label: "风格滤镜", count: 1 });
  if (params.skinSmooth > 0) out.push({ id: "legacySkinSmooth", label: "旧的全图磨皮", count: 1 });
  const lines = legacyFaceLines(params);
  if (lines > 0) out.push({ id: "legacyLines", label: "额头细纹 / 法令纹", count: lines });
  const detailKeys: AdjustmentKey[] = ["clarity", "sharpness", "grain", "vignette"];
  const detailCount = detailKeys.filter((key) => Math.abs(params[key]) >= 0.001).length;
  if (detailCount > 0) out.push({ id: "detail", label: "清晰度 / 锐化 / 颗粒 / 暗角", count: detailCount });
  const levelKeys: AdjustmentKey[] = ["whites", "blacks"];
  const levelCount = levelKeys.filter((key) => Math.abs(params[key]) >= 0.001).length;
  if (levelCount > 0) out.push({ id: "levels", label: "白色色阶 / 黑色色阶", count: levelCount });
  return out;
}

/** 清除全部旧调整：一次性归零，进一条撤销记录。 */
export function clearLegacyAdjustments(params: EditParams): EditParams {
  const next: EditParams = {
    ...params,
    faces: params.faces.map((face) => ({
      ...face,
      beauty: { ...face.beauty, foreheadLines: 0, nasolabial: 0 },
    })),
    reshape: [],
    body: [],
    background: { ...params.background, enabled: false, mode: "original", imagePath: "" },
    subjectMask: { ...params.subjectMask, enabled: false, strokes: [], invert: false },
    manualMask: { ...params.manualMask, enabled: false, strokes: [], invert: false },
    repairPoints: [],
    hsl: {
      red: { hue: 0, saturation: 0, luminance: 0 },
      orange: { hue: 0, saturation: 0, luminance: 0 },
      yellow: { hue: 0, saturation: 0, luminance: 0 },
      green: { hue: 0, saturation: 0, luminance: 0 },
      aqua: { hue: 0, saturation: 0, luminance: 0 },
      blue: { hue: 0, saturation: 0, luminance: 0 },
      purple: { hue: 0, saturation: 0, luminance: 0 },
      magenta: { hue: 0, saturation: 0, luminance: 0 },
    },
    curves: { rgb: [], red: [], green: [], blue: [] },
    lut: null,
    skinSmooth: 0,
    clarity: 0,
    sharpness: 0,
    grain: 0,
    vignette: 0,
    whites: 0,
    blacks: 0,
  };
  return clampParams(next);
}

/** 六项之外、仍在生效的「隐藏美肤项」：仅用于高级设置的诚实提示。 */
export function hiddenBeautyFields(params: EditParams): string[] {
  const out: string[] = [];
  const active = params.faces.some((face) => face.enabled && beautyIsActive(face.beauty));
  if (!active) return out;
  for (const face of params.faces) {
    if (face.beauty.foreheadLines > 0 || face.beauty.nasolabial > 0) {
      out.push("额头细纹 / 法令纹");
      break;
    }
  }
  return out;
}
