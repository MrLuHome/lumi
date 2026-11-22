# ROUND 10 — ChangeSet 保存架构设计

日期：2026-11-22
阶段：A（保存架构加固，P0）
基线：`c92d626`（ROUND 9.1 报告提交）

## 1. 问题与目标

ROUND 9.1 的保存仍是“前端重建整包 `ProjectDoc` → 后端整体覆盖”。虽然已通过
`projectUuid` + `revision` 拒绝旧快照与跨项目写入，并保留未知字段，但存在结构性风险：

- 前端每次保存都要重新拼装完整文档，任何拼装遗漏都会变成静默数据丢失；
- 删除和修改没有区别，无法证明“只改了明确声明的字段”；
- 保存失败与成功之间缺少请求级幂等，IPC 超时重试可能重复执行破坏性操作；
- 没有 Debug 构建下的持久化往返断言。

阶段 A 引入类型化 ChangeSet，同时**保留整包保存作为新项目首次写文件的路径**，
不重写既有架构。

## 2. 协议

```rust
ChangeSet {
    project_id: String,       // 必须等于磁盘 projectUuid
    base_revision: u64,       // 前端读到的代数
    request_id: String,       // 幂等键（非空）
    operations: Vec<Operation>,
    created_at: String,
    rebase_on_conflict: bool, // 冲突时在最新状态上重放增量
}
```

提交入口：`apply_project_changeset(path, changeset)` →
`image_engine::changeset::apply`。

前端入口：`src/state/project.ts::buildChangeSet(state, requestId)`，
`App.handleSaveProject` 在项目已有 `projectPath + projectUuid` 时使用；
新项目首次保存仍走 `save_project` 整包写入，成功后调用
`setPersistenceBaseline` 把写盘结果作为后续增量基线。

## 3. 操作类型（适配现有数据模型）

| 操作 | 目标 | 说明 |
| --- | --- | --- |
| `updateDocMeta` | 项目级 `activeIndex/export/compute` | 只允许这三个键 |
| `updatePhotoParams` | `photos[i].params` | 排除 `faces/subjectMask/manualMask/background` |
| `updateFaceParams` | `params.faces[]`（按 `faceUuid`） | 美肤参数补丁 |
| `updateReshapeParams` | `params.reshape[]`（按 `faceUuid`） | 五官塑形补丁 |
| `updateSubjectMask` | `params.subjectMask` | 主体蒙版参数 |
| `updateManualMask` | `params.manualMask` | 手动蒙版参数 |
| `updateBackground` | `params.background` | 背景参数 |
| `updateRawParams` | `photos[i].raw.params` | RAW 不存在时 `TARGET_NOT_FOUND` |
| `updateColorParams` | `photos[i].color.output` | ICC/色彩输出 |
| `updateExportParams` | `photos[i].export` 或项目 `export` | `photo` 为空表示项目级 |
| `addFace` / `removeFace` | `params.faces[]` | 新增/删除必须显式，UUID 唯一 |
| `addMask` / `updateMask` / `removeMask` | `photos[i].masks[]` | 蒙版层按 `id` |
| `upsertFaceRecord` / `removeFaceRecord` | `photos[i].faces[]` | 持久化人脸记录按 `uuid` |
| `addPhoto` | `photos[]` | 路径不可重复 |
| `updateSource` | `photos[i].source` | `metadata: null` 表示清空快照 |
| `updateBatchParams` | 多张照片的 `params` | 同一补丁应用到指定路径 |

未实现 `UpdatePreset`：当前项目文档不包含预设数据，预设由独立模块管理，
把它塞进 ChangeSet 会制造重复数据源。

## 4. 字段语义

- **缺失键 = 不修改**：补丁只覆盖出现的键。
- **`null` = 显式置空**：合法仅限 `Option` 字段（如 `source.metadata`、`lut`）；
  对非 `Option` 字段写 `null` 会在反序列化阶段失败并整包回滚。
- **未知补丁键 = 拒绝**：每个操作按目标类型的默认序列化结果生成允许键集合，
  拼写错误返回 `INVALID_OPERATION`，不会悄悄写进项目。
- **数组 = 整体替换**：`curves`、`points` 等数组按声明值替换；新增/删除实体
  （人脸、蒙版、人脸记录、照片）必须使用显式 Add/Remove 操作。
- **未知磁盘字段 = 保留**：操作直接作用在磁盘 JSON 树上，未触碰的子树（包括
  本版本不认识的字段、AI 扩展数据）原样写回。
- **派生字段**：`subjects` 在提交前由 `sync_subject_records` 从 `params` 重建，
  不接受前端直接写入。

## 5. 目标实体标识

- 项目：`projectUuid`，缺失时采用请求中的 `projectId`（旧项目），
  不匹配 → `PROJECT_MISMATCH`，绝不写入。
- 照片：`photos[].path`（现有模型没有照片 UUID，路径是唯一稳定标识）。
- 人脸：`params.faces[].faceUuid` / `photos[].faces[].uuid`。
- 蒙版：`masks[].id`。
- 任一目标不存在 → `TARGET_NOT_FOUND`，整包不提交。

## 6. 并发与冲突

- 所有保存（整包与 ChangeSet）共用 `project::lock_saves()`，同一时刻只有一个写入事务。
- 加锁后重新读取磁盘 JSON，再校验 `projectUuid` 与 `revision`。
- `base_revision != disk_revision`：
  - 默认返回 `REVISION_CONFLICT`，响应携带 `currentRevision`；
  - `rebaseOnConflict = true` 时，在**刚读取的最新状态**上重新套用增量操作。
    旧快照不参与合并，因此不会覆盖其他窗口写入的字段。
- 前端默认开启 rebase；若未来需要严格人工确认，可关闭该标志。

## 7. 原子提交

`changeset::apply` 的执行顺序：

1. 加锁 → 读取磁盘 JSON；
2. 校验项目身份、幂等 token、revision；
3. 按数组顺序应用全部操作（任一步失败立即返回，未写盘）；
4. 更新 `revision = disk + 1`、`updatedAt`，保留 `createdAt`；
5. 反序列化为 `ProjectDoc` 做结构校验，同步 `subjects` 并写回 JSON 树；
6. 一次 `serde_json::to_vec_pretty` → `write_atomically`（临时文件 → fsync →
   回读 → `.bak` → rename → 目录 fsync → 最终回读）；
7. Debug 构建执行往返断言；
8. 返回 `{ projectUuid, revision, duplicate }`。

失败路径不会执行第 6 步，因此不存在“提交一半”。

## 8. 幂等

- 幂等 token = `requestId` + 操作内容 SHA-256 前 8 字节。
- 项目文件保存最近 64 个 token（`lumiAppliedRequests`，属于未知字段，旧版本会原样保留）。
- 相同 token 重放：直接返回当前 `revision` 与 `duplicate: true`，不执行操作，
  **且在 revision 冲突检查之前判定**——覆盖“保存成功但响应丢失”的 IPC 重试。
- 相同 `requestId` 但操作不同：不视为重放，避免掩盖客户端 bug。

## 9. Debug 往返断言（A4）

仅在 `debug_assertions` 下执行：

```
目标状态（内存 typed doc）→ 写盘 → 重新读取 → 反序列化 → 规范化比较
```

- 递归比较序列化后的 JSON 树；
- 数字容差 `1e-6`；
- UUID、topology、模型版本、蒙版 `strokeHash`、未知字段全部严格相等
  （字符串/对象键集合精确比较）；
- 差异输出字段路径与 `projectUuid`、`revision`，不输出照片内容或个人信息；
- 一旦有差异，命令返回 `STORAGE_ERROR`，不会把保存标记为成功。
- Release 构建不执行全量比较（`#[cfg(debug_assertions)]`）。

## 10. 迁移（A6）

- `project::load_with_report` 返回 `migration_log`，命令层透传到前端并在打开项目时提示；
  同时追加到 `~/Library/Logs/LumiRetouch/migration.log`。
- 迁移只“补默认值”，不删除字段：
  - 未知字段原样保留；
  - 不支持的 AI 数据（未知结构）原样保留；
  - 稠密关键点不清空；
  - RAW 解码器版本与 ICC/色彩配置不重置；
  - 迁移不修改磁盘文件。
- 版本高于当前支持（`> PROJECT_VERSION`）直接拒绝打开。
- 迁移失败（解析/结构错误）返回错误，原文件保持不变。

## 11. 未覆盖与后续

- 首次保存（新项目）仍是整包写入：此时磁盘上没有可合并的基线，属于创建而非覆盖。
- 补丁键校验只覆盖目标子树的顶层键；更深的拼写错误会落进 `extra` 并在 Debug
  往返断言中保持稳定，不会破坏其他字段。
- 跨进程并发（例如外部编辑器同时改文件）仍只靠 revision 与原子写防护，
  没有文件锁；单应用内已由 `SAVE_LOCK` 串行化。
