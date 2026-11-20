import { invoke } from "@tauri-apps/api/core";
import type {
  BeautyAnalysis,
  BeautyProgress,
  EditParams,
  ExportResult,
  ExportSettings,
  FaceDetectionResult,
  FaceModelStatus,
  FaceProgress,
  MaskPreview,
  PhotoAsset,
  ProjectDoc,
  ProjectOpenResult,
  RenderProgress,
  RenderResult,
  SkinModelStatus,
} from "../types";
import { isDesktopRuntime } from "./platform";

export interface DragDropHandlers {
  onEnter?: () => void;
  onLeave?: () => void;
  onDrop?: (paths: string[]) => void;
}

/** 导入照片：桌面端走系统文件选择器，浏览器端退化为 <input type=file>。 */
export async function pickImages(): Promise<PhotoAsset[]> {
  if (!isDesktopRuntime()) return pickImagesInBrowser();
  return invoke<PhotoAsset[]>("pick_images");
}

export async function importPaths(paths: string[]): Promise<PhotoAsset[]> {
  if (!isDesktopRuntime() || paths.length === 0) return [];
  return invoke<PhotoAsset[]>("load_images", { paths });
}

export async function renderPreview(
  path: string,
  params: EditParams,
  requestId: number,
  maxEdge?: number,
): Promise<RenderResult> {
  if (!isDesktopRuntime()) {
    throw new Error("浏览器预览模式不执行桌面图像管线，请使用桌面端运行");
  }
  return invoke<RenderResult>("render_preview", {
    requestId,
    path,
    params,
    maxEdge: maxEdge ?? null,
  });
}

export async function cancelRender(requestId: number): Promise<void> {
  if (!isDesktopRuntime()) return;
  await invoke("cancel_render", { requestId }).catch(() => undefined);
}

export async function exportImage(
  path: string,
  params: EditParams,
  settings: ExportSettings,
  destination?: string,
): Promise<ExportResult> {
  if (!isDesktopRuntime()) {
    throw new Error("导出需要在桌面端运行");
  }
  return invoke<ExportResult>("export_image", {
    requestId: Date.now() % 1_000_000_000,
    path,
    params,
    settings,
    destination: destination ?? null,
  });
}

export async function saveProject(
  project: ProjectDoc,
  destination?: string,
): Promise<{ path: string; bytes: number }> {
  if (!isDesktopRuntime()) throw new Error("保存项目需要在桌面端运行");
  return invoke("save_project", { project, destination: destination ?? null });
}

export async function openProject(destination?: string): Promise<ProjectOpenResult> {
  if (!isDesktopRuntime()) throw new Error("打开项目需要在桌面端运行");
  return invoke<ProjectOpenResult>("open_project", { destination: destination ?? null });
}

export async function pickLut(): Promise<{ path: string; name: string } | null> {
  if (!isDesktopRuntime()) return null;
  return invoke("pick_lut");
}

export async function autoWhiteBalance(path: string): Promise<{ temperature: number; tint: number }> {
  if (!isDesktopRuntime()) throw new Error("自动白平衡需要在桌面端运行");
  return invoke("auto_white_balance", { path });
}

export async function faceModelStatus(): Promise<FaceModelStatus> {
  if (!isDesktopRuntime()) {
    throw new Error("人脸检测需要在桌面端运行");
  }
  return invoke<FaceModelStatus>("face_model_status");
}

export async function detectFaces(path: string): Promise<FaceDetectionResult> {
  if (!isDesktopRuntime()) throw new Error("人脸检测需要在桌面端运行");
  return invoke<FaceDetectionResult>("detect_faces", { path });
}

export async function skinModelStatus(): Promise<SkinModelStatus> {
  if (!isDesktopRuntime()) {
    throw new Error("皮肤分割状态需要在桌面端读取");
  }
  return invoke<SkinModelStatus>("skin_model_status");
}

/** 一键美肤：真实执行检测 → 皮肤区域 → 指标分析，返回可编辑建议参数。 */
export async function analyzeBeauty(path: string): Promise<BeautyAnalysis> {
  if (!isDesktopRuntime()) throw new Error("AI 美肤需要在桌面端运行");
  return invoke<BeautyAnalysis>("analyze_beauty", { path });
}

/** 生成与渲染完全一致的皮肤蒙版可视化（用于蒙版显示与手动微调）。 */
export async function previewSkinMask(path: string, params: EditParams): Promise<MaskPreview> {
  if (!isDesktopRuntime()) throw new Error("皮肤蒙版预览需要在桌面端运行");
  return invoke<MaskPreview>("preview_skin_mask", { path, params });
}

export async function subscribeRenderProgress(
  handler: (progress: RenderProgress) => void,
): Promise<() => void> {
  if (!isDesktopRuntime()) return () => {};
  const event = await import("@tauri-apps/api/event");
  const unlisten = await event.listen<RenderProgress>("lumi://render-progress", (payload) =>
    handler(payload.payload),
  );
  return unlisten;
}

export async function subscribeFaceProgress(
  handler: (progress: FaceProgress) => void,
): Promise<() => void> {
  if (!isDesktopRuntime()) return () => {};
  const event = await import("@tauri-apps/api/event");
  const unlisten = await event.listen<FaceProgress>("lumi://face-progress", (payload) =>
    handler(payload.payload),
  );
  return unlisten;
}

export async function subscribeBeautyProgress(
  handler: (progress: BeautyProgress) => void,
): Promise<() => void> {
  if (!isDesktopRuntime()) return () => {};
  const event = await import("@tauri-apps/api/event");
  const unlisten = await event.listen<BeautyProgress>("lumi://beauty-progress", (payload) =>
    handler(payload.payload),
  );
  return unlisten;
}

/** 监听 Tauri 窗口拖拽文件事件，返回取消订阅函数。 */
export async function subscribeToFileDrop(handlers: DragDropHandlers): Promise<() => void> {
  if (!isDesktopRuntime()) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") {
      handlers.onEnter?.();
    } else if (payload.type === "leave") {
      handlers.onLeave?.();
    } else if (payload.type === "drop") {
      handlers.onDrop?.(payload.paths);
    }
  });
}

async function pickImagesInBrowser(): Promise<PhotoAsset[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/jpeg,image/png,image/webp";
    input.multiple = true;

    const finish = (assets: PhotoAsset[]) => {
      input.remove();
      resolve(assets);
    };

    input.onchange = async () => {
      const files = Array.from(input.files ?? []);
      const assets = await Promise.all(files.map(toBrowserAsset));
      finish(assets.filter((asset): asset is PhotoAsset => asset !== null));
    };
    input.oncancel = () => finish([]);
    input.click();
  });
}

async function toBrowserAsset(file: File): Promise<PhotoAsset | null> {
  if (!/^image\/(jpeg|png|webp)$/.test(file.type)) return null;
  try {
    const bitmap = await createImageBitmap(file);
    const url = URL.createObjectURL(file);
    const format = file.type === "image/png" ? "PNG" : file.type === "image/webp" ? "WebP" : "JPEG";
    return {
      id: `browser:${file.name}:${file.size}:${file.lastModified}`,
      path: file.name,
      fileName: file.name,
      format,
      bitDepth: 8,
      width: bitmap.width,
      height: bitmap.height,
      fileSize: file.size,
      hasAlpha: file.type === "image/png",
      thumbUrl: url,
      previewUrl: url,
      previewWidth: bitmap.width,
      previewHeight: bitmap.height,
    };
  } catch {
    return null;
  }
}
