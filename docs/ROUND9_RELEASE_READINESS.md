# ROUND 9 — 发行就绪评估

- 日期：2026-11-21
- 总体状态：**NOT RELEASE READY**
- 阻塞项：LibRaw 法律审核未完成、NME 精度证据缺失、Windows 未验证

## 1. 按任务书条件逐项

| # | 条件 | 状态 | 依据 |
| --- | --- | --- | --- |
| 1 | 用户项目数据不丢、不错配 | 已达成本轮目标 | 固定根因 + 保存合并修复 + 65 前端 / 431 Rust 测试 + GUI 复验 |
| 2 | 加载阶段与 UI 状态分离 | 已实现 | `idle/loading/restoring/ready/failed/closing` + `requestId` |
| 3 | 默认参数不得覆盖真实值 | 已实现 | 非 `ready` 禁止写入；`setFaces` 拒绝 |
| 4 | 保存/重开参数、人脸、蒙版一致 | 已验证 | GUI 4 轮打开/保存/重启；20 次自动化恢复 |
| 5 | 用户 ONNX 模型真实推理链路 | **部分达成** | Worker 全链路以 `ProtocolTestOnly` 模型跑通；无合法真实关键点模型的精度/语义验证 |
| 6 | Worker 隔离与异常不崩主应用 | 已验证（macOS） | 7 个集成测试 + 异常矩阵；Windows 未验证 |
| 7 | 不得自动解锁正式 LEVEL 3 | 已实现 | 导入后仅「可用」，RuntimeTested 不解锁 |
| 8 | LibRaw 合规材料齐全 | 已备齐并随包分发 | 源码包、双许可、COPYRIGHT、RELINK、构建脚本 |
| 9 | LibRaw 法律审核 | **未完成** | `LEGAL_REVIEW_PENDING`（外部条件） |
| 10 | 自动化发行检查 | 已实现 | `scripts/check_release_compliance.sh` 缺材料 → BLOCKED |
| 11 | NME ≥50 张授权集与指标 | **未完成** | 无授权数据集 → `BLOCKED`，未伪造指标 |
| 12 | 真实精度门槛 | 已预置，未测定 | 门槛在结果前固定 |
| 13 | macOS 正式包验收 | 通过 | `.app` + DMG，GUI 全链路 |
| 14 | Windows 包验收 | **未验证** | 无环境，不写 PASS |

## 2. 阻塞项与解除条件

### 2.1 LibRaw 法律审核（高）

- 现状：材料齐全、脚本全 PASS，但法律判断不由工程方替代。
- 解除：法务确认 LGPL-2.1 静态链接 + 源码包 + 重链接说明满足 §6，并在
  `ROUND9_LIBRAW_COMPLIANCE.md` 更新状态为已审核。
- 在此之前不得对外发布含 LibRaw 的正式安装包，不得宣称“合规”。

### 2.2 NME 精度证据（中高，影响能力宣称）

- 现状：评测工具链、标注规范、预置门槛齐备；无授权数据集。
- 解除：采集 ≥50 张授权样本 → 双人标注 → 冻结 Apple Vision 映射 → 运行
  `scripts/nme_evaluate.py` 并把结果写入 `ROUND9_NME_EVALUATION.md`。
- 在此之前 LEVEL 3 /“精准关键点”类宣称不得出现；现有 UI 已锁定 LEVEL 3。

### 2.3 Windows（中）

- 现状：未构建 NSIS、未实机启动。
- 解除：在 Windows 环境构建并执行 `ROUND9_GUI_ACCEPTANCE.md` 中的同款场景。

## 3. 可以发布的情形（如果业务需要提前发布）

只能作为**不含 LibRaw 的受限预览版**或内部版本，并满足：

- 移除 RAW 支持或改为动态链接并完成对应义务；
- 界面不出现任何精度宣称，LEVEL 3 保持锁定；
- 明确标注 macOS-only、Windows 未验证。

当前构建**不满足**上述“受限预览版”的自动条件，因此本报告不给出可发布结论。

## 4. 结论

工程侧第九轮目标（数据完整性、Worker 推理链路、合规材料、评测工具）已完成；
发行侧仍被**法律审核**与**精度证据**两个外部条件阻塞。状态：`NOT RELEASE READY`。
