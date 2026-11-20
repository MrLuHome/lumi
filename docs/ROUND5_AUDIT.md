# ROUND 5 审查与架构 — BODY & BACKGROUND ENGINE 1.0

审查日期：2026-11-20
基线分支：`main`
基线提交：`ba183f7`（第四轮面部塑形引擎、faceId 持久化、MediaPipe 评估）
工作区状态：干净（`git status` 无改动）

---

## 1. 现有能力核对（以实际代码为准）

| 能力 | 位置 | 状态 |
| --- | --- | --- |
| 16-bit 图像引擎与渲染管线 | `src-tauri/src/image_engine/` | 存在，可复用 |
| 预览/导出共用管线 | `pipeline::render_with_report` | 存在 |
| 第三轮美肤 | `ai_engine/retouch`、`skin_segmentation` | 存在，不修改 |
| 第四轮面部塑形 | `src-tauri/src/face_reshape/` | 存在，不修改 |
| 人脸 UUID 持久化 / 匹配 | `face_reshape/identity.rs`、项目 v2 | 存在，复用模式 |
| LandmarkerProvider | `face_reshape/landmarker.rs` | 存在，第五轮沿用同一设计语言 |
| 手动变形笔触 | `face_reshape/operations/manual.rs` | 存在，但作用域绑定人脸；第五轮另建身体笔触 |
| 任务取消 | `image_engine/cancel.rs`、渲染任务合并 | 存在 |
| 项目保存 | `image_engine/project.rs`（v2） | 存在，需升级 v3（向后兼容） |
| 本地推理框架 | `tract-onnx 0.23` | 存在；当前只加载用户提供的 YuNet |
| 模型资产 | 仓库内无 ONNX/TFLite 权重 | 无新增权重可用 |

第四轮成果完整存在，本轮只增量接入，不重写、不删除。

---

## 2. 本轮缺口

1. 没有人物 Alpha 蒙版、没有发丝/边缘处理、没有手动蒙版工具。
2. 没有身体姿态 Provider、没有身材变形、没有身体局部推拉。
3. 没有背景虚化/景深/纯色/渐变/图片替换。
4. 管线没有人物分割、身体变形、背景合成阶段，也没有对应报告出口。
5. 项目 v3 字段缺失：人物对象、身体参数、蒙版笔触、背景参数、蒙版来源与版本。

---

## 3. 模型与授权审查结论（详细见 `ROUND5_MODEL_LICENSES.md`）

- 仓库没有任何已授权的人物分割/姿态权重；`tract-onnx` 只能运行 ONNX，不能直接运行 MediaPipe `.task` / PyTorch 检查点。
- BiRefNet：官方代码 MIT，模型卡标注 MIT，但**具体 ONNX 变体的权重来源、训练数据、再分发条件未逐一核实**；模型体积与 `tract-onnx` 算子兼容性未验证。
- SAM 2：代码与检查点 Apache-2.0，但依赖 PyTorch，不能直接塞进 Rust 主程序；需要独立推理进程与额外分发体积，本轮未验证通过。
- MediaPipe Pose Landmarker：运行时与模型为 Apache-2.0，但桌面分发路径未定（与第四轮 478 点评估相同的阻塞）。
- RVM：GPL-3.0，不作为商业发行默认方案。
- **结论：AI 分割与 AI 姿态本轮保持“接口 + 禁用 + 原因”，不下载、不打包、不宣称完成。**
- 人物蒙版以**用户手动笔触 + 本地非 AI Alpha 处理**交付，UI 明确标注“手动蒙版/非 AI 分割”，不把手动算法描述为模型推理。
- 身材调整以**蒙版轮廓 + 用户锚点 + 位移场上限**交付，不把姿态点或检测框包装成精确人体轮廓。

---

## 4. 架构设计

```text
src-tauri/src/ai_engine/
├── segmentation/
│   ├── mod.rs        SubjectMask、MaskProvider、能力状态、缓存键
│   ├── model.rs      模型元数据（id/version/license/SHA 待用户下载后校验）
│   ├── inference.rs  DisabledAiProvider（明确阻塞原因）
│   ├── manual.rs     手动笔触 → Alpha（添加/擦除/硬度/羽化/反转/扩展/收缩/平滑）
│   ├── alpha.rs      Alpha 统计、羽化、边缘带、二值化保护
│   ├── refinement.rs 边缘平滑、Alpha 羽化、前景去污染（三者分离）
│   ├── hair.rs       发丝保护（高频边缘带内的 Alpha 保留策略）
│   └── cache.rs      进程内蒙版缓存（键含源图哈希 + 笔触哈希 + 版本）
├── body/
│   ├── mod.rs        BodyEdit、SubjectBody、能力状态、入口
│   ├── pose.rs       PoseProvider trait + DisabledPoseProvider
│   ├── landmarks.rs  从蒙版提取每行左右轮廓与身体锚点
│   ├── deformation.rs 收腰/肩/胯/腿/手臂的平滑位移场
│   ├── mesh.rs       网格化位移场、反向采样、位移上限
│   └── constraints.rs 人脸/眼睛保护、背景保护、折叠与 Jacobian 检查
└── background/
    ├── mod.rs        入口与顺序
    ├── blur.rs       背景专用模糊（可分离盒式近似高斯）与景深模拟
    ├── depth.rs      由蒙版与纵向梯度模拟景深强度（明确非 3D 深度）
    ├── replace.rs    纯色/渐变/本地图片背景（cover/contain、缩放、偏移）
    ├── composite.rs  Alpha 合成、去污染、前景清晰保护
    └── params.rs     背景参数与夹紧

src-tauri/src/commands/
├── segmentation.rs  能力查询、蒙版预览、蒙版统计
├── body.rs          身体能力查询、姿态状态
└── background.rs    背景图片校验/元数据（不复制、不上传）
```

`image_engine/params.rs` 增加：

- `SubjectMaskParams`：enabled、invert、feather、expand、smooth、hairProtect、decontaminate、strokes（归一化坐标）。
- `BodyShape`：waist/shoulders/hips/thighs/calves/arms/legs，范围 -100..100。
- `BodyAdjust`：subjectId（稳定 UUID）、enabled、shape、anchors、strokes。
- `BackgroundParams`：mode（Original/Blur/SolidColor/Gradient/Image）、blur、depth、brightness、temperature、saturation、color、gradient、imagePath、imageScale/offset/blur/fit、decontaminate。
- `EditParams` 增加 `mask`、`body`、`background`，保持 serde default，旧参数不变。

---

## 5. 管线顺序（预览与导出共用）

```text
原图
  ↓
手动 Alpha 构建（笔触 + 羽化 + 发丝保护）
  ↓
身体变形（仅 Alpha 内、保护人脸框、位移上限、Jacobian 检查）
  ↓
用同一位移场反向采样 Alpha（蒙版与像素同步）
  ↓
背景处理（只改背景；虚化/景深/纯色/渐变/图片；去污染）
  ↓
Alpha 合成
  ↓
第四轮面部塑形（人脸区域受保护，坐标不变）
  ↓
第三轮美肤
  ↓
调色 / 清晰度 / 锐化 / 颗粒 / 暗角
  ↓
输出
```

- 没有任何第五轮参数时走原路径，像素与第三、四轮完全一致。
- 背景参数变化不触发行人脸检测；蒙版与身体参数不重复加载模型（本轮无模型）。
- 缓存键：源图 SHA-256 + 输出尺寸 + 蒙版笔触哈希 + 蒙版版本 + 身体参数 + 背景参数。

---

## 6. 项目数据 v3

- `PROJECT_VERSION = 3`；v1/v2 项目继续可打开，新字段默认关闭。
- `ProjectPhoto` 增加：
  - `subjects: Vec<ProjectSubject>`：`subjectId`（UUID，不复用人脸 ID）、蒙版笔触、蒙版来源/版本/置信度、身体参数、手动锚点与笔触。
  - `maskMeta: MaskMeta`：来源（`manual` / `ai-disabled`）、模型 ID/版本、覆盖度、哈希。
- 不把 Alpha 数组写入 JSON；笔触可完整重放。未来接入 AI 模型时，把压缩 Alpha 资源写到项目同目录并在 JSON 记录相对路径 + SHA-256。
- 手动修正永不因模型版本变化被覆盖。

---

## 7. 测试计划

- 单元：Alpha 添加/擦除/反转/羽化/扩展/平滑；背景纯色/渐变/图片/模糊；身体各参数；位移上限；人脸保护；背景保护；Jacobian 折叠检测。
- 端到端：无参数恒等；背景替换只改背景；身体收腰改人物但背景不变；蒙版同步；全分辨率导出与预览参数一致。
- 项目：v3 往返、v2 迁移、蒙版笔触与背景路径恢复、模型缺失仍可打开。
- 前端：参数夹紧、默认中性、复合更新、蒙版工具状态。
- 构建：`cargo test`、`cargo clippy`、`pnpm test`、`pnpm typecheck`、`pnpm build`，并做桌面启动验证（可行时）。

---

## 8. 风险与降级

| 风险 | 降级 |
| --- | --- |
| 无合规 AI 分割权重 | 手动蒙版完整可用，AI Provider 禁用并显示原因 |
| 无 AI 姿态 | 身材调整基于蒙版轮廓与用户锚点，UI 标注“轮廓级近似” |
| 手动蒙版质量依赖用户 | 提供蒙版预览、反转、收缩/扩张、羽化、发丝保护、撤销重做 |
| 全分辨率蒙版计算耗时 | 预览降分辨率、笔触包围盒、缓存；导出全分辨率 |
| 背景替换边缘残留 | 前景去污染 + Alpha 羽化 + 边缘带处理 |

本轮不进入第六轮。
