# ROUND 9 阶段 1：项目加载链路专项审计（先查不改）

> 基线：`fd4512a`（父 `a9970dc`，第八轮 `479416f`）
> 性质：只读审计。本文件不代表已修复，修复方案见第 6 节，落地在阶段 2。
> 结论标记：`确认` = 有代码路径与数据证据；`高置信推断` = 代码路径成立但未在打包应用复现；
> `未确认` = 未找到证据，不编造。

## 1. 审计目标

复现并解释第八轮 GUI 验收记录的缺陷：

> 保存项目后重开，首次重开人脸参数（skinSmooth 17 / texture 33 / spots 11）曾短暂显示 0，第二次重开正常。

要求：不得用延迟渲染、隐藏 UI 或补默认值掩盖；必须区分加载状态与 UI 状态。

## 2. 完整调用链

```
[用户点击 打开项目]
  src/App.tsx  handleOpenProject()
    │
    ├─ 1. openProject()                      src/lib/backend.ts:189
    │      invoke("open_project")
    │        └─ src-tauri/src/commands/project.rs:97 open_project
    │             └─ src-tauri/src/image_engine/project.rs:load()
    │                  ├─ std::fs::metadata / read（上限 64MB）
    │                  ├─ serde_json::from_slice::<ProjectDoc>  // 结构错误直接 Err
    │                  └─ 版本 & 空照片校验
    │
    ├─ 2. importPaths(paths)                  src/lib/backend.ts:67
    │      invoke("load_images")
    │        └─ src-tauri/src/commands/library.rs:89 load_images
    │             └─ 解码 + 缩略图；PhotoAsset.id = 绝对路径
    │
    ├─ 3. store.loadProject(items, project)   src/state/editorStore.ts:810
    │      ├─ 先为每个 item 写入 DEFAULT_PARAMS（第 820 行）
    │      ├─ 再按 entry.path 覆盖为 clampParams(entry.params)（第 826 行）
    │      └─ 一次性 set(...) 提交 photos/params/history/faces 等
    │
    ├─ 4. store.setProjectPath(opened.path)   src/state/editorStore.ts:869
    │
    └─ 5. 逐张照片异步恢复（串行 await）
           for item of items:
             detectFaces(item.path)              src/lib/backend.ts:211
               └─ src-tauri/src/commands/faces.rs:53 detect_faces
                    ├─ engine.detect(...)
                    ├─ FaceFrame::from_detected（uuid = ""）
                    └─ FaceRepository::store → FaceTracker::assign
                         前一轮为空时 → new_face_uuid()（全新会话 UUID）
             setFaces(item.id, detection)         src/state/editorStore.ts:1050
               └─ 按 uuid / faceId 合并；都不匹配 → defaultFaceBeauty / defaultFaceReshape
             restoreFaces(item.path, records, hash)  src/lib/backend.ts:318
               └─ src-tauri/src/commands/reshape.rs:108 restore_faces
                    ├─ identity::match_faces → restored[].faceId = 检测期 frame.id
                    ├─ repo.apply_identities → face.id = id_from_uuid(项目 UUID)
                    └─ 返回 faces（此时 id 已被改写）
             applyFaceRestore(item.id, restored)  src/state/editorStore.ts:1086
               └─ restored.get(face.id)  // face.id 已是改写后的 id
                  未命中 → defaultFaceBeauty / defaultFaceReshape
```

## 3. 十类原因逐一排查

| # | 可能原因 | 结论 | 证据 |
| --- | --- | --- | --- |
| ① | 前端先用默认值初始化后收到数据库参数 | **部分成立** | `loadProject` 先为全部 item 写 `DEFAULT_PARAMS`，再按路径覆盖；路径不一致或字段被过滤时静默保留 0 |
| ② | 加载期组件重挂载导致默认值覆盖 | 未确认 | `Inspector`/`AiPortraitPanel` 不持有参数的本地副本，无 `key` 强制重挂载；未找到覆盖路径。阶段 3 仍需回归验证 |
| ③ | 两次加载并发、旧请求晚返回覆盖 | **成立（风险）** | `handleOpenProject` 无 requestId/项目身份守卫；`applyFaceRestore` 仅按 `photoId`（= 路径）写入，跨项目同图会串数据 |
| ④ | 自动保存在加载完成前把默认值写入 | **成立（用户触发路径）** | 当前无自动保存定时器；但 Cmd+S/保存按钮在 Restoring 期间完全可用，会把内存中的 0 写盘 |
| ⑤ | 反序列化失败静默 Default | **部分成立** | `ProjectDoc` 用 serde 严格报错（正确）；但 `EditParams` 内缺失/非法字段经 `clampParams` 静默归 0，无错误码与结构化日志 |
| ⑥ | SQLite 读取与前端状态提交竞争 | 不适用 | 项目为 JSON 文件 + `rename` 原子写，无 SQLite |
| ⑦ | StrictMode / effect 重复执行 | 未发现 | 加载由用户操作触发；`main.tsx` 的 StrictMode 不重复执行该路径 |
| ⑧ | 切换后旧项目异步回调仍写全局状态 | **成立** | 同 ③；第 5 步是串行 `await`，切换项目后仍会继续执行 |
| ⑨ | 人脸参数晚于面板渲染 | **确认（根因）** | 见第 4 节：`setFaces` + `applyFaceRestore` 在提交后二次改写 `params.faces`，并以默认值覆盖 |
| ⑩ | 加载失败的错误回退覆盖原始数据 | 部分成立 | 单张 `detectFaces` 失败被 `catch {}` 吞掉；项目解析失败会报错，但没有 Failed 状态与恢复入口 |

## 4. 根因（确认）

### 4.1 证据链

以实际保存的项目文件 `~/Documents/lumi-project-1.lumi` 为样本：

```json
photos[0].params.faces[0] = {
  "faceId": 1700055149,
  "faceUuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19",
  "beauty": { "skinSmooth": 17, "texture": 33, "spots": 11, ... }
}
photos[0].faces[0] = { "uuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19", "model": "yunet-5" }
```

即：磁盘数据正确，参数与人脸记录 UUID 一致。问题发生在前端内存合并阶段。

第 1 步：`loadProject` 正确提交了 17/33/11（`clampParams` 保留 `beauty` 子对象）。

第 2 步：全新应用会话中 `FaceRepository` 为空。`FaceTracker::assign` 对没有前一轮匹配的人脸调用 `new_face_uuid()`（`src-tauri/src/ai_engine/face_frame.rs:181-190`），因此 `detect_faces` 返回：
- `uuid` = 本次会话新 UUID（≠ 项目 UUID）
- `id` = `id_from_uuid(新 UUID)`（≠ 项目 `faceId`）

第 3 步：`setFaces`（`src/state/editorStore.ts:1054-1069`）先按 `faceUuid` 匹配，再按 `faceId` 匹配，两者都不命中，于是写入 `defaultFaceBeauty(face.id, face.uuid)` / `defaultFaceReshape(...)`，即**全 0**，并把 `params.faces` 的项目 UUID 覆盖成新 UUID。

第 4 步：`restore_faces` 用几何高置信匹配成功，但 `FaceMatch.face_id` 取的是**改写前**的检测 id（`src-tauri/src/face_reshape/identity.rs:288-294`）；随后 `apply_identities` 把仓库内 `face.id` 改写为 `id_from_uuid(项目 UUID)`（`src-tauri/src/state.rs:223-227`），返回的 `faces` 已是新 id。

第 5 步：`applyFaceRestore`（`src/state/editorStore.ts:1089-1115`）用 `restored.get(face.id)` 查表，键是检测期 id、`face.id` 是改写后 id，**查不到**；`matched` 为 `undefined`，再次落回默认值。同时它读取的 `current.faces` 已被第 3 步污染。

第 6 步：此时界面已是可编辑状态，`params.faces` 为 0。用户一旦保存，`buildProjectDoc`（`src/state/project.ts:23`）会把 0 写入项目文件——**真实数据丢失**。

第 7 步（解释“第二次正常”）：同一会话第二次重开时，`FaceRepository` 中已由 `apply_identities` 写回项目 UUID 与新 id，`detect_faces` 返回项目 UUID/id，`setFaces` 与 `restored.get` 都能命中，参数恢复正确。

### 4.2 与观测现象的一致性

| 观测 | 解释 |
| --- | --- |
| 首次重开显示 0 | 全新会话 UUID 与项目 UUID 不同 → 两次默认值覆盖 |
| 第二次重开正常 | 仓库已持有项目 UUID → 匹配成功 |
| 重启应用后再次出现 | 仓库是进程内内存，重启即清空 → 必然可复现（待阶段 3 用自动化确认） |
| 参数“短暂”显示 0 | 恢复流程串行 await；面板先渲染被污染的 0，后续重开/重分析才纠正 |

### 4.3 为什么不是“刷新时序”

不是 React 渲染时序，也不是 StrictMode。`setFaces`/`applyFaceRestore` 是**有意的业务写入**，把 `defaultFaceBeauty` 当成“未匹配时的安全默认值”写进了全局参数。用延迟渲染或隐藏 UI 只会让 0 晚一点出现，无法消除写盘风险。

## 5. 其他已确认缺陷（同属 P0-A）

| ID | 缺陷 | 位置 | 影响 |
| --- | --- | --- | --- |
| R9-LOAD-02 | 无加载状态机；`loadProject` 提交即视为 Ready，人脸恢复仍在后台 | `editorStore.ts:810-867`、`App.tsx:handleOpenProject` | Restoring 期间可编辑、可保存、可切换 |
| R9-LOAD-03 | 无 requestId / projectId 守卫 | `App.tsx:120-200` | A 加载中切 B，A 的 detect/restore 回调覆盖 B |
| R9-LOAD-04 | `loadProject` 先写默认值再按路径覆盖，未匹配项静默保留 | `editorStore.ts:819-831` | 路径规范化差异 → 整张照片参数为 0 且无提示 |
| R9-LOAD-05 | `clampParams` 对缺失/非法字段静默归 0 | `params.ts:939-1030` | 损坏项目“看起来正常”，无错误码与日志 |
| R9-LOAD-06 | 无加载快照校验（项目 ID、源图 hash、人脸 UUID 与蒙版引用一致性） | `project.rs:load` | 无法在提交前拒绝不一致数据 |
| R9-LOAD-07 | 单张人脸检测失败被静默吞掉 | `App.tsx` `catch {}` | 无日志、无 Failed 状态 |

## 6. 最小修复方案（阶段 2 落地）

原则：不重写，复用现有 store；不引入新状态库。

1. 前端新增加载状态机 `Idle / Loading / Restoring / Ready / Failed / Closing`（放 `editorStore`），只有 `Ready` 允许编辑与保存。
2. `loadProject` 改为“快照 + 原子提交”：
   - 先构建完整 `params`，不预填默认值；按 `path` 精确匹配，未匹配照片进入 `missing` 列表；
   - 校验人脸记录 UUID 唯一、蒙版引用一致、源图 hash 存在；
   - 不合法字段给出 `loadError` 与日志，不静默归 0。
3. 拆分检测写回：
   - 新增 `setDetectedFaces(photoId, faces)`，只更新 `faces` 检测结果，不改 `params.faces`；
   - `applyFaceRestore` 改为按 `restored[recordIndex].faceUuid` 从**加载快照**合并，禁止用 `defaultFaceBeauty` 覆盖已有非零参数；无匹配时保留原值并进入 pending。
   - Rust 侧让 `FaceMatch` 额外返回 `final_face_id`（`apply_identities` 之后），消除 id 空间不一致。
4. 保存守卫：非 `Ready` 状态拒绝保存并提示；加载中切换项目/关窗时作废旧任务（requestId + AbortController 语义）。
5. 结构化日志：`load_started / database_loaded / params_restored / faces_restored / masks_restored / state_committed / ui_ready / load_failed / save_started / save_completed`，带 `project_id`、`request_id`、耗时、错误码、数据版本；不记录照片内容与隐私路径。

## 7. 修复前 / 修复后行为差异（预期）

| 场景 | 修复前 | 修复后 |
| --- | --- | --- |
| 冷启动后首次重开 | 人脸参数被写成 0，可继续保存 | 人脸参数保持磁盘值；恢复完成前只读 |
| 恢复期间按 Cmd+S | 保存全 0 参数 | 拒绝保存并提示“项目恢复中” |
| 路径不一致 / 字段损坏 | 静默 0，界面看似正常 | Failed + 明确原因 + 原文件不受影响 |
| A 加载中切 B | A 的回调可能覆盖 B | requestId 失配，A 的任务作废 |
| 20 次连续重开 | 依赖会话缓存，冷启动必现 0 | 每次一致，含冷启动 |

## 8. 未确认部分与进一步方案

- ②「组件重挂载」未找到证据，但阶段 3 的 20 次开关与并发测试会覆盖。
- “参数 0”在验收截图中已被后续操作纠正，未能在打包应用稳定复现为持续 0；本审计以代码路径 + 项目文件证据定位，阶段 3 需用自动化测试证明冷启动必现与修复后必不复现。
- 项目文件损坏时的 `serde` 错误文本会包含绝对路径（`AppError::Decode`），阶段 2 需脱敏。
