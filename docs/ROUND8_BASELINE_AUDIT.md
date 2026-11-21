# ROUND 8 基线审计（阶段 0）

基线提交：`3d90597`（第七轮主体）+ `09799e9`（收尾文档）。审计日期：2026-11-21。
审计方式：直接读取源码、测试与构建配置，不依据报告文字推断。

## 1. 已有能力与真实实现

| 能力 | 实际实现 | 结论 |
| --- | --- | --- |
| 人脸检测 | `src-tauri/src/ai/face.rs` · YuNet 2023mar（tract-onnx，CPU，640×640） | 可用，随包模型 SHA256 校验 |
| 关键点 | YuNet 5 点（右眼、左眼、鼻尖、左右嘴角） | 仅几何锚点，不是稠密轮廓 |
| 人脸身份 | `ai_engine/face_frame.rs` · UUID + FNV 稳定 ID + IoU 跟踪 | 可用，跨检测顺序稳定 |
| 项目持久化 | `face_reshape/identity.rs` · `ProjectFace`（UUID/bbox/初始点/score/模型版本） | 可用；重开需重新检测后匹配 |
| 手动变形 | `face_reshape/operations/manual.rs`（推/拉/放大/缩小/保护/恢复） | 可用，LEVEL 1 |
| 轮廓级塑形 | `face_reshape/face_mesh.rs` 脸型椭圆环 + 5 点锚点 | 可用，LEVEL 2 |
| LEVEL 3 参数 | `image_engine/params.rs` 已有 `EyeShape/NoseShape/MouthShape/ChinShape` 与算法 `operations/{eyes,nose,mouth,chin}.rs` | 算法存在，但渲染被 `LEVEL3_ENABLED=false` 清零 |
| 质量保护 | `face_reshape/quality.rs`（三角翻转、面积比、位移上限） | 可用 |
| 渲染管线 | `image_engine/pipeline.rs`：美肤 → 塑形 → 调色 → 清晰度/锐化/颗粒/暗角 | 预览与导出一致 |
| 撤销/重做 | 前端 zustand 每图独立历史 | 可复用 |
| 多人脸 | `EditParams.faces` / `reshape` 按 faceUuid 独立 | 可复用 |

## 2. 已有接口

- `LandmarkerProvider`（`face_reshape/landmarker.rs`）：`info/initialize/detect/to_image/confidence/unload`，输出 `LandmarkDetection { points, confidence, quality, dense, model }`。只有 `Yunet5Provider` 与 `DisabledDenseProvider` 两个实现。
- `FaceEngine::detect(model_path, preview, width, height, progress)`：返回原图像素坐标的框与 5 点。
- `FaceRepository::store/get/signature/apply_identities`：会话内人脸缓存、版本号与缓存键。
- `pipeline::render_with_report(image, params, faces, cancel, progress)`：塑形从 `faces` 读取，`FaceFrame` 是唯一几何入口。
- Tauri 命令：`face_model_status`、`detect_faces`、`reshape_capability`、`reshape_mesh`、`export_face_records`、`restore_faces`、`confirm_face_match`。

## 3. 已有测试

- `src-tauri/src/ai/face.rs` 无独立测试；`ai/model.rs` 有 SHA256 测试。
- `face_reshape/mod.rs`、`landmark.rs`、`landmarker.rs`、`face_mesh.rs`、`mesh.rs`、`operations/*`、`quality.rs`、`identity.rs` 均有单元测试。
- 集成测试：`tests/beauty.rs`、`tests/beauty_real.rs`、`tests/engine.rs`、`tests/face_detection.rs`、`tests/reshape_persistence.rs`、`tests/reshape_pipeline.rs`、`tests/raw_real.rs`。
- 前端：`src/**/*.test.ts(x)`（57 个用例），`npm run typecheck`、`npm run build`。

## 4. 可直接复用的模块

| 模块 | 复用方式 |
| --- | --- |
| `FaceFrame` / `FaceGeometry` | 保持为渲染几何入口，新增可选稠密轮廓字段，不另起数据结构 |
| `FaceRepository` / `FaceTracker` | 继续负责 UUID 稳定与缓存版本 |
| `identity::match_faces` | Provider 切换后仍用它匹配旧 UUID |
| `face_reshape/face_mesh.rs` + `mesh.rs` + `warp.rs` | 只在控制点生成阶段增加稠密轮廓点，不改变形/三角剖分/重采样 |
| `quality.rs` | 继续做三角翻转与位移上限；新增检测质量门控独立存在 |
| `tract-onnx` 0.23 | 用户 ONNX 模型解析与 CPU 推理，无需新增运行时 |
| `Image16` / `CancelToken` / `AppError` | 新 Provider 统一使用 |

## 5. 必须调整的接口

1. `FaceFrame` 增加可选 `dense`（语义化轮廓 + topology ID + provider），继续用 UUID 匹配；旧项目字段缺省为 `None`。
2. `face_reshape::LEVEL3_ENABLED` 由全局常量改为**按人脸质量门控**；没有合格稠密数据的脸仍然清零。
3. `reshape_capability` 的禁用原因改为指向“AI 人像面板的门控状态”，不再永久写死。
4. `detect_faces` 保留（YuNet 兼容路径）；新增 `face_analyze` 走统一 Provider Registry。
5. `FaceRepository::signature` 版本号在写入稠密结果时必须递增，保证渲染缓存失效。

## 6. 不允许破坏的兼容性

- 项目版本 3 的 JSON 字段与旧项目可打开；新增字段全部 `#[serde(default)]`。
- `FaceReshape` 既有参数键（`face/eyes/nose/mouth/chin/manual`）不重命名。
- 预览与导出共用同一管线；导出拒绝覆盖原图。
- 前七轮 RAW / ICC / GPU / 批量 / 预设 / 蒙版能力不修改。
- 没有高级模型时，LEVEL 1/2 与普通修图必须继续可用。

## 7. 当前缺失的能力（本轮要解决）

- 没有统一 `FaceIntelligenceProvider`，能力声明散落在 `LandmarkerProvider` 与 UI 字符串里。
- 没有拓扑注册表、语义 ID、坐标空间声明、按区域的能力证据。
- 没有真实稠密关键点 Provider：LEVEL 3 无合格数据。
- 没有用户自备 ONNX 模型导入、校验、隔离推理与许可证状态。
- 没有商业 SDK 的书面/实测证据（只有第四轮的二手评估）。
- 没有 Face Parsing / Matting Provider。本轮只交付接口与能力状态，不伪造分割。
- 没有 Provider 切换后“拓扑兼容 + 参数待确认”的独立报告。

## 8. 运行环境

- macOS 14（Apple Silicon M4，16 GB），Rust 1.95.0，Node v22.15.0。
- 内置 YuNet 模型位于 `src-tauri/resources/models/yunet/`，SHA256 已固定。
- 现有依赖：`tract-onnx 0.23`、`image 0.25`、`lcms2 6.3`、`wgpu 30.0.2`。
- 本机具备 `clang` 与 Apple Vision 框架，可做 macOS 原生桥接 POC；无 Windows 实机。
