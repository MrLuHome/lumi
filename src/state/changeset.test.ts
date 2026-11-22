import { describe, expect, it } from "vitest";
import { createEditorStore } from "./editorStore";
import { buildChangeSet, buildFieldPatch, buildProjectSnapshot } from "./project";
import { defaultParams } from "./params";
import type { PhotoAsset, ProjectDoc, ProjectPhoto } from "../types";

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

function basePhoto(path: string): ProjectPhoto {
  return {
    path,
    params: defaultParams(),
    masks: [],
    export: {
      format: "jpeg",
      quality: 92,
      resize: null,
      filenameTemplate: "",
      watermark: null,
    } as never,
    sourceHash: "hash",
    faces: [],
    subjects: [],
    maskMeta: null,
    source: { sourceType: "raster", path, hash: "hash", metadata: { camera: "X" } },
    raw: null,
    color: null,
    futurePhotoField: { keep: true },
  } as ProjectPhoto;
}

function baseDoc(): ProjectDoc {
  return {
    version: 3,
    createdAt: "2026-01-01T00:00:00.000Z",
    activeIndex: 0,
    export: {
      format: "jpeg",
      quality: 92,
      resize: null,
      filenameTemplate: "",
      watermark: null,
    } as never,
    photos: [basePhoto("/photos/a.jpg")],
    projectUuid: "project-uuid-1",
    revision: 7,
    updatedAt: "2026-01-01T00:00:00.000Z",
  };
}

function openStore() {
  const store = createEditorStore();
  const assets = [makePhoto("photo-a", "/photos/a.jpg")];
  const requestId = store.getState().beginProjectLoad("/tmp/a.lumi");
  const snapshot = buildProjectSnapshot(assets, baseDoc(), "project-1");
  expect(store.getState().commitProjectSnapshot(requestId, snapshot)).toBe(true);
  store.getState().finishProjectLoad(requestId);
  return store;
}

describe("ROUND 10 ChangeSet 构造", () => {
  it("只输出发生变化的字段", () => {
    const store = openStore();
    store.getState().updateParam("exposure", 12);

    const changeset = buildChangeSet(store.getState(), "req-1");
    expect(changeset).not.toBeNull();
    expect(changeset!.projectId).toBe("project-uuid-1");
    expect(changeset!.baseRevision).toBe(7);
    expect(changeset!.requestId).toBe("req-1");
    expect(changeset!.rebaseOnConflict).toBe(true);
    const params = changeset!.operations.filter((op) => op.type === "updatePhotoParams");
    expect(params).toHaveLength(1);
    expect(params[0]).toMatchObject({
      type: "updatePhotoParams",
      photo: "/photos/a.jpg",
      patch: { exposure: 12 },
    });
    // 未修改字段不应出现在补丁里。
    expect(params[0]).not.toMatchObject({ patch: { contrast: expect.anything() } });
  });

  it("没有修改时不生成 ChangeSet", () => {
    const store = openStore();
    expect(buildChangeSet(store.getState(), "req-2")).toBeNull();
  });

  it("buildFieldPatch 保留显式 null 并忽略未变化字段", () => {
    const patch = buildFieldPatch(
      { exposure: 0, metadata: { camera: "X" }, lut: { path: "/a.cube" } },
      { exposure: 0, metadata: null, lut: { path: "/a.cube" } },
    );
    expect(patch).toEqual({ metadata: null });
  });

  it("buildFieldPatch 对浮点使用容差、对数组整体替换", () => {
    expect(buildFieldPatch({ exposure: 12 }, { exposure: 12.00001 })).toEqual({});
    expect(buildFieldPatch({ exposure: 12 }, { exposure: 12.5 })).toEqual({ exposure: 12.5 });
    expect(buildFieldPatch({ curves: [1, 2] }, { curves: [1, 3] })).toEqual({ curves: [1, 3] });
  });

  it("嵌套对象只提交变化的分支", () => {
    const patch = buildFieldPatch(
      { beauty: { skinSmooth: 0, texture: 0 } },
      { beauty: { skinSmooth: 8, texture: 0 } },
    );
    expect(patch).toEqual({ beauty: { skinSmooth: 8 } });
  });
});
