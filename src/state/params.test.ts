import { describe, expect, it } from "vitest";
import {
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_INDEX,
  activeParamCount,
  clampParams,
  defaultBeauty,
  findFaceBeauty,
  formatParamValue,
  isNeutral,
  paramsEqual,
  paramsKey,
  withFaceBeauty,
} from "./params";

describe("参数定义", () => {
  const numericKeys = Object.keys(DEFAULT_PARAMS).filter(
    (key) => !["hsl", "curves", "lut", "faces", "manualMask", "repairPoints", "skinSmooth"].includes(key),
  );

  it("每个数值参数在参数组中恰好出现一次", () => {
    const keys = PARAM_GROUPS.flatMap((group) => group.params.map((definition) => definition.key));
    expect(keys.sort()).toEqual(numericKeys.sort());
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("默认值全部为 0 且判定为未调整", () => {
    expect(numericKeys.every((key) => DEFAULT_PARAMS[key as keyof typeof DEFAULT_PARAMS] === 0)).toBe(
      true,
    );
    expect(Object.values(DEFAULT_PARAMS.hsl).every((channel) => channel.hue === 0)).toBe(true);
    expect(isNeutral(DEFAULT_PARAMS)).toBe(true);
    expect(activeParamCount(DEFAULT_PARAMS)).toBe(0);
  });

  it("越界与非数值输入被收敛到合法区间", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      exposure: 500,
      shadows: -500,
      vignette: -20,
      grain: Number.NaN,
    });
    expect(clamped.exposure).toBe(100);
    expect(clamped.shadows).toBe(-100);
    expect(clamped.vignette).toBe(0);
    expect(clamped.grain).toBe(0);
  });

  it("统计已调整参数数量", () => {
    expect(activeParamCount({ ...DEFAULT_PARAMS, exposure: 12, grain: 5 })).toBe(2);
  });

  it("双向参数带正负号，单向参数直接显示", () => {
    expect(formatParamValue(PARAM_INDEX.exposure, 12)).toBe("+12");
    expect(formatParamValue(PARAM_INDEX.exposure, -8)).toBe("-8");
    expect(formatParamValue(PARAM_INDEX.vignette, 30)).toBe("30");
    expect(formatParamValue(PARAM_INDEX.grain, 0)).toBe("0");
  });

  it("paramsKey 随参数变化，缓存键稳定可复现", () => {
    expect(paramsKey(DEFAULT_PARAMS)).toBe(paramsKey({ ...DEFAULT_PARAMS }));
    expect(paramsKey({ ...DEFAULT_PARAMS, tint: 4 })).not.toBe(paramsKey(DEFAULT_PARAMS));
  });

  it("paramsEqual 逐字段比较", () => {
    expect(paramsEqual(DEFAULT_PARAMS, { ...DEFAULT_PARAMS })).toBe(true);
    expect(paramsEqual(DEFAULT_PARAMS, { ...DEFAULT_PARAMS, clarity: 1 })).toBe(false);
  });

  it("HSL 通道参数参与夹紧、计数与中性判定", () => {
    const edited = clampParams({
      ...DEFAULT_PARAMS,
      hsl: {
        ...DEFAULT_PARAMS.hsl,
        red: { hue: 300, saturation: -500, luminance: 12 },
      },
    });
    expect(edited.hsl.red.hue).toBe(100);
    expect(edited.hsl.red.saturation).toBe(-100);
    expect(edited.hsl.red.luminance).toBe(12);
    expect(isNeutral(edited)).toBe(false);
    expect(activeParamCount(edited)).toBe(1);
  });

  it("曲线点被排序、过滤并夹紧到 0..1", () => {
    const edited = clampParams({
      ...DEFAULT_PARAMS,
      curves: {
        ...DEFAULT_PARAMS.curves,
        rgb: [
          { x: 1.4, y: 1.2 },
          { x: 0.4, y: 0.6 },
          { x: Number.NaN, y: 0.2 },
          { x: -0.2, y: 0 },
        ],
      },
    });
    expect(edited.curves.rgb).toEqual([
      { x: 0, y: 0 },
      { x: 0.4, y: 0.6 },
      { x: 1, y: 1 },
    ]);
    expect(activeParamCount(edited)).toBe(1);
    expect(paramsKey(edited)).not.toBe(paramsKey(DEFAULT_PARAMS));
  });

  it("LUT 强度被夹紧，空路径视为未设置", () => {
    const withLut = clampParams({
      ...DEFAULT_PARAMS,
      lut: { path: "/tmp/a.cube", name: "a", strength: 420 },
    });
    expect(withLut.lut?.strength).toBe(100);
    expect(activeParamCount(withLut)).toBe(1);
    const empty = clampParams({
      ...DEFAULT_PARAMS,
      lut: { path: "", name: "x", strength: 100 },
    });
    expect(empty.lut).toBeNull();
  });
});

describe("多人脸美肤参数", () => {
  it("按稳定 ID 排序并夹紧", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      faces: [
        { faceId: 2, enabled: true, beauty: { ...defaultBeauty(), skinSmooth: 500, whitening: -20 } },
        { faceId: 1, enabled: false, beauty: defaultBeauty() },
      ],
    });
    expect(clamped.faces.map((face) => face.faceId)).toEqual([1, 2]);
    expect(clamped.faces[1]?.beauty.skinSmooth).toBe(100);
    expect(clamped.faces[1]?.beauty.whitening).toBe(0);
    expect(clamped.faces[0]?.enabled).toBe(false);
  });

  it("每张脸参数独立存储，不互相污染", () => {
    let params = withFaceBeauty(DEFAULT_PARAMS, 3, (face) => ({
      ...face,
      beauty: { ...face.beauty, skinSmooth: 25 },
    }));
    params = withFaceBeauty(params, 8, (face) => ({
      ...face,
      beauty: { ...face.beauty, skinSmooth: 45, blemish: 70 },
    }));
    expect(findFaceBeauty(params, 3)?.beauty.skinSmooth).toBe(25);
    expect(findFaceBeauty(params, 8)?.beauty.skinSmooth).toBe(45);
    expect(findFaceBeauty(params, 8)?.beauty.blemish).toBe(70);
    expect(activeParamCount(params)).toBe(2);
  });

  it("手动蒙版坐标与半径被规范化", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      manualMask: {
        enabled: true,
        invert: false,
        feather: 400,
        strength: -5,
        strokes: [{ erase: false, radius: 0.9, points: [{ x: -1, y: 2 }] }],
      },
      repairPoints: [{ x: -2, y: 3, radius: 1 }],
    });
    expect(clamped.manualMask.feather).toBe(100);
    expect(clamped.manualMask.strength).toBe(0);
    expect(clamped.manualMask.strokes[0]?.radius).toBe(0.25);
    expect(clamped.manualMask.strokes[0]?.points[0]).toEqual({ x: 0, y: 1 });
    expect(clamped.repairPoints[0]).toEqual({ x: 0, y: 1, radius: 0.25 });
  });
});
