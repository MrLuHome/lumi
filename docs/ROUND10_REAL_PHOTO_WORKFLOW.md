# ROUND 10 阶段 B：真实照片端到端工作流

> 结论：macOS 打包应用完成「导入 → 基础修图 → 人像美肤 → 五官塑形 → 保存 → 退出 → 重开 → 导出」全流程。
> 期间修复了一个真实照片下五官塑形被安全策略完全回退的缺陷（见 B4），并补齐 TIFF 导入。
> RAW 导入继续关闭（LibRaw `LEGAL_REVIEW_PENDING`），UI 明确拒绝，不假装成功。

## 1. 测试语料与授权

照片仅用于本地功能验收，不用于模型训练，不进入公开数据集，不随发行包分发。

| 文件 | 尺寸 | 用途 | 来源 / 许可 |
| --- | --- | --- | --- |
| `portrait-a.jpg` | 3000×4495 | 单人近景，主流程 | Unsplash `photo-1544005313-94ddf0286df2`，Unsplash License |
| `portrait-b.jpg` | 3000×4499 | 第二张单人照、批量导出 | Unsplash `photo-1500648767791-00dcc994a43e`，Unsplash License |
| `portrait-group.jpg` | 3000×1688 | 多人脸、批量导出 | Unsplash `photo-1529156069898-49953e39b3ac`，Unsplash License |
| `photo.png` | 1335×2000 | PNG 导入/导出 | 由 `portrait-a.jpg` 本地转码生成 |
| `photo.tiff` | 1335×2000 | TIFF 导入/导出 | 由 `portrait-a.jpg` 本地转码生成 |
| `photo-orient6.jpg` | 3000×4495 存储，显示 4495×3000 | EXIF Orientation=6 | 由 `portrait-a.jpg` 写入方向标签生成 |
| `photo-p3.jpg` | 1335×2000 | Display P3 ICC | 本地转码并嵌入 Display P3 |
| `photo-noicc.jpg` | 1335×2000 | 无 ICC 回退 | 本地转码剥离 ICC |
| `large-24mp.jpg` | 6000×4000 | 大分辨率导入/导出 | 由 `portrait-a.jpg` 放大生成 |
| `large-45mp.jpg` | 9000×5000 | 性能阶段使用 | 由 `portrait-a.jpg` 放大生成 |
| `corrupt.jpg` | 195 KB 随机字节 | 损坏文件报错 | 本地生成 |
| `fake-unsupported.cr3` | 1.9 KB 文本 | 不支持的扩展名 | 本地生成 |
| `sample-canon.cr2` | 5.8 MB | 真实 RAW 拒绝路径 | raw.pixls.us，CC0 |

语料目录：`/tmp/lumi-round10/photos`（环境变量 `LUMI_TEST_PHOTOS` 指向它）。
SHA-256 由 `shasum -a 256` 现场生成；完整值记录在阶段 F 的测试日志中。
注意：`/tmp` 为临时目录，测试日志与截图同时保留在 `docs/ROUND10_*` 的引用中，不随仓库分发照片本身。

## 2. 自动化管线验证

命令：

```bash
cd src-tauri
LUMI_TEST_PHOTOS=/tmp/lumi-round10/photos cargo test --test round10_real_photos -- --nocapture
```

结果：`5 passed; 0 failed`。覆盖：

| 测试 | 验证点 |
| --- | --- |
| `real_photos_import_render_and_export` | 8 类样张解码 → 默认参数渲染 → JPEG/PNG/TIFF 导出 → 回读尺寸；PNG 中心像素 8-bit 往返容差 ≤400；JPEG 必须含 ICC；源文件字节不变 |
| `orientation_is_applied_on_import` | EXIF Orientation=6 后显示尺寸交换 |
| `corrupt_and_raw_files_fail_explicitly` | 损坏 JPEG 与 CR2 都必须明确报错；CR2 文案含「未开放 RAW」 |
| `dng_and_tiff_extensions_are_classified_honestly` | TIFF 为支持扩展名，CR2 不是；TIFF 标签正确 |
| `real_portrait_reshape_at_moderate_strength_changes_pixels` | 真实人像瘦脸 50 必须产生非零 `applied_scale` 与 `warped_pixels`（B4 回归） |

导入层同步补齐：

- `image` crate 开启 `tiff` feature；
- `decoder::SUPPORTED_EXTENSIONS` 增加 `tif/tiff`；
- `pick_images` 与拖拽导入过滤器增加 `tif/tiff`；
- RAW 扩展名统一返回「当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）」；
- 空选择提示更新为「支持 JPG / PNG / WebP / TIFF」。

## 3. GUI 验收（打包 .app）

应用：`src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`
截图：`/tmp/lumi-round10/gui/shots/`（step01–step14）

| 步骤 | 结果 | 证据与观察 |
| --- | --- | --- |
| 启动空状态 | PASS | 图库 00，Inspector「等待照片」 |
| 导入 JPG | PASS | `portrait-a.jpg 3000×4495`、`portrait-group.jpg 3000×1688`；竖幅方向正确；图库 02 |
| 导入 TIFF/PNG | PASS | `photo.tiff 1335×2000`、`photo.png 1335×2000` 均被接受 |
| 损坏文件 | PASS | 提示「图片解码失败：无法识别图片数据：The image format could not be determined」，图库不新增条目 |
| RAW（CR2） | PASS（明确拒绝） | 文件面板过滤后提示「没有找到可导入的图片（支持 JPG / PNG / WebP / TIFF）」 |
| 基础修图 | PASS | 曝光 30 / 对比度 20 / 饱和度 25；⌘Z 回到饱和度 0、⇧⌘Z 回到 25；顶栏「3 项已调整」 |
| 人像美肤 | PASS | 「AI 人像 → 分析当前照片」检出 1 张人脸 1201×1800，拒绝 `yunet-5` 冒充稠密关键点；自动皮肤分析 1 张脸；磨皮 50 后皮肤可见平滑，皮肤占比 19% |
| 五官塑形 | PASS（修复后） | 整体瘦脸 50 真实改变轮廓；0↔50 分屏可见差异；无「降强度 0%」 |
| 保存 | PASS | 提示「项目已保存：round10.lumi · 11.6 KB」，磁盘 11,865 字节 |
| 退出/重开 | PASS | ⌘Q 后进程消失；重开项目 5 项参数全部恢复 |
| 单张导出 | PASS | JPEG 5.1 MB / 662 ms；PNG 63.1 MB / 1.07 s；TIFF 77.2 MB / 585 ms，均 3000×4495 |
| 批量导出 | PASS | 2/2 完成、0 失败、0 跳过，耗时 1.67 s；`portrait-a-lumi.jpg` 5,259,949 B，`portrait-group-lumi.jpg` 1,013,484 B |
| 退出 | PASS | `pgrep` 无进程 |

导出细节：

- PNG/TIFF 在嵌入 ICC 时固定输出 16-bit（PNG RGBA16、TIFF RGB16 未压缩）；JPEG 为 8-bit。
- JPEG 嵌入 ICC 已由自动化测试逐张校验。
- 单张导出使用原分辨率，不缩放；预览与导出共用 `pipeline::render`。
- 批量导出沿用第六轮既有实现，支持失败重试与覆盖确认；本轮只做了 2 张冒烟。

## 4. B1–B6 对照

### B1 导入

- JPEG/PNG/TIFF/WebP 通过；RAW 关闭且明确拒绝；EXIF 方向通过；相机信息与 ICC 在信息面板读取；24MP 通过。
- 损坏文件明确报错；重复导入去重沿用既有逻辑（按规范化路径）；缺失源文件在打开项目时按既有「缺失源」流程处理。
- 不修改原图：自动化测试对每张样张做字节级前后比较。

### B2 基础修图

- 曝光/对比度/高光/阴影/白平衡/色温/色调/饱和度/HSL/曲线/锐化/降噪/暗角/色彩配置均存在既有实现与测试。
- 本轮 GUI 实测曝光、对比度、饱和度；撤销/重做有效；保存重开后数值一致。
- 0 值为恒等：`pipeline::render` 在 `params.is_neutral()` 时直接复用原图。

### B3 人像美肤

- 皮肤蒙版来自几何+颜色分割，不宣称语义识别；无检测结果时明确提示并跳过，不退化为全图磨皮。
- 眼睛/眉毛/嘴唇/头发保护由蒙版权重实现；GUI 观察磨皮 50 后五官边缘保持。
- Level 3 精细能力仍由门控关闭，UI 显示「缺少稠密关键点」。

### B4 五官塑形（本轮关键修复）

发现缺陷：真实近景人像在「整体瘦脸 50」时被安全策略将强度降为 0%，滑杆可动但像素完全不变。

根因：

1. `delaunay` 输出三角形绕向不一致，`flipped_triangles()` 把初始绕向当作折叠；文档注释要求调用方统一，但调用方未做。
2. 真实人脸鼻尖位置偏低时，鼻部控制点与脸型环控制点在局部坐标里几乎重合（实测相距 1.9 px / 0.0045 眼距）。两点一个保持不动、一个随瘦脸内收，任何塑形都会翻折相邻三角形，安全策略因此逐级降强度直到 0。

修复（均在既有架构内）：

- `triangulation::delaunay`：统一输出正绕向；并用 `index_of` 把去重后的索引映射回原始点索引，修掉重复点导致的后续索引整体错位。
- `face_mesh`：构建控制点前按保护级别焊接重合点（固定点 > 五官环 > 脸型环），同一位置只保留一个位移；焊接半径 0.01 眼距。
- 回归测试：
  - `triangulation::winding_is_normalized_and_duplicates_keep_original_indices`
  - `face_mesh::coincident_feature_and_face_rings_are_welded_before_deformation`
  - `round10_real_photos::real_portrait_reshape_at_moderate_strength_changes_pixels`

修复后效果：真实人像瘦脸 50 → 安全策略保留 42% 强度（约 90 万像素被形变），GUI 中 0↔50 分屏可见轮廓变化；当参数确实超出安全范围时仍如实提示降强度，不静默。

仍需如实说明：

- 当前只有 YuNet 5 点，眼部/鼻部/唇部/下巴等 LEVEL 3 精细项显示「缺少稠密关键点」，不宣传为精准五官识别。
- 极端参数仍可能触发「检测到网格折叠或位移超限，已自动降低强度」；这是保护机制，不是崩溃。
- 自动降强度百分比在首次重开滚动面板时曾闪现通用提示，随后重算与再次重开均不再出现；未复现为稳定缺陷，记入已知问题观察项。

### B5 身材与背景

- 身材依赖手动人物蒙版；无蒙版时明确提示「未检测到可用的身材蒙版，身材调整未改变像素」。
- 背景处理必须有主体蒙版，否则提示并跳过。
- 本轮未做新的算法改动；既有单元/集成测试保持通过。GUI 未对身材/背景做逐步截图，列为阶段 C/F 的补充验收项。

### B6 导出

- JPEG/PNG/TIFF 均通过；单张与批量通过；覆盖保护与失败重试沿用批量引擎既有逻辑。
- 预览与导出共用 `pipeline::render`，参数一致；导出不修改源文件。
- 已知限制：单张导出不提供 8-bit PNG/TIFF 选项；PNG/TIFF 在 ICC 开启时固定 16-bit。

## 5. 已知限制与风险

1. RAW 功能在 1.0 Beta 关闭；LibRaw 许可未完成前不发布包含该组件的正式包。
2. 人脸只有 5 点，LEVEL 3 精细塑形不可用；UI 已门控。
3. 身材/背景本轮只做回归，不做新增 GUI 证据（阶段 C/F 补）。
4. 测试照片来源为 Unsplash，仅本地验收；不得再分发或用于训练。
5. 语料位于 `/tmp`，重启机器后需重新准备；测试命令已在文档与阶段 F 日志中固定。
