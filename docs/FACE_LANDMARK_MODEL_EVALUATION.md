# 第四轮 · 稠密人脸关键点模型评估

评估日期：2026-11-20
结论先行：**LEVEL 3 保持禁用**。MediaPipe Face Landmarker（478 点）在 macOS 上完成官方模型与运行时验证，模型与运行时均为 Apache-2.0；但尚未完成 Tauri 内部分发集成、Windows 实机验证和遮挡/大角度质量评估，因此不得作为已完成功能交付。

## 1. 为什么需要稠密关键点

第三轮随包发布的 YuNet 只输出 5 个关键点（双眼、鼻尖、左右嘴角），足以建立人脸坐标系和轮廓级网格，但无法可靠定位：

- 上下眼睑与眼角（精确放大眼睛、改变眼型）
- 鼻翼、鼻孔边界（鼻翼宽度）
- 上下唇线、嘴角（唇厚、嘴角）
- 下颌线细分、下巴尖度

用人脸框或 5 点推算这些边界属于伪造关键点，会直接产生穿帮变形，因此本轮不采用。

## 2. MediaPipe Face Landmarker（478 点）

### 2.1 官方模型

| 项目 | 值 |
| --- | --- |
| 官方文档 | https://ai.google.dev/edge/mediapipe/solutions/vision/face_landmarker |
| 模型包 | `face_landmarker.task`（float16，版本目录 `1`） |
| 下载地址 | https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task |
| 下载日期 | 2026-11-20 |
| 文件大小 | 3,758,596 字节 |
| SHA-256 | `64184e229b263107bc2b804c6625db1341ff2bb731874b0bcc2fe6544e0bc9ff` |
| 输出点数 | 每脸 478 个 3D 关键点 + 52 个 blendshape（可选） |

模型包内资产（`unzip -l` 实测）：

| 资产 | 大小（字节） | 日期 |
| --- | --- | --- |
| `face_detector.tflite`（BlazeFace short-range） | 229,746 | 2023-05-03 |
| `face_landmarks_detector.tflite`（FaceMesh-V2） | 2,553,590 | 2023-05-03 |
| `geometry_pipeline_metadata_landmarks.binarypb` | 19,376 | 2023-05-03 |
| `face_blendshapes.tflite` | 955,312 | 2023-05-03 |

### 2.2 许可证

| 对象 | 许可证 | 依据 |
| --- | --- | --- |
| BlazeFace short-range 权重 | Apache License 2.0 | 官方模型卡 “LICENSED UNDER Apache License, Version 2.0” |
| FaceMesh-V2 权重 | Apache License 2.0 | 官方模型卡 “LICENSED UNDER Apache License, Version 2.0” |
| Blendshape V2 权重 | Apache License 2.0 | 官方模型卡 “LICENSED UNDER Apache License, Version 2.0” |
| MediaPipe Tasks 运行时（Python 1.2.3） | Apache License 2.0 | MediaPipe 仓库 LICENSE |

Apache-2.0 允许商业使用、修改与再分发，但分发时必须保留许可证与 NOTICE，并对修改文件作出说明。当前项目尚未把模型与运行时打进安装包，因此不涉及分发；一旦接入，需要：

1. 在安装包内附带 Apache-2.0 LICENSE 与 NOTICE；
2. 在“关于/第三方许可”页面列出 MediaPipe 与模型版本、SHA-256；
3. 不删除模型包内任何资产，不做未声明的权重转换。

### 2.3 macOS 实测（已完成）

环境：macOS，MediaPipe Python 1.2.3，官方 `face_landmarker.task`，`RunningMode.IMAGE`，`num_faces=4`。

| 测试图 | 尺寸 | 检出人脸 | 每脸点数 | 推理中位耗时 | 重复运行最大偏差 |
| --- | --- | --- | --- | --- | --- |
| `portrait.jpg` | 1680×2240 | 1 | 478 | 3.70 ms | 0 |
| `portrait_hires.jpg` | 3360×4480 | 1 | 478 | 4.25 ms | 0 |
| `portrait_dark.jpg` | 1680×2240 | 1 | 478 | 3.76 ms | 0 |
| `portrait_bright.jpg` | 1680×2240 | 1 | 478 | 3.63 ms | 0 |
| `group.jpg` | 1680×2240 | 4 | 478×4 | 10.42 ms | 0 |
| `no_face.jpg` | 1600×1200 | 0 | — | 0.66 ms | — |

- 进程峰值 RSS：约 242 MiB（含 Python、NumPy、MediaPipe 运行库）。
- 重复运行同一输入 5 次，关键点最大偏差为 0（IMAGE 模式无随机性，稳定）。
- 角度测试（同一人像缩放后旋转/镜像）：原图、镜像、逆时针 25°/45°/75°、顺时针 35° 均检出 1 张脸、每脸 478 点。注意这是平面旋转，不等于真实侧脸轮廓测试。

### 2.4 Windows 与桌面分发（未完成，阻塞 LEVEL 3）

- 本机无 Windows 环境，**未完成 Windows 实机验证**。
- MediaPipe Tasks 官方桌面预编译库没有可直接链接的 macOS/Windows 通用 C++ 二进制；可选路径：
  1. **Python sidecar**：打包独立 Python 运行时 + `mediapipe` 包。体积增加约 100 MB 以上，启动与进程间通信复杂，但官方支持 macOS/Windows。
  2. **C++ 源码构建 + Rust FFI**：需要 Bazel/CMake 工具链、平台矩阵构建与签名，工程成本高。
  3. **Web Tasks（WASM）**：`@mediapipe/tasks-vision` 在 Tauri WebView（WKWebView / WebView2）内运行，需要本地托管 WASM 与模型，验证 WebGL/WASM SIMD 与内存上限。
- 以上任一路径都需要独立的技术验证与 Windows 回归测试，本轮不做临时转换（例如为迁就 ONNX Runtime 擅自转换 `.task`），也不打包未经审查的权重。

### 2.5 结论

**MediaPipe Face Landmarker 是当前最合适的 LEVEL 3 候选**：点数充足、许可证清晰、macOS 精度与稳定性良好、单脸约 4 ms。但在 Windows 验证、桌面分发方案和真实侧脸/遮挡质量评估完成前，LEVEL 3 继续禁用；UI 必须显示禁用原因，不能用无效滑块冒充。

## 3. 商业 SDK 评估

| SDK | 关键点能力 | 平台 | 授权方式 | 本轮结论 |
| --- | --- | --- | --- | --- |
| NVIDIA Maxine AR SDK | 人脸检测、面部关键点（官方文档约 126 点）、表情/视线 | Windows / Linux，需要 RTX GPU | NVIDIA 软件许可 / AI Enterprise，商业授权需商务确认 | 不采用：强制 NVIDIA GPU，macOS 无支持 |
| Banuba Face AR SDK | 100+ 关键点、3D 网格、表情 | iOS / Android / Web / Windows / macOS | 商业授权，按年/按应用计费 | 待商务评估；无许可证不下发 |
| Luxand FaceSDK | 约 70 个面部特征点 | Windows / macOS / Linux / iOS / Android | 商业授权 | 点数不足以支撑精细五官网格 |
| Visage Technologies visage\|SDK | 66 点 + 稠密网格模块 | 跨平台（含桌面） | 商业授权，模块单独计价 | 待商务评估；稠密网格需额外许可 |
| ArcSoft ArcFace / ArcAR | 106 点、稠密网格 | Windows / Linux / Android / iOS | 商业授权（区域限制） | 待商务与合规评估 |
| DeepAR | 100+ 点、面部网格 | Web / 移动 / 桌面 | 商业授权 | 待商务评估 |

商业 SDK 均无法在本仓库内直接验证许可证与运行效果；未取得书面授权前不得下载 SDK、不得打包其运行时。

## 4. 当前交付的替代方案

- **LEVEL 1**：用户手动推拉/放大缩小/保护/恢复，作用在真实图像像素与位移场上，不依赖稠密关键点。
- **LEVEL 2**：基于 YuNet 检测框与 5 点建立轮廓环，只做整体宽度、长度、左右轮廓和下颌的保守调整；UI 明确标注“轮廓级近似调整，不代表精确面部关键点识别”。
- **LEVEL 3**：保留统一 `LandmarkerProvider` 接口与参数结构，生产渲染通过 `LEVEL3_ENABLED=false` 清零；能力查询返回禁用原因。

## 5. 后续验收清单（进入 LEVEL 3 前必须全部通过）

1. 选定桌面分发路径并完成 macOS + Windows 安装包实机测试。
2. 验证 478 点在侧脸 30°/60°、俯仰 20°、眼镜、口罩/手部遮挡、多人重叠下的稳定性阈值。
3. 建立关键点编号到语义部位的映射表（眼睑、唇线、鼻翼等），并写单元测试。
4. 建立稠密关键点版本与项目持久化记录的兼容策略。
5. 完成 Apache-2.0 许可与 NOTICE 分发审查。
6. 通过网格质量保护（三角翻转、位移上限）与真实人像回归测试后，才可打开 `LEVEL3_ENABLED`。
