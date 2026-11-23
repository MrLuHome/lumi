#!/bin/zsh
# S1.2 一键精修端到端诊断驱动（三阶段）。
#
# 用法：zsh scripts/s1_2_one_click_diagnostics.sh
# 三态：照片缺失 → Rust panic「缺少照片导致未执行」；阶段失败 → 非零退出；
#       默认 cargo test 显示 ignored（未执行），绝不静默假绿。
set -euo pipefail
cd "$(dirname "$0")/.."

export S1_2_DIR="docs/assets/s1_2"
mkdir -p "$S1_2_DIR"

echo "═══ 阶段 1/3：Rust 分析 dump（检测 + 逐脸美肤 + 整图分析）═══"
S1_2_PHASE=dump cargo test --test s1_2_diagnostics phase_dump -- --ignored --nocapture

echo "═══ 阶段 2/3：真实 buildOneClickPlan 三档参数（vitest 桥）═══"
S1_2_DIR="$S1_2_DIR" pnpm exec vitest run src/state/s1_2_plan_bridge.test.ts

echo "═══ 阶段 3/3：真实管线渲染 + 像素统计 + 断言 + 对比图 ═══"
S1_2_PHASE=render cargo test --test s1_2_diagnostics phase_render -- --ignored --nocapture

echo "═══ 诊断完成：$S1_2_DIR/{stem}.diag.json + {stem}/ 对比图 ═══"
