# 第三方资产授权清单

本项目使用的所有非自研资产（代码依赖、模型代码、模型权重、数据集）的授权登记表，用于在开发与分发前快速核对法律风险。

## 更新规则

以下任一情况发生时，必须在同一次提交中同步登记到对应表格：

- 新增、升级或移除代码依赖（含间接依赖中引入的强传染性许可）
- 引入模型代码（训练/推理/数据处理脚本、CUDA 算子、第三方仓库 vendored 代码）
- 下载或引用模型权重（含微调产物、量化版本、LoRA 等衍生权重）
- 使用数据集（训练、微调、评测、演示素材）

每条记录需填写完整，不确定的字段写「待确认」并在备注中说明，不要留空或凭印象填写。许可证名称以实际 LICENSE 文件为准，不以 README 描述为准。

## 1. 代码依赖

| 名称 | 版本或 commit | 来源链接 | 许可证 | 是否允许商用 | 分发/再分发方式 | 备注（附加条款、CLA、引用要求） |
| --- | --- | --- | --- | --- | --- | --- |
| Tauri | 2.x | https://github.com/tauri-apps/tauri | MIT OR Apache-2.0 | 是 | 随桌面应用二进制分发 | 需保留版权与许可证声明 |
| React / React DOM | 19.x | https://github.com/facebook/react | MIT | 是 | 打包进前端资源 | 保留版权声明 |
| zustand | 5.x | https://github.com/pmndrs/zustand | MIT | 是 | 打包进前端资源 | — |
| lucide-react | 0.5x | https://github.com/lucide-icons/lucide | ISC | 是 | 打包进前端资源 | 保留 ISC 声明 |
| vite | 8.x | https://github.com/vitejs/vite | MIT | 是 | 仅构建期，不分发 | — |
| vitest | 5.x | https://github.com/vitest-dev/vitest | MIT | 是 | 仅测试期，不分发 | — |
| TypeScript | 5.9.x | https://github.com/microsoft/TypeScript | Apache-2.0 | 是 | 仅构建期，不分发 | — |
| image | 0.25.x | https://github.com/image-rs/image | MIT OR Apache-2.0 | 是 | 随应用二进制分发 | 解码/编码 JPG/PNG/WebP，传递依赖保持同许可 |
| tract-onnx | 0.23.x | https://github.com/sonos/tract | Apache-2.0 OR MIT | 是 | 随应用二进制分发 | 纯 Rust ONNX 推理；未使用 ONNX Runtime 二进制 |
| sha2 | 0.10.x | https://github.com/RustCrypto/hashes | MIT OR Apache-2.0 | 是 | 随应用二进制分发 | 校验模型 SHA256 |
| serde / serde_json | 1.x | https://github.com/serde-rs/serde | MIT OR Apache-2.0 | 是 | 随应用二进制分发 | — |
| thiserror | 2.x | https://github.com/dtolnay/thiserror | MIT OR Apache-2.0 | 是 | 随应用二进制分发 | — |
| rfd | 0.15.x | https://github.com/PolyMeilex/rfd | MIT | 是 | 随应用二进制分发 | 系统原生文件对话框 |
| tauri-plugin-opener | 2.x | https://github.com/tauri-apps/plugins-workspace | MIT OR Apache-2.0 | 是 | 随应用二进制分发 | — |

> 说明：`image` 的传递依赖（jpeg-decoder、png、image-webp 等）均为 MIT/Apache-2.0 系许可；发布前建议用 `cargo license` 做一次全量核对。

## 2. 模型代码

| 名称 | 版本或 commit | 来源链接 | 许可证 | 是否允许商用 | 分发/再分发方式 | 备注（附加条款、CLA、引用要求） |
| --- | --- | --- | --- | --- | --- | --- |
| YuNet（OpenCV Zoo 模型卡与推理说明） | face_detection_yunet_2023mar | https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet | MIT（OpenCV Zoo 仓库 LICENSE） | 是 | 仅以文档与许可证形式随包说明 | 论文：Yunet: A tiny millisecond-level face detector（Shiqi Yu et al.）；库代码不随包分发，仅使用模型权重 |

## 3. 模型权重

| 名称 | 版本或 commit | 来源链接 | 许可证 | 是否允许商用 | 分发/再分发方式 | 备注（附加条款、CLA、引用要求） |
| --- | --- | --- | --- | --- | --- | --- |
| YuNet face detection ONNX | 2023mar | https://huggingface.co/opencv/face_detection_yunet（镜像自 OpenCV Zoo） | MIT | 是（MIT 允许商业使用与再分发） | 打包进 Tauri `bundle.resources`，路径 `resources/models/yunet/` | 文件：`face_detection_yunet_2023mar.onnx`；SHA256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`；大小 232,589 字节；输入 `1×3×640×640` float32 BGR（0..255）；输出 12 个张量：`cls_8/16/32`、`obj_8/16/32`、`bbox_8/16/32`、`kps_8/16/32`；CPU 推理（tract）。必须随包保留 MIT LICENSE 原文（`resources/models/yunet/LICENSE`） |
| 皮肤区域分割权重（第三轮） | — | — | — | 未打包 | 不随包分发 | **当前没有可确认允许商用的权重，因此未打包任何分割模型。** 评估过 BiSeNet Face Parsing / CelebAMask-HQ 系权重、InsightFace 系解析模型与 MediaPipe Face Mesh：前两者的训练数据（CelebAMask-HQ 等）与权重条款多限制研究用途，仓库代码许可证不能推断权重可商用；MediaPipe Face Mesh 为 Apache-2.0，但 tflite→ONNX 转换后的权重分发与 tract 算子兼容性尚未完成法律与技术验证。第三轮实际使用自研关键点几何蒙版（YuNet 5 点 + 肤色似然），未使用任何未审查授权的权重。`skin_segmentation/model.rs` 保留注册表与商用许可闸门，后续拿到书面商用授权后按 SHA256 与许可证白名单校验加载。 |

## 4. 数据集

| 名称 | 版本或 commit | 来源链接 | 许可证 | 是否允许商用 | 分发/再分发方式 | 备注（附加条款、CLA、引用要求） |
| --- | --- | --- | --- | --- | --- | --- |
| 项目测试照片集 | 第二轮 | `src-tauri/tests/fixtures/images/` | 待确认 | 仅用于自动化测试，不得进入发布产物 | 仅存在于源码测试目录，不打包进应用 | 由上一轮临时样张派生（暗光/高光/拼贴/EXIF/16-bit 等变体）；原始人像来源与肖像授权尚未确认，发布前必须替换为自有或已授权素材 |
| 第三轮合成测试图 | 第三轮 | `src-tauri/tests/beauty.rs` 内程序生成 | 自研，无第三方权利 | 是 | 不落盘为资产，测试运行时生成 | 纯色肤色 + 确定性纹理/瑕疵，覆盖不同肤色、暗光、强光、多人脸、高分辨率；不包含任何真实人物肖像，因此无肖像权问题 |

## 需特别警惕的授权情形（核对清单）

引入资产时逐项过一遍，命中任一项都要在备注列写清具体条款原文位置：

- [ ] 权重标注「仅限研究 / 非商业用途」，或使用条款独立于仓库代码许可证（如模型卡上的自定义协议）
- [ ] 数据集仅限学术研究，或禁止再分发原始样本 / 衍生数据
- [ ] GPL / AGPL / SSPL 等传染性许可，或依赖中包含此类间接依赖
- [ ] 要求署名、引用论文、保留版权声明、标注「Powered by」等归属义务
- [x] 商用需单独申请商业授权（双许可模式）—— YuNet 为 MIT，无此限制
- [ ] 需签署 CLA 或额外使用协议才能使用
- [ ] 许可证版本不明、无 LICENSE 文件，或仓库与权重协议互相冲突
- [ ] 训练数据来源本身存在版权或肖像权争议（人像类项目尤其注意）—— 测试照片集待确认

清单只用于提示核对方向，具体结论以各资产许可证原文为准。

## 当前状态

- 代码依赖：已登记主要直接依赖，未发现 GPL / AGPL 传染性许可。
- 模型代码与权重：YuNet 2023mar 已登记来源、版本、SHA256、输入输出格式与 MIT 许可证；模型仅通过官方镜像获取，未做任何微调或再量化。
- 皮肤分割：**未打包任何第三方分割权重**。本轮使用自研关键点几何蒙版与 YCbCr 肤色似然；`ai_engine::skin_segmentation::model` 中的注册表为空，加载前会校验许可证白名单与 SHA256，非商用权重会被拒绝。
- 数据：自动化测试照片集仅用于本地测试，来源授权待确认；第三轮新增测试全部使用程序生成的合成图，不含真实肖像；未引入训练数据集。
