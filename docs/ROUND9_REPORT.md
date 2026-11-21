# ROUND 9 — 最终报告

- 日期：2026-11-21
- 范围：数据完整性、ONNX 运行链路、发行准备、精度证据
- 结论摘要：工程目标完成；**发行状态 NOT RELEASE READY**，阻塞于 LibRaw 法律审核与
  NME 精度数据授权；Windows 未验证。

## A. Git 信息

| 项 | 值 |
| --- | --- |
| 第八轮基线 | `479416f` |
| 第八轮收尾 | `a9970dc` |
| 本轮起点 HEAD | `fd4512a`（回填第八轮 SHA 的文档提交） |
| 本轮提交 | `5e92e22` 恢复原子快照/状态机/人脸 UUID 写回 |
|  | `05c15e0` 加载根因审计与规格 |
|  | `6fe3515` ONNX 协议模型与 Worker 推理 |
|  | `d8b81c8` LibRaw 合规材料与检查脚本 |
|  | `34e4dca` NME 评测工具链与标注规范 |
|  | `eb4516d` 保存数据丢失修复、导出计时、验收文档 |
| 收尾提交 | `1662282` 最终审计报告与加载中关闭回归用例 |
| 历史完整性 | 未重置、未覆盖任何既有提交 |

主要修改文件：

- 前端：`src/state/editorStore.ts`、`src/state/project.ts`、`src/App.tsx`、
  `src/components/StatusBar.tsx`、`src/types.ts`、`src/lib/diagnostics.ts`、
  `src/state/projectRecovery.test.ts`
- Rust：`commands/project.rs`、`commands/reshape.rs`、`commands/render.rs`、
  `image_engine/project.rs`、`face_reshape/identity.rs`、
  `face_intelligence/eval_export.rs`、`face_intelligence/mod.rs`、`main.rs`
- 测试：`src-tauri/tests/onnx_worker.rs`、`tests/fixtures/onnx/*.onnx`、
  `scripts/make_protocol_models.py`
- 发行：`src-tauri/resources/libraw/*`、`scripts/check_release_compliance.sh`、
  `src-tauri/tauri.conf.json`
- 文档：`docs/ROUND9_*.md`、`docs/specs/round9/*`、`docs/assets/round9/*.png`

## B. 项目恢复

**根因（加载，R9-LOAD-01）**：冷启动后 `setFaces` 用内存会话 UUID 匹配项目 UUID
失败，写入全 0 的 `defaultFaceBeauty`；随后 `applyFaceRestore` 又用检测期 `face.id`
（而映射值是改写后的 id）查表失败，再次落入默认值。结果：首次重开参数为 0，
同会话第二次重开正常。详见 `ROUND9_PROJECT_LOAD_AUDIT.md`。

**根因（保存，R9-SAVE-01）**：`save_project` 用会话人脸记录整体覆盖
`photo.faces`。加载期按 Provider 一致性丢弃的稠密结论被写回磁盘——打开→保存
即永久删除用户已有的 85 点分析结果（文件 27,281 B → 8,070 B）。

**修复位置**：

- `src/state/editorStore.ts`：`beginProjectLoad/commitProjectSnapshot/beginFaceRestore/
  finishProjectLoad/failProjectLoad/closeProject`；非 `ready` 禁止 `commit/apply`；
  `applyFaceRestore` 按 `faceUuid` 合并，未匹配保留原参数。
- `src/App.tsx`：加载带 `requestId`，提交前校验 `projectId`，失败进入 `failed`。
- `src-tauri/src/commands/project.rs`：`merge_saved_faces` 按 UUID 合并，
  dense/level3 成对保留，未检测人脸记录保留。
- `src-tauri/src/face_reshape/identity.rs` + `commands/reshape.rs`：`final_face_id`
  写回，消除检测期/恢复期 ID 空间不一致。

**状态机**：`idle → loading → restoring → ready`，异常 `failed`，关闭 `closing`；
仅 `ready` 允许编辑与保存。

**验证**：

- 自动化 20 次连续恢复：参数逐次一致（`src/state/projectRecovery.test.ts`，7 用例）；
- A 加载中切 B，A 晚返回不覆盖 B（requestId 守卫，已断言）；
- 恢复期编辑/保存被拒绝；加载中关闭后迟到的提交被丢弃（新增用例）；
- 缺失人脸保留参数、损坏项目进入 `failed` 且不生成空项目、多人脸参数不交错；
- GUI 实测 4 轮「打开 → 保存 → 退出 → 冷启动 → 重开」：17/33/11 始终一致，
  dense 10 区域 / 85 点、`level3.denseLandmarks=true` 保留。
- 详见 `ROUND9_PROJECT_RECOVERY_TESTS.md`。

## C. ONNX

| 项 | 值 |
| --- | --- |
| 模型 | `protocol68.onnx`（自建最小协议模型，`ProtocolTestOnly`） |
| 来源/许可 | 本仓库脚本生成，CC0-1.0 |
| SHA256 | `a0b1c9c15fbc1e1af48fb1347a680a1b464b55016be9ff82b388c0bf46808e7b` |
| 输入 | `[1,3,64,64]` float32，NCHW |
| 输出 | `[1,68,3]`，归一化裁剪坐标 |
| 拓扑 | `face-68` |

**链路（真实 Worker 子进程）**：导入 → SHA256 → Manifest → 结构与算子检查 →
Worker 启动 → 模型加载 → 预处理 → 推理 → 输出长度/数值校验 → 语义映射 →
坐标转换 → Provider 输出。实测 68 点、单次 13–14 ms，UI 导入后显示「可用」。

**异常矩阵**（`src-tauri/tests/onnx_worker.rs`，7 用例全部通过）：损坏文件、
不支持算子、输入维度错误、输出维度错误、NaN/Inf、越界坐标、Worker 启动失败、
超时、连续推理、模型切换、多人脸输入、重复加载。主应用均未崩溃。

**安全边界**：边长 ≤1024、元素 ≤3,145,728、模型 ≤512 MiB、推理超时 10 s、
`RLIMIT_AS` 1.5 GiB、`RLIMIT_CPU` 10 s（Unix）。**不是完整沙箱**，Windows 无等价限制。

**能力等级**：`Imported / StructurallyValidated / ProtocolTested / RuntimeTested /
AccuracyUnverified / AccuracyVerified`。当前最高到 `RuntimeTested`，
**未解锁正式 LEVEL 3**。真实第三方关键点模型未集成、未验证（无合法模型），
这一项如实保持未完成。

详见 `ROUND9_ONNX_RUNTIME_VALIDATION.md`。

## D. LibRaw

| 项 | 值 |
| --- | --- |
| 版本 | `libraw_rs_vendor 1.0.0`（内置 LibRaw 0.21.1） |
| 许可 | LGPL-2.1-only 或 CDDL-1.0，本项目选 LGPL-2.1 |
| 链接 | 静态（`otool -L` 无 libraw 动态依赖；二进制含 `LibRaw 0.21.1` 与 C++ 符号） |
| 源码包 | `libraw_rs_vendor-1.0.0-source.tar.gz`，SHA256 `58317a28…` |
| 随包材料 | 源码包、LGPL-2.1 全文、CDDL-1.0 全文、COPYRIGHT、SOURCE.md、RELINK.md、MODIFICATIONS.md、libraw-build.sh |
| 打包 | `bundle.resources` 含 `resources/libraw/*`，已核验存在于 `.app` |
| 自动检查 | `scripts/check_release_compliance.sh`：材料/产物全部 PASS，仍输出 BLOCKED |
| 法律状态 | `LEGAL_REVIEW_PENDING` |

未完成法律事项：法务确认静态链接材料组合是否满足 §6、是否需要额外目标文件、
EULA 兼容性、Windows 包内容复核。

详见 `ROUND9_LIBRAW_COMPLIANCE.md`。

## E. NME

| 项 | 值 |
| --- | --- |
| 样本数 | **0**（无授权数据集） |
| 授权 | 未取得 |
| 标注人员 | 无（规范已就绪） |
| 语义映射 | 8 区域 + Semantic ID 规范已定义；Apple Vision 映射**未冻结**（无数据验证） |
| NME / P50 / P95 | **未计算，不报告** |
| 失败率 | **未计算** |
| 状态 | `BLOCKED` / `AccuracyUnverified` |

已交付：`scripts/nme_evaluate.py`（授权校验、按 semanticId 配对、预置门槛、
JSON/CSV 输出）、`--nme-predict` 预测导出、标注规范、预置门槛。
工具链验证：度量自检 6 项 PASS；真实照片导出 85 点（仅证明链路可用，不是精度）。

局限：无法把稳定性或工具链可用性当作精度；没有任何真实误差数字。

详见 `ROUND9_NME_EVALUATION.md`、`ROUND9_LANDMARK_ANNOTATION_GUIDE.md`。

## F. GUI 与打包

- macOS 27.2.1 / Apple M4 / 16 GB：`.app` 16.52 MiB、DMG 8.31 MiB，实机全流程 PASS；
- Windows NSIS：**未构建、未验证**；
- 截图：`docs/assets/round9/01`–`13`；
- 性能：项目加载 145–158 ms，双击到稳定 0.79–0.95 s，内存 99.5 → 283.8 MB，
  连续重开无持续增长（376.7 → 292.7 → 272.2 MB），导出 258 ms（不含对话框），
  Apple Vision 117 ms，ONNX 推理 13–14 ms。

详见 `ROUND9_GUI_ACCEPTANCE.md`。

## G. 测试

| 命令 | 结果 |
| --- | --- |
| `cargo test` | **431 passed / 0 failed / 7 ignored**（12 个目标） |
| `cargo clippy --all-targets -- -D warnings` | 0 warning |
| `npm run typecheck` | 通过 |
| `npm test -- --run` | **66 passed / 0 failed**（6 文件） |
| `npm run build` | 通过（453.9 kB / gzip 133.7 kB） |
| `npm run app:build` | 通过（.app + DMG） |
| `python3 scripts/nme_evaluate.py --self-test` | 6 项 PASS |
| `sh scripts/check_release_compliance.sh` | 材料 PASS，结论 BLOCKED（法律未审） |

未执行：Windows 构建/实机、真实第三方 ONNX 模型、NME 真实数据集。

## H. 风险

**已解决**

- 首次重开参数归零（根因固定并修复，20 次自动化 + 4 轮 GUI 复验）；
- 保存删除已有稠密关键点（真实数据丢失，已修复并保留旧门控配对）；
- 导出耗时误把文件对话框停留时间计入；
- 诊断日志可能写出绝对路径。

**未解决**

- LibRaw 法律审核（发行阻塞，外部条件）；
- NME 精度证据（能力宣称阻塞，外部条件）；
- 真实第三方关键点模型集成与精度；
- Windows 全链路；
- Worker 非完整沙箱、`.lumi.bak` 仅一份。

**下轮建议**

1. 取得授权数据集并按规范完成标注与映射冻结，产出真实 NME；
2. 推动 LibRaw 法律审核并归档结论；
3. 在 Windows 环境完成构建与 Worker 异常验证；
4. 为真实关键点模型准备合法来源，跑通并区分 RuntimeTested / AccuracyVerified；
5. 备份策略从单份 `.bak` 升级为带时间戳的多版本保留。

本轮结束，不进入第十轮。
