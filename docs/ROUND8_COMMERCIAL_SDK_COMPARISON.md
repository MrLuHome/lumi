# ROUND 8 — 商业 SDK 比较

- 日期：2026-11-21
- 结论先行：本轮**没有采购任何商业 SDK**，**没有把任何商业 SDK 二进制放进发行包**。
  Banuba 与 NVIDIA Maxine 只做了公开资料调查，POC 未执行；Apple Vision 已完成真实 POC。
- 表内“待验证”表示没有实测或没有书面确认，不代表支持或不支持。

## 1. 比较表

| 维度 | Banuba Face AR | NVIDIA Maxine AR | Apple Vision |
|---|---|---|---|
| macOS | 待验证（未获取 SDK） | 公开资料未列出 macOS 支持；未实测 | **实测**（macOS 14 / Apple M4） |
| Windows | 待验证 | 待验证（需 NVIDIA GPU，未实测） | 不支持（系统无 Vision 框架） |
| 68 点 | 待验证 | 待验证 | 不适用：返回自有语义区域（85 点） |
| 126 点 | 待验证 | 待验证 | 不适用 |
| Face Mesh | 待验证 | 待验证 | 不支持（`VNDetectFaceLandmarksRequest` 不带 3D 网格） |
| Face Parsing | 待验证 | 待验证 | 不支持（关键点请求不输出逐像素语义） |
| Skin Segmentation | 待验证 | 待验证 | 不支持 |
| Hair Segmentation | 待验证 | 待验证 | 不支持 |
| 离线处理 | 待书面确认 | 待书面确认 | **实测**：本机进程内，无网络请求 |
| 商业分发 | 待合同确认（未询价） | 待合同确认（未询价） | 需遵守 Apple 平台条款，具体条款待法务确认 |
| 价格 | 待正式报价 | 待询价 | 无单独授权费（随 macOS 提供），未做商业条款确认 |
| 静态照片输入 | 待验证 | 待验证 | **实测**（5 张样张，含 24MP 降采样与多人脸） |
| 多人脸 | 待验证 | 待验证 | **实测**（合影 4 张脸） |
| 24MP / 45MP 输入 | 待验证 | 待验证 | 24MP 已验证（按最长边缩放分析）；45MP 未验证 |

## 2. Apple Vision POC（已执行）

- 桥接：`vision_bridge.m`（Objective-C，ARC）+ `apple_vision.rs`，Rust 侧只接收 JSON 化的
  归一化点与区域类型；`VNRecognizedPoint2D` 等厂商类型不进入核心结构。
- 接口：`VNDetectFaceLandmarksRequest` + `VNDetectFaceRectanglesRequest`；
  区域点通过 `-pointsInImageOfSize:` 获取，返回左下原点，桥接层转换为左上原点归一化坐标。
- 左右语义：Vision 的 `leftEye/rightEye` 为观察者视角；桥接后
  `canonicalize_sides()` 按图像 x 交换为被摄者视角，测试已校验。
- 实测数字（2026-11-21，debug 测试构建）：
  - 85 个稠密点/脸；5 张样张全部检出；
  - 单次分析 58–71 ms（1400 px 最长边，含 4 人合影）；
  - 质量探针 3 次 29/27/27 ms，最大偏差 0.000000，ratio 0.000000；
  - 置信度：24 个样本全部为 1.000（Vision 的 `observation.confidence` 饱和，
    无逐点置信度；不得把 1.000 解释为“逐点质量完美”）。
- 限制：仅 macOS；不提供 parsing/matting/mesh；商业分发需遵守 macOS 应用条款（待法务确认）。

## 3. Banuba POC（未执行）

未申请试用、未购买、未下载 SDK、未引入任何二进制或示例模型。因此：

- 无法确认其“仅分析能力”是否可单独授权；
- 无法确认能否绕开其渲染引擎接入 LUMI 自己的 Warp；
- 无法确认离线激活、MAU/设备限制、订阅到期后的已发行版本处理；
- 无法确认静态照片（非摄像头）分析的官方支持程度与导出格式。

上述字段全部为“待验证”，不得在销售材料中声称支持。

## 4. NVIDIA Maxine POC（未执行）

未安装 Maxine SDK、未引入 TensorRT/CUDA 依赖。公开资料显示其 AR SDK 面向
Windows/Linux + NVIDIA GPU；任务书明确要求“不能作为 macOS 通用 Provider”。
即使未来接入，也只能作为 Windows+NVIDIA 可选 Provider，不能替代 macOS 方案。

## 5. 结论

1. 目前唯一可用的稠密关键点 Provider 是 macOS 系统框架 Apple Vision，已真实跑通。
2. Banuba / Maxine 不具备任何可宣称的实测能力；价格与商业条款均为待验证。
3. 本轮不触发采购，不集成商业 SDK，发行包不含其二进制。
4. 若未来需要跨平台商业能力，必须先取得书面条款并做独立 POC 工程，再谈集成。
