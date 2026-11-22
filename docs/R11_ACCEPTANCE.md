# R11 · 验收报告（ACCEPTANCE）

## ⚠️ 验收对象（先读这一行）

```
$ git rev-parse HEAD
945bd46863d11af2bcff2e5398774456b5e124a4      # = 945bd46（docs 回填）
$ git log --oneline -2
945bd46 docs(r11): 回填本轮代码提交 SHA
ff91cee feat(r11): 统一模型注册表与推理准入链
$ git status --short          # 验收开始时（21:38）：干净，仅 3 个未跟踪
?? .workbuddy/
?? docs/REVIEW_PERF_SECURITY.md
?? docs/UX_EXPERIENCE_AUDIT_2026-11-22.md
```

**本报告分两段**：§1–§10 的结论建立在对 `ff91cee` 的复跑之上（验收开始时工作区确实干净）；
§11 是在最终提交 `96c716d` 上的**签收复核**。两段的对应关系见下表。
验收进行到 21:40 之后工作区曾出现工程师的未提交改动（`TensorSpec.shape` → `Cow<'static,[usize]>`、
新增 `InferenceBackend::inspect()` 等），**那批改动已作为 `96c716d` 单独提交并复核**，不是"未覆盖"状态。

| 项 | 值 |
|----|----|
| **最终验收对象** | **`16f64a125b30fcfbbf079c75b169a89a58181e65`**（`fix(r11): 收紧空转测试与只读探测契约`，10 files / +523 / −54） |
| 签收后 HEAD | `6c4bb48`（`docs(r11): 回填第 4 次提交 SHA`，仅 docs，无代码） |
| 前一个代码提交 | `96c716d4a3e35bf66a1ed62828a37fc9415f38d0`（`feat(r11): 补 InferenceBackend::inspect 与 TensorSpec 动态形状`，14 files / +622 / −148） |
| 首轮基准提交 | `ff91cee6114d162c532da87082926715737faa2f`（HEAD 曾为 `945bd46`） |
| 期间 docs 提交 | `09cfb91`、`4c41b95`（仅回填/修正 SHA，无代码） |
| 隔离复跑环境 | `git worktree` @ `/tmp/r11v`（先 detached `ff91cee` → 签收切 `96c716d` → 终版切 `16f64a1`），与主工作区隔离 |
| 工作区状态 | 终版签收时**干净**（`git status --short` 只剩 4 个未跟踪：`.workbuddy/`、本报告、`REVIEW_PERF_SECURITY.md`、`UX_EXPERIENCE_AUDIT_*.md`） |
| 结论覆盖范围 | §1–§10 来自 `ff91cee`；§11 是 `96c716d` 的签收复核；§12 是终版 `16f64a1` 的签收复核 |

> **哪些结论在哪一个 SHA 上得出**：
> - `ff91cee` 得出（未在 96c716d 重验，因为相关文件未改）：§3 `Raw255` 判定、§4 许可先于读取对照实验、
>   §5 假绿清单、§6 全部缺陷条目、§7 离线与依赖红线、§8 性能数字。
> - `96c716d` 上**重新跑过**：§11 的四件套（lib / model_registry / clippy / no-default-features）、
>   对外契约对比（`RegistrySnapshot` 字段、`src/types.ts`、`ModelSpec` 去 `Copy` 的调用点）。
> - 一句话区分：**`ff91cee` 定「R11 做了什么、做得对不对」；`96c716d` 定「最后一版能不能签收」。**

---

> 验收人：严过关（QA Engineer） · 团队 `software-lumi-r11` · 任务 #3
> 上游：`docs/R11_AUDIT_AND_DESIGN.md`（设计）、`docs/R11_ROUND_REPORT.md`（工程师自述）
> 验收日期：2026-11-22
> 原则：**工程师自述一律视为待证。** 本文每个「通过」都有真实命令输出或可复现实验支撑；
> 无法复现的一律降级为「部分通过」并写明缺口。

---

## 0. 结论（先读这一段）

**终版签收：`16f64a1` —— 代码与测试质量债均已达标，R11 可以正式关账、进入 R12。**
（本段结论在 `16f64a1` 上复核，详见 §12；§11 是上一版 `96c716d` 的中间签收记录，其「未交付项」已被 §12 覆盖。）

首轮（`ff91cee`）未发现阻断级**代码**缺陷，也未发现「假绿」性质的系统性作弊，
但发现 **4 个名不副实的弱测试** 和 **1 个未被任何自动化测试覆盖的验收点**（T04③）——
这 5 项在终版中**全部补齐并逐条复核过**（§12.3）。

流程侧另记一事（已收口）：验收期间（21:40 起）工作区被工程师并发修改且一度编译不过（`E0382`），
相关改动先后作为 `96c716d`、`16f64a1` 两次提交落地，本报告均已复核。

| 维度 | 结果 |
|------|------|
| 基线四件套复跑（`ff91cee`） | **全部与自报一致**（466 / 563 / 0 warning / 466） |
| 中间签收（`96c716d`） | 471 / 18 / 0 warning / 471 —— 代码通过，测试债未偿（§11） |
| **终版签收（`16f64a1`）** | **477 / 18 / 0 warning**（§12.1） |
| 前端复跑 | **一致**（vitest 81 passed、tsc 0 error） |
| 10 条验收条件 | **9 PASS / 1 部分通过 / 0 FAIL**（A8 的命令层仍无端到端测试，但已补 4 条契约测试，见 §12.3） |
| 对外契约（`96c716d`） | **零变化**（`RegistrySnapshot` 字段、`types.ts` 仅加注释、`ModelSpec` 去 `Copy` 无调用点损坏） |
| 弱测试 / 假绿 | **0 条**（§5 初查的 4 条已在 `16f64a1` 全部改造成「真能失败」，§12.3 逐条核） |
| 缺陷 | 阻断 0 / 严重 1（S2：R13 前置地雷 `skin_segmentation::status()` 仍会算 SHA）/ 一般 4（N1/N3/N6 已关闭，N2/N4/N5 保留） |
| 离线与依赖红线 | **通过**（零网络调用、`Cargo.toml`/`Cargo.lock` 零改动） |
| 主线程阻塞 R1 | **本轮合规**（`snapshot` 实测 0.0097 ms，浅检查 0.0711 ms），但**未根治**（＝ S2） |

---

## 1. 复跑数字 vs 自报数字（逐项对照）

复跑命令均在**干净 worktree**（`/tmp/r11v`，detached `ff91cee`）或工作区未变脏前执行。

| # | 命令 | 工程师自报 | QA 复跑实测 | 一致 |
|---|------|-----------|-------------|------|
| 1 | `cargo test --lib` | 466 passed / 0 failed / 3 ignored | **466 / 0 / 3**（5.74s） | ✅ |
| 2 | `cargo test`（全量） | 563 passed / 0 failed / 7 ignored | **563 / 0 / 7** | ✅ |
| 3 | `cargo clippy --all-targets` | 0 warning | **0 warning**（干净 target 强制全量重编 1m05s，输出无任何 `warning:`） | ✅ |
| 4 | `cargo test --no-default-features --lib` | 466 / 0 / 3 | **466 / 0 / 3**（5.89s） | ✅ |
| 5 | `tests/beauty_real.rs` 未跳过 | 4 例 / 27.88s | **4 passed / 0 ignored / 33.75s** | ✅（耗时为本机差异） |
| 6 | `pnpm test` | 81 passed | **81 passed**（8 files, 303ms） | ✅ |
| 7 | `tsc --noEmit` | 0 error | **0 error**（exit 0，无输出） | ✅ |

**全量 563 的分 target 明细**（QA 实测，用于证明「不是只有数字对、target 少跑了」）：

```
lib                      466 passed  3 ignored
doc-tests                  0
beauty                    12 passed  1 ignored
beauty_real                4 passed  0 ignored  (33.75s)   ← 真实样张，未跳过
engine                    10 passed  1 ignored
face_detection             4 passed  0 ignored             ← 见 §8
face_intelligence          5 passed  0 ignored             ← 见 §8
model_registry            17 passed  0 ignored             ← R11 新增
onnx_worker                7 passed  0 ignored
raw_real                   2 passed
reshape_persistence        3 passed
reshape_pipeline           3 passed  1 ignored
round10_changeset         11 passed
round10_performance        5 passed
round10_real_photos        5 passed
round8_benchmark           0 passed  1 ignored
round9_1_persistence       9 passed
合计                     563 passed  7 ignored  / 0 failed
```

7 个 ignored 全部是**既有**手工基准（`batch_memory_bench`、`round6_performance_baseline`、
`gpu_benchmark_report`、`benchmark_beauty`、`benchmark_full_pipeline`、
`reshape_perf_baseline_12mp`、`round8_quality_benchmark`），**R11 新增代码里 0 个 `#[ignore]`**
（按文件统计 `#[test]`/`#[ignore]` 已核对）。

**结论：自报数字全部属实，无一处夸大。**

---

## 2. 逐条验收表

> ⚠️ 说明：R11 任务书原文不在我的输入里。下表 10 条按**设计文档**
> `docs/R11_AUDIT_AND_DESIGN.md:1327/1336/1345/1354/1363`
> 的 T01–T05「验收点」归纳而来（这是可逐字对照的唯一权威清单），并合并了主理人交办的 10 项独立验证。

| # | 验收条件（出处） | 实测证据 | 结论 |
|---|------------------|----------|------|
| A1 | 登记表定义完整、`YUNET` 字段与实测一致、零新增依赖（T01①④） | 独立核验：`shasum -a 256` = `8f2383e4…52fa4` = `YUNET.sha256`；`ls -l` = 232,589 B = `YUNET.size_bytes`；`LICENSE` 首行 `MIT License` + `Copyright (c) 2020 Shiqi Yu` 与 `evidence[1].quote` 逐字一致；`git diff 572f8f0 ff91cee -- src-tauri/Cargo.toml src-tauri/Cargo.lock` **输出为空** | **PASS** |
| A2 | 许可四字段独立；`Unverified` 不默认 Allowed；`unverifiedFields` 单列；不据仓库 LICENSE 推断（T01②） | `license.rs:104-139` 四字段独立 + `unverified_fields()`；`gate()`（:181-211）对 `Builtin` 要求 weights/commercialUse/redistribution 三字段均非 Unverified、非 NotAllowed；测试 `unverified_commercial_field_blocks_bundled_weights` 断言 `which == "commercialUse"`；`registry_snapshot_is_frontend_ready` 断言 `unverifiedFields == ["trainingData"]`（未被合并进 commercialUse） | **PASS** |
| A3 | `weights == NotAllowed` → 拒绝**且不读文件**（T01③） | **QA 做了对照实验（§4）**：同一文件、同一权限（`chmod 000`）、只改许可 → 许可 `Allowed` 时错误是 `corrupt: 无法读取文件头：Permission denied`；许可 `NotAllowed` 时 findings **只有** `licenseBlocked`。证明读取确实没发生 | **PASS（强证明）** |
| A4 | 判定链顺序逐步可验；`findings` 一次给多个问题；`locate()` 是纯函数（T02①②③） | `verify.rs:31-44`（许可先于一切 IO，早返回）→ `:47`（存在）→ `:56`（体积/头）→ `:62`（契约）→ `:78`（SHA）；测试 `license_failure_blocks_before_missing` 逐字断言 `kinds == ["licenseBlocked", "missing"]`；`contract_is_checked_before_sha256` 断言形状错 + SHA 错时结论是 `versionMismatch`；`locate_is_a_pure_function_without_tauri` 用空 `&[]` 断言 | **PASS** |
| A5 | `snapshot()` 不调 `sha256_file`；serde 输出 camelCase 且字段齐全（T02④⑤） | 代码：`mod.rs:189-200` 只用 `is_file()` + `metadata()`，`sha256` 取 `spec.sha256`；**QA 用计时独立证明**：`Registry::snapshot` 实测 **0.0097 ms**，而单次 `sha256_file` 实测 **13.5 ms** —— 差 3 个数量级，物理上不可能是现算。字段：`registry_snapshot_is_frontend_ready` 逐字段断言 20 个 model 字段 + 9 个 license 字段 + 6 个 summary 字段 | **PASS** |
| A6 | `code()` 11 分支全覆盖；`TensorBatch` 拒绝数量/NaN/Inf；真实 YuNet `load` 成功（T03①②③） | `error.rs:222-227` `assert_eq!(codes, ERROR_CODES.to_vec())` + `codes.len() == 11`（**这是真覆盖**：把 11 个变体映射结果与常量逐一比对）；`tensor.rs` 4 条测试断言到 `code()` 级别（`E_MODEL_CONTRACT` / `E_INPUT`）；`cpu_tract.rs::load_produces_a_runnable_plan_and_reports_capabilities` 加载真实权重并断言 `output_count() == 12` | **PASS** |
| A7 | `face_detection` / `beauty_real` / `face_intelligence` 全绿；`detect()` 旧签名行为不变；GpuBackend stub（T03④⑤⑥） | 三个目标真实执行（见 §1 明细 + §8 具体 test 名）；`gpu_backend_is_unavailable_with_a_reason` + `gpu_backend_rejects_load_with_backend_code`（`E_BACKEND`）；**但**「旧签名等价」测试是**空对空**（§5-W1） | **部分通过** |
| A8 | 4 个新命令 `spawn_blocking`（例外 `model_unload`）；`snapshot` < 5ms；`modelLoad("不存在")` 返回结构化 `E_MODEL_MISSING`；`isModelError` 可用；三面板零改动（T04①–⑤） | `model_status.rs:123/146/175` 三处 `spawn_blocking`，`model_unload`（:229）同步且注释说明；snapshot 实测 0.0097 ms；`FacePanel/BeautyPanel/ModelHubPanel` 零改动（`git show --stat ff91cee` 无 `src/components/**`）；**但** `model_load` 命令层**零测试**，T04③ 只在 `FaceEngine::load_checked` 层验证（§6-N1）；`git diff` 确认 `AppError` 的 `Serialize` 未改 | **部分通过** |
| A9 | 皮肤 4 条既有单测 + `segmentation` 那条仍绿；回退文案逐字保留（T05①②） | 定点复跑 **20 passed / 0 failed**：`skin_segmentation::model::tests::{rejects_non_commercial_weights, rejects_research_only_license_even_if_flagged_commercial, rejects_sha_mismatch_and_accepts_matching_hash, status_falls_back_to_geometry_without_weights}` + `segmentation::model::tests::no_model_is_marked_verified_without_weights`；文案用 `git show 572f8f0:…` 与 `git show ff91cee:…` 逐字 `grep reason:` 比对，**两版字符串完全相同** | **PASS** |
| A10 | 全量 0 failed；clippy 0 warning；双特性通过；`tauri.conf.json` 未改但台账写明；离线红线（T05③④⑤⑥） | 见 §1（1–4 行）+ §7 | **PASS** |

**计分：PASS 8 / 部分通过 2 / FAIL 0。**

---

## 3. `ValueRange::Raw255` 偏离的真伪 —— **QA 支持工程师，反对设计文档**

主理人要求独立判断，以下结论不引用工程师的论证，独立推导。

**第一步：内部缓冲区到底是什么量纲？**
`src-tauri/src/image_engine/buffer.rs:10`
```rust
pub type Image16 = ImageBuffer<Rgba<u16>, Vec<u16>>;
```
像素是 **`u16`**，值域 `0..=65535`（`CHANNEL_MAX = 65535.0`，同文件 :12）。

**第二步：预处理把 u16 变成什么？**
`src-tauri/src/ai/face.rs:32` `const PIXEL_SCALE: f32 = 257.0;`，:277-279
```rust
input[index]         = value[2] as f32 / PIXEL_SCALE;  // B
input[plane + index] = value[1] as f32 / PIXEL_SCALE;  // G
input[2*plane+index] = value[0] as f32 / PIXEL_SCALE;  // R
```
`65535 / 257 = 255.0` —— 该除法在 f32 下**精确**成立
（`u16::MAX`、`257.0`、`255.0` 三者都可精确表示，仓库内 `assert_eq!(f32::from(u16::MAX) / 257.0, 255.0)` 用的是精确等号，且通过）。

**因此：送进 ONNX 的张量是 `0..255`，不是 `0..1`。**
`/257.0` 只有在像素是 `u8` 时才是「归一化到 0..1」；这里是 `u16`，所以它是**「16 位压回 8 位刻度」**，
与 `ai/model.rs::MODEL_INPUT` 里那句 `BGR 0..255` 完全一致。

**第三步：如果按设计登记成 `ZeroOne`，运行时会发生什么？**（三档后果，按严重度排序）

1. **立刻硬失败，不是精度下降**：`TensorSpec::validate`（`tensor.rs:169-182`）会用
   `ValueRange::ZeroOne.bounds() == (0.0, 1.0)` 校验每个元素，容差 `1e-3`。
   640×640×3 的输入里几乎每个元素都 > 1.001 → 第一次 `TensorBatch::new_for` 就返回
   `E_INPUT: 输入第 N 个元素 255 超出值域 0..1`。**人脸检测 100% 不可用**，而且错误信息会
   误导人以为「图像有问题」。
2. **若有人为了「让校验通过」而改预处理**（这是最可能的后续发展）：把 `/257.0` 改成
   `/65535.0` 或用 `ValueRange::apply(unit)` 的通用预处理 → 送进 YuNet 的张量变成 `0..1`，
   YuNet 的分数塌到 `< 0.01`，`SCORE_THRESHOLD = 0.6` 全部过滤掉 →
   **表现为「AI 突然一张脸都检测不到」，且不报任何错误**。这正是设计文档 R9 想避免的那个
   255 倍误差 —— 而按设计登记 `ZeroOne`，恰恰是**主动制造**它。
3. `ValueRange::apply()`（`tensor.rs:103-110`）的语义也证明这一点：`Raw255 => unit * 255.0`。
   若登记 `Raw255`，未来通用预处理按契约把 0..1 映射成 0..255，与现有 `/257.0` 结果**等价**；
   若登记 `ZeroOne`，通用预处理会原样透传 0..1，与现有实现冲突。

**结论：设计文档 R9 的前提（「实现是 /257.0，实际 0..1」）是错的，工程师的 `Raw255` 是对的。**
这不是「偏离设计」，而是「修正设计中的事实错误」，且已用双重护栏锁死：
`ai/model.rs::model_input_matches_the_structured_spec`（字面常量 ↔ 结构化契约）、
`tensor.rs::raw255_is_the_yunet_scale_for_u16_pixels`、
`tests/model_registry.rs::yunet_value_range_is_raw255_consistent_with_the_implementation`。
建议 R12 起**把这个结论回写进设计文档**，避免下一轮又被「纠正」回去。

---

## 4. 「许可先于读取」—— 把弱证明升级为强证明

主理人质疑得对：`non_commercial_weights_are_rejected_before_any_read`（`tests/model_registry.rs:170`）
与 `license_failure_blocks_before_missing`（:239）**本身只断言了 findings 的种类与顺序**，
属于「代理证据」—— 它成立的前提是「`findings` 是全量的」，而这一点靠的是代码自觉。

QA 在隔离 worktree 里补了一个**只改一个变量的对照实验**（临时文件，不进仓库）：

| 组 | 文件 | 权限 | 许可 | 期望 | 实测 |
|----|------|------|------|------|------|
| 对照 A | 真实 YuNet 拷贝，体积/内容合法 | `chmod 000` | `Allowed` | 必须报「读不了」 | `corrupt`，`detail = "无法读取文件头：Permission denied (os error 13)"` ✅ |
| 实验 A | **同一个文件**（同一 inode、同一权限） | `chmod 000` | `NotAllowed` | 只能报许可 | `findings = ["licenseBlocked"]`，`resolved_path = None` ✅ |
| 对照 B | 路径位置放一个**目录** | — | `Allowed` | 报 missing（`is_file()` 为 false） | `["missing"]` ✅ |
| 实验 B | 同上目录 | — | `NotAllowed` | 只许 license + missing | `["licenseBlocked", "missing"]` ✅ |

**为什么这是强证明**：对照组 A 用同一份文件证明了「这个路径此刻确实读不了」
（`Permission denied`）。实验组 A 与它唯一的差别是许可结论，而结果里**没有**任何 `corrupt` 项。
若实现真的尝试读过文件，必然产生 `Permission denied → corrupt`。因此「未授权权重没有被读过」
在本轮实现上是**成立的事实**，不只是断言。

同时静态侧的 `verify.rs:31-44` 也支持这个结论：许可分支里的 `return health` 出现在
`check_size_and_header`（唯一会 `File::open` 的地方）之前，且中间没有任何 `match`/闭包旁路。

复现命令（写在 `/tmp/r11v/src-tauri/tests/qa_probe.rs`，仓库零改动）：
```bash
git worktree add /tmp/r11v ff91cee
cd /tmp/r11v/src-tauri && CARGO_TARGET_DIR=/tmp/r11v-target \
  cargo test --test qa_probe -- --nocapture --test-threads=1
```

---

## 5. 弱测试 / 假绿清单（重点）

按「名字承诺 vs 实际断言」是否匹配分类。**没有**发现 `#[ignore]` 掩盖、也没发现把关键路径 mock 掉的情况
（本轮新代码里没有任何 mock 框架，全部用真实 232 KB 权重与真实临时文件）。

### 有效测试（抽查 16 条，证据充分）

`tampered_weight_is_detected_as_corrupt_with_both_digests`（保持体积、只翻最后 1 字节 →
**只有 SHA 能发现**，并断言 expected/actual 两个摘要）、`license_failure_blocks_before_missing`、
`contract_is_checked_before_sha256`、`unverified_commercial_field_blocks_bundled_weights`、
`wrong_input_shape_is_a_version_mismatch`、`registry_snapshot_is_frontend_ready`、
`snapshot_state_reflects_recorded_checks`、`state.rs::state_maps_every_kind`（5 种失败态全覆盖）、
`error.rs::every_variant_has_a_stable_code`（11 码逐一比对常量）、
`tensor.rs::range_validation_matches_declared_value_range`、
`verify.rs::sha_mismatch_is_corrupt_not_missing` / `wrong_size_is_corrupt_before_hashing` /
`zip_disguised_as_onnx_is_rejected_by_header`、`builtin.rs::yunet_artifact_on_disk_matches_the_registration`、
`locate.rs::candidates_preserve_order_and_covers_both_resource_layouts`、
`snapshot.rs::snapshot_serializes_with_camel_case_and_all_fields`。

### 弱测试 / 名不副实（4 条）

| 编号 | 位置 | 名字承诺 | 实际断言 | 为什么弱 / 影响 |
|------|------|----------|----------|-----------------|
| **W1** | `src/ai/face.rs:398` `detect_and_detect_with_cancel_are_equivalent` | 「新旧签名等价」（设计 T03⑤ 要求「输出 struct 相等」） | 只比 `faces.len()`、`model`、`image_width`、`image_height` | 输入是**纯灰 64×64**，QA 实测 **检测结果为 0 张脸** → 该测试实际是 `0 == 0`，**空对空**。若未来关键点换算出错、后处理坐标缩放错，只要有 0 张脸它照样绿。`elapsed_ms` 不比较是合理的，但 `faces` 的内容完全没有比较 |
| **W2** | `src-tauri/tests/model_registry.rs:405` `model_runtime_state_strings_match_the_frontend_union` | 「与前端联合类型一致」 | 构造了含 **7** 个期望字符串的数组，但真正调用 `as_str()` 的只有**前 2 个**（`Verified` / `Ready`），最后只断言 `expected.len() == 7` | 另外 5 个状态（`licenseBlocked`/`missing`/`corrupt`/`versionMismatch`/`loadFailed`）**从未被比较**。且期望值是测试里手抄的副本，**改了 `src/types.ts` 也不会红** —— 前后端状态字符串漂移无人防守 |
| **W3** | `src-tauri/src/ai_engine/model_registry/spec.rs:156` `model_id_follows_the_naming_contract` | 「模型 ID 遵守命名契约」 | 校验的是函数内硬编码的**字符串字面量** `"face-detect-yunet-2023mar"`，不是 `builtin::ALL` 里的任何一个条目 | 这条测试**永远不会失败**（除非改测试自己）。真正该做的是遍历 `builtin::ALL`，断言每个 `id` 合规且**不重复** |
| **W4** | `src-tauri/src/ai_engine/model_registry/mod.rs:324` `snapshot_reads_real_model_without_hashing` | 「不计算哈希」 | 断言 `entry.sha256 == YUNET.sha256` | 现算和不现算**值都一样**，该断言无法区分。真正能区分的是计时（QA 在 §2/A5 用 0.0097 ms vs 13.5 ms 三个数量级差异补上了，但仓库里没有这条护栏） |

补充（不算假绿，但建议加强）：
- `findings_expose_every_problem_not_just_the_first`（`tests/model_registry.rs:257`）用
  `findings.len() >= 2`，没有断言具体 kinds；应改为逐字断言 `["licenseBlocked", "missing"]`
  （同文件 :252 那条已经这么做了）。
- `cancelled_inference_returns_e_cancelled_without_panicking`（:273）第二半只用 `is_err()`
  （`detect_with_cancel` 降级路径），属补充断言，可接受。
- `uncancelled_inference_still_works_after_the_cancel_test`（:288）同样用纯灰 96×96（0 张脸），
  只断言 `result.model == "YuNet 2023mar"`。它能证明「不 panic、能返回」，**不能**证明「推理结果正确」。
  真正守着推理正确性的是 `tests/face_detection.rs`（真实样张，本轮全绿）。

---

## 6. 缺陷清单

> 分级：**阻断** = 必须修完才能验收；**严重** = 必须在下一轮开始前修；**一般** = 排期修。
> 按边界要求，QA 只报告、**未改动任何 `src-tauri/**` 或 `src/**`**。

### 阻断级：0 条（代码层面）

流程问题（不影响 `ff91cee` 本身，但影响「谁能被签收」）：

> **P1（阻断签收，非代码缺陷）** 验收期间工作区被并发修改且**当前编译不过**。
> 时间线：QA 基线复跑 21:39:34 完成（此时 `git status` 干净，仅 3 个 `??`）；
> 21:40:09 起 10 个文件被改动（`ai/model.rs`、`inference/{backend,cpu_tract,gpu_stub,mod,tensor}.rs`、
> `model_registry/{builtin,snapshot,spec}.rs`、`docs/R11_AUDIT_AND_DESIGN.md`）；
> 21:41:45 QA 跑定点回归时报
> `error[E0382]: borrow of moved value: wrong_spec @ src/ai_engine/inference/cpu_tract.rs:502-503`。
> 根因是该重构把 `TensorSpec.shape` 改成 `Cow<'static,[usize]>`、`ModelSpec` 去掉 `Copy`，
> 于是 `TensorBatch { spec: wrong_spec, data: vec![0.0; wrong_spec.element_count()] }` 变成了「移后借用」。
> 截至 21:48，改动范围已从 10 个文件扩到 **12 个**，并**包含 `src-tauri/tests/model_registry.rs`**
> —— 也就是本报告 §1/§5 里那 17 条验收测试所对应的文件本身正在被改。
> 这意味着：**当前工作区的 17 条测试已不等于 `ff91cee` 的 17 条测试**，
> 若不指定基准就讨论「测试是否有效」会失去意义。
> **处置**：本报告一律以 `ff91cee` 为基准；请工程师把 WIP 收尾并提交（或还原到可编译）
> 之后再开一次「签收确认」。**不要**用当前工作区去向主理人报「全绿」。
> **后续（21:52 复核）**：该 `E0382` 已被修掉（`let element_count = wrong_spec.element_count();`
> 提前求值），`cargo check --lib --tests` **通过**（5.91s，0 warning）。因此 P1 现在只剩
> 「**交付物未冻结**」一个主张：13 个文件未提交，`git log` 仍停在 `945bd46`。
> 本报告其余所有结论对 `ff91cee` 成立，且不受该 WIP 影响。

### 严重级：2 条

> **S1 · W1 空对空等价性测试** —— `src/ai/face.rs:398`（`blank_preview` 在 :354）
> - 复现：`cargo test --lib ai::face::tests::detect_and_detect_with_cancel_are_equivalent`（绿）；
>   QA 另测同一输入：`[blank] faces=0 model=YuNet 2023mar infer=1005ms`
> - 期望：设计 T03⑤ 要求「单测断言两者输出 struct 相等」，即 `faces` 的点位/分数、`image_width/height` 全等
> - 实际：输入必然产出 0 脸 → 只比了「0 == 0」与几个元字段
> - 影响：本轮确实没坏（`tests/face_detection.rs` 用真实样张兜住了「检测得到脸」），
>   但**这条测试不保护任何东西**。建议改用 `tests/` 里的真实 `portrait.jpg`，比较 `faces` 全字段（除 `elapsed_ms`）。
>
> **S2 · R13 前置地雷：皮肤 `status()` 一旦有登记就会整文件哈希** —— `src/ai_engine/skin_segmentation/model.rs:63`（`status()` 内 `verify::check(spec, &base_dirs, false)`）
> - 事实：`verify::check(..., include_contract=false)` 仍会走 ⑤ **SHA256**（`verify.rs:78`）。
>   当前 `builtin::SKIN_MODELS` 为空 → 循环体不执行 → 无 IO，所以本轮**合规**。
> - 期望：面板挂载时的状态查询永远是 O(1) 浅检查（`check_shallow`，实测 0.0711 ms）
> - 实际：一旦 R13 登记一条皮肤权重，`skin_model_status`（面板挂载即调用）会做 232 KB～数十 MB 的哈希 → **R1 原样复活**
> - 影响：不是本轮缺陷（工程师已在 `R11_ROUND_REPORT.md` §10.2-R1 主动写明「风险只是被绕开，没有根治」），
>   但必须在 R13 动 `SKIN_MODELS` **之前**改成 `check_shallow` 或落盘缓存。**记为 R13 的强制前置条件。**

### 一般级：6 条

> **N1 · `model_load` / `model_unload` / `model_registry_check` 三个新命令零测试**
> 位置：`src-tauri/src/commands/model_status.rs`（250 行，`#[test]` 数量 = **0**）
> 期望：设计 T04③ 明确要求「`modelLoad("不存在")` 返回 `{code:"E_MODEL_MISSING"}` 而非字符串」
> 实际：没有任何测试调用过这三个命令；该行为只在 `FaceEngine::load_checked` 层被
> `engine_missing_model_surfaces_a_structured_error` 间接覆盖（`tests/model_registry.rs:125`）。
> 影响：`failure_of()`（:76-109）这种「把 InferenceError 还原成 ModelFailure 写回 registry」的映射
> 出错不会被发现（比如 `ModelCorrupt` 分支把 `actual_sha` 硬编码成 `None`，见 :93）。
> 建议：把 `failure_of` / `status_from_state` 提为纯函数并单测（不需要 Tauri 上下文）。

> **N2 · `header_is_onnx` 的 Git-LFS 判据是死代码** —— `src-tauri/src/ai_engine/model_registry/verify.rs:238`
> 代码：`let mut header = [0u8; 8]; … if header.starts_with(b"version https://git-lfs")`
> 期望：LFS 指针文件被这条判据识别
> 实际：`header` 最多 8 字节，而 needle 有 23 字节 → `starts_with` **恒为 false**（切片比 needle 短必然 false）
> 影响：低（LFS 指针 ~130 B，会被前面的 `size_bytes` 检查先拦住并报 `corrupt`）。
> **同一模式在 `face_intelligence/model_hub/security.rs:133` 已存在（R11 之前的代码，属沿袭而非新增）**，
> 建议两处一起修（读 64 字节即可）。

> **N3 · `verify.rs` 文档引用了不存在的测试名** —— `verify.rs:9`
> 代码：`` `tests/model_registry.rs::license_block_never_reads_the_file` 断言 `` 
> 实际：`tests/model_registry.rs` 里没有这个函数，真实名字是 `non_commercial_weights_are_rejected_before_any_read`
> 影响：会让人误以为有更强的证明（正好是主理人这次质疑的那一点）。建议连同 §4 的更强实验一起写进注释。

> **N4 · `isModelError` 的类型谓词与运行时对象不符** —— `src/lib/backend.ts:729`
> 代码：`export function isModelError(value: unknown): value is LumiModelError`，判据是 `code` 以 `"E_"` 开头
> 实际：`invoke` 抛的是**普通对象**（注释里也这么写），不是 `LumiModelError` 实例；
> 类型谓词却向 TS 承诺「这是 `LumiModelError`」。当前 `LumiModelError` 只有数据字段，无方法，故不会出错；
> 但将来若给它加方法（如 `toUserMessage()`），调用处会在运行时 `undefined is not a function`。
> 另：任何 `code: "E_*"` 的非模型错误（如未来的 `E_EXPORT`）都会被误判为模型错误。
> 建议：谓词改成 `value is ModelErrorPayload`（一个纯接口），并在注释里列出白名单码。

> **N5 · 双真相残留（工程师已自述）** —— `src-tauri/src/ai_engine/skin_segmentation/model.rs`
> `status()` 已转发 registry，但 `REGISTERED_SKIN_MODELS`（空表）、`verify()`、`locate()`、
> `is_commercial_license()` 仍然保留，只被 4 条既有单测使用；因为是 `pub`，clippy 不会报 dead_code。
> 影响：下一个人很可能改错地方。建议 R13 一并删除或改成 registry 薄封装（与报告 §10.1-3 一致）。

> **N6 · `TensorSpec::validate` 缺「expected > 0」守卫：`[0,0,0,0]` 占位契约 + 空数据能通过校验**
> —— **【已关闭】`96c716d` 加了 `is_placeholder()` 守卫，见 §11.3 复核**
> 位置：`src-tauri/src/ai_engine/inference/tensor.rs:156-184`（`validate`）；占位形状来源 `model_registry/snapshot.rs:192`
> 期望：契约形状含 0（"未登记 / 占位"）时，构造张量必须直接拒绝并说明原因
> 实际（QA 最小用例实测，跑在 `ff91cee` 上，与后续 `Cow` 改动**无关**）：
> ```
> element_count(shape=[0,0,0,0])                = 0
> TensorBatch::new(shape=[0,0,0,0], data=[])    = Ok(0)               ← 空数据被放行
> TensorBatch::new(shape=[0,0,0,0], data=[1.0]) = E_MODEL_CONTRACT    ← 非空被挡
> check_contract(占位 spec, 真实 YuNet)          = E_MODEL_VERSION     ← 占位不当通配
> ```
> 机理：`element_count()` 是 `shape.iter().product()`，`[0,0,0,0]` 乘积为 `0`，于是 `validate` 的三道关
> 对空切片全部空转（长度 `0==0`、`position(!is_finite)` 得 `None`、值域 `find(越界)` 得 `None`）。
> **当前不可达**（因此定级「一般」而非「严重」），三层兜底：① 占位契约只用于前端展示，全仓库无路径回灌
> `TensorBatch::new_for`（唯一调用点 `ai/face.rs:281` 用 `YUNET.input`）；② `CpuTractBackend::run`
> 用 `same_as`（精确相等）拦；③ `check_contract` 的通配是 `actual == 0`（**模型侧**动态维），
> 不是 `expected == 0`，故「spec 写 0」不会通配真实模型（已实测 `E_MODEL_VERSION`）。
> 建议：`validate` 开头加 `if expected == 0 { return Err(ContractMismatch { … "契约形状含 0（未登记/占位）" }) }`
> + 一条「空数据 + 占位形状必须 `is_err()`」的测试（3 行代码）。这样错误会出现在**构造处**而不是后端。
> 附带说明：`shape_matches` 的反面语义（模型侧动态维为 0 时 spec 写任何值都算匹配）是**有意**的（支持动态 batch），
> 本轮 YuNet 全静态不受影响，但应在注释里点明，否则将来接 L2/L3 动态模型时容易被误读成"契约都校验过了"。

### 观察项（不计缺陷，但需主理人知情）

- **O1 · 训练数据许可仍是 `Unverified` 却随包分发**：`gate()` 不拦 `training_data`（`license.rs:141-146`
  只 gate 三个字段），这是**有意设计**（只披露不阻塞），`unverifiedFields` 如实列为 `["trainingData"]`。
  但从合规角度这是「未闭合项」：WIDER FACE 数据集条款未核实，建议要一份法务签字，或 R12 里补证。
- **O2 · `release` profile 的 `panic = "abort"` 让 `catch_unwind` 失效**（工程师已自述 §10.1-6）。
  即正式包里算子 panic 不会被隔离成 `E_INFER`。本轮不改，但建议记录到 R12 风险清单。
- **O3 · `face_model_status` 仍是同步命令**（`commands/faces.rs:47`）。但它走 `check_shallow`
  （实测 0.0711 ms，只 stat + 读 8 字节），不构成 R1 问题。**符合**设计 D7 的取舍。
- **O4 · `contract_is_checked_before_sha256` 证明的「顺序」依赖 `include_contract=true` 才成立**；
  热路径 `FaceEngine::load` 用的是 `include_contract=false`（契约推迟到后端解析后补验），
  所以「链上第 ④ 步在前」在热路径上不是字面连续的。工程师已在 `R11_ROUND_REPORT.md` §10.1-4 写明，
  可观测语义一致（都产出 `E_MODEL_VERSION`，并用 `wrong_input_shape_is_a_version_mismatch` 覆盖）。**接受**。

---

## 7. 离线与依赖红线

| 检查 | 方法 | 结果 |
|------|------|------|
| 新代码无网络调用 | `grep -rnE "reqwest\|ureq\|TcpStream\|std::net\|https?://" src-tauri/src src-tauri/tests` 共 9 处命中，逐条人工核对 | **全部是字符串字面量，无一处发起请求**：`builtin.rs:106/114/128/129`（证据 URL 与 `what_to_check` 提示文案）、`verify.rs:238`（`starts_with(b"version https://git-lfs")` 前缀比较）、`model_hub/security.rs:133`（同上，既有代码）、`providers/{apple_vision,yunet}.rs`（既有能力描述）、`tests/onnx_worker.rs:41`（测试夹具 JSON 里的许可 URL）。**无 `reqwest`/`ureq`/`TcpStream`/`std::net` 任何使用** |
| 依赖表零改动 | `git diff 572f8f0 ff91cee --stat -- src-tauri/Cargo.toml src-tauri/Cargo.lock` | **输出为空**（文件未被改动） |
| `tract-onnx` 版本 | 代码与报告一致（0.23，`Cargo.toml` 未动） | ✅ |

---

## 8. 性能数字复现（至少 1 项 —— 实际复现了 5 项）

在同一台机器、同一 debug profile、`ff91cee` 源码的隔离 worktree 上重测（`/tmp/r11v/src-tauri/tests/qa_probe.rs`）：

| 操作 | 报告值 | QA 实测 | 比值 | 判断 |
|------|--------|---------|------|------|
| `sha256_file`（232,589 B，20 次均值） | 9.586 ms | **13.528 ms** | 1.41× | 同量级 |
| `verify::check_shallow` | 0.109 ms | **0.0711 ms** | 0.65× | 同量级，**仍远低于 1 ms 红线** |
| `inspect_contract`（只解析） | 19.234 ms | **26.948 ms** | 1.40× | 同量级 |
| `verify::check_at`（全链） | 28.339 ms | **44.538 ms** | 1.57× | 同量级 |
| `Registry::snapshot`（1 个内置模型） | 0.004 ms | **0.0097 ms** | 2.4× | 同量级（均为 µs 级；若现算哈希必 ≥ 9.5 ms） |
| YuNet 单次推理（640×640，debug） | 880–974 ms | **1005 ms** | ~1.05× | 同量级 |

报值系统性偏低约 1.4×，属机器/负载差异（我的工作区在同时跑 clippy 与前端测试），
**不影响任何结论**。内部一致性校验通过：`check_at ≈ inspect_contract + sha256`
（报告 19.2+9.6=28.8 vs 28.3；实测 26.9+13.5=40.4 vs 44.5），说明这几个数字是真实测量而非编造。

**关键结论**：`snapshot` 0.0097 ms 与 `sha256` 13.5 ms 差 3 个数量级，**物理上证明 snapshot 没有读全文件算哈希**，
`check_shallow` 0.0711 ms 证明 `face_model_status` 不会阻塞主线程。§2/A5 与 §2/A8 的结论由此独立成立。

**关于「真实照片验收」的诚实性**：`R11_ROUND_REPORT.md:310-312` 明确写「**未执行** GUI 手工验收（如实说明）」，
并列出 3 条留待人工确认项。**如实，未粉饰。** 自动化侧确有真实样张覆盖：
`beauty_real` 4 例（33.75s）、`round10_real_photos` 5 例、`raw_real` 2 例，均真实执行且全绿。

**关于 `face_detection` / `beauty_real` / `face_intelligence` 是否真的执行**（看 test 名，不只看 passed 数）：
- `tests/face_detection.rs`：`detects_face_in_single_portrait`、`dark_portrait_still_detected_or_fails_explicitly`、
  `reports_zero_faces_for_landscape_without_people`、`...` — **4 passed，0 ignored**
- `tests/beauty_real.rs`：`real_group_faces_get_disjoint_owned_masks`、`real_portrait_retouch_changes_skin_but_not_eyes`、
  `real_portraits_have_sane_skin_masks_and_protected_features` — **4 passed，0 ignored，33.75s**
- `tests/face_intelligence.rs`：`dense_gate_unlocks_level3_warp`、`apple_vision_returns_real_dense_regions`、
  `project_face_roundtrips_dense_analysis` … — **5 passed，0 ignored**

---

## 9. 工作区与提交范围

| 检查 | 结果 |
|------|------|
| 提交范围（`git show --stat ff91cee`） | 29 files / +7181 / −116，与报告 §12 逐字一致。只含 `src-tauri/src`、`src-tauri/tests`、`src/lib`、`src/types.ts`、3 份 docs。**无** `.workbuddy/`、`REVIEW_PERF_SECURITY.md`、`UX_EXPERIENCE_AUDIT_*.md`、`target/`、`.env` |
| 基线复跑时的工作区（21:38） | 干净，只剩 3 个未跟踪：`?? .workbuddy/`、`?? docs/REVIEW_PERF_SECURITY.md`、`?? docs/UX_EXPERIENCE_AUDIT_2026-11-22.md` ✅ **与交办要求一致** |
| 复跑结束时的工作区（21:41:45 → 21:48 仍在变） | **已变脏**：先是 10 个 ` M` 文件，21:48 已增至 **12 个**（含 `src-tauri/tests/model_registry.rs`）+ 上述 3 个 `??` + 本报告 `?? docs/R11_ACCEPTANCE.md`；且 `cargo test --lib` 编译失败（`E0382`）。见 §6-P1 |
| `AppError` 的 `Serialize` 未被改动 | `git diff 572f8f0 ff91cee -- src-tauri/src/error.rs` → **输出为空** ✅ |

---

## 10. 结论：**可以进入 R12（YES）**

**判定依据**：本轮 4 条红线（不回归 / 零 warning / 双特性 / 离线）全部实测通过；
R11 的核心承诺（许可闸门先于一切 IO、`Raw255` 量纲单一真相、11 个稳定错误码、快照 O(1)）
都有**独立于工程师自述的**实证；`ValueRange::Raw255` 的「偏离」经独立推导确认为**修正设计错误**。

**进入 R12 的 3 个前置条件**（P1 已在 `96c716d` 完成，见 §11）：

1. **P1【已完成】**：工程师把 `ModelIntrospection` WIP 收尾并提交 → 产出 `96c716d`，
   `cargo test --lib` 471 全绿、clippy 0 warning、双特性一致（§11.1）。
2. **S1（R12 第一件事，⚠️ `96c716d` 未做）**：把 `detect_and_detect_with_cancel_are_equivalent`
   从「纯灰 0 脸」换成真实样张并比较 `faces` 全字段（§5-W1、§11.4）。顺手把 W2/W3/W4 一起补强
   —— 4 条合计约 30 行改动，但它们是目前「测试看起来很多、实际不防守」的主要来源。
   **这是本次签收唯一的不合格项。**
3. **S2（R13 硬性前置）**：动 `builtin::SKIN_MODELS` 之前，必须把
   `skin_segmentation::status()` 里的 `verify::check(…, false)` 换成 `check_shallow`
   或落盘缓存，否则 R1 立刻复活（§6-S2）。

**不建议**把 N2（LFS 死判据）、N4/N5 作为 R12 的阻塞项，但建议 N1（命令层零测试）与 N2
排进 R12（都在 1 小时内）。N6 已关闭。

---

## 11. 签收复核（在 `96c716d` 上）

### 11.1 四件套复跑

| # | 命令 | `ff91cee` | `96c716d` | 判断 |
|---|------|-----------|-----------|------|
| 1 | `cargo test --lib` | 466 / 0 / 3 | **471 / 0 / 3**（5.48s） | ✅ +5 例，无回退 |
| 2 | `cargo test --test model_registry` | 17 / 0 | **18 / 0**（1.36s） | ✅ +1 例 |
| 3 | `cargo clippy --all-targets` | 0 warning | **0 warning**（在 `/tmp/r11v` 切到 `96c716d`、删掉探针后强制重编，7.55s，无任何 `warning:`/`error:`） | ✅ |
| 4 | `cargo test --no-default-features --lib` | 466 / 0 / 3 | **471 / 0 / 3**（7.26s） | ✅ 与默认特性一致 |

`+5` 的来源：`tensor.rs` 的 `placeholder_shape_is_rejected_instead_of_silently_passing`、
`same_as_compares_every_field` 里新增的 `Cow::Owned` 判等断言、`spec.rs` / `snapshot.rs` / `builtin.rs`
各随 `Cow` 迁移新增的断言；`model_registry.rs` 的 `+1` 是 `inspect_covers_the_l2_l3_read_only_shape`（对应设计 T03⑨）。

### 11.2 对外契约：**无变化**（这是 `Cow` 改造最需要确认的一点）

| 检查 | 结论 | 证据 |
|------|------|------|
| `RegistrySnapshot` / `ModelEntryView` 字段 | **零增删** | `git diff 945bd46 96c716d -- snapshot.rs` 只有三处：`input: spec.input` → `input: spec.input.clone()`、占位形状 `&[0,0,0,0]` → `Cow::Borrowed(&[0,0,0,0])`、以及一条新断言 `assert!(view.input.is_placeholder())` |
| `TensorSpec` 的 JSON 形态 | **不变** | `serde` 对 `Cow<'static,[usize]>` 与 `&[usize]` 同样序列化为数组；`registry_snapshot_is_frontend_ready` 里 `model["input"]["shape"] == [1,3,640,640]` 仍通过（18/18 绿） |
| `src/types.ts` | **仅新增注释**，无类型/联合类型变更 | diff 只有 6 行 `//` 注释（D6 处置约定：`E_MODEL_CORRUPT` 与 `E_MODEL_VERSION` 必须同处置） |
| `src/types.ts` 是否被漏提交 | **没有漏** | `git show --name-only 96c716d` 含 `src/types.ts`（这一点是主理人特别提醒的风险，已核实） |
| `ModelSpec` 由 `Copy` → `Clone` 的调用点 | **无损坏** | `cargo check --lib --tests` 0 error；`cargo test --lib` 471 全绿；唯一依赖 `Copy` 的测试夹具 `leak_shape`（`Box::leak`）已随提交改成 `owned_shape`（`Cow::Owned`）；`builtin::YUNET` 是 `const`，`..YUNET` 结构体更新语法不需要 `Copy` |

### 11.3 新增内容审定：`is_placeholder()` 守卫 —— **我的 N6 被采纳，且比我的建议更好**

`tensor.rs:185-189` 新增 `is_placeholder()`（`shape` 全为 0 或空形状），`validate()` **首行**即拒绝：

```rust
if self.is_placeholder() {
    return Err(InferenceError::ContractMismatch {
        model_id: String::new(),
        detail: format!("契约形状 {} 是占位值（全 0），未真实描述张量，拒绝推理", format_shape(self.shape.as_ref())),
    });
}
```

比我建议的 `expected == 0` 更好：它把「为什么是 0」的语义（占位）显式化，错误信息可读。
配套测试 `tensor.rs:362` 覆盖双向（占位 + 空数据 → `E_MODEL_CONTRACT` 且消息含「占位」；`UNIT` 不是占位）。
`snapshot.rs` 也补了 `assert!(view.input.is_placeholder())`，把「用户模型契约是占位」写成断言。

**QA 复验**：在 `ff91cee` 上我实测 `TensorBatch::new([0,0,0,0], [])` 为 `Ok(0)`（§6-N6）；
现在同样的调用返回 `E_MODEL_CONTRACT`。**该缺陷已关闭。**

一处小提醒（不阻塞）：`is_placeholder()` 用 `.all(|dim| *dim == 0)`，对**空形状** `[]`（标量张量）
会因空集合的 `all()` 恒真而被判为占位。本轮无标量模型，不影响；将来接标量输出时应改成
`self.shape.is_empty() || self.shape.iter().all(|d| *d == 0)` 并显式决定标量是否允许。

### 11.4 ⚠️ 未交付：4 条空转测试与 `verify.rs` 注释**一个字都没改**

> **【本节已过时 —— 保留作为时间线记录】** 下面 6 项在**终版 `16f64a1` 中全部补齐**，
> 逐条复核见 **§12.3**。保留此节是为了说明「`96c716d` 当时确实没做」，避免后来者误以为
> 这轮从一开始就是达标的。

这是本次签收唯一的不合格项。主理人的收尾清单包含「修 4 条空转测试 → 补 `verify.rs` 强证明测试 →
补命令层序列化契约测试」，但 `96c716d` 的文件清单里**没有** `src-tauri/src/ai/face.rs`、
**没有** `src-tauri/src/ai_engine/model_registry/verify.rs`、**没有** `src-tauri/src/commands/model_status.rs`。
逐条核对（全部与 `ff91cee` 逐字节相同）：

| 编号 | 位置 | 现状（`96c716d`） | 能失败吗 |
|------|------|-------------------|----------|
| W1 | `src/ai/face.rs:398` | `git diff 945bd46 96c716d -- src-tauri/src/ai/face.rs` **输出为空**（文件根本不在提交里）。仍是纯灰 64×64，我实测 0 张脸 → 仍在比 `0 == 0` | ❌ 不能（除非函数签名级破坏） |
| W2 | `tests/model_registry.rs:405` | 函数体一字未改：`states` 数组仍只有 `Verified`/`Ready` 两个元素，仍只比 `expected[0]`/`expected[1]`，仍靠 `assert_eq!(expected.len(), 7)` 凑数；**没有 `include_str!`，没有遍历 7 个变体** | ❌ 不能（改 `types.ts` 也不会红） |
| W3 | `spec.rs:156` | 一字未改：仍校验硬编码字面量 `"face-detect-yunet-2023mar"`，**不遍历 `builtin::ALL`** | ❌ 永不失败 |
| W4 | `mod.rs:324` | 一字未改：仍只有 `assert_eq!(entry.sha256, builtin::YUNET.sha256)`，**没有"翻一字节长度不变"的构造，也没有计时** | ❌ 不能（现算与不现算值一样） |
| N3 | `verify.rs:9` | 仍写着 `` `tests/model_registry.rs::license_block_never_reads_the_file` `` —— 该函数不存在 | — （文档缺陷，仍在） |
| N1 | `commands/model_status.rs` | 仍 **0 个 `#[test]`**；命令层序列化契约测试未补 | — （覆盖缺口，仍在） |

**影响评估（为什么不把它当"阻断"，但也不能当"已修"）**：
R11 的功能正确性**不依赖**这 4 条测试 —— 真正的护栏是 `tests/face_detection.rs`（真实样张检测到脸）、
`registry_snapshot_is_frontend_ready`（20+9+6 个字段逐一断言）、`error.rs::every_variant_has_a_stable_code`
（11 码逐一比对）。所以**代码可以签收**。但测试**可信度**没有提升：这 4 条仍会在未来给出"绿色假安全感"，
尤其 W2（前后端状态字符串）与 W4（快照不哈希）正好守着两个最容易悄悄漂移的地方。

### 11.5 签收结论（针对 `96c716d`，已被 §12 取代）

> **【已被 §12 取代】** 下面是当时对 `96c716d` 的判断，保留以记录时间线。

- **代码（`96c716d`）：签收通过。** 无回归（471/18 全绿、clippy 0 warning、双特性一致）、
  对外契约零变化、`Cow` 改造干净、并额外关掉了一个静默校验洞（N6）。
- **测试质量债：未偿还。** 4 条空转测试 + N3 注释 + N1 命令层零测试，与 `96c716d` 之前完全相同。
- **建议**：不要把这条债留给 R12 的"有空再说"。要求工程师补一个**只含测试与注释、不碰产品代码**的提交
  （`ai/face.rs` 测试、`tests/model_registry.rs` 一条、`spec.rs` 一条、`mod.rs` 一条、`verify.rs` 三行注释、
  `commands/model_status.rs` 两条纯函数测试），风险接近零、耗时约 30–60 分钟。补完后我做一次
  `cargo test --lib` + `cargo test --test model_registry` 即可关账。
- **若主理人决定不补**：R11 仍可进 R12（功能已达标），但必须在 R12 任务书里把上述 6 条**显式登记为
  R11 遗留债**，并在报告里把 A7/A8 保持"部分通过"，不得改写成"全部通过"。

---

## 12. 终版签收（`16f64a1`）

### 12.1 复跑

| # | 命令 | `96c716d` | `16f64a1` | 判断 |
|---|------|-----------|-----------|------|
| 1 | `cargo test --lib` | 471 / 0 / 3 | **477 / 0 / 3**（7.16s） | ✅ +6 例（`commands/model_status.rs` 4 条 + `verify.rs` 强证明 + 其它），无回退 |
| 2 | `cargo test --test model_registry` | 18 / 0 | **18 / 0**（1.25s） | ✅ 条数不变（W2/W3/W4 是**改造/改名**而非新增，这正是想要的结果） |
| 3 | `cargo clippy --all-targets` | 0 warning | **0 warning**（`/tmp/r11v` 切 `16f64a1` 后 `touch src/lib.rs` 强制重编，输出 `Checking lumi-retouch` → `Finished in 4.26s`，**无任何 `warning:`**） | ✅ |
| 4 | 工作区 | 干净 | **干净**（仅 4 个未跟踪） | ✅ |

`git show --stat 16f64a1`：10 files / +523 / −54，含 `ai/face.rs`(+49)、`verify.rs`(+78)、
`commands/model_status.rs`(+132)、`spec.rs`(+54)、`mod.rs`(+45)、`tests/model_registry.rs`(+101)、
`tensor.rs`(+37)、`builtin.rs`(+8)、`snapshot.rs`(+8)、`ROUND_REPORT.md`。
`src/types.ts` 不在本提交中 —— **正常**：W2 只是**读取**它（`include_str!`），联合类型本身无需改动。

### 12.2 关键测试的执行证据

`cargo test --lib` 输出里可见（不是只数总数）：
```
test ai::face::tests::detect_and_detect_with_cancel_are_equivalent_on_a_real_portrait ... ok
```
这条通过本身就证明了「真实人像确实检出了人脸」（测试内 `assert!(!legacy.faces.is_empty())` 会先执行），
即比较不再是 `0 == 0`。

### 12.3 四条空转测试 + 两项缺口：逐条判定「现在能不能失败」

| 编号 | 终版实现（`16f64a1`） | 能失败吗 | 判据 |
|------|----------------------|----------|------|
| W1 | `ai/face.rs:398` 改名为 `detect_and_detect_with_cancel_are_equivalent_on_a_real_portrait`；用 `tests/fixtures/images/portrait.jpg` + `decoder::decode` + `fit_within(…,1800)`；先 `assert!(!legacy.faces.is_empty(), "真实人像未检出人脸，本测试会退化成 0 == 0")`；再逐字段比 `id/uuid/score/x/y/width/height` + 每个 landmark 的 `x/y`（`close()` 按量级缩放的 epsilon） | ✅ **能** | 若把 `detect_with_cancel` 的 `width/height` 传错、或后处理坐标换算错、或关键点顺序错，逐字段比较必红；人脸数为 0 时前置断言先红 |
| W2 | `tests/model_registry.rs:405` 函数体重写：**7 个变体全部构造**并逐一 `assert_eq!(state.as_str(), expected)`；再 `include_str!("../../src/types.ts")` 切出 `export type ModelState = …;` 段，**双向**比对（每个 Rust 状态必须出现在 TS union 里；TS union 里每个字面量必须出现在 Rust 已知状态里） | ✅ **能** | 在 `types.ts` 里删掉一个状态字面量 → 正向断言红；在 `types.ts` 里多加一个 → 反向断言红。**这正是我建议的跨语言锁，且比原建议更严** |
| W3 | `spec.rs:185` 改名为 `every_registered_model_id_follows_the_naming_contract`；遍历 `builtin::ALL`，校验长度/字符集/首尾非 `-`/无 `--`/**任务前缀**（`task_prefix(spec.task)`）/版本尾段长度/**不重复**；开头 `assert!(!builtin::ALL.is_empty())` 防「空表空转」 | ✅ **能** | 往登记表加一条不合规或重复的 ID → 立刻红 |
| W4 | `mod.rs:330` 改名为 `snapshot_does_not_hash_the_weight`；构造「拷真实权重 + **翻最后 1 字节、长度不变**」，登记目录指向该副本；断言 `snapshot` 仍报 `verified`（＝没算哈希）、`sha256` 来自登记表、`size_on_disk_bytes` 来自 `metadata`；**再加对照组** `verify::check_at(同一副本)` → `corrupt`，证明这份副本确实坏了 | ✅ **能** | 谁把 `snapshot()` 改成算 SHA，第一条断言立刻变 `corrupt` → 红；对照组保证实验不是「文件其实没问题」 |
| N3 | `verify.rs:9-19` 注释重写，列出**三组**测试：两条 `tests/model_registry.rs` 的原文名 + 新增的 `verify::tests::license_block_never_reads_the_file`；后者**确实存在**（`verify.rs:386`） | ✅ 已修 | 注释里三个名字我都 `grep "fn <name>"` 核实存在 |
| N1 | `commands/model_status.rs` 新增 4 条 `#[test]`：`runtime_status_serializes_with_camel_case_keys`（6 个 key + 值）、`status_from_state_maps_failures_into_the_view`、`inference_error_serializes_to_the_four_field_contract`、`failure_of_maps_every_code_to_a_matching_kind`（5 种失败态逐一映射，并**显式记录** `corrupt` 分支不回填 `actual_sha` 的现状） | ✅ 已修 | 命令层序列化契约（camelCase + 四字段）现在有护栏；`failure_of` 的映射错位会被抓 |

**额外收获（超出我的验收要求）**：`verify.rs:386` 新增的 `license_block_never_reads_the_file`
把我 §4 那套「`chmod 000` + 只改许可的对照实验」**固化成常驻测试**，并且比我更严谨 ——
它加了 `#[cfg(unix)]`、加了「体积必须与登记值一致，否则会先被第③步挡下」的前置说明、
加了 root 可绕过 `0o000` 时的 `eprintln!` 跳过分支、并同时断言两组（许可未过 → 只有 `licenseBlocked`；
许可通过 → `corrupt` 且 detail 含 `os error 13`）。这条测试的可信度高于我报告 §4 的一次性实验。

### 12.4 终版结论：**R11 可正式关账**

- 代码：477 / 18 全绿、clippy 0 warning、双特性一致、对外契约零变化、无回归。
- 测试质量：**§5 的 4 条弱测试 0 条残留**，N1/N3/N6 关闭。原来的「4/17 ≈ 24% 空转率」现在是 **0**。
- 仍**未解决**（不阻塞 R11 关账，必须写进 R12/R13 计划）：
  1. **S2（严重）**：`skin_segmentation/model.rs:63` 的 `status()` 用 `verify::check(…,false)` **仍含 SHA**。
     `SKIN_MODELS` 为空所以本轮无事，但 **R13 一登记权重，面板挂载就会整文件哈希 → R1 复活**。
     **这是动 `SKIN_MODELS` 的硬性前置条件。**
  2. **N2（一般）**：`verify.rs:238` 与 `model_hub/security.rs:133` 的 Git-LFS 判据是死代码
     （8 字节 header 比 23 字节 needle）。有体积检查兜底，不严重。
  3. **N4（一般）**：`isModelError` 的类型谓词向 TS 承诺 `LumiModelError` 实例，实际是普通对象
     （现在无方法所以不炸；将来给该类加方法会运行时出错）。
  4. **N5（一般）**：`skin_segmentation` 的双真相残留（`REGISTERED_SKIN_MODELS`/`verify`/`locate`/
     `is_commercial_license` 只剩测试在用，`pub` 所以无 dead_code 警告）。
  5. **O1（观察）**：`trainingData` 仍是 `Unverified` 却随包分发（设计上"只披露不阻塞"），建议要法务签字。
  6. **O2（观察）**：`release` profile `panic = "abort"` 让 `catch_unwind` 失效，正式包不隔离算子 panic。

---

## 附录 · 证据留存

| 文件 | 内容 |
|------|------|
| `/tmp/r11qa/01_lib.log` | `cargo test --lib`（`ff91cee`）尾部 + `466 passed; 0 failed; 3 ignored` |
| `/tmp/r11qa/02_nodefault_lib.log` | `--no-default-features --lib`（`ff91cee`）同上 |
| `/tmp/r11qa/03_clippy.log` | 工作区 clippy（缓存命中，`Finished in 0.52s`，**故不可作为零警告证明**） |
| `/tmp/r11qa/04_full.log` | 全量 `cargo test`（`ff91cee`）18 个 target 明细，合计 563 / 0 / 7 |
| `/tmp/r11qa/05_pnpm.log` | vitest `Tests 81 passed (81)` |
| `/tmp/r11qa/06_tsc.log` | `tsc --noEmit` 空输出 + `EXIT_TSC=0` |
| `/tmp/r11qa/07_clippy_fresh.log` | `ff91cee` 干净 worktree 强制 clippy：`Finished in 1m05s`，无任何 `warning:` |
| `/tmp/r11qa/10_lib_new.log` | `96c716d` `cargo test --lib` → `471 passed; 0 failed; 3 ignored` |
| `/tmp/r11qa/11_mr_new.log` | `96c716d` `cargo test --test model_registry` → `18 passed; 0 failed` |
| `/tmp/r11qa/12_clippy_new.log` | `96c716d` 强制 clippy：`Finished in 7.55s`，无 `warning`/`error` |
| `/tmp/r11qa/13_nodef_new.log` | `96c716d` `--no-default-features --lib` → `471 passed; 0 failed; 3 ignored` |
| `/tmp/r11qa/20_lib4.log` | `16f64a1` `cargo test --lib` → `477 passed; 0 failed; 3 ignored`（含 `detect_and_detect_with_cancel_are_equivalent_on_a_real_portrait ... ok`） |
| `/tmp/r11qa/21_mr4.log` | `16f64a1` `cargo test --test model_registry` → `18 passed; 0 failed` |
| `/tmp/r11qa/22_clippy4.log` / `23_clippy4_forced.log` | `16f64a1` clippy：`Checking lumi-retouch` → `Finished in 4.26s`，无任何 `warning:` |
| `/tmp/r11v` | `git worktree`（`ff91cee` → `96c716d` → `16f64a1`），曾含 `src-tauri/tests/qa_probe.rs`（对照实验 + 性能复现 + `[0,0,0,0]` 最小用例），签收前已删除以免污染 clippy |

> 本报告**未修改** `src-tauri/**` 与 `src/**` 的任何一行（`git show --name-only` 可核）；
> `/tmp/r11v` 为隔离 worktree，可用 `git worktree remove /tmp/r11v --force` 清理，对主工作区零影响。
