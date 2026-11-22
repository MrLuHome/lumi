import { describe, expect, it } from "vitest";
import { createEditorStore } from "./editorStore";
import { buildProjectDoc, buildProjectSnapshot } from "./project";
import { clampParams, defaultFaceBeauty, defaultFaceReshape, defaultParams } from "./params";
import type { EditParams } from "./params";
import type { PhotoAsset, ProjectDoc, ProjectPhoto } from "../types";

const PHOTO_A = "photo-a";
const PHOTO_B = "photo-b";
const UUID_A = "18e9fd0f-9771-49b8-8001-df85c8fbfc19";
const UUID_B = "28e9fd0f-9771-49b8-8001-df85c8fbfc20";

function makePhoto(id: string, path: string): PhotoAsset {
  return {
    id,
    path,
    fileName: path.split("/").pop() ?? path,
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
  };
}

/** 覆盖本轮要求的所有数据类别，并附带旧客户端不认识的字段。 */
function richParams(): EditParams {
  const base = defaultParams();
  return {
    ...base,
    exposure: 24,
    contrast: -8,
    temperature: 12,
    tint: 3,
    highlights: -20,
    shadows: 15,
    whites: 5,
    blacks: -6,
    saturation: 9,
    vibrance: 11,
    clarity: 8,
    sharpness: 12,
    grain: 4,
    vignette: 7,
    skinSmooth: 5,
    faces: [
      {
        ...defaultFaceBeauty(101, UUID_A),
        beauty: { ...defaultFaceBeauty(101, UUID_A).beauty, skinSmooth: 17, texture: 33, spots: 11 },
      },
      {
        ...defaultFaceBeauty(202, UUID_B),
        beauty: { ...defaultFaceBeauty(202, UUID_B).beauty, whitening: 44, nasolabial: 22 },
      },
    ],
    reshape: [
      {
        ...defaultFaceReshape(101, UUID_A),
        enabled: true,
        face: { ...defaultFaceReshape(101, UUID_A).face, slim: 22, jaw: -11 },
        eyes: { ...defaultFaceReshape(101, UUID_A).eyes, size: 14 },
        nose: { ...defaultFaceReshape(101, UUID_A).nose, tip: -6 },
        mouth: { ...defaultFaceReshape(101, UUID_A).mouth, width: 5 },
        chin: { ...defaultFaceReshape(101, UUID_A).chin, length: 9 },
        manual: { enabled: false, strokes: [] },
      },
    ],
    subjectMask: {
      ...base.subjectMask,
      enabled: true,
      feather: 22,
      expand: -14,
      strokes: [
        {
          kind: "paint",
          radius: 0.12,
          hardness: 55,
          strength: 90,
          points: [
            { x: 0.32, y: 0.44 },
            { x: 0.36, y: 0.48 },
          ],
        } as EditParams["subjectMask"]["strokes"][number],
      ],
    },
    body: [
      {
        subjectId: "subject-1",
        enabled: true,
        shape: { waist: -30, shoulders: 12, hips: 8, thighs: 0, calves: 0, arms: 0, legs: 0 },
        anchors: [
          { part: "waistLeft", x: 0.3, y: 0.6, dx: -0.03, dy: 0.01, radius: 0.12, locked: true },
        ],
        strokes: [],
        maskStrokes: [],
      } as EditParams["body"][number],
    ],
    background: {
      ...base.background,
      enabled: true,
      mode: "solidColor",
      color: "#102030",
      blur: 62,
      depth: 41,
    },
    manualMask: {
      enabled: true,
      invert: false,
      feather: 33,
      strength: 88,
      strokes: [
        { erase: false, radius: 0.05, points: [{ x: 0.5, y: 0.5 }] },
        { erase: true, radius: 0.03, points: [{ x: 0.52, y: 0.52 }] },
      ],
    },
    repairPoints: [{ x: 0.4, y: 0.6, radius: 0.02 }],
    hsl: {
      ...base.hsl,
      orange: { hue: 12, saturation: 18, luminance: -4 },
    },
    curves: {
      ...base.curves,
      rgb: [
        { x: 0, y: 0 },
        { x: 0.5, y: 0.55 },
        { x: 1, y: 1 },
      ],
    },
    lut: { path: "/luts/portrait.cube", name: "portrait", strength: 65 },
    // 未知参数字段：新版本写下的字段，旧客户端保存时不得丢弃。
    futureParamField: { enabled: true, strength: 0.5 },
  } as EditParams;
}

function richPhoto(path: string): ProjectPhoto {
  return {
    path,
    params: richParams(),
    masks: [
      {
        id: "mask-1",
        name: "面部提亮",
        kind: "radial",
        enabled: true,
        invert: false,
        feather: 30,
        opacity: 80,
        points: [{ x: 0.5, y: 0.4, radius: 0.2 }],
      },
      {
        id: "mask-2",
        name: "皮肤蒙版",
        kind: "brush",
        enabled: true,
        invert: true,
        feather: 12,
        opacity: 60,
        points: [
          { x: 0.2, y: 0.2, radius: 0.05 },
          { x: 0.3, y: 0.3, radius: 0.05 },
        ],
      },
    ],
    export: { format: "png", quality: 88, resize: null, filenameTemplate: "a", watermark: null } as never,
    sourceHash: "sha256-a",
    faces: [
      {
        uuid: UUID_A,
        bbox: { x: 0.1, y: 0.1, width: 0.2, height: 0.24 },
        landmarks: [
          { x: 0.15, y: 0.2 },
          { x: 0.25, y: 0.2 },
          { x: 0.2, y: 0.25 },
          { x: 0.17, y: 0.29 },
          { x: 0.23, y: 0.29 },
        ],
        score: 0.94,
        model: "yunet-5",
        confidence: 0.94,
        maskRef: "mask-1",
        dense: {
          topologyId: "face-68",
          providerId: "apple-vision",
          providerVersion: "vision-rev3",
          imageWidth: 4000,
          imageHeight: 3000,
          boundingBox: { x: 0.1, y: 0.1, width: 0.2, height: 0.24 },
          confidence: 0.91,
          regions: [
            {
              id: "eye.left",
              points: Array.from({ length: 6 }, (_, index) => ({ x: 0.15 + index * 0.001, y: 0.2 })),
            },
            {
              id: "nose.contour",
              points: Array.from({ length: 8 }, (_, index) => ({ x: 0.18 + index * 0.001, y: 0.23 })),
            },
          ],
        },
        level3: {
          providerId: "apple-vision",
          providerVersion: "vision-rev3",
          topologyId: "face-68",
          modelVersion: "macOS Vision revision 3",
          denseLandmarks: true,
          unlocked: false,
          reason: "test",
          faceUuid: UUID_A,
          lockedAt: "2026-11-21T00:00:00Z",
          futureGateField: "keep-me",
        },
        futureFaceField: { note: "preserve" },
      } as unknown as ProjectPhoto["faces"][number],
      {
        uuid: UUID_B,
        bbox: { x: 0.6, y: 0.1, width: 0.18, height: 0.2 },
        landmarks: [],
        score: 0.88,
        model: "yunet-5",
        confidence: 0.88,
        maskRef: null,
      },
    ],
    subjects: [
      {
        subjectId: "subject-1",
        maskSource: "manual",
        maskModelVersion: "manual-alpha-1",
        maskStrokes: 1,
        body: { waist: -30, shoulders: 12, hips: 8, thighs: 0, calves: 0, arms: 0, legs: 0 },
        anchors: 1,
      },
    ],
    maskMeta: {
      source: "manual",
      modelId: "manual-alpha",
      modelVersion: "manual-alpha-1",
      coverage: 0.37,
      strokeHash: "abc123",
      revision: 4,
    },
    source: {
      sourceType: "raster",
      path,
      hash: "sha256-a",
      metadata: { make: "Canon", model: "R5", iso: 400 },
    },
    raw: {
      params: { exposureEv: 0.75 } as never,
      decoderVersion: "libraw-0.21.1+lumi-mhc-1",
    },
    color: { output: { profile: "displayP3", intent: "relative" } as never },
    // 未知照片字段。
    futurePhotoField: { layers: [1, 2, 3] },
  };
}

function richDoc(): ProjectDoc {
  const photoB = richPhoto("/photos/b.jpg");
  photoB.params = clampParams({ ...defaultParams(), exposure: -5 });
  photoB.faces = [];
  photoB.masks = [];
  photoB.color = { output: { profile: "srgb", intent: "perceptual" } as never };
  photoB.export = { format: "jpeg", quality: 70, resize: null, filenameTemplate: "b", watermark: null } as never;
  return {
    version: 3,
    createdAt: "2026-11-20T10:00:00.000Z",
    activeIndex: 0,
    export: { format: "jpeg", quality: 92, resize: null, filenameTemplate: "", watermark: null } as never,
    compute: { preference: "cpu" },
    photos: [richPhoto("/photos/a.jpg"), photoB],
    projectUuid: "project-uuid-1",
    revision: 7,
    updatedAt: "2026-11-20T11:00:00.000Z",
    futureDocField: { schemaHint: "v4" },
  };
}

function openInStore(photoAssets: PhotoAsset[], doc: ProjectDoc) {
  const store = createEditorStore();
  const requestId = store.getState().beginProjectLoad("/tmp/rich.lumi");
  const snapshot = buildProjectSnapshot(photoAssets, doc, "project-1");
  expect(store.getState().commitProjectSnapshot(requestId, snapshot)).toBe(true);
  store.getState().finishProjectLoad(requestId);
  return store;
}

function saveAndReopen(photoAssets: PhotoAsset[], doc: ProjectDoc): ProjectDoc {
  const store = openInStore(photoAssets, doc);
  const saved = buildProjectDoc(store.getState());
  // 模拟 Rust 保存：revision 递增、updatedAt 更新，其它不变。
  return { ...saved, revision: (saved.revision ?? 0) + 1, updatedAt: "2026-11-21T00:00:00.000Z" };
}

function photoOf(doc: ProjectDoc, path: string): ProjectPhoto {
  const found = doc.photos.find((photo) => photo.path === path);
  if (!found) throw new Error(`缺少照片 ${path}`);
  return found;
}

describe("ROUND 9.1 持久化等价性", () => {
  it("打开→保存→重开：元数据、蒙版、元数据快照与未知字段全部保留", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const reopened = saveAndReopen(assets, doc);

    expect(reopened.version).toBe(3);
    expect(reopened.createdAt).toBe(doc.createdAt);
    expect(reopened.projectUuid).toBe("project-uuid-1");
    expect(reopened.revision).toBe(8);
    expect(reopened.compute?.preference).toBe("cpu");
    expect(reopened.futureDocField).toEqual({ schemaHint: "v4" });

    const a = photoOf(reopened, "/photos/a.jpg");
    expect(a.masks).toEqual(photoOf(doc, "/photos/a.jpg").masks);
    expect(a.source?.metadata).toEqual({ make: "Canon", model: "R5", iso: 400 });
    expect(a.raw?.decoderVersion).toBe("libraw-0.21.1+lumi-mhc-1");
    expect(a.color?.output).toEqual({ profile: "displayP3", intent: "relative" });
    expect(a.futurePhotoField).toEqual({ layers: [1, 2, 3] });
    expect((a.params as unknown as Record<string, unknown>).futureParamField).toEqual({
      enabled: true,
      strength: 0.5,
    });

    const faces = a.faces as unknown as Array<Record<string, unknown>>;
    expect(faces).toHaveLength(2);
    expect(faces[0]!.uuid).toBe(UUID_A);
    expect(faces[0]!.model).toBe("yunet-5");
    expect(faces[0]!.maskRef).toBe("mask-1");
    expect((faces[0]!.dense as { topologyId: string }).topologyId).toBe("face-68");
    expect((faces[0]!.dense as { regions: unknown[] }).regions).toHaveLength(2);
    expect((faces[0]!.level3 as { denseLandmarks: boolean }).denseLandmarks).toBe(true);
    expect(faces[0]!.futureFaceField).toEqual({ note: "preserve" });
    expect(faces[1]!.uuid).toBe(UUID_B);
  });

  it("打开→不改→保存→重开：每类参数与磁盘一致（含浮点容差）", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const reopened = saveAndReopen(assets, doc);
    const before = clampParams(photoOf(doc, "/photos/a.jpg").params);
    const after = clampParams(photoOf(reopened, "/photos/a.jpg").params);

    for (const key of [
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
    ] as const) {
      expect(Math.abs(after[key] - before[key])).toBeLessThan(1e-6);
    }
    expect(after.faces).toEqual(before.faces);
    expect(after.reshape).toEqual(before.reshape);
    expect(after.body).toEqual(before.body);
    expect(after.background).toEqual(before.background);
    expect(after.subjectMask).toEqual(before.subjectMask);
    expect(after.manualMask).toEqual(before.manualMask);
    expect(after.repairPoints).toEqual(before.repairPoints);
    expect(after.hsl).toEqual(before.hsl);
    expect(after.curves).toEqual(before.curves);
    expect(after.lut).toEqual(before.lut);

    // 非活动照片的项目级设置保持原值，不被当前会话的全局值覆盖。
    const b = photoOf(reopened, "/photos/b.jpg");
    expect(b.export).toEqual(photoOf(doc, "/photos/b.jpg").export);
    expect(b.color?.output).toEqual({ profile: "srgb", intent: "perceptual" });
  });

  it("改一项→保存→重开：只改变目标参数，其它数据不动", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const store = openInStore(assets, doc);
    store.getState().updateParam("exposure", 55);
    const saved = buildProjectDoc(store.getState());
    const reopened = saveAndReopen(assets, saved);

    const a = photoOf(reopened, "/photos/a.jpg");
    expect(a.params.exposure).toBe(55);
    expect(a.params.contrast).toBe(-8);
    expect(a.params.faces).toHaveLength(2);
    expect(a.params.body[0]?.shape.waist).toBe(-30);
    expect(a.faces).toHaveLength(2);
    expect(a.masks).toHaveLength(2);
  });

  it("打开→不改→保存：活动照片的导出/色彩/未知字段与磁盘完全一致", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const reopened = saveAndReopen(assets, doc);
    const before = photoOf(doc, "/photos/a.jpg");
    const after = photoOf(reopened, "/photos/a.jpg");

    // ROUND 9.1 R9-SAVE-09：活动照片的导出设置曾被会话默认值整体覆盖。
    expect(after.export).toEqual(before.export);
    expect((after.export as unknown as Record<string, unknown>).filenameTemplate).toBe("a");
    expect(after.color).toEqual(before.color);
  });

  it("显式修改导出设置：只覆盖活动照片，未知字段与其它照片不受影响", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const store = openInStore(assets, doc);
    store.getState().setExportSettings({ format: "webp", quality: 77 });
    const saved = buildProjectDoc(store.getState());

    const a = photoOf(saved, "/photos/a.jpg");
    expect(a.export?.format).toBe("webp");
    expect(a.export?.quality).toBe(77);
    expect((a.export as unknown as Record<string, unknown>).filenameTemplate).toBe("a");
    expect(photoOf(saved, "/photos/b.jpg").export).toEqual(photoOf(doc, "/photos/b.jpg").export);
  });

  it("活动照片不是第一张时：加载与保存使用该照片自己的色彩/导出", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = { ...richDoc(), activeIndex: 1 };
    const store = openInStore(assets, doc);
    expect(store.getState().colorSettings?.profile).toBe("srgb");
    expect(store.getState().exportSettings.format).toBe("jpeg");
    expect(store.getState().exportSettings.quality).toBe(70);

    const saved = buildProjectDoc(store.getState());
    expect(photoOf(saved, "/photos/a.jpg").export).toEqual(photoOf(doc, "/photos/a.jpg").export);
    expect(photoOf(saved, "/photos/b.jpg").export).toEqual(photoOf(doc, "/photos/b.jpg").export);
    expect(photoOf(saved, "/photos/a.jpg").color).toEqual(photoOf(doc, "/photos/a.jpg").color);
    expect(photoOf(saved, "/photos/b.jpg").color).toEqual(photoOf(doc, "/photos/b.jpg").color);
  });

  it("保存→再保存→重开：第二次保存不改变数据，只递增 revision", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const once = saveAndReopen(assets, doc);
    const twice = saveAndReopen(assets, once);

    expect(twice.revision).toBe((once.revision ?? 0) + 1);
    expect(twice.createdAt).toBe(doc.createdAt);
    expect(twice.projectUuid).toBe(doc.projectUuid);
    expect(photoOf(twice, "/photos/a.jpg").params).toEqual(photoOf(once, "/photos/a.jpg").params);
    expect(photoOf(twice, "/photos/a.jpg").masks).toEqual(photoOf(once, "/photos/a.jpg").masks);
    expect(photoOf(twice, "/photos/a.jpg").faces).toEqual(photoOf(once, "/photos/a.jpg").faces);
  });

  it("连续 20 次打开→保存→重开：参数、人脸、蒙版、UUID 完全一致", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    let doc = richDoc();
    const expected = clampParams(photoOf(doc, "/photos/a.jpg").params);
    const expectedFaces = photoOf(doc, "/photos/a.jpg").faces;
    const expectedMasks = photoOf(doc, "/photos/a.jpg").masks;
    for (let round = 0; round < 20; round += 1) {
      doc = saveAndReopen(assets, doc);
      const photo = photoOf(doc, "/photos/a.jpg");
      expect(clampParams(photo.params)).toEqual(expected);
      expect(photo.faces).toEqual(expectedFaces);
      expect(photo.masks).toEqual(expectedMasks);
      expect(photo.source?.metadata).toEqual({ make: "Canon", model: "R5", iso: 400 });
      expect(doc.createdAt).toBe("2026-11-20T10:00:00.000Z");
      expect(doc.projectUuid).toBe("project-uuid-1");
    }
    expect(doc.revision).toBe(27);
  });

  it("旧项目缺少新增字段：加载后给出默认元数据，不报错也不伪造 UUID", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg")];
    const legacy = {
      version: 2,
      createdAt: "2025-01-01T00:00:00Z",
      activeIndex: 0,
      export: { format: "jpeg", quality: 92, resize: null, filenameTemplate: "", watermark: null },
      photos: [
        {
          path: "/photos/a.jpg",
          params: { exposure: 12 },
          masks: [],
          export: null,
          sourceHash: "sha256-a",
          faces: [],
          subjects: [],
          maskMeta: null,
        },
      ],
    } as unknown as ProjectDoc;
    const reopened = saveAndReopen(assets, legacy);
    expect(reopened.createdAt).toBe("2025-01-01T00:00:00Z");
    expect(reopened.projectUuid ?? "").toBe("");
    expect(reopened.revision).toBe(1);
    expect(photoOf(reopened, "/photos/a.jpg").params.exposure).toBe(12);
  });

  it("未知字段不会因为一次部分编辑而消失", () => {
    const assets = [makePhoto(PHOTO_A, "/photos/a.jpg"), makePhoto(PHOTO_B, "/photos/b.jpg")];
    const doc = richDoc();
    const store = openInStore(assets, doc);
    store.getState().updateParam("exposure", 3);
    const saved = buildProjectDoc(store.getState());
    expect(saved.futureDocField).toEqual({ schemaHint: "v4" });
    expect(photoOf(saved, "/photos/a.jpg").futurePhotoField).toEqual({ layers: [1, 2, 3] });
    expect(
      (photoOf(saved, "/photos/a.jpg").params as unknown as Record<string, unknown>).futureParamField,
    ).toEqual({
      enabled: true,
      strength: 0.5,
    });
    const faces = photoOf(saved, "/photos/a.jpg").faces as unknown as Array<Record<string, unknown>>;
    expect(faces[0]!.futureFaceField).toEqual({ note: "preserve" });
  });
});
