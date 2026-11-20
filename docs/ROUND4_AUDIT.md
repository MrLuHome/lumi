# ROUND 4 审查与方案 — FACE RESHAPE ENGINE 4.0

基线：第三轮提交 `91a8120`。本文件只记录审查结论、关键点模型评估、阻塞项与降级方案、实施计划；代码完成后另出 `docs/ROUND4_REPORT.md`。

## 1. 审查结论（以实际代码为准）

| 能力 | 实际实现 | 结论 |
| --- | --- | --- |
| 人脸检测 | YuNet 2023mar，`ai/face.rs` + `FaceEngine`，1800px 预览推理，CPU（tract） | 可用 |
| 关键点 | 仅 5 点：右眼、左眼、鼻尖、左右嘴角（`FaceFrame.landmarks`） | **不足以支撑五官网格** |
| 人脸 ID | `FaceTracker` IoU≥0.25 贪心匹配；`FaceRepository` 按路径缓存 `faces + version` | 会话内稳定；跨项目重开存在错配风险（见 P1-1） |
| 人脸几何 | `FaceGeometry`：双眼中点原点、眼距为单位、v 轴向下；`to_local/from_local` | 可复用为网格局部坐标 |
| 皮肤蒙版 | 几何椭圆 + YCbCr 似然 + 五官挖除 + 羽化；`apply_ownership` 处理多人重叠 | 可复用于变形边界保护 |
| 渲染管线 | `pipeline::render`：美肤 → 调色 → 清晰度/锐化/颗粒/暗角；预览与导出共用 | 需要插入变形阶段 |
| 非破坏 | 所有编辑只存 `EditParams`，渲染从 `cached.full` 复制；导出拒绝覆盖源文件 | 可复用 |
| 撤销/重做 | 前端 zustand 每图独立历史，600ms 合并窗口，快照为完整参数 | 可复用 |
| 多人脸参数 | `EditParams.faces: Vec<FaceBeauty>`，按 `faceId` 独立 | 可复用结构 |
| 高清导出 | 全尺寸 `pipeline::render`，参数与预览一致 | 可复用 |
| 缓存 | 人脸结果按路径缓存；渲染缓存键含人脸签名 | 可复用 |

### 渲染顺序

```
原图 → 美肤（蒙版/瑕疵/磨皮/肤色/眼周）→ 调色 → 清晰度 → 锐化 → 颗粒 → 暗角
```

变形必须插在美肤之后、调色之前：这样皮肤蒙版仍按原始关键点计算，不会因为先变形导致蒙版错位；输出像素仍被真实重采样。

## 2. 缺陷与阻塞

### P0-1（阻塞本轮的模型问题）：没有合规的 68/106 点关键点

YuNet 只输出 5 点。用 5 点拟合眼睑、鼻翼、唇轮廓属于“硬凑高质量网格”，按要求禁止。本轮评估了可用候选：

| 候选 | 点数 | 代码许可 | 权重许可风险 | 工程风险 | 结论 |
| --- | --- | --- | --- | --- | --- |
| InsightFace 2d106det | 106 | MIT | 模型卡明确仅限非商业研究 | ONNX，tract 可跑 | **不可用** |
| dlib shape_predictor_68 | 68 | Boost | 权重训练自 iBUG 300-W，商用需单独授权 | — | **不可用** |
| PIPNet / FAN / 3DDFA_V2 | 68 / 98 | MIT 等 | 训练集 300W/WFLW 限研究；3DDFA 依赖 BFM 非商用 | 需转换 | **不可用** |
| MediaPipe Face Mesh / Face Landmarker | 468 | Apache-2.0（仓库） | 具体 `.task` 权重与训练数据条款需书面确认；TFLite→tract 兼容未验证 | 预处理/后处理链路长 | 待验证，不打包 |
| Qualcomm AI Hub Facial-Landmark-Detection (`facemap_3dmm`) | 3DMM | BSD-3-Clause（模型卡） | 训练数据为 Qualcomm 私有；3DMM 基底（可能含 BFM）授权未明 | TFLite，需 tract-tflite；输出解码依赖基底 | 待验证，不打包 |

**结论：本轮不打包任何稠密关键点权重。** 不拿未审查权重凑数，也不用 5 点伪造 68 点网格。

### 降级方案（本轮实际执行）

1. 新建统一变形引擎与关键点能力层 `LandmarkSet { source: "yunet-5" | "dense-68" | "dense-106", confidence, points }`，把“能力不足”变成显式状态，而不是假装关键点齐全。
2. 稠密网格变形（眼睑、唇轮廓、鼻翼、下巴尖）在 `yunet-5` 下**不启用**，接口保留，UI 显示“需要已授权的 68/106 点模型”。
3. 可可靠实现的真实像素变形：
   - **轮廓级脸型塑形**：以人脸框椭圆为轮廓控制环（明确标注为框级近似，不声称关键点网格），做局部 MLS/网格位移，向内平滑衰减，保护五官与背景；
   - **局部径向微调**：以可靠锚点（双眼中心、鼻尖、左右嘴角）做小范围、低上限的径向缩放/位移，用于保守的双眼间距、嘴角、鼻头大小等；
   - **手动局部变形**：推动/拉动/放大/缩小/保护/恢复，基于图像坐标的自由变形，不依赖任何关键点；
   - **网格显示**：显示实际使用的控制网格（框级环 + 锚点），让用户看到这不是 68 点网格。
4. 上唇/下唇厚度、眼宽/眼高、眼角、鼻翼宽度、下巴尖度等需要稠密轮廓的参数在 UI 中禁用并说明原因，不伪造效果。

### P1-1：项目重开后人脸 ID 可能错配

项目文档只保存参数，不保存人脸检测结果（`src/state/project.ts` 仅序列化 `params`）。同一张照片重新检测时若漏检一张脸，后续 `faceId` 会平移，导致 A 的塑形参数落到 B 上。第三轮的 IoU 跟踪只在同一应用会话内有效。

**本轮修复**：项目文档保存每张图归一化 `faces`，加载项目时调用后端 `import_faces` 把人脸写回 `FaceRepository`，保证 ID 与参数一致；重新检测时仍走 IoU 跟踪。

### P1-2：管线没有变形阶段，也没有质量报告出口

`pipeline::render_with_progress` 只返回图片，`FaceRetouchReport` 也被丢弃。本轮新增 `ReshapeReport`（每脸位移上限、翻转数、拉伸比、是否降强度），并在预览结果中回传摘要，前端显示“质量保护已介入”。

### P2

- 蒙版未单独缓存；变形网格也需要缓存，否则滑块每次重算三角剖分（本轮用“关键点版本 + 参数版本”做网格缓存）。
- 大图不分块，全尺寸内存处理；变形只处理 ROI，避免整图额外缓冲。
- 无 GPU；CPU 回退始终可用。
- 自动祛痘跳过原因未回传 UI（第三轮遗留，本轮不扩大范围）。

## 3. 新增模块设计（`src-tauri/src/face_reshape/`）

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 统一入口 `apply_face_reshape(image, faces, params, cancel) -> ReshapeReport` |
| `landmark.rs` | `LandmarkSet`、能力检测、置信度、标准化坐标、稳定性校验 |
| `face_mesh.rs` | 由关键点能力生成控制点集：框级轮廓环 + 可靠锚点；版本号 |
| `mesh.rs` | 控制网格结构、包围盒、邻接、网格缓存键 |
| `triangulation.rs` | Bowyer–Watson Delaunay；拒绝退化三角形；单元测试 |
| `deformation.rs` | 控制点位移 → MLS 仿射变形场；位移上限与能量约束 |
| `warp.rs` | 逆映射 + 双线性/超采样重采样，只处理 ROI，保留 alpha |
| `boundary.rs` | 面部边界、五官保护区、背景保护、发际线约束 |
| `blending.rs` | 变形区与原图的羽化混合，避免接缝 |
| `mask.rs` | 手动保护蒙版、局部恢复、笔触栅格化 |
| `quality.rs` | 三角翻转、折叠、位移上限、纹理拉伸、边界连续性检查；超限自动降强度 |
| `presets.rs` | 自然/轻度/强力三档，默认全部 0 |
| `operations/{face,eyes,nose,mouth,chin,manual}.rs` | 各部位参数 → 控制点位移 |

统一接口（伪签名）：

```rust
pub fn reshape_face(
    image: &mut Image16,
    face: &FaceFrame,
    params: &FaceReshape,
    cancel: &CancelToken,
) -> AppResult<FaceReshapeReport>;
```

输出：变形后像素、使用的能力等级、网格统计、质量报告（翻转/折叠/最大位移/最大拉伸）。

## 4. 参数与数据

```rust
pub struct FaceReshape {
    pub face_id: u32,
    pub enabled: bool,
    pub face: FaceShape,     // slim, width, jaw, cheekbone, length, balance_l/r
    pub eyes: EyeShape,      // size_l/r, width, height, corner, distance, balance
    pub nose: NoseShape,     // bridge, alar, tip, height, balance
    pub mouth: MouthShape,   // width, upper, lower, corner, symmetry, size
    pub chin: ChinShape,     // length, width, point, jawline
    pub manual: ManualWarp,  // strokes: push/pull/zoom/protect/restore + radius/strength/feather
}
```

所有数值 `-100..100`（尺寸类 0..100 或 -100..100），Rust 侧再夹紧到算法安全上限；单脸、单部位重置；默认全部 0。

## 5. 实施顺序

1. 参数结构 + 深夹紧 + 项目持久化（含人脸）。
2. `landmark/face_mesh/mesh/triangulation/deformation/warp/boundary/blending/quality` 引擎与单元测试。
3. `operations` 各部位（先做可可靠实现的轮廓与局部径向，稠密参数走能力门控）。
4. 管线接入（美肤后、调色前）、网格/蒙版缓存、取消检查、质量报告回传。
5. 前端面板与画布手动工具（图像坐标、撤销笔触、网格显示）。
6. 自动化测试、构建、实际启动、报告与提交。

## 6. 验收口径

- 变形必须真实改变像素（测试断言像素差异与几何位移），不是 CSS/关键点标记；
- 背景在保护区域外的变化 < 1/255，人脸边界无接缝；
- 三角网格无翻转（有向面积同号），位移上限内不折叠；
- 同一参数下预览与全尺寸导出结果在缩放后一致（同一算法路径、归一化坐标）；
- 多人脸参数独立，重叠不污染；
- 撤销/重做、单项/单脸/全部重置、项目保存恢复可用；
- 稠密关键点相关参数在 `yunet-5` 下明确禁用并提示，不产生假效果。
