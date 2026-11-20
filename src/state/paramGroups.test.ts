import { describe, expect, it } from "vitest";
import type { FaceReshape } from "./params";
import { defaultBeauty, defaultFaceShape, defaultParams } from "./params";
import {
  DEFAULT_COPY_GROUPS,
  OBJECT_LEVEL_GROUPS,
  applyCopy,
  describeGroups,
  extractCopy,
} from "./paramGroups";

function fakeReshape(faceUuid: string, slim: number): FaceReshape {
  return {
    faceId: 1,
    faceUuid,
    enabled: true,
    face: { ...defaultFaceShape(), slim },
    eyes: {} as never,
    nose: {} as never,
    mouth: {} as never,
    chin: {} as never,
    manual: {} as never,
  };
}

function source() {
  const params = defaultParams();
  params.exposure = 14;
  params.contrast = 8;
  params.temperature = 12;
  params.saturation = -6;
  params.hsl.orange.saturation = 15;
  params.curves.rgb = [
    { x: 0, y: 0 },
    { x: 1, y: 1 },
  ];
  params.lut = { path: "/Users/demo/teal.cube", name: "teal.cube", strength: 80 };
  params.skinSmooth = 20;
  params.manualMask.enabled = true;
  params.manualMask.strokes = [
    { erase: false, radius: 0.04, points: [{ x: 0.2, y: 0.2 }] },
  ];
  params.reshape = [fakeReshape("face-a", 30)];
  params.faces = [
    {
      faceId: 1,
      faceUuid: "face-a",
      enabled: true,
      beauty: { ...defaultBeauty(), skinSmooth: 40, toneEven: 20 },
    },
  ];
  return params;
}

describe("批量参数复制", () => {
  it("默认分组不包含任何对象级字段", () => {
    const copied = extractCopy(source(), DEFAULT_COPY_GROUPS);
    expect(copied.values).not.toHaveProperty("faces");
    expect(copied.values).not.toHaveProperty("manualMask");
    expect(copied.values).not.toHaveProperty("reshape");
    expect(copied.values).not.toHaveProperty("subjectMask");
    expect(copied.values).not.toHaveProperty("body");
    expect(copied.values).not.toHaveProperty("background");
    expect(copied.values).not.toHaveProperty("repairPoints");
    expect(copied.values.exposure).toBe(14);
    expect(copied.values.hsl?.orange.saturation).toBe(15);
    expect(copied.lutName).toBe("teal.cube");
  });

  it("美肤只以模板形式记录最大强度，不携带人脸 UUID", () => {
    const copied = extractCopy(source(), ["beauty"]);
    expect(copied.beautyTemplate?.skinSmooth).toBe(40);
    expect(copied.beautyTemplate?.toneEven).toBe(20);
    expect(JSON.stringify(copied.values)).not.toContain("face-a");
    expect(JSON.stringify(copied.beautyTemplate)).not.toContain("face-a");
  });

  it("应用到目标照片时保留目标自己的蒙版与锚点", () => {
    const copied = extractCopy(source(), DEFAULT_COPY_GROUPS);
    const target = defaultParams();
    target.manualMask.enabled = true;
    target.manualMask.strokes = [
      { erase: false, radius: 0.02, points: [{ x: 0.7, y: 0.6 }] },
    ];
    target.reshape = [fakeReshape("target-face", -12)];
    target.faces = [];
    const result = applyCopy(target, copied);
    expect(result.params.exposure).toBe(14);
    expect(result.params.manualMask.strokes[0]!.points[0]!.x).toBe(0.7);
    expect(result.params.reshape[0]!.face.slim).toBe(-12);
    expect(result.params.reshape[0]!.faceUuid).toBe("target-face");
    expect(result.warnings.some((item) => item.includes("没有检测到人脸"))).toBe(true);
  });

  it("LUT 名称不一致时给出提示且不静默替换", () => {
    const copied = extractCopy(source(), ["lut"]);
    const target = defaultParams();
    target.lut = { path: "/Users/demo/warm.cube", name: "warm.cube", strength: 40 };
    const result = applyCopy(target, copied);
    expect(result.warnings.some((item) => item.includes("teal.cube"))).toBe(true);
    expect(result.params.lut?.name).toBe("warm.cube");
  });

  it("分组名称可用于向用户说明将覆盖的内容", () => {
    expect(describeGroups(["light", "color"])).toEqual(["基础光影", "色彩"]);
    expect(OBJECT_LEVEL_GROUPS.length).toBeGreaterThan(0);
  });
});
