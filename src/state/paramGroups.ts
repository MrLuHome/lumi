import type { BeautyParams, EditParams } from "../types";
import { cloneParams } from "./params";

export type CopyGroupId =
  | "light"
  | "whiteBalance"
  | "color"
  | "hsl"
  | "curves"
  | "lut"
  | "detail"
  | "beauty";

export interface CopyGroupDef {
  id: CopyGroupId;
  label: string;
  fields: (keyof EditParams)[];
}

/** 可批量同步的纯参数组。 */
export const COPY_GROUPS: CopyGroupDef[] = [
  { id: "light", label: "基础光影", fields: ["exposure", "contrast", "highlights", "shadows", "whites", "blacks"] },
  { id: "whiteBalance", label: "白平衡", fields: ["temperature", "tint"] },
  { id: "color", label: "色彩", fields: ["saturation", "vibrance"] },
  { id: "hsl", label: "HSL", fields: ["hsl"] },
  { id: "curves", label: "曲线", fields: ["curves"] },
  { id: "lut", label: "LUT", fields: ["lut"] },
  { id: "detail", label: "细节", fields: ["clarity", "sharpness", "grain", "vignette"] },
  { id: "beauty", label: "美肤模板", fields: ["skinSmooth"] },
];

/**
 * 对象级参数：人脸 UUID、人物 UUID、手工蒙版笔触、变形锚点、背景图片引用。
 * 批量复制永远不包含这些字段，避免把某个人的五官或蒙版套到别人身上。
 */
export const OBJECT_LEVEL_GROUPS: { label: string; reason: string }[] = [
  { label: "人脸独立美肤", reason: "按人脸 UUID 归属，跨图不复制" },
  { label: "面部塑形 / 五官", reason: "锚点属于具体人脸，盲目复制会改变身份特征" },
  { label: "手动人物蒙版", reason: "笔触对应具体构图，默认排除" },
  { label: "身材参数", reason: "按人物 UUID 归属，跨图不复制" },
  { label: "背景处理", reason: "背景图片路径与抠图结果不可跨图复用" },
  { label: "手动蒙版与修复点", reason: "坐标与笔触属于当前照片" },
];

export const DEFAULT_COPY_GROUPS: CopyGroupId[] = [
  "light",
  "whiteBalance",
  "color",
  "hsl",
  "curves",
  "lut",
  "detail",
  "beauty",
];

const BEAUTY_FIELDS: (keyof BeautyParams)[] = [
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

export interface ParamCopy {
  groups: CopyGroupId[];
  /** 纯参数快照，绝不包含 faces / reshape / mask / body / background。 */
  values: Partial<EditParams>;
  /** 美肤模板：字段到强度的最大值，应用时只写入目标照片自己的人脸。 */
  beautyTemplate: Partial<BeautyParams> | null;
  /** 缺失 LUT 时给出提示，避免静默套用错误效果。 */
  lutName: string | null;
}

function beautyTemplateOf(source: EditParams): Partial<BeautyParams> | null {
  const faces = source.faces.filter((face) => face.enabled);
  if (faces.length === 0) return null;
  const template: Partial<BeautyParams> = {};
  for (const field of BEAUTY_FIELDS) {
    let max = 0;
    for (const face of faces) max = Math.max(max, face.beauty[field] ?? 0);
    if (max > 0) template[field] = max;
  }
  return Object.keys(template).length > 0 ? template : null;
}

/** 从源照片提取勾选分组，供批量粘贴。 */
export function extractCopy(source: EditParams, groups: CopyGroupId[]): ParamCopy {
  const values: Partial<EditParams> = {};
  for (const group of COPY_GROUPS) {
    if (!groups.includes(group.id)) continue;
    for (const field of group.fields) {
      // 字段都在 EditParams 上；这里用受控复制，绝不展开整份对象。
      (values as Record<string, unknown>)[field] = source[field];
    }
  }
  return {
    groups: [...groups],
    values: structuredClone(values),
    beautyTemplate: groups.includes("beauty") ? beautyTemplateOf(source) : null,
    lutName: groups.includes("lut") ? source.lut?.name ?? null : null,
  };
}

export interface CopyResult {
  params: EditParams;
  warnings: string[];
}

/** 把参数快照合并到目标照片；美肤模板只作用于目标照片自己的人脸。 */
export function applyCopy(target: EditParams, copy: ParamCopy): CopyResult {
  const merged = cloneParams(target);
  Object.assign(merged, structuredClone(copy.values));
  const warnings: string[] = [];

  if (copy.lutName) {
    if (!target.lut) {
      warnings.push(`源照片使用 LUT「${copy.lutName}」，目标照片未选择 LUT，已跳过 LUT 参数`);
      merged.lut = null;
    } else if (target.lut.name !== copy.lutName) {
      warnings.push(
        `源照片 LUT「${copy.lutName}」与目标 LUT「${target.lut.name}」不同，请确认后再应用`,
      );
      merged.lut = structuredClone(target.lut);
    }
  }

  if (copy.beautyTemplate) {
    if (merged.faces.length === 0) {
      warnings.push("目标照片没有检测到人脸，美肤模板已跳过");
    } else {
      merged.faces = merged.faces.map((face) => ({
        ...face,
        beauty: { ...face.beauty, ...copy.beautyTemplate },
      }));
    }
  }
  return { params: merged, warnings };
}

/** 供 UI 展示：本次将覆盖的分组名称。 */
export function describeGroups(groups: CopyGroupId[]): string[] {
  return COPY_GROUPS.filter((group) => groups.includes(group.id)).map((group) => group.label);
}
