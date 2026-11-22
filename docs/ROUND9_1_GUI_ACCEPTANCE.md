# ROUND 9.1 阶段 4：打包应用 GUI 数据完整性验收

- 应用：`src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`（16.52 MiB）
- 构建来源：基线 `9923436` + 本轮工作区修复；`npm run app:build` 成功（含 `tsc --noEmit && vite build`）
- 平台：macOS（arm64），本地实机
- 证据目录：`/tmp/lumi-round91-gui/evidence/`，关键截图压缩副本在 `docs/assets/round9_1/`
- 测试项目：`/tmp/lumi-round91-gui/*.lumi`（不写入用户项目目录，不触碰 `~/Documents/lumi-project-1.lumi`）

## 1. 测试项目

| 项目 | 照片 | 覆盖数据 |
| --- | --- | --- |
| `p1-color.lumi` | 2 | 基础调色（曝光 24 / 对比 -8 / 白平衡 12 / 9 / 饱和度 3 / 鲜艳度 11）、自动人脸检测 |
| `p2-face-dense.lumi` | 1 | 单人脸，`apple-vision` 稠密关键点 85 点、LEVEL 3 门控、美肤 17/33/11 |
| `p2b-group-multiface.lumi` | 1（4 人拼贴） | 多人脸、不同 faceUUID、连续保存 |
| `p3-masks-v2.lumi` | 1 | 手动蒙版（2 笔触/33/88）、人物蒙版（2 笔触/55/-10/20/30/45）、2 个旧版蒙版图层 |
| `p4-combo-v2.lumi` | 1 | 塑形（瘦脸 22/下颌 -11/眼 14/鼻 -6/嘴 5/下巴 9）、身材（-30/12/8）、背景（虚化/62/41） |
| `p5-raw-icc-v2.lumi` | 2 | 每照片不同 RAW/ICC/导出设置、`filenameTemplate` 未知字段、doc/photo/params 未知字段 |

合计 6 个项目、8 张照片，覆盖任务书要求的 5 类项目（第 6 个是多项目切换/多照片补充）。

## 2. 每个项目的执行流程

`启动 → 打开项目 → 核对界面参数 → ⌘S 保存 → ⌘Q 退出 → 验证进程结束 → 重启 → 重开 → 再次核对 → ⌘S → ⌘Q`

最终构建上又统一执行了一轮（2026-11-21 00:11–00:18），每个项目均完成两次保存与两次打开。

## 3. 结果

| 项目 | 界面核对 | 文件核对（保存后） | 结论 |
| --- | --- | --- | --- |
| p1 | 三处检查点均显示 24 / -8 / 12 / 9 / 3 / 11 | 与保存前深度 diff 为空（排除 revision/updatedAt） | PASS |
| p2 | 美肤 17/33/11；1 张人脸；重开一致 | 人脸记录 1 条，UUID 不变，`apple-vision` 85 点、LEVEL 3 仍在 | PASS |
| p2b | 4 张人脸 A/B/C/D，重开 ID 完全一致 | 4 条记录、UUID 唯一无重复，连续两次保存无膨胀 | PASS |
| p3v2 | 手动蒙版 33/88；人物蒙版 55/-10/20/30/45，重开一致 | 两个蒙版笔触与参数、2 个旧版蒙版图层全部保留 | PASS |
| p4v2 | 瘦脸 22、下颌 -11、身材 -30/12/8、背景 62/41 一致；五官 4 项因 LEVEL 3 未解锁而不显示滑杆 | 六个塑形值、身材、背景全部保留；无默认值覆盖 | PASS（见 4.2） |
| p5v2 | 照片1 Display P3/相对/开/开；照片2 sRGB/感知/关/关；重开一致 | 两照片 `color`/`export` 原样保留（含 `filenameTemplate`），RAW 显影与未知字段保留 | PASS |

连续 20 次打开/保存/重开的机器级等价性见 `ROUND9_1_PERSISTENCE_EQUIVALENCE.md` 与 `ROUND9_1_SAVE_REGRESSION.md`；GUI 侧完成了每个项目至少 2 次完整往返。

### 3.1 保存前后项目文件

| 项目 | 大小（字节） | SHA-256（前 16 位） |
| --- | --- | --- |
| p1-color.lumi | 10,556 | 28be6f33ad20eecc |
| p2-face-dense.lumi | 27,469 | 4f2f83f01bb5056a |
| p2b-group-multiface.lumi | 21,315 | 94e0203147d39391 |
| p3-masks-v2.lumi | 9,063 | 4c08c1b9fc937001 |
| p4-combo-v2.lumi | 8,105 | 59352a1c5968d4e6 |
| p5-raw-icc-v2.lumi | 14,382 | 203d12621d14c76b |

对比方法：保存前复制到 `pre-final/`，保存后用 Python 递归比较除 `revision`/`updatedAt`/`sourceHash` 外的完整 JSON。p2b 仅有人脸框浮点在第 7 位小数的差异（f32 重新检测），其余 4 个项目深度 diff 为空。

### 3.2 最终包冒烟

在最终 `app:build` 产物上再次执行 p1 与 p5v2 的完整往返（截图 34–37）。p1 前后深度 diff 为空；p5v2 仅新增了打开时自动检测到的人脸记录与 `params.faces/reshape` 条目（纯新增），原有 `color/export/raw/未知字段` 无变化。p5v2 文件因此由 12,453 B 增至 14,382 B。

## 4. 验收中发现的问题

### 4.1 R9.1-SAVE-09（P0，已修复）

首次 P5 验收发现：

- 打开项目后不做任何修改，直接 ⌘S：活动照片 `export` 从 `png/88/filenameTemplate=a` 变成项目默认的 `jpeg/92`。
- `ExportSettings` 里的 `filenameTemplate` 在 Rust 反序列化时被丢弃（前端也无法写回）。
- 色彩面板把项目里的 `displayP3` 显示为后端默认 `sRGB`。

修复：

- 会话增加 `colorDirty/exportDirty`；未修改时保存按磁盘原值写回，修改时只写活动照片并与原记录合并。
- 活动照片的 `color/export` 初始化改为读取该照片自己的记录（不再永远取第一张）。
- Rust `ExportSettings.extra` 用 `serde(flatten)` 保留未知字段（移除 `Copy`，调用点改为借用/克隆）。
- 色彩面板以项目设置为事实来源，后端 status 仅用于能力/警告展示。
- 回归：`persistence.test.ts` 新增 3 个用例（活动照片不改即保存、显式修改只影响活动照片、活动索引非 0）；Rust 等价性测试断言 `filenameTemplate` 保留。

修复后重新打包，P5v2 全部 PASS。

### 4.2 LEVEL 3 五官滑杆未显示

p4v2 的眼/鼻/嘴/下巴滑杆显示「缺少稠密关键点」、「LEVEL 3 · 结构门控 · 未解锁」。这是**产品既定门控**：当前会话 Provider 为 YuNet 5 点，不能解锁五官级塑形；磁盘上的 14/-6/5/9 没有被改写。加载与保存的持久化等价性已由文件核对证明。未把“界面不可编辑”伪装为“数据丢失”，也未为通过验收而绕过门控。

### 4.3 p2 的稠密点界面

p2 的 AI 人像面板显示当前 Provider（YuNet 5 点）能力，未展示已保存的 `apple-vision` 记录。文件核对确认 85 点与 LEVEL 3 完整保留；界面展示的是当前 Provider 能力，不是项目数据。已作为 UI 语义记录，不影响持久化结论。

### 4.4 误触「从图库移除」

P5 首轮操作中，验收人员误点缩略图右上角「×」，导致当前会话图库从 2 张变为 1 张；未在该状态下保存，磁盘文件仍为 2 张。复测时只点击缩略图图片本身，图库始终为 02。该事件为操作误触，不构成产品缺陷。

## 5. 截图

| 文件 | 内容 |
| --- | --- |
| `05-p1-resaved.jpg` | p1 第二次保存提示 |
| `07-p2-opened.jpg` / `09-p2-reopened.jpg` | p2 首次打开 / 重开，美肤 17/33/11 |
| `11-p2b-saved-twice.jpg` | p2b 连续保存，4 张人脸 |
| `18-p3v2-opened.jpg` | p3v2 蒙版参数 |
| `20-p4v2-opened.jpg` | p4v2 塑形/身材/背景 |
| `25-p5v2-photo1-color.jpg` / `26-p5v2-photo2-color.jpg` | p5v2 两张照片不同 ICC |
| `31-final-p1-opened.jpg` | 最终构建回归（p1） |
| `33-dmg-launch.jpg` | 从 DMG 复制后启动 |

完整原始 PNG 在 `/tmp/lumi-round91-gui/evidence/`（编号 01–33）。

## 6. DMG 与打包

- DMG：`src-tauri/target/release/bundle/dmg/LUMI RETOUCH_0.1.0_aarch64.dmg`（8.32 MiB）。
- `hdiutil attach` 成功，卷内包含 `LUMI RETOUCH.app` 与 `Applications` 链接。
- 复制到 `/tmp/lumi-dmg-test/` 后 `open` 启动成功（进程存在），截图 `33-dmg-launch.png`，随后正常退出。
- 签名：ad-hoc（本地构建，未做公证）；不属于本轮数据完整性范围。

## 7. 未覆盖 / 限制

- 仅在 macOS arm64 实机验收；未宣称 Windows 已验收。
- 色彩设置已持久化并在界面正确显示，但本轮未重新验证 ICC 在导出像素链路中的实际色彩变换（属渲染管线范畴）。
- p5 的 JPEG 文件带 RAW 参数时，RAW 面板按设计提示“不是 RAW 文件”；RAW 参数本身在文件中保留，未在界面提供编辑入口。
- 旧版 `ProjectPhoto.masks` 图层没有编辑界面，但保存时原样保留（p3v2 已验证）。
