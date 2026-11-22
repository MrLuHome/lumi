# ROUND 9.1 阶段 1：项目持久化等价性测试

> 基线：`9923436ff96a193e1ead4ee43f069f71bdeae059`
> 本文只记录真实执行结果；所有比较基于磁盘文件经正式 `project::save/load` 往返后的内容。

## 1. ProjectIntegritySnapshot 覆盖范围

等价性由“持久化文档树”承载（Rust `ProjectDoc` / 前端 `ProjectDoc`），测试快照即读回后的完整文档。覆盖矩阵：

| 数据类别 | 比较内容 | 严格度 |
| --- | --- | --- |
| 项目元数据 | `version`、`createdAt`、`projectUuid`、`revision`、`updatedAt` | createdAt/uuid 严格相等；revision 单调 +1；updatedAt 允许变化 |
| 照片 | `path`、顺序、`sourceHash` | 严格相等 |
| 基础调色 | 曝光/对比/高光/阴影/白平衡/饱和/清晰度/颗粒/暗角 | 浮点 ±1e-6 |
| HSL/曲线/LUT | 8 通道、4 曲线、LUT 路径与强度 | 浮点 ±1e-6 |
| 美肤 | 每人脸 `beauty` 全字段 + `faceId/faceUuid/enabled` | 严格相等 |
| 蒙版 | `masks[]` 全字段（id/kind/points/feather/opacity/enabled/invert） | 严格相等 + 内容 SHA256 |
| 人脸 | `uuid`、`bbox`、`landmarks`、`score`、`model`、`confidence`、`maskRef` | 严格相等 |
| 稠密关键点 | `topologyId`、`providerId`、`providerVersion`、区域数、每区点数（合计 85） | 严格相等 |
| LEVEL 3 | 门控 provider/topology/`denseLandmarks`/四区域门控 | 严格相等 |
| 五官塑形 | `face/eyes/nose/mouth/chin/manual` | 浮点 ±1e-6 |
| 身材 | `body[].shape`、`anchors`、`maskStrokes` | 浮点 ±1e-6 / 严格长度 |
| 背景 | `mode/color/blur/depth/brightness/...` | 浮点 ±1e-6 |
| RAW | 显影参数 + `decoderVersion` | 严格相等 |
| ICC/色彩 | `color.output`（profile/intent/黑点/嵌入） | 严格相等 |
| 源文件元数据 | `source.metadata`（相机/型号/ISO） | 严格相等 |
| 未知字段 | 文档级、照片级、参数级、人脸级 | 键与值均必须存在 |
| 编辑历史 | 不进入项目文件（仅内存） | 产品规则：不持久化，重开后为空 |
| 批量/预设 | 不进入项目文件（会话级） | 同上 |

蒙版“像素内容”校验方式：对 `masks + manualMask + subjectMask + maskMeta` 的序列化结果做 SHA256
（`mask_digest`），逐轮比较 hash，而不是比较文件大小。

## 2. 测试流程（全部走正式保存/加载）

Rust（`src-tauri/tests/round9_1_persistence.rs`，真实临时目录）：

1. `save_load_save_preserves_every_category`：写入含全部类别的项目 → `load` → `save_with_meta` → 再次 `load` → 递归比较整棵树（键集合、数组长度、浮点容差）→ 再保存一次比较。
2. `twenty_roundtrips_are_identical`：连续 20 轮 `save → load`，逐轮比较快照、蒙版 hash、人脸 JSON、createdAt、projectUuid。
3. `partial_edit_does_not_touch_other_photos`：只改照片 A 曝光，照片 B 逐字段不变。
4. `legacy_project_without_identity_gets_uuid_and_keeps_created_at`：旧项目缺 `projectUuid/revision` 时首次保存生成 UUID、createdAt 不变。
5. `stale_revision_and_foreign_project_are_rejected`、`write_failure_keeps_original_and_leaves_no_partial_file`、`corrupt_file_is_rejected_without_touching_backup`、`project_version_newer_than_supported_is_rejected`：见 `ROUND9_1_FAILURE_INJECTION.md`。
6. `save_and_load_performance`：2/8/50 张照片的保存与加载耗时。

前端（`src/state/persistence.test.ts`，覆盖“前端重建式保存”这一丢失面）：

- 打开→保存→重开：元数据、蒙版、源文件元数据、未知字段、人脸稠密点全部保留。
- 打开→不改→保存→重开：全部参数类别与磁盘一致（浮点 ±1e-6）。
- 改一项→保存→重开：目标参数变化，其余数据不动。
- 保存→再保存→重开：第二次保存不改变数据，只递增 revision。
- 连续 20 次：参数、人脸、蒙版、UUID、createdAt 完全一致。
- 旧项目缺少新增字段：正常加载、默认元数据、不伪造 UUID。
- 未知字段在一次部分编辑后仍存在。
- 打开→不改→保存：活动照片的导出/色彩（含 `filenameTemplate` 等未知字段）与磁盘完全一致。
- 显式修改导出设置：只覆盖活动照片，未知字段与其它照片不受影响。
- 活动照片索引非 0：加载与保存使用该照片自己的色彩/导出。

## 3. 真实执行结果

```
$ cargo test --test round9_1_persistence
running 9 tests
test save_load_save_preserves_every_category ... ok
test twenty_roundtrips_are_identical ... ok
test partial_edit_does_not_touch_other_photos ... ok
test stale_revision_and_foreign_project_are_rejected ... ok
test write_failure_keeps_original_and_leaves_no_partial_file ... ok
test corrupt_file_is_rejected_without_touching_backup ... ok
test legacy_project_without_identity_gets_uuid_and_keeps_created_at ... ok
test project_version_newer_than_supported_is_rejected ... ok
test save_and_load_performance ... ok
test result: ok. 9 passed; 0 failed
```

```
$ npm test -- --run src/state/persistence.test.ts
 Test Files  1 passed (1)
      Tests  10 passed (10)
```

关键断言摘录：

| 断言 | 结果 |
| --- | --- |
| 20 轮后 `mask_digest` 与首轮一致 | 通过 |
| 20 轮后 `faces` JSON 与首轮逐字节一致 | 通过 |
| 20 轮后 `createdAt`/`projectUuid` 不变 | 通过 |
| 50 张照片项目保存后文件 1,499,759 B，读回 50 张 | 通过 |
| 未知字段 `futureDocField/futurePhotoField/futureParamField/futureFaceField` 在最终文件中存在 | 通过 |
| 非活动照片的 `color`/`export` 不被全局设置覆盖 | 通过 |
| 打开后不修改即保存：活动照片的 `color`/`export` 及其未知字段不变 | 通过 |
| 导出设置里的 `filenameTemplate` 等未知字段加载后仍在（Rust `ExportSettings.extra`） | 通过 |
| 蒙版层 `masks` 内容与引用（`maskRef`）保留 | 通过 |

## 4. 性能（debug 构建，单次测量）

| 项目 | 照片数 | 文件大小 | 保存 | 加载 |
| --- | --- | --- | --- | --- |
| small | 2 | 60,319 B | 15.99 ms | 1.88 ms |
| medium | 8 | 240,229 B | 22.51 ms | 6.83 ms |
| large | 50 | 1,499,759 B | 98.55 ms | 26.98 ms |

50 张照片（每张两人脸 + 85 点稠密 + 双蒙版）在 debug 下保存 < 100 ms；release 打包应用实测见 GUI 报告。

## 5. 局限

- 稠密关键点/门控**内部**的未知子字段仍是严格 schema（`DenseFace`/`Level3Gate` 未加 `extra`），
  新版在这些子对象里追加字段时旧客户端会忽略；文档级/照片级/参数级/人脸记录级已保留。列为下一轮改进项。
- 编辑历史与批量队列不持久化，属于明确产品规则，不是丢失。
- 图像像素不写入项目文件，导出像素一致性由渲染回归覆盖，不在本阶段范围。
