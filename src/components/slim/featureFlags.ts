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
