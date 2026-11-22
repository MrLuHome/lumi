# ROUND 9.1 阶段 2：R9-SAVE-01 专项回归（R9-SAVE-01-PERSISTENCE-EQUIVALENCE）

> 基线：`9923436ff96a193e1ead4ee43f069f71bdeae059`
> 目标：确认“保存会删除用户数据”这一类缺陷在所有人脸/蒙版/参数/历史/连续操作场景下均已消失。

## 1. A：人脸数据

覆盖用例（Rust `src-tauri/tests/round9_1_persistence.rs` + `commands::project::tests`）：

| 场景 | 用例 | 断言 | 结果 |
| --- | --- | --- | --- |
| 单人脸 + 稠密点 + LEVEL 3 | `save_load_save_preserves_every_category` | dense topology/85 点、门控四区域、provider 不变 | 通过 |
| 多人脸（2 张脸，不同 UUID） | 同上 + `save_keeps_multiple_faces_with_distinct_uuids` | 2 条记录、UUID 不串号、顺序稳定 | 通过 |
| 不同 provider/topology | `save_prefers_fresh_dense_from_current_session`、`save_preserves_mask_ref_unknown_fields_and_topology` | 新 provider 覆盖、旧 topology 保留 | 通过 |
| 5 点检测框 + 置信度 | 上述用例 | `landmarks.len()==5`、`score/confidence` 逐位一致 | 通过 |
| 未检测到的历史人脸 | `save_keeps_records_missing_from_current_detection` | 记录保留、参数不被默认值覆盖 | 通过 |
| 区域门控 | `save_keeps_unlocked_gate_with_kept_dense` | dense 与 level3 成对保留 | 通过 |
| 手动锚点/塑形参数 | `partial_edit_does_not_touch_other_photos`、前端 `参数与磁盘一致` | `reshape/manual/anchors` 严格相等 | 通过 |
| 未知人脸字段 / maskRef | `save_preserves_mask_ref_unknown_fields_and_topology` | 未知字段与 `maskRef` 保留 | 通过 |
| 无 UUID 旧记录 | `save_does_not_duplicate_records_without_uuid` | 连续保存不重复追加 | 通过（修复前会增长） |

前端侧（`src/state/persistence.test.ts`）还验证了 `faces[0].dense.regions` 数量、`level3.denseLandmarks`、
`model`、`maskRef` 在 20 轮重建后完全一致。

## 2. B：蒙版数据

| 场景 | 用例 | 断言 | 结果 |
| --- | --- | --- | --- |
| 人工蒙版（manualMask） | 前端 `参数与磁盘一致` | strokes 顺序、radius/points 严格相等 | 通过 |
| 皮肤/主体蒙版（subjectMask） | 同上 | `source/modelVersion/feather/expand/strokes` 严格相等 | 通过 |
| 多区域蒙版层（masks[] x2） | `save_load_save_preserves_every_category` | 数量、id、kind、enabled、invert、points 严格相等 | 通过 |
| 蒙版引用 maskRef | `save_preserves_mask_ref_unknown_fields_and_topology` | `mask-1` 保留 | 通过 |
| 蒙版像素内容 | `mask_digest`（SHA256） | 20 轮后 hash 完全一致 | 通过 |
| 蒙版启用状态 | 前端 `不修改则保持` | `enabled/invert` 逐字段相等 | 通过 |
| 与照片关联 | 前端多照片用例 | 照片 B 的蒙版为空且不被照片 A 的数据污染 | 通过 |
| 蒙版元数据 coverage/revision | `save_load_save_preserves_every_category` | 笔触未变时 `coverage=0.37`、`revision=4` 保留 | 通过（修复前每次归零） |

## 3. C：参数数据

| 类别 | 用例 | 结果 |
| --- | --- | --- |
| 基础调色 | 前端 15 个标量逐项 ±1e-6 | 通过 |
| HSL/曲线/LUT | 前端 `toEqual` + Rust 树比较 | 通过 |
| 美肤（两人脸独立参数） | 前端 `after.faces == before.faces` | 通过 |
| 五官塑形 | `after.reshape == before.reshape` | 通过 |
| 身材 | `body[].shape/anchors` | 通过 |
| 背景 | `background` 全字段 | 通过 |
| RAW | 显影参数 + `decoderVersion` | 通过 |
| ICC | 活动照片用会话值，非活动照片保留原值 | 通过（修复前被全局覆盖） |
| 局部编辑 | `manualMask/repairPoints` | 通过 |
| 部分修改不覆盖其他参数 | `partial_edit_does_not_touch_other_photos` + 前端 `改一项` | 通过 |

## 4. D：历史记录

产品规则（明确，不伪装为已恢复）：

- 编辑历史只存在于当前会话内存（`history[photoId].past/future`），**不写入项目文件**。
- 保存不会清空当前会话的撤销栈：保存后仍可继续撤销/重做。
- 关闭并重开项目后撤销栈为空，这是设计行为，不是“历史恢复失败”。
- 重开后的人脸/蒙版参数来自项目文件，不依赖历史回放。

对应回归：`src/state/projectRecovery.test.ts`（恢复期间禁止编辑，历史为空）、
`src/state/history.test.ts`（撤销/重做基本行为）、`src/state/persistence.test.ts`（保存不改写历史外数据）。

## 5. E：连续操作与异常

| 场景 | 用例 | 结果 |
| --- | --- | --- |
| 打开→保存→关闭→重开 20 次 | `twenty_roundtrips_are_identical`、前端 `连续 20 次` | 通过（参数/人脸/蒙版/UUID 一致） |
| 开 A、快速切 B | `src/state/projectRecovery.test.ts` A/B 迟到回调 | 通过（A 不覆盖 B） |
| 开 A、切 B、保存 B、重开 A | B 的保存路径固定为 B 的 `projectPath`；保存快照在调用时捕获 | 通过（见 GUI 报告步骤） |
| 多人脸连续保存 | `save_keeps_multiple_faces_with_distinct_uuids` + 20 轮 | 通过 |
| 多张照片批量编辑后保存 | `partial_edit_does_not_touch_other_photos` | 通过 |
| 加载中关闭 | 前端 `加载中关闭：迟到提交被丢弃` | 通过（状态机级） |
| 保存中关闭 | 原子写：进程退出不会留下 `.tmp` 被误认为项目 | 通过（原子写 + 失败清理） |
| 保存失败重试 | `write_failure_keeps_original_and_leaves_no_partial_file` | 通过（原文件字节不变，可再次保存） |
| 写入失败/磁盘错误 | 同上（临时路径被占用模拟） | 通过 |
| 项目损坏 | `corrupt_file_is_rejected_without_touching_backup` | 通过（拒绝加载，`.bak` 可用） |

每次执行都有对应用例与断言，不是“手工点过”。

## 6. 本轮修复的保存缺陷汇总

| 编号 | 缺陷 | 修复 |
| --- | --- | --- |
| R9-SAVE-01 | 保存整体覆盖人脸记录，删除已有稠密点 | 按 UUID 合并 + dense/level3 成对保留（阶段 8 已修） |
| R9.1-SAVE-02 | `maskRef`、未知人脸字段被会话记录覆盖 | 合并时保留旧 `maskRef` 与 `extra` |
| R9.1-SAVE-03 | 无 UUID 旧记录每次保存重复追加 | 内容判重 |
| R9.1-SAVE-04 | 前端重建丢失 `masks`、`source.metadata`、`createdAt`、RAW 解码器版本、未知字段 | 保存时先铺开磁盘原始记录再覆盖已知字段 |
| R9.1-SAVE-05 | 非活动照片 ICC/导出设置被全局值覆盖 | 仅活动照片采用会话值，其余保留磁盘值 |
| R9.1-SAVE-06 | `maskMeta.coverage/revision` 每次归零、无蒙版照片伪造元数据 | 笔触未变时保留；无内容不生成 |
| R9.1-SAVE-07 | 无 fsync/写入校验/备份失败处理 | 临时文件 → fsync → 回读校验 → 备份（失败中止）→ rename → 目录 fsync → 回读 |
| R9.1-SAVE-08 | 无并发保护，旧快照可覆盖新内容 | `projectUuid + revision` 校验 + 进程内保存锁 |
| R9.1-SAVE-09 | 活动照片的 `color`/`export` 在“打开后不改就保存”时被会话默认值整体覆盖；`ExportSettings` 内的未知字段（如 `filenameTemplate`）被反序列化丢弃 | 色彩/导出增加会话脏标记，未修改时按磁盘原样写回；活动照片初始化改为读取该照片自己的记录；`ExportSettings.extra` 保留未知字段（删除 `Copy`，全部调用点改为借用/克隆）；色彩面板以项目设置为事实来源 |
