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
  it("每个 EditParams 字段在参数组中恰好出现一次", () => {
    const keys = PARAM_GROUPS.flatMap((group) => group.params.map((definition) => definition.key));
    expect(keys.sort()).toEqual(Object.keys(DEFAULT_PARAMS).sort());
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("默认值全部为 0 且判定为未调整", () => {
    expect(Object.values(DEFAULT_PARAMS).every((value) => value === 0)).toBe(true);
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
});
