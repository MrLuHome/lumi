# ROUND 8 — Face Parsing 基准

- 状态：**不支持 / 未验证**（没有执行任何 parsing 测量，因为没有可用 Provider）
- 结论：LEVEL 3 不使用 parsing，也不会用 68 点轮廓冒充 parsing。

## 1. 为什么没有数字

| Provider | `face_parsing` | 原因 |
|---|---|---|
| `yunet-5` | false | 模型只输出 5 点 |
| `apple-vision` | false | `VNDetectFaceLandmarksRequest` 只返回特征点，不返回逐像素语义标签 |
| 用户 ONNX | false | Model Hub 只接受 `face-68` 关键点契约，不接受 parsing 图 |
| Banuba / Maxine | 待验证 | 未获取 SDK、未采购、未运行 POC |

`ProviderCapabilities::verified_only` 会让 `faceParsing` 保持 `Unsupported`，
UI 的“分割”chip 显示为关闭状态，tooltip 为“不支持”。没有任何地方把稠密关键点
区域填充成解析图。

## 2. 本应测量的指标（未执行）

| 指标 | 定义 | 状态 |
|---|---|---|
| 像素准确率 | 预测标签与人工标注逐像素一致比例 | 未验证 |
| mIoU | 各类别 IoU 平均（皮肤/眉毛/眼/鼻/唇/头发/背景） | 未验证 |
| 边界 F1 | 边界带（2 px）内的精确率/召回率 | 未验证 |
| 置信度分布 | 逐像素或逐区域概率分布 | 未验证 |
| 耗时 / 内存 | 全分辨率下的推理时间与峰值 RSS | 未验证 |

## 3. 若要启用，需要什么

1. 一个明确允许静态照片、允许商业分发（或明确研究限制）的 parsing 模型；
2. 逐像素标注的评测集（如 CelebAMask-HQ 的许可需单独确认）；
3. 新的 `RegionKind`/`SemanticMask` 契约与坐标一致性测试；
4. 独立的质量门槛（mIoU 与边界 F1），不能沿用“关键点稳定性”门槛；
5. 单独的 UI 门控，不自动把 parsing 解锁与 LEVEL 3 绑定。

在这些条件满足前，`min_dense_points` 与稳定性通过都**不会**开启 parsing，
也不会把“脸缘 17 点”解释为皮肤分割边界。
