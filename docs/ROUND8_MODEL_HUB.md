# ROUND 8 — LUMI MODEL HUB

- 代码：`src-tauri/src/face_intelligence/model_hub/{mod,manifest,security,validator,probe,inference,storage}.rs`
- 命令：`pick_face_model_file / import_face_model / list_face_models / revalidate_face_model / remove_face_model`
- 定位：用户自备**单文件 ONNX 人脸关键点模型**的安全导入路径。不是任意模型运行器。

## 1. 目录与文件

```text
<app_data>/face-models/
└── <model-id>/
    ├── manifest.json      # 输入/输出契约、许可证声明、源路径、SHA256
    ├── model.onnx         # 复制后的模型本体（只读）
    └── validation.json    # 最近一次校验/冒烟结果
```

- `model-id` 由导入时生成或校验：`[A-Za-z0-9._-]{1,64}`，不能以 `.` 开头，路径必须落在根目录下。
- 原始文件不会被修改或移动；manifest 记录 `sourcePath` 与 `sha256` 供追溯。
- 重新验证会重新计算 SHA256；文件被替换时报告哈希不一致，不沿用旧结论。

## 2. Manifest 契约

| 字段 | 约束 |
|---|---|
| `schemaVersion` | 必须为 1 |
| `task` | 仅 `faceLandmark` |
| `topologyId` | 仅 `face-68`（当前唯一有逐点语义定义的稠密拓扑） |
| `input.layout` | 仅 `nchw` |
| `input.width/height` | 32..1024，且必须静态 |
| `input.normalization` | `zeroToOne` / `minusOneToOne` / `imagenet` / `raw255` |
| `input.cropMargin` | 0..0.5 |
| `output.layout` | `n2 / n3 / c2n / c3n`，由静态形状推断 |
| `output.coordinate` | `normalizedCrop` 或 `pixelCrop`，由 Provider 映射回原图 |
| `license` | 记录用户声明；LUMI 始终标记 `UserUnverified` |

不支持 `face-126`、parsing、matting、任意图模型或带后处理脚本的模型。
这类模型会在导入阶段被拒绝并给出原因，而不是“先跑起来再说”。

## 3. 导入校验（任务书 15 项对照）

| # | 检查项 | 实现 | 状态 |
|---|---|---|---|
| 1 | 文件存在性 | `symlink_metadata` + 普通文件检查 | 已完成 |
| 2 | 文件尺寸 | 0 字节拒绝；>512 MB 拒绝 | 已完成 |
| 3 | SHA256 | 流式哈希，写入 manifest 与 validation | 已完成 |
| 4 | ONNX 结构 | `proto_model_for_path` 解析，失败给出“可能损坏或不是 ONNX” | 已完成 |
| 5 | 模型算子 | `model_for_path` + `into_optimized`/`into_runnable` 编译检查 | 已完成 |
| 6 | 输入 Tensor | 单输入、`[1,3,H,W]`、float32、静态 | 已完成 |
| 7 | 输出 Tensor | 单输出、3 维、单批次、N∈5..2000、2/3 通道 | 已完成 |
| 8 | 静态形状 | `TypedFact.shape.as_concrete()` 必须可用 | 已完成 |
| 9 | 动态维度 | 动态维度直接拒绝并说明 | 已完成 |
| 10 | 能力声明 | 仅 `faceLandmark` + `face-68` | 已完成 |
| 11 | 许可证声明 | 记录但不采信；固定 `UserUnverified` | 已完成 |
| 12 | 小尺寸测试推理 | 全零输入冒烟，`catch_unwind` 隔离 panic | 已完成 |
| 13 | 结果数值检查 | 输出元素 >0 且有限值比例 >99% | 已完成 |
| 14 | 内存消耗检查 | 仅有文件体积上限与子进程隔离；**没有**逐模型峰值内存测量 | 部分 |
| 15 | 超时测试 | 应用自身子进程 `--face-model-probe`，20 s 超时 kill | 已完成 |

## 4. 隔离探针

- 校验与冒烟推理不在主进程执行，而在 `LUMI RETOUCH` 自身以
  `--face-model-probe <request.json> <response.json>` 启动的子进程里执行。
- 子进程只读取 JSON 请求（模型路径、输入尺寸等），不执行模型内任何脚本，
  不把路径拼进 shell。
- 超时 20 s 直接 kill；子进程崩溃/panic 只会得到失败结论，主进程继续运行。
- `probe_sync` 供测试使用同一条校验链路（不启动子进程）。

## 5. 失败行为

- 任何一步失败：模型保持“未启用”，UI 显示具体原因（哪一项、期望是什么、实际是什么）。
- 不静默回退到 `yunet-5` 继续“看起来成功”；也不缓存失败模型的“可用”状态。
- 删除模型只删除 `<app_data>/face-models/<id>`，不触碰用户原始文件。

## 6. 真实样本验证状态

| 项 | 状态 |
|---|---|
| 拒绝 ZIP/NumPy/ELF/Mach-O/PDF/Git LFS 伪装 | 已验证（单元测试） |
| 外部数据（`external_data`）拒绝 | 已验证（单元测试） |
| 路径越界 / 非法 ID | 已验证（单元测试） |
| SHA256 稳定性与被篡改检测 | 已验证（单元测试） |
| 真实第三方 68 点 ONNX 端到端导入 + 推理 | **未验证**：仓库不附带、也未下载任何第三方权重，无合法样本可用 |
| 动态形状模型拒绝 | 单元测试覆盖结构判断；真实动态模型样本未验证 |
| 超时 kill | 代码路径存在；未构造慢模型实测 | 

结论：安全边界已按代码路径实现并有单元测试；**端到端真实模型验证仍缺样本**，
不能宣称“任意 68 点 ONNX 已验证可用”。

## 7. UI

`模型库` 工作区提供：文件选择、模型名称、拓扑（固定 face-68）、归一化方式、
输出布局、输出坐标、裁剪边距、许可证名称（用户声明）。导入后展示
SHA256、文件体积、输入/输出形状、算子数、许可证状态与“使用/重新验证/移除”操作。
