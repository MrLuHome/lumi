# R11 · AI 技术底座与模型准入 · 本轮报告

> 执行者：寇豆码（Engineer） · 团队 `software-lumi-r11` · 任务 #2
> 仓库：`/Users/lu/Documents/lumi人像` · 分支 `main` · 起点 HEAD `572f8f0`
> 上游设计：`docs/R11_AUDIT_AND_DESIGN.md`（高见远）—— 执行期间由 1247 行更新到 **1444 行**，
> 三条更正（`inspect()` / 模块树 / 流式 SHA）的落地情况见 §11.1.1
> 完成日期：2026-11-22

---

## 1. 本轮目标

把「四个互不相识的模型登记处」收敛成**一份权威登记表**，并给推理层补上
统一加载接口、统一 Tensor IO、统一错误类型；对外暴露可判别的模型状态与 11 个
稳定错误码。**不推倒重写、不删现有功能、不做 R12–R16。**

具体交付（对应设计的 T01–T05）：

1. `ai_engine/model_registry/` —— 权威登记层（8 个文件，不含 Tauri 依赖）；
2. `ai_engine/inference/` —— 统一推理抽象（6 个文件），trait 5 个方法含 `inspect()`；
   `CpuTractBackend` 是唯一真实实现，`GpuBackend` 只做 stub，`tract-onnx 0.23` 一行未动；
3. `ai/model.rs` 常量改为登记表派生 + `ai/face.rs` 走准入链 + `detect_with_cancel`（加法）；
4. `commands/model_status.rs` 4 个新命令 + 前端 `types.ts` / `backend.ts` 契约；
5. 旧表收敛（`skin_segmentation::status()` 转发 registry）+ 许可台账
   `docs/MODEL_LICENSE_REGISTER.md`；
6. `tests/model_registry.rs` 17 条验收测试。

---

## 2. 修改文件清单

### 2.1 新增（17 个文件）

| 文件 | 行数级别 | 职责 |
|------|----------|------|
| `src-tauri/src/ai_engine/model_registry/mod.rs` | 大 | `Registry`（`RwLock<BTreeMap>`）、`require`、用户模型只读枚举 |
| `src-tauri/src/ai_engine/model_registry/spec.rs` | 中 | `ModelTask` / `ModelFormat` / `ModelOrigin` / `OutputHead` / `Provenance` / `ModelSpec` |
| `src-tauri/src/ai_engine/model_registry/license.rs` | 大 | `Verdict3` / `LicenseRecord` / `LicenseEvidence` / **唯一闸门 `gate()`** |
| `src-tauri/src/ai_engine/model_registry/state.rs` | 大 | `ModelFailure`(5 态) / `ModelRuntimeState`(7 态) / `ModelHealth`(blocking+findings) |
| `src-tauri/src/ai_engine/model_registry/locate.rs` | 小 | 纯函数 `locate(&[PathBuf], dir, file)`，不碰 `AppHandle` |
| `src-tauri/src/ai_engine/model_registry/verify.rs` | 大 | 准入链实现（许可→存在→体积/头→契约→SHA）+ 浅检查 |
| `src-tauri/src/ai_engine/model_registry/builtin.rs` | 大 | `YUNET` 常量（唯一真相）+ 空皮肤/分割登记表 |
| `src-tauri/src/ai_engine/model_registry/snapshot.rs` | 大 | 前端视图（camelCase）+ `RegistrySnapshot` + 汇总 |
| `src-tauri/src/ai_engine/inference/mod.rs` | 小 | re-export |
| `src-tauri/src/ai_engine/inference/error.rs` | 中 | `InferenceError` 11 变体 + `code()` + 手写 `Serialize` |
| `src-tauri/src/ai_engine/inference/tensor.rs` | 中 | `Dtype`/`Layout`/`ChannelOrder`/`ValueRange`/`TensorSpec`（`shape: Cow`）/`TensorBatch` |
| `src-tauri/src/ai_engine/inference/backend.rs` | 中 | `InferenceBackend` trait（含 `inspect()`）/ `ModelIntrospection` / `LoadedPlan` / `PlanHandle` / `RunOptions` |
| `src-tauri/src/ai_engine/inference/cpu_tract.rs` | 大 | 唯一真实后端：解析→契约校验→优化→编译；`catch_unwind` 隔离 panic |
| `src-tauri/src/ai_engine/inference/gpu_stub.rs` | 小 | `GpuBackend`（`available:false` + 理由，**不接 wgpu**） |
| `src-tauri/src/commands/model_status.rs` | 中 | 4 个命令（3 个 `spawn_blocking` + 1 个纯内存同步） |
| `src-tauri/tests/model_registry.rs` | 大 | 17 条 R11 验收测试 |
| `docs/MODEL_LICENSE_REGISTER.md` | — | 许可证据台账（人可读） |

### 2.2 修改（10 个文件）

| 文件 | 改动 | 风险控制 |
|------|------|----------|
| `src-tauri/src/ai_engine/mod.rs` | +2 行 `pub mod inference; pub mod model_registry;` | 极低 |
| `src-tauri/src/ai/model.rs` | 常量改为 `builtin::YUNET` 派生；`MODEL_INPUT` 加回归测试锁死量纲 | 签名与符号名不变 |
| `src-tauri/src/ai/face.rs` | 加载走准入链 + `CpuTractBackend`；新增 `detect_with_cancel`/`detect_checked`/`load_checked`/`is_loaded`/`unload`；`status()` 真正填 `error` | **本文件是唯一能弄红 `beauty_real` 的地方**，已单独回归 |
| `src-tauri/src/ai_engine/skin_segmentation/model.rs` | `status()` 转发 registry；回退文案逐字未改 | 4 条既有单测仍全绿 |
| `src-tauri/src/ai_engine/segmentation/model.rs` | 仅模块注释标注「调研目录，非加载源」 | 零逻辑变更 |
| `src-tauri/src/commands/beauty.rs` | `skin_model_status` → `async`+`spawn_blocking`（形状不变）；`preview_skin_mask` 不再吞掉失败脸（`MaskPreview.warnings`） | 返回形状兼容（新增字段可省） |
| `src-tauri/src/commands/mod.rs` | +1 行 `pub mod model_status;` | 极低 |
| `src-tauri/src/lib.rs` | `.manage(Arc<Registry>)` + 4 个 handler | 极低 |
| `src/lib/backend.ts` | +`LumiModelError` / `isModelError` / 4 个函数 | 只追加，未改既有函数 |
| `src/types.ts` | +R11 类型；`MaskPreview.warnings?` 可选字段 | 只追加 |

### 2.3 明确未改

- `src-tauri/Cargo.toml`：**零新增依赖**（`sha2`/`thiserror`/`serde`/`tract-onnx`/`rayon` 已足够）。
- `src-tauri/tauri.conf.json`：按 Q3 不改，`resources/models/skin/**` 本轮不入包，
  已写进 `docs/MODEL_LICENSE_REGISTER.md` 第 5 节作为台账。
- `src/components/**`：**零改动**（`BeautyPanel` / `FacePanel` / `ModelHubPanel` 全部不动）。
- `src-tauri/src/face_intelligence/**`：零改动（registry 只**读** `model_hub` 的 manifest，单向依赖）。
- `AppError` 的 `Serialize`：**未改**（仍是裸字符串，10+ 处前端 `catch` 不受影响）。

---

## 3. 核心实现说明

### 3.1 准入判定链（顺序固定，代码即文档）

`ai_engine/model_registry/verify.rs::check_at`：

```text
① 许可闸门（license::gate，纯内存，µs）
      └─ 未过 → push(LicenseNotCleared)；仍然只做一次 is_file()，
                连体积/头/SHA 全部跳过 → 直接返回
② 文件存在（is_file，一次 metadata）
③ 体积 + 格式头（metadata + 读 8 字节，排除 ZIP/ELF/Mach-O/PDF/NumPy/LFS 指针）
④ 契约（inspect_contract：tract 只解析不优化，比对输入形状 + 12 个 head 形状）
⑤ SHA256（model_hub::security::sha256_file，流式 1 MB 缓冲，不整文件进内存）
⑥ 解析（CpuTractBackend::load：解析→再验契约→优化→编译）
```

为什么许可必须第一：法务优先（未授权权重一个字节都不读）、成本最低、语义最重要。
为什么还要 `findings` 全量：`blocking` 取顺序第一个，但 `Missing` 不得掩盖 `License` ——
`ModelHealth { blocking, findings, resolved_path }` 同时给两者。

**「许可未过就没读文件」是被测试证明的**，不是口头承诺：
`tests/model_registry.rs::non_commercial_weights_are_rejected_before_any_read`
构造了一个「文件存在、内容完全不是 ONNX、体积也不符」的权重，配一份
`weights = NotAllowed` 的许可记录，断言 `findings.len() == 1` 且唯一发现是
`licenseBlocked` —— 如果实现先做体积/哈希，必然出现 `corrupt`。

### 3.2 「Missing 掩盖 License」的正面用例

`license_failure_blocks_before_missing`：许可 `NotAllowed` + 路径不存在时

```
blocking = licenseBlocked
findings = ["licenseBlocked", "missing"]   ← 两个都要有
state    = "licenseBlocked"
```

### 3.3 未核实字段

- `LicenseRecord` 四字段独立：`weights` / `training_data` / `commercial_use` / `redistribution`。
- `unverified_fields()` → `["trainingData"]`（YuNet 当前唯一未核实项）。
- `RegistrySnapshot.models[].license.unverifiedFields` 原样输出，前端可显示
  「权重可商用 · 数据集条款待核」。
- `license.note` 明确写了「训练数据许可未核实、上游权重仓许可未核实」，
  **没有**根据仓库 LICENSE 推断权重可商用。

### 3.4 统一推理抽象

- `InferenceBackend` 四个方法：`id` / `capabilities` / `load` / `run` / `unload`。
- `CpuTractBackend`：`model_for_path → into_typed → 契约校验 → into_optimized → into_runnable`，
  `run` 内部 `catch_unwind`（先例 `model_hub/inference.rs:95`）——注意 release
  profile 是 `panic = "abort"`，`catch_unwind` 只在 dev/test 生效，这一点写进了代码注释。
- `GpuBackend`：`available:false` + 理由“缺少 tract GPU 后端在 640×640 上的 benchmark 依据”，
  `load/run/unload` 一律 `E_BACKEND`。**没有引入 wgpu、没有替换运行时。**
- `TensorBatch::new` 校验：元素数（→`E_MODEL_CONTRACT`）、NaN/Inf（→`E_INPUT`）、
  **值域**（按 `ValueRange::bounds()`）。

### 3.4.1 `inspect()`：只读结构探测（设计更新后新增）

`InferenceBackend` 有 5 个方法，其中 `inspect()` 是设计更新（1247 → 1444 行，§8.1）新增的：

```rust
fn inspect(&self, path: &Path, cancel: &CancelToken) -> Result<ModelIntrospection, InferenceError>;
```

为什么不靠 `load()` 覆盖：`face_intelligence/model_hub/validator.rs:87` 用
`proto_model_for_path`（只读 `ir_version` / `opset` / 形状，**不编译 runnable**），
`:100` 才用 `model_for_path`，是两个不同 API。`load()` 一定会编译 runnable，
对"只想看一眼这个文件长什么样"的导入校验是过度开销，也不能容忍编译失败。

实现分两段，与 validator 的两段式完全对应：

1. `tract_onnx::onnx().proto_model_for_path(path)` → `ir_version` / `opset` / `operator_count`；
2. `model_for_path(..).into_typed()` → 输入输出形状（符号维记 `0`，可读描述进 `declarations`）。

`declarations` 承载"`TensorSpec` 表达不了"的信息，而不是硬塞：ONNX **不声明值域与通道顺序**
（那是训练侧约定），`inspect()` 只能给中性占位，并把这件事显式写进 `declarations`；
非 F32 dtype 的 IO 也进 `declarations` 而不进 `inputs/outputs`（`Dtype` 只有 F32，硬写就是撒谎）。

`GpuBackend::inspect` 一律返回 `E_BACKEND`。
`registry::verify` 的契约判定复用 `inspect()`（`inspect_contract` 成了它的薄封装），
因此**全仓只有一处 tract 解析入口**。

### 3.5 取消（复用，不新造）

- 直接用 `image_engine::cancel::CancelToken`（`src-tauri/src/image_engine/cancel.rs:9`），零新类型。
- `FaceEngine::detect` 签名与行为完全不变，内部转调
  `detect_with_cancel(.., &CancelToken::default())`（空令牌 = 永不取消）。
  三处调用方（`commands/faces.rs:73`、`commands/beauty.rs:226`、
  `providers/yunet.rs:146`）**零改动**。
- 检查点：`load` 前后、预处理每 1/8、`plan.run()` 前后、后处理前。
  **`plan.run()` 内部无法中断**（tract 0.23 不暴露中断钩子）——已在代码注释与本文写明，
  不承诺做不到的可取消性。

### 3.6 前端契约

- 新命令返回 `InferenceError` → `{code, message, modelId, detail}`（手写 `Serialize`）。
- 11 个稳定错误码与 `ERROR_CODES` 常量同源，并有单测断言
  `sample().iter().map(code) == ERROR_CODES`，防止新增变体漏映射。
- `isModelError` **不用 `instanceof`**（`invoke` 抛出的是普通对象），按 `code` 前缀 `E_` 判定。
- `modelLoad` / `modelUnload` 内部 `catch` 后重新 `throw new LumiModelError(...)`，
  前端可 `switch (error.code)` 做「缺失→重装 / 损坏→重新校验 / 未授权→灰按钮」分支。

---

## 4. 新增模型及许可证情况

**本轮没有新增任何模型权重。** 登记表只收敛了原有的 YuNet 一条：

| 模型 | ID | 权重许可 | 商用 | 再分发 | 训练数据 | 未核实字段 |
|------|----|----------|------|--------|----------|------------|
| YuNet 2023mar | `face-detect-yunet-2023mar` | Allowed (MIT) | Allowed | Allowed | **Unverified** | `trainingData` |

- 皮肤解析 / 人物分割登记表**刻意留空**（没有通过许可审查的权重）。
- 用户自备模型（Model Hub）不进正式清单：`official = false`、`license.state = userUnverified`、
  不参与默认美肤 / 默认精修（主理人裁决 Q1）。
- 完整证据链（URL + 原文摘句 + 访问日期）见 `docs/MODEL_LICENSE_REGISTER.md`

---

## 5. 新增 API 与数据结构

### 5.1 Tauri 命令（4 个，已注册到 `lib.rs` 的 `generate_handler!`）

| 命令 | 线程 | 返回 | 说明 |
|------|------|------|------|
| `model_registry_snapshot` | `spawn_blocking` | `Result<RegistrySnapshot, InferenceError>` | 只读内存 + `is_file` + `metadata`，**不算哈希**；实测 0.004 ms |
| `model_registry_check` | `spawn_blocking` | 同上 | 全量：体积 + 头 + 契约 + SHA；`modelIds` 省略 = 全部 |
| `model_load` | `spawn_blocking` | `Result<ModelRuntimeStatus, InferenceError>` | 预热，走完整闸门后填 plan |
| `model_unload` | **同步**（纯内存 + `is_file`/`metadata`） | 同上 | 释放 plan；同步安全性在代码注释里说明 |

命名避开了已被占用的 `list_face_models`（`commands/face_intelligence.rs`）。

### 5.2 `RegistrySnapshot`（camelCase，逐字对齐 `src/types.ts`）

```jsonc
{
  "schemaVersion": 1,
  "generatedAtUnix": 1795300000,
  "models": [{
    "id": "face-detect-yunet-2023mar",
    "displayName": "YuNet 2023mar",
    "version": "2023mar",
    "task": "faceDetect",
    "origin": "builtin",
    "official": true,                       // ← R11 新增：是否属于正式注册清单
    "format": "onnx",
    "file": "face_detection_yunet_2023mar.onnx",
    "sizeBytes": 232589,
    "sizeOnDiskBytes": 232589,
    "sha256": "8f2383e4…52fa4",
    "input": { "dtype": "f32", "layout": "nchw", "shape": [1,3,640,640],
               "valueRange": "raw255", "channelOrder": "bgr" },
    "outputs": [{ "name": "cls_8", "shape": [1,6400,1], "semantic": "stride 8 分类置信度" }, /* …12 个 */],
    "license": {
      "state": "verifiedCommercial",
      "spdx": "MIT",
      "weights": "allowed",
      "commercialUse": "allowed",
      "redistribution": "allowed",
      "trainingData": "unverified",
      "unverifiedFields": ["trainingData"],
      "evidence": [ /* officialPage + bundledLicenseFile，含 quote 与 accessedAt */ ],
      "note": "权重许可 MIT 已核对…；训练数据许可未核实…"
    },
    "state": "verified",
    "loaded": false,
    "path": "/…/resources/models/yunet/face_detection_yunet_2023mar.onnx",
    "failure": null,
    "findings": [],
    "checkedAtUnix": 1795300000
  }],
  "summary": { "total": 1, "official": 1, "ready": 0, "verified": 1, "blocked": 0, "userImported": 0 }
}
```

### 5.3 `ModelRuntimeStatus`

```jsonc
{ "modelId": "face-detect-yunet-2023mar", "state": "ready", "loaded": true,
  "elapsedMs": 128, "failure": null, "backend": "cpu-tract" }
```

### 5.4 错误体（`InferenceError` 序列化，11 码）

```jsonc
{ "code": "E_MODEL_CORRUPT", "message": "模型文件损坏：face-detect-yunet-2023mar",
  "modelId": "face-detect-yunet-2023mar", "detail": "SHA256 与登记值不符" }
```

`E_MODEL_LICENSE / E_MODEL_MISSING / E_MODEL_CORRUPT / E_MODEL_VERSION / E_MODEL_LOAD /
E_MODEL_CONTRACT / E_INPUT / E_INFER / E_TIMEOUT / E_CANCELLED / E_BACKEND`

### 5.4.1 `ModelIntrospection`（`inspect()` 的返回，Rust 侧）

```rust
pub struct ModelIntrospection {
    pub ir_version: Option<i64>,
    pub opset: Option<i64>,
    pub inputs: Vec<TensorSpec>,      // 符号维记 0（通配）
    pub outputs: Vec<TensorSpec>,
    pub operator_count: usize,
    pub declarations: Vec<String>,    // TensorSpec 表达不了的原始描述（符号维/非 F32/未知值域）
}
```

本轮**不暴露给前端**：设计 §8.6 验收点 ⑨ 只要求"能测形状可覆盖，不迁移 validator"，
没有命令消费它，因此不加对应的 TS 类型（避免出现没有调用方的空 API）。

### 5.5 TypeScript

`ModelErrorCode` / `ModelState` / `ModelFailure` / `ModelLicenseState` / `LicenseVerdict` /
`ModelLicenseEvidence` / `ModelLicenseView` / `TensorSpecView` / `ModelOutputHeadView` /
`ModelEntryView` / `RegistrySnapshot` / `ModelRuntimeStatus`；
函数 `modelRegistrySnapshot()` / `modelRegistryCheck()` / `modelLoad()` / `modelUnload()`；
类与判别函数 `LumiModelError` / `isModelError()`。

---

## 6. 自动化测试结果（真实命令 + 真实数字）

| # | 命令 | 结果 | 基线 | 判定 |
|---|------|------|------|------|
| V1 | `cargo test --lib` | **471 passed / 0 failed / 3 ignored** | 397 passed / 0 failed / 3 ignored | ✅ +74 条新单测，0 失败 |
| V2 | `cargo test`（全量 17 个 target） | **569 passed / 0 failed / 7 ignored** | 477 passed / 0 failed / 7 ignored | ✅ +92 条，0 失败 |
| V3 | `cargo clippy --all-targets` | **0 warning / 0 error**（`Finished dev profile`，无输出） | 0 warning | ✅ |
| V4 | `cargo check --no-default-features --lib` | **通过**（`Finished dev profile in 3.81s`） | 通过 | ✅ |
| V5 | `cargo test --no-default-features --lib` | **471 passed / 0 failed / 3 ignored** | 397 passed | ✅ 与 V1 完全一致 |
| V6 | `cargo test --test face_detection --test beauty --test beauty_real --test face_intelligence` | 4 + 12 + 4 + 5 = **25 passed / 0 failed**（`beauty_real` 真实跑满 **48.75 s**，未跳过） | 同 | ✅ 真实推理路径零回归 |
| V7 | `pnpm test`（前端 vitest） | **81 passed / 8 files**，235ms | 81 passed | ✅ |
| V8 | `npx tsc --noEmit` | **无输出（0 error）** | 0 | ✅ |

### 6.1 全量 target 明细（V2 展开）

| target | passed | ignored |
|--------|--------|---------|
| `src/lib.rs` | 471 | 3 |
| `tests/beauty.rs` | 12 | 1 |
| `tests/beauty_real.rs` | 4 | 0 |
| `tests/engine.rs` | 10 | 1 |
| `tests/face_detection.rs` | 4 | 0 |
| `tests/face_intelligence.rs` | 5 | 0 |
| **`tests/model_registry.rs`** | **18** | 0 |
| `tests/onnx_worker.rs` | 7 | 0 |
| `tests/raw_real.rs` | 2 | 0 |
| `tests/reshape_persistence.rs` | 3 | 0 |
| `tests/reshape_pipeline.rs` | 3 | 1 |
| `tests/round10_changeset.rs` | 11 | 0 |
| `tests/round10_performance.rs` | 5 | 0 |
| `tests/round10_real_photos.rs` | 5 | 0 |
| `tests/round8_benchmark.rs` | 0 | 1 |
| `tests/round9_1_persistence.rs` | 9 | 0 |
| **合计** | **569** | **7** |

### 6.2 `tests/model_registry.rs` 18 条验收测试逐条对应任务书验收点

| 验收点 | 测试名 |
|--------|--------|
| SHA256 通过 | `real_weight_passes_sha256_and_reports_verified` |
| SHA256 不符（损坏） | `tampered_weight_is_detected_as_corrupt_with_both_digests`（改最后 1 字节，体积不变，只有 SHA 能发现） |
| 缺失 → 错误码 + 降级 | `missing_weight_reports_e_missing_and_degrades_to_geometry`（含皮肤回退文案断言） |
| 缺失（引擎侧） | `engine_missing_model_surfaces_a_structured_error` |
| 版本不匹配 → `E_MODEL_VERSION` | `wrong_input_shape_is_a_version_mismatch`、`contract_is_checked_before_sha256` |
| 未授权 → 拒绝且**未读文件** | `non_commercial_weights_are_rejected_before_any_read` |
| 未核实商用 → 拒绝随包 | `unverified_commercial_field_blocks_bundled_weights` |
| 用户自备不纳入强闸门（Q1） | `user_imported_unverified_licence_is_allowed_locally_only` |
| 链序：License 不得被 Missing 掩盖 | `license_failure_blocks_before_missing`、`findings_expose_every_problem_not_just_the_first` |
| 推理可取消 | `cancelled_inference_returns_e_cancelled_without_panicking`、`uncancelled_inference_still_works_after_the_cancel_test` |
| 快照可序列化（camelCase/字段齐全） | `registry_snapshot_is_frontend_ready`、`snapshot_state_reflects_recorded_checks`、`model_runtime_state_strings_match_the_frontend_union` |
| R9 量纲护栏 | `yunet_value_range_is_raw255_consistent_with_the_implementation` |
| `inspect()` 覆盖 L2/L3 只读结构（§8.6 ⑨） | `inspect_covers_the_l2_l3_read_only_shape`；单测 `inspect_reports_ir_version_opset_and_io_without_compiling` / `inspect_is_cancellable` / `inspect_rejects_a_non_onnx_file`；`gpu_backend_rejects_inspect_with_backend_code` |

---

## 7. 真实照片验收结果

**未执行 GUI 手工验收（如实说明）。**

原因：
1. 本轮所有改动都在 `src-tauri/src/{ai,ai_engine,commands}` 与 `src/{lib,types}` 的类型层，
   **`src/components/**` 零改动**，没有任何 UI 行为变化需要人眼确认；
2. 主链路（美肤分析、批量导出、项目保存）由真实样张的自动化测试覆盖：
   `tests/beauty_real.rs`（6 张真实样张 / 27.88 s）、`tests/round10_real_photos.rs`、
   `tests/round9_1_persistence.rs`、`tests/raw_real.rs` 全绿；
3. 本轮未启动 `pnpm app:dev`（GUI 需要人工逐项点击，且需要真实安装包环境验证
   `resource_dir()` 布局；后者在 macOS 开发态由 `dev_base_dirs()` 回退覆盖）。

**留待人工确认的项（下一轮必须做）**：
- 打包后的 `resource_dir()` 双候选是否命中（本轮只在开发态验证过回退路径）；
- `BeautyPanel` 里 `skinModelStatus()` 由同步改异步后，挂载时序是否仍一致
  （返回形状未变、字段未变，理论零影响）；
- 面板「显示蒙版」在部分脸失败时是否把 `warnings` 展示出来。

---

## 8. 性能与内存数据（实测，debug profile）

用临时基准测试（`cargo test -- --nocapture`，跑完即删）测得：

| 操作 | 耗时 | 调用频率 | 说明 |
|------|------|----------|------|
| `sha256_file`（232,589 B） | **9.586 ms/次**（20 次均值） | 每次进程首次加载一次 | 流式 1 MB 缓冲；debug 未优化，release 约 1 ms 量级 |
| `inspect()`（proto 解析 + 类型推导，**不编译**） | **19.729 ms/次** | 仅 `model_registry_check` 与 `model_load` | 契约判定第 ④ 步；proto 解析相对类型推导几乎免费（旧 `inspect_contract` 19.234 ms） |
| `load()`（解析 + 优化 + 编译） | **230.527 ms/次** | 每次进程首次加载一次 | 有 `is_loaded()` 提前返回，不会进每张照片的路径 |
| `verify::check_at`（完整链，含契约 + SHA） | **28.315 ms/次** | 仅显式 check / 首次 load | = ④ + ⑤ 之和，符合预期 |
| `verify::check_shallow`（①②③，不读全文件） | **0.109 ms/次** | `face_model_status`（同步命令） | **远低于 1 ms 红线**（对比 R1 的整文件哈希） |
| `Registry::snapshot`（1 个内置模型） | **0.004 ms/次** | 面板挂载 | 纯内存 + `is_file` + `metadata` |
| YuNet 单次推理（640×640，debug） | 880–974 ms | 每次检测 | 与基线同量级（debug）；release 为几十 ms 级 |

**内存**：本轮没有新增常驻大对象。
- `Registry` 只存 `BTreeMap<String, RuntimeSlot>`（每个 slot 几十字节 + findings 字符串）；
- plan 仍只有一份，由共享 `FaceEngine` 的 `Arc<LoadedPlan>` 持有（`Arc` 克隆不复制权重）；
- `snapshot` 不读权重文件、不缓存权重字节；
- 唯一新增的临时分配是 `run()` 里 `input.data.clone()`（1.2 M f32 ≈ 4.9 MB / 次），
  与既有实现的输入 Vec 同量级；
- `inspect()` 的 `declarations` 是每 IO 一条短字符串（YuNet 13 条），随返回值一起释放；
- `TensorSpec.shape` 的 `Cow` 让登记表保持 `Borrowed`（零堆分配），
  只有 `inspect()` / 用户模型的路径才用 `Owned`。

**输入侧对齐**：`sha256_file` 9.6 ms 是「首次加载一次」，有 `is_loaded()` 提前返回，
不会出现在每张照片的路径上。

---

## 9. 未完成事项（本轮明确不做）

| 项 | 原因 |
|----|------|
| R2 模型重复加载（`YunetProvider` 私有 `FaceEngine`） | 任务书 §6 明确「只记录不修」，尤其点名「模型重复加载」 |
| R3 `model_hub/inference.rs` 每次调用重解析 | 同上，留 R12 |
| R4 缓存键不含模型身份 | 同上，留 R16（专业精修的前置条件） |
| R10 项目文件不记录模型身份 | 同上 |
| R11 撤销粒度（一键美肤不可拆到单脸） | 同上 |
| 皮肤 / 关键点 / 分割的真实推理 | R12–R16 范围 |
| `resources/models/skin/**` 入打包列表 | 主理人裁决 Q3：无权重可打包 |
| Windows `resource_dir` 第二候选实测 | 主理人裁决 Q5：`locate()` 已接受候选列表，上 Windows 只改命令层一处 |
| GUI 人工验收 | 见第 7 节 |

---

## 10. 已知问题（新发现，只记录不修）

### 10.1 本轮引入的技术债

1. **manifest 读取有两份实现**：`Registry::user_models()` 直接读
   `<root>/<id>/manifest.json`，而没有走 `model_hub::storage::ModelHubStore::load()`。
   原因：`load()` 会对权重做流式 SHA256，放进面板挂载路径就是新的 R1。
   代价是 manifest 字段契约有两处消费点（`deserialize` 的仍是同一个
   `ModelManifest` 结构，所以字段漂移会编译期报错，风险可控）。
   **建议下一轮**：给 `ModelHubStore` 增加 `list_metadata()`（不哈希）并让 registry 转调。
2. **运行时状态有两处**：`Registry` 记元数据状态，`FaceEngine` 记 plan 是否在内存。
   `model_registry_snapshot` 由命令层把 `engine.is_loaded()` 回填成 `state: "ready"`。
   这是「registry 不持有 plan（避免第二份 tract plan）」的必然结果，但读代码的人
   容易误以为 registry 知道全部真相。
3. **`skin_segmentation::model` 出现双真相残留**：`status()` 已转发 registry，但
   `REGISTERED_SKIN_MODELS`（空表）、`verify()`、`locate()`、`is_commercial_license()`
   仍保留，只被 4 条既有单测使用。因为它们是 `pub`，编译器不会给 dead_code 警告。
   **建议**：R13 接入真实皮肤权重时，把这些函数删掉或改成 registry 的薄封装。
4. **热路径与 check API 的契约校验深度不同**：`FaceEngine::load` 把第 ④ 步推迟到
   `CpuTractBackend::load`（解析后校验），`model_registry_check` 在准入链内就地校验。
   可观测语义一致（都产出 `E_MODEL_VERSION`），但避免了 232 KB 权重被解析两遍。
   代价是「顺序」在代码里不是字面连续的，已在 `verify.rs` 与本文写明。
5. **用户自备模型在 registry 里恒为 `verified`**（只要 `present && validation_ok`），
   不重新做哈希复核。真复核要读最多 512 MB 权重，属 R4/R3 同类问题。
6. **`Registry::check*` 的 spec 参数已从 `&'static ModelSpec` 放宽为 `&ModelSpec`**：
   因为 `ModelSpec` 不再是 `Copy`（D11），`&builtin::YUNET` 拿到的是语句级临时借用，
   传不进 `&'static` 形参。`Registry` 本来就只读取 spec（`id`/`sha256` 都拷成 `String`），
   放宽后语义不变，但也意味着"注册表只接受静态登记项"这个约束现在靠**约定**而不是类型系统。
7. **`release` profile `panic = "abort"`** 使 `CpuTractBackend::run` 的
   `catch_unwind` 在正式包里失效（只在 dev/test 生效）。已在代码注释标注；
   若要在正式包也隔离 panic，需要改 profile，属全局决策，本轮不动。

### 10.2 沿用设计 §6 的历史风险（R1–R13，本轮只记录）

- **R1** `skin_model_status` 同步命令整文件哈希：本轮把命令改成
  `async + spawn_blocking`（T05），且 `status()` 已改为转发 registry（当前登记表为空，
  不产生任何 IO）。**但风险只是被绕开，没有根治** —— 一旦皮肤登记表有内容，
  `verify::check(spec, dirs, false)` 仍会做 SHA；届时必须改用 `check_shallow` 或落盘缓存。
- **R2** 同一模型被 2–3 个 `FaceEngine` 各自解析：未修（任务书点名只记录）。
- **R4** 缓存键不含模型身份：未修。
- **R5** 内存峰值（24 MP + 5 脸 ≈ 480 MB）：未修。
- **R6** 错误被吞：部分缓解 —— `FaceModelStatus.error` 现在真的会填（缺失/体积/格式）；
  `preview_skin_mask` 不再静默丢脸（`MaskPreview.warnings`）。`AppError` 仍是字符串。
- **R7** 错误文案错配：`InferenceError → AppError` 走 `Task` 而不是 `Unsupported`，
  不再出现「不支持的图片格式：人脸模型 SHA256 校验失败」。但旧路径
  （`commands/faces.rs`、`commands/beauty.rs` 自己的 `AppError::Unsupported`）未改。
- **R8** `resources/models/skin/**` 不在打包列表：按 Q3 不改，已进台账。
- **R9** 常量与实现量纲：**已修正为单一真相**（见 11.1 的偏离说明）。
- **R10 / R11 / R12 / R13**：未修（R12 的「预留可取消签名」已通过
  `detect_with_cancel` 完成）。

---

## 11. 对架构师设计的偏离（必读）

### 11.1 【重要】`value_range` 实现为 `Raw255` 而不是 `ZeroOne`

设计 §6 R9 与任务书都写：「常量写 “BGR 0..255”，实现是 `/257.0`（实际 0..1），
用 `ValueRange::ZeroOne` 表达」。**这个前提不成立，我按事实实现为 `Raw255`。**

证据（双重）：

1. **类型证据**：`src-tauri/src/image_engine/buffer.rs:10`
   `pub type Image16 = ImageBuffer<Rgba<u16>, Vec<u16>>;` —— 像素是 **u16**（0..65535）。
   `ai/face.rs` 的 `value[2] as f32 / 257.0`，而 `u16::MAX / 257.0 == 255.0`。
   即 **0..65535 / 257 → 0..255**，与常量写的「BGR 0..255」完全一致。
   只有当像素是 u8 时 `/257.0` 才是 0..1，但这里不是。
2. **行为反证**：R9 说「若输入量纲错，YuNet 分数会 <0.01」。若实际输入真是 0..1
   而模型期望 0..255，那么 `tests/face_detection.rs` 的
   `assert!(face.score > 0.6)` 必然失败；该测试本轮前后**都通过**
   （`portrait.jpg` 检测到 1 张脸，`group.jpg` 4 张）。

若按设计的 `ZeroOne` 实现，未来任何「按契约做的通用预处理」都会把 0..255 的
输入再归一化一次（或误以为无需放大），**恰好制造 R9 想避免的那个 255 倍误差**。
因此：
- `builtin::YUNET.input.value_range = ValueRange::Raw255`；
- `ai/model.rs::MODEL_INPUT` 仍是 `"1×3×640×640 float32（BGR，0..255）"`，
  但**由单测 `model_input_matches_the_structured_spec` 强制等于
  `YUNET.input.format_human()`**，任何量纲漂移立刻变红；
- `tests/model_registry.rs::yunet_value_range_is_raw255_consistent_with_the_implementation`
  与 `inference/tensor.rs::raw255_is_the_yunet_scale_for_u16_pixels` 双重锁定
  `u16::MAX / 257.0 == 255.0`。

`ValueRange::ZeroOne` 枚举仍然保留在 `tensor.rs` 里（未来的 RGB 0..1 模型要用），
只是 YuNet 不登记它。

### 11.1.1 设计更新（1247 → 1444 行）三条更正的落地情况

| # | 更正要求 | 落地 |
|---|----------|------|
| ① | `InferenceBackend` 必须含 `inspect()` | **已加**。trait 现在 5 个方法（`id` / `capabilities` / `inspect` / `load` / `run` / `unload`）。`CpuTractBackend::inspect` 用 `proto_model_for_path` 取 `ir_version`/`opset`，再 `into_typed` 取形状；`GpuBackend::inspect` 返回 `E_BACKEND`；`inspect_contract` 改为它的薄封装（全仓唯一 tract 解析入口）。验收：单测 3 条 + `gpu_backend_rejects_inspect_with_backend_code` + 集成测 `inspect_covers_the_l2_l3_read_only_shape` |
| ② | `tensor.rs` 是游离文件，接进模块树 | **提交 `ff91cee` 时已经接好**，此前提基于中间态快照：`src/ai_engine/inference/mod.rs` 存在，`src-tauri/src/ai_engine/mod.rs:7` 为 `pub mod inference;`，其中 `pub mod tensor;` 参与编译（470 条单测里包含 `inference::tensor::tests` 的 6 条）。本轮按新版 §8.2 修订了它的公开签名（见 D11） |
| ③ | SHA256 走流式，不许整文件读入 | **原本就是**。`model_registry/verify.rs:13` 只 `use ...::model_hub::security::sha256_file`（1 MB 缓冲流式），`:78` 是唯一调用点，全仓新代码零 `sha256_hex` 调用（`ai/model.rs::sha256_hex` 保留给既有 `skin_segmentation` 与它的单测，未动）。没有引入第 7 个哈希实现 |

**架构师已在设计文档 §14 复核并采纳本项**：R9 更正为「本质不是写错，而是契约以人读字符串表达、无法机器校验」，并确认
`ai/face.rs` 的 `u16 / 257.0` **实际产生 0..255，与常量一致**。因此 `Raw255` 是正确登记值，不是偏离遗留。

### 11.3 架构师复核（设计 1444 → 1495 行，§14）的落地情况

| §14 项 | 裁决 | 本轮落地 |
|--------|------|----------|
| D6 ④↔⑤ 可交换 | 可接受（附条件 ①②③） | ① 条件已满足：`verify::check_at(.., include_contract = true)` 保持完整顺序（`model_registry_check` 走的就是它）；② 已把"两码必须同处置"写进 `src/types.ts` 的 `ModelErrorCode` 上方注释；③ 顺序说明已在 `verify.rs` 与本文 §3.1 写明 |
| D7 `status()` 用浅检查 | 可接受，但 `error` 前端未渲染 | 记录为下一轮（§14.3-1）。本轮**不动** `FacePanel.tsx`：任务书要求"文案不得回归"，而渲染新错误串属于可见文案变更，应由产品确认后再做 |
| D1/D2 `Vec<TensorBatch>` + `TensorSpec` 合并 | 可接受，**唯一要求**：配 `is_placeholder()` 或显式拒绝全 0 形状 | **已实现**：`TensorSpec::is_placeholder()` + `TensorSpec::validate()` 首行显式拒绝（返回 `E_MODEL_CONTRACT`），单测 `placeholder_shape_is_rejected_instead_of_silently_passing`；`snapshot.rs` 的用户模型占位形状也加了断言 `view.input.is_placeholder()` |
| D3 `ModelOrigin` 去 payload | 结果可接受，**但理由不成立** | **已更正**：`spec.rs` 的注释改为真实理由——用户模型 ID 直接取自 manifest，**没有信息可丢**（避免一个 ID 两个真相）；并显式写了"**不要**把理由写成'带 payload 的枚举无法进 const'，那是错的" |
| D8 未改 `providers/yunet.rs` | 可接受，遵守正确 | 无动作 |
| D4 / D5 / D9 / D10 | 可接受 | 无动作。D10 的"note 必须写明 sha256 未本机复核"已在 `LicenseView::user_unverified` 的 note 里；`state` 也确实不会标 `verified`（未通过隔离验证 → `loadFailed`） |

### 11.2 其它偏离（都是为了让设计能编译/能正确表达）

| # | 设计写法 | 实现 | 原因 |
|---|----------|------|------|
| D1 | `InferenceBackend::run -> Result<TensorBatch, _>` | `-> Result<Vec<TensorBatch>, _>` | YuNet 有 **12 个输出 head**，单个 `TensorBatch` 无法表达多 head 契约 |
| D2 | `ModelSpec { input: ModelIo, output: ModelIo }`（`ModelIo` 内含 outputs） | `input: TensorSpec` + `outputs: &'static [OutputHead]` | `ModelIo` 与 `TensorSpec` 字段完全重复。`outputs` 用 `&'static [OutputHead]`（声明期形状一律静态）；`TensorSpec.shape` 用 `Cow<'static, [usize]>`，见 D11 |
| D3 | `ModelOrigin::UserImported { manifest_id: String }` | `ModelOrigin::UserImported`（无 payload） | 带 `String` 的枚举无法在 `const` 里构造；用户模型 ID 由 manifest 直接提供 |
| D4 | `RegistrySnapshot` 只有 `models` + `summary{total,ready,verified,blocked}` | 追加 `models[].official` 与 `summary.{official,userImported}` | 主理人裁决 Q1 要求「明确不进入正式注册清单」，需要一个显式布尔位 + 独立计数，否则 `total` 会被用户模型污染 |
| D5 | `model_registry_snapshot` 是同步命令 | `async` + `spawn_blocking` | 任务书 T04 验收点明确要求「4 个命令全部 spawn_blocking，唯一例外 model_unload」 |
| D6 | 契约判定（第 ④ 步）在准入链内就地做 | 热路径（`FaceEngine::load`）推迟到 `CpuTractBackend::load`；`model_registry_check` 就地做 | 否则 232 KB 权重会被解析两遍（实测 parse-only 19.2 ms，正式包虽快但没必要）。可观测语义一致（都是 `E_MODEL_VERSION`），已在代码注释与 §10.1-4 记录 |
| D7 | `FaceModelStatus.error` 从 `registry::snapshot()` 取 | 用 `verify::check_shallow`（①②③，不读全文件）现算 | `face_model_status` 是**同步**命令，走完整 `snapshot()`（含用户 manifest 读取）反而更贵；shallow 实测 0.109 ms，且真正解决了 R6-② |
| D8 | T03「改 `providers/yunet.rs:33` 注入共享 `FaceEngine`」 | **未改** | 任务书 §6 明确点名「模型重复加载」只记录不修；改它要动 `FaceIntelligence::new` 与 `reload_models()`，属 R2 范围 |
| D9 | `MaskPreview` 加「可选字段或复用 method」 | 加 `warnings: Vec<String>`（`skip_serializing_if` 空）+ `types.ts` 可选字段 | 选「可选字段」这条；`method` 是给 UI 展示当前蒙版算法用的，塞错误会破坏语义 |
| D10 | `snapshot.rs` 里 `sha256` 取自 spec | 一致，但**用户模型**的 `sha256` 取自 manifest 的声明值 | 用户模型不做哈希复核（见 §10.1-5），字段名与语义在 `license.note` 里说明 |
| D11 | `TensorSpec.shape: Vec<usize>` | `TensorSpec.shape: Cow<'static, [usize]>`（`TensorSpec::declared(&'static [usize], ..)` 走 `Borrowed`，`TensorSpec::observed(Vec<usize>, ..)` 走 `Owned`） | **纯 `Vec<usize>` 无法在 const 里构造**：`const YUNET` 需要 `shape: Cow::Borrowed(&[..])`（零堆分配），而 `inspect()` 必须能表达任意 ONNX 文件的动态形状（`Cow::Owned`）。`Cow` 同时满足两者，因此"声明契约"与"观测契约"仍是同一个类型，没有分裂成两套结构。代价：`TensorSpec` / `ModelSpec` 不再是 `Copy`（`bytes` 级 clone 出现在 3 处：`LoadedPlan.input_spec`、`TensorBatch.spec`、`ModelEntryView.input`） |
| D13 | （§14.3-2 建议）加 `is_placeholder()` | 已加：`is_placeholder()` = 形状全 0 / 空形状；`validate()` 首行拒绝占位契约（`E_MODEL_CONTRACT`） | 全 0 形状的 `element_count() == 0`，若不拦，"空数据"会静默通过元素数校验。本项原标"下一轮"，因为只需几行且能关掉一个正确性口子，本轮直接做掉 |
| D12 | （设计未涉及）`ai/model.rs` 从 `YUNET.file` 派生的写法 | 字面量提成 `builtin::YUNET_FILE` / `YUNET_SHA256` / … 等独立 `const`，`ModelSpec` 与 `MODEL_*` 都引用它们 | 由 D11 派生出的硬约束：`const X: &str = CONST_ITEM.field;` 对含 `Cow` 的 const 会报 `E0493: destructor of ModelSpec cannot be evaluated at compile-time`（求值出的临时量需要在常量期 drop）。提成独立 `const` 后，「单一真相」与「const 链」同时保住，且 `MODEL_*` 仍是编译期常量（`commands/faces.rs:22` 的 `resources.join(..).join(MODEL_FILE)` 无需改） |

---

## 12. Git commit SHA 与工作区状态

提交序列（每次都在提交后重跑全量验证）：

| # | SHA | 内容 |
|---|-----|------|
| 1 | `ff91cee6114d162c532da87082926715737faa2f` | `feat(r11): 统一模型注册表与推理准入链`（29 files, +7181 / -116） |
| 2 | `945bd46` | `docs(r11): 回填本轮代码提交 SHA` |
| 3 | 见 `git log -1` | `feat(r11): 补 InferenceBackend::inspect 与 TensorSpec 动态形状`（设计 1247→1444 的三条更正） |
- 提交范围（**未用 `git add -A`**）：
  `src-tauri/src`、`src-tauri/tests`、`src/lib`、`src/types.ts`、
  `docs/R11_AUDIT_AND_DESIGN.md`、`docs/MODEL_LICENSE_REGISTER.md`、`docs/R11_ROUND_REPORT.md`
- 未提交（保持未跟踪）：`docs/REVIEW_PERF_SECURITY.md`、`docs/UX_EXPERIENCE_AUDIT_2026-11-22.md`、`.workbuddy/`
- `src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`：**本轮无改动**，因此不在提交列表里。

---

## 13. 下一轮建议（按优先级）

1. **R13 皮肤解析接入**：先补 `builtin::SKIN_MODELS` 一条通过许可审查的权重
   （四字段全 Allowed，训练数据条款必须先核实），并**同步 `tauri.conf.json`**。
   接之前先按 §10.2-R1 把 `status()` 的校验换成 `check_shallow` 或落盘缓存，
   否则面板挂载会卡主线程。
2. **R12 关键点稠密化**：registry 已支持 `FaceLandmark` 任务与多 head 契约，
   只需登记 + 后处理实现；`CpuTractBackend` 的 contract 校验会自动拦住契约不符的权重。
3. **收敛 §10.1-1**：给 `ModelHubStore` 加 `list_metadata()`（不哈希），
   让 `Registry::user_models()` 转调，消除两份 manifest 读取。
4. **R4 缓存键含模型身份**：`FaceRepository` / `AnalysisCache` 的键加
   `(model_id, model_sha256)`；这是 R16 专业精修的前置条件，也是 registry 存在的最大价值兑现点。
5. **R2 plan 复用**：`YunetProvider` 接收共享 `FaceEngine`（`Arc` 克隆）而非
   `FaceEngine::default()`；registry 的 `(id, sha256)` 键可以直接给 plan 缓存用。
6. **手工 GUI 验收**（§7 留项）：打包后跑一遍 `skinModelStatus` 挂载、蒙版显示、
   批量导出、项目保存。
