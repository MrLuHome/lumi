# ROUND 9 规格：数据完整性、ONNX 运行链路与发行准备

> 基线：`fd4512a`（父 `a9970dc`，第八轮 `479416f`）。不得重置或覆盖历史。
> 阶段 1 审计已完成并交付：[ROUND9_PROJECT_LOAD_AUDIT.md](../../ROUND9_PROJECT_LOAD_AUDIT.md)。
> 本规格是阶段 2 之后的实现约定；阶段 1 为只读审计，已完成。

## 1. 已确认事实（来自阶段 1）

- **R9-LOAD-01（P0，确认）**：冷启动后重开项目，`setFaces` 用会话新 UUID 匹配项目 UUID 失败，
  写入 `defaultFaceBeauty`（全 0）；随后 `applyFaceRestore` 因 `restored` 的 `faceId` 是
  `apply_identities` 改写前的检测 id，查表失败，再次写入默认值。内存参数被清零，保存即写盘。
  - `src/state/editorStore.ts:1050-1083`（setFaces）
  - `src/state/editorStore.ts:1086-1131`（applyFaceRestore）
  - `src-tauri/src/commands/reshape.rs:108-189`（restore_faces）
  - `src-tauri/src/face_reshape/identity.rs:288-294`（face_id = 检测期 id）
  - `src-tauri/src/state.rs:210-236`（apply_identities 改写 id）
  - `src-tauri/src/ai_engine/face_frame.rs:181-190`（空仓库 → new_face_uuid）
- 同一会话第二次重开正常，与“首次 0、第二次正常”的验收记录完全吻合。
- 无加载状态机、无 requestId/projectId 守卫、无保存禁区、`clampParams` 静默归 0。

## 2. 非目标

- 不重写项目模型，不引入 zustand 之外的状态库。
- 不重新引入 ONNX Runtime（继续 `tract-onnx`）。
- 不把 RuntimeTested 升级为 AccuracyVerified；不把 ProtocolTestOnly 写成真实推理。
- 不进入第十轮。

## 3. 阶段 2：状态机与完整性修复（决策完成）

### 3.1 状态机

在 `src/state/editorStore.ts` 增加：

```ts
export type ProjectLoadPhase = "idle" | "loading" | "restoring" | "ready" | "failed" | "closing";

interface LoadSession {
  requestId: number;      // 单调递增
  projectPath: string;
  projectId: string;      // 项目文件的稳定标识（见 3.3）
  phase: ProjectLoadPhase;
  error: { code: string; message: string } | null;
  missing: string[];      // 路径不存在的照片
  warnings: string[];
}
```

规则：
- 仅 `phase === "ready"` 允许 `updateParam/updateParams/commit/apply` 与保存。
- `loading/restoring/failed/closing` 下写入返回 no-op 并记录一次诊断日志（开发模式）。
- 切换项目或关窗：`requestId` 自增使旧任务失效；旧任务所有 `set` 前校验
  `state.loadSession.requestId === captured`，否则丢弃。
- `failed` 必须展示 `code` 与可执行恢复方式（重新选择文件 / 修复后重试 / 以只读打开），
  不得渲染成空项目。

新增 actions：
- `beginProjectLoad(path) -> requestId`
- `commitProjectSnapshot(requestId, snapshot)`
- `beginFaceRestore(requestId)`
- `finishProjectLoad(requestId)`
- `failProjectLoad(requestId, code, message)`
- `closeProject()`

### 3.2 原子快照

`loadProject` 改为两段式，不再预填 `DEFAULT_PARAMS`：

1. `buildProjectSnapshot(items, doc)`（新函数，放 `src/state/project.ts`）：
   - 按 `path` 精确匹配；未匹配的照片进入 `missing`，绝不保留默认参数；
   - 逐字段校验 `EditParams`：非有限数值、超范围、beauty 缺失 → 记录 `warnings` 并标记
     `paramsInvalid`，不静默归 0；`clampValue` 只做范围收敛，不做缺失填充；
   - 校验 `ProjectFace.uuid` 非空且唯一；`params.faces[].faceUuid` 必须能在
     `ProjectFace.uuid` 或 `""`（旧项目）中找到；不一致 → `warnings`，不覆盖参数；
   - 校验 `masks[]` 引用完整性（层 id 唯一，`points` 坐标有限）。
2. 校验通过后 `commitProjectSnapshot` 一次性提交 `photos/params/history/faces/raw/color`，
   `phase = "restoring"`；人脸恢复全部完成后 `finishProjectLoad` → `ready`。

### 3.3 projectId 与完整性摘要

- `projectId` = `sha256(projectPath + createdAt + photos[].sourceHash)` 的前 16 字节十六进制，
  纯前端计算即可；不重复计算图片 SHA256，复用 `sourceHash`。
- 保存前记录 `paramsDigest`（JSON 规范化后的 SHA-256），保存后回读校验摘要一致；
  不一致 → `save_completed` 标记 `verified: false` 并告警。

### 3.4 人脸恢复写回（核心修复）

Rust：
- `FaceMatch` 增加 `final_face_id: u32`，在 `restore_faces` 中于 `apply_identities` 之后回填。
- `FaceRestoreResult.faces` 保持不变。

前端：
- 新增 `setDetectedFaces(photoId, result)`：只写 `faces[photoId]`，**不改** `params.faces`。
- `applyFaceRestore(photoId, result)`：
  - 以 `result.restored[].faceUuid` 为唯一键，从 `loadSnapshot.params[photoId].faces`
    中查找原始参数；
  - 用 `final_face_id` 作为新 `faceId`；
  - 找不到时保留原参数并移入 `pendingFaceMatches`，**禁止** `defaultFaceBeauty` 覆盖非零参数；
  - 仅当原参数本身为空（新检测脸）时才创建默认项。
- `setFaces`（普通重新分析）保留现有语义，但在 `phase !== "ready"` 时不得调用。

### 3.5 保存与关窗

- `handleSaveProject`：`phase !== "ready"` → 提示并返回；
- `beforeunload` / 窗口关闭：`phase === "restoring"` 时等待当前 `spawn_blocking` 读完成或直接
  `closeProject()`；不得在 restore 未完成时触发保存。
- 原子写已存在（`project.rs:save` 临时文件 + rename），保留；增加写前备份
  `*.lumi.bak`（仅当目标已存在且校验通过），失败保留原文件。

### 3.6 验证（阶段 3 自动化）

前端 Vitest：
- 冷启动恢复：`loadProject` → `setDetectedFaces(新UUID)` → `applyFaceRestore(项目UUID)`，
  断言 `params.faces` 仍为 17/33/11；
- 20 次连续恢复，每次深比较；
- A 加载中切 B：A 的迟到回调不改变 B；
- 恢复中保存被拒绝；
- 缺字段/非法值/人脸记录缺失/蒙版缺失/旧版本/损坏文件六类异常快照测试。
Rust：
- `restore_faces` 返回 `final_face_id` 与新 id 一致；
- 项目 JSON roundtrip 与旧版本迁移。

## 4. 阶段 4-5：ONNX 正常推理链路

- 复用 `tract-onnx` 与现有 Worker（`--face-model-infer`）。
- 准备一个来源/许可清晰、可本地测试的模型；记录来源、版本、SHA256、输入尺寸与布局、
  归一化、输出布局、语义拓扑、是否真输出关键点。
- 若只能自建最小协议模型，标记 `ProtocolTestOnly`，不得写成真实人脸模型推理通过。
- 全链路日志：导入 → SHA256 → Manifest → 结构与算子 → Worker 启动 → 加载 → 预处理 →
  推理 → 输出校验 → 语义映射 → 坐标转换 → Provider → 前端。
- 异常矩阵与资源限制按任务书执行；平台无法提供的限制如实记录。
- 能力等级：`Imported / StructurallyValidated / ProtocolTested / RuntimeTested /
  AccuracyUnverified / AccuracyVerified`，RuntimeTested 不自动解锁 LEVEL 3。

## 5. 阶段 6：LibRaw 合规

- 实际版本、源码来源、许可选择、链接方式（静态/动态）、是否修改、补丁、构建脚本。
- 发行包材料：许可证文本、完整源码包、修改记录、重链接材料/目标文件、获取方式。
- 新增 `scripts/check_release_compliance.sh`（或 Rust 子命令）：逐项检查，缺材料返回
  `BLOCKED` 且退出码非 0；不得输出 Release Ready。
- 文档状态固定 `LEGAL_REVIEW_PENDING`，不得写 COMPLIANT。

## 6. 阶段 7：NME 评测

- ≥50 张有合法授权的照片，覆盖姿态/遮挡/光照/多人脸/分辨率。
- 统一 8 区域语义与 Semantic ID；预测与 GT 必须经显式语义映射，不按数组下标比较。
- 指标：Overall/Eye/Nose/Mouth/Contour NME、P50、P95、Failure Rate、分组报告；
  写明归一化分母与不可见点规则；门槛在看到结果前预置。
- 输出 `docs/ROUND9_NME_EVALUATION.md`、`docs/ROUND9_LANDMARK_ANNOTATION_GUIDE.md`。
- 授权无法取得 → `BLOCKED`，不得伪造指标。

## 7. 阶段 8-9：打包与最终审计

- `cargo test`、`cargo clippy --all-targets -- -D warnings`、`npm run typecheck`、
  `npm test -- --run`、`npm run build`、`npm run app:build`。
- macOS 实机验收；Windows 无环境则明确“未验证”，不写 PASS。
- 性能：冷/热启动、加载恢复、峰值内存、连续重开、Worker 启动/首载/热推理/连续推理、
  崩溃恢复时间，不只报最好一次。
- 产出：`ROUND9_REPORT.md`、`ROUND9_PROJECT_RECOVERY_TESTS.md`、
  `ROUND9_ONNX_RUNTIME_VALIDATION.md`、`ROUND9_LIBRAW_COMPLIANCE.md`、
  `ROUND9_NME_EVALUATION.md`、`ROUND9_LANDMARK_ANNOTATION_GUIDE.md`、
  `ROUND9_SECURITY_REVIEW.md`、`ROUND9_GUI_ACCEPTANCE.md`、`ROUND9_RELEASE_READINESS.md`。

## 8. 提交与回滚

- 每阶段一个可回滚提交，信息前缀 `fix(round9):` / `feat(round9):` / `docs(round9):`。
- 阶段 2 单独提交，便于只回滚修复不影响审计文档。
- 不 `reset --hard`，不改写已推送历史。

## 9. 验收门槛（需用户确认）

1. 阶段 3 的 20 次恢复与并发测试全部通过，含冷启动必现用例；
2. 恢复期间保存被拒绝且不产生半写入；
3. ONNX 正常推理链路至少达到 `RuntimeTested`（真实模型）或如实标 `ProtocolTestOnly`；
4. LibRaw 自动检查返回 `BLOCKED` 或材料齐全但法律状态仍 `LEGAL_REVIEW_PENDING`；
5. NME 有结果则给真实指标，无授权则 `BLOCKED`；
6. macOS 打包验收有截图与日志，Windows 未运行明确标注。
