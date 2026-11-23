/**
 * S1.2 一键精修计划桥（诊断三阶段之 2）。
 *
 * 用**真实的** `buildOneClickPlan` / `applyOneClickPlan` 把 Rust 侧 dump 的
 * 分析结果转成三档 EditParams，供 `phase_render` 渲染。
 *
 * 未设置 `S1_2_DIR` 时整组跳过（vitest 显示 skipped，不冒充通过）。
 */
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { applyOneClickPlan, buildOneClickPlan, tonalAnchors } from "./slim";
import type { OneClickMode } from "./slim";
import { DEFAULT_PARAMS, defaultFaceBeauty, defaultFaceReshape } from "./params";
import type { EditParams } from "./params";
import type { AnalysisReport, BeautyAnalysis, FaceDetectionResult } from "../types";

const dir = process.env.S1_2_DIR ?? "";
const enabled = dir.length > 0 && existsSync(join(dir, "manifest.json"));
const MODES: OneClickMode[] = ["natural", "refined", "advanced"];

interface DumpInput {
  photo: string;
  category: string;
  overall: number;
  detection: FaceDetectionResult;
  beauty: BeautyAnalysis;
  report: AnalysisReport;
}

describe.skipIf(!enabled)("S1.2 真实 buildOneClickPlan 计划桥", () => {
  it("为每张 dump 照片生成三档参数", () => {
    const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8")) as {
      stem: string;
    }[];
    expect(manifest.length).toBeGreaterThan(0);

    for (const { stem } of manifest) {
      const inputPath = join(dir, `${stem}.input.json`);
      expect(existsSync(inputPath), `缺少 ${inputPath}`).toBe(true);
      const input = JSON.parse(readFileSync(inputPath, "utf8")) as DumpInput;

      // 镜像 editorStore.setFaces：检测结果先对齐出中性、enabled 的人脸条目。
      const current: EditParams = {
        ...DEFAULT_PARAMS,
        faces: input.detection.faces.map((face) => defaultFaceBeauty(face.id, face.uuid)),
        reshape: input.detection.faces.map((face) => defaultFaceReshape(face.id, face.uuid)),
      };
      const anchors = tonalAnchors(input.report);

      for (const mode of MODES) {
        const plan = buildOneClickPlan({
          photoId: `diag-${stem}`,
          mode,
          overall: input.overall,
          beauty: input.beauty,
          report: input.report,
          current,
          applied: null,
          anchors,
          detection: input.detection,
        });
        const next = applyOneClickPlan(current, plan);
        writeFileSync(
          join(dir, `${stem}.params.${mode}.json`),
          JSON.stringify(next, null, 2),
        );
        writeFileSync(
          join(dir, `${stem}.plan.${mode}.json`),
          JSON.stringify({ mode, overall: input.overall, plan }, null, 2),
        );
      }
      console.log(`[plan-bridge] ${stem}: 三档参数已生成`);
    }
  });
});
