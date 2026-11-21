import type { ProjectDoc, ProjectPhoto } from "../types";
import { DEFAULT_COLOR_SETTINGS, DEFAULT_RAW_PARAMS } from "../types";
import type { EditorState } from "./editorStore";
import { DEFAULT_PARAMS, cloneParams } from "./params";

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
  const photos: ProjectPhoto[] = state.photos.map((photo) => {
    const raw = isRawPath(photo.path);
    const rawParams = { ...DEFAULT_RAW_PARAMS, ...(state.rawByPhoto[photo.id] ?? {}) };
    return {
      path: photo.path,
      params: cloneParams(state.params[photo.id] ?? DEFAULT_PARAMS),
      masks: [],
      export: state.exportSettings,
      sourceHash: state.projectSourceHashes[photo.id] ?? "",
      faces: state.projectFaceRecords[photo.id] ?? [],
      // 第五轮：人物/蒙版记录由 Rust 端保存时按参数同步，这里只提供占位。
      subjects: [],
      maskMeta: null,
      // 第七轮：来源与 RAW/色彩设置；非 RAW 不写 raw 字段。
      source: {
        sourceType: raw ? "raw" : "raster",
        path: photo.path,
        hash: state.projectSourceHashes[photo.id] ?? "",
        metadata: null,
      },
      raw: raw
        ? { params: rawParams, decoderVersion: "libraw-0.21.1+lumi-mhc-1" }
        : null,
      color: { output: state.colorSettings ?? DEFAULT_COLOR_SETTINGS },
    };
  });
  const found = state.photos.findIndex((photo) => photo.id === state.activeId);
  return {
    version: 3,
    createdAt: new Date().toISOString(),
    activeIndex: found >= 0 ? found : 0,
    export: state.exportSettings,
    compute: { preference: state.computePreference ?? "auto" },
    photos,
  };
}

/** 项目文件的默认保存文件名（仅用于文件对话框建议值）。 */
export function suggestedProjectName(state: EditorState): string {
  const first = state.photos[0];
  return first ? `${first.fileName.replace(/\.[^.]+$/, "")}.lumi` : "未命名项目.lumi";
}
