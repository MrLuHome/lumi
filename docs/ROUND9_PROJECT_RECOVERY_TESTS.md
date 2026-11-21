# ROUND 9 阶段 3：项目恢复自动化回归测试

> 基线：`fd4512a` · 对应修复：阶段 2（原子快照 + 受限状态机 + 人脸参数按 UUID 写回）
> 本文件记录真实执行结果，不是计划。OS 级关窗行为在阶段 8 GUI 验收补测。

## 1. 测试环境

| 项 | 值 |
| --- | --- |
| 系统 | macOS 27.2.1 (darwin arm64) |
| CPU / 内存 | Apple M4 / 16 GB |
| Node / npm | 22.15.0 / 10.9.2 |
| Rust / Cargo | 1.95.0 |
| 前端测试 | Vitest 5.1.1 |
| Rust 测试 | `cargo test`（debug） |

## 2. 覆盖矩阵

| 编号 | 场景 | 自动化 | 用例 |
| --- | --- | --- | --- |
| A | 连续恢复 20 次参数一致 | 是 | `连续恢复 20 次参数完全一致` |
| B | A 加载中切 B，A 迟到回调不得覆盖 B | 是 | `A 加载中切 B：A 的迟到回调不覆盖 B` |
| C | 读取中关窗 / 半写入 | 部分 | 原子写 + 回读校验 + `.lumi.bak`；OS 关窗在阶段 8 |
| D | 缺字段 / 非法参数 / 人脸缺失 / 蒙版缺失 / 旧版本 / 损坏文件 | 是（前端 + Rust） | `参数损坏时进入 Failed…`、`未匹配的人脸记录保留原参数…`、`v2_project_without_new_fields_loads_with_defaults`、`project_without_round7_fields_still_opens`、`future_version_is_rejected` |
| E | 加载中保存 / 切换 / 多请求同时返回 | 是 | `恢复期间禁止编辑与保存`、`A 加载中切 B` |
| F | 多照片 / 多人脸 / 每脸独立参数 | 是 | `多人脸各自独立参数，恢复后不交错` |

额外核心用例：
- `冷启动恢复：会话新 UUID 不得把项目人脸参数覆盖成 0` —— 直接复现阶段 1 根因。

## 3. 前端真实执行结果

```
$ npm test -- --run

 ✓ src/lib/format.test.ts (4 tests)
 ✓ src/state/history.test.ts (5 tests)
 ✓ src/state/params.test.ts (20 tests)
 ✓ src/state/paramGroups.test.ts (5 tests)
 ✓ src/state/projectRecovery.test.ts (7 tests)
 ✓ src/state/editorStore.test.ts (24 tests)

 Test Files  6 passed (6)
      Tests  65 passed (65)
   Duration  161ms
```

关键断言（摘自 `src/state/projectRecovery.test.ts`）：

| 断言 | 修前 | 修后 |
| --- | --- | --- |
| `setDetectedFaces` 后 `faces[0].beauty.skinSmooth` | 0（被默认值覆盖） | 17 |
| `applyFaceRestore` 后 `faceId` | 检测临时 ID | `1700055149`（项目 ID） |
| `applyFaceRestore` 后 `beauty` | 全 0 | `{17, 33, 11}` |
| 恢复期间 `updateParam("exposure")` | 生效并推入历史 | no-op，历史为空 |
| A/B 切换后 `activeId` | 可能被 A 覆盖 | 恒为 B |
| 损坏参数项目 | 空项目 + 默认值 | `failed` + `project-invalid`，不提交 |

20 次循环的实现：每次 `createEditorStore()` 模拟冷启动，重新 `beginProjectLoad → commitProjectSnapshot → setDetectedFaces(随机会话 UUID) → applyFaceRestore → finishProjectLoad`，断言 beauty 三项与 `faces.length` 完全一致。

## 4. Rust 真实执行结果

```
$ cargo test --lib commands::reshape
running 3 tests
test final_face_id_falls_back_to_uuid_derived_id ... ok
test final_face_id_keeps_pending_detection_id ... ok
test final_face_id_uses_persisted_identity_not_detection_id ... ok
test result: ok. 3 passed; 0 failed

$ cargo test --lib image_engine::project
running 8 tests ... ok
test result: ok. 8 passed; 0 failed

$ cargo test（全量，阶段 2 后）
lib: 376 passed, 3 ignored
integration: 全部通过（face_detection 4、face_intelligence 5、raw_real 2、reshape_persistence 3、reshape_pipeline 3 + 1 ignored、round8_benchmark 0 + 1 ignored）
```

## 5. 完整性保护验证

| 措施 | 实现位置 | 验证 |
| --- | --- | --- |
| 写入回读校验 | `project.rs::save` | `project_roundtrip_keeps_params_and_masks` 二次保存后读回一致 |
| 旧版本备份 | `project.rs::save` → `*.lumi.bak` | 同上断言 `.bak` 存在 |
| 原子写 | 临时文件 + `rename` | 保留 |
| 损坏不覆盖原文件 | 解析失败在 `load` 返回 Err，`save` 不执行 | `参数损坏时进入 Failed…` |
| 人脸 UUID + 参数一致 | `buildProjectSnapshot` 产出 warnings | 同步扫描 `params.faces[].faceUuid` |
| 源图哈希关联 | 复用 `sourceHash`，不重算图片 SHA256 | 阶段 1 数据样本 + `replace_source_blocks_automatic_parameter_restore` |

## 6. 未覆盖 / 待阶段 8

- OS 级「读取中直接关窗」无法由 Vitest 驱动；当前依靠：恢复期禁止保存、读文件在 `spawn_blocking`、写入原子化 + 回读校验。
- 大项目（50+ 照片）的峰值内存与恢复耗时在阶段 8 记录。
- 多人脸 + 多蒙版组合的像素级一致性属于渲染回归，不在本阶段范围。

## 7. 结论

- 阶段 1 根因（冷启动人脸参数被默认值覆盖）已被自动化用例固定，修复后可重复验证。
- 恢复状态机、requestId 守卫、保存禁区、损坏项目 Failed 路径均有真实断言。
- 无失败用例，无跳过掩盖；ignored 的 3 项为既有性能/基准测试。
