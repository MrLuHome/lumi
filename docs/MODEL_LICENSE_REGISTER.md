# 模型许可台账（MODEL LICENSE REGISTER）

> 维护人：LUMI 团队 · 最近更新：2026-11-23（皮肤语义分割权重接入）
> 本文是人可读版的许可台账。机器可读版本是
> `src-tauri/src/ai_engine/model_registry/builtin.rs` 里的 `ModelSpec` 常量，
> 两者必须同步；不一致时**以代码为准**，并立刻修正本文。

---

## 1. 规则（先读这一节）

1. **许可四字段独立**：`weights`（权重本身）、`training_data`（训练数据）、
   `commercial_use`（商用）、`redistribution`（再分发）。任何一个缺失都必须记为
   `Unverified`，不得默认成 `Allowed`。
2. **不得根据本仓库的 LICENSE 推断权重可商用**。字段来源只承认三类：
   - 官方发布页 / README 原文
   - 随包 LICENSE 文件原文
   - 数据集官方条款
3. 未核实字段必须出现在 `unverifiedFields` 里，前端可以显示
   「权重可商用，数据集条款待核」，而不是把未核实的东西说成已核实。
4. 准入判定顺序固定：**许可 → 文件存在 → 格式 → 契约 → SHA256 → 解析**。
   未通过许可的权重**一个字节都不读**（连哈希都不算）。
5. 用户自备模型（Model Hub）按主理人裁决**不纳入强制许可闸门**，
   但永远标为 `userUnverified`、`official = false`，不进入正式注册清单，
   不参与默认美肤 / 默认精修。

---

## 2. 随包权重

### 2.1 YuNet 2023mar · 人脸检测

| 项 | 值 |
|----|----|
| 模型 ID | `face-detect-yunet-2023mar` |
| 显示名 / 版本 | YuNet 2023mar / `2023mar` |
| 任务 | 人脸检测（5 点关键点 + 边框） |
| 文件 | `src-tauri/resources/models/yunet/face_detection_yunet_2023mar.onnx` |
| 体积 | 232,589 B（实测） |
| SHA256 | `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4` |
| 输入契约 | `[1,3,640,640]` F32 NCHW **BGR 0..255** |
| 输出契约 | 12 个 head：cls / obj / bbox / kps × stride 8/16/32 |
| SPDX | MIT |

**许可四字段**

| 字段 | 结论 | 依据 |
|------|------|------|
| `weights` | **Allowed** | ① 随包 `resources/models/yunet/LICENSE` 全文为 MIT，`Copyright (c) 2020 Shiqi Yu`；② opencv_zoo `models/face_detection_yunet` README「License」小节原文：“All files in this directory are licensed under MIT License.”（2026-11-22 访问） |
| `commercial_use` | **Allowed** | MIT 条款原文允许 “use, copy, modify, merge, publish, distribute, sublicense, and/or sell” |
| `redistribution` | **Allowed** | 同上（可再分发，需保留版权声明与许可声明） |
| `training_data` | **Unverified** ⚠️ | 需人工核对 WIDER FACE 数据集使用条款（<https://shuoyang1213.me/WIDERFACE/>）；上游 README 未声明数据集许可 |

**未核实字段**：`["trainingData"]`

**另外一项未核实**（不在四字段内，但需登记）：上游权重仓
`https://github.com/ShiqiYu/libfacedetection.train` 的 LICENSE 未核对；
opencv_zoo README 只声明「本目录内文件」为 MIT。

**结论**：权重可商用、可随包再分发；**训练数据条款未核实**，需法务复核后
才能说「整条数据链路已清」。UI 上应显示为「已验证·允许商用」+
「数据集条款待核」，不得显示为完全干净。

**溯源**

- 权重页：<https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet>
- 许可页：<https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet/LICENSE>
- 随包 LICENSE：`src-tauri/resources/models/yunet/LICENSE`
- 核对日期 / 核对人：2026-11-22 / 高见远（人工核对官方页面与随包 LICENSE 原文）

---

### 2.2 Selfie Multiclass 256×256 · 皮肤语义分割

| 项 | 值 |
|----|----|
| 模型 ID | `skin-parse-mediapipe-selfie-multiclass-256` |
| 显示名 / 版本 | Selfie Multiclass 皮肤解析 / `256x256` |
| 任务 | 皮肤语义分割（6 类逐像素：背景/头发/身体皮肤/面部皮肤/衣物/其他） |
| 文件 | `src-tauri/resources/models/skin/selfie_multiclass_256x256.onnx` |
| 体积 | 16,453,400 B（实测） |
| SHA256 | `485774fe148451ca3f6c53626654b9a5ea34d291fa07ddac53725aa35ec07587`（实测） |
| 输入契约 | `[1,3,256,256]` F32 NCHW **RGB [-1,1]**（`(x/127.5)-1`） |
| 输出契约 | `Identity`，`[1,6,256,256]` F32 NCHW **logits**，需后处理沿 axis=1 softmax 后取 `p(2)+p(3)` |
| SPDX | Apache-2.0 |

**许可四字段**

| 字段 | 结论 | 依据 |
|------|------|------|
| `weights` | **Allowed** | 官方模型卡 PDF 原文 `LICENSED UNDER Apache License, Version 2.0`（下载 SHA256 `59372fce0666d4c75b3877540fa77f80ac4b2a0f75e914711417f85fc490a9e6`，1,378,158 B）；随包 `LICENSE` 为 Apache-2.0 全文（11,357 B，来源 `LICENSE-selfie-multiclass`）；官方 repo `LICENSE` 同为 Apache-2.0（2026-11-23 访问） |
| `commercial_use` | **Allowed** | Apache-2.0 §2 明示 `use … sell`；模型卡 `Intended Uses` 含 AR/视频会议/娱乐，未含商用禁止 |
| `redistribution` | **Allowed** | Apache-2.0 §4，需保留版权/许可/NOTICE；第三方先例 `https://github.com/16sakuraa/PortraitColorLab/blob/main/THIRD_PARTY_NOTICES.md` 已按此随包分发并附 `LICENSE-selfie-multiclass` |
| `training_data` | **Unverified** ⚠️ | 模型卡只写 `Model is trained on thousands of samples with diverse identities …`，**未命名数据集、未声明许可**；TFLite 内嵌元数据无 `license`/数据集字符串。需 Google/法务书面确认 |

**未核实字段**：`["trainingData"]`

**结论**：权重 / 商用 / 再分发三项 Allowed，符合项目现行闸门（`license.rs` 训练数据仅披露、不阻塞，与 YuNet 同口径）可随包接入。
**训练数据许可未披露**，UI 状态与本文均如实展示，**不得**说成「整条数据链路已清」。
若法务要求「四字段全部 Allowed」，当前**不可接入**，缺口为官方训练数据集名称与许可声明。

**溯源 / 转换说明**

- 官方 TFLite：`https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/1/selfie_multiclass_256x256.tflite`（16,371,837 B，SHA256 `c6748b1253a99067ef71f7e26ca71096cd449baefa8f101900ea23016507e0e0`）。
- 模型卡：`https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf`。
- 本轮 ONNX：第三方从官方 TFLite 转换（tf2onnx 1.16.1 / opset 13 / NCHW），
  随包 `LICENSE` 为 Apache-2.0 全文。转换细节与待办见
  `src-tauri/resources/models/skin/CONVERSION.md`。
- **待办（不阻塞本轮接入）**：我方从官方 TFLite 自行重跑 tf2onnx 转换，固化脚本与产物 SHA 后覆盖
  `builtin::SELFIE_SKIN` 的 `size_bytes` / `sha256`；独立复现转换产物与 TFLite 的数值一致性（第三方声明 ≤0.0001）。
- 核对日期：2026-11-23；核对方式：模型卡 PDF 原文 + 随包 LICENSE 原文 + 本地 SHA256 实测 + tract 0.23.10 推理实测。

---

## 3. 空登记表（刻意留空）

| 登记表 | 内容 | 原因 |
|--------|------|------|
| `builtin::SEGMENTATION_MODELS` | 空 | 没有取得可再分发的 ONNX 权重与 SHA-256；`ai_engine::segmentation::CATALOG` 是**调研目录**，不是加载源 |

> `builtin::SKIN_MODELS` 已于 2026-11-23 登记 Selfie Multiclass（见 §2.2），不再为空。
> 空登记表 ≠ 没有功能。它表示「没有通过审查的权重」，引擎会明确回退并在
> 状态里说明原因，不用未授权权重冒充 AI 能力。

---

## 4. 用户自备模型（不上随包清单）

- 存放位置：应用数据目录 `<app_data>/face-models/<id>/`（不写安装包内部）。
- 许可结论：**永远 `UserUnverified`**。用户勾选的「商用/再分发」只是声明，
  不参与判定，只写进 `note` 供人复核。
- 清单归属：`registry.snapshot()` 里 `official = false`，
  `summary.userImported` 单独计数，**不计入** `official`。
- 默认管线：不参与默认美肤 / 默认精修（`ModelSpec::is_default_eligible()` 恒 false）。

---

## 5. ⚠️ 新增随包权重时必须同步的清单

1. 在 `src-tauri/src/ai_engine/model_registry/builtin.rs` 里新增 `ModelSpec`，
   并把四字段许可 + 证据链填全（未核实就写 `Unverified { what_to_check }`）。
2. **同步修改 `src-tauri/tauri.conf.json` 的 `resources` 列表**
   （当前为 `resources/models/yunet/*`、`resources/models/skin/*` 与 `resources/libraw/*`）。
   漏改的后果：开发机能跑、打包后 `locate()` 永远返回 `None`，静默回退
   （历史风险 R8）。
3. 在本文第 2 节补一行记录。
4. 若新增 `task`，确认 `inference::CpuTractBackend` 有对应的后处理实现，
   否则 `model_load` 会返回 `E_BACKEND`。
5. 新增随包权重时，状态查询命令（`skin_model_status`）必须走
   `verify::check_shallow`，**禁止**在面板挂载路径上算 SHA256（历史风险 R1）。

**当前随包资源清单**

| 资源 | 用途 |
|------|------|
| `resources/models/yunet/face_detection_yunet_2023mar.onnx` + `LICENSE` | 人脸检测 |
| `resources/models/skin/selfie_multiclass_256x256.onnx` + `LICENSE` + `CONVERSION.md` | 皮肤语义分割（训练数据许可 Unverified，已披露） |
| `resources/libraw/*` | RAW 解码（LGPL 审核前 Beta 包可排除） |

---

## 6. 相关代码位置

| 职责 | 文件 |
|------|------|
| 登记表（唯一真相） | `src-tauri/src/ai_engine/model_registry/builtin.rs` |
| 许可三态与闸门 | `src-tauri/src/ai_engine/model_registry/license.rs` |
| 准入链与失败态 | `src-tauri/src/ai_engine/model_registry/{state,verify}.rs` |
| 前端视图 | `src-tauri/src/ai_engine/model_registry/snapshot.rs` |
| 结构化错误（11 码） | `src-tauri/src/ai_engine/inference/error.rs` |
| 验收测试 | `src-tauri/tests/model_registry.rs` |
| 审计与设计 | `docs/R11_AUDIT_AND_DESIGN.md` |
