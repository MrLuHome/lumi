# ROUND 9.1 阶段 0：项目存储与保存链路审计

> 基线：`9923436ff96a193e1ead4ee43f069f71bdeae059`（分支 `main`，审计开始时工作区干净）
> 性质：先查不改。本文记录审计事实；修复见阶段 3 与 `ROUND9_1_SAVE_REGRESSION.md`。
> 存储形态：单个 JSON 项目文件（`.lumi`），不是 SQLite/数据库表。

## 1. 当前存储结构

项目文件由 Rust `ProjectDoc`（`src-tauri/src/image_engine/project.rs`）序列化，`version = 3`：

| 层级 | 结构 | 关键字段 |
| --- | --- | --- |
| 文档 | `ProjectDoc` | `version`、`createdAt`、`activeIndex`、`export`、`compute`、`photos` |
| 照片 | `ProjectPhoto` | `path`、`params`、`masks`、`export`、`sourceHash`、`faces`、`subjects`、`maskMeta`、`source`、`raw`、`color` |
| 参数 | `EditParams` | 曝光/对比/高光/阴影、HSL、曲线、LUT、美肤 `faces[]`、塑形 `reshape[]`、身材 `body[]`、背景 `background`、蒙版 `manualMask`/`subjectMask`、`repairPoints` |
| 人脸 | `ProjectFace` | `uuid`、`bbox`、`landmarks`、`score`、`model`、`confidence`、`maskRef`、`dense`、`level3` |
| 稠密点 | `DenseFace` | `topologyId`、`providerId`、`providerVersion`、`regions[]`（含每点 `semanticId`） |
| 级联 | `Level3Gate` | `providerId`、`topologyId`、`denseLandmarks`、`eyes/nose/mouth/chin` 门控 |
| 蒙版层 | `masks[]` | 旧版可重放蒙版（`id`、`kind`、`enabled`、`feather`、`points`） |
| 元数据 | `maskMeta` | `source`、`modelId`、`coverage`、`strokeHash`、`revision` |
| 源文件 | `source` | `sourceType`、`path`、`hash`、`metadata`（相机/文件信息） |
| RAW | `raw` | 显影参数、`decoderVersion` |
| 色彩 | `color` | 输出 Profile、意图、黑点补偿、嵌入 |
| 未知字段 | 各层 `extra` | 本轮新增的保留区（见阶段 3） |

## 2. 加载调用链

```
用户点击「打开项目」
  └─ src/App.tsx handleOpenProject
      ├─ backend.openProject()                    (Tauri invoke)
      ├─ Rust commands/project.rs open_project
      │   └─ image_engine/project.rs load(path)
      │       ├─ 读取文件、版本检查（> PROJECT_VERSION 拒绝）
      │       └─ serde 反序列化 ProjectDoc
      ├─ backend.importPaths(photo.paths)         → 源图导入/缓存
      ├─ state/project.ts computeProjectId(路径 + 文档)
      ├─ state/project.ts buildProjectSnapshot(item, doc, projectId)
      │   ├─ clampParams(entry.params)            → 非法参数收敛
      │   ├─ projectFaceRecords[photo.id] = entry.faces
      │   ├─ projectSourceHashes[photo.id] = entry.sourceHash
      │   └─ scanPhotoIssues → warnings / fatal
      ├─ editorStore.commitProjectSnapshot(requestId, snapshot)
      │   └─ 校验 requestId + fatal → 一次性提交（原子）
      ├─ editorStore.beginFaceRestore(requestId)
      ├─ 每张照片：detectFaces → setDetectedFaces
      │            restoreFaces → applyFaceRestore（按 UUID 写回）
      └─ editorStore.finishProjectLoad(requestId) → phase = ready
```

UI 只在 `phase === "ready"` 时允许编辑/保存；`loading/restoring` 期间的 `updateParam` 为 no-op。

## 3. 保存调用链

```
用户点击「保存项目」/⌘S
  └─ src/App.tsx handleSaveProject
      ├─ 守卫：photos 非空且 loadSession.phase === "ready"
      ├─ state/project.ts buildProjectDoc(state)
      │   ├─ 每张照片：params ← cloneParams(store) （+ 未知字段合并）
      │   ├─ masks ← 打开时的磁盘记录
      │   ├─ source.metadata ← 打开时的磁盘记录
      │   ├─ export/color ← 活动照片用会话值，其余保留磁盘值
      │   ├─ faces ← store.projectFaceRecords
      │   └─ createdAt/projectUuid/revision ← store.projectMeta
      └─ backend.saveProject(doc, destination)
          └─ Rust commands/project.rs save_project
              ├─ 解析目标路径（已有项目沿用，否则另存为）
              ├─ FaceRepository 快照 → merge_saved_faces(磁盘记录, 会话人脸)
              ├─ image_engine/project.rs save_with_meta
              │   ├─ 版本检查 / projectUuid 校验 / revision 比较
              │   ├─ sync_subject_records（派生 subjects / maskMeta）
              │   ├─ 临时文件 → write_all → fsync → 回读校验
              │   ├─ 复制 `.lumi.bak`（失败则中止）
              │   ├─ 原子 rename → 目录 fsync → 最终回读校验
              └─ 返回 path/bytes/projectUuid/revision → 前端回写 projectMeta
```

## 4. 每类数据的写入入口

| 数据类别 | 唯一写入入口 | 覆盖方式 | 审计结论 |
| --- | --- | --- | --- |
| 项目元数据 | `buildProjectDoc` | 前端整体构建 | **曾丢失**：`createdAt` 每次保存被重置；无 `projectUuid/revision` |
| 照片顺序/路径 | `buildProjectDoc` | 前端整体构建 | 正常 |
| 基础调色/HSL/曲线/LUT | `EditParams` | 前端整体构建 | 正常，未知字段**曾丢失** |
| 美肤/塑形 | `EditParams.faces/reshape` | 前端整体构建 | 正常，faceId 由恢复写回 |
| 身材/背景/蒙版参数 | `EditParams.body/background/subjectMask/manualMask` | 前端整体构建 | 正常 |
| 蒙版层 `masks` | `buildProjectDoc` | 前端整体构建 | **曾丢失**：无条件写 `[]` |
| 源图哈希 | `projectSourceHashes` | 前端整体构建 | 正常（复用缓存，不重算） |
| 源文件元数据 | `buildProjectDoc` | 前端整体构建 | **曾丢失**：无条件写 `null` |
| 人脸记录 | `merge_saved_faces` | Rust 按 UUID 合并 | R9-SAVE-01 已修复；未知字段/`maskRef` 仍**曾丢失** |
| 稠密关键点/LEVEL 3 | `merge_saved_faces` + `ProjectFace` | Rust 成对保留 | 已修复；无 UUID 记录**曾可重复追加** |
| 蒙版元数据 | `sync_subject_records` | Rust 派生覆盖 | **曾丢失**：`coverage`/`revision` 每次重置为 0/1 |
| RAW 显影参数 | `buildProjectDoc` | 前端整体构建 | 参数保留；`decoderVersion` **曾丢失** |
| ICC/色彩输出 | `buildProjectDoc` | 前端整体构建 | **曾丢失**：所有照片被写成同一个全局设置 |
| 每张照片导出设置 | `buildProjectDoc` | 前端整体构建 | **曾丢失**：被全局导出设置覆盖 |
| 编辑历史 | 仅内存（`history`） | 不持久化 | 产品规则见阶段 2 D |
| 批量队列 | 仅内存 | 不持久化 | 现状；不在项目文件内 |
| 未知字段 | `extra`（本轮新增） | 原样保留 | **曾静默丢弃** |

## 5. 数据丢失风险分级

| 级别 | 风险 | 触发条件 | 用户可见性 |
| --- | --- | --- | --- |
| P0 | 打开→保存删除稠密关键点 | 已修复（R9-SAVE-01） | 重开提示「需重新分析」 |
| P0 | 蒙版层/源文件元数据每次保存被清空 | 任意保存 | 不可见 |
| P0 | 未知字段被下一次保存删除 | 新版本写过的项目 | 不可见，降级后完全丢失 |
| P1 | `createdAt` 每次保存被重置 | 任意保存 | 不可见 |
| P1 | 非活动照片的 ICC/导出设置被全局值覆盖 | 多照片项目保存 | 预览/导出结果变化 |
| P0 | 活动照片的 ICC/导出设置被会话默认值覆盖（打开后不改直接保存也会发生） | 多照片项目保存 | 导出格式/质量/文件名模板被改，且无提示 |
| P0 | `ExportSettings` 内的未知字段（如 `filenameTemplate`）反序列化后被丢弃 | 任意保存 | 旧/新版本导出字段消失 |
| P1 | `maskMeta.coverage/revision` 每次归零 | 任意保存 | 覆盖率显示/缓存失效 |
| P2 | 无人脸照片被写入空 `maskMeta` | 任意保存 | 无 |
| P2 | 无 UUID 旧人脸记录随保存次数增长 | 连续保存旧项目 | 人脸列表重复 |
| P2 | 并发保存/A-B 串写无保护 | 双窗口或快速切换 | 可能覆盖更新内容 |
| P2 | 保存中断无 fsync/校验 | 断电/磁盘满 | 可能出现截断文件 |

## 6. 本轮修改文件清单

- `src-tauri/src/image_engine/project.rs`：`projectUuid`/`revision`/`updatedAt`、未知字段 `extra`、原子写（fsync/校验/备份失败中止/临时文件清理）、旧快照与跨项目保护、`sync_subject_records` 保留覆盖率与不伪造空元数据。
- `src-tauri/src/image_engine/export.rs`：`ExportSettings.extra` 未知字段保留（移除 `Copy` 派生，调用点改为借用/克隆）。
- `src-tauri/src/commands/batch.rs`：适配 `ExportSettings` 不再 `Copy`（克隆设置、测试字面量补 `extra`）。
- `src/components/ColorPanel.tsx`：色彩面板以项目设置为事实来源，不再被后端默认状态覆盖。
- `src-tauri/src/face_reshape/identity.rs`：`ProjectFace.extra` 未知人脸字段保留。
- `src-tauri/src/commands/project.rs`：`save_with_meta` 返回身份/代数；`merge_saved_faces` 保留 `maskRef`/未知字段；无 UUID 记录内容判重；新增 4 个回归用例。
- `src-tauri/tests/round9_1_persistence.rs`（新增）：9 个真实磁盘等价性/故障注入用例。
- `src/types.ts`、`src/state/project.ts`、`src/state/editorStore.ts`、`src/lib/backend.ts`、`src/App.tsx`：快照保留原始照片记录、`projectMeta`、保存后回写、`createdAt` 稳定、未知字段合并；色彩/导出按活动照片加载并带脏标记。
- `src/state/persistence.test.ts`（新增）：10 个前端持久化等价性用例。
- 文档：`docs/ROUND9_1_*.md`。

## 7. 审计结论

1. 项目文件是单文件 JSON，**没有数据库事务**；等价的“事务”只能通过临时文件 + fsync + 校验 + 原子 rename 实现（本轮已补全）。
2. 主要丢失面在前端 `buildProjectDoc` 的“重建式保存”，其次是 Rust `save_project` 的整体覆盖；两者都已在阶段 3 修复。
3. 自动保存功能当前不存在，因此没有“加载期自动保存写默认值”的路径；保存入口仍加了 `phase === ready` 守卫。
4. 加载失败不会回退为空项目：版本过高/JSON 损坏直接报错，前端进入 `failed`。
