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
  skinSmooth: number;
  hsl: HslParams;
  curves: CurveSet;
  lut: LutSettings | null;
}

export type AdjustmentKey = Exclude<keyof EditParams, "hsl" | "curves" | "lut">;

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
      { key: "skinSmooth", label: "磨皮", ...UNIPOLAR },
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

/** 深拷贝并夹紧所有参数，项目文件来自磁盘时同样经过这里。 */
export function clampParams(input: EditParams): EditParams {
  const next = defaultParams();
  const source = input as Partial<EditParams>;
  for (const definition of Object.values(PARAM_INDEX)) {
    next[definition.key] = clampValue(source[definition.key], definition.min, definition.max);
  }
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
