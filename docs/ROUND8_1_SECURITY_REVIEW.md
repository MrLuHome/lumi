# ROUND 8.1 安全审查：用户 ONNX 隔离与精度证据

> 基线：`479416f8011000ebe5ea365a01f384633d190cf6`
> 范围：`face_intelligence` 用户模型加载/推理、质量证据与 UI 文案。
> 结论先行：**用户 ONNX 的真实推理已移入独立 Worker 子进程；但这不是完整沙盒，
> 仍存在明确剩余风险，Provider 保持实验性，不得宣传为可安全加载任意来源模型。**

## 1. 修改前的风险

| 风险 | 修改前 | 影响 |
| --- | --- | --- |
| 真实推理在主进程 | `OnnxLandmarkProvider` 持有 `Arc<LandmarkModel>`，tract 直接在本进程加载/运行 | 恶意模型可耗尽内存、触发 panic/abort，拖垮整个应用 |
| 冒烟子进程被高估 | 导入冒烟在子进程执行，但运行时推理仍在主进程 | 「已隔离」的说法只覆盖导入，不覆盖实际使用 |
| 无输入/输出上限 | 模型可声明任意输入尺寸、输出任意长度 | 内存放大、长时间占用 CPU |
| 无时间/内存/CPU 限制 | 子进程只有外层超时（仅冒烟） | 推理可无限运行 |

## 2. 修改后的实际隔离范围

### 2.1 主进程做什么

- 读取并校验 `manifest.json` / SHA256 / 安装状态（不含模型计算图）；
- 按 manifest 做裁剪、缩放、归一化，生成 NCHW `f32` 张量；
- 把张量写入临时文件 `<tmp>/lumi-face-input-<pid>-<seq>/input.f32`；
- 启动应用自身的 Worker 子进程，传入 `--face-model-infer <request.json> <response.json>`；
- 读取并校验响应：`ok`、68 点、有限性、坐标范围；
- 通过 `coordinate::crop_to_image` 映射回原图归一化坐标。

**主进程不调用 tract、不解析 ONNX 计算图、不加载用户模型权重。**

### 2.2 Worker 子进程做什么

入口：`src-tauri/src/main.rs` 在 Tauri 启动前分流 `--face-model-infer`，
调用 `model_hub::probe::run_cli` → `infer_sync`：

1. 校验 `width/height ∈ [32, 1024]`；
2. 校验张量元素数 `3*w*h ≤ 3,145,728`；
3. 校验 `expected_points == 68`；
4. 读取输入文件（上限 64 MB），校验文件长度 = 元素数 × 4；
5. 校验每个 `f32` 有限；
6. 从模型目录读取 `manifest.json`；
7. `inference::run_plan`：tract 解析/优化/编译/运行，`catch_unwind` 包裹推理；
8. 校验输出形状、68 点、NaN/Inf、坐标范围；
9. 结果写回 JSON；任何失败都以 `ok=false + error` 返回。

### 2.3 硬限制

| 限制 | 值 | 实现 |
| --- | --- | --- |
| 输入边长 | 32..1024 | `infer_sync` 前置校验 |
| 张量元素数 | ≤ 3,145,728（3×1024×1024） | `MAX_INFER_ELEMENTS` |
| 输出点数 | 必须 68 | `expected_points` + 响应校验 |
| 输入文件 | ≤ 64 MB | `read_limited` |
| 运行时间 | 10 s | `DEFAULT_INFER_TIMEOUT_MS` + 轮询 kill |
| 地址空间 | 1.5 GB | `RLIMIT_AS`（`pre_exec`，Unix） |
| CPU 时间 | 10 s | `RLIMIT_CPU`（`pre_exec`，Unix） |
| 崩溃/超时 | 主进程只收到错误 | `try_wait` + `kill` + 响应缺失判定 |

### 2.4 故障行为

- Worker panic：`run_plan` 内 `catch_unwind` 转为错误；若进程仍退出，主进程判定「未产生结果」。
- Worker 崩溃：响应文件缺失 → `FaceError::Inference`，主应用不退出。
- Worker 超时：主进程 `kill` + `wait`，返回「模型推理超时」。
- 输出异常：点数不符、NaN/Inf、坐标越界 → 整次分析失败，不降级、不猜测。
- 没有 Worker 可执行文件：`FaceError::Unsupported`，**不做主进程回退**。

## 3. 剩余风险（必须如实告知）

1. **不是操作系统级沙盒**：没有 `sandbox-exec`、App Sandbox、seccomp 或单独低权限用户；
   Worker 仍以当前用户身份运行，可读写用户有权访问的文件。
2. **RLIMIT 不是强保证**：macOS 上 `RLIMIT_AS` 对部分映射不一定生效；
   `RLIMIT_CPU` 只限制 CPU 时间，不限制墙钟阻塞。
3. **临时文件**：输入张量与请求/响应 JSON 落在系统临时目录；
   正常路径会删除，进程被强杀时可能残留。当前未加密、未做权限收紧。
4. **模型文件仍由 Worker 直接读取**：Worker 与主进程同二进制，
   攻击面从「主进程」缩小到「子进程」，但没有消除。
5. **manifest 由主进程解析**：JSON 解析风险仍在主进程，但已限制文件大小与字段。
6. **模型本身未审计**：结构/算子/冒烟通过不等于安全；Provider 仍标记为实验性、许可未核实。
7. **预处理在主进程**：大图裁剪/缩放会占用主进程内存，受 `max_edge` 与输入边长限制，但非零成本。

## 4. 产品与文案约束

- Provider 卡片必须保留「实验性：推理在隔离 Worker 中执行，模型仍来自用户，未审计」。
- 不得出现「可安全加载任意来源模型」「沙盒隔离」「已审计」等表述。
- 导入失败必须给出具体原因（结构、算子、冒烟、SHA256、路径），不得静默降级。
- 许可证状态保持 `UserUnverified`，不得进入官方商用模型列表。

## 5. 精度证据边界

| 证据 | 当前状态 | 允许的表述 |
| --- | --- | --- |
| 稳定性 | 重复分析一致性可测（`measure_stability`） | 「重复运行一致」 |
| 结构验证 | 区域点数/顺序/有限性/宽度可测（`evaluate_checks`） | 「结构检查通过」 |
| 定位精度 | **未测**（无人工标注集、无 NME） | 「尚未验证」 |

明确禁止：

- 把重复运行一致解释为「定位准确」；
- 把检测级 `confidence` 当作逐点 `confidence`；
- 用「精确五官」暗示精度已认证；
- 在无 NME 的情况下写「精度已验证 / 行业领先 / 像素级准确」。

## 6. 审查结论

- 用户 ONNX 的真实推理已不在主进程执行，满足「独立 Worker + 限额 + 崩溃不影响主应用」的最低要求；
- 隔离是**进程级**而非**系统级沙盒**，剩余风险已在产品文案与文档中标注；
- 用户 ONNX 继续作为实验性 Provider，默认不宣传、不随包分发；
- 定位精度仍为未验证，等待 ROUND 9 的 NME 评测。
