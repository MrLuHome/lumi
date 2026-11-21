# ROUND 8.1 报告：验收整改与可靠性修复

> 基线：`479416f8011000ebe5ea365a01f384633d190cf6`
> 性质：验收整改，不是第九轮；未增加大型新功能，未推倒重写。
> 新提交：见文末「提交」一节（提交后回填）。
> 关联文档：[安全审查](ROUND8_1_SECURITY_REVIEW.md)、[GUI 验收](ROUND8_1_GUI_ACCEPTANCE.md)、
> [NME 方案](ROUND9_NME_PREPARATION.md)、[LibRaw 行动清单](ROUND9_LIBRAW_ACTION_PLAN.md)。

## 1. 修复总览

| # | 缺陷 | 修改文件 | 结果 |
| --- | --- | --- | --- |
| 1 | LEVEL 3 最终 gate 仍依赖整体 `quality.passed`，单区域失败会连带锁定其他区域 | `quality.rs`、`registry.rs` | 公共检查与区域检查拆分；区域只依赖本区域结构；公共失败整体锁定 |
| 2 | `measure_stability` 依赖数组下标，点数不一致时用 zip 静默忽略 | `quality.rs`、`registry.rs` | 一对一几何匹配；Provider/拓扑/模型版本校验；区域点数硬校验 |
| 3 | 把重复一致/检测置信度当作定位精度；「精确五官」文案误导 | `quality.rs`、`AiPortraitPanel.tsx`、`ReshapePanel.tsx`、`editorStore.ts` | 稳定性 / 结构 / 精度三份独立证据；Apple Vision 明确精度未验证 |
| 4 | 导入冒烟在子进程，真实 ONNX 推理仍在主进程 | `providers/onnx.rs`、`model_hub/{inference,probe,mod}.rs`、`main.rs`、`Cargo.toml` | 真实推理移入独立 Worker；输入/元素/输出/时间/CPU/内存限额；无 Worker 不回退 |
| 5 | 切换 Provider 后旧 dense/level3 仍可解锁五官；项目恢复不校验 Provider/拓扑/模型/来源 | `state.rs`、`commands/face_intelligence.rs`、`commands/reshape.rs`、`identity.rs`、前端 store | 切换即清空；恢复时身份不一致直接丢弃并锁定 |
| 6 | 回归与打包验收 | 全仓库 | 已执行（见第 3、4 节；未执行项在 GUI 文档中标注） |

## 2. 关键实现

### 2.1 分项门控（要求一）

- 新增 `FaceChecks { common, regions }` 与 `evaluate_checks()`：
  - 公共检查：置信度、总点数、四锚点齐全时的纵向顺序；
  - 眼部：左右眼各 ≥ 4 点且宽度 ≥ `min_eye_width`；
  - 鼻部：≥ 6 点且宽度 ≥ 0.8×`min_eye_width`；
  - 唇部：外唇 ≥ 8 点、内唇 ≥ 3 点、宽度达标；
  - 下巴：脸缘 ≥ 9 点。
- `level3_gate()` 不再读取 `quality.passed`：
  - 公共检查失败 → 四区一起锁定并附原因；
  - 否则逐区独立判定，一个区域失败不影响其他区域。
- `Level3Gate` 增加 `provider_version`、`model_version` 与 `matches_identity()`。

### 2.2 稳定性（要求二）

- 新增 `StabilityRun { provider_id, topology_id, model_version, faces }`。
- 每次重复分析必须携带身份；`measure_stability(expected_provider, expected_topology, expected_model, runs, ...)`：
  - Provider / 拓扑 / 模型版本任一不一致 → 失败；
  - 人脸数不一致 → 失败；
  - 每张脸按 IoU ≥ 0.30 或中心距 ≤ 0.15 一对一贪心匹配，不依赖下标；
  - 每个语义区域必须存在且点数一致，缺失/多出/点数不同 → 失败；
  - 点必须有限且在 `[-0.01, 1.01]`，bbox 必须有限且宽高 > 0。
- 结论文案明确：只衡量重复一致性，不代表定位精度。

### 2.3 精度证据（要求三）

- UI 新增「精度证据」三行：稳定性 / 结构验证 / 定位精度（未验证：缺人工标注与 NME）。
- Apple Vision 显示：「稳定性与结构检查已通过，定位精度尚未经人工标注验证」。
- `ReshapePanel` 标题改为「五官轮廓精修（LEVEL 3 · 结构门控）」，
  展示 Provider、Provider 版本、拓扑、模型版本，并提示「通过」不等于位置准确。
- 撤销标签由「精确五官」改为「五官轮廓精修」。

### 2.4 用户 ONNX 隔离（要求四）

详见 [ROUND8_1_SECURITY_REVIEW.md](ROUND8_1_SECURITY_REVIEW.md)。摘要：

- 主进程只做 manifest 校验、裁剪/缩放/归一化、临时文件读写；
- Worker 子进程（应用自身 `--face-model-infer`）负责 tract 加载与推理；
- 限额：输入 32..1024、元素 ≤ 3,145,728、输出必须 68 点、10 s 超时、
  `RLIMIT_AS` 1.5 GB、`RLIMIT_CPU` 10 s；
- Worker 崩溃/超时/输出异常只返回错误，主应用不退出；
- 没有 Worker 可执行文件时返回 `Unsupported`，不做主进程回退；
- Provider 仍为实验性，文案注明「推理隔离，模型未审计」。

### 2.5 Provider 切换与项目恢复（要求五）

- `FaceRepository::clear_analyses()`：切换 Provider 时清空所有 `dense`/`level3`。
- `select_face_provider` 返回 `clearedFaces`，消息说明需重新分析。
- 前端 `clearFaceAnalyses` 同步清空本地人脸数据（AiPortraitPanel / ModelHubPanel）。
- `ProjectFace::restorable_analysis()`：
  - 源图哈希变化 → 丢弃；
  - `provider_id` / `provider_version` / `topology_id` / `model_version` 任一不一致 → 丢弃；
  - 旧项目无 `dense`/`level3` → 保持兼容，LEVEL 3 锁定。
- `restore_faces` 返回 `discardedDense`，App 提示「旧结论已锁定，需重新分析」。

## 3. 新增测试与真实输出

### 3.1 分项门控 / 稳定性（`quality.rs`）

新增/更新：

- `single_region_failure_does_not_lock_others`（删除 Nose，只锁鼻部）
- `multiple_region_failures_only_lock_those`（删除 Nose + InnerLips，只锁这两区）
- `common_confidence_failure_locks_all_regions`（公共检查失败，四区一起锁）
- `verified_provider_with_stability_unlocks_all_regions`
- `face_order_change_matches_by_geometry`（人脸顺序翻转仍按几何匹配）
- `region_point_count_change_fails`（区域点数变化必须失败）
- `missing_region_fails`（缺失区域必须失败）
- `topology_mismatch_fails`（不同拓扑必须失败）
- `model_version_mismatch_fails`（模型版本不一致必须失败）
- `non_finite_point_fails`（NaN 点必须失败）

真实输出：

```text
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 302 filtered out
```

（`cargo test --lib face_intelligence`）

### 3.2 用户 ONNX Worker（`model_hub/probe.rs`）

新增：

- `infer_rejects_oversized_input_before_reading`（2048×2048 直接拒绝）
- `infer_rejects_wrong_point_contract`（期望 5 点必须拒绝）
- `infer_rejects_missing_input_file`（输入缺失返回可读错误）

真实输出：

```text
running 5 tests
test ...infer_rejects_oversized_input_before_reading ... ok
test ...infer_rejects_wrong_point_contract ... ok
test ...infer_rejects_missing_input_file ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 371 filtered out
```

### 3.3 项目恢复（`identity.rs`）

新增：

- `matching_identity_restores_dense_and_gate`
- `provider_switch_discards_old_dense`
- `provider_version_mismatch_discards_old_dense`
- `model_version_mismatch_discards_old_dense`
- `topology_mismatch_discards_old_dense`
- `replaced_source_discards_old_dense`
- `old_project_without_dense_stays_compatible`

真实输出：

```text
test result: ok. 77 passed; 0 failed; 0 ignored; 0 measured; 296 filtered out
```

（`cargo test --lib face_reshape`）

### 3.4 前端

新增 `clearFaceAnalyses` 测试：

```text
✓ src/state/editorStore.test.ts (24 tests)
Test Files  5 passed (5)
Tests       58 passed (58)
```

## 4. 全量回归（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test` | **通过**：lib 373 passed / 3 ignored；集成测试 12+4+10+4+5+2+3+3 等全部通过 |
| `cargo clippy --all-targets` | **通过**：无 error / warning |
| `npm run typecheck` | **通过**：无输出 |
| `npm test -- --run` | **通过**：58 tests |
| `npm run build` | **通过**：`dist/assets/index-*.js 446.63 kB` |
| `npm run app:build` | 见 [GUI 验收](ROUND8_1_GUI_ACCEPTANCE.md) |

未执行项一律在 GUI 文档标「未验证」，不写 PASS。

## 5. GUI 验收

见 [ROUND8_1_GUI_ACCEPTANCE.md](ROUND8_1_GUI_ACCEPTANCE.md)。截图目录：
`docs/assets/round8_1/`。

## 6. 未解决问题

1. **定位精度未验证**：没有人工标注集与 NME，UI 已明确标注。
2. **ONNX 隔离是进程级，不是系统级沙盒**：见安全审查第 3 节。
3. **LibRaw 未合规**：缺随包源码、补丁、重链接材料与法律审核。
4. **Apple Vision 无逐点置信度**：当前逐点 confidence 是检测级 confidence 的传播，
   UI 已注明「检测级 confidence 不等于逐点精度」。
5. **GUI 未覆盖项**：以 GUI 验收文档为准，未执行项不得视为通过。

## 7. 提交

- 基线：`479416f8011000ebe5ea365a01f384633d190cf6`
- ROUND 8.1 代码与文档提交：`a9970dc06ea71bc8d3962277733e19cda0808043`
  （`fix(round8.1): 分项门控、稳定性匹配、ONNX Worker 隔离与恢复校验`）
- 修改文件与缺陷对应关系见第 1 节。
