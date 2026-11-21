# ROUND 8 — Face Topology 与 LEVEL 3 门控

- 代码：`face_intelligence/topology.rs`、`types.rs`、`dense.rs`、`quality.rs`、`face_reshape/face_mesh.rs`
- 原则：语义先于编号。变形引擎只读 `RegionKind`，不认识任何模型的点索引。

## 1. 已登记的拓扑

| 拓扑 ID | 版本 | 点数 | 稠密 | 来源 | 状态 |
|---|---|---|---|---|---|
| `yunet-5` | 2023mar | 5 | 否 | OpenCV Zoo YuNet（MIT，随包） | 已验证 |
| `apple-vision-landmarks` | VNDetectFaceLandmarksRequest | 不固定 | 是 | macOS 系统框架（无随包权重） | macOS 已验证 |
| `face-68` | iBUG-300W | 68 | 是 | 标注约定（未随包权重） | 结构已定义；真实用户模型未验证 |
| `face-126` | nvidia-maxine-ar | 126 | 是 | NVIDIA 文档 | 仅登记结构；逐点语义待供应商确认，未接入 |

`Topology` 校验规则：`topology_id` 必须匹配；各区域点数落在 `min_points..max_points`
（`face-68` 为精确值）；缺失必需区域或点数超界时返回可读错误，不自动修复。

## 2. 语义区域与来源

| RegionKind | YuNet 5 | Apple Vision | face-68 | 用途 |
|---|---|---|---|---|
| FaceContour | — | faceContour | 0..16 | 脸型/下颌 |
| RightEyebrow | — | rightEyebrow | 17..21 | 眉形（只读） |
| LeftEyebrow | — | leftEyebrow | 22..26 | 眉形（只读） |
| Nose | 鼻尖 1 点 | nose（含鼻翼） | 27..35 | 鼻部精修 |
| NoseCrest | — | noseCrest | — | 鼻梁中线（只读） |
| MedianLine | — | medianLine | — | 面部中线（只读） |
| RightEye | 中心 1 点 | rightEye | 36..41 | 眼部精修 |
| LeftEye | 中心 1 点 | leftEye | 42..47 | 眼部精修 |
| OuterLips | 左右嘴角 2 点 | outerLips | 48..59 | 唇形 |
| InnerLips | — | innerLips | 60..67 | 牙齿保护参考（只读） |

左右规则：统一按**被摄者**视角。Apple Vision 的 `leftEye/rightEye` 按观察者视角命名，
桥接层用 `canonicalize_sides()` 按图像 x 归一化并重建 `semanticId`；
集成测试 `apple_vision_returns_real_dense_regions` 校验图像左侧区域语义为被摄者右眼。

## 3. 5 点与 68 点的边界

- 5 点：只有眼中心、鼻尖、左右嘴角。`FaceResult.regions` 为空，
  `dense::assemble_dense` 得到 0 点，`level3_gate` 直接锁定。
- 68 点：按 iBUG 语义分组为 8 个区域；**不插值**，**不升级**为网格。
- 5 点不会被“补”成眼睑/唇线；68 点不会被当作 parsing 或 Matting。

## 4. LEVEL 3 门控（quality.rs）

### 4.1 阈值（默认，UI 只读展示）

| 项 | 值 |
|---|---|
| `min_confidence` | 0.45 |
| `max_stability_ratio` | 0.02（2% 眼距） |
| `min_eye_width` | 0.018（归一化宽度） |
| `min_mouth_width` | 0.02 |
| `min_dense_points` | 20 |

### 4.2 解锁顺序

1. Provider 的 `verified_capabilities.dense_landmarks == true`（只有 `Verified` 证据放行）；
2. 该 Provider 已有本机质量探针结果，且 `stability.passed == true`；
3. 单张人脸结构校验通过（置信度、点数、眼睑宽度、鼻/唇宽度、区域顺序）；
4. 逐项门控：

| 门控 | 依赖的已验证能力 | 依赖的结构检查 |
|---|---|---|
| `eyes` | `eye_landmarks` + `eyelid_landmarks` | 左右眼各 ≥4 点且宽度 ≥ 0.018 |
| `nose` | `nose_landmarks` | 鼻部 ≥6 点且宽度合理 |
| `mouth` | `lip_landmarks` | 外唇闭合且宽度 ≥ 0.02 |
| `chin` | `dense_landmarks` | 脸缘 ≥9 点且顺序合理 |

任何一项缺失时，对应 `FeatureGate.enabled=false`，`reason` 写入具体原因；
`summary` 汇总“已解锁/未解锁”。前端只渲染 `enabled=true` 的滑块组，
未解锁组显示原因文本，不显示无效滑块。

### 4.3 实测结论（macOS Apple Silicon M4，2026-11-21）

- Apple Vision：`probe(portrait.jpg, 3 runs)` → `verified=true`，
  `max_deviation=0.000000`，`ratio=0.000000`（0% 眼距）；
  探针后 5 张样张（含 4 人合影、过曝、深色、高分辨率）全部 `eyes/nose/mouth/chin = true`。
- YuNet：无稠密能力，探针直接返回“不适用”，`verified=false`，LEVEL 3 始终锁定。
- 顺序注意：首次分析若发生在质量探针之前，该次结果的 `level3` 会以
  “尚未完成重复分析稳定性实测”锁定；UI 的“质量验证”按钮通过后会自动重跑分析。
  这是有意行为：不把未实测的稳定性当作已通过。

## 5. 变形引擎的消费方式

`face_mesh::push_dense_points`：

| RegionKind | 控制点类型 | 权重 |
|---|---|---|
| FaceContour | `Face { ring: 3 }` | 1.0 |
| RightEye / LeftEye | `Eye { side, ring: 2 }` | 1.0 |
| Nose | `Nose { ring: 2 }` | 1.0 |
| OuterLips | `Mouth { ring: 2 }` | 1.0 |
| InnerLips / NoseCrest / MedianLine / Eyebrow | `Fixed` | — |

越界（|u| 或 |v| > 3）或非有限点直接跳过。未解锁时 `dense=None`，
控制点数量与第四轮完全一致（实测 235 个）；解锁 Apple Vision 后为 320 个。
