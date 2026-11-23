# 皮肤语义分割模型接入设计（合法权重 + tract 实证 + 逐文件改造）

> 角色：架构设计 + 技术实证（只读，不改 `src/`、`src-tauri/src/` 任何代码，不改其他 docs）。
> 调研日期：2026-11-23 · 基线提交：`88e949a` · 运行时：`tract-onnx 0.23.10`（`Cargo.lock:5365`，`Cargo.toml:31` 声明 `0.23`）。
> 本文是**新交付文档**；`docs/BEAUTY_ROOT_CAUSE_AUDIT.md`、`docs/MODEL_LICENSE_REGISTER.md` 等既有文档未改动。

---

## 0. 结论先行（一句话）

**唯一首选：MediaPipe Selfie Multiclass 256×256（TFLite → ONNX opset 13）**，它是三个候选中唯一同时满足「模型卡明示 Apache-2.0 → 权重/商用/再分发三字段全干净」「直接给出 `face-skin`/`body-skin` 皮肤语义」「**已实测** tract 0.23.10 完整加载+推理成功」的方案；**但其训练数据许可未被官方披露（Unverified）**——按本项目现行闸门（`license.rs:141-146`：训练数据仅披露、不阻塞）可与 YuNet 同口径随包分发并如实披露；若主理人坚持「四字段**全部 Allowed** 才叫全干净」，则严格结论是**当前没有四字段全干净的可用权重 → 不可接入**，缺口 = 训练数据许可声明（见 §5）。

| 候选 | 四字段判定 | 结论 |
|---|---|---|
| **A. MediaPipe Selfie Multiclass 256×256** | 权重 Allowed / 商用 Allowed / 再分发 Allowed / 训练数据 **Unverified** | **唯一首选**（门禁三字段过、语义最贴合、tract 实测可跑） |
| B. MODNet | 权重 Allowed / 商用 Allowed（代码层）/ 再分发 Allowed / 训练数据 **NotAllowed**（PPM-100 alpha 为 CC BY-NC-SA 4.0） | **备选但不推荐商用**：训练数据明确非商用，且输出是人物 alpha 非皮肤语义、权重约 25 MB |
| C. MediaPipe Selfie Segmentation landscape | 权重 Allowed / 商用 Allowed / 再分发 Allowed / 训练数据 Unverified | 仅「人 vs 背景」二类，无法定位脸部皮肤 → 语义不足，不作首选 |

**绝不伪造**：在权重真正登记并通过许可/SHA/契约、且人工核对训练数据缺口之前，UI 文案必须继续沿用现有 `method="landmark-geometry"`（几何皮肤区域），不得声称 AI 语义分割。

---

## 1. 许可证实证（不只看 README 一句话）

### 1.1 方法与证据规则

- 对每个候选分别核对：① 官方仓库 `LICENSE` **原文**（WebFetch raw 文件）；② 权重/模型卡**自身**的许可声明；③ 训练数据来源与许可；④ 商用与再分发条款。
- 严格区分三件事（项目铁律，`license.rs:3-8`、`MODEL_LICENSE_REGISTER.md:10-20`）：**代码 Apache-2.0 ≠ 权重可商用 ≠ 训练数据可再分发**。
- 所有结论给出可引用出处（仓库 URL + 文件路径），访问日期 2026-11-23。

### 1.2 四字段证据表

| 字段 | A. MediaPipe Selfie Multiclass | B. MODNet | C. Selfie Segmentation landscape |
|---|---|---|---|
| **权重许可** | **Allowed**（模型卡 PDF 原文 `LICENSED UNDER Apache License, Version 2.0`；文件 SHA256 `59372fce…4a9e6` 已本地核验） | **Allowed**（README `License` 原文：*"The code, models, and demos in this repository … are released under the Apache License 2.0 license."*；`LICENSE` 全文 Apache-2.0） | **Allowed**（MediaPipe 模型卡 Apache-2.0；repo `LICENSE` 全文 Apache-2.0） |
| **训练数据许可** | **Unverified** ⚠️：模型卡只写 `Model is trained on thousands of samples with diverse identities …`，**未命名数据集、未声明许可**；TFLite 内嵌元数据无 `license`/数据集字符串（本地 `strings` 检查为空） | **NotAllowed** ⚠️：论文用 Adobe Matting Dataset + PPM-100；PPM 仓库 `README.md#license` 原文 *"All annotated alpha mattes in PPM are released under the Creative Commons Attribution NonCommercial ShareAlike 4.0 license."*；且注明部分图像因许可被替换 | **Unverified**（同上，官方未披露训练数据许可） |
| **商业使用** | **Allowed**：Apache-2.0 §2 明示 `use … sell`；模型卡 `Intended Uses` 含 AR/视频会议/娱乐，未含商用禁止 | 权重层面 Allowed（Apache-2.0）；**但训练数据 NC 条款使整体商用不可推荐**（法务风险） | **Allowed**（同 A） |
| **再分发** | **Allowed**：Apache-2.0 §4，需保留版权/许可/NOTICE；第三方已按此随包分发并附 `LICENSE-selfie-multiclass`（全文 Apache-2.0） | **Allowed**（Apache-2.0 §4），但训练数据 NC 不改变「权重可再分发」这一事实，也不消除整体风险 | **Allowed**（同 A） |
| **SPDX** | `Apache-2.0` | `Apache-2.0` | `Apache-2.0` |
| **门禁结论**（`license.rs:181-210`） | `weights`/`commercialUse`/`redistribution` 三项均 Allowed → **放行**；`trainingData` Unverified → 进入 `unverifiedFields` 披露 | 权重三项可放行，但训练数据 NotAllowed → 法务不应按「干净」登记 | 门禁放行，但语义不足 |

### 1.3 可引用出处（URL + 文件路径）

**A. MediaPipe Selfie Multiclass 256×256**
- 官方仓库：`https://github.com/google-ai-edge/mediapipe`；仓库 `LICENSE` 原文：`https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/LICENSE`（Apache-2.0 全文，末尾含 `tasks/cc/text/.../utf/` 的 Rob Pike & Ken Thompson 例外条款；该例外不覆盖皮肤模型）。
- **模型卡（关键证据）**：`https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf`
  - 本地下载 `SHA256 = 59372fce0666d4c75b3877540fa77f80ac4b2a0f75e914711417f85fc490a9e6`，1,378,158 B。
  - 原文摘句：`LICENSED UNDER / Apache License, Version 2.0`；作者 `Adel Ahmadyan, Google`；日期 `May 10, 2023`；类别附录 `1 background / 2 hair / 3 body-skin / 4 face-skin / 5 clothes / 6 others`。
  - 训练数据相关原文（第 2 页 `Factors and Subgroups → ENVIRONMENTS`）：`Model is trained on thousands of samples with diverse identities (i.e. unique subjects), various blendshape combinations representing both common and random expressions and transformations (e.g. rotations).` —— **仅此一句，无数据集名称/许可**。
  - 边界原文（第 2 页 Limitations）：`may not provide pixel perfect masks`；`degrading the environment light, backlit subjects, adding noise, or fast motions … degradation of quality`。
- 第三方合规再分发示例（证明可行路径）：`https://github.com/16sakuraa/PortraitColorLab/blob/main/THIRD_PARTY_NOTICES.md`，条目原文：`*/models/selfie_multiclass_256x256.onnx … Modified: converted from TFLite to ONNX with tf2onnx 1.16.1 at opset 13 … Apache 2.0 (web/models/LICENSE-selfie-multiclass …)`；随包许可原文文件 `web/models/LICENSE-selfie-multiclass`（Apache-2.0 全文）。

**B. MODNet**
- 官方仓库：`https://github.com/ZHKKKe/MODNet`；`LICENSE`：`https://raw.githubusercontent.com/ZHKKKe/MODNet/master/LICENSE`（Apache-2.0 全文）；`README.md#license` 原文如上（明确覆盖 `models`）。
- `onnx/README.md`：`https://github.com/ZHKKKe/MODNet/tree/master/onnx` —— ONNX 版本由社区贡献者 `@manthan3C273` 提供，官方仅给 Google Drive 直链，仓库内**没有**预编译 ONNX。
- 训练数据证据：`https://arxiv.org/abs/2011.11961`（论文，摘要含 `Adobe Matting Dataset` 与 `PPM-100`）；`https://github.com/ZHKKKe/PPM` 的 `README.md#license`（PPM alpha 为 CC BY-NC-SA 4.0）。

**C. MediaPipe Selfie Segmentation landscape**
- 同 A 的 repo `LICENSE`；模型方案文档 `https://developers.google.com/mediapipe/solutions/vision/selfie_segmentation`（输入 `144×256×3`，输出 `256×256×1`，二类：人/背景）。仅用于背景排除，不进入首选。

---

## 2. 权重可获得性与 tract 0.23 兼容性实证

### 2.1 权重链接、格式、SHA256（本地下载核验）

| 项 | 值 |
|---|---|
| 官方 TFLite 直链 | `https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/1/selfie_multiclass_256x256.tflite` |
| 格式 / 精度 | TFLite / float32（官方仅 TFLite，**无官方 ONNX**，必须转换） |
| 体积 | 16,371,837 B（本地实测） |
| SHA256 | `c6748b1253a99067ef71f7e26ca71096cd449baefa8f101900ea23016507e0e0`（本地实测，与 HF `litert-community/MediaPipe-ImageSegmenter-Selfie-Multiclass` 公示值一致） |
| 可用 ONNX（转换产物） | 来源 `https://github.com/16sakuraa/PortraitColorLab`，路径 `web/models/selfie_multiclass_256x256.onnx`；体积 **16,453,400 B**；**SHA256 = `485774fe148451ca3f6c53626654b9a5ea34d291fa07ddac53725aa35ec07587`**（本地实测） |
| 转换口径（第三方声明） | tf2onnx 1.16.1，opset 13，NCHW 输入/输出，3 个 Resize 节点坐标模式由 `tf_half_pixel_for_nn` 改为 `asymmetric`；声明与 TFLite 输出误差 ≤ 0.0001，**本轮未独立复现该数值一致性** |

> 注：ONNX 是第三方从官方 TFLite 转换而来、并随包附 Apache-2.0 许可文件。随包前应由我方**从官方 TFLite 自行重新转换**并对转换脚本与产物做可复现构建，避免直接信任第三方二进制；本轮因 Python 3.13 环境无可用的 `tensorflow-cpu` wheel，未在本机重跑 tf2onnx 转换（标为**待实测**），但已对现成 ONNX 做了完整结构解析与 tract 实测。

### 2.2 输入/输出契约与预处理口径

- **输入**：`input_29`，`F32`，shape `[1,3,256,256]`（NCHW，RGB），归一化 `mean=127.5, std=127.5` → 实际送值域 `[-1,1]`（`(x - 127.5)/127.5`，等价于 `2·(x/255) - 1`）。现有 `ValueRange` 已有现成变体 **`MinusOneToOne`**（`inference/tensor.rs:75-76`），且其 `apply(unit) = unit*2.0 - 1.0`（`tensor.rs:105`）正好实现该预处理，登记时直接用它即可；**不可**图省事登记成 `ZeroOne`/`Raw255`——预处理实际送值与登记口径不一致会让输出整体失真，且 `TensorSpec::validate` 会按登记值域拒绝（参见 `builtin.rs:54-59` 对 YuNet 值域的同类教训）。
- **输出**：`Identity`，`F32`，shape `[1,6,256,256]`（NCHW）。0-based 类别：`0 background / 1 hair / 2 body-skin / 3 face-skin / 4 clothes / 5 others`。
- **关键陷阱（已实测）**：本 ONNX 的最后一层是 `Conv`，**输出是 logits 而非概率**——`to_plain_array_view::<f32>()` 实测 `min=-1.4928, max=3.0486`。图内 6 个 `Softmax` 全部位于 ViT 线性注意力内部（axis 缺省），**不是类别维 softmax**。因此 SkinParse 后处理**必须自行沿 class 轴（NCHW 下 axis=1）做 softmax**，再取 `face-skin(3)+body-skin(2)` 通道作为皮肤置信度；否则会产生“负数当概率”的错误蒙版。
- **输出空间映射**：模型对整幅（裁剪后）256×256 图输出同分辨率 6 通道；接入时需把 `[256,256]` 概率图按人脸包围盒**双线性上采样**回局部坐标，再与几何蒙版融合。

### 2.3 转换后 ONNX 算子清单与 tract 支持标注

Python `onnx` 1.23.2 解析现成 ONNX 结果：`ir_version=7`，`opset_import=[('',13), ('ai.onnx.ml',2)]`，实际使用 domain 仅 `''`，**232 个节点，10 种算子**：

| 算子 | 数量 | tract 0.23.10 支持 | 依据 |
|---|---|---|---|
| `Conv`（含 depthwise `group=32`） | 85 | **支持（已实测）** | 全图编译+推理通过，depthwise 分组卷积被正确执行 |
| `Relu` | 31 | **支持（已实测）** | 同上 |
| `Transpose` | 34 | **支持（已实测）** | 同上（NCHW↔NHWC 转换由 tf2onnx 显式插入） |
| `Reshape` | 20 | **支持（已实测）** | 同上 |
| `Add` | 25 | **支持（已实测）** | 同上 |
| `Mul` | 18 | **支持（已实测）** | 同上 |
| `ReduceSum` | 6 | **支持（已实测）** | 同上（attention 的逐行归一化） |
| `Softmax` | 6 | **支持（已实测）** | 同上（attention 内部，axis 缺省） |
| `Resize` | 6 | **支持（已实测）** | 同上；含 `mode=linear/half_pixel` 与 `mode=nearest/asymmetric` 两类，坐标模式均被 tract 0.23.10 接受 |
| `ConvTranspose` | 1 | **支持（已实测）** | 同上（segmentation head 上采样，`strides=[2,2]`） |

**未出现的高风险算子**：`NonZero`、`Gather`、`Slice`、`Where`、`Loop/If`、`Scatter*`、`DynamicQuantizeLinear` —— 审计中担心的 `tf2onnx` 产物 `NonZero` 问题在本转换产物中**不存在**，这是该转换路径（opset 13 + 显式 NCHW）的关键优势。

### 2.4 tract 实测记录（明确标注）

- **环境**：`/tmp/tract-probe`（**不在仓库内**），`Cargo.toml` 固定 `tract-onnx = "=0.23.10"`，`cargo run --offline`，2026-11-23。
- **动作**：`onnx().model_for_path()` → `into_typed()` → 读取输入/输出 shape 与 dtype → `into_optimized()` → `into_runnable()` → 喂入 `[1,3,256,256]` F32 合成张量 → `run()`。
- **输出（原文摘录）**：
  ```
  [probe] input[0] shape=Some([1, 3, 256, 256]) dtype=F32
  [probe] output[0] shape=Some([1, 6, 256, 256]) dtype=F32
  [probe] into_optimized + into_runnable OK
  [probe] ran output[0] shape=[1, 6, 256, 256] min=-1.4928 max=3.0486
  [probe] RESULT: FULL INFERENCE OK
  ```
- **已实测（本轮）**：ONNX 文件可被 tract 0.23.10 解析、类型推导、优化、编译、执行；输入输出形状与声明一致；上表 10 种算子全部跑通；输出为 logits（需外部 softmax）。
- **待实测（本轮未做，不得写成已验证）**：
  1. 转换产物与原 TFLite 的**数值一致性**（第三方称 ≤0.0001，未独立复现）；
  2. 真实人像上的**精度/观感**（正脸、侧脸、深肤色、暗光、遮挡）与耗时；
  3. 我方从官方 TFLite **自行重跑 tf2onnx** 的可复现构建；
  4. 打包后 `resource_dir()` 下的真实路径定位与 `inspect_contract` 准入链回归。
- 注意：本轮**未**在应用代码路径（`CpuTractBackend::load/run`）内实测，仅用同版本 tract 独立探针验证运行时可行性；接入后必须补 `src-tauri/tests/` 集成测试。

---

## 3. 接入设计（可执行任务粒度，逐文件改动清单）

> 原则：保持 `build_skin_mask` 的签名与所有调用点**不变**（`commands/beauty.rs:113,189`、`image_engine/pipeline.rs:290`），把“几何 or 模型”的分派封装在 `skin_segmentation` 模块内部，避免大范围改动调用链。登记表的**唯一真相**是 `model_registry::builtin`，皮肤专用 `RegisteredSkinModel` 旧结构（`model.rs:16-33`）不再维护第二份真相，仅保留 `SkinModelStatus` 作为前端视图。

### 3.0 R11 遗留（必须先修，否则主线程整文件哈希复活）

- `src-tauri/src/ai_engine/skin_segmentation/model.rs:63`：`verify::check(spec, &base_dirs, false)` —— `check()` 内部走 `check_at(..., include_contract=false)`，仍执行**第 ⑤ 步 SHA256 流式全文件哈希**（`verify.rs:86-108`）。该函数由 `commands/beauty.rs:55` 的 `skin_model_status` 在面板挂载时调用，**登记权重后会重新引入 R1 级主线程/阻塞线程整文件读盘问题**。
- 改法：新增登记条目后，把 `model.rs:63` 换成按路径的 `verify::check_shallow(spec, &resolved_path)`（`verify.rs:114-143`：仅许可 → 存在 → 体积/头，**不算哈希**），路径由 `locate::locate` / `verify::check` 的候选目录结果取得。SHA256 只在 `InferenceBackend::load()`（真正加载时、后台线程）由准入链完整校验一次。
- 顺序固定：**先改 `check_shallow`，再登记权重**。否则一旦 `SKIN_MODELS` 非空，`skin_model_status` 立即开始逐文件哈希。

### 3.1 登记层（`src-tauri/src/ai_engine/model_registry/builtin.rs`）

新增 `ModelSpec` 常量（例如 `SELFIE_SKIN`），字段取值：

| 字段 | 值 |
|---|---|
| `id` | `skin-parse-mediapipe-selfie-multiclass-256` |
| `task` | `ModelTask::SkinParse` |
| `format` / `origin` | `Onnx` / `Builtin` |
| `dir` / `file_name` | `models/skin` / `selfie_multiclass_256x256.onnx` |
| `size_bytes` / `sha256` | `16_453_400` / `485774fe148451ca3f6c53626654b9a5ea34d291fa07ddac53725aa35ec07587`（**我方自行转换后必须重算并覆盖此值**） |
| `input` | `TensorSpec::declared(&[1,3,256,256], ValueRange::MinusOneToOne, ChannelOrder::Rgb)`（`tensor.rs:75-76` 已有该变体，无需新增枚举） |
| `outputs` | 1 个 head：`{ name: "Identity", shape: &[1,6,256,256], semantic: "6 类逐像素 logits（含 face-skin/body-skin），需 axis=1 softmax" }` |
| `license` | `weights: Allowed` / `commercial_use: Allowed` / `redistribution: Allowed` / `training_data: Unverified{ what_to_check: "MediaPipe 模型卡未披露训练数据集名称与许可，需 Google/法务确认" }`；`evidence` 收录 §1.3 的模型卡 PDF + repo LICENSE + Apache-2.0 随包文件三条 |
| `provenance` | `weights_url` = TFLite 直链；`license_url` = 模型卡 PDF；`bundled_license_file = Some("src-tauri/resources/models/skin/LICENSE")` |
| 表 | `SKIN_MODELS = &[&SELFIE_SKIN]`；`ALL = &[&YUNET, &SELFIE_SKIN]`；`find/ids` 自动生效 |

配套：`tests` 中 `skin_and_segmentation_tables_stay_empty_until_a_weight_passes_review()`（`builtin.rs:216`）必须改为断言“皮肤表恰好 1 条且许可门禁通过、训练数据字段出现在 `unverified_fields()`”；`license.rs` 的门禁测试语义不变（训练数据非门禁字段）。

### 3.2 资源与打包

- 新增文件（实施阶段）：`src-tauri/resources/models/skin/selfie_multiclass_256x256.onnx`（16.45 MB）+ `src-tauri/resources/models/skin/LICENSE`（Apache-2.0 全文）+ 建议附 `CONVERSION.md`（记录 tf2onnx 版本、opset、Resize 坐标模式改写、从官方 TFLite 重算的 SHA）。
- `src-tauri/tauri.conf.json` 的 `bundle.resources` 增加 3 项：`resources/models/skin/selfie_multiclass_256x256.onnx`、`resources/models/skin/LICENSE`、`resources/models/skin/CONVERSION.md`。漏改的后果见 `MODEL_LICENSE_REGISTER.md:100-103`（R8：打包后 `locate()` 永远 `None`，静默回退）。
- `skin_segmentation/model.rs::locate()`（`:113-125`）的候选目录已含 `resources/models/skin`，**无需改动**；`skin_base_dirs()`（`:103-111`）保持。

### 3.3 许可台账同步（只写待办，本轮不改既有 docs）

实施时须同步 `docs/MODEL_LICENSE_REGISTER.md`：第 2 节新增「Selfie Multiclass 皮肤解析」条目（四字段 + Unverified 披露 + 证据链 + SHA + 体积）；第 3 节空表删除 `builtin::SKIN_MODELS` 行；第 5 节资源清单更新。本文不改该文件。

### 3.4 推理层（新增文件 `src-tauri/src/ai_engine/skin_segmentation/onnx.rs`）

建议新增独立模块，职责单一，避免污染几何蒙版代码：

1. `load(resource_dir) -> Option<LoadedPlan>`：`builtin::SELFIE_SKIN` → `locate` → `CpuTractBackend::load`（内部再验契约 + SHA，后台线程调用）；失败返回 `None` 并记录原因字符串。
2. `infer(plan, crop_rgb_u8, face_bbox) -> AppResult<SkinMask>`：
   - 以 `FaceGeometry` 包围盒取方形裁剪（建议 1.5–3× 脸框，全脸与少量上下文；model card/第三方模型卡建议），resize 至 256×256 双线性；
   - 归一化 `(x/127.5 - 1)` 生成 `[1,3,256,256]` F32，`ChannelOrder::Rgb`；
   - `CpuTractBackend::run` → `[1,6,256,256]` logits；
   - **沿 axis=1 做 softmax**（注意数值稳定：减 channel 最大值）；取 `p(face-skin=3) + p(body-skin=2)` 作为皮肤置信度（可自适应阈值或加权，需真实样张调参）；
   - 把 256×256 概率图按脸框映射回局部像素坐标，生成与几何蒙版同结构的 `SkinMask { x0,y0,width,height,alpha }`。
3. 与现有几何蒙版融合：建议 `alpha_final = max(alpha_model, alpha_geometry * gate)` 或 `alpha_model * (1-五官保护) + alpha_geometry * 五官保护`；**保留** `build_mask_from_geometry` 中的五官保护与高光裁切补偿作为安全网（模型对眼睛/嘴唇并非逐像素完美）。手动画笔/橡皮擦叠加与 `invert` 逻辑保持在 `build_mask_from_geometry` 尾部，模型分支调用同一套后处理（可抽成 `finalize_mask(alpha, geometry, manual, …)`）。
4. 性能：plan 用 `OnceLock`/`State` 缓存，避免每张脸重复 `load`；`max_batch=1`（`backend.rs` 能力声明），多脸串行调用并设置 `timeout`/`CancelToken`。推理在 `spawn_blocking` 内（调用点已是），且 `face.rs:10` 明确 tract 不暴露中断钩子，取消只在阶段边界生效——需在 UI 文案上如实说明。

### 3.5 分派与状态（改 `skin_segmentation/mod.rs`、`model.rs`）

- `build_skin_mask(image, face, manual)` 内部：若 `model::status()` 可用且 `onnx::load()` 成功 → 走模型分支；否则 → 原 `build_mask_from_geometry`。分派结果写入 `SkinModelStatus.method`（几何仍为 `landmark-geometry`；模型为 `onnx:skin-parse-mediapipe-selfie-multiclass-256`）。
- `model.rs::status()`（`:60-100`）：`verify::check` → `check_shallow`（见 §3.0）；已加载分支的 `license` 字段要写 `Apache-2.0` 且**附带 `trainingData: Unverified` 披露**，不可只写「已加载通过许可审查」而隐藏缺口。`reason` 文案建议：`"已加载 Selfie Multiclass 皮肤解析（训练数据许可未披露，待法务复核）"`。
- 回退路径的 `reason` 文案继续保留现有诚实说明（`model.rs:97`），不得在模型未真正加载时说“AI 分割”。

### 3.6 失败降级矩阵（不崩溃、如实说明）

| 失败场景 | 检测位置 | 行为 |
|---|---|---|
| 模型缺失 / 体积不符 / 非 ONNX 头 | `verify::check_shallow` | `blocking` 非空 → `status.available=false` → 几何蒙版；`reason` 带 searched 路径 |
| 许可未过（四字段被改坏） | `license::gate`（纯内存，先于文件 IO） | 直接 `LicenseNotCleared` → 几何蒙版；**不读文件字节** |
| SHA256 不符 / 解析失败 | `CpuTractBackend::load`（后台） | `load()` 返回 `None` → 几何蒙版 + warning；**不得**尝试“先跑起来再说” |
| 契约不符（输入/输出 shape 变） | `inspect_contract` / `check_contract` | `E_MODEL_VERSION` → 几何蒙版 + warning |
| 推理 panic | `cpu_tract.rs:364` 的 `catch_unwind` | 已隔离为 `E_INFER` → 几何蒙版 + warning；进程不挂 |
| 输出非概率/NaN | `onnx::infer` 的 softmax 前检查 | 拒绝本张脸 → 几何蒙版 + warning |
| 多人脸 / 大图超时 | `RunOptions.timeout` | 超时 → 几何蒙版 + warning；UI 不卡死 |
| 用户自备模型 | `ModelHub` | 永远 `UserUnverified`、`official=false`，不进 `SKIN_MODELS`，不参与默认美肤（既有规则不变） |

所有降级必须把失败原因写入 `SkinModelStatus.reason` 或 `BeautyAnalysis.warnings`（前端已有展示位，`commands/beauty.rs:70,163`），**禁止静默吞掉**。

### 3.7 验收清单（实施阶段）

1. `cargo test`（Rust 478+ 既有测试契约不回归）；新增 `builtin` 条目测试、`verify::check_shallow` 不读全文件测试（参考 `verify.rs:348-362`）、`inspect_contract` 对新模型形状断言 `[1,3,256,256] → [1,6,256,256]`。
2. 失败注入：删权重 / 改 SHA / 改许可证字段 / 塞非 ONNX 文件 → 全部回退几何蒙版且不崩溃，`warnings` 非空。
3. 像素级：同一人像在「几何」与「模型」两路径导出蒙版叠加图，断言模型路径的脸颊 ROI 覆盖提升、眼睛/嘴唇 ROI 仍受保护；深肤色/暗光样张单独跑（见 §4 边界）。
4. 契约：ONNX 输出经 axis=1 softmax 后每像素各类概率和 ≈ 1，皮肤通道无负值。
5. 打包：按 `tauri.conf.json` resources 全量打包后，`skin_model_status` 返回 `available=true`（浅检查）且实际推理成功；卸载权重后回退文案正确。

---

## 4. 风险清单（含覆盖边界）

1. **tract 兼容性边界**：本轮结论“已实测”仅覆盖 `/tmp/tract-probe` 的同版本 tract 0.23.10 对该**现成第三方 ONNX** 的加载/编译/推理；在应用 `CpuTractBackend::load/run` 路径、打包环境、我方自转产物上**仍待实测**。不得写成“应用内已验证”。
2. **转换精度损失**：第三方声明误差 ≤0.0001 未独立复现；`Resize` 坐标模式由 `tf_half_pixel_for_nn` 改为 `asymmetric` 是在 2× 尺度上“取到相同像素”的近似，其他尺度/边界可能有偏差；softmax 位置差异（TFLite 在任务库侧、ONNX 需我方做）是最大的语义一致性风险。
3. **预处理契约**：`[-1,1]` 有现成的 `ValueRange::MinusOneToOne`（`tensor.rs:75-76`）可用，但预处理必须**真的**按 `(x-127.5)/127.5` 送值并与登记口径一致；若误登记为 `ZeroOne`/`Raw255` 会导致输出整体失真或 `validate` 拒绝（参照 `builtin.rs:54-59` 对 YuNet 的同类教训）。
4. **覆盖边界（模型卡原文证据）**：
   - 深肤色：256×256 模型在 `Tone_9 + Tone_10`（Monk 量表最深）上 mean IoU **68.25**，全体 77.23；512 模型深色组 71.86 vs 全体 81.10——**深肤色明显退化**，需单独样张验证，不可宣称“全肤色一致”。
   - 暗光/背光/噪声/快速运动/大遮挡：模型卡 `Limitations` 明示 `degradation of quality`。
   - 侧脸/小脸/极端姿态：第三方模型卡总结 `small faces, extreme poses, occlusion and difficult lighting degrade results`；本项目几何蒙版本身也是 5 点正面几何，侧脸覆盖同样有限——两者都需真实样张边界测试。
   - 类别 `others` 是异构 accessory 集合，不应被当作皮肤。
5. **体积/打包**：ONNX 16.45 MB（约为 YuNet 232 KB 的 70 倍），会增大 dmg/nsis 安装包；需评估分发体积与首启加载耗时，且 `resources` 漏改会导致静默回退（R8）。
6. **训练数据缺口（合规）**：MediaPipe 模型卡不披露训练数据许可；若法务要求“四字段全干净”，则当前**不可接入**，缺口 = 官方训练数据来源/许可声明（见 §5）。
7. **R11 哈希回归**：`model.rs:63` 未改 `check_shallow` 前登记权重 → `skin_model_status` 每次面板挂载都做整文件哈希（R1 级问题复活）。必须先修再登记。
8. **多人脸性能**：plan 缓存与 `max_batch=1` 决定多脸为串行推理；tract 不支持中断正在执行的 `run`，超时/取消只能作用于阶段边界（`face.rs:10`）。

---

## 5. 「不可接入」判定与缺口（严格口径）

- 若判定标准是“**四个字段全部 Allowed**”：三个候选**都不满足**——A/C 训练数据 `Unverified`，B 训练数据 `NotAllowed`（PPM-100 alpha 为 CC BY-NC-SA 4.0）。此时结论是 **不可接入**，UI 必须继续显示几何方案，不得声称 AI 皮肤语义分割。
- 缺口清单（补齐任一即可重评）：
  1. 向 Google/法务取得 MediaPipe Selfie Multiclass 训练数据集名称与许可书面说明（当前模型卡只有 `thousands of samples` 一句）；
  2. 若改用 MODNet，需取得 Adobe Matting Dataset 与 PPM-100 的**商用授权**，否则其 CC BY-NC-SA 4.0 训练数据使商用发行不成立；
  3. 我方从官方 TFLite 自行重跑 tf2onnx 转换并固化脚本/SHA（当前依赖第三方产物）；
  4. 真实人像（含深肤色/暗光/侧脸）精度与性能基线。
- 若采用项目**现行闸门口径**（`license.rs:141-146`：训练数据仅披露、不阻塞，与 YuNet 先例一致），则 A 可作为唯一首选推进，但状态/UI 必须显著披露 `trainingData: Unverified`，禁止把“权重可商用”说成“整条数据链路已清”。

---

## 6. 交付与复现命令（供复核）

```bash
# 1) 官方权重 + 模型卡
curl -sL -o /tmp/selfie_multiclass_256x256.tflite \
  "https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/1/selfie_multiclass_256x256.tflite"
curl -sL -o /tmp/multiclass_card.pdf \
  "https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf"
shasum -a 256 /tmp/selfie_multiclass_256x256.tflite /tmp/multiclass_card.pdf
pdftotext -layout /tmp/multiclass_card.pdf - | grep -i -A6 "LICENSED UNDER\|trained on"

# 2) 现成 ONNX（仅用于实证，不入库）
git clone --depth 1 --filter=blob:none --sparse https://github.com/16sakuraa/PortraitColorLab.git /tmp/PortraitColorLab
cd /tmp/PortraitColorLab && git sparse-checkout set web/models
shasum -a 256 web/models/selfie_multiclass_256x256.onnx

# 3) ONNX 结构解析（/tmp 虚拟环境：pip install onnx）
python - <<'PY'
import onnx, collections
m = onnx.load('/tmp/PortraitColorLab/web/models/selfie_multiclass_256x256.onnx')
print(m.ir_version, [(o.domain, o.version) for o in m.opset_import])
print(dict(sorted(collections.Counter(n.op_type for n in m.graph.node).items())))
print([(i.name, [d.dim_value for d in i.type.tensor_type.shape.dim]) for i in m.graph.input])
print([(o.name, [d.dim_value for d in o.type.tensor_type.shape.dim]) for o in m.graph.output])
PY

# 4) tract 0.23.10 实测（探针在 /tmp/tract-probe，依赖 tract-onnx =0.23.10）
cd /tmp/tract-probe && cargo run --offline -- \
  /tmp/PortraitColorLab/web/models/selfie_multiclass_256x256.onnx
```

> 本轮所有下载与探针文件均在 `/tmp`，**未写入仓库**；`src/`、`src-tauri/src/` 与既有 `docs/` 文件均未改动。
