import type {
  ChangeOperation,
  ChangeSet,
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

/**
 * `buildProjectDoc` 实际读取的状态子集。
 * 收窄类型便于在测试中构造最小状态，也明确保存只依赖这些字段。
 */
export type ProjectDocSource = Pick<
  EditorState,
  | "photos"
  | "activeId"
  | "params"
  | "exportSettings"
  | "rawProjectPhotos"
  | "rawDoc"
  | "projectMeta"
  | "projectFaceRecords"
  | "projectSourceHashes"
  | "colorSettings"
  | "colorDirty"
  | "exportDirty"
  | "computePreference"
  | "rawByPhoto"
>;

/** 从当前编辑状态构建可保存的项目文件。原图路径只读引用，不包含像素。 */
export function buildProjectDoc(state: ProjectDocSource): ProjectDoc {
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

/** 有独立操作类型的参数子树，不放进 updatePhotoParams 的整体补丁。 */
const SPECIAL_PARAM_KEYS = new Set(["faces", "subjectMask", "manualMask", "background"]);

/**
 * 把“当前状态 vs 磁盘原始文档”的差异转换成类型化操作。
 *
 * 只产出真正变化的字段；未变化的字段、未知字段与派生字段（subjects）不会出现在
 * 补丁里，因此后端不需要整包覆盖就能得到正确结果。
 * 返回 `null` 表示没有可保存的增量（新项目首次保存仍走整包路径）。
 */
export function buildChangeSet(state: ProjectDocSource, requestId: string): ChangeSet | null {
  const baseDoc = state.rawDoc;
  if (!baseDoc || !baseDoc.projectUuid) return null;
  const current = buildProjectDoc(state);
  const operations: ChangeOperation[] = [];

  const baseMeta = {
    activeIndex: baseDoc.activeIndex ?? 0,
    export: baseDoc.export ?? DEFAULT_EXPORT,
    compute: baseDoc.compute ?? { preference: "auto" as const },
  };
  const nextMeta = {
    activeIndex: current.activeIndex,
    export: current.export ?? DEFAULT_EXPORT,
    compute: current.compute ?? null,
  };
  const metaPatch = buildFieldPatch(baseMeta, nextMeta);
  if (Object.keys(metaPatch).length > 0) {
    operations.push({ type: "updateDocMeta", patch: metaPatch });
  }

  const baseByPath = new Map((baseDoc.photos ?? []).map((entry) => [entry.path, entry]));
  for (const photo of current.photos) {
    const prior = baseByPath.get(photo.path);
    if (!prior) {
      operations.push({ type: "addPhoto", photo });
      continue;
    }
    pushPhotoOperations(operations, photo.path, prior, photo);
  }

  if (operations.length === 0) return null;
  return {
    projectId: baseDoc.projectUuid,
    baseRevision: state.projectMeta.revision || baseDoc.revision || 0,
    requestId,
    operations,
    createdAt: new Date().toISOString(),
    // 增量操作可以在最新状态上重新套用；绝不会用旧快照覆盖其他字段。
    rebaseOnConflict: true,
  };
}

function pushPhotoOperations(
  operations: ChangeOperation[],
  photo: string,
  prior: ProjectPhoto,
  current: ProjectPhoto,
): void {
  // 基线必须使用“加载时会归一化后的参数”，否则 clamp 造成的差异会被误判为修改。
  const priorParams = mergeUnknownKeys(clampParams(prior.params), prior.params);
  const paramsPatch = buildFieldPatch(
    omitKeys(priorParams, SPECIAL_PARAM_KEYS),
    omitKeys(current.params, SPECIAL_PARAM_KEYS),
  );
  if (Object.keys(paramsPatch).length > 0) {
    operations.push({ type: "updatePhotoParams", photo, patch: paramsPatch });
  }

  // 人脸美肤参数：按 faceUuid 精确匹配，新增/删除必须显式。
  const priorFaces = asObjectArray(priorParams.faces);
  const currentFaces = asObjectArray(current.params?.faces);
  const facesByUuid = new Map(
    priorFaces
      .filter((face) => typeof face.faceUuid === "string" && face.faceUuid)
      .map((face) => [face.faceUuid as string, face]),
  );
  for (const face of currentFaces) {
    const uuid = typeof face.faceUuid === "string" ? face.faceUuid : "";
    if (!uuid) continue;
    const old = facesByUuid.get(uuid);
    if (!old) {
      operations.push({ type: "addFace", photo, face });
    } else {
      const patch = buildFieldPatch(old, face);
      if (Object.keys(patch).length > 0) {
        operations.push({ type: "updateFaceParams", photo, faceUuid: uuid, patch });
      }
    }
    facesByUuid.delete(uuid);
  }
  for (const uuid of facesByUuid.keys()) {
    operations.push({ type: "removeFace", photo, faceUuid: uuid });
  }

  pushPatch(operations, "updateSubjectMask", photo, priorParams.subjectMask, current.params?.subjectMask);
  pushPatch(operations, "updateManualMask", photo, priorParams.manualMask, current.params?.manualMask);
  pushPatch(operations, "updateBackground", photo, priorParams.background, current.params?.background);

  if (prior.raw?.params && current.raw?.params) {
    const patch = buildFieldPatch(prior.raw.params, current.raw.params);
    if (Object.keys(patch).length > 0) {
      operations.push({ type: "updateRawParams", photo, patch });
    }
  }
  if (prior.color?.output && current.color?.output) {
    const patch = buildFieldPatch(prior.color.output, current.color.output);
    if (Object.keys(patch).length > 0) {
      operations.push({ type: "updateColorParams", photo, patch });
    }
  }
  if (prior.export && current.export) {
    const patch = buildFieldPatch(prior.export, current.export);
    if (Object.keys(patch).length > 0) {
      operations.push({ type: "updateExportParams", photo, patch });
    }
  }

  const sourcePatch = buildFieldPatch(prior.source ?? {}, current.source ?? {});
  if (Object.keys(sourcePatch).length > 0) {
    operations.push({ type: "updateSource", photo, patch: sourcePatch });
  }

  // 蒙版层：显式新增/更新/删除。
  const masksById = new Map(
    asObjectArray(prior.masks)
      .filter((mask) => typeof mask.id === "string" && mask.id)
      .map((mask) => [mask.id as string, mask]),
  );
  for (const mask of asObjectArray(current.masks)) {
    const id = typeof mask.id === "string" ? mask.id : "";
    if (!id) continue;
    const old = masksById.get(id);
    if (!old) {
      operations.push({ type: "addMask", photo, mask });
    } else {
      const patch = buildFieldPatch(old, mask);
      if (Object.keys(patch).length > 0) {
        operations.push({ type: "updateMask", photo, maskId: id, patch });
      }
    }
    masksById.delete(id);
  }
  for (const id of masksById.keys()) {
    operations.push({ type: "removeMask", photo, maskId: id });
  }

  // 持久化人脸记录：按 UUID upsert，删除显式。
  const recordsByUuid = new Map(
    asObjectArray(prior.faces)
      .filter((record) => typeof record.uuid === "string" && record.uuid)
      .map((record) => [record.uuid as string, record]),
  );
  for (const record of asObjectArray(current.faces)) {
    const uuid = typeof record.uuid === "string" ? record.uuid : "";
    if (!uuid) continue;
    const old = recordsByUuid.get(uuid);
    if (!old || !jsonEqual(old, record)) {
      operations.push({ type: "upsertFaceRecord", photo, record });
    }
    recordsByUuid.delete(uuid);
  }
  for (const uuid of recordsByUuid.keys()) {
    operations.push({ type: "removeFaceRecord", photo, faceUuid: uuid });
  }
}

function pushPatch(
  operations: ChangeOperation[],
  type: "updateSubjectMask" | "updateManualMask" | "updateBackground",
  photo: string,
  prior: unknown,
  current: unknown,
): void {
  if (!isPlainObject(current)) return;
  const patch = buildFieldPatch(isPlainObject(prior) ? prior : {}, current);
  if (Object.keys(patch).length > 0) {
    operations.push({ type, photo, patch });
  }
}

/**
 * 生成“只包含变化字段”的补丁：对象递归比较，数组/标量整体替换。
 * 数字使用 1e-4 容差（参数在 Rust 侧是 f32），其余严格比较。
 */
export function buildFieldPatch(base: unknown, current: unknown): Record<string, unknown> {
  if (!isPlainObject(current)) return {};
  if (!isPlainObject(base)) return cloneJson(current);
  const patch: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(current)) {
    if (!(key in base)) {
      patch[key] = cloneJson(value);
      continue;
    }
    const previous = base[key];
    if (isPlainObject(value) && isPlainObject(previous)) {
      const nested = buildFieldPatch(previous, value);
      if (Object.keys(nested).length > 0) patch[key] = nested;
      continue;
    }
    if (!jsonEqual(previous, value)) patch[key] = cloneJson(value);
  }
  return patch;
}

function omitKeys(value: unknown, keys: Set<string>): Record<string, unknown> {
  if (!isPlainObject(value)) return {};
  const out: Record<string, unknown> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (!keys.has(key)) out[key] = entry;
  }
  return out;
}

function asObjectArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value)
    ? value.filter((entry): entry is Record<string, unknown> => isPlainObject(entry))
    : [];
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function jsonEqual(a: unknown, b: unknown): boolean {
  if (typeof a === "number" && typeof b === "number") {
    return Math.abs(a - b) <= 1e-4;
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((entry, index) => jsonEqual(entry, b[index]));
  }
  if (isPlainObject(a) || isPlainObject(b)) {
    if (!isPlainObject(a) || !isPlainObject(b)) return false;
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    for (const key of keys) {
      if (!jsonEqual(a[key], b[key])) return false;
    }
    return true;
  }
  return a === b;
}

function cloneJson<T>(value: T): T {
  if (value === undefined) return value;
  return JSON.parse(JSON.stringify(value)) as T;
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

