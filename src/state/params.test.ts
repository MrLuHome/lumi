import { describe, expect, it } from "vitest";
import {
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_INDEX,
  activeParamCount,
  clampParams,
  formatParamValue,
  isNeutral,
  paramsEqual,
  paramsKey,
} from "./params";

describe("参数定义", () => {
  const numericKeys = Object.keys(DEFAULT_PARAMS).filter(
    (key) => !["hsl", "curves", "lut"].includes(key),
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
    expect(formatParamValue(PARAM_INDEX.skinSmooth, 0)).toBe("0");
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
