# ROUND 10 阶段 0：基线审计与实施计划

审计时间：2026-11-22
工作目录：`/Users/lu/Documents/lumi人像`
分支：`main`

## 0. 结论摘要

- 本地基线提交与任务书一致：代码提交 `fa41a06bacaacf7bf729a6f85aea5c39d488c607`，报告提交 `c92d62631d8d1c0ed772857d0b5c0e73b062d51c`，后者是前者的直接子提交（仅新增 `docs/ROUND9_1_REPORT.md`）。
- 本地 `main` HEAD 为 `c92d626`，工作区干净，无未提交修改。
- 远端 `origin/main` HEAD 为 `fd4512a8cc23da66a98f0ed785a6f1fc1b575372`（第八轮 8.1 收尾），**落后本地 10 个提交**。第九轮与 9.1 的提交只存在于本地。本轮以本地 `c92d626` 为代码基线，不推送、不重置、不改写历史；发布前需确认远端同步策略（见风险 R-01）。
- 第九轮与 9.1 报告的关键声明与代码、测试一致：未知字段容器、`projectUuid + revision`、保存锁、原子写、人脸 UUID 合并、前端保留原始记录均存在；Rust 443 passed / 7 ignored，前端 76 passed，typecheck 通过。
- 现有架构是**单文件 JSON 项目**，没有 SQLite，没有自动保存，也没有窗口关闭拦截。第十轮阶段 A 的 ChangeSet 需要建立在现有 JSON 存储之上，而不是引入数据库。

## 1. 提交与工作区核验

```
$ git rev-parse HEAD
c92d62631d8d1c0ed772857d0b5c0e73b062d51c
$ git rev-parse fa41a06
fa41a06bacaacf7bf729a6f85aea5c39d488c607
$ git rev-parse c92d626
c92d62631d8d1c0ed772857d0b5c0e73b062d51c
$ git merge-base --is-ancestor fa41a06 c92d626   # 退出码 0
$ git status --short                            # 无输出
```

```
$ git ls-remote origin refs/heads/main
fd4512a8cc23da66a98f0ed785a6f1fc1b575372  refs/heads/main
$ git rev-list --left-right --count origin/main...HEAD
0	10
```

- 本地领先远端 10 个提交，远端领先本地 0 个。
- 远端 main 停在第八轮 `docs(round8.1): 回填验收提交 SHA`，不包含第九轮、9.1 的任何代码与文档。
- 关系：`fa41a06` → `c92d626`（单父提交），`c92d626` 仅新增 9.1 最终报告，没有改写代码。

## 2. 报告与代码一致性抽查

| 9.1 报告声明 | 代码/测试证据 | 结论 |
| --- | --- | --- |
| 未知字段在 doc/photo/params/face/export/raw/color 保留 | 各结构 `#[serde(flatten, default)] extra`；`mergeUnknownKeys`；Rust 断言 `futureRawField` 等 | 一致 |
| `projectUuid + revision` 并发保护 | `ProjectDoc` 字段、`save_with_meta` 冲突拒绝、`SAVE_LOCK` | 一致 |
| 原子写 + fsync + 回读 + 备份 | `write_atomically` 全流程 | 一致 |
| 人脸按 UUID 合并、保留 dense/level3/maskRef | `merge_saved_faces`、8 个人脸合并单测 | 一致 |
| 前端不再整体重建丢字段 | `buildProjectDoc` 先铺开 `raw` 再覆盖；保留 masks/source/createdAt/decoderVersion | 一致 |
| 测试规模 | `cargo test`：384 + 12 + 4 + 10 + 4 + 5 + 7 + 2 + 3 + 3 + 9 = 443 passed，7 ignored；前端 76 passed；typecheck 通过 | 一致 |

未发现报告夸大或代码缺失。

## 3. 当前存储与调用链

### 3.1 存储形态

- 项目：单个 `.lumi` JSON 文件（`ProjectDoc`），不包含预览像素。
- 备份：同目录 `.lumi.bak`（覆盖前复制）。
- 临时文件：同目录 `.lumi.tmp`（写入、校验后 rename）。
- 无数据库、无 SQLite、无服务端。E1 中提到的 SQLite 数据在现有架构中不存在，发布检查需按“无数据库”记录。
- 无自动保存定时器；`grep autosave/autoSave/自动保存` 无结果。
- 无 `onCloseRequested`/`beforeunload` 处理；关闭窗口不会提示保存。

### 3.2 加载链

```
App.handleOpenProject
  → backend.openProject(destination)
  → commands/project.rs::open_project
  → image_engine/project.rs::load          // 64MB 上限、版本检查、JSON 解析
  → importPaths / computeProjectId
  → buildProjectSnapshot                   // 参数、人脸、蒙版、RAW、ICC、未知字段
  → commitProjectSnapshot                  // 六态状态机，仅 ready 可编辑
  → restore_faces（按框/IoU 匹配记录并回填 UUID）
  → finishProjectLoad
```

失败路径：版本高于 `PROJECT_VERSION(3)` 直接拒绝；JSON 损坏返回 `Decode`；照片为空拒绝；照片路径无法匹配时进入 failed 并提示，不写入磁盘。

### 3.3 保存链（当前仍是“整包文档”）

```
App.handleSaveProject
  → buildProjectDoc(state)                 // 以 rawDoc/rawProjectPhotos 为底重建整包
  → backend.saveProject(project, destination)
  → commands/project.rs::save_project
      → 按源路径合并 FaceRepository 中的会话人脸（merge_saved_faces）
      → 补 source_hash
      → image_engine/project.rs::save_with_meta
          → 版本检查
          → SAVE_LOCK
          → projectUuid 采用/冲突拒绝
          → revision 旧快照拒绝
          → revision = max(disk, incoming) + 1
          → sync_subject_records
          → 序列化 → write_atomically
```

当前协议没有 ChangeSet：前端提交完整 `ProjectDoc`，Rust 只做人脸合并与 revision 校验。A1–A3 需要在不破坏该整包回退路径的前提下，增加显式变更协议。

### 3.4 自动保存 / 关窗

- 不存在。阶段 A5 的“自动保存与手动保存并发”在现状下只能通过两个并发 `save_project` 调用模拟；阶段 C 需要决定是否增加自动保存或关窗提示（属于产品行为，不是存储重构）。
- 当前若用户直接关闭窗口，未保存修改会丢失；这是第十轮必须解决的可用性风险（P1，非 P0 静默数据损坏，因为用户主动未保存）。

### 3.5 迁移

- 旧项目 v1/v2/v3 依赖 serde `default` 补齐字段，无显式迁移函数、无迁移日志。
- 版本高于 3 拒绝加载/保存。
- 缺失 `projectUuid` 的项目在首次保存时补 UUID，`createdAt` 保留。
- A6 要求“迁移失败保留原项目并留日志”，当前只有错误返回，没有迁移日志；需要在保存/加载路径补充结构化日志，且不得改写原文件。

### 3.6 导出链

```
frontend exportImage
  → commands/render.rs::export_image
  → ImageCache/decoder 读取原图
  → pipeline::render(params)               // 与预览相同的 EditParams
  → export::write(target, source, image, settings, bit_depth)
      → 拒绝覆盖源文件
      → 编码 JPEG/PNG/TIFF
```

- 预览与导出使用同一 `pipeline::render`，参数同源。
- 导出失败不会修改源文件；目标文件写入为直接写，未复用项目保存的原子写。阶段 B6/E 需要验证失败恢复，必要时补导出原子写。

## 4. 第十轮风险清单

| 编号 | 风险 | 级别 | 处理阶段 |
| --- | --- | --- | --- |
| R-01 | 远端 main 落后本地 10 个提交，第九轮/9.1 未同步到 GitHub | 发布风险 | 阶段 E 前确认同步；本轮不改写历史 |
| R-02 | 保存仍为整包文档，前端任何漏字段都会造成覆盖；ChangeSet 尚未存在 | P0 | 阶段 A |
| R-03 | 无自动保存、无关窗提示，未保存修改会丢失 | P1 | 阶段 C |
| R-04 | 无显式迁移日志；旧 schema 仅靠 serde 默认值 | P1 | 阶段 A6 |
| R-05 | 导出目标直接写文件，失败可能留下半成品 | P1 | 阶段 B6 / E |
| R-06 | LEVEL 3 五官能力受 Provider 门控，YuNet 5 点不可用于稠密五官 | 产品限制 | 阶段 B4 / E4，文档明确 |
| R-07 | LibRaw 仍 `LEGAL_REVIEW_PENDING`；Windows 无实机 | 发布限制 | 阶段 E，Beta 判定必须体现 |
| R-08 | 无 SQLite，E1 的“SQLite 数据”检查不适用 | 文档差异 | 阶段 E 记录为 N/A |

## 5. 第十轮实施计划（按任务书顺序）

### 阶段 A：保存架构加固

1. 定义 `ChangeSet { project_id, base_revision, request_id, operations, created_at }` 与 operation 枚举；只覆盖现有数据模型已有字段，删除必须显式。
2. Rust 端新增 `apply_change_set`：校验 project UUID、实体 UUID、操作顺序；以磁盘最新文档为 base 应用变更；未知字段原样保留；不支持的操作显式报错。
3. 冲突：`base_revision != disk.revision` 返回结构化冲突错误（含最新 revision），不自动用旧快照覆盖；前端提示重新加载/合并。
4. 原子提交：沿用 `write_atomically`；全部操作成功后才递增 revision；响应返回最新 revision。
5. Debug 往返断言：`cfg(debug_assertions)` 下对目标文档做序列化→反序列化→规范化比较（UUID/topology/模型版本/蒙版 hash/浮点容差），差异输出字段路径与实体 UUID；Release 不执行全量比较。
6. 幂等：`request_id` 去重表（按 project UUID + request_id，最近 N 条），重复请求返回上次结果，不重复执行删除类操作。
7. 测试：重复 request_id、双窗口、自动/手动并发、A/B 切换、旧 revision、IPC 超时重试、成功未收到响应、同照片连改。
8. 迁移：迁移前后保留未知字段与 AI 数据；失败保留原文件并写日志。

### 阶段 B–F

- B：用有授权来源的照片完成导入/调色/美肤/塑形/身材背景/导出，记录来源与授权；不支持 RAW 显式失败；预览导出同管线。
- C：逐项检查 17 个交互点；补关窗未保存提示或自动保存策略；错误与空状态。
- D：24MP/45MP/多张/多人脸/多蒙版/50 张批量导出，记录冷启动、打开、首预览、参数响应、保存、导出、内存、CPU、GPU。
- E：macOS .app/.dmg、权限、模型目录、旧项目升级；LibRaw 保持 `LEGAL_REVIEW_PENDING`；Windows 保持未验证；模型清单与许可。
- F：全量自动化、GUI 证据、DMG 安装启动、真实照片保存重开导出；发布判定三档之一。

## 6. 阶段 0 判定

- 基线：**通过**（本地）。
- 报告一致性：**通过**。
- 工作区：**干净**。
- 远端同步：**存在差异，已记录为 R-01，不阻塞本地开发，但影响发布**。
- 允许进入阶段 A。
