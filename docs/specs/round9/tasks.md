# ROUND 9 任务清单

> 规格：[spec.md](./spec.md) · 阶段 1 审计：[ROUND9_PROJECT_LOAD_AUDIT.md](../../ROUND9_PROJECT_LOAD_AUDIT.md)

## 阶段 0 基线

- [x] 核对 `HEAD=fd4512a`、父 `a9970dc`、工作区干净
- [x] 记录工具链：rustc/cargo 1.95.0、Node 22.15.0、npm 10.9.2、macOS 27.2.1、Apple M4、16GB
- [x] 阶段 1 只读审计完成

## 阶段 1 加载根因审计（已完成，只查不改）

- [x] 梳理调用链 App → backend → commands/project.rs → project.rs → editorStore → 面板
- [x] 十类原因逐项判定并给出证据
- [x] 确认根因 R9-LOAD-01：`setFaces` + `applyFaceRestore` 以默认值覆盖项目人脸参数
- [x] 交付 `docs/ROUND9_PROJECT_LOAD_AUDIT.md`

## 阶段 2 状态机与完整性修复

- [x] `editorStore` 增加 `ProjectLoadPhase` 与 `loadSession`（requestId/projectId/phase/error/missing/warnings）
- [x] 新增 `beginProjectLoad / commitProjectSnapshot / beginFaceRestore / finishProjectLoad / failProjectLoad / closeProject`
- [x] `src/state/project.ts` 新增 `buildProjectSnapshot`：严格匹配、不做默认填充、返回 missing/warnings
- [x] 移除 `loadProject` 中 `params[item.id] = DEFAULT_PARAMS` 预填
- [x] `clampParams` 拆分“范围收敛”与“缺失检测”；缺失/非法字段进入 warnings
- [x] 新增 `setDetectedFaces`（只写检测结果，不改 `params.faces`）
- [x] 重写 `applyFaceRestore`：按 `faceUuid` 从加载快照合并，禁止默认值覆盖，未匹配进 pending
- [x] Rust `FaceMatch` 增加 `final_face_id` 并在 `apply_identities` 后回填
- [x] `handleSaveProject` 增加 `phase === "ready"` 守卫
- [x] 项目切换/关窗使旧 requestId 失效
- [x] `projectId` 摘要、保存后回读校验、`*.lumi.bak` 备份
- [x] 结构化日志（开发模式开关，发行可关）
- [x] 提交 `fix(round9): 项目恢复原子快照与受限状态机`

## 阶段 3 恢复回归测试

- [x] A 连续恢复 20 次，深比较全部参数
- [x] B 快速切换 A→B，A 迟到回调不覆盖 B
- [ ] C 读取中关窗：原子写+回读校验已实现；OS 级关窗留阶段 8 实测
- [x] D 缺字段/非法参数/人脸缺失/蒙版缺失/旧版本/损坏文件
- [x] E 加载中触发保存/切工作区/关闭、多请求同时返回
- [x] F 多照片/多人脸/每脸独立参数/多蒙版/历史与撤销栈
- [x] Rust：`restore_faces` 返回 `final_face_id` 一致性、旧版本迁移
- [x] 输出真实测试日志与关键断言 → `docs/ROUND9_PROJECT_RECOVERY_TESTS.md`
- [x] 提交 `test(round9): 项目恢复回归矩阵`

## 阶段 4 ONNX 模型准备与协议验证

- [x] 选定合法测试模型，登记来源/版本/SHA256/许可/输入输出/语义拓扑
- [x] 无关键点模型则自建最小协议模型，标 `ProtocolTestOnly`
- [x] Manifest、结构、算子检查与 Worker 启动
- [x] 提交 `feat(round9): ONNX 协议模型与校验`

## 阶段 5 Worker 真实推理与安全

- [x] 跑通导入→推理→输出校验→语义映射→坐标转换→Provider 输出
- [x] 输入/张量/输出/模型大小/超时/CPU/内存/IPC 限额核对
- [x] 异常矩阵：损坏/不支持算子/维度错误/NaN/越界/启动失败/超时/意外退出/IPC 损坏/连续推理/切换/多人脸/重复加载
- [x] 能力等级严格区分，RuntimeTested 不解锁 LEVEL 3
- [x] 输出 `docs/ROUND9_ONNX_RUNTIME_VALIDATION.md`
- [x] 提交 `feat(round9): ONNX Worker 正常推理与异常安全`

## 阶段 6 LibRaw 合规

- [x] 核查实际版本、源码来源、许可、链接方式、补丁、构建脚本
- [x] 整理发行包材料与重链接说明
- [x] 新增发行检查脚本，缺材料返回 `BLOCKED`
- [x] 输出 `docs/ROUND9_LIBRAW_COMPLIANCE.md`（`LEGAL_REVIEW_PENDING`）
- [x] 提交 `docs(round9): LibRaw 合规材料与检查脚本`

## 阶段 7 NME 评测

- [ ] 采集/授权 ≥50 张，划分开发/验证/测试集（同人不得跨集泄漏）— **BLOCKED：无授权数据集**
- [x] 统一 8 区域语义与 Semantic ID，标注规范
- [x] 可重复评测脚本，输出 JSON/CSV、总体与分区、P50/P95、失败率、分组
- [x] 预置门槛后再看结果
- [x] 输出 `ROUND9_NME_EVALUATION.md`、`ROUND9_LANDMARK_ANNOTATION_GUIDE.md`
- [x] 无授权则标 `BLOCKED`
- [x] 提交 `test(round9): NME 评测`

## 阶段 8 打包与跨平台回归

- [x] 全量命令：cargo test 431 passed / clippy -D warnings / typecheck / vitest 65 / build / app:build
- [x] macOS .app + DMG 实机验收（启动/恢复/Apple Vision/ONNX Worker/模型库/保存/导出）
- [x] Windows：无环境，明确记录为未验证（不写 PASS）
- [x] 性能记录（冷/热、内存、Worker、崩溃恢复）
- [x] 输出 `docs/ROUND9_GUI_ACCEPTANCE.md`
- [x] 提交 `docs(round9): GUI 验收与性能`

## 阶段 9 最终审计

- [ ] `ROUND9_REPORT.md`（A–H）
- [x] `ROUND9_SECURITY_REVIEW.md`
- [x] `ROUND9_RELEASE_READINESS.md`
- [ ] 提交并停止，不进入第十轮
