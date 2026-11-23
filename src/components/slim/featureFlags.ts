/**
 * 精简版的功能开关。
 *
 * 产品要求：主界面只保留「一键精修」「手动调整」两个入口；
 * 旧面板代码**不删**，只用这里控制可达性，便于回滚与老用户找回能力。
 */

/** 主界面是否使用双入口（false 时回退到旧的九工作区切换）。 */
export const SLIM_UI_ENABLED = true;

/** 旧面板（批量 / RAW / 预设 / 色彩 / 性能 / AI 人像 / 组件仓库）是否编译进产物。 */
export const LEGACY_PANELS_ENABLED = true;

/**
 * 「文件」菜单里保留的旧能力入口（主理人裁决 Q3：能力不删，但不能出现在主入口）。
 * 批量处理与 RAW 影楼/专业用户还在用，所以留菜单入口；其余旧面板彻底隐藏。
 */
export const LEGACY_MENU_ITEMS: { workspace: "batch" | "raw"; label: string }[] = [
  { workspace: "batch", label: "批量处理…" },
  { workspace: "raw", label: "RAW 显影…" },
];

/**
 * 开发者调试图层（人脸框 / 关键点 / ID 文字 / 网格 / 身体锚点）。
 * 正常预览与导出默认关闭；仅 DEV 或显式 `VITE_DEBUG_OVERLAYS=1` 时可开启。
 * 导出走 Rust 管线，本开关只影响 DOM/SVG 叠加层。
 */
export const DEBUG_OVERLAYS_ENABLED: boolean = (() => {
  try {
    if (import.meta.env.VITE_DEBUG_OVERLAYS === "1") return true;
    if (typeof localStorage !== "undefined" && localStorage.getItem("lumi.debugOverlays") === "1") {
      return true;
    }
  } catch {
    // 忽略隐私模式等存储错误
  }
  return Boolean(import.meta.env.DEV);
})();
