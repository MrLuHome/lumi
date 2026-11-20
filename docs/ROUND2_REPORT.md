# ROUND 2 交付报告：LUMI RETOUCH 核心图像引擎

基线提交：`14deaa9`（第一轮工程底座）
本轮范围：统一图像引擎、专业调色、非破坏性项目、预览/高清渲染、真实本地人脸检测、前端面板升级、测试与构建验证。
本轮未开始第三轮功能。

## 1. 完成情况总览

| 交付项 | 状态 | 说明 |
| --- | --- | --- |
| 工程审查文档 | 完成 | [ROUND2_AUDIT.md](ROUND2_AUDIT.md)，含已有功能、问题分级、改动清单与计划 |
| 统一 Rust 图像引擎 | 完成 | `src-tauri/src/image_engine/`，16-bit 内部表示，预览/导出同一条管线 |
| 专业调色 | 完成 | 14 个基础滑杆 + HSL 八通道 + RGB/单通道曲线 + 3D LUT + 自动白平衡 |
| 非破坏性编辑 | 基本完成 | 原图只读；参数/导出设置/蒙版数组序列化；项目保存/打开/复制粘贴/重置/撤销重做。蒙版仅持久化，未提供蒙版绘制 UI |
| 预览与高清渲染 | 完成 | 适应窗口/100%/200%/滚轮/拖动/分屏对比/按住看原图；3200px 高清预览；后端任务取消与结果缓存 |
| AI 人脸检测 | 完成 | YuNet 2023mar ONNX + tract 纯 CPU 推理；框/5 关键点/多人脸/进度/失败回退 |
| 前端升级 | 完成 | 基础/光线/色彩/HSL/曲线/细节/人脸信息 + 导出设置；所有按钮连接真实后端 |
| 测试照片集 | 完成 | 10 类样本，覆盖格式、EXIF、透明、16-bit、多人、无人脸、损坏 |
| 自动化测试 | 完成 | Rust 64 项通过（1 项手动基准忽略），前端 37 项通过 |
| 构建与提交 | 见第 7 节 | 前端与应用构建已验证，代码已本地提交 |

## 2. 已完成功能明细

### 2.1 统一图像引擎

- 内部统一使用 16-bit RGBA（`Image16`），8-bit 输入按 `×257` 归一化；16-bit PNG 保留原始精度；导出 16-bit PNG 走高位深路径。
- 支持 JPG/JPEG/PNG/WebP；EXIF Orientation 1–8 在解码阶段应用；损坏/空文件返回明确 `Decode` 错误；超过 1.2 亿像素的图片拒绝处理，避免内存失控。
- 预览与导出调用同一个 `pipeline::render_with_progress`，仅输入分辨率不同；分阶段进度回调（磨皮 → 色彩 → 细节 → 完成）。
- 所有命令通过 `spawn_blocking` 执行，不阻塞 UI 线程；`RenderRegistry` 用 `AtomicBool` 取消旧任务，阶段边界与逐像素双重检查；旧结果不会覆盖新结果（前端 requestId + 后端取消）。

### 2.2 专业调色

- 基础：曝光、对比度、高光、阴影、白色色阶、黑色色阶、色温、色调、饱和度、自然饱和度、清晰度、锐化、颗粒、暗角、磨皮。
- 进阶：HSL 八通道（红/橙/黄/绿/青/蓝/紫/品红）× 色相/饱和度/明度；RGB 与 R/G/B 曲线；3D `.cube` LUT（三线性插值、强度 0–100、1D LUT 明确报错）；灰世界自动白平衡。
- 渲染顺序：磨皮 → 白平衡/曝光/光影/色阶/HSL/饱和度/曲线/LUT（单次遍历）→ 清晰度 → 锐化 → 颗粒 → 暗角。
- 每个滑杆均为真实像素运算：双向拖动、方向键微调、数值输入、单项重置（↺）、双击归零、参数保存到项目、撤销/重做、恢复默认；无任何 CSS/透明度伪装。
- 中性参数直接复用原图，避免无意义渲染；非中性参数才进入管线。

### 2.3 非破坏性编辑

- 原始文件只读：所有命令以路径加载，导出拒绝写入原图路径；导出采用临时文件 + 原子改名。
- 统一编辑状态：原始图片信息、调色参数、人像参数（磨皮）、蒙版数组、编辑历史（按图独立）、导出设置（格式/质量）。
- 撤销/重做：按图独立 100 步栈，拖动滑杆按参数键 600ms 合并，HSL/曲线拖动同样合并；新编辑清空重做分支。
- 单项重置（滑杆/分组/HSL 通道/曲线通道）、全部重置、参数复制与粘贴（跨照片）。
- 项目保存/打开：JSON `.lumi`（version 1），保存参数、蒙版数组、导出设置、当前照片索引；打开时重新导入原图并按路径恢复参数，缺失文件会提示恢复比例。
- 保存项目不写入预览图，不会把当前预览当成最终结果。

### 2.4 预览与高清渲染

- 适应窗口、100%、200%、滚轮锚点缩放、拖动、分屏对比、按住 `\` 看原图、双击在适应/100% 间切换。
- 缩放 ≥105% 时请求 3200px 高清预览；导出始终使用原始分辨率。
- 缓存：图片解码缓存（1GB 预算，LRU）与渲染结果缓存（160MB / 12 条，键 = 路径 + 尺寸 + 参数 JSON）；撤销/重做与来回切换可直接命中。
- 滑杆防抖 70ms；新任务自动取消旧任务；进度事件 `lumi://render-progress` 同时驱动画布与状态栏。
- 大图限制：解码阶段拒绝 >1.2 亿像素；预览/高清预览按最大边缩放，避免一次性占用过多内存。

### 2.5 AI 人脸检测

- 模型：YuNet `face_detection_yunet_2023mar.onnx`，232,589 字节，SHA256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`，MIT（OpenCV Zoo），允许商业使用与再分发。
- 推理：tract-onnx 0.23 纯 Rust CPU，无 ONNX Runtime 二进制；启动时校验 SHA256；输入 `1×3×640×640` float32 BGR，输出 12 个张量按 `cls/obj/bbox/kps × stride 8/16/32` 解码；NMS IoU 0.3，置信度阈值 0.6。
- 能力：人脸数量、边界框、5 个关键点、多人脸独立 ID（按 x 排序）、加载/推理进度、失败回退（模型缺失/校验失败/解码失败均返回明确错误）、CPU 推理。
- 前端：检测按钮、模型信息（版本/许可证/SHA256/来源/输入输出）、检测框与关键点叠加、多人脸列表切换、高亮当前人脸、显示/隐藏叠加层。
- 所有检测结果来自真实模型输出，无任何模拟数据；无人脸照片返回 0 条，不伪造结果。
- 模型随 Tauri `bundle.resources` 分发，`LICENSE` 原文一并打包；授权信息登记在根目录 [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md)。

### 2.6 前端升级

- 保留第一轮暗色三栏视觉风格与设计令牌。
- 右侧面板：基础、光线、色彩、HSL、曲线、LUT、细节、人脸信息、导出设置；分组可折叠、分组重置。
- 画布：拖动、缩放、分屏对比、原图对比、检测框/关键点、渲染进度（阶段 + 百分比）、错误提示、拖拽导入遮罩。
- 菜单与快捷键：文件（导入/打开项目/保存项目/导出/退出）、编辑（撤销/重做/复制参数/粘贴参数/重置全部）、视图（适应/100%/200%/分屏对比）。
- 所有交互均连接真实 Tauri 命令；浏览器预览模式仅用于界面走查，会明确提示渲染与导出需桌面端。

## 3. 架构变化

- 旧 `src-tauri/src/imaging/` 单层工具集删除，替换为 `image_engine/` 门面：`decoder/encoder/buffer/color/geometry/pipeline/preview/export/project/params/cancel` + `operations/{blur,grade,detail}`。
- 新增 `ai/` 模块：`model`（元数据与 SHA256）、`postprocess`（网格解码、NMS、坐标映射）、`face`（模型加载、推理、状态）。
- 命令层拆分为 `library`（导入）、`render`（预览/取消/导出）、`project`（项目/LUT/自动白平衡）、`faces`（模型状态/检测）。
- 状态层：`ImageCache` 改为 16-bit 缓存并引入内存预算；新增 `RenderRegistry`、`RenderCache`、`FaceEngine` 三个 managed 状态。
- 前端状态：`EditParams` 扩展 HSL/曲线/LUT；store 增加项目、剪贴板、人脸、渲染进度、导出设置；新增 `project.ts` 序列化门面。
- 参数仍由前端与 Rust 手工同步，但统一使用 camelCase，默认值与夹紧逻辑两端一致；项目文件加载时前端再做一次深夹紧。

## 4. 修改/新增文件

### 新增

- `src-tauri/src/image_engine/`：`mod.rs`、`params.rs`、`buffer.rs`、`color.rs`、`geometry.rs`、`cancel.rs`、`decoder.rs`、`encoder.rs`、`pipeline.rs`、`preview.rs`、`export.rs`、`project.rs`、`operations/{mod,blur,grade,detail}.rs`
- `src-tauri/src/ai/`：`mod.rs`、`model.rs`、`postprocess.rs`、`face.rs`
- `src-tauri/src/commands/faces.rs`、`src-tauri/src/commands/project.rs`
- `src-tauri/resources/models/yunet/face_detection_yunet_2023mar.onnx`、`LICENSE`
- `src-tauri/tests/engine.rs`、`src-tauri/tests/face_detection.rs`
- `src-tauri/tests/fixtures/images/`：10 类测试照片
- `src/components/CurveEditor.tsx`、`src/components/HslPanel.tsx`、`src/components/FacePanel.tsx`
- `src/state/project.ts`
- `docs/ROUND2_AUDIT.md`、`docs/ROUND2_REPORT.md`

### 修改

- Rust：`Cargo.toml`、`tauri.conf.json`、`lib.rs`、`state.rs`、`commands/{mod,library,render}.rs`
- 前端：`App.tsx`、`types.ts`、`state/{params,editorStore}.ts`、`lib/backend.ts`、`hooks/usePreviewRenderer.ts`、`components/{TopBar,Inspector,CanvasStage,StatusBar}.tsx`、`components/ui/Slider.tsx`、`styles/app.css`
- 测试：`state/{params,editorStore}.test.ts`
- 许可证：`THIRD_PARTY_LICENSES.md`

### 删除

- `src-tauri/src/imaging/{mod,adjustments,blur,decode,encode}.rs`（由新引擎替代，无外部引用）

## 5. 测试结果

### 5.1 自动化测试

| 套件 | 命令 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --lib` | 50 passed |
| 引擎集成测试 | `cargo test --test engine` | 10 passed，1 ignored（手动性能基准） |
| 人脸检测集成测试（真实模型） | `cargo test --test face_detection` | 4 passed |
| 前端单元测试 | `pnpm test -- --run` | 4 个文件，37 passed |
| 前端类型检查/构建 | `pnpm build` | 通过 |

覆盖点：

- 解码：JPG/WebP/透明 PNG/16-bit PNG/EXIF 旋转/损坏文件。
- 参数：默认值、夹紧、HSL 计数、曲线排序过滤、LUT 夹紧、中性判定。
- 图像处理：中性恒等、调色改变像素、相同参数确定性、预览与导出结果一致性、高分辨率缩放。
- 撤销/重做：单组重置、全部重置、HSL 合并、曲线重置、跨图独立历史。
- 项目：保存/加载参数与曲线往返、参数复制粘贴、项目恢复导出设置。
- 人脸：单人 1 张、四人拼贴 4 张、无人脸 0 张、暗光 1 张；坐标与 5 关键点边界检查。
- 导出：拒绝覆盖原图、文件尺寸与重载分辨率、格式扩展名。
- 任务取消：已取消 token 渲染返回 `Cancelled`；新请求取消旧请求；渲染缓存命中与淘汰。
- 异常恢复：损坏文件明确报错；模型缺失返回明确错误；项目缺图提示恢复比例。

### 5.2 真实模型验证

Release 构建，macOS 本机：

| 样本 | 检测结果 | 推理耗时 | 含模型总耗时 |
| --- | --- | --- | --- |
| 单人肖像 1680×2240 | 1 张 | 21 ms | 63 ms |
| 四人拼贴 | 4 张 | 21 ms | 63 ms |
| 无人脸风景 | 0 张 | 22 ms | 70 ms |
| 暗光肖像 | 1 张 | 21 ms | 63 ms |

### 5.3 性能数据（Release，macOS 本机）

| 场景 | 耗时 |
| --- | --- |
| 1350×1800 全效果渲染 | 98 ms |
| 1350×1800 JPEG 预览编码 | 32 ms（492 KB data URL） |
| 3360×4480（15MP）全效果渲染 | 629 ms |
| 3360×4480 JPEG 导出（渲染 + 编码 + 写盘） | 205 ms（不含解码），输出 1678 KB |
| 人脸推理（640×640 输入） | 约 21 ms |

调试构建明显更慢（人脸推理约 1.3 s/张），以上数据仅代表 release。

## 6. 兼容性与平台

- macOS：本机完成构建、启动、导入、调色、撤销、项目保存/打开、导出、人脸检测验证。
- Windows：本轮**未实机验证**。代码未使用 macOS 专属 API；文件对话框使用 `rfd`，路径处理使用 `PathBuf`；但窗口标题栏样式与打包产物未在 Windows 测试，不能声称已验证。
- 浏览器：仅支持界面走查与本地文件选择预览；调色、导出、人脸检测需要 Tauri 桌面端。

## 7. 构建与提交

- `pnpm build`：通过（TypeScript + Vite）。
- `pnpm test -- --run`：37 项通过。
- `cargo test`：64 项通过，1 项手动基准忽略。
- `pnpm tauri build --debug --bundles app`：通过，产物 `src-tauri/target/debug/bundle/macos/LUMI RETOUCH.app`（108.98 MiB）。
- 打包应用启动验证：直接打开 `.app`，进程存活、无崩溃报告，界面正常渲染（顶栏菜单、图库、画布、参数面板、状态栏均可见）。
- 代码已按本轮范围本地提交（`016f8f9`），未推送远端。

## 8. 未完成事项

1. 蒙版：数据结构与项目序列化已就位（`MaskLayer/MaskPoint/MaskKind`），但本轮未实现画笔/渐变蒙版 UI 与局部调整算法；保存的蒙版数组当前为空。
2. ICC 色彩配置未保留：仅保留像素与 EXIF 方向；导出不写回 EXIF/ICC 元数据。
3. 大图分块渲染：当前按整图缩放与整体渲染，>1.2 亿像素直接拒绝，未实现分块流式处理。
4. Windows 打包与交互未验证。
5. 测试照片集的原始人像来源与肖像授权待确认（见许可证表），发布前需替换为自有或已授权素材。
6. 人脸关键点尚未用于局部磨皮/液化等后续人像算法，仅做检测与展示。
7. 批量导出、预设管理未实现。

## 9. 已知问题

- 高清预览上限 3200px：对超过 3200px 的图片，200% 查看仍会放大预览而非原始像素；导出不受影响。
- 预览通过 data URL 经 IPC 传输，单张约 0.5 MB；极大图或极高频拖动会增加 IPC 压力（已用 70ms 防抖 + 后端取消缓解）。
- 项目文件只存路径不存像素：原图被移动/删除后无法恢复，只能提示缺失。
- 自动白平衡为灰世界估计，极端单色场景可能偏差较大；可在应用后手动微调。
- 曲线编辑器最多 16 个控制点（前端夹紧），超出会被截断。
- 浏览器预览模式下所有滑块只更新状态，不会看到真实像素变化（界面已提示）。
- 开发模式首次人脸检测需加载并优化模型，耗时明显高于后续推理。

## 10. 下一轮建议

1. 蒙版系统：画笔/线性渐变/径向渐变 + 局部参数，复用现有 `MaskLayer` 结构与渲染管线，增加蒙版可视化与羽化。
2. 色彩管理：读取 ICC 并转换到工作色彩空间，导出回写 ICC/EXIF，支持嵌入版权信息。
3. 渲染性能：分块并行 + 下采样代理渲染，滑杆拖动时使用低分辨率代理，松手后 3200px 补渲染。
4. 人脸关键点落地：基于 5 点做自动磨皮区域、瘦脸/大眼等局部调整（需新增真实算法与测试，禁止只加 UI）。
5. 批量处理与预设：批量导出、参数预设导入导出、项目最近列表。
6. 平台验证：Windows 实机打包与交互测试，补充 CI 矩阵。
7. 测试素材：替换为明确授权的测试人像，并把基准测试纳入 CI 性能门禁。
