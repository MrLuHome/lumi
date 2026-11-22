# 模型许可台账（MODEL LICENSE REGISTER）

> 维护人：LUMI 团队 · 最近更新：2026-11-22（R11）
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

## 3. 空登记表（刻意留空）

| 登记表 | 内容 | 原因 |
|--------|------|------|
| `builtin::SKIN_MODELS` | 空 | BiSeNet / CelebAMask-HQ 系权重受数据集非商用条款限制，InsightFace 多数模型仅限研究，均不满足打包条件。当前皮肤蒙版是**几何方法**（关键点 + 肤色似然），不是 AI 分割，UI 文案已如实说明 |
| `builtin::SEGMENTATION_MODELS` | 空 | 没有取得可再分发的 ONNX 权重与 SHA-256；`ai_engine::segmentation::CATALOG` 是**调研目录**，不是加载源 |

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
   （当前为 `resources/models/yunet/*` 与 `resources/libraw/*`）。
   漏改的后果：开发机能跑、打包后 `locate()` 永远返回 `None`，静默回退
   （历史风险 R8）。
3. 在本文第 2 节补一行记录。
4. 若新增 `task`，确认 `inference::CpuTractBackend` 有对应的后处理实现，
   否则 `model_load` 会返回 `E_BACKEND`。
5. R11 明确**没有**把 `resources/models/skin/**` 加入打包列表
   （主理人裁决 Q3）：无权重可打包，加了也只是空目录。

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
