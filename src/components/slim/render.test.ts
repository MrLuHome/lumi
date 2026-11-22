import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import App from "../../App";
import { OneClickPanel } from "./OneClickPanel";
import { ManualNotice, ManualPanel } from "./ManualPanel";
import { AdvancedSettings } from "./AdvancedSettings";
import { useEditorStore } from "../../state/editorStore";
import { plainMessage } from "./analysis";
import { DEFAULT_PARAMS } from "../../state/params";
import { ITEM_META, SIX_ITEM_ORDER } from "../../state/slim";
import type { PhotoAsset } from "../../types";

/**
 * 界面冒烟测试：两个入口必须能在「无照片 / 有照片」两种状态下渲染。
 * 用 `renderToStaticMarkup` 直接渲染组件树，任何元素/属性错误都会在这里炸出来。
 */

function photo(id = "slim"): PhotoAsset {
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

const BANNED = [
  "Provider",
  "LEVEL",
  "置信度",
  "语义分割",
  "关键点",
  "Tensor",
  "蒙版",
  "SHA256",
  "ONNX",
  "推理",
  "算法",
];

describe("双入口界面冒烟", () => {
  it("没有照片时两个入口给出可操作的引导，而不是空白", () => {
    const html = renderToStaticMarkup(createElement(App));
    expect(html).toContain("一键精修");
    expect(html).toContain("手动调整");
    expect(html).toContain("导入");
    for (const word of BANNED) expect(html).not.toContain(word);
  });

  it("有照片时一键精修面板可渲染三档与整体强度", () => {
    useEditorStore.setState({ photos: [photo()], activeId: "slim", params: { slim: DEFAULT_PARAMS } });
    const html = renderToStaticMarkup(createElement(OneClickPanel, { photo: photo() }));
    expect(html).toContain("自然");
    expect(html).toContain("精致");
    expect(html).toContain("高级");
    expect(html).toContain("整体强度");
    expect(html).toContain("一键精修");
    expect(html).toContain("对比");
    for (const word of BANNED) expect(html).not.toContain(word);
  });

  it("有照片时手动面板渲染六项，每项都有开关与滑块", () => {
    const html = renderToStaticMarkup(createElement(ManualPanel, { photo: photo() }));
    for (const id of SIX_ITEM_ORDER) {
      // 每项都有开关（role=switch）与滑块（range），且都用中文名做无障碍标签。
      expect(html).toContain(`role="switch" aria-checked="false" aria-label="${ITEM_META[id].label}"`);
      expect(html).toContain(`type="range"`);
    }
    // 没有检测结果时六项必须是禁用的，不能是可点的假按钮。
    expect(html).toContain("is-disabled");
    expect(html).toContain("磨皮美肤");
    expect(html).toContain("祛痘祛斑");
    expect(html).toContain("眼睛美化");
    expect(html).toContain("肤色提亮");
    expect(html).toContain("光影调整");
    expect(html).toContain("照片色调");
    expect(html).toContain("恢复默认");
    expect(html).toContain("高级设置");
    for (const word of BANNED) expect(html).not.toContain(word);
  });

  it("高级设置的旧调整清单会列出仍在生效的旧参数", () => {
    const legacy = {
      ...DEFAULT_PARAMS,
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
    };
    useEditorStore.setState({ params: { slim: legacy } });
    const html = renderToStaticMarkup(
      createElement(AdvancedSettings, {
        photo: photo(),
        params: legacy,
        detection: null,
      }),
    );
    // 折叠态：只显示汇总，不泄漏工程词。
    expect(html).toContain("高级设置");
    expect(html).toContain("1 项旧调整在生效");
    for (const word of BANNED) expect(html).not.toContain(word);
  });

  it("分析失败时给出白话原因与「重试」，而不是静默的假滑块（D1）", () => {
    // 提示区是纯 props 组件：SSR 下 zustand 只会读到初始 state，所以这里直接喂 props。
    const html = renderToStaticMarkup(
      createElement(ManualNotice, {
        faceStatus: "ready",
        hasDetection: true,
        noFace: false,
        faceError: null,
        faceProgress: null,
        faceCount: 1,
        reportStatus: "error",
        reportError: "照片读不出来：文件不见了",
        reportReady: false,
        retrying: false,
        onRetry: () => undefined,
        onRetryFace: () => undefined,
      }),
    );
    expect(html).toContain("重试");
    expect(html).toContain("照片读不出来：文件不见了");
    expect(html).toContain("其余四项不受影响");
    // 失败态不能同时画「正在分析」。
    expect(html).not.toContain("正在看这张照片的光影和色调");
    for (const word of BANNED) expect(html).not.toContain(word);
  });

  it("人脸检测失败也有「重试」入口，不是死路（D1 同类）", () => {
    const html = renderToStaticMarkup(
      createElement(ManualNotice, {
        faceStatus: "error",
        hasDetection: false,
        noFace: false,
        faceError: "找不到照片：/tmp/a.jpg",
        faceProgress: null,
        faceCount: 0,
        reportStatus: "idle",
        reportError: null,
        reportReady: false,
        retrying: false,
        onRetry: () => undefined,
        onRetryFace: () => undefined,
      }),
    );
    expect(html).toContain("重试");
    expect(html).toContain("找不到照片");
  });

  it("分析中的提示是真实进度，未分析时给一句白话说明（D1）", () => {
    const loading = renderToStaticMarkup(
      createElement(ManualNotice, {
        faceStatus: "ready",
        hasDetection: true,
        noFace: false,
        faceError: null,
        faceProgress: null,
        faceCount: 1,
        reportStatus: "loading",
        reportError: null,
        reportReady: false,
        retrying: false,
        onRetry: () => undefined,
        onRetryFace: () => undefined,
      }),
    );
    expect(loading).toContain("正在看这张照片的光影和色调");

    const idle = renderToStaticMarkup(
      createElement(ManualNotice, {
        faceStatus: "ready",
        hasDetection: true,
        noFace: false,
        faceError: null,
        faceProgress: null,
        faceCount: 1,
        reportStatus: "idle",
        reportError: null,
        reportReady: false,
        retrying: false,
        onRetry: () => undefined,
        onRetryFace: () => undefined,
      }),
    );
    expect(idle).toContain("会先看一遍这张照片");
  });

  it("后端错误里的工程词会被替换成白话，但真实原因仍然保留", () => {
    const raw = "人脸模型不可用：ONNX 推理失败，Provider level 3 置信度 0.2，用时 812 ms";
    const plain = plainMessage(raw);
    for (const word of BANNED) expect(plain).not.toContain(word);
    expect(plain).toContain("人脸");
    expect(plain.length).toBeGreaterThan(6);
  });
});
