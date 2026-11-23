# Selfie Multiclass 256×256 ONNX 随包说明

## 1. 本文件随包的内容

- `selfie_multiclass_256x256.onnx`（16,453,400 B）
- `LICENSE`（Apache License 2.0 全文，11,357 B）

## 2. 来源与转换口径

| 项 | 值 |
|---|---|
| 官方原始权重 | `https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/1/selfie_multiclass_256x256.tflite`（16,371,837 B，SHA256 `c6748b1253a99067ef71f7e26ca71096cd449baefa8f101900ea23016507e0e0`） |
| 官方模型卡 | `https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf`（原文 `LICENSED UNDER Apache License, Version 2.0`） |
| 现成 ONNX 来源 | `https://github.com/16sakuraa/PortraitColorLab`，路径 `web/models/selfie_multiclass_256x256.onnx` |
| 转换工具 | tf2onnx 1.16.1，opset 13，NCHW 输入/输出（第三方声明） |
| 转换细节 | 3 个 Resize 节点坐标模式由 `tf_half_pixel_for_nn` 改为 `asymmetric`；第三方声明与 TFLite 输出误差 ≤ 0.0001（**本轮未独立复现**） |
| 第三方随包许可 | `web/models/LICENSE-selfie-multiclass`（Apache-2.0 全文，已原样复制为随包 `LICENSE`） |
| 本 ONNX 实测 | 16,453,400 B，SHA256 `485774fe148451ca3f6c53626654b9a5ea34d291fa07ddac53725aa35ec07587` |

## 3. 输入输出契约（接入方必读）

- 输入：`input_29`，F32，NCHW `[1,3,256,256]`，RGB，值域 **[-1,1]**（`(x/127.5)-1`）。
- 输出：`Identity`，F32，NCHW `[1,6,256,256]`，**logits，不是概率**。
- 类别（0-based）：`0 background / 1 hair / 2 body-skin / 3 face-skin / 4 clothes / 5 others`。
- 后处理必须**自行沿 axis=1（类别维）做 softmax**，再取 `p(2)+p(3)` 作为皮肤置信度。
  图内 6 个 Softmax 都在 ViT 注意力内部，不是类别维 softmax；直接把负数当概率会得到错误蒙版。

## 4. 许可四字段与未披露项

| 字段 | 结论 |
|---|---|
| 权重许可 | Allowed（模型卡原文 Apache-2.0） |
| 商业使用 | Allowed（Apache-2.0 §2） |
| 再分发 | Allowed（Apache-2.0 §4，需保留版权/许可/NOTICE） |
| 训练数据许可 | **Unverified**：模型卡只写 `trained on thousands of samples with diverse identities`，未命名数据集、未声明许可 |

训练数据缺口在 UI 状态与 `docs/MODEL_LICENSE_REGISTER.md` 中如实披露；**不得**说成"整条数据链路已清"。

## 5. 待办（不阻塞本轮接入）

1. 我方从官方 TFLite 用固定版本 tf2onnx **自行重跑转换**，固化脚本与产物 SHA，替换第三方二进制；替换后必须同步更新
   `src-tauri/src/ai_engine/model_registry/builtin.rs::SELFIE_SKIN` 的 `size_bytes` / `sha256` 与本文件第 2 节。
2. 独立复现转换产物与官方 TFLite 的数值一致性（第三方声明 ≤0.0001）。
3. 真实人像（含深肤色 / 暗光 / 侧脸 / 遮挡）精度与耗时基线；模型卡明示深肤色退化（深色组 mean IoU 68.25 vs 全体 77.23）。
