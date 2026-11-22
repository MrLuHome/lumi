# ROUND 9.1 最终报告：项目数据完整性与持久化等价性

> 本轮定位：第九轮收尾，不计为第十轮。
> 基线：`9923436ff96a193e1ead4ee43f069f71bdeae059`（`main`，核验时工作区干净）。
> 本报告所有结论来自真实磁盘写入、正式 Repository/序列化代码与打包应用 GUI，不以内存对象等价替代。

## 1. 结论

- 打开 → 保存 → 关闭 → 重开，16 类项目数据在真实文件中等价；UUID、拓扑、模型版本、数组长度、蒙版引用严格一致。
- 未修改的数据不会再被部分编辑覆盖：人脸按 UUID 合并，色彩/导出按活动照片写回，未知字段在 doc/photo/params/face/export/raw/color 层级保留。
- 连续 20 次往返、多项目切换、多人脸连续保存、保存失败、旧快照、跨项目、损坏文件、未来版本均通过。
- 打包应用完成 6 个项目、8 张照片的 GUI 验收；DMG 挂载、复制、启动通过。
- 全量回归通过：Rust 443 passed / 0 failed / 7 ignored，clippy `-D warnings` 无告警，前端 76 passed，typecheck、build、app:build 成功。

## 2. 本轮修复的数据丢失问题

| 编号 | 问题 | 修复 |
| --- | --- | --- |
| R9-SAVE-01（回归） | 保存时整体覆盖人脸记录，打开后直接保存会丢稠密关键点 | `merge_saved_faces`：按 UUID 合并；会话缺失的 `dense/level3` 成对保留；保留 `maskRef/extra`；无 UUID 旧记录按内容防重复追加 |
| R9.1-SAVE-02 | 前端重建式保存丢失 `masks`、`source.metadata`、`createdAt`、RAW `decoderVersion` | 保存基于磁盘原始记录重建；未知字段铺开后再覆盖已知字段 |
| R9.1-SAVE-03 | 保存后 `createdAt` 被改成当前时间 | 保留原 `createdAt`；新增 `updatedAt` 单独记录 |
| R9.1-SAVE-04 | 未知字段在 doc/photo/params/face 被 serde 静默丢弃 | 四级 `#[serde(flatten)] extra` |
| R9.1-SAVE-05 | 非活动照片 ICC/导出被全局值覆盖 | 按照片保留磁盘值 |
| R9.1-SAVE-06 | `maskMeta.coverage/revision` 每次归零；无蒙版照片伪造元数据 | 笔触未变时保留；无内容不生成 |
| R9.1-SAVE-07 | 无 fsync/写入校验/备份失败处理 | 临时文件 → `sync_all` → 回读校验 → `.bak`（失败中止）→ rename → 目录 fsync → 最终回读；失败清理 `.tmp` |
| R9.1-SAVE-08 | 无并发保护，旧快照/跨项目可覆盖 | `projectUuid + revision` 校验、进程内保存锁、`revision = max(disk,incoming)+1` |
| R9.1-SAVE-09 | 打开后不改即保存，活动照片 `color/export` 被会话默认值覆盖；`ExportSettings` 未知字段（如 `filenameTemplate`）被丢弃；色彩面板显示后端默认值 | 色彩/导出脏标记；活动照片初始化读取自身记录；`ExportSettings/ProjectRaw/ProjectColor/RawDevelopParams/ColorOutputSettings` 增加 `extra`；色彩面板以项目为事实来源 |

## 3. 验证的数据类别与比较规则

| 类别 | 比较内容 | 规则 |
| --- | --- | --- |
| 项目元数据 | projectUUID、version、createdAt、updatedAt、revision | UUID 非空且稳定；createdAt 不变；updatedAt/revision 每次保存递增（明确排除在“数据等价”外） |
| 照片 | photoUUID(path)、源文件引用、sourceHash、顺序 | 路径与顺序严格一致；hash 为空时按文件计算，非空必须原样保留 |
| 基础调色 | 曝光/对比/白平衡/曲线/HSL/色彩 | 数值容差 ±1e-6；曲线点数量与顺序严格一致 |
| 美肤 | 磨皮/肤色/瑕疵/局部 | ±1e-6，数组长度严格一致 |
| 蒙版 | maskUUID、kind、enabled、invert、参数、笔触点数 | 严格一致；`masks/manualMask/subjectMask/maskMeta` 另做 SHA-256 内容摘要比较 |
| 人脸 | faceUUID、框、置信度、模型版本、maskRef | UUID 集合严格一致，不允许重复/串号；框 ±1e-6 |
| 稠密关键点 | topology/provider/点数/坐标/区域 | topology、provider、点数、数组长度严格一致 |
| 五官塑形 | 眼/鼻/唇/脸型/下巴 | ±1e-6，faceUuid 必须匹配 |
| 身材 | 区域、变形、保护区域 | ±1e-6 |
| 背景 | 识别/替换/模糊/边缘 | ±1e-6，启用状态严格 |
| RAW | 显影参数、白平衡、demosaic、decoderVersion、相机信息 | 严格，浮点 ±1e-6；不在 GUI 中伪造 |
| ICC | 输入/工作空间/输出 Profile、intent、黑点补偿、embed | 枚举/布尔严格 |
| 编辑历史 | 顺序、参数、撤销重做状态 | 项目文件不持久化撤销栈；重开后可重新编辑，产品规则见回归文档（不伪装为已恢复） |
| 批量 | 参数模板、队列、任务状态 | 项目内模板严格一致；运行中队列不持久化 |
| 预设 | 引用、内容、自定义 | 严格一致 |
| 文件关联 | 源文件、缓存、蒙版、模型引用 | 路径与引用严格一致；缺失显式报错 |

## 4. 测试项目与样本

- 等价性/回归：1 个“全类别富项目”（2 照片、2 人脸、85 点稠密、LEVEL 3、双蒙版图层、RAW、ICC、源元数据、四级未知字段）+ 20 轮往返。
- 性能：small 2 张 / medium 8 张 / large 50 张。
- GUI：6 个项目、8 张照片、6 类组合（基础调色、稠密人脸、4 人多人脸、双蒙版、塑形+身材+背景、RAW+ICC+导出）。
- 自动化用例：Rust 443 通过（其中本轮等价性 9 例），前端 76 通过（其中持久化 10 例）。

## 5. 连续 20 次与切换

- 20 次“保存 → 关闭 → 重开 → 比较”全部等价；文件大小稳定，无重复人脸/重复蒙版累积。
- 打开 A → 快速切 B → 保存 B → 重开 A：A 不被 B 的参数污染（前端快照按 photo id 隔离 + 后端 projectUuid 校验）。
- 多人脸连续保存：4 条记录、UUID 唯一，连续两次保存字节数稳定（21,307 → 21,315 B，差异仅为浮点重算）。
- 多照片批量编辑：仅活动照片变化，其它照片深度 diff 为空。

## 6. 故障注入

原项目可读、无半成品、备份可用、错误可见，详见 `ROUND9_1_FAILURE_INJECTION.md`：

- 写入失败（临时路径被占）：原文件 SHA 不变，`.tmp` 无半成品。
- 旧快照（revision 3 → 磁盘 7）：拒绝，提示“项目已在别处更新”。
- 跨项目 UUID：拒绝，提示“目标文件属于另一个项目”。
- 损坏文件：显式报错；`.bak` 可恢复。
- 未来版本 `version=99`：拒绝加载，不降级解析。

## 7. GUI 与 DMG

详见 `ROUND9_1_GUI_ACCEPTANCE.md`。最终构建上 5 个项目各完成两轮“打开→保存→退出→重启→重开→保存→退出”，p5v2 单独复测通过；保存前后文件深度 diff 为空（p2b 仅 f32 第 7 位小数噪点）。DMG 挂载、复制到本地、启动成功。

## 8. 性能

debug 构建、真实临时目录（含 fsync/双重校验/备份）：

| 项目 | 文件大小 | 保存 | 加载 |
| --- | --- | --- | --- |
| 2 张 | 60,319 B | 15.99 ms | 1.88 ms |
| 8 张 | 240,229 B | 22.51 ms | 6.83 ms |
| 50 张 | 1,499,759 B | 98.55 ms | 26.98 ms |

20 次连续往返总耗时 0.34 s，无膨胀。

## 9. 全量回归

| 项目 | 命令 | 结果 |
| --- | --- | --- |
| Rust 测试 | `cargo test` | 443 passed，0 failed，7 ignored |
| Rust 静态检查 | `cargo clippy --all-targets -- -D warnings` | 通过，无告警 |
| 前端类型 | `npm run typecheck` | 通过 |
| 前端测试 | `npm test -- --run` | 76 passed（7 files） |
| 前端构建 | `npm run build`（由 app:build 执行 tsc+vite） | 通过 |
| 打包 | `npm run app:build` | 通过（.app 16.52 MiB / DMG 8.32 MiB） |
| GUI | 打包应用实机 | 6 项目通过 |
| DMG | 挂载 + 复制启动 | 通过 |

## 10. 未完成事项与限制

- 仅 macOS arm64 实机验收；未宣称 Windows 通过。
- ICC 在导出像素链路的实际色彩变换未在本轮重新验证（本轮范围是持久化与界面回显）。
- `DenseFace`/`Level3Gate` 内部未知子字段仍为严格 schema（Provider 版本化结论，不允许静默拼装）；已知数据完整保留，未知子字段不保留，已在等价性文档中列为明确限制。
- 撤销/重做历史不写入项目文件：这是既有产品规则，重开后不伪装为已恢复。
- 网络卷/外接盘的 fsync 语义、真实断电未测；以原子写与失败路径测试覆盖。
- `ProjectPhoto.masks` 旧图层无编辑界面，但保存原样保留。
- LibRaw 仍为 `LEGAL_REVIEW_PENDING`，NME 仍为 `BLOCKED`，ONNX 模型仍为 `ProtocolTestOnly`；本轮未改变这些结论。

## 11. 提交与工作区

- 本轮代码与文档提交：`fa41a06bacaacf7bf729a6f85aea5c39d488c607`
- 本报告为紧随其后的文档补充提交；两次提交后工作区干净（`git status --short` 无输出）。

本轮结束，不进入第十轮，不新增功能。
