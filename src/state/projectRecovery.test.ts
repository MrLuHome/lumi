import { describe, expect, it } from "vitest";
import { createEditorStore } from "./editorStore";
import { buildProjectSnapshot } from "./project";
import { defaultFaceBeauty, type EditParams } from "./params";
import type {
  DetectedFace,
  FaceDetectionResult,
  FaceRestoreResult,
  PhotoAsset,
  ProjectDoc,
  ProjectFaceRecord,
  ProjectPhoto,
} from "../types";

const UUID_A = "18e9fd0f-9771-49b8-8001-df85c8fbfc19";
const FACE_ID_A = 1700055149;
const SESSION_UUID = "7b1c2c5e-0000-4000-8000-000000000001";
const SESSION_FACE_ID = 424242;

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

function faceRecord(uuid: string, _faceId: number, model = "yunet-5"): ProjectFaceRecord {
  return {
    uuid,
    bbox: { x: 0.3, y: 0.2, width: 0.2, height: 0.2 },
    landmarks: [
      { x: 0.35, y: 0.28 },
      { x: 0.45, y: 0.28 },
      { x: 0.4, y: 0.33 },
      { x: 0.36, y: 0.38 },
      { x: 0.44, y: 0.38 },
    ],
    score: 0.97,
    model,
    confidence: 0.97,
    maskRef: null,
  };
}

function faceParams(uuid: string, faceId: number, smooth = 17, texture = 33, spots = 11) {
  const base = defaultFaceBeauty(faceId, uuid);
  return {
    ...base,
    beauty: { ...base.beauty, skinSmooth: smooth, texture, spots },
  };
}

function makePhotoDoc(
  path: string,
  params: EditParams,
  uuid = UUID_A,
  faceId = FACE_ID_A,
): ProjectPhoto {
  return {
    path,
    params,
    masks: [],
    export: null,
    sourceHash: "sha256-source",
    faces: [faceRecord(uuid, faceId)],
    subjects: [],
    maskMeta: null,
  };
}

function makeDoc(photos: ProjectPhoto[], activeIndex = 0): ProjectDoc {
  return {
    version: 3,
    createdAt: "2026-11-20T00:00:00Z",
    activeIndex,
    export: {
      format: "jpeg",
      quality: 92,
      resize: null,
      filenameTemplate: "",
      watermark: null,
    } as ProjectDoc["export"],
    photos,
  };
}

function detectedFace(id: number, uuid: string): DetectedFace {
  return {
    id,
    uuid,
    score: 0.97,
    x: 300,
    y: 200,
    width: 800,
    height: 800,
    landmarks: [
      { x: 350, y: 280 },
      { x: 450, y: 280 },
      { x: 400, y: 330 },
      { x: 360, y: 380 },
      { x: 440, y: 380 },
    ],
  };
}

function detectionOf(face: DetectedFace): FaceDetectionResult {
  return {
    faces: [face],
    imageWidth: 4000,
    imageHeight: 3000,
    elapsedMs: 12,
    inferenceMs: 8,
    model: "yunet-5",
  };
}

function restoreResult(
  _sessionFace: DetectedFace,
  finalFace: DetectedFace,
  link: { faceId: number; finalFaceId: number; uuid: string } | null,
): FaceRestoreResult {
  return {
    restored: link
      ? [
          {
            recordIndex: 0,
            faceId: link.faceId,
            finalFaceId: link.finalFaceId,
            faceUuid: link.uuid,
            score: 0.98,
            confidence: "high",
          },
        ]
      : [],
    pending: [],
    missing: [],
    sourceChanged: false,
    faces: [finalFace],
    discardedDense: 0,
  };
}

function openProject(store: ReturnType<typeof createEditorStore>, photo: PhotoAsset, doc: ProjectDoc) {
  const requestId = store.getState().beginProjectLoad("/tmp/project.lumi");
  const snapshot = buildProjectSnapshot([photo], doc, "project-1");
  const committed = store.getState().commitProjectSnapshot(requestId, snapshot);
  return { requestId, committed, snapshot };
}

function paramsOf(store: ReturnType<typeof createEditorStore>, id: string): EditParams {
  const value = store.getState().params[id];
  if (!value) throw new Error(`缺少 ${id} 的参数`);
  return value;
}

function facesOf(store: ReturnType<typeof createEditorStore>, id: string) {
  return paramsOf(store, id).faces;
}

describe("ROUND 9 项目恢复", () => {
  it("冷启动恢复：会话新 UUID 不得把项目人脸参数覆盖成 0", () => {
    const store = createEditorStore();
    const photo = makePhoto("a");
    const params: EditParams = { faces: [faceParams(UUID_A, FACE_ID_A)] } as EditParams;
    const doc = makeDoc([makePhotoDoc(photo.path, params)]);
    const { requestId, committed } = openProject(store, photo, doc);
    expect(committed).toBe(true);
    expect(facesOf(store, photo.id)[0]!.beauty.skinSmooth).toBe(17);

    // 全新会话：检测返回不同的 UUID 与 ID。
    const sessionFace = detectedFace(SESSION_FACE_ID, SESSION_UUID);
    store.getState().setDetectedFaces(photo.id, detectionOf(sessionFace), requestId);
    expect(facesOf(store, photo.id)[0]!.beauty.skinSmooth).toBe(17);

    // 身份恢复完成后，参数必须仍然保留，faceId 改为最终 ID。
    const finalFace = detectedFace(FACE_ID_A, UUID_A);
    store
      .getState()
      .applyFaceRestore(
        photo.id,
        restoreResult(sessionFace, finalFace, {
          faceId: SESSION_FACE_ID,
          finalFaceId: FACE_ID_A,
          uuid: UUID_A,
        }),
        requestId,
      );
    const restored = facesOf(store, photo.id);
    expect(restored).toHaveLength(1);
    expect(restored[0]!.faceId).toBe(FACE_ID_A);
    expect(restored[0]!.faceUuid).toBe(UUID_A);
    expect(restored[0]!.beauty).toMatchObject({ skinSmooth: 17, texture: 33, spots: 11 });

    store.getState().finishProjectLoad(requestId);
    expect(store.getState().loadSession.phase).toBe("ready");
    expect(store.getState().loadSnapshot).toBeNull();
  });

  it("连续恢复 20 次参数完全一致", () => {
    const photo = makePhoto("a");
    const expected = { skinSmooth: 17, texture: 33, spots: 11 };
    for (let round = 0; round < 20; round += 1) {
      const store = createEditorStore();
      const params: EditParams = { faces: [faceParams(UUID_A, FACE_ID_A)] } as EditParams;
      const doc = makeDoc([makePhotoDoc(photo.path, params)]);
      const { requestId } = openProject(store, photo, doc);
      const sessionFace = detectedFace(SESSION_FACE_ID + round, `session-${round}`);
      store.getState().setDetectedFaces(photo.id, detectionOf(sessionFace), requestId);
      store.getState().applyFaceRestore(
        photo.id,
        restoreResult(sessionFace, detectedFace(FACE_ID_A, UUID_A), {
          faceId: sessionFace.id,
          finalFaceId: FACE_ID_A,
          uuid: UUID_A,
        }),
        requestId,
      );
      store.getState().finishProjectLoad(requestId);
      const actual = facesOf(store, photo.id)[0]!.beauty;
      expect(actual).toMatchObject(expected);
      expect(facesOf(store, photo.id)).toHaveLength(1);
    }
  });

  it("A 加载中切 B：A 的迟到回调不覆盖 B", () => {
    const store = createEditorStore();
    const photoA = makePhoto("a");
    const photoB = makePhoto("b");
    const docA = makeDoc([makePhotoDoc(photoA.path, { faces: [faceParams(UUID_A, FACE_ID_A)] } as EditParams)]);
    const docB = makeDoc([
      makePhotoDoc(
        photoB.path,
        { faces: [faceParams("uuid-b", 222, 5, 6, 7)] } as EditParams,
        "uuid-b",
        222,
      ),
    ]);

    const a = openProject(store, photoA, docA);
    const b = openProject(store, photoB, docB);
    expect(a.committed).toBe(true);
    expect(b.committed).toBe(true);

    // A 的迟到检测与恢复必须被拒绝。
    store.getState().setDetectedFaces(photoA.id, detectionOf(detectedFace(9, "late")), a.requestId);
    store.getState().applyFaceRestore(
      photoA.id,
      restoreResult(detectedFace(9, "late"), detectedFace(FACE_ID_A, UUID_A), {
        faceId: 9,
        finalFaceId: FACE_ID_A,
        uuid: UUID_A,
      }),
      a.requestId,
    );
    store.getState().finishProjectLoad(a.requestId);

    expect(store.getState().activeId).toBe(photoB.id);
    expect(facesOf(store, photoB.id)[0]!.beauty.skinSmooth).toBe(5);
    expect(store.getState().loadSession.requestId).toBe(b.requestId);
    expect(store.getState().loadSession.phase).toBe("restoring");
  });

  it("恢复期间禁止编辑与保存", () => {
    const store = createEditorStore();
    const photo = makePhoto("a");
    const doc = makeDoc([makePhotoDoc(photo.path, { faces: [faceParams(UUID_A, FACE_ID_A)] } as EditParams)]);
    const { requestId } = openProject(store, photo, doc);
    expect(store.getState().loadSession.phase).toBe("restoring");

    store.getState().updateParam("exposure", 25);
    expect(paramsOf(store, photo.id).exposure).toBe(0);
    expect(store.getState().history[photo.id]!.past).toHaveLength(0);

    store.getState().finishProjectLoad(requestId);
    store.getState().updateParam("exposure", 25);
    expect(paramsOf(store, photo.id).exposure).toBe(25);
  });

  it("未匹配的人脸记录保留原参数，不因缺失而丢弃", () => {
    const store = createEditorStore();
    const photo = makePhoto("a");
    const params: EditParams = { faces: [faceParams(UUID_A, FACE_ID_A)] } as EditParams;
    const doc = makeDoc([makePhotoDoc(photo.path, params)]);
    const { requestId } = openProject(store, photo, doc);
    const sessionFace = detectedFace(SESSION_FACE_ID, SESSION_UUID);
    store.getState().setDetectedFaces(photo.id, detectionOf(sessionFace), requestId);
    store.getState().applyFaceRestore(
      photo.id,
      {
        restored: [],
        pending: [],
        missing: [faceRecord(UUID_A, FACE_ID_A)],
        sourceChanged: false,
        faces: [detectedFace(FACE_ID_A, UUID_A)],
        discardedDense: 0,
      },
      requestId,
    );
    const faces = facesOf(store, photo.id);
    expect(faces.some((face) => face.faceUuid === UUID_A && face.beauty.skinSmooth === 17)).toBe(true);
  });

  it("参数损坏时进入 Failed，不使用默认值伪装成功", () => {
    const store = createEditorStore();
    const photo = makePhoto("a");
    const bad = { ...makePhotoDoc(photo.path, {} as EditParams), params: undefined as never };
    const requestId = store.getState().beginProjectLoad("/tmp/broken.lumi");
    const snapshot = buildProjectSnapshot([photo], makeDoc([bad]), "broken");
    const committed = store.getState().commitProjectSnapshot(requestId, snapshot);
    expect(committed).toBe(false);
    expect(store.getState().loadSession.phase).toBe("failed");
    expect(store.getState().loadSession.error?.code).toBe("project-invalid");
    expect(store.getState().photos).toHaveLength(0);
  });

  it("多人脸各自独立参数，恢复后不交错", () => {
    const store = createEditorStore();
    const photo = makePhoto("a");
    const uuidB = "uuid-b";
    const faceA = faceParams(UUID_A, FACE_ID_A, 11, 12, 13);
    const faceB = faceParams(uuidB, 222, 21, 22, 23);
    const doc = makeDoc([
      {
        ...makePhotoDoc(photo.path, { faces: [faceA, faceB] } as EditParams),
        faces: [faceRecord(UUID_A, FACE_ID_A), faceRecord(uuidB, 222)],
      },
    ]);
    const { requestId } = openProject(store, photo, doc);
    const sessionA = detectedFace(1, "session-a");
    const sessionB = detectedFace(2, "session-b");
    store.getState().setDetectedFaces(
      photo.id,
      { ...detectionOf(sessionA), faces: [sessionA, sessionB] },
      requestId,
    );
    store.getState().applyFaceRestore(
      photo.id,
      {
        restored: [
          { recordIndex: 0, faceId: 1, finalFaceId: FACE_ID_A, faceUuid: UUID_A, score: 0.99, confidence: "high" },
          { recordIndex: 1, faceId: 2, finalFaceId: 222, faceUuid: uuidB, score: 0.98, confidence: "high" },
        ],
        pending: [],
        missing: [],
        sourceChanged: false,
        faces: [detectedFace(FACE_ID_A, UUID_A), detectedFace(222, uuidB)],
        discardedDense: 0,
      },
      requestId,
    );
    const faces = new Map(
      facesOf(store, photo.id).map((face) => [face.faceUuid, face.beauty]),
    );
    expect(faces.get(UUID_A)).toMatchObject({ skinSmooth: 11, texture: 12, spots: 13 });
    expect(faces.get(uuidB)).toMatchObject({ skinSmooth: 21, texture: 22, spots: 23 });
  });
});
