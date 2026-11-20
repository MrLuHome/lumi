export interface HslChannel {
  /** 色相偏移 -100..100，映射到约 ±30°。 */
  hue: number;
  saturation: number;
  luminance: number;
}

export type HslKey = "red" | "orange" | "yellow" | "green" | "aqua" | "blue" | "purple" | "magenta";

export type HslParams = Record<HslKey, HslChannel>;

export interface CurvePoint {
  x: number;
  y: number;
}

export type CurveChannel = "rgb" | "red" | "green" | "blue";

export type CurveSet = Record<CurveChannel, CurvePoint[]>;

export interface LutSettings {
  path: string;
  name: string;
  /** 0..100，0 等同不应用。 */
  strength: number;
}

export type ExportFormat = "jpeg" | "png" | "webp";

export interface ExportSettings {
  format: ExportFormat;
  quality: number;
}

/** 单张人脸的 AI 美肤参数，全部 0..100，与 Rust `BeautyParams` 一致。 */
export interface BeautyParams {
  skinSmooth: number;
  texture: number;
  detail: number;
  blemish: number;
  spots: number;
  blackheads: number;
  toneEven: number;
  whitening: number;
  redness: number;
  dullness: number;
  darkCircles: number;
  eyeLines: number;
  foreheadLines: number;
  nasolabial: number;
}

export type BeautyKey = keyof BeautyParams;

export interface FaceBeauty {
  faceId: number;
  /** 稳定人脸 UUID；项目保存与重开匹配的主键。 */
  faceUuid: string;
  enabled: boolean;
  beauty: BeautyParams;
}

/** 轮廓级脸型参数，全部 -100..100。LEVEL 2 只开放整体宽度/长度/轮廓微调。 */
export interface FaceShape {
  slim: number;
  left: number;
  right: number;
  jaw: number;
  cheekbone: number;
  width: number;
  length: number;
  balance: number;
}

export type FaceShapeKey = keyof FaceShape;

/** LEVEL 3 参数：结构保留以便项目兼容，当前引擎会清零且 UI 禁用。 */
export interface EyeShape {
  size: number;
  left: number;
  right: number;
  width: number;
  height: number;
  distance: number;
  corner: number;
}

export interface NoseShape {
  bridge: number;
  alar: number;
  tip: number;
  height: number;
  balance: number;
}

export interface MouthShape {
  width: number;
  upper: number;
  lower: number;
  corner: number;
  symmetry: number;
  size: number;
}

export interface ChinShape {
  length: number;
  width: number;
  point: number;
  jawline: number;
}

export type WarpMode = "push" | "pull" | "zoomIn" | "zoomOut" | "protect" | "restore";

/** 手动变形笔触，坐标按原图归一化。 */
export interface WarpStroke {
  mode: WarpMode;
  /** 半径，按图像宽度归一化。 */
  radius: number;
  /** 0..100。 */
  strength: number;
  /** 羽化 0..100。 */
  feather: number;
  points: MaskPoint[];
}

export interface ManualWarp {
  enabled: boolean;
  strokes: WarpStroke[];
}

/** 单张人脸的塑形参数。 */
export interface FaceReshape {
  faceId: number;
  faceUuid: string;
  enabled: boolean;
  face: FaceShape;
  eyes: EyeShape;
  nose: NoseShape;
  mouth: MouthShape;
  chin: ChinShape;
  manual: ManualWarp;
}

export function defaultFaceShape(): FaceShape {
  return { slim: 0, left: 0, right: 0, jaw: 0, cheekbone: 0, width: 0, length: 0, balance: 0 };
}

function emptyEyes(): EyeShape {
  return { size: 0, left: 0, right: 0, width: 0, height: 0, distance: 0, corner: 0 };
}

function emptyNose(): NoseShape {
  return { bridge: 0, alar: 0, tip: 0, height: 0, balance: 0 };
}

function emptyMouth(): MouthShape {
  return { width: 0, upper: 0, lower: 0, corner: 0, symmetry: 0, size: 0 };
}

function emptyChin(): ChinShape {
  return { length: 0, width: 0, point: 0, jawline: 0 };
}

export function defaultFaceReshape(faceId: number, faceUuid = ""): FaceReshape {
  return {
    faceId,
    faceUuid,
    enabled: true,
    face: defaultFaceShape(),
    eyes: emptyEyes(),
    nose: emptyNose(),
    mouth: emptyMouth(),
    chin: emptyChin(),
    manual: { enabled: false, strokes: [] },
  };
}

export function faceShapeIsActive(face: FaceShape): boolean {
  return (Object.keys(face) as FaceShapeKey[]).some((key) => Math.abs(face[key]) >= 0.001);
}

export function faceReshapeIsActive(entry: FaceReshape): boolean {
  return (
    faceShapeIsActive(entry.face) ||
    (entry.manual.enabled && entry.manual.strokes.length > 0)
  );
}

export function findFaceReshape(params: EditParams, faceId: number): FaceReshape | undefined {
  return params.reshape.find((face) => face.faceId === faceId);
}

/** 返回替换了指定人脸塑形参数的副本；没有条目时自动创建。 */
export function withFaceReshape(
  params: EditParams,
  faceId: number,
  faceUuid: string,
  updater: (face: FaceReshape) => FaceReshape,
): EditParams {
  const existing = findFaceReshape(params, faceId) ?? defaultFaceReshape(faceId, faceUuid);
  const next = updater({
    ...existing,
    faceUuid: existing.faceUuid || faceUuid,
    face: { ...existing.face },
    manual: { ...existing.manual, strokes: [...existing.manual.strokes] },
  });
  const reshape = params.reshape.filter((face) => face.faceId !== faceId);
  reshape.push(next);
  reshape.sort((a, b) => a.faceId - b.faceId);
  return { ...params, reshape };
}

export interface MaskPoint {
  x: number;
  y: number;
}

/** 手动蒙版笔触，坐标按图像归一化，缩放后仍然准确。 */
export interface MaskStroke {
  erase: boolean;
  /** 画笔半径，按图像宽度归一化（0.002..0.25）。 */
  radius: number;
  points: MaskPoint[];
}

export interface ManualMask {
  enabled: boolean;
  invert: boolean;
  feather: number;
  strength: number;
  strokes: MaskStroke[];
}

/** 第五轮手动人物蒙版笔触类型。它不是 AI 分割结果，全部由用户画笔产生。 */
export type SubjectMaskStrokeKind = "paint" | "erase" | "foreground" | "background";

/** 手动人物蒙版笔触，坐标按图像宽/高归一化。 */
export interface SubjectMaskStroke {
  kind: SubjectMaskStrokeKind;
  /** 画笔半径，按图像宽度归一化（0.002..0.5）。 */
  radius: number;
  /** 硬度 0..100，100 为硬边。 */
  hardness: number;
  /** 强度 0..100。 */
  strength: number;
  points: MaskPoint[];
}

/** 手动人物蒙版参数，与 Rust `SubjectMaskParams` 对应。 */
export interface SubjectMaskParams {
  enabled: boolean;
  invert: boolean;
  /** 羽化 0..100。 */
  feather: number;
  /** 收缩/扩张 -100..100，正值扩张。 */
  expand: number;
  /** 边缘平滑 0..100。 */
  smooth: number;
  /** 发丝辅助 0..100。 */
  hairProtect: number;
  /** 边缘去污染 0..100。 */
  decontaminate: number;
  strokes: SubjectMaskStroke[];
  /** `manual` 表示本地手动笔触，不是 AI 模型输出。 */
  source: string;
  modelVersion: string;
}

export function defaultSubjectMask(): SubjectMaskParams {
  return {
    enabled: false,
    invert: false,
    feather: 40,
    expand: 0,
    smooth: 15,
    hairProtect: 0,
    decontaminate: 40,
    strokes: [],
    source: "manual",
    modelVersion: "manual-alpha-1",
  };
}

export function subjectMaskHasContent(mask: SubjectMaskParams): boolean {
  return mask.enabled && mask.strokes.length > 0;
}

/** 身材参数，全部 -100..100，0 表示不变。 */
export interface BodyShape {
  waist: number;
  shoulders: number;
  hips: number;
  thighs: number;
  calves: number;
  arms: number;
  legs: number;
}

export type BodyShapeKey = keyof BodyShape;

export function defaultBodyShape(): BodyShape {
  return { waist: 0, shoulders: 0, hips: 0, thighs: 0, calves: 0, arms: 0, legs: 0 };
}

export function bodyShapeIsActive(shape: BodyShape): boolean {
  return (Object.keys(shape) as BodyShapeKey[]).some((key) => Math.abs(shape[key]) >= 0.001);
}

/** 用户手动放置的身体控制点部位（编辑辅助标签，不是 AI 识别结果）。 */
export type BodyAnchorPart =
  | "shoulderLeft"
  | "shoulderRight"
  | "waistLeft"
  | "waistRight"
  | "hipLeft"
  | "hipRight"
  | "thighLeft"
  | "thighRight"
  | "kneeLeft"
  | "kneeRight"
  | "calfLeft"
  | "calfRight"
  | "armLeft"
  | "armRight"
  | "custom";

export const BODY_ANCHOR_LABELS: Record<BodyAnchorPart, string> = {
  shoulderLeft: "左肩",
  shoulderRight: "右肩",
  waistLeft: "左腰",
  waistRight: "右腰",
  hipLeft: "左髋",
  hipRight: "右髋",
  thighLeft: "左大腿",
  thighRight: "右大腿",
  kneeLeft: "左膝",
  kneeRight: "右膝",
  calfLeft: "左小腿",
  calfRight: "右小腿",
  armLeft: "左臂",
  armRight: "右臂",
  custom: "自定义",
};

/** 手动身材锚点，坐标与位移都按归一化比例保存。 */
export interface BodyAnchor {
  part: BodyAnchorPart;
  x: number;
  y: number;
  dx: number;
  dy: number;
  /** 影响半径，按图像短边归一化。 */
  radius: number;
  locked: boolean;
}

export function defaultBodyAnchor(part: BodyAnchorPart = "custom"): BodyAnchor {
  return { part, x: 0.5, y: 0.5, dx: 0, dy: 0, radius: 0.12, locked: false };
}

/** 单个身材对象参数；`subjectId` 为稳定人物 UUID（与人脸 UUID 不混用）。 */
export interface BodyAdjust {
  subjectId: string;
  enabled: boolean;
  shape: BodyShape;
  anchors: BodyAnchor[];
  strokes: WarpStroke[];
  /** 该人物独立蒙版笔触；为空时使用全局人物蒙版。 */
  maskStrokes: SubjectMaskStroke[];
}

export function defaultBodyAdjust(subjectId: string): BodyAdjust {
  return {
    subjectId,
    enabled: true,
    shape: defaultBodyShape(),
    anchors: [],
    strokes: [],
    maskStrokes: [],
  };
}

export function bodyAdjustIsActive(entry: BodyAdjust): boolean {
  return (
    bodyShapeIsActive(entry.shape) ||
    entry.anchors.length > 0 ||
    entry.strokes.length > 0
  );
}

export type BackgroundMode = "original" | "blur" | "solidColor" | "gradient" | "image";
export type BackgroundFit = "cover" | "contain" | "stretch";

export interface GradientStop {
  position: number;
  color: string;
}

export interface BackgroundGradient {
  angle: number;
  stops: GradientStop[];
}

/** 背景处理参数，与 Rust `BackgroundParams` 对应。 */
export interface BackgroundParams {
  enabled: boolean;
  mode: BackgroundMode;
  blur: number;
  /** 模拟景深 0..100，不是真实 3D 深度。 */
  depth: number;
  brightness: number;
  temperature: number;
  saturation: number;
  color: string;
  gradient: BackgroundGradient;
  /** 本地背景图片路径，不会上传到任何服务器。 */
  imagePath: string;
  imageFit: BackgroundFit;
  imageScale: number;
  imageOffsetX: number;
  imageOffsetY: number;
  imageBlur: number;
  decontaminate: number;
}

export function defaultBackground(): BackgroundParams {
  return {
    enabled: false,
    mode: "original",
    blur: 45,
    depth: 30,
    brightness: 0,
    temperature: 0,
    saturation: 0,
    color: "#1f2933",
    gradient: {
      angle: 0,
      stops: [
        { position: 0, color: "#1c2733" },
        { position: 100, color: "#4a6076" },
      ],
    },
    imagePath: "",
    imageFit: "cover",
    imageScale: 100,
    imageOffsetX: 0,
    imageOffsetY: 0,
    imageBlur: 0,
    decontaminate: 60,
  };
}

export function backgroundIsActive(bg: BackgroundParams): boolean {
  return bg.enabled && bg.mode !== "original";
}

export function findBodyAdjust(params: EditParams, subjectId: string): BodyAdjust | undefined {
  return params.body.find((entry) => entry.subjectId === subjectId);
}

/** 返回替换了指定人物身材参数的副本；没有条目时自动创建。 */
export function withBodyAdjust(
  params: EditParams,
  subjectId: string,
  updater: (entry: BodyAdjust) => BodyAdjust,
): EditParams {
  const existing = findBodyAdjust(params, subjectId) ?? defaultBodyAdjust(subjectId);
  const next = updater({
    ...existing,
    shape: { ...existing.shape },
    anchors: existing.anchors.map((anchor) => ({ ...anchor })),
    strokes: existing.strokes.map((stroke) => ({ ...stroke, points: [...stroke.points] })),
    maskStrokes: existing.maskStrokes.map((stroke) => ({ ...stroke, points: [...stroke.points] })),
  });
  const body = params.body.filter((entry) => entry.subjectId !== subjectId);
  body.push(next);
  body.sort((a, b) => a.subjectId.localeCompare(b.subjectId));
  return { ...params, body };
}

/** 手动修复点：只修复该点半径内的候选瑕疵，可逐点撤销。 */
export interface RepairPoint {
  x: number;
  y: number;
  /** 作用半径，按图像宽度归一化。 */
  radius: number;
}

export interface EditParams {
  exposure: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  temperature: number;
  tint: number;
  saturation: number;
  vibrance: number;
  clarity: number;
  sharpness: number;
  grain: number;
  vignette: number;
  /** 第二轮旧字段，保留项目兼容；新逻辑一律使用按人脸参数。 */
  skinSmooth: number;
  faces: FaceBeauty[];
  /** 第四轮：按稳定 faceUuid 归属的面部塑形参数。 */
  reshape: FaceReshape[];
  /** 第五轮：手动人物蒙版（非 AI 分割）。 */
  subjectMask: SubjectMaskParams;
  /** 第五轮：按稳定人物 UUID 归属的身材参数。 */
  body: BodyAdjust[];
  /** 第五轮：背景处理参数。 */
  background: BackgroundParams;
  manualMask: ManualMask;
  repairPoints: RepairPoint[];
  hsl: HslParams;
  curves: CurveSet;
  lut: LutSettings | null;
}

export type AdjustmentKey =
  | "exposure"
  | "contrast"
  | "highlights"
  | "shadows"
  | "whites"
  | "blacks"
  | "temperature"
  | "tint"
  | "saturation"
  | "vibrance"
  | "clarity"
  | "sharpness"
  | "grain"
  | "vignette";

export const BEAUTY_KEYS: BeautyKey[] = [
  "skinSmooth",
  "texture",
  "detail",
  "blemish",
  "spots",
  "blackheads",
  "toneEven",
  "whitening",
  "redness",
  "dullness",
  "darkCircles",
  "eyeLines",
  "foreheadLines",
  "nasolabial",
];

export const BEAUTY_LABELS: Record<BeautyKey, string> = {
  skinSmooth: "磨皮",
  texture: "纹理保护",
  detail: "细节恢复",
  blemish: "自动祛痘",
  spots: "祛斑",
  blackheads: "黑头",
  toneEven: "肤色均匀",
  whitening: "美白",
  redness: "泛红改善",
  dullness: "暗沉改善",
  darkCircles: "黑眼圈淡化",
  eyeLines: "眼下细纹",
  foreheadLines: "额头细纹",
  nasolabial: "法令纹",
};

export function defaultBeauty(): BeautyParams {
  return {
    skinSmooth: 0,
    texture: 0,
    detail: 0,
    blemish: 0,
    spots: 0,
    blackheads: 0,
    toneEven: 0,
    whitening: 0,
    redness: 0,
    dullness: 0,
    darkCircles: 0,
    eyeLines: 0,
    foreheadLines: 0,
    nasolabial: 0,
  };
}

export function defaultManualMask(): ManualMask {
  return { enabled: false, invert: false, feather: 40, strength: 100, strokes: [] };
}

export function beautyIsActive(beauty: BeautyParams): boolean {
  return BEAUTY_KEYS.some((key) => Math.abs(beauty[key]) >= 0.001);
}

export function defaultFaceBeauty(faceId: number, faceUuid = ""): FaceBeauty {
  return { faceId, faceUuid, enabled: true, beauty: defaultBeauty() };
}

export function findFaceBeauty(params: EditParams, faceId: number): FaceBeauty | undefined {
  return params.faces.find((face) => face.faceId === faceId);
}

/** 返回替换了指定人脸参数的副本；没有人脸条目时自动创建。 */
export function withFaceBeauty(
  params: EditParams,
  faceId: number,
  updater: (face: FaceBeauty) => FaceBeauty,
  faceUuid = "",
): EditParams {
  const existing = findFaceBeauty(params, faceId) ?? defaultFaceBeauty(faceId, faceUuid);
  const next = updater({
    ...existing,
    faceUuid: existing.faceUuid || faceUuid,
    beauty: { ...existing.beauty },
  });
  const faces = params.faces.filter((face) => face.faceId !== faceId);
  faces.push(next);
  faces.sort((a, b) => a.faceId - b.faceId);
  return { ...params, faces };
}

export interface ParamDefinition {
  key: AdjustmentKey;
  label: string;
  min: number;
  max: number;
  step: number;
  /** 以 0 为中点的双向滑杆（曝光、色温等）。 */
  bipolar: boolean;
}

export interface ParamGroup {
  id: string;
  label: string;
  hint?: string;
  params: ParamDefinition[];
}

export const HSL_CHANNELS: { key: HslKey; label: string; color: string }[] = [
  { key: "red", label: "红", color: "#d9635a" },
  { key: "orange", label: "橙", color: "#dc9a55" },
  { key: "yellow", label: "黄", color: "#c9c45c" },
  { key: "green", label: "绿", color: "#74b37a" },
  { key: "aqua", label: "青", color: "#6db5b6" },
  { key: "blue", label: "蓝", color: "#6f93cf" },
  { key: "purple", label: "紫", color: "#9c82ce" },
  { key: "magenta", label: "品红", color: "#c87aab" },
];

export const CURVE_CHANNELS: { key: CurveChannel; label: string; color: string }[] = [
  { key: "rgb", label: "RGB", color: "#e0b57c" },
  { key: "red", label: "红", color: "#e07a6f" },
  { key: "green", label: "绿", color: "#7fcb92" },
  { key: "blue", label: "蓝", color: "#7fa8e0" },
];

export const DEFAULT_EXPORT: ExportSettings = { format: "jpeg", quality: 92 };

function neutralHsl(): HslParams {
  return {
    red: { hue: 0, saturation: 0, luminance: 0 },
    orange: { hue: 0, saturation: 0, luminance: 0 },
    yellow: { hue: 0, saturation: 0, luminance: 0 },
    green: { hue: 0, saturation: 0, luminance: 0 },
    aqua: { hue: 0, saturation: 0, luminance: 0 },
    blue: { hue: 0, saturation: 0, luminance: 0 },
    purple: { hue: 0, saturation: 0, luminance: 0 },
    magenta: { hue: 0, saturation: 0, luminance: 0 },
  };
}

export function emptyCurves(): CurveSet {
  return { rgb: [], red: [], green: [], blue: [] };
}

/** 与 Rust 端 `EditParams` 字段一一对应，新增参数必须两边同步。 */
export function defaultParams(): EditParams {
  return {
    exposure: 0,
    contrast: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    temperature: 0,
    tint: 0,
    saturation: 0,
    vibrance: 0,
    clarity: 0,
    sharpness: 0,
    grain: 0,
    vignette: 0,
    skinSmooth: 0,
    faces: [],
    reshape: [],
    subjectMask: defaultSubjectMask(),
    body: [],
    background: defaultBackground(),
    manualMask: defaultManualMask(),
    repairPoints: [],
    hsl: neutralHsl(),
    curves: emptyCurves(),
    lut: null,
  };
}

export const DEFAULT_PARAMS: EditParams = defaultParams();

const BIPOLAR = { min: -100, max: 100, step: 1, bipolar: true } as const;
const UNIPOLAR = { min: 0, max: 100, step: 1, bipolar: false } as const;

export const PARAM_GROUPS: ParamGroup[] = [
  {
    id: "basic",
    label: "基础",
    params: [
      { key: "exposure", label: "曝光", ...BIPOLAR },
      { key: "contrast", label: "对比度", ...BIPOLAR },
    ],
  },
  {
    id: "light",
    label: "光线",
    params: [
      { key: "highlights", label: "高光", ...BIPOLAR },
      { key: "shadows", label: "阴影", ...BIPOLAR },
      { key: "whites", label: "白色色阶", ...BIPOLAR },
      { key: "blacks", label: "黑色色阶", ...BIPOLAR },
    ],
  },
  {
    id: "color",
    label: "色彩",
    params: [
      { key: "temperature", label: "色温", ...BIPOLAR },
      { key: "tint", label: "色调", ...BIPOLAR },
      { key: "saturation", label: "饱和度", ...BIPOLAR },
      { key: "vibrance", label: "自然饱和度", ...BIPOLAR },
    ],
  },
  {
    id: "detail",
    label: "细节",
    params: [
      { key: "clarity", label: "清晰度", ...UNIPOLAR },
      { key: "sharpness", label: "锐化", ...UNIPOLAR },
      { key: "grain", label: "颗粒", ...UNIPOLAR },
      { key: "vignette", label: "暗角", ...UNIPOLAR },
    ],
  },
];

export const PARAM_INDEX: Record<AdjustmentKey, ParamDefinition> = Object.fromEntries(
  PARAM_GROUPS.flatMap((group) => group.params).map((definition) => [definition.key, definition]),
) as Record<AdjustmentKey, ParamDefinition>;

export const PARAM_LABELS: Record<AdjustmentKey, string> = Object.fromEntries(
  PARAM_GROUPS.flatMap((group) => group.params).map((definition) => [definition.key, definition.label]),
) as Record<AdjustmentKey, string>;

function clampValue(value: unknown, min: number, max: number): number {
  const numeric = typeof value === "number" && Number.isFinite(value) ? value : 0;
  return Math.min(max, Math.max(min, numeric));
}

function sanitizeHex(value: unknown, fallback: string): string {
  const toHex = (candidate: string) => {
    const body = candidate.trim().replace(/^#/, "");
    return /^[0-9a-fA-F]{6}$/.test(body) ? `#${body.toUpperCase()}` : null;
  };
  const fallbackHex = toHex(fallback) ?? fallback;
  if (typeof value !== "string") return fallbackHex;
  return toHex(value) ?? fallbackHex;
}

const SUBJECT_STROKE_KINDS: SubjectMaskStrokeKind[] = ["paint", "erase", "foreground", "background"];

function clampSubjectStrokes(input: unknown): SubjectMaskStroke[] {
  if (!Array.isArray(input)) return [];
  return input
    .filter((stroke) => Array.isArray(stroke?.points) && stroke.points.length > 0)
    .slice(0, 400)
    .map((stroke) => ({
      kind: SUBJECT_STROKE_KINDS.includes(stroke?.kind) ? stroke.kind : "paint",
      radius: clampValue(stroke?.radius, 0.002, 0.5),
      hardness: clampValue(stroke?.hardness, 0, 100),
      strength: clampValue(stroke?.strength, 0, 100),
      points: stroke.points
        .filter((point: MaskPoint) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
        .slice(0, 8000)
        .map((point: MaskPoint) => ({ x: clampValue(point.x, 0, 1), y: clampValue(point.y, 0, 1) })),
    }))
    .filter((stroke) => stroke.points.length > 0);
}

function clampSubjectMask(input: unknown): SubjectMaskParams {
  const base = defaultSubjectMask();
  const source = (input ?? {}) as Partial<SubjectMaskParams>;
  return {
    enabled: source.enabled === true,
    invert: source.invert === true,
    feather: clampValue(source.feather, 0, 100),
    expand: clampValue(source.expand, -100, 100),
    smooth: clampValue(source.smooth, 0, 100),
    hairProtect: clampValue(source.hairProtect, 0, 100),
    decontaminate: clampValue(source.decontaminate, 0, 100),
    strokes: clampSubjectStrokes(source.strokes),
    source: typeof source.source === "string" && source.source.trim() ? source.source : base.source,
    modelVersion:
      typeof source.modelVersion === "string" && source.modelVersion.trim()
        ? source.modelVersion
        : base.modelVersion,
  };
}

const ANCHOR_PARTS: BodyAnchorPart[] = [
  "shoulderLeft",
  "shoulderRight",
  "waistLeft",
  "waistRight",
  "hipLeft",
  "hipRight",
  "thighLeft",
  "thighRight",
  "kneeLeft",
  "kneeRight",
  "calfLeft",
  "calfRight",
  "armLeft",
  "armRight",
  "custom",
];

function clampBodyShape(input: unknown): BodyShape {
  const source = (input ?? {}) as Partial<BodyShape>;
  const shape = defaultBodyShape();
  for (const key of Object.keys(shape) as BodyShapeKey[]) {
    shape[key] = clampValue(source[key], -100, 100);
  }
  return shape;
}

function clampBody(input: unknown): BodyAdjust[] {
  if (!Array.isArray(input)) return [];
  const seen = new Set<string>();
  return input
    .filter((entry) => typeof entry?.subjectId === "string" && entry.subjectId.trim())
    .slice(0, 32)
    .map((entry) => {
      const anchors: BodyAnchor[] = Array.isArray(entry.anchors)
        ? entry.anchors
            .slice(0, 200)
            .map((anchor: BodyAnchor) => ({
              part: ANCHOR_PARTS.includes(anchor?.part) ? anchor.part : "custom",
              x: clampValue(anchor?.x, 0, 1),
              y: clampValue(anchor?.y, 0, 1),
              dx: clampValue(anchor?.dx, -0.15, 0.15),
              dy: clampValue(anchor?.dy, -0.15, 0.15),
              radius: clampValue(anchor?.radius, 0.02, 0.5),
              locked: anchor?.locked === true,
            }))
        : [];
      const strokes: WarpStroke[] = Array.isArray(entry.strokes)
        ? entry.strokes
            .filter((stroke: WarpStroke) => Array.isArray(stroke?.points) && stroke.points.length > 0)
            .slice(0, 400)
            .map((stroke: WarpStroke) => ({
              mode: stroke.mode,
              radius: clampValue(stroke.radius, 0.004, 0.25),
              strength: clampValue(stroke.strength, 0, 100),
              feather: clampValue(stroke.feather, 0, 100),
              points: stroke.points
                .filter((point) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
                .slice(0, 4000)
                .map((point) => ({ x: clampValue(point.x, 0, 1), y: clampValue(point.y, 0, 1) })),
            }))
        : [];
      return {
        subjectId: entry.subjectId as string,
        enabled: entry.enabled !== false,
        shape: clampBodyShape(entry.shape),
        anchors,
        strokes,
        maskStrokes: clampSubjectStrokes(entry.maskStrokes),
      };
    })
    .filter((entry) => {
      if (seen.has(entry.subjectId)) return false;
      seen.add(entry.subjectId);
      return true;
    })
    .sort((a, b) => a.subjectId.localeCompare(b.subjectId));
}

function clampBackground(input: unknown): BackgroundParams {
  const base = defaultBackground();
  const source = (input ?? {}) as Partial<BackgroundParams>;
  const modes: BackgroundMode[] = ["original", "blur", "solidColor", "gradient", "image"];
  const fits: BackgroundFit[] = ["cover", "contain", "stretch"];
  const mode = modes.includes(source.mode as BackgroundMode) ? source.mode! : base.mode;
  const stops = Array.isArray(source.gradient?.stops)
    ? source.gradient!.stops
        .slice(0, 8)
        .map((stop) => ({
          position: clampValue(stop?.position, 0, 100),
          color: sanitizeHex(stop?.color, "#000000"),
        }))
        .sort((a, b) => a.position - b.position)
    : base.gradient.stops;
  const imagePath = typeof source.imagePath === "string" ? source.imagePath : "";
  return {
    enabled: source.enabled === true,
    mode: mode === "image" && !imagePath.trim() ? "original" : mode,
    blur: clampValue(source.blur, 0, 100),
    depth: clampValue(source.depth, 0, 100),
    brightness: clampValue(source.brightness, -100, 100),
    temperature: clampValue(source.temperature, -100, 100),
    saturation: clampValue(source.saturation, -100, 100),
    color: sanitizeHex(source.color, base.color),
    gradient: {
      angle: clampValue(source.gradient?.angle, 0, 360),
      stops: stops.length > 0 ? stops : base.gradient.stops,
    },
    imagePath,
    imageFit: fits.includes(source.imageFit as BackgroundFit) ? source.imageFit! : base.imageFit,
    imageScale: clampValue(source.imageScale, 50, 200),
    imageOffsetX: clampValue(source.imageOffsetX, -100, 100),
    imageOffsetY: clampValue(source.imageOffsetY, -100, 100),
    imageBlur: clampValue(source.imageBlur, 0, 100),
    decontaminate: clampValue(source.decontaminate, 0, 100),
  };
}

/** 深拷贝并夹紧所有参数，项目文件来自磁盘时同样经过这里。 */
export function clampParams(input: EditParams): EditParams {
  const next = defaultParams();
  const source = input as Partial<EditParams>;
  for (const definition of Object.values(PARAM_INDEX)) {
    next[definition.key] = clampValue(source[definition.key], definition.min, definition.max);
  }
  next.skinSmooth = clampValue(source.skinSmooth, 0, 100);
  const faces = Array.isArray(source.faces) ? source.faces : [];
  const seen = new Set<number>();
  next.faces = faces
    .filter((face) => Number.isFinite(face?.faceId))
    .map((face) => {
      const faceId = Math.max(0, Math.round(face.faceId));
      const beauty = defaultBeauty();
      for (const key of BEAUTY_KEYS) {
        beauty[key] = clampValue(face.beauty?.[key], 0, 100);
      }
      return {
        faceId,
        faceUuid: typeof face.faceUuid === "string" ? face.faceUuid : "",
        enabled: face.enabled !== false,
        beauty,
      };
    })
    .filter((face) => {
      if (seen.has(face.faceId)) return false;
      seen.add(face.faceId);
      return true;
    })
    .sort((a, b) => a.faceId - b.faceId);

  const reshapeSource = Array.isArray(source.reshape) ? source.reshape : [];
  const seenReshape = new Set<number>();
  const warpModes: WarpMode[] = ["push", "pull", "zoomIn", "zoomOut", "protect", "restore"];
  next.reshape = reshapeSource
    .filter((entry) => Number.isFinite(entry?.faceId))
    .map((entry) => {
      const faceId = Math.max(0, Math.round(entry.faceId));
      const base = defaultFaceReshape(
        faceId,
        typeof entry.faceUuid === "string" ? entry.faceUuid : "",
      );
      base.enabled = entry.enabled !== false;
      for (const key of Object.keys(base.face) as FaceShapeKey[]) {
        base.face[key] = clampValue(entry.face?.[key], -100, 100);
      }
      for (const key of Object.keys(base.eyes) as (keyof EyeShape)[]) {
        base.eyes[key] = clampValue(entry.eyes?.[key], -100, 100);
      }
      for (const key of Object.keys(base.nose) as (keyof NoseShape)[]) {
        base.nose[key] = clampValue(entry.nose?.[key], -100, 100);
      }
      for (const key of Object.keys(base.mouth) as (keyof MouthShape)[]) {
        base.mouth[key] = clampValue(entry.mouth?.[key], -100, 100);
      }
      for (const key of Object.keys(base.chin) as (keyof ChinShape)[]) {
        base.chin[key] = clampValue(entry.chin?.[key], -100, 100);
      }
      const strokes = Array.isArray(entry.manual?.strokes) ? entry.manual.strokes : [];
      base.manual = {
        enabled: entry.manual?.enabled === true,
        strokes: strokes
          .filter((stroke) => warpModes.includes(stroke?.mode) && Array.isArray(stroke?.points) && stroke.points.length > 0)
          .slice(0, 400)
          .map((stroke) => ({
            mode: stroke.mode,
            radius: clampValue(stroke.radius, 0.004, 0.25),
            strength: clampValue(stroke.strength, 0, 100),
            feather: clampValue(stroke.feather, 0, 100),
            points: stroke.points
              .filter((point) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
              .slice(0, 4000)
              .map((point) => ({ x: clampValue(point.x, 0, 1), y: clampValue(point.y, 0, 1) })),
          }))
          .filter((stroke) => stroke.points.length > 0),
      };
      return base;
    })
    .filter((entry) => {
      if (seenReshape.has(entry.faceId)) return false;
      seenReshape.add(entry.faceId);
      return true;
    })
    .sort((a, b) => a.faceId - b.faceId);

  next.subjectMask = clampSubjectMask(source.subjectMask);
  next.body = clampBody(source.body);
  next.background = clampBackground(source.background);

  const manual = source.manualMask as Partial<ManualMask> | undefined;
  const strokes = Array.isArray(manual?.strokes) ? manual.strokes : [];
  next.manualMask = {
    enabled: manual?.enabled === true,
    invert: manual?.invert === true,
    feather: clampValue(manual?.feather, 0, 100),
    strength: clampValue(manual?.strength, 0, 100),
    strokes: strokes
      .filter((stroke) => Array.isArray(stroke?.points) && stroke.points.length > 0)
      .slice(0, 400)
      .map((stroke) => ({
        erase: stroke.erase === true,
        radius: clampValue(stroke.radius, 0.002, 0.25),
        points: stroke.points
          .filter((point) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
          .slice(0, 4000)
          .map((point) => ({ x: clampValue(point.x, 0, 1), y: clampValue(point.y, 0, 1) })),
      }))
      .filter((stroke) => stroke.points.length > 0),
  };
  const points = Array.isArray(source.repairPoints) ? source.repairPoints : [];
  next.repairPoints = points
    .filter((point) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
    .slice(0, 200)
    .map((point) => ({
      x: clampValue(point.x, 0, 1),
      y: clampValue(point.y, 0, 1),
      radius: clampValue(point.radius, 0.004, 0.25),
    }));
  const hsl = source.hsl as Partial<HslParams> | undefined;
  for (const channel of HSL_CHANNELS) {
    const value = hsl?.[channel.key];
    next.hsl[channel.key] = {
      hue: clampValue(value?.hue, -100, 100),
      saturation: clampValue(value?.saturation, -100, 100),
      luminance: clampValue(value?.luminance, -100, 100),
    };
  }
  const curves = source.curves as Partial<CurveSet> | undefined;
  for (const channel of CURVE_CHANNELS) {
    const points = curves?.[channel.key];
    next.curves[channel.key] = Array.isArray(points)
      ? points
          .filter((point) => Number.isFinite(point?.x) && Number.isFinite(point?.y))
          .map((point) => ({
            x: Math.min(1, Math.max(0, point.x)),
            y: Math.min(1, Math.max(0, point.y)),
          }))
          .sort((a, b) => a.x - b.x)
          .slice(0, 16)
      : [];
  }
  const lut = source.lut;
  next.lut =
    lut && typeof lut.path === "string" && lut.path
      ? {
          path: lut.path,
          name: typeof lut.name === "string" && lut.name ? lut.name : lut.path.split("/").pop() || "LUT",
          strength: clampValue(lut.strength, 0, 100),
        }
      : null;
  return next;
}

export function cloneParams(input: EditParams): EditParams {
  return clampParams(JSON.parse(JSON.stringify(input)) as EditParams);
}

export function isNeutral(params: EditParams): boolean {
  if (activeParamCount(params) > 0) return false;
  return true;
}

export function activeParamCount(params: EditParams): number {
  let count = 0;
  for (const key of Object.keys(PARAM_INDEX) as AdjustmentKey[]) {
    if (Math.abs(params[key] - DEFAULT_PARAMS[key]) >= 0.001) count += 1;
  }
  for (const channel of HSL_CHANNELS) {
    const value = params.hsl[channel.key];
    if (
      Math.abs(value.hue) >= 0.001 ||
      Math.abs(value.saturation) >= 0.001 ||
      Math.abs(value.luminance) >= 0.001
    ) {
      count += 1;
    }
  }
  for (const channel of CURVE_CHANNELS) {
    if (params.curves[channel.key].length > 0) count += 1;
  }
  if (params.lut && params.lut.strength > 0) count += 1;
  if (params.skinSmooth > 0.001) count += 1;
  for (const face of params.faces) {
    if (face.enabled && beautyIsActive(face.beauty)) count += 1;
  }
  for (const face of params.reshape) {
    if (face.enabled && faceReshapeIsActive(face)) count += 1;
  }
  if (subjectMaskHasContent(params.subjectMask)) count += 1;
  for (const entry of params.body) {
    if (entry.enabled && bodyAdjustIsActive(entry)) count += 1;
  }
  if (backgroundIsActive(params.background)) count += 1;
  return count;
}

/** 参数对象结构固定，序列化结果可直接作为缓存键。 */
export function paramsKey(params: EditParams): string {
  return JSON.stringify(params);
}

export function paramsEqual(a: EditParams, b: EditParams): boolean {
  return paramsKey(a) === paramsKey(b);
}

/** 参数展示文本：双向参数带正负号，单向参数直接显示数值。 */
export function formatParamValue(definition: ParamDefinition, value: number): string {
  if (!definition.bipolar) return String(Math.round(value));
  const rounded = Math.round(value);
  return `${rounded > 0 ? "+" : ""}${rounded}`;
}
