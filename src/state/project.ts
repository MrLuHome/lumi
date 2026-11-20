import type { ProjectDoc, ProjectPhoto } from "../types";
import type { EditorState } from "./editorStore";
import { DEFAULT_PARAMS, cloneParams } from "./params";

/** 从当前编辑状态构建可保存的项目文件。原图路径只读引用，不包含像素。 */
export function buildProjectDoc(state: EditorState): ProjectDoc {
  const photos: ProjectPhoto[] = state.photos.map((photo) => ({
    path: photo.path,
    params: cloneParams(state.params[photo.id] ?? DEFAULT_PARAMS),
    masks: [],
    export: state.exportSettings,
  }));
  const found = state.photos.findIndex((photo) => photo.id === state.activeId);
  return {
    version: 1,
    createdAt: new Date().toISOString(),
    activeIndex: found >= 0 ? found : 0,
    export: state.exportSettings,
    photos,
  };
}

/** 项目文件的默认保存文件名（仅用于文件对话框建议值）。 */
export function suggestedProjectName(state: EditorState): string {
  const first = state.photos[0];
  return first ? `${first.fileName.replace(/\.[^.]+$/, "")}.lumi` : "未命名项目.lumi";
}
