import { afterEach, describe, expect, it, vi } from "vitest";
import {
  COALESCE_WINDOW_MS,
  ZOOM_MAX,
  createEditorStore,
  selectActiveParams,
  selectCanRedo,
  selectCanUndo,
  selectEffectUrl,
} from "./editorStore";
import { DEFAULT_PARAMS } from "./params";
import { buildProjectDoc } from "./project";
import type { PhotoAsset } from "../types";

function makePhoto(id: string, overrides: Partial<PhotoAsset> = {}): PhotoAsset {
  return {
    id,
    path: `/photos/${id}.jpg`,
    fileName: `${id}.jpg`,
    format: "JPEG",
    bitDepth: 8,
    width: 4000,
    height: 3000,
    fileSize: 4_200_000,
    hasAlpha: false,
    thumbUrl: "data:image/jpeg;base64,thumb",
    previewUrl: "data:image/jpeg;base64,preview",
    previewWidth: 1800,
    previewHeight: 1350,
    ...overrides,
  };
}

type Store = ReturnType<typeof createEditorStore>;

function paramsOf(store: Store, id = "a") {
  const value = store.getState().params[id];
  if (!value) throw new Error(`缺少 ${id} 的参数`);
  return value;
}

function historyOf(store: Store, id = "a") {
  const stack = store.getState().history[id];
  if (!stack) throw new Error(`缺少 ${id} 的历史`);
  return stack;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("编辑器状态", () => {
  it("导入后自动选中第一张并初始化参数与历史", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);

    expect(store.getState().activeId).toBe("a");
    expect(paramsOf(store)).toEqual(DEFAULT_PARAMS);
    expect(historyOf(store).past).toHaveLength(0);
    expect(selectActiveParams(store.getState())).toEqual(DEFAULT_PARAMS);
  });

  it("重复导入相同路径不会产生重复条目", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);

    expect(store.getState().photos.map((photo) => photo.id)).toEqual(["a", "b"]);
  });

  it("连续拖动滑杆只生成一条历史记录，撤销与重做生效", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-20T08:00:00Z"));
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);

    store.getState().updateParam("exposure", 10);
    vi.advanceTimersByTime(COALESCE_WINDOW_MS - 100);
    store.getState().updateParam("exposure", 30);

    expect(historyOf(store).past).toHaveLength(1);
    expect(historyOf(store).past[0]?.state).toEqual(DEFAULT_PARAMS);
    expect(paramsOf(store).exposure).toBe(30);
    expect(selectCanUndo(store.getState())).toBe(true);

    store.getState().undo();
    expect(paramsOf(store).exposure).toBe(0);
    expect(selectCanRedo(store.getState())).toBe(true);

    store.getState().redo();
    expect(paramsOf(store).exposure).toBe(30);
  });

  it("超过合并窗口后开始新的历史记录", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-20T08:00:00Z"));
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);

    store.getState().updateParam("contrast", 20);
    vi.advanceTimersByTime(COALESCE_WINDOW_MS + 100);
    store.getState().updateParam("contrast", 40);

    expect(historyOf(store).past).toHaveLength(2);
  });

  it("切换滑杆会分别记录历史", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-20T08:00:00Z"));
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);

    store.getState().updateParam("exposure", 20);
    store.getState().updateParam("grain", 25);

    expect(historyOf(store).past).toHaveLength(2);
  });

  it("参数值会被夹在合法范围内，重复赋值不产生历史", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);

    store.getState().updateParam("vignette", 999);
    expect(paramsOf(store).vignette).toBe(100);

    store.getState().updateParam("vignette", 100);
    expect(historyOf(store).past).toHaveLength(1);
  });

  it("新的编辑会清空重做栈", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().updateParam("exposure", 20);
    store.getState().undo();
    expect(selectCanRedo(store.getState())).toBe(true);

    store.getState().updateParam("shadows", 12);
    expect(selectCanRedo(store.getState())).toBe(false);
  });

  it("每张照片拥有独立的参数与历史", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);

    store.getState().updateParam("exposure", 40);
    store.getState().setActive("b");
    store.getState().updateParam("contrast", 15);

    expect(paramsOf(store, "a").exposure).toBe(40);
    expect(paramsOf(store, "b").contrast).toBe(15);
    expect(historyOf(store, "a").past).toHaveLength(1);
    expect(historyOf(store, "b").past).toHaveLength(1);
    expect(paramsOf(store, "b").exposure).toBe(0);
  });

  it("重置单组与重置全部各生成一条历史记录", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().updateParam("highlights", 20);

    store.getState().resetGroup("light");
    expect(paramsOf(store).highlights).toBe(0);
    expect(historyOf(store).past).toHaveLength(2);

    store.getState().updateParam("tint", 10);
    store.getState().resetAll();
    expect(paramsOf(store)).toEqual(DEFAULT_PARAMS);
    expect(historyOf(store).past).toHaveLength(4);
  });

  it("HSL 连续调整合并为一条历史，可撤销重做", () => {
    vi.useFakeTimers();
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().updateHsl("red", "hue", 10);
    store.getState().updateHsl("red", "hue", 25);
    expect(historyOf(store).past).toHaveLength(1);
    expect(paramsOf(store).hsl.red.hue).toBe(25);
    store.getState().undo();
    expect(paramsOf(store).hsl.red.hue).toBe(0);
    store.getState().redo();
    expect(paramsOf(store).hsl.red.hue).toBe(25);
    vi.useRealTimers();
  });

  it("参数可复制到剪贴板并粘贴到另一张照片", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);
    store.getState().updateParam("exposure", 40);
    store.getState().updateHsl("blue", "luminance", -20);
    store.getState().copyParams();

    store.getState().setActive("b");
    store.getState().pasteParams();
    expect(paramsOf(store, "b").exposure).toBe(40);
    expect(paramsOf(store, "b").hsl.blue.luminance).toBe(-20);
    expect(historyOf(store, "b").past).toHaveLength(1);

    // 粘贴后的参数与原照片相互独立。
    store.getState().updateParam("exposure", 5);
    expect(paramsOf(store, "a").exposure).toBe(40);
  });

  it("打开项目后恢复每张照片的参数与导出设置", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);
    store.getState().updateParam("exposure", 30);
    store.getState().setExportSettings({ format: "png", quality: 90 });
    const doc = buildProjectDoc(store.getState());
    expect(doc.photos).toHaveLength(2);
    expect(doc.photos[0]!.params.exposure).toBe(30);

    const reopened = createEditorStore();
    reopened.getState().loadProject([makePhoto("a"), makePhoto("b")], doc);
    expect(paramsOf(reopened, "a").exposure).toBe(30);
    expect(reopened.getState().exportSettings.format).toBe("png");
    expect(historyOf(reopened, "a").past).toHaveLength(0);
  });

  it("曲线重置会清空通道且可撤销", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setCurve("rgb", [
      { x: 0, y: 0 },
      { x: 0.5, y: 0.7 },
      { x: 1, y: 1 },
    ]);
    expect(paramsOf(store).curves.rgb).toHaveLength(3);
    store.getState().resetCurve("rgb");
    expect(paramsOf(store).curves.rgb).toHaveLength(0);
    store.getState().undo();
    expect(paramsOf(store).curves.rgb).toHaveLength(3);
  });

  it("移除照片时同步清理参数、历史与渲染缓存", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a"), makePhoto("b")]);
    store.getState().updateParam("exposure", 10);
    store.getState().markRendered("a", "effect-url", "key", 12);

    store.getState().removePhoto("a");
    const state = store.getState();
    expect(state.photos.map((photo) => photo.id)).toEqual(["b"]);
    expect(state.activeId).toBe("b");
    expect(state.params.a).toBeUndefined();
    expect(state.history.a).toBeUndefined();
    expect(state.renderedPreviews.a).toBeUndefined();
  });

  it("适应窗口与缩放会更新视图并夹紧范围", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setViewport({ width: 1000, height: 700 });
    store.getState().setFitZoom(0.15);

    expect(store.getState().zoom).toBeCloseTo(0.15);
    store.getState().setZoom(99);
    expect(store.getState().zoom).toBe(ZOOM_MAX);
    expect(store.getState().autoFit).toBe(false);

    store.getState().fitToWindow();
    expect(store.getState().zoom).toBeCloseTo(0.15);
    expect(store.getState().pan).toEqual({ x: 0, y: 0 });
  });

  it("适应窗口状态下图像不能被拖出视口", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setViewport({ width: 1000, height: 700 });
    store.getState().setFitZoom(0.15);
    store.getState().setPan({ x: 400, y: 400 });

    expect(store.getState().pan).toEqual({ x: 0, y: 0 });
  });

  it("效果图地址：未调整时复用原图，调整后使用渲染结果", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    expect(selectEffectUrl(store.getState())).toBe("data:image/jpeg;base64,preview");

    store.getState().updateParam("exposure", 20);
    expect(selectEffectUrl(store.getState())).toBe("data:image/jpeg;base64,preview");

    store.getState().markRendered("a", "data:image/jpeg;base64,effect", "key", 20);
    expect(selectEffectUrl(store.getState())).toBe("data:image/jpeg;base64,effect");
  });

  it("对比模式与分割线状态可切换并被夹紧", () => {
    const store = createEditorStore();
    store.getState().toggleCompare();
    expect(store.getState().compareMode).toBe("split");
    store.getState().setCompareSplit(1.4);
    expect(store.getState().compareSplit).toBe(0.98);
    store.getState().setCompareSplit(0);
    expect(store.getState().compareSplit).toBe(0.02);
    store.getState().toggleCompare();
    expect(store.getState().compareMode).toBe("off");
  });
});

describe("美肤参数与蒙版历史", () => {
  it("按稳定 ID 修改每张脸，不互相污染且可撤销重做", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().updateFaceBeauty(0, "skinSmooth", 25);
    store.getState().updateFaceBeauty(1, "skinSmooth", 45);
    store.getState().updateFaceBeauty(1, "blemish", 70);

    let params = paramsOf(store);
    expect(params.faces.find((face) => face.faceId === 0)?.beauty.skinSmooth).toBe(25);
    expect(params.faces.find((face) => face.faceId === 1)?.beauty.skinSmooth).toBe(45);
    expect(params.faces.find((face) => face.faceId === 1)?.beauty.blemish).toBe(70);

    store.getState().undo();
    params = paramsOf(store);
    expect(params.faces.find((face) => face.faceId === 1)?.beauty.blemish).toBe(0);
    store.getState().undo();
    store.getState().undo();
    expect(paramsOf(store).faces.every((face) => face.beauty.skinSmooth === 0)).toBe(true);
    store.getState().redo();
    store.getState().redo();
    expect(paramsOf(store).faces.find((face) => face.faceId === 1)?.beauty.skinSmooth).toBe(45);
  });

  it("手动蒙版可逐笔撤销与恢复", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    const stroke = { erase: false, radius: 0.03, points: [{ x: 0.5, y: 0.5 }] };
    store.getState().appendMaskStroke(stroke);
    store.getState().appendMaskStroke({ ...stroke, erase: true });
    expect(paramsOf(store).manualMask.strokes).toHaveLength(2);
    expect(paramsOf(store).manualMask.enabled).toBe(true);

    store.getState().undoMaskStroke();
    expect(paramsOf(store).manualMask.strokes).toHaveLength(1);
    store.getState().clearMaskStrokes();
    expect(paramsOf(store).manualMask.strokes).toHaveLength(0);
  });

  it("手动修复点可单独撤销", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().addRepairPoint(0.4, 0.5, 0.03);
    store.getState().addRepairPoint(0.6, 0.5, 0.03);
    expect(paramsOf(store).repairPoints).toHaveLength(2);
    store.getState().undoRepairPoint();
    expect(paramsOf(store).repairPoints).toHaveLength(1);
    expect(paramsOf(store).repairPoints[0]?.x).toBe(0.4);
  });

  it("RAW 显影参数按照片保存并可重置", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("raw", { path: "/photos/raw.CR3" })]);
    const id = store.getState().activeId!;
    expect(store.getState().rawByPhoto[id]).toBeUndefined();
    store.getState().setRawParams(id, { exposureEv: 1.2, wbMode: "custom", temperature: 5200 });
    expect(store.getState().rawByPhoto[id]?.exposureEv).toBe(1.2);
    expect(store.getState().rawByPhoto[id]?.wbMode).toBe("custom");
    expect(store.getState().rawDirty).toBe(true);
    store.getState().resetRawParams(id);
    expect(store.getState().rawByPhoto[id]).toBeUndefined();
    expect(store.getState().rawDirty).toBe(false);
  });

  it("项目文件写入 RAW/色彩/计算字段，非 RAW 不写 raw", () => {
    const store = createEditorStore();
    store.getState().addPhotos([
      makePhoto("raw", { path: "/photos/raw.CR3" }),
      makePhoto("jpg"),
    ]);
    const rawId = store.getState().photos[0]!.id;
    store.getState().setRawParams(rawId, { exposureEv: 0.5 });
    store.getState().setColorSettings({
      profile: "displayP3",
      customPath: null,
      intent: "perceptual",
      blackPointCompensation: true,
      embed: true,
    });
    store.getState().setComputePreference("gpu");
    const doc = buildProjectDoc(store.getState());
    expect(doc.version).toBe(3);
    expect(doc.compute?.preference).toBe("gpu");
    expect(doc.photos[0]?.source?.sourceType).toBe("raw");
    expect(doc.photos[0]?.raw?.params.exposureEv).toBe(0.5);
    expect(doc.photos[0]?.color?.output.profile).toBe("displayP3");
    expect(doc.photos[1]?.raw).toBeNull();
  });
});

describe("Provider 切换后的本地人脸数据", () => {
  it("清空稠密关键点与 LEVEL 3 门控，但保留 5 点检测结果", () => {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setFaces("a", {
      faces: [
        {
          id: 1,
          uuid: "face-uuid",
          score: 0.9,
          x: 0.1,
          y: 0.1,
          width: 0.2,
          height: 0.2,
          landmarks: [],
          dense: { regions: [] } as never,
          level3: { providerId: "apple-vision" } as never,
        },
      ],
      imageWidth: 100,
      imageHeight: 100,
      elapsedMs: 1,
      inferenceMs: 1,
      model: "apple-vision",
    });

    store.getState().clearFaceAnalyses("a");
    const face = store.getState().faces.a?.faces[0];
    expect(face?.dense).toBeNull();
    expect(face?.level3).toBeNull();
    expect(face?.landmarks).toEqual([]);
    expect(face?.uuid).toBe("face-uuid");
  });
});
