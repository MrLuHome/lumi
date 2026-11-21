# ROUND 8 — Provider API

- 代码：`src-tauri/src/face_intelligence/provider.rs`、`capability.rs`、`types.rs`、`coordinate.rs`
- 原则：Provider 只负责“模型输出 → LUMI 统一数据”，核心引擎不依赖任何厂商类型或关键点编号。

## 1. Trait

```rust
pub trait FaceProvider: Send + Sync {
    fn id(&self) -> &str;
    fn descriptor(&self) -> ProviderDescriptor;
    fn analyze(
        &self,
        input: &FaceInput<'_>,
        options: &AnalysisOptions,
        cancel: &CancelToken,
    ) -> Result<FaceAnalysisResult, FaceError>;
    fn health_check(&self) -> ProviderHealth;
    fn unload(&self) {}          // 默认空实现，由 Provider 自行释放模型内存
    fn loaded(&self) -> bool { true }
}
```

- `FaceInput` 持有 `Image16`（16-bit RGBA）与原始宽高；Provider 可自行缩放到推理尺寸，
  但**返回值坐标必须映射回输入图**（0..1 归一化）。
- `AnalysisOptions`：`max_faces=16`、`min_confidence=0.35`、`include_masks=false`、
  `include_mesh=false`、`max_edge=2048`。
- `sanitize_result` 在 Provider 返回后统一去除：低于置信度的人脸、非有限坐标、
  空区域，并按 `include_masks/include_mesh` 裁掉未请求的数据。

## 2. 错误

| 变体 | 含义 |
|---|---|
| `Unavailable` | 当前平台/安装包不提供该 Provider |
| `NotInitialized` | 模型未加载或已卸载 |
| `InvalidInput` | 空图、尺寸超限 |
| `Inference` | 模型损坏、算子失败、内存不足 |
| `Timeout` | 推理超时，结果被放弃 |
| `Unsupported` | 明确不支持的能力 |
| `Cancelled` | 用户取消 |

错误必须可读；禁止“失败后悄悄改用别的 Provider”或返回空结果冒充成功。

## 3. 能力声明与证据

`ProviderCapabilities` 是布尔能力位（检测/稀疏/稠密/眼睑/鼻/唇/网格/parsing/皮肤/头发/Matting/多人脸/离线/静态图/最大分辨率）。
`CapabilityEvidence` 为每个能力记录等级：

| 等级 | 含义 | 是否放行 LEVEL 3 |
|---|---|---|
| `Verified` | 本机真实照片实测通过并记录数字 | 是 |
| `Declared` | 文档或模型卡声明 | 否 |
| `Unverified` | 无证据，或实测未通过 | 否 |
| `Unsupported` | 模型确实没有该输出 | 否 |

`ProviderCapabilities::verified_only(&evidence)` 只保留 `Verified` 的位；
`descriptor.verified_capabilities` 由它生成，门控只看这一份。

质量探针通过后 `apply_verification` 会把相关证据升级为 `Verified`，并把测量文本写入
`measurement`（例如“3 次重复最大偏差 0.000% 眼距；1 张人脸结构校验全部通过”）。
探针未通过时，`Declared` 会被降为 `Unverified` 并写明失败原因。

## 4. 许可证元数据

`LicenseInfo { name, url, state, note }`，`LicenseState`：

| 状态 | 本机可用 | 可作为官方商用默认 |
|---|---|---|
| `VerifiedCommercial` | 是 | 是 |
| `VerifiedPersonal` | 是 | 否 |
| `SystemFramework` | 是（按平台条款） | 否 |
| `ResearchOnly` | 是 | 否 |
| `UserUnverified` | 是（用户自担责任） | 否 |
| `Prohibited` / `Incompatible` / `Removed` | 否 | 否 |

用户自备模型**永远**是 `UserUnverified`，即使 manifest 写了 MIT 或“允许商用”。
目前没有任何 Provider 处于 `VerifiedCommercial` 以外被当作官方商用模型。

## 5. 统一输出

```text
FaceAnalysisResult
├── faces: Vec<FaceResult>
│   ├── face_uuid, bounding_box(0..1), confidence
│   ├── landmarks: Vec<Landmark { x, y, confidence, semantic_id }>
│   ├── regions: Vec<DenseRegion { kind, points, closed }>
│   ├── masks: Vec<SemanticMask>      // 当前没有 Provider 输出
│   └── mesh: Option<FaceMesh>        // 当前没有 Provider 输出
├── provider_id / provider_version / model_version
├── topology_id
└── processing_time_ms / image_width / image_height / warnings
```

- `semantic_id` 是稳定字符串（如 `eye.left.lid.3`），不承诺与任何厂商索引一致。
- 坐标：原点左上，x 向右、y 向下，单位归一化。
- `topology_id` 描述点集语义；跨拓扑结果不会被混用（`topology::validate` 会报错）。

## 6. 现有 Provider

| ID | 拓扑 | 点 | 许可 | 状态 |
|---|---|---|---|---|
| `yunet-5` | `yunet-5` | 5 | 已验证·允许商用（随包） | 已验证 |
| `apple-vision` | `apple-vision-85` | 85 | 系统框架·需遵守平台条款 | macOS 已验证 |
| `onnx:<id>` | `face-68` | 68 | 用户自备·未验证 | 代码路径验证；真实模型样本未验证 |

`apple-vision` 在非 macOS 平台不会注册；`onnx:*` 只有在 Model Hub 校验通过且被启用后才注册。

## 7. 实现新 Provider 的检查清单

1. 输出只使用 `types.rs` 的结构，不把厂商类型泄漏到返回值。
2. 坐标映射回输入图，并处理 EXIF 已旋转后的图像（输入已是正立像素）。
3. 左右眼/左右眉使用**被摄者**视角的 `RegionKind`；厂商命名相反时在 Provider 内部归一化
   （Apple Vision 已用 `canonicalize_sides()` 按图像 x 处理）。
4. 没有的输出就是空，不插值、不猜测。
5. 置信度不可得时如实说明（Apple Vision 无逐点置信度，使用 `observation.confidence`，
   本机实测恒为 1.000，已在文档标注）。
6. `unload()` 释放模型与大缓冲；重载失败必须返回 `NotInitialized` 或 `Inference` 原因。
7. 单元测试至少覆盖：坐标归一化、越界裁剪、空输出、错误路径、拓扑不匹配。
