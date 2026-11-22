# ROUND 10 — 持久化验证报告

日期：2026-11-22
阶段：A（ChangeSet、并发、幂等、迁移）
基线：`c92d626`

## 1. 测试命令与结果

| 命令 | 结果 |
| --- | --- |
| `cd src-tauri && cargo test` | 462 passed / 0 failed / 7 ignored |
| `cd src-tauri && cargo clippy --all-targets -- -D warnings` | 通过，无 warning |
| `npm run typecheck` | 通过 |
| `npm test -- --run` | 8 个文件 / 81 tests passed |
| `npx vitest run src/state/changeset.test.ts` | 5 passed |

说明：

- Rust 测试全部在 `debug` profile 下运行，因此每条 ChangeSet 保存都会执行 A4 的
  磁盘回读往返断言；断言失败会直接让测试失败。
- 7 个 ignored 是既有的质量基准/需要显式 `--ignored` 的性能用例，本轮未改动。

## 2. 新增测试

### 2.1 Rust 单元测试（`image_engine/changeset.rs`，8 个）

- `patch_only_touches_declared_fields`：只改 `exposure`，`contrast`、`sourceHash`、
  照片级/参数级/文档级未知字段全部保持不变，`createdAt` 不被改写。
- `null_clears_optional_but_missing_leaves_value`：`metadata: null` 清空快照，
  未出现的 `hash` 保持原值。
- `unknown_patch_key_is_rejected`：`exposrue` 拼写错误 → `INVALID_OPERATION`，
  磁盘 revision 不变。
- `project_and_entity_ids_are_validated`：缺失照片 → `TARGET_NOT_FOUND`；
  错误 `projectId` → `PROJECT_MISMATCH`。
- `stale_revision_is_rejected_without_writing`：旧 revision → `REVISION_CONFLICT`，
  返回 `currentRevision`，磁盘参数未被修改。
- `repeated_request_is_idempotent`：同一 requestId 重放不重复新增蒙版，
  revision 不递增。
- `removal_must_be_explicit`：对不存在蒙版的 update/remove 均失败且不写盘。
- `batch_operation_applies_same_patch`：批量补丁生效。

### 2.2 Rust 集成测试（`tests/round10_changeset.rs`，11 个，真实磁盘）

| 用例 | 覆盖的任务书场景 |
| --- | --- |
| `concurrent_writers_do_not_lose_updates` | 两个编辑窗口同时保存：8 线程同基准并发写不同参数，冲突方重读 revision 后重试，最终 8 个字段全部保留 |
| `same_base_revision_second_save_is_rejected` | 旧 revision 保存：第二次被拒绝，旧快照未覆盖新值 |
| `response_lost_retry_is_idempotent` | IPC 超时重试 / 保存成功但前端未收到响应：`duplicate=true`，蒙版不重复 |
| `same_request_id_with_different_operations_is_not_treated_as_replay` | requestId 复用但内容不同 → 不误判为重放 |
| `switching_projects_cannot_cross_write` | 保存 A 后切到 B：跨项目写入被拒绝 |
| `rapid_edits_on_same_photo_keep_latest_value_and_increment_revision` | 同一照片快速连续修改 20 次：最终值正确，revision 单调递增 |
| `failed_operation_keeps_original_file_untouched` | 部分操作失败：文件字节完全不变（无半提交） |
| `changeset_roundtrip_preserves_masks_faces_and_unknown_fields` | 蒙版点坐标、人脸记录 `maskRef`、来源 hash、未知字段往返一致 |
| `rebase_reapplies_increments_on_latest_state_without_overwriting` | 冲突时重新读取最新状态再合并：他人字段保留 |
| `migration_keeps_unknown_fields_dense_raw_and_color` | v1 项目迁移：未知字段、稠密关键点（topology）、RAW decoderVersion、ICC profile 全部保留；生成迁移日志；磁盘文件不变 |
| `higher_version_project_is_rejected` | 版本 99 项目拒绝打开 |

### 2.3 前端测试（`src/state/changeset.test.ts`，5 个）

- 只输出变化字段（修改 `exposure` 后补丁仅含 `exposure`）；
- 无修改时 `buildChangeSet` 返回 `null`；
- `buildFieldPatch` 保留显式 `null`，忽略未变化字段；
- 浮点容差 `1e-4`，数组整体替换；
- 嵌套对象只提交变化分支。

前端还新增了“打开后无编辑 → 不产生补丁”的回归，修复过程中发现
`clampParams` 归一化（如背景/弯曲参数）会被误判为修改，基线现改用
`clampParams(prior.params)` 后再比较。

## 3. 关键验证点对照

| 任务书要求 | 实现 | 证据 |
| --- | --- | --- |
| 只更新明确修改的字段 | 补丁仅含变化键 | 单元 `patch_only_touches_declared_fields` |
| 未出现字段保留 | deep merge，缺失不重置 | 同上 |
| 删除必须显式 | Add/Remove 操作 | `removal_must_be_explicit` |
| null/缺失严格区分 | `null` 仅对 `Option` 合法 | `null_clears_optional...` |
| 未知字段保留 | 直接编辑磁盘 JSON 树 | 往返 + 迁移用例 |
| 校验项目 ID | `PROJECT_MISMATCH` | `switching_projects...` |
| 校验实体 UUID | `TARGET_NOT_FOUND` / UUID 唯一性 | 单元 + 集成 |
| 确定执行顺序 | `operations` 数组顺序 | 全部用例 |
| 重复请求不重复破坏 | requestId+指纹 token，先于 revision 检查 | `response_lost_retry...` |
| revision 冲突保护 | 默认拒绝；可选 rebase | `same_base_revision...` / `rebase...` |
| 禁止旧快照覆盖 | 增量合并；旧快照不参与 | `same_base_revision...` |
| 整包原子提交 | 单次 `write_atomically` | `failed_operation...` |
| 成功后递增 revision | `disk + 1` | `rapid_edits...` |
| 响应含最新 revision | `ProjectSaved.revision` | 前端保存流程 |
| Debug 往返断言 | `debug_assertions` 回读比较 | 所有 Rust 测试均在 debug 下执行 |
| Release 不做全量比较 | `#[cfg(debug_assertions)]` | 代码检查 |
| 迁移日志 | `migration_log` + 日志文件 | migration 用例 |
| 迁移不重置 RAW/ICC/稠密点 | 无删除步骤 | migration 用例 |

## 4. 已知限制

1. 照片目标使用 `path` 而不是 UUID（现有数据模型没有照片 UUID）。
2. 首次保存新项目仍为整包写入（创建文件），之后切换为 ChangeSet。
3. 补丁键校验只到目标子树顶层；更深的未知键会作为数据保留，不会影响其他字段。
4. 没有跨进程文件锁，只保证单应用内串行 + revision 冲突保护。
5. rebase 是“增量重放”，若两个窗口修改同一字段，后保存者覆盖该字段值
   （revision 递增，不丢失其他字段）；UI 尚不提示“字段级冲突”。

## 5. 结论

阶段 A 的 A1–A6 全部落地，并通过并发、幂等、原子性与迁移回归。
持久化路径具备可验证的“不静默丢数据”保证，允许进入阶段 B。

工作区在提交前包含本轮代码与文档改动，提交后应为干净状态。
