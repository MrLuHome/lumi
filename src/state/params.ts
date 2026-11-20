export interface EditParams {
  exposure: number;
  contrast: number;
  highlights: number;
  shadows: number;
  temperature: number;
  tint: number;
  saturation: number;
  vibrance: number;
  clarity: number;
  sharpness: number;
  grain: number;
  vignette: number;
  skinSmooth: number;
}

export type AdjustmentKey = keyof EditParams;

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

/** 与 Rust 端 `EditParams` 字段一一对应，新增参数必须两边同步。 */
export const DEFAULT_PARAMS: EditParams = {
  exposure: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  temperature: 0,
  tint: 0,
  saturation: 0,
  vibrance: 0,
  clarity: 0,
  sharpness: 0,
  grain: 0,
  vignette: 0,
  skinSmooth: 0,
};

const BIPOLAR = { min: -100, max: 100, step: 1, bipolar: true } as const;
const UNIPOLAR = { min: 0, max: 100, step: 1, bipolar: false } as const;

export const PARAM_GROUPS: ParamGroup[] = [
  {
    id: "light",
    label: "光影",
    params: [
      { key: "exposure", label: "曝光", ...BIPOLAR },
      { key: "contrast", label: "对比度", ...BIPOLAR },
      { key: "highlights", label: "高光", ...BIPOLAR },
      { key: "shadows", label: "阴影", ...BIPOLAR },
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
  {
    id: "portrait",
    label: "人像",
    hint: "基础磨皮 · AI 精修模块待接入",
    params: [{ key: "skinSmooth", label: "磨皮", ...UNIPOLAR }],
  },
];

export const PARAM_INDEX: Record<AdjustmentKey, ParamDefinition> = Object.fromEntries(
  PARAM_GROUPS.flatMap((group) => group.params).map((definition) => [definition.key, definition]),
) as Record<AdjustmentKey, ParamDefinition>;

export const PARAM_LABELS: Record<AdjustmentKey, string> = Object.fromEntries(
  PARAM_GROUPS.flatMap((group) => group.params).map((definition) => [definition.key, definition.label]),
) as Record<AdjustmentKey, string>;

export function clampParams(input: EditParams): EditParams {
  const next = { ...input };
  for (const definition of Object.values(PARAM_INDEX)) {
    const value = Number.isFinite(next[definition.key]) ? next[definition.key] : 0;
    next[definition.key] = Math.min(definition.max, Math.max(definition.min, value));
  }
  return next;
}

export function isNeutral(params: EditParams): boolean {
  return (Object.keys(DEFAULT_PARAMS) as AdjustmentKey[]).every(
    (key) => Math.abs(params[key] - DEFAULT_PARAMS[key]) < 0.001,
  );
}

export function activeParamCount(params: EditParams): number {
  return (Object.keys(DEFAULT_PARAMS) as AdjustmentKey[]).filter(
    (key) => Math.abs(params[key] - DEFAULT_PARAMS[key]) >= 0.001,
  ).length;
}

export function paramsEqual(a: EditParams, b: EditParams): boolean {
  return (Object.keys(DEFAULT_PARAMS) as AdjustmentKey[]).every((key) => a[key] === b[key]);
}

/** 用作渲染结果缓存的键：参数相同则命中，避免重复请求 Rust 管线。 */
export function paramsKey(params: EditParams): string {
  return (Object.keys(DEFAULT_PARAMS) as AdjustmentKey[])
    .map((key) => `${key}:${params[key]}`)
    .join("|");
}

/** 参数展示文本：双向参数带正负号，单向参数直接显示数值。 */
export function formatParamValue(definition: ParamDefinition, value: number): string {
  if (!definition.bipolar) return String(Math.round(value));
  const rounded = Math.round(value);
  return `${rounded > 0 ? "+" : ""}${rounded}`;
}
