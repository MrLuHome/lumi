import { invoke } from "@tauri-apps/api/core";
import type { EditParams, PhotoAsset } from "../types";
import { isDesktopRuntime } from "./platform";

export interface RenderResult {
  previewUrl: string;
  width: number;
  height: number;
  elapsedMs: number;
}

export interface ExportResult {
  path: string;
  bytes: number;
  width: number;
  height: number;
  elapsedMs: number;
}

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

export async function renderPreview(path: string, params: EditParams): Promise<RenderResult> {
  if (!isDesktopRuntime()) {
    throw new Error("浏览器预览模式不执行桌面图像管线，请使用桌面端运行");
  }
  return invoke<RenderResult>("render_preview", { path, params });
}

export async function exportImage(
  path: string,
  params: EditParams,
  destination?: string,
): Promise<ExportResult> {
  if (!isDesktopRuntime()) {
    throw new Error("导出需要在桌面端运行");
  }
  return invoke<ExportResult>("export_image", {
    path,
    params,
    destination: destination ?? null,
  });
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
