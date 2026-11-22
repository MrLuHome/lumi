import type {
  ColorOutputSettings,
  ComputeBackend,
  ExportSettings,
  PhotoAsset,
  ProjectDoc,
  ProjectFaceRecord,
  ProjectPhoto,
  RawDevelopParams,
} from "../types";
import { DEFAULT_COLOR_SETTINGS, DEFAULT_RAW_PARAMS } from "../types";
import type { EditParams } from "./params";
import type { EditorState } from "./editorStore";
import { DEFAULT_EXPORT, DEFAULT_PARAMS, clampParams, cloneParams } from "./params";

const RAW_EXTENSIONS = [
  "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "rw2", "orf", "raf", "dng", "pef",
  "srw", "erf", "kdc", "mrw", "x3f", "3fr", "fff", "iiq", "mos", "mef", "rwl", "raw",
];

export function isRawPath(path: string): boolean {
  const match = /\.([a-z0-9]+)$/i.exec(path);
  return match ? RAW_EXTENSIONS.includes(match[1]!.toLowerCase()) : false;
}

/** 从当前编辑状态构建可保存的项目文件。原图路径只读引用，不包含像素。 */
export function buildProjectDoc(state: EditorState): ProjectDoc {
  const rawDoc = state.rawDoc ?? null;
  const rawByPath = new Map((rawDoc?.photos ?? []).map((photo) => [photo.path, photo]));
  const photos: ProjectPhoto[] = state.photos.map((photo) => {
    const raw = rawByPath.get(photo.path) ?? state.rawProjectPhotos[photo.id] ?? null;
    const active = photo.id === state.activeId;
    const rawParams = { ...DEFAULT_RAW_PARAMS, ...(state.rawByPhoto[photo.id] ?? {}) };
    // 先铺开磁盘原始记录（含未知字段），再用本次会话的已知字段覆盖。
    // 这样“打开→保存”不会把不认识的字段静默删掉。
    const params = mergeUnknownKeys(
      cloneParams(state.params[photo.id] ?? DEFAULT_PARAMS),
      raw?.params,
    );
    return {
      ...(raw ?? {}),
      path: photo.path,
      params,
      // 蒙版层是旧版可重放数据，当前面板不编辑它；保存必须原样保留。
      masks: Array.isArray(raw?.masks) ? raw!.masks : [],
      export: state.exportDirty && active
        ? mergeUnknownKeys({ ...state.exportSettings }, raw?.export)
        : raw?.export ?? state.exportSettings,
      sourceHash: state.projectSourceHashes[photo.id] ?? "",
      faces: state.projectFaceRecords[photo.id] ?? [],
      subjects: raw?.subjects ?? [],
      maskMeta: raw?.maskMeta ?? null,
      source: {
        ...(raw?.source ?? {}),
        sourceType: isRawPath(photo.path) ? "raw" : "raster",
        path: photo.path,
        hash: state.projectSourceHashes[photo.id] ?? "",
        // 相机/文件元数据快照只用于展示，保存时不得丢失。
        metadata: raw?.source?.metadata ?? null,
      },
      // 已存在的 RAW 记录必须原样保留（含解码器版本与未知字段）；
      // 非 RAW 照片只有在磁盘上确实带 raw 记录时才保留。
      raw:
        isRawPath(photo.path) || raw?.raw
          ? {
              ...(raw?.raw ?? {}),
              params: rawParams,
              decoderVersion: raw?.raw?.decoderVersion ?? "libraw-0.21.1+lumi-mhc-1",
            }
          : null,
      color: state.colorDirty && active
        ? {
            ...(raw?.color ?? {}),
            output: mergeUnknownKeys(
              { ...(state.colorSettings ?? DEFAULT_COLOR_SETTINGS) },
              raw?.color?.output,
            ),
          }
        : raw?.color ?? { output: state.colorSettings ?? DEFAULT_COLOR_SETTINGS },
    };
  });
  const found = state.photos.findIndex((photo) => photo.id === state.activeId);
  return {
    ...(rawDoc ?? {}),
    version: 3,
    // createdAt 是项目身份的一部分，重复保存不得改写。
    createdAt: state.projectMeta.createdAt || rawDoc?.createdAt || new Date().toISOString(),
    activeIndex: found >= 0 ? found : 0,
    export: state.exportSettings,
    compute: { ...(rawDoc?.compute ?? {}), preference: state.computePreference ?? "auto" },
    photos,
    projectUuid: state.projectMeta.projectUuid || rawDoc?.projectUuid || "",
    revision: state.projectMeta.revision || rawDoc?.revision || 0,
  };
}

/** 保留磁盘上存在、但当前类型不认识的顶层字段（只处理对象一级）。 */
function mergeUnknownKeys<T extends object>(live: T, raw: unknown): T {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return live;
  const merged: Record<string, unknown> = { ...(raw as Record<string, unknown>) };
  for (const [key, value] of Object.entries(live as Record<string, unknown>)) {
    merged[key] = value;
  }
  return merged as T;
}

/** 项目文件的默认保存文件名（仅用于文件对话框建议值）。 */
export function suggestedProjectName(state: EditorState): string {
  const first = state.photos[0];
  return first ? `${first.fileName.replace(/\.[^.]+$/, "")}.lumi` : "未命名项目.lumi";
}

/** 一次加载的完整快照。只有校验通过后才会原子提交到 store。 */
export interface ProjectSnapshot {
  projectId: string;
  photos: PhotoAsset[];
  params: Record<string, EditParams>;
  projectFaceRecords: Record<string, ProjectFaceRecord[]>;
  projectSourceHashes: Record<string, string>;
  /** 每张照片在磁盘上的原始记录，用于保存时保留蒙版/元数据/未知字段。 */
  rawPhotos: Record<string, ProjectPhoto>;
  rawDoc: ProjectDoc;
  projectMeta: ProjectMeta;
  rawByPhoto: Record<string, RawDevelopParams>;
  color: ColorOutputSettings | null;
  exportSettings: ExportSettings;
  computePreference: ComputeBackend;
  activeId: string;
  missing: string[];
  warnings: string[];
  /** 有照片的参数结构损坏，需按 Failed 处理而不是静默使用默认值。 */
  fatal: string | null;
}

/** 项目文件级身份信息：重复保存必须保持 createdAt / projectUuid 不变。 */
export interface ProjectMeta {
  projectUuid: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

const NUMERIC_PARAM_KEYS = [
  "exposure",
  "contrast",
  "highlights",
  "shadows",
  "whites",
  "blacks",
  "temperature",
  "tint",
  "saturation",
  "vibrance",
  "clarity",
  "sharpness",
  "grain",
  "vignette",
  "skinSmooth",
] as const;

/** 只做诊断，不修改数据。用于在提交前把“静默归零”变成可追踪的警告。 */
function scanPhotoIssues(entry: ProjectPhoto): string[] {
  const warnings: string[] = [];
  const name = entry.path.split(/[\\/]/).pop() ?? entry.path;
  const params = entry.params as Partial<EditParams> | undefined;
  if (!params || typeof params !== "object") {
    return [`${name}：缺少参数对象`];
  }
  for (const key of NUMERIC_PARAM_KEYS) {
    const value = params[key];
    if (typeof value !== "number" || !Number.isFinite(value)) {
      warnings.push(`${name}：参数 ${key} 缺失或非有限值`);
    }
  }
  if (Array.isArray(params.faces)) {
    for (const face of params.faces) {
      if (!face || typeof face !== "object" || !face.beauty) {
        warnings.push(`${name}：人脸参数缺少 beauty 结构`);
      }
    }
  }
  const records = Array.isArray(entry.faces) ? entry.faces : [];
  const uuids = new Set<string>();
  for (const record of records) {
    if (!record?.uuid) continue;
    if (uuids.has(record.uuid)) warnings.push(`${name}：人脸记录 UUID 重复`);
    uuids.add(record.uuid);
  }
  for (const face of Array.isArray(params.faces) ? params.faces : []) {
    if (face?.faceUuid && uuids.size > 0 && !uuids.has(face.faceUuid)) {
      warnings.push(`${name}：参数人脸 UUID 不在项目人脸记录中`);
    }
  }
  const masks = Array.isArray(entry.masks) ? entry.masks : [];
  const maskIds = new Set<string>();
  for (const mask of masks as Array<Record<string, unknown>>) {
    const id = typeof mask?.id === "string" ? mask.id : "";
    if (id) {
      if (maskIds.has(id)) warnings.push(`${name}：蒙版层 ID 重复`);
      maskIds.add(id);
    }
    const points = Array.isArray(mask?.points) ? mask.points : [];
    for (const point of points as Array<Record<string, unknown>>) {
      if (
        typeof point?.x !== "number" ||
        typeof point?.y !== "number" ||
        !Number.isFinite(point.x) ||
        !Number.isFinite(point.y)
      ) {
        warnings.push(`${name}：蒙版点坐标非法`);
        break;
      }
    }
  }
  return warnings;
}

/**
 * 构建加载快照：按路径严格匹配，不预填默认参数，不静默覆盖。
 * 任何异常都进入 warnings；结构性损坏进入 fatal。
 */
export function buildProjectSnapshot(
  items: PhotoAsset[],
  doc: ProjectDoc,
  projectId: string,
): ProjectSnapshot {
  const byPath = new Map(items.map((item) => [item.path, item]));
  const params: Record<string, EditParams> = {};
  const projectFaceRecords: Record<string, ProjectFaceRecord[]> = {};
  const projectSourceHashes: Record<string, string> = {};
  const rawPhotos: Record<string, ProjectPhoto> = {};
  const rawByPhoto: Record<string, RawDevelopParams> = {};
  const missing: string[] = [];
  const warnings: string[] = [];
  let fatal: string | null = null;
  let color: ColorOutputSettings | null = null;
  let exportSettings: ExportSettings = doc.export ?? DEFAULT_EXPORT;

  for (const entry of doc.photos) {
    const item = byPath.get(entry.path);
    if (!item) {
      missing.push(entry.path);
      continue;
    }
    if (!entry.params || typeof entry.params !== "object") {
      fatal = `${entry.path}：参数结构损坏`;
      continue;
    }
    warnings.push(...scanPhotoIssues(entry));
    params[item.id] = clampParams(entry.params);
    projectFaceRecords[item.id] = Array.isArray(entry.faces) ? entry.faces : [];
    projectSourceHashes[item.id] = entry.sourceHash ?? "";
    rawPhotos[item.id] = entry;
    if (entry.raw?.params) rawByPhoto[item.id] = { ...DEFAULT_RAW_PARAMS, ...entry.raw.params };
    if (!color && entry.color?.output) color = entry.color.output;
  }

  const matched = items.filter((item) => params[item.id]);
  const active =
    matched[Math.min(Math.max(doc.activeIndex ?? 0, 0), Math.max(matched.length - 1, 0))] ?? matched[0];
  if (!active && items.length > 0) {
    fatal = fatal ?? "项目中的照片路径与当前文件无法匹配";
  }
  // ROUND 9.1：会话里的色彩/导出设置必须来自**活动照片**，不能永远取第一张，
  // 否则保存会把第一张的设置写进当前照片。
  if (active) {
    const activeEntry = rawPhotos[active.id];
    color = activeEntry?.color?.output ?? color;
    if (activeEntry?.export) exportSettings = activeEntry.export as ExportSettings;
  }

  return {
    projectId,
    photos: items,
    params,
    projectFaceRecords,
    projectSourceHashes,
    rawPhotos,
    rawDoc: doc,
    projectMeta: {
      projectUuid: doc.projectUuid ?? "",
      revision: doc.revision ?? 0,
      createdAt: doc.createdAt ?? "",
      updatedAt: doc.updatedAt ?? "",
    },
    rawByPhoto,
    color,
    exportSettings,
    computePreference: doc.compute?.preference ?? "auto",
    activeId: active?.id ?? items[0]?.id ?? "",
    missing,
    warnings,
    fatal,
  };
}

/** 项目身份摘要：路径 + 创建时间 + 每张照片的来源哈希。 */
export async function computeProjectId(path: string, doc: ProjectDoc): Promise<string> {
  const seed = [path, doc.createdAt ?? "", ...doc.photos.map((photo) => photo.sourceHash ?? "")].join("\n");
  const subtle = globalThis.crypto?.subtle;
  if (subtle) {
    const bytes = new TextEncoder().encode(seed);
    const digest = await subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest).slice(0, 16))
      .map((value) => value.toString(16).padStart(2, "0"))
      .join("");
  }
  // 极少数环境没有 WebCrypto：退回 FNV-1a 64，仅用于标识，不用于安全用途。
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(seed)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

