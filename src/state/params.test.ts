import { describe, expect, it } from "vitest";
import {
  DEFAULT_PARAMS,
  PARAM_GROUPS,
  PARAM_INDEX,
  activeParamCount,
  bodyAdjustIsActive,
  clampParams,
  defaultBackground,
  defaultBeauty,
  defaultBodyAdjust,
  defaultFaceReshape,
  defaultSubjectMask,
  faceReshapeIsActive,
  findFaceBeauty,
  formatParamValue,
  isNeutral,
  paramsEqual,
  paramsKey,
  subjectMaskHasContent,
  withBodyAdjust,
  withFaceBeauty,
  withFaceReshape,
} from "./params";

describe("参数定义", () => {
  const numericKeys = Object.keys(DEFAULT_PARAMS).filter(
    (key) =>
      ![
        "hsl",
        "curves",
        "lut",
        "faces",
        "reshape",
        "subjectMask",
        "body",
        "background",
        "manualMask",
        "repairPoints",
        "skinSmooth",
      ].includes(key),
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
        { faceId: 2, faceUuid: "", enabled: true, beauty: { ...defaultBeauty(), skinSmooth: 500, whitening: -20 } },
        { faceId: 1, faceUuid: "", enabled: false, beauty: defaultBeauty() },
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

describe("面部塑形参数", () => {
  it("默认不产生任何变形", () => {
    const entry = defaultFaceReshape(2, "uuid-2");
    expect(faceReshapeIsActive(entry)).toBe(false);
    expect(entry.faceUuid).toBe("uuid-2");
    expect(isNeutral(DEFAULT_PARAMS)).toBe(true);
    expect(activeParamCount(DEFAULT_PARAMS)).toBe(0);
  });

  it("参数、笔触与坐标都被夹紧", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      reshape: [
        {
          faceId: 3,
          faceUuid: "uuid-3",
          enabled: true,
          face: { slim: 500, left: -400, right: 20, jaw: 0, cheekbone: 0, width: 0, length: 0, balance: 0 },
          eyes: { size: 999, left: 0, right: 0, width: 0, height: 0, distance: 0, corner: 0 },
          nose: { bridge: 0, alar: 0, tip: 0, height: 0, balance: 0 },
          mouth: { width: 0, upper: 0, lower: 0, corner: 0, symmetry: 0, size: 0 },
          chin: { length: 0, width: 0, point: 0, jawline: 0 },
          manual: {
            enabled: true,
            strokes: [
              {
                mode: "push",
                radius: 2,
                strength: 500,
                feather: -20,
                points: [
                  { x: -1, y: 2 },
                  { x: 0.4, y: 0.5 },
                ],
              },
            ],
          },
        },
      ],
    });
    const entry = clamped.reshape[0]!;
    expect(entry.face.slim).toBe(100);
    expect(entry.face.left).toBe(-100);
    expect(entry.eyes.size).toBe(100);
    expect(entry.manual.strokes[0]?.radius).toBe(0.25);
    expect(entry.manual.strokes[0]?.strength).toBe(100);
    expect(entry.manual.strokes[0]?.feather).toBe(0);
    expect(entry.manual.strokes[0]?.points[0]).toEqual({ x: 0, y: 1 });
    expect(activeParamCount(clamped)).toBe(1);
  });

  it("复合更新只改目标人脸并保留其余人脸", () => {
    const first = defaultFaceReshape(1, "uuid-a");
    const second = defaultFaceReshape(2, "uuid-b");
    let params = { ...DEFAULT_PARAMS, reshape: [first, second] };
    params = withFaceReshape(params, 2, "uuid-b", (entry) => ({
      ...entry,
      face: { ...entry.face, slim: 40 },
    }));
    expect(findFaceBeauty(params, 2)).toBeUndefined();
    expect(params.reshape.find((face) => face.faceId === 1)?.face.slim).toBe(0);
    expect(params.reshape.find((face) => face.faceId === 2)?.face.slim).toBe(40);
    expect(params.reshape.find((face) => face.faceId === 2)?.faceUuid).toBe("uuid-b");
  });
});

describe("第五轮人物蒙版与背景参数", () => {
  it("默认蒙版没有笔触且不激活", () => {
    const mask = defaultSubjectMask();
    expect(subjectMaskHasContent(mask)).toBe(false);
    expect(isNeutral(DEFAULT_PARAMS)).toBe(true);
    expect(activeParamCount(DEFAULT_PARAMS)).toBe(0);
  });

  it("笔触、坐标与 Alpha 处理参数都被夹紧", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      subjectMask: {
        ...defaultSubjectMask(),
        enabled: true,
        feather: 500,
        expand: -400,
        smooth: -20,
        strokes: [
          {
            kind: "foreground",
            radius: 2,
            hardness: -5,
            strength: 300,
            points: [
              { x: -1, y: 2 },
              { x: 0.4, y: 0.5 },
            ],
          },
        ],
      },
    });
    expect(clamped.subjectMask.feather).toBe(100);
    expect(clamped.subjectMask.expand).toBe(-100);
    expect(clamped.subjectMask.smooth).toBe(0);
    expect(clamped.subjectMask.strokes[0]?.radius).toBe(0.5);
    expect(clamped.subjectMask.strokes[0]?.hardness).toBe(0);
    expect(clamped.subjectMask.strokes[0]?.strength).toBe(100);
    expect(clamped.subjectMask.strokes[0]?.points[0]).toEqual({ x: 0, y: 1 });
    expect(activeParamCount(clamped)).toBe(1);
  });

  it("身材参数按人物隔离并可撤销到默认", () => {
    let params = withBodyAdjust(DEFAULT_PARAMS, "subject-1", (entry) => ({
      ...entry,
      shape: { ...entry.shape, waist: 40 },
      anchors: [
        { part: "waistLeft", x: -1, y: 0.5, dx: 2, dy: 0, radius: 2, locked: true },
      ],
    }));
    params = withBodyAdjust(clampParams(params), "subject-2", (entry) => ({
      ...entry,
      shape: { ...entry.shape, hips: 25 },
    }));
    expect(params.body).toHaveLength(2);
    expect(params.body[0]?.subjectId).toBe("subject-1");
    expect(params.body[0]?.shape.waist).toBe(40);
    expect(params.body[1]?.shape.hips).toBe(25);
    expect(bodyAdjustIsActive(defaultBodyAdjust("subject-1"))).toBe(false);
    expect(activeParamCount(params)).toBe(2);
  });

  it("背景模式与颜色被校验，空图片路径退回原始背景", () => {
    const clamped = clampParams({
      ...DEFAULT_PARAMS,
      background: {
        ...defaultBackground(),
        enabled: true,
        mode: "image",
        imagePath: "",
        color: "red",
        blur: 400,
      },
    });
    expect(clamped.background.mode).toBe("original");
    expect(clamped.background.blur).toBe(100);
    expect(clamped.background.color).toBe("#1F2933");
    expect(activeParamCount(clamped)).toBe(0);
  });
});
