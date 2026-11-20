# ROUND 2 工程审查

审查基线：`14deaa9`（第一轮工程底座）。审查范围：前端目录、Rust 后端、Tauri 命令接口、导入与显示、状态管理、撤销重做、构建方式、依赖与许可证。

## 1. 已有功能与完成度

| 模块 | 现状 | 完成度 | 结论 |
| --- | --- | --- | --- |
| 工程骨架 | Tauri 2 + React 19 + TS + Vite + Rust，macOS debug 包可启动 | 可用 | 保留 |
| 导入 | JPG/JPEG/PNG/WebP，系统选择器 + 拖拽，按路径去重 | 可用 | 保留，补错误分型 |
| 解码 | `image` crate 解码，处理 EXIF Orientation，预览长边 1800、缩略图 400 | 基本可用 | 需支持 16-bit 路径与损坏文件错误 |
| 预览 | 缩略图 → 预览图 → 数据 URL，前端防抖 70ms、陈旧响应丢弃 | 可用 | 需后端任务取消与高清预览 |
| 调色 | 曝光/对比/高光/阴影/色温/色调/饱和/自然饱和/清晰度/锐化/颗粒/暗角/磨皮 | 部分 | 缺白色/黑色色阶、HSL、曲线、LUT、白平衡 |
| 显示 | 适应窗口、100%、滚轮锚点缩放、拖动、分屏对比、按住看原图 | 可用 | 保留，补 200% 高清预览 |
| 状态管理 | Zustand，按图独立参数与历史，600ms 合并 | 可用 | 需扩展 HSL/曲线/项目/剪贴板 |
| 撤销重做 | 栈式，100 步上限，新编辑清空重做 | 可用 | 保留 |
| 导出 | 原尺寸渲染，JPG/PNG/WebP | 部分 | 缺导出设置、EXIF 保留 |
| 界面 | 专业暗色三栏工作台 | 可用 | 需增加基础/HSL/曲线/人脸面板 |
| 测试 | 前端 30 项、Rust 26 项 | 部分 | 缺测试照片集、异常与任务取消测试 |

## 2. 存在的问题

### P0（本轮必须处理）

1. **调色能力不完整**：`src-tauri/src/imaging/adjustments.rs` 缺少白色色阶、黑色色阶、HSL 八通道、RGB/色彩曲线、白平衡偏移、LUT；现有色温/色调只是简单通道缩放，不是真正的白平衡。
2. **参数定义重复**：`src/state/params.ts` 与 Rust `EditParams` 手工同步，字段漂移风险高；新增 HSL/曲线会放大问题。
3. **无项目持久化**：参数、历史、蒙版、导出设置无法保存/重新打开；原图路径、参数只能存在内存。
4. **无高清预览**：预览固定长边 1800，100% 以上必然模糊；导出虽为原尺寸，但预览与导出体验断层。
5. **后端不可取消**：旧请求仅靠前端丢弃结果，Rust 侧仍在完整计算；高频拖动会排队占用 CPU。见 `src/hooks/usePreviewRenderer.ts` 与 `src-tauri/src/commands/render.rs`。
6. **无人脸检测**：无模型、无命令、无叠加层，右侧“人像”仅有基础磨皮占位。
7. **滑杆无数值输入**：`src/components/ui/Slider.tsx` 只显示数值，不能键入精确值；重置依赖双击，缺少显式单项重置按钮。

### P1（影响正确性/性能）

8. **色彩精度**：内部全程 `u8` RGBA，PNG 16-bit 输入会被 `DynamicImage` 降为 8-bit；预览与导出同算法，但高位深信息丢失。
9. **内存上限粗放**：`ImageCache` 固定 6 张全分辨率 RGBA（24MP × 4B ≈ 96MB/张），最坏约 600MB；无内存预算与分块处理。
10. **导出固定 JPEG 92 / PNG / 无损 WebP**，无质量、色彩空间、元数据选项。
11. **缺少坏文件与部分失败的可读错误**：`load_paths` 只返回最后一个错误，多文件导入部分失败时前端看不到明细。
12. **撤销合并策略仅按 600ms + 同键**，拖动停止后若在窗口内再次微调会被合并，边界行为未测试。
13. **对比模式只支持分屏**，无左右/上下布局与同步缩放校验测试。

### P2（架构/工程）

14. `imaging` 目录职责混合：解码、编码、模糊、调色同层，缺独立引擎门面；本轮拆为 `image_engine/`。
15. 命令层直接返回 data URL，IPC 负载大；高清图会放大该问题，需要限制并走缩略/分块。
16. 无性能基准与测试照片集，无法回归清晰度/锐化/HSL 等改动。
17. Windows 未实机验证，`titleBarStyle: Overlay` 在 Windows 不生效。
18. README 仍为占位内容；第三方许可证表为空，模型引入后必须登记。

## 3. 需要修改/新增的文件

### 新增

- `src-tauri/src/image_engine/`：`mod.rs`、`decoder.rs`、`encoder.rs`、`buffer.rs`、`color.rs`、`geometry.rs`、`pipeline.rs`、`preview.rs`、`export.rs`、`project.rs`、`operations/`（light/color/hsl/curves/lut/detail/portrait/blur）
- `src-tauri/src/ai/`：`mod.rs`、`face.rs`、`model.rs`、`postprocess.rs`
- `src-tauri/src/commands/faces.rs`、`src-tauri/src/commands/project.rs`
- `src/components/panels/`：基础/HSL/曲线/人脸面板；`src/components/FaceOverlay.tsx`
- `tests/fixtures/` 测试照片集与生成脚本
- `docs/ROUND2_REPORT.md`

### 修改

- `src-tauri/Cargo.toml`：tract-onnx、tract-ndarray、sha2、chrono（或 time）、serde_json 已有
- `src-tauri/src/lib.rs`：注册新命令、AI 状态
- `src-tauri/src/state.rs`：缓存改为内存预算 + 引擎图片类型
- `src-tauri/src/imaging/*`：保留兼容包装，内部转调 `image_engine`
- `src-tauri/tauri.conf.json`：resources 打包模型
- `src/state/params.ts`、`src/state/editorStore.ts`、`src/types.ts`：HSL/曲线/项目/剪贴板
- `src/components/Inspector.tsx`、`CanvasStage.tsx`、`TopBar.tsx`、`StatusBar.tsx`、`ui/Slider.tsx`
- `src/lib/backend.ts`、`src/hooks/usePreviewRenderer.ts`
- `THIRD_PARTY_LICENSES.md`：登记 YuNet 模型与 tract

## 4. 本轮实施计划

1. **引擎重构**：建立 `image_engine` 门面，统一预览/导出入口，16-bit 输入路径，明确错误类型。
2. **专业调色**：补齐白色/黑色色阶、白平衡、HSL 八通道、RGB 与色彩曲线、基础 3D LUT（.cube），全部真实改像素。
3. **非破坏性**：参数/蒙版/导出设置统一为项目结构，JSON 保存与打开，参数复制粘贴，单项/全部重置。
4. **预览与渲染**：预览缓存键、高清渲染命令、请求 ID + 取消标志、分块导出预留。
5. **AI 人脸**：YuNet 2023mar ONNX（MIT，SHA256 `8f2383e4…`）经 tract-onnx CPU 推理，输出框/5 关键点/多人脸，进度事件与失败回退。
6. **前端**：面板分组、数值输入、单项重置、关键点叠加、多人脸切换、进度/错误提示。
7. **测试**：测试照片集 + Rust/前端自动化，覆盖解码、调色、撤销、项目、人脸、导出、取消、异常。
8. **收尾**：构建、测试、真实运行验证、许可证登记、`docs/ROUND2_REPORT.md`、本地提交。

## 5. 许可证与模型合规

- 计划采用 OpenCV Zoo 的 YuNet `face_detection_yunet_2023mar.onnx`，MIT License，允许商业使用与再分发，需保留版权与许可声明。
- 模型来源（Hugging Face 镜像 `opencv/face_detection_yunet`）、SHA256、输入输出格式将在加载器常量与许可证表中登记。
- 不引入 InsightFace 等“仅研究用途”权重；不打包任何未登记模型。

## 6. 验收口径

- 每个滑块都有后端真实像素处理与数值输入，禁止 CSS/透明度伪装。
- 原图只读；项目保存的是参数与蒙版，不保存预览图。
- 导出为原始分辨率；预览与导出调用同一 `pipeline::render`。
- 人脸框与关键点必须来自模型输出，未加载模型时明确报错，不生成模拟结果。
- macOS 实测；Windows 无环境则标注“未验证”，不伪造结果。
