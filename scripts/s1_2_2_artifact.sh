#!/bin/zsh
# S1.2.2 伪影逐阶段定位驱动（dump → 真实计划桥 → 定位输出）。
# 三态：缺问题原图 → Rust panic；阶段失败 → 非零退出；默认 ignored=未执行。
set -euo pipefail
cd "$(dirname "$0")/.."

export S1_2_2_DIR="docs/assets/s1_2_2"
mkdir -p "$S1_2_2_DIR"

echo "═══ 阶段 1/4：问题照片分析 dump ═══"
S1_2_2_PHASE=dump cargo test --manifest-path src-tauri/Cargo.toml --release --test s1_2_2_artifact_localize phase_dump -- --ignored --nocapture

echo "═══ 阶段 2/4：真实 buildOneClickPlan（自然档） ═══"
S1_2_DIR="$S1_2_2_DIR" pnpm exec vitest run src/state/s1_2_plan_bridge.test.ts

echo "═══ 阶段 3/4：15 阶段输出 + 消融 A–H + 伪影检测 ═══"
S1_2_2_PHASE=localize cargo test --manifest-path src-tauri/Cargo.toml --release --test s1_2_2_artifact_localize phase_localize -- --ignored --nocapture

echo "═══ 阶段 4/4：§4.2 网格实验（点 vs 三线性 Splat × σ 矩阵） ═══"
S1_2_2_PHASE=gridexp cargo test --manifest-path src-tauri/Cargo.toml --release --test s1_2_2_artifact_localize phase_grid_experiment -- --ignored --nocapture

echo "═══ 完成：$S1_2_2_DIR/user_wechat/ + grid_experiment.json ═══"
