import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  AnalysisReport,
  BeautyAnalysis,
  FaceMetrics,
  FaceSuggestion,
  PhotoAsset,
  RetouchSuggestion,
  SkinModelStatus,
} from "../types";
import { paramsEqual } from "./params";
import {
  DEFAULT_PARAMS,
  defaultBackground,
  defaultBeauty,
  defaultBodyAdjust,
  defaultBodyShape,
  defaultFaceBeauty,
  defaultFaceReshape,
  defaultParams,
} from "./params";
import { buildProjectDoc, buildProjectSnapshot } from "./project";
import { COALESCE_WINDOW_MS, createEditorStore, selectEffectUrl } from "./editorStore";
import {
  DEFAULT_ONE_CLICK_MODE,
  ITEM_META,
  ITEM_PROFILE,
  MODE_FACTOR,
  SIX_ITEM_ORDER,
  applyOneClickPlan,
  auditBrushwork,
  auditLegacyAdjustments,
  buildOneClickPlan,
  buildSixItemWrite,
  clearLegacyAdjustments,
  directionHint,
  invertItemStrength,
  itemsFromParams,
  needsGlobalAnalysis,
  toAppliedPlan,
  tonalAnchors,
  tonalDeltas,
  withAnchors,
  writeSixItem,
  type SixItemId,
} from "./slim";

// ────────────────────────────── 测试脚手架 ──────────────────────────────

function metrics(faceId: number): FaceMetrics {
  return {
    faceId,
    skinRatio: 0.62,
    meanLuma: 0.48,
    texture: 0.22,
    blemishDensity: 0.05,
    chromaVariation: 0.11,
    redness: 0.09,
    darkRatio: 0.12,
    highlightRatio: 0.03,
    darkCircleDelta: 0.08,
    eyeTexture: 0.3,
    foreheadTexture: 0.2,
    nasolabialTexture: 0.2,
  };
}

function suggestion(faceId: number, suggested: Partial<typeof defaultBeauty extends () => infer T ? T : never>) {
  return {
    faceId,
    metrics: metrics(faceId),
    suggested: { ...defaultBeauty(), ...suggested },
    notes: [],
  } satisfies FaceSuggestion;
}

function skinModel(): SkinModelStatus {
  return {
    available: false,
    method: "geometric",
    name: "",
    version: "",
    license: "",
    commercialUse: false,
    sha256: "",
    input: "",
    outputs: "",
    path: null,
    reason: "",
    registeredCount: 0,
  };
}

function beautyAnalysis(faces: FaceSuggestion[]): BeautyAnalysis {
  return {
    faces,
    method: "local",
    model: "local",
    elapsedMs: 12,
    skinModel: skinModel(),
    warnings: [],
  };
}

function paramSuggestion(field: string, current: number, suggested: number): RetouchSuggestion {
  return {
    id: field,
    group: "光影",
    label: field,
    target: "param",
    field,
    current,
    suggested,
    reason: "测试",
    confidence: 0.8,
    applicable: true,
  };
}

function analysisReport(suggestions: RetouchSuggestion[]): AnalysisReport {
  return {
    imageHash: "hash",
    algorithmVersion: "local-analysis-1",
    resultVersion: 1,
    analyzedAtMs: 0,
    mode: "standard",
    faces: 1,
    metrics: [],
    suggestions,
    warnings: [],
    capabilities: {
      faceDetection: true,
      beautyAnalysis: true,
      manualMask: true,
      background: true,
    },
  };
}

function detection(ids: number[]) {
  return {
    faces: ids.map((id) => ({
      id,
      uuid: `uuid-${id}`,
      score: 0.9,
      bbox: { x: 0.2, y: 0.2, width: 0.3, height: 0.3 },
      landmarks: [],
      dense: null,
      level3: null,
    })),
    model: "local",
    elapsedMs: 1,
    warnings: [],
  } as unknown as Parameters<typeof writeSixItem>[1];
}

// ────────────────────────────── 六项映射 ──────────────────────────────

describe("六项映射（手动调整）", () => {
  it("强度 0 时六项全部写入空对象（参数必须回到中性，预览才能复用原图）", () => {
    for (const id of SIX_ITEM_ORDER) {
      if (ITEM_META[id].adjustment.length > 0) {
        expect(
          writeSixItem(DEFAULT_PARAMS, null, id, 0, { anchors: {}, deltas: {} }),
        ).toEqual(DEFAULT_PARAMS);
        continue;
      }
      expect(buildSixItemWrite(id, 0)).toEqual({});
      const next = writeSixItem(DEFAULT_PARAMS, null, id, 0);
      expect(next.faces).toEqual([]);
    }
  });

  it("强度 100 等于文档给定的剖面", () => {
    expect(buildSixItemWrite("skin", 100)).toEqual({ skinSmooth: 70, texture: 55, detail: 28 });
    expect(buildSixItemWrite("blemish", 100)).toEqual({ blemish: 90, spots: 60, blackheads: 35 });
    expect(buildSixItemWrite("eyes", 100)).toEqual({ darkCircles: 90, eyeLines: 70 });
    expect(buildSixItemWrite("tone", 100)).toEqual({
      whitening: 50,
      toneEven: 65,
      dullness: 45,
      redness: 40,
    });
  });

  it("任意强度下 spots 都不超过 blemish（引擎取 max，否则滑块不单调）", () => {
    for (let s = 0; s <= 100; s += 1) {
      const write = buildSixItemWrite("blemish", s);
      const blemish = write.blemish ?? 0;
      const spots = write.spots ?? 0;
      expect(spots).toBeLessThanOrEqual(blemish);
    }
  });

  it("强度递增时每个字段都不下降（单调）", () => {
    for (const id of ["skin", "blemish", "eyes", "tone"] as SixItemId[]) {
      let previous: Record<string, number> = {};
      for (let s = 0; s <= 100; s += 10) {
        const write = buildSixItemWrite(id, s);
        for (const [key, value] of Object.entries(write)) {
          expect(value).toBeGreaterThanOrEqual(previous[key] ?? 0);
        }
        previous = { ...previous, ...write };
      }
    }
  });

  it("眼睛美化只碰黑眼圈与眼下细纹，绝不碰眼形或塑形", () => {
    const write = buildSixItemWrite("eyes", 100);
    expect(Object.keys(write).sort()).toEqual(["darkCircles", "eyeLines"]);
    const next = writeSixItem(DEFAULT_PARAMS, detection([1]), "eyes", 100);
    expect(next.reshape).toEqual([]);
    for (const face of next.faces) {
      expect(face.beauty.skinSmooth).toBe(0);
      expect(face.beauty.blemish).toBe(0);
    }
  });

  it("写入会覆盖所有已检测人脸，并为缺条目的人脸自动建条目", () => {
    const next = writeSixItem(DEFAULT_PARAMS, detection([1, 2]), "skin", 60);
    expect(next.faces.map((face) => face.faceUuid)).toEqual(["uuid-1", "uuid-2"]);
    for (const face of next.faces) {
      expect(face.beauty.skinSmooth).toBe(42);
    }
  });

  it("磨皮写入会同时清掉旧的全图磨皮，避免滑块是 0 皮肤还在磨", () => {
    const base = { ...defaultParams(), skinSmooth: 70 };
    const next = writeSixItem(base, detection([1]), "skin", 0);
    expect(next.skinSmooth).toBe(0);
  });

  it("⑤⑥ 在「未分析 / 分析中 / 上次失败」时都必须先重新分析（D1 判定表）", () => {
    const ready = { report: analysisReport([paramSuggestion("exposure", 0, 18)]), status: "ready" };
    const loading = { report: null, status: "loading" };
    const failed = { report: null, status: "error" };
    expect(needsGlobalAnalysis("light", null)).toBe(true);
    expect(needsGlobalAnalysis("color", null)).toBe(true);
    expect(needsGlobalAnalysis("light", loading)).toBe(true);
    expect(needsGlobalAnalysis("light", failed)).toBe(true);
    expect(needsGlobalAnalysis("light", ready)).toBe(false);
    // ①②③④ 永远不需要先分析。
    expect(needsGlobalAnalysis("skin", null)).toBe(false);
    expect(needsGlobalAnalysis("blemish", failed)).toBe(false);
  });

  it("光影/色调没有建议时返回空写入，而不是写 0 偏移", () => {
    const base = { ...defaultParams(), exposure: 8 };
    const next = writeSixItem(base, null, "light", 60, { anchors: {}, deltas: {} });
    expect(next).toEqual(base);
    expect(buildSixItemWrite("light", 100)).toEqual({});
  });

  it("光影/色调按「锚点 + 建议增量 × 强度」写入", () => {
    const base = { ...defaultParams(), exposure: 4 };
    const next = writeSixItem(base, null, "light", 50, {
      anchors: { exposure: 4 },
      deltas: { exposure: 20 },
    });
    expect(next.exposure).toBe(14);
    const zero = writeSixItem(base, null, "light", 0, {
      anchors: { exposure: 4 },
      deltas: { exposure: 20 },
    });
    expect(zero.exposure).toBe(4);
  });

  it("写入值不会越过安全上限", () => {
    const next = writeSixItem({ ...defaultParams(), exposure: 25 }, null, "light", 100, {
      anchors: { exposure: 25 },
      deltas: { exposure: 20 },
    });
    expect(next.exposure).toBe(30);
  });
});

// ────────────────────────────── 反解强度 ──────────────────────────────

describe("反解强度（两个入口不能各存一份滑块状态）", () => {
  it("写入后能反解回同一个强度", () => {
    for (const strength of [0, 17, 40, 60, 88, 100]) {
      const params = writeSixItem(DEFAULT_PARAMS, detection([1]), "skin", strength);
      const inverted = invertItemStrength(params, "skin");
      expect(inverted.strength).toBe(strength);
      expect(inverted.manual).toBe(false);
    }
  });

  it("老项目只设了 skinSmooth=70、纹理与细节为 0 时给出最接近的强度并标记为微调过", () => {
    const legacy = {
      ...defaultParams(),
      faces: [{ faceId: 1, faceUuid: "uuid-1", enabled: true, beauty: { ...defaultBeauty(), skinSmooth: 70 } }],
    };
    const inverted = invertItemStrength(legacy, "skin");
    expect(inverted.strength).toBeGreaterThan(40);
    expect(inverted.strength).toBeLessThan(100);
    expect(inverted.manual).toBe(true);
  });

  it("光影反解用锚点与建议增量还原强度", () => {
    const ctx = { anchors: { exposure: 0 }, deltas: { exposure: 20 } };
    const params = writeSixItem(DEFAULT_PARAMS, null, "light", 75, ctx);
    expect(params.exposure).toBe(15);
    expect(invertItemStrength(params, "light", ctx).strength).toBe(75);
  });

  it("正负建议混在一起时也能反解出正确强度", () => {
    const ctx = { anchors: { exposure: 0, highlights: 0 }, deltas: { exposure: 18, highlights: -12 } };
    const params = writeSixItem(DEFAULT_PARAMS, null, "light", 60, ctx);
    expect(params.exposure).toBeCloseTo(10.8, 1);
    expect(params.highlights).toBeCloseTo(-7.2, 1);
    expect(invertItemStrength(params, "light", ctx).strength).toBe(60);
  });

  it("itemsFromParams 与写入保持同一口径", () => {
    let params = DEFAULT_PARAMS;
    params = writeSixItem(params, detection([1]), "blemish", 60);
    params = writeSixItem(params, detection([1]), "tone", 30);
    const items = itemsFromParams(params);
    expect(items.blemish).toEqual({ enabled: true, strength: 60 });
    expect(items.tone).toEqual({ enabled: true, strength: 30 });
    expect(items.eyes).toEqual({ enabled: false, strength: 0 });
  });
});

// ────────────────────────────── 一键精修 ──────────────────────────────

describe("一键精修计划", () => {
  const beauty = beautyAnalysis([
    suggestion(1, {
      skinSmooth: 60,
      texture: 40,
      detail: 20,
      blemish: 40,
      spots: 30,
      blackheads: 20,
      darkCircles: 50,
      eyeLines: 30,
      whitening: 30,
      toneEven: 40,
      dullness: 30,
      redness: 20,
      foreheadLines: 40,
      nasolabial: 35,
    }),
  ]);
  const report = analysisReport([
    paramSuggestion("exposure", 0, 18),
    paramSuggestion("contrast", 0, -8),
    paramSuggestion("temperature", 0, 12),
  ]);

  const baseInput = {
    photoId: "a",
    mode: DEFAULT_ONE_CLICK_MODE,
    overall: 100,
    beauty,
    report,
    current: { ...defaultParams(), faces: [] },
    applied: null,
    anchors: {},
    detection: detection([1]),
  };

  it("三档系数单调递增；皮肤类高级档为满档，调光类刻意弱于皮肤", () => {
    for (const id of SIX_ITEM_ORDER) {
      expect(MODE_FACTOR.natural[id]).toBeLessThan(MODE_FACTOR.refined[id]);
      expect(MODE_FACTOR.refined[id]).toBeLessThan(MODE_FACTOR.advanced[id]);
    }
    for (const id of ["skin", "blemish", "eyes", "tone"] as SixItemId[]) {
      expect(MODE_FACTOR.advanced[id]).toBe(1);
    }
    // 曝光作用于全图，权重必须显著低于局部皮肤，否则观感只剩「稍微变亮」。
    expect(MODE_FACTOR.advanced.light).toBeLessThan(0.7);
    expect(MODE_FACTOR.advanced.color).toBeLessThan(0.6);
  });

  it("皮肤类字段 = 满档剖面 × 档位增益 × 问题分析调节，三档单调递增", () => {
    const natural = buildOneClickPlan({ ...baseInput, mode: "natural" });
    const refined = buildOneClickPlan({ ...baseInput, mode: "refined" });
    const advanced = buildOneClickPlan({ ...baseInput, mode: "advanced" });
    const valueOf = (plan: typeof natural, key: "skinSmooth") => plan.faces[0]?.beauty[key] ?? 0;
    // 建议 60 已到 skinSmooth 建议上限 55 → severity 封顶 1.15。
    const round1 = (v: number) => Math.round(v * 10) / 10; // 写入端保留一位小数
    expect(valueOf(natural, "skinSmooth")).toBe(round1(70 * 0.45 * 1.15));
    expect(valueOf(refined, "skinSmooth")).toBe(round1(70 * 0.7 * 1.15));
    expect(valueOf(advanced, "skinSmooth")).toBe(round1(70 * 1.15));
    expect(valueOf(natural, "skinSmooth")).toBeLessThan(valueOf(refined, "skinSmooth"));
    expect(valueOf(refined, "skinSmooth")).toBeLessThan(valueOf(advanced, "skinSmooth"));
  });

  it("典型 AI 建议下三档 skinSmooth 精确落在 31.5 / 49 / 70，不再只有手动满档的 20%", () => {
    // 典型建议 27.5（分析器常见 22~33，上限 55）→ severity = 0.85 + 0.3×0.5 = 1.0。
    const typical = beautyAnalysis([suggestion(1, { skinSmooth: 27.5 })]);
    const valueOf = (mode: "natural" | "refined" | "advanced") =>
      buildOneClickPlan({ ...baseInput, mode, beauty: typical }).faces[0]?.beauty.skinSmooth ?? 0;
    expect(valueOf("natural")).toBeCloseTo(31.5, 1);
    expect(valueOf("refined")).toBeCloseTo(49, 1);
    expect(valueOf("advanced")).toBeCloseTo(70, 1);
  });

  it("一键路径 exposure 相对锚点最多移动 ±8（手动路径不受限）", () => {
    const strong = analysisReport([paramSuggestion("exposure", 0, 100)]);
    const plan = buildOneClickPlan({ ...baseInput, mode: "advanced", report: strong });
    expect(plan.params.exposure).toBe(8);
    const dark = analysisReport([paramSuggestion("exposure", 0, -100)]);
    expect(buildOneClickPlan({ ...baseInput, mode: "advanced", report: dark }).params.exposure).toBe(-8);
  });

  it("纹理保护类字段（texture/detail）不得随档位上升，否则高档抵消磨皮力度", () => {
    const valueOf = (mode: "natural" | "refined" | "advanced", key: "texture" | "detail") =>
      buildOneClickPlan({ ...baseInput, mode }).faces[0]?.beauty[key] ?? 0;
    for (const key of ["texture", "detail"] as const) {
      const natural = valueOf("natural", key);
      const refined = valueOf("refined", key);
      const advanced = valueOf("advanced", key);
      // 旧逻辑把 texture/detail 与 skinSmooth 乘同一档位增益（0.45/0.7/1.0），
      // 引擎 eps/retention 随之上升，恰好抵消 skinSmooth 提升 → 三档肉眼不可分辨。
      expect(refined).toBeLessThanOrEqual(natural);
      expect(advanced).toBeLessThanOrEqual(refined);
      // 但仍要保住「不塑料脸」的纹理保护底线，不能归零。
      expect(advanced).toBeGreaterThan(0);
    }
  });

  it("三档曝光有可分辨梯度，不得有两档都撞到同一个 clamp 上限", () => {
    const exposureOf = (mode: "natural" | "refined" | "advanced") =>
      buildOneClickPlan({ ...baseInput, mode }).params.exposure ?? 0;
    const natural = exposureOf("natural");
    const refined = exposureOf("refined");
    const advanced = exposureOf("advanced");
    expect(natural).toBeLessThan(refined);
    expect(refined).toBeLessThan(advanced);
    // baseInput 的 exposure 建议是 18（足够大）：旧逻辑下精致/高级都会撞到全局上限 8。
    expect([natural, refined, advanced].filter((v) => v === 8).length).toBeLessThanOrEqual(1);
  });

  it("没痘时不硬塞祛痘组（瑕疵密度无信号）", () => {
    const clean = beautyAnalysis([
      {
        ...suggestion(1, { skinSmooth: 27.5, blemish: 40, spots: 30, blackheads: 20 }),
        metrics: { ...metrics(1), blemishDensity: 0.0001 },
      },
    ]);
    const plan = buildOneClickPlan({ ...baseInput, beauty: clean, mode: "advanced" });
    for (const key of ["blemish", "spots", "blackheads"] as const) {
      expect(plan.faces[0]?.beauty[key]).toBeUndefined();
    }
    // 其它皮肤项照常写入。
    expect(plan.faces[0]?.beauty.skinSmooth).toBeGreaterThan(0);
  });

  it("写入的人脸条目即使老项目里 enabled=false，也会被强制启用", () => {
    const legacy = {
      ...defaultParams(),
      faces: [
        { faceId: 1, faceUuid: "uuid-1", enabled: false, beauty: defaultBeauty() },
      ],
    };
    const plan = buildOneClickPlan({ ...baseInput, current: legacy });
    expect(plan.faces.length).toBeGreaterThan(0);
    expect(applyOneClickPlan(legacy, plan).faces[0]?.enabled).toBe(true);
  });

  it("一键计划永不写入额头细纹与法令纹（主理人裁决 Q1）", () => {
    const plan = buildOneClickPlan({ ...baseInput, mode: "advanced" });
    for (const face of plan.faces) {
      expect(face.beauty.foreheadLines).toBeUndefined();
      expect(face.beauty.nasolabial).toBeUndefined();
    }
    const next = applyOneClickPlan(baseInput.current, plan);
    for (const face of next.faces) {
      expect(face.beauty.foreheadLines).toBe(0);
      expect(face.beauty.nasolabial).toBe(0);
    }
  });

  it("整体强度 0 时什么都不写", () => {
    const plan = buildOneClickPlan({ ...baseInput, overall: 0 });
    expect(plan.fields).toBe(0);
    expect(plan.faces).toEqual([]);
    expect(plan.params).toEqual({});
  });

  it("G14：一键计划可写入瘦脸 reshape 与瘦身 body", () => {
    const plan = buildOneClickPlan({ ...baseInput, mode: "advanced" });
    const withShape = {
      ...plan,
      reshape: [
        {
          faceId: 1,
          faceUuid: "uuid-1",
          face: {
            slim: 28,
            left: 0,
            right: 0,
            jaw: 12,
            cheekbone: 8,
            width: 10,
            length: 0,
            balance: 0,
          },
        },
      ],
      body: [
        {
          subjectId: "subject-1",
          shape: {
            waist: -30,
            shoulders: 0,
            hips: -12,
            thighs: -10,
            calves: 0,
            arms: -8,
            legs: 0,
          },
        },
      ],
      fields: plan.fields + 2,
    };
    const next = applyOneClickPlan(baseInput.current, withShape);
    const reshape = next.reshape.find((e) => e.faceUuid === "uuid-1" || e.faceId === 1);
    expect(reshape?.enabled).toBe(true);
    expect(reshape?.face.slim).toBe(28);
    const body = next.body.find((e) => e.subjectId === "subject-1");
    expect(body?.enabled).toBe(true);
    expect(body?.shape.waist).toBe(-30);
    // 不带 shape 的计划不碰 reshape/body。
    const plain = applyOneClickPlan(baseInput.current, plan);
    expect(plain.reshape).toEqual(baseInput.current.reshape);
    expect(plain.body).toEqual(baseInput.current.body);
  });

  it("没有 applied 记录且当前已有调整时要先确认，不静默覆盖", () => {
    const current = {
      ...defaultParams(),
      exposure: 20,
      faces: [
        { faceId: 1, faceUuid: "uuid-1", enabled: true, beauty: { ...defaultBeauty(), skinSmooth: 60 } },
      ],
    };
    const plan = buildOneClickPlan({ ...baseInput, current });
    expect(plan.needsConfirm).toBe(true);
    expect(plan.conflicts).toBeGreaterThan(0);
  });

  it("用户手动改过的字段被保留，并统计保留数量", () => {
    const first = buildOneClickPlan({ ...baseInput, mode: "advanced" });
    const applied = toAppliedPlan(first, "a", "advanced", 100);
    const written = applyOneClickPlan(baseInput.current, first);
    // 用户把磨皮从 60 手动改成 20。
    const edited = {
      ...written,
      faces: written.faces.map((face) => ({ ...face, beauty: { ...face.beauty, skinSmooth: 20 } })),
    };
    // 换到精致档重新生成计划：磨皮必须保留用户值，其它字段按新档位重算。
    const plan = buildOneClickPlan({
      ...baseInput,
      mode: "refined",
      current: edited,
      applied,
    });
    expect(plan.kept).toBeGreaterThan(0);
    expect(plan.faces[0]?.beauty.skinSmooth).toBeUndefined();
    // texture/detail 已不随档位变化，换档时它们不需要重写；这里检查真正随档位重算的字段。
    expect(plan.faces[0]?.beauty.blemish).toBeGreaterThan(0);
    // 用户值原样留在参数里。
    expect(applyOneClickPlan(edited, plan).faces[0]?.beauty.skinSmooth).toBe(20);
  });

  it("第二次一键（参数没变）不产生任何写入，界面可以提示「已经是最新效果」", () => {
    const first = buildOneClickPlan({ ...baseInput, mode: "advanced" });
    const applied = toAppliedPlan(first, "a", "advanced", 100);
    const current = applyOneClickPlan(baseInput.current, first);
    const second = buildOneClickPlan({ ...baseInput, mode: "advanced", current, applied });
    expect(second.fields).toBe(0);
    expect(second.kept).toBe(0);
  });

  it("写入不破坏六项之外的参数，并会自动补建人脸条目", () => {
    const current = { ...defaultParams(), clarity: 22, grain: 9 };
    const plan = buildOneClickPlan({ ...baseInput, current });
    const next = applyOneClickPlan(current, plan);
    expect(next.clarity).toBe(22);
    expect(next.grain).toBe(9);
    expect(next.faces).toHaveLength(1);
    expect(next.faces[0]?.faceUuid).toBe("uuid-1");
  });

  it("分析时把光影色调恢复到锚点，避免二次缩放与「建议自我吞并」", () => {
    const anchors = tonalAnchors(report);
    expect(anchors).toEqual({ exposure: 0, contrast: 0, temperature: 0 });
    const deltas = tonalDeltas(report);
    expect(deltas.exposure).toBe(18);
    const current = { ...defaultParams(), exposure: 18 };
    const restored = withAnchors(current, anchors);
    expect(restored.exposure).toBe(0);
  });

  it("给出了白话方向说明，且不含工程术语", () => {
    const hint = directionHint("light", tonalDeltas(report));
    expect(hint).toContain("偏暗");
    expect(hint).toContain("向右");
    for (const banned of ["模型", "置信度", "分割", "关键点", "Tensor", "LEVEL", "Provider", "SHA256", "ONNX"]) {
      expect(hint ?? "").not.toContain(banned);
    }
  });
});

// ────────────────────────────── 旧调整清理 ──────────────────────────────

describe("旧调整清理", () => {
  it("能发现仍在生效、但新界面看不到的旧调整，并能一键清除", () => {
    const legacy = {
      ...defaultParams(),
      hsl: { ...DEFAULT_PARAMS.hsl, red: { hue: 12, saturation: 0, luminance: 0 } },
      lut: { path: "/tmp/a.cube", name: "a", strength: 80 },
      skinSmooth: 55,
      reshape: [
        {
          faceId: 1,
          faceUuid: "uuid-1",
          enabled: true,
          face: { slim: 30, left: 0, right: 0, jaw: 0, cheekbone: 0, width: 0, length: 0, balance: 0 },
          eyes: { size: 0, left: 0, right: 0, width: 0, height: 0, distance: 0, corner: 0 },
          nose: { bridge: 0, alar: 0, tip: 0, height: 0, balance: 0 },
          mouth: { width: 0, upper: 0, lower: 0, corner: 0, symmetry: 0, size: 0 },
          chin: { length: 0, width: 0, point: 0, jawline: 0 },
          manual: { enabled: false, strokes: [] },
        },
      ],
      faces: [
        {
          faceId: 1,
          faceUuid: "uuid-1",
          enabled: true,
          beauty: { ...defaultBeauty(), foreheadLines: 40, nasolabial: 30 },
        },
      ],
    };
    const found = auditLegacyAdjustments(legacy);
    const ids = found.map((item) => item.id);
    expect(ids).toContain("reshape");
    expect(ids).toContain("lut");
    expect(ids).toContain("hsl");
    expect(ids).toContain("legacySkinSmooth");
    expect(ids).toContain("legacyLines");

    const cleared = clearLegacyAdjustments(legacy);
    expect(auditLegacyAdjustments(cleared)).toEqual([]);
    expect(cleared.skinSmooth).toBe(0);
    expect(cleared.faces[0]?.beauty.foreheadLines).toBe(0);
    expect(auditLegacyAdjustments(DEFAULT_PARAMS)).toEqual([]);
  });
});

describe("六项文案", () => {
  it("六项都有开关名与白话说明，且不含禁用词", () => {
    const banned = ["模型", "置信度", "语义分割", "关键点", "Tensor", "蒙版", "LEVEL", "Provider", "SHA256", "ONNX", "推理"];
    for (const id of SIX_ITEM_ORDER) {
      const meta = ITEM_META[id];
      expect(meta.label.length).toBeGreaterThan(1);
      expect(meta.hint.length).toBeGreaterThan(4);
      for (const word of banned) {
        expect(meta.label).not.toContain(word);
        expect(meta.hint).not.toContain(word);
      }
    }
    expect(ITEM_PROFILE.skin.skinSmooth).toBe(70);
  });
});

// ────────────────────────────── store 契约 ──────────────────────────────

function makePhoto(id: string): PhotoAsset {
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
  };
}

describe("store 契约（手动六项）", () => {
  function setup() {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setFaces("a", detection([1])!);
    return store;
  }

  it("一次拖动只产生一条撤销记录，撤销后回到中性", () => {
    const store = setup();
    store.getState().applySlimItem("skin", 60);
    const stack = store.getState().history["a"];
    expect(stack?.past).toHaveLength(1);
    expect(store.getState().params["a"]?.faces[0]?.beauty.skinSmooth).toBe(42);
    store.getState().undo();
    expect(store.getState().params["a"]?.faces[0]?.beauty.skinSmooth).toBe(0);
    // 回到中性后预览直接复用原图，不发后端。
    expect(selectEffectUrl(store.getState())).toBe("data:image/jpeg;base64,preview");
  });

  it("单项重置只影响该项，其它五项保持不动", () => {
    const store = setup();
    store.getState().applySlimItem("skin", 60);
    store.getState().applySlimItem("tone", 40);
    const toneBefore = store.getState().params["a"]?.faces[0]?.beauty.toneEven;
    store.getState().resetSlimItem("skin");
    const params = store.getState().params["a"]!;
    expect(params.faces[0]?.beauty.skinSmooth).toBe(0);
    expect(params.faces[0]?.beauty.texture).toBe(0);
    expect(params.faces[0]?.beauty.toneEven).toBe(toneBefore);
  });

  it("关闭开关写回 0 并把强度记住，再打开回到原来的力度", () => {
    const store = setup();
    store.getState().applySlimItem("blemish", 55);
    expect(store.getState().slimItems.blemish.strength).toBe(55);
    store.getState().setSlimItemEnabled("blemish", false);
    expect(store.getState().params["a"]?.faces[0]?.beauty.blemish).toBe(0);
    expect(store.getState().slimItems.blemish.strength).toBe(55);
    store.getState().setSlimItemEnabled("blemish", true);
    expect(store.getState().params["a"]?.faces[0]?.beauty.blemish).toBeCloseTo(49.5, 1);
  });

  it("旧调整清理进入撤销栈，且清除后审计为空", () => {
    const store = setup();
    store.getState().updateParam("clarity", 30);
    expect(auditLegacyAdjustments(store.getState().params["a"]!).length).toBeGreaterThan(0);
    store.getState().clearLegacyAdjustments();
    expect(auditLegacyAdjustments(store.getState().params["a"]!)).toEqual([]);
    store.getState().undo();
    expect(store.getState().params["a"]?.clarity).toBe(30);
  });

  it("旧调整清理不碰手绘内容：默认保留笔触与点选修复点（D5）", () => {
    const store = setup();
    store.getState().updateParams(
      (current) => ({
        ...current,
        clarity: 25,
        manualMask: {
          ...current.manualMask,
          enabled: true,
          strokes: [{ erase: false, radius: 0.05, points: [{ x: 0.4, y: 0.4 }] }],
        },
        subjectMask: {
          ...current.subjectMask,
          enabled: true,
          strokes: [
            { kind: "paint" as const, radius: 0.05, hardness: 50, strength: 100, points: [{ x: 0.5, y: 0.5 }] },
          ],
        },
        repairPoints: [{ x: 0.3, y: 0.3, radius: 0.02 }],
      }),
      "手绘",
    );

    const ids = auditLegacyAdjustments(store.getState().params["a"]!).map((item) => item.id);
    expect(ids).toContain("detail");
    expect(ids).not.toContain("manualMask");
    expect(ids).not.toContain("subjectMask");
    expect(ids).not.toContain("repairPoints");

    const brush = auditBrushwork(store.getState().params["a"]!);
    expect(brush.strokes).toBe(2);
    expect(brush.repairPoints).toBe(1);

    // 默认不带 includeBrushwork：手绘内容必须原样保留。
    store.getState().clearLegacyAdjustments();
    const kept = store.getState().params["a"]!;
    expect(kept.clarity).toBe(0);
    expect(kept.manualMask.strokes).toHaveLength(1);
    expect(kept.subjectMask.strokes).toHaveLength(1);
    expect(kept.repairPoints).toHaveLength(1);

    // 用户显式勾选后才清。
    store.getState().clearLegacyAdjustments(true);
    const cleared = store.getState().params["a"]!;
    expect(cleared.manualMask.strokes).toHaveLength(0);
    expect(cleared.subjectMask.strokes).toHaveLength(0);
    expect(cleared.repairPoints).toHaveLength(0);
  });

  it("连发写入在 600ms 合并窗口内只留一条撤销记录，超过窗口才开新条目（D6）", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-22T08:00:00Z"));
    const store = setup();

    store.getState().applySlimItem("skin", 20);
    vi.advanceTimersByTime(COALESCE_WINDOW_MS - 100);
    store.getState().applySlimItem("skin", 45);
    expect(store.getState().history["a"]?.past).toHaveLength(1);

    vi.advanceTimersByTime(COALESCE_WINDOW_MS + 100);
    store.getState().applySlimItem("skin", 70);
    expect(store.getState().history["a"]?.past).toHaveLength(2);

    // 换到另一项一定开新条目（合并键是 slim:<id>）。
    store.getState().applySlimItem("eyes", 30);
    expect(store.getState().history["a"]?.past).toHaveLength(3);
  });

  it("老项目（既有 .lumi 结构）里的塑形/身材/背景/分色/滤镜在加载后仍能被发现并清除", () => {
    // 用项目层真实往返一次：写盘 → 读盘 → 审计。clampParams 若丢了这些字段，
    // 「旧调整清理」就会失效，用户再也关不掉看不到的改动。
    const source = createEditorStore();
    source.getState().addPhotos([makePhoto("a")]);
    source.getState().updateParams((current) => {
      const reshape = defaultFaceReshape(1, "uuid-1");
      return {
        ...current,
        skinSmooth: 55,
        hsl: { ...current.hsl, red: { hue: 12, saturation: 0, luminance: 0 } },
        lut: { path: "/tmp/old.cube", name: "old", strength: 80 },
        reshape: [{ ...reshape, face: { ...reshape.face, slim: 30 } }],
        body: [
          {
            ...defaultBodyAdjust("subject-1"),
            shape: { ...defaultBodyShape(), waist: 20 },
          },
        ],
        background: { ...defaultBackground(), enabled: true, mode: "blur" },
        faces: [
          {
            ...defaultFaceBeauty(1, "uuid-1"),
            beauty: { ...defaultBeauty(), nasolabial: 22 },
          },
        ],
      };
    }, "构造老项目");

    const doc = buildProjectDoc(source.getState());
    const restored = createEditorStore();
    restored.getState().addPhotos([makePhoto("a")]);
    const requestId = restored.getState().beginProjectLoad("fixture.lumi");
    const snapshot = buildProjectSnapshot([makePhoto("a")], doc, "fixture");
    expect(restored.getState().commitProjectSnapshot(requestId, snapshot)).toBe(true);
    restored.getState().finishProjectLoad(requestId);

    const found = auditLegacyAdjustments(restored.getState().params["a"]!).map((item) => item.id);
    expect(found).toEqual(
      expect.arrayContaining([
        "reshape",
        "body",
        "background",
        "hsl",
        "lut",
        "legacySkinSmooth",
        "legacyLines",
      ]),
    );

    restored.getState().clearLegacyAdjustments();
    expect(auditLegacyAdjustments(restored.getState().params["a"]!)).toEqual([]);
  });
});

// ────────────────────────────── ⑤⑥ 分析失败后的恢复（D1） ──────────────────────────────

describe("⑤⑥ 分析失败的恢复路径（D1）", () => {
  function setup() {
    const store = createEditorStore();
    store.getState().addPhotos([makePhoto("a")]);
    store.getState().setFaces("a", detection([1])!);
    return store;
  }

  it("分析中/失败时 report 必须是 null，不能留占位报告", () => {
    const store = setup();
    store.getState().setSlimReportStatus("a", "loading");
    expect(store.getState().slimReports["a"]?.report).toBeNull();
    store.getState().setSlimReportStatus("a", "error", "照片读不出来");
    expect(store.getState().slimReports["a"]?.report).toBeNull();
    expect(store.getState().slimReports["a"]?.status).toBe("error");
    expect(store.getState().slimReports["a"]?.error).toBe("照片读不出来");
  });

  it("失败期间拖 ⑤⑥ 不产生假写入：参数与撤销栈都不动，也不留假的开关记忆", () => {
    const store = setup();
    store.getState().setSlimReportStatus("a", "error", "boom");
    const before = store.getState().params["a"]!;
    store.getState().applySlimItem("light", 60);
    expect(paramsEqual(before, store.getState().params["a"]!)).toBe(true);
    expect(store.getState().history["a"]?.past ?? []).toHaveLength(0);
    expect(store.getState().slimItems.light.enabled).toBe(false);
    expect(store.getState().slimItems.light.strength).toBe(0);
  });

  it("重试成功（report 真值回来）后同一个滑块立即生效", () => {
    const store = setup();
    store.getState().setSlimReportStatus("a", "error", "boom");
    expect(store.getState().slimReports["a"]?.report).toBeNull();
    // 模拟「重试」拿到真实结果
    store.getState().setSlimReport(
      "a",
      analysisReport([paramSuggestion("exposure", 0, 18)]),
      { exposure: 0 },
      9,
    );
    expect(store.getState().slimReports["a"]?.status).toBe("ready");
    store.getState().applySlimItem("light", 60);
    expect(store.getState().params["a"]?.exposure).toBeCloseTo(10.8, 1);
    expect(store.getState().history["a"]?.past).toHaveLength(1);
  });

  it("失败后再成功一次，不会把旧的占位结论当成「已分析」", () => {
    const store = setup();
    store.getState().setSlimReportStatus("a", "loading");
    store.getState().setSlimReportStatus("a", "error", "boom");
    // 没有结论时反解结果是 0，而不是某个凭空的强度
    expect(invertItemStrength(store.getState().params["a"]!, "light", {}).strength).toBe(0);
    store.getState().setSlimReport("a", analysisReport([]), {}, 5);
    expect(store.getState().slimReports["a"]?.report).not.toBeNull();
  });
});

afterEach(() => {
  vi.useRealTimers();
});
