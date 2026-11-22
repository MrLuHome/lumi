import { invoke } from "@tauri-apps/api/core";
import type {
  AnalysisResponse,
  AppliedSuggestions,
  BackgroundImageInfo,
  BatchExportRequest,
  BatchSnapshot,
  BatchTask,
  BeautyAnalysis,
  BeautyProgress,
  BodyAnchorPreset,
  BodyCapability,
  ColorOutputSettings,
  ColorStatus,
  ComputeBackend,
  DisplayPreviewResult,
  DisplayStatus,
  EditParams,
  ExportResult,
  ExportSettings,
  FaceDetectionResult,
  FaceModelStatus,
  FaceProgress,
  FaceRestoreResult,
  GpuBenchmarkResult,
  GpuStatus,
  MaskPreview,
  MeshPreview,
  PhotoAsset,
  PresetList,
  PresetParams,
  PresetPreviews,
  PresetSaved,
  ProjectDoc,
  ProjectFaceRecord,
  ProjectOpenResult,
  ProfileSummary,
  RawCapabilities,
  RawDevelopParams,
  RawDevelopResult,
  RawInfo,
  RenderProgress,
  RenderResult,
  ReshapeCapability,
  RetouchMode,
  RetouchSuggestion,
  SegmentationCapability,
  SkinModelStatus,
  SoftProofResult,
  SubjectMaskPreview,
  SubjectMaskParams,
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
  rawParams?: RawDevelopParams | null,
): Promise<RenderResult> {
  if (!isDesktopRuntime()) {
    throw new Error("浏览器预览模式不执行桌面图像管线，请使用桌面端运行");
  }
  return invoke<RenderResult>("render_preview", {
    requestId,
    path,
    params,
    maxEdge: maxEdge ?? null,
    rawParams: rawParams ?? null,
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
  rawParams?: RawDevelopParams | null,
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
    rawParams: rawParams ?? null,
  });
}

/** 第七轮：RAW 元数据、能力矩阵与显影预览。 */
export async function rawInfo(path: string): Promise<RawInfo> {
  if (!isDesktopRuntime()) throw new Error("RAW 解析需要在桌面端运行");
  return invoke<RawInfo>("raw_info", { path });
}

export async function rawPreview(
  path: string,
  params?: Partial<RawDevelopParams> | null,
): Promise<RawDevelopResult> {
  if (!isDesktopRuntime()) throw new Error("RAW 显影需要在桌面端运行");
  return invoke<RawDevelopResult>("raw_preview", { path, params: params ?? null });
}

export async function rawCapabilities(): Promise<RawCapabilities> {
  return invoke<RawCapabilities>("raw_capabilities");
}

/** 第七轮：色彩管理与输出 Profile。 */
export async function colorStatus(): Promise<ColorStatus> {
  return invoke<ColorStatus>("color_status");
}

export async function colorSetOutput(settings: ColorOutputSettings): Promise<ColorStatus> {
  return invoke<ColorStatus>("color_set_output", { settings });
}

export async function colorCheckProfile(path: string): Promise<ProfileSummary> {
  return invoke<ProfileSummary>("color_check_profile", { path });
}

export async function displayProfileRefresh(): Promise<DisplayStatus> {
  return invoke<DisplayStatus>("display_profile_refresh");
}

/** 软打样：当前照片渲染后做输出 Profile 往返，返回预览与色域统计。 */
export async function softProof(
  path: string,
  params: EditParams,
  warn = true,
): Promise<SoftProofResult> {
  return invoke<SoftProofResult>("soft_proof", { path, params, warn });
}

/** 显示器色彩预览：sRGB → ColorSync 显示器 Profile，PNG 内嵌显示 ICC。 */
export async function displayPreview(
  path: string,
  params: EditParams,
): Promise<DisplayPreviewResult> {
  return invoke<DisplayPreviewResult>("display_preview", { path, params });
}

/** 第七轮：计算后端状态与基准。 */
export async function gpuStatus(): Promise<GpuStatus> {
  return invoke<GpuStatus>("gpu_status");
}

export async function gpuBenchmark(): Promise<GpuBenchmarkResult> {
  return invoke<GpuBenchmarkResult>("gpu_benchmark");
}

export async function setComputeBackend(preference: ComputeBackend): Promise<GpuStatus> {
  return invoke<GpuStatus>("set_compute_backend", { preference });
}

export async function saveProject(
  project: ProjectDoc,
  destination?: string,
): Promise<{ path: string; bytes: number; projectUuid: string; revision: number }> {
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
  if (!isDesktopRuntime()) throw new Error("美肤分析需要在桌面端运行");
  return invoke<BeautyAnalysis>("analyze_beauty", { path });
}

/** 生成与渲染完全一致的皮肤蒙版可视化（用于蒙版显示与手动微调）。 */
export async function previewSkinMask(path: string, params: EditParams): Promise<MaskPreview> {
  if (!isDesktopRuntime()) throw new Error("皮肤蒙版预览需要在桌面端运行");
  return invoke<MaskPreview>("preview_skin_mask", { path, params });
}

/** 当前真实可用的塑形能力（LEVEL 1 + LEVEL 2）。 */
export async function reshapeCapability(): Promise<ReshapeCapability> {
  if (!isDesktopRuntime()) throw new Error("塑形能力查询需要在桌面端运行");
  return invoke<ReshapeCapability>("reshape_capability");
}

/** 第五轮：手动人物蒙版能力与 AI 分割不可用原因。 */
export async function segmentationCapability(): Promise<SegmentationCapability> {
  if (!isDesktopRuntime()) {
    return {
      aiAvailable: false,
      manualAvailable: true,
      providers: [],
      note: "手动人物蒙版：本地笔触 + Alpha 处理，非 AI 自动分割",
    };
  }
  return invoke<SegmentationCapability>("segmentation_capability");
}

/** 生成手动蒙版预览；`mode` 为 overlay（红色遮罩）或 alpha（黑白）。 */
export async function previewSubjectMask(
  path: string,
  params: SubjectMaskParams,
  mode: "overlay" | "alpha" = "overlay",
): Promise<SubjectMaskPreview> {
  if (!isDesktopRuntime()) throw new Error("人物蒙版预览需要在桌面端运行");
  return invoke<SubjectMaskPreview>("preview_subject_mask", { path, params, mode });
}

/** 第五轮：身材能力与姿态模型不可用原因。 */
export async function bodyCapability(): Promise<BodyCapability> {
  if (!isDesktopRuntime()) {
    return {
      poseAvailable: false,
      provider: "disabled",
      license: "",
      reason: "浏览器模式不执行本地算法",
      note: "身材调整使用手动蒙版轮廓 + 用户锚点，不依赖 AI 姿态。",
    };
  }
  return invoke<BodyCapability>("body_capability");
}

/** 根据手动蒙版轮廓给出锚点建议；锚点仍需用户确认。 */
export async function bodyAnchorPresets(
  path: string,
  mask: SubjectMaskParams,
): Promise<BodyAnchorPreset[]> {
  if (!isDesktopRuntime()) throw new Error("锚点建议需要在桌面端运行");
  return invoke<BodyAnchorPreset[]>("body_anchor_presets", { path, mask });
}

/** 选择本地背景图片（不复制、不上传）。 */
export async function pickBackgroundImage(): Promise<BackgroundImageInfo | null> {
  if (!isDesktopRuntime()) return null;
  return invoke<BackgroundImageInfo | null>("pick_background_image");
}

/** 项目重开时校验背景图片是否仍可用。 */
export async function backgroundImageInfo(path: string): Promise<BackgroundImageInfo> {
  if (!isDesktopRuntime()) throw new Error("背景图片校验需要在桌面端运行");
  return invoke<BackgroundImageInfo>("background_image_info", { path });
}

/** 获取真实控制网格，用于画布网格显示。 */
export async function reshapeMesh(
  path: string,
  faceId: number,
  params: EditParams,
): Promise<MeshPreview | null> {
  if (!isDesktopRuntime()) return null;
  return invoke<MeshPreview | null>("reshape_mesh", { path, faceId, params });
}

/** 保存项目前读取持久化人脸记录（UUID / 框 / 关键点 / 模型版本）。 */
export async function exportFaceRecords(
  path: string,
): Promise<{ sourceHash: string; faces: ProjectFaceRecord[] }> {
  if (!isDesktopRuntime()) return { sourceHash: "", faces: [] };
  return invoke("export_face_records", { path });
}

/** 项目重开：重新检测后按几何匹配并恢复稳定 UUID。 */
export async function restoreFaces(
  path: string,
  records: ProjectFaceRecord[],
  sourceHash?: string,
): Promise<FaceRestoreResult> {
  if (!isDesktopRuntime()) {
    return {
      restored: [],
      pending: [],
      missing: records,
      sourceChanged: false,
      faces: [],
      discardedDense: 0,
    };
  }
  return invoke<FaceRestoreResult>("restore_faces", {
    path,
    records,
    sourceHash: sourceHash ?? null,
  });
}

/** 人工确认低置信匹配：把项目中的 UUID 绑定到当前检测脸。 */
export async function confirmFaceMatch(
  path: string,
  faceId: number,
  faceUuid: string,
): Promise<{ faces: FaceDetectionResult["faces"] }> {
  if (!isDesktopRuntime()) throw new Error("人脸确认需要在桌面端运行");
  return invoke("confirm_face_match", { path, faceId, faceUuid });
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

/** 本地分析：真实统计 + 人脸检测结果，生成可编辑建议。 */
export async function analyzeRetouch(
  path: string,
  params: EditParams,
  mode: RetouchMode,
): Promise<AnalysisResponse> {
  if (!isDesktopRuntime()) throw new Error("智能精修建议需要在桌面端运行");
  return invoke<AnalysisResponse>("analyze_retouch", { path, params, mode });
}

/** 把用户勾选的建议逐字段合并为普通参数，可继续编辑并撤销。 */
export async function applyRetouchSuggestions(
  path: string,
  params: EditParams,
  suggestions: RetouchSuggestion[],
): Promise<AppliedSuggestions> {
  if (!isDesktopRuntime()) throw new Error("应用建议需要在桌面端运行");
  return invoke<AppliedSuggestions>("apply_retouch_suggestions", {
    path,
    params,
    suggestions,
  });
}

export async function listPresets(): Promise<PresetList> {
  if (!isDesktopRuntime()) return { presets: [], schemaVersion: 1 };
  return invoke<PresetList>("list_presets");
}

/** 用当前照片真实渲染每个预设的缩略图。 */
export async function presetPreviews(
  path: string,
  params: EditParams,
  size?: number,
): Promise<PresetPreviews> {
  if (!isDesktopRuntime()) return { previews: [], failed: [] };
  return invoke<PresetPreviews>("preset_previews", { path, params, size: size ?? null });
}

/** 单张预设实时预览：只在缩略尺寸上渲染，不触发全尺寸运算。 */
export async function presetPreview(
  path: string,
  params: EditParams,
  presetId: string,
  strength: number,
  size?: number,
): Promise<{ url: string; warnings: string[] }> {
  if (!isDesktopRuntime()) throw new Error("预设预览需要在桌面端运行");
  return invoke("preset_preview", { path, params, presetId, strength, size: size ?? null });
}

/** 把当前照片参数提取为可保存的预设子集（不含对象级数据）。 */
export async function presetParamsFromEdit(params: EditParams): Promise<PresetParams> {
  if (!isDesktopRuntime()) throw new Error("保存预设需要在桌面端运行");
  return invoke<PresetParams>("preset_params_from_edit", { params });
}

export async function applyPresetToPhoto(
  path: string,
  params: EditParams,
  presetId: string,
  strength: number,
): Promise<AppliedSuggestions> {
  if (!isDesktopRuntime()) throw new Error("应用预设需要在桌面端运行");
  return invoke<AppliedSuggestions>("apply_preset_to_photo", {
    path,
    params,
    presetId,
    strength,
  });
}

export async function saveCustomPreset(
  presetId: string | null,
  name: string,
  description: string,
  params: PresetParams,
): Promise<PresetSaved> {
  if (!isDesktopRuntime()) throw new Error("保存预设需要在桌面端运行");
  return invoke<PresetSaved>("save_custom_preset", {
    presetId,
    name,
    description,
    params,
  });
}

export async function deleteCustomPreset(presetId: string): Promise<void> {
  if (!isDesktopRuntime()) throw new Error("删除预设需要在桌面端运行");
  await invoke("delete_custom_preset", { presetId });
}

export async function duplicateCustomPreset(
  presetId: string,
  name: string,
): Promise<PresetSaved> {
  if (!isDesktopRuntime()) throw new Error("复制预设需要在桌面端运行");
  return invoke<PresetSaved>("duplicate_custom_preset", { presetId, name });
}

export async function renameCustomPreset(
  presetId: string,
  name: string,
): Promise<PresetSaved> {
  if (!isDesktopRuntime()) throw new Error("重命名预设需要在桌面端运行");
  return invoke<PresetSaved>("rename_custom_preset", { presetId, name });
}

export async function exportPreset(
  presetId: string,
  destination?: string,
): Promise<{ path: string; preset: unknown }> {
  if (!isDesktopRuntime()) throw new Error("导出预设需要在桌面端运行");
  return invoke("export_preset", { presetId, destination: destination ?? null });
}

export async function importPreset(source?: string): Promise<PresetSaved> {
  if (!isDesktopRuntime()) throw new Error("导入预设需要在桌面端运行");
  return invoke<PresetSaved>("import_preset", { source: source ?? null });
}

export async function batchStart(request: BatchExportRequest): Promise<BatchSnapshot> {
  if (!isDesktopRuntime()) throw new Error("批量导出需要在桌面端运行");
  return invoke<BatchSnapshot>("batch_start", { request });
}

export async function batchStatus(): Promise<BatchSnapshot> {
  if (!isDesktopRuntime()) throw new Error("批量任务状态需要在桌面端读取");
  return invoke<BatchSnapshot>("batch_status");
}

export async function batchPause(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_pause");
}

export async function batchResume(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_resume");
}

export async function batchCancelCurrent(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_cancel_current");
}

export async function batchCancelAll(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_cancel_all");
}

export async function batchRetryFailed(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_retry_failed");
}

export async function batchClearCompleted(): Promise<BatchSnapshot> {
  return invoke<BatchSnapshot>("batch_clear_completed");
}

export async function batchFailures(): Promise<BatchTask[]> {
  return invoke<BatchTask[]>("batch_failures");
}

export async function pickBatchOutputDir(): Promise<string | null> {
  if (!isDesktopRuntime()) return null;
  return invoke<string | null>("pick_batch_output_dir");
}

export async function subscribeBatchProgress(
  handler: (snapshot: BatchSnapshot) => void,
): Promise<() => void> {
  if (!isDesktopRuntime()) return () => {};
  const event = await import("@tauri-apps/api/event");
  const unlisten = await event.listen<BatchSnapshot>("lumi://batch-progress", (payload) =>
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

/** 第八轮：人脸智能 Provider 与 Model Hub。 */
export async function listFaceProviders(): Promise<import("../types").ProviderDescriptor[]> {
  if (!isDesktopRuntime()) return [];
  return invoke("list_face_providers");
}

export async function faceIntelligenceStatus(): Promise<import("../types").FaceIntelligenceStatus> {
  return invoke("face_intelligence_status");
}

export async function selectFaceProvider(
  providerId: string,
): Promise<import("../types").ProviderSwitchReport> {
  return invoke("select_face_provider", { providerId });
}

export async function analyzePhotoFaces(path: string): Promise<import("../types").AnalyzeFacesReport> {
  return invoke("analyze_photo_faces", { path });
}

export async function probeFaceQuality(
  path: string,
  runs = 2,
): Promise<import("../types").QualityReport> {
  return invoke("probe_face_quality", { path, runs });
}

export async function faceQualityThresholds(): Promise<import("../types").QualityThresholds> {
  return invoke("face_quality_thresholds");
}

export async function listFaceModels(): Promise<import("../types").InstalledModel[]> {
  if (!isDesktopRuntime()) return [];
  return invoke("list_face_models");
}

export async function pickFaceModelFile(): Promise<string | null> {
  if (!isDesktopRuntime()) return null;
  return invoke("pick_face_model_file");
}

export async function importFaceModel(
  request: import("../types").ImportFaceModelInput,
): Promise<import("../types").InstalledModel> {
  return invoke("import_face_model", { request });
}

export async function revalidateFaceModel(modelId: string): Promise<import("../types").InstalledModel> {
  return invoke("revalidate_face_model", { modelId });
}

export async function removeFaceModel(modelId: string): Promise<void> {
  return invoke("remove_face_model", { modelId });
}
