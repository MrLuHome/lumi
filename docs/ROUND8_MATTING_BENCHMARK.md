# ROUND 8 — Hair / Face Matting 基准

- 状态：**不支持 / 未验证**（没有可用 Matting Provider，也没有执行测量）
- 结论：不把关键点轮廓、皮肤区域或二值蒙版当成 Alpha Matting。

## 1. 能力现状

| 能力 | `yunet-5` | `apple-vision` | 用户 ONNX | 状态 |
|---|---|---|---|---|
| 人脸 Matting（发丝级 Alpha） | false | false | false | 不支持 |
| 皮肤分割 | false | false | false | 不支持 |
| 头发分割 | false | false | false | 不支持 |
| 面部网格（3D Mesh） | false | false | false | 不支持 |

Apple Vision 的 `VNFaceLandmarks2D` 只提供特征点；它既不是逐像素分割，
也不含发丝 Alpha。`face_mesh` 字段在桌面 Provider 中均为 `None`。

## 2. 本应测量的指标（未执行）

| 指标 | 定义 | 状态 |
|---|---|---|
| SAD | 预测 Alpha 与 GT Alpha 的绝对差之和 | 未验证 |
| MSE / Grad / Conn | 复合 Matting 误差（含梯度与连通性） | 未验证 |
| 发丝保留率 | 细发区域（≤2 px）的 Alpha 召回 | 未验证 |
| 边缘过渡宽度 | Alpha 从 0.1 到 0.9 的像素宽度分布 | 未验证 |
| 耗时 / 内存 | 全分辨率单张耗时与峰值 RSS | 未验证 |

## 3. 约束

1. 即使未来接入 Matting，也必须与 `face_reshape` 完全解耦：
   蒙版只影响图像合成，不参与几何变形控制点。
2. 不得把 68 点外唇/脸缘连成多边形填充后称为“面部蒙版”。
3. 不得把 `SemanticMask`（二值/灰度区域）宣传为“发丝级抠图”。
4. 引入任何 Matting 权重前，必须先确认许可证允许静态照片与商业分发。

## 4. 与 LEVEL 3 的关系

LEVEL 3 的五官精修只消费 `DenseRegion` 控制点，不依赖 Matting。
因此 Matting 缺失**不会**阻塞 LEVEL 3；反过来，LEVEL 3 通过也**不会**解锁 Matting。
