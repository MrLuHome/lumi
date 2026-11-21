# ROUND 7 — GPU 基准与回退结论

- 机器：macOS / Apple M4 / 16 GB
- 后端：Metal（wgpu 30.0.2 探测到 `metal`，设备 `Apple M4`）
- 结论：**GPU 内核可用，但在 release 打包应用里端到端比 CPU 慢（0.34×–0.38×），
  Auto 策略按门槛保持 CPU；GPU 不默认启用。**
- 证据：应用内“运行基准”截图 `docs/assets/round7/05-gpu-benchmark-panel.png`；
  dev 测试二进制输出 `docs/assets/round7/gpu-benchmark.log`。

## 1. release 打包应用实测（权威数据）

在正式 `.app` 的性能面板点击“运行基准”，本机 Metal 结果：

| 尺寸 | MP | CPU | 上传 | GPU 运算 | GPU 总 | 倍率 |
|---|---:|---:|---:|---:|---:|---:|
| 1600×1200 | 1.9 | 25.1 ms | 2.1 ms | 63.7 ms | 65.7 ms | **0.38×** |
| 3000×4000 | 12.0 | 106.9 ms | 29.8 ms | 252.8 ms | 282.6 ms | **0.38×** |
| 6000×4000 | 24.0 | 203.4 ms | 23.4 ms | 568.6 ms | 592.0 ms | **0.34×** |

- Auto 判定档位为 3000×4000：**0.38× < 1.25×**，因此：
  - 当前后端显示“GPU 可用（未启用）”；
  - 说明显示“未达 1.25× 门槛（实测 0.38×），使用 CPU”；
  - 状态栏显示“CPU 计算”。
- 这是 release 优化后的真实结果：CPU 路径经过 `opt-level = "s"` + LTO 后远快于 dev，
  而 GPU 运算受 WGSL 分发/读回与内核效率限制，没有收益。

## 2. dev 测试二进制基准（仅用于回归对照）

命令：`gpu_benchmark_report --ignored --nocapture`（`cargo test --lib` 的 dev 构建）

| 尺寸 | CPU | 上传 | GPU 运算 | GPU 总 | 倍率 |
|---|---:|---:|---:|---:|---:|
| 1600×1200 | 69.432 ms | 0.778 ms | 27.147 ms | 27.925 ms | 2.49× |
| 3000×4000 | 427.811 ms | 7.260 ms | 287.349 ms | 294.609 ms | 1.45× |
| 6000×4000 | 910.281 ms | 29.810 ms | 695.722 ms | 725.533 ms | 1.25× |

- dev CPU 未优化，倍率虚高；**不能作为是否启用的依据**。
- 应用内基准使用当前 release 构建重新测量，因此最终结论以 §1 为准。
- 本机 GPU 运算耗时在 dev 与 release 下接近（例如 12MP 287ms vs 253ms），
  说明瓶颈在 GPU 内核/数据传输，而不是 CPU 测量环境。

## 3. 一致性

| 测试 | 结果 |
|---|---|
| `gpu_exposure_matches_cpu_when_available` | 真实 Metal 上通过（通道差 < 0.01） |
| `gpu_curve_matches_cpu_when_available` | 真实 Metal 上通过；LUT 三通道均按 `values[index_r/g/b]` 取值 |
| `ops` 单元测试 | sRGB 往返、曝光增益、恒等曲线、饱和度保持灰色 |
| `injected_gpu_failure_falls_back_to_cpu` | 注入初始化失败后：`used_gpu == false`，像素与 CPU 结果逐位相等，`status.available == false`，原因包含注入信息 |

一致性门槛（spec）：平均 ΔE00 ≤0.5、P95 ≤1.0。本轮使用逐通道绝对差 <0.01
（16-bit 归一化后约 0.65 LSB），未单独输出 ΔE00/P95；这一项按未测试标注。

## 4. 回退策略（已实现并实测）

| 场景 | 行为 |
|---|---|
| 无可用适配器 / 初始化失败 | `GpuStatus.available=false`，`reason` 记录原因，执行走 CPU |
| Shader 编译 / 绑定 / 读回失败 | `exposure_with_fallback` / `curve_with_fallback` 捕获后返回 CPU 结果，`used_gpu=false` |
| Auto 未完成基准 | 保持 CPU，原因“尚未完成基准验证” |
| Auto 基准 <1.25× | 保持 CPU，原因包含实测倍率（本机即为该情况） |
| 用户选 GPU 但设备不可用 | 状态显示 CPU 与失败原因；数据路径仍可完成 |
| 用户选 CPU | 强制 CPU，不初始化 GPU |

回退不改变输出：`injected_gpu_failure_falls_back_to_cpu` 断言 CPU 与回退结果逐位一致。

## 5. 主编辑管线接入状态（未完成，不得标记为完成）

- 本轮 GPU 只实现并验证两个独立内核：**曝光/色彩矩阵**与**曲线 LUT/饱和度**。
- `pipeline::render` 的主编辑管线**仍为纯 CPU**。原因：把 GPU 插入现有算法顺序需要改变
  第六轮已验证的输出语义（尤其是美肤/塑形/背景与 16-bit 舍入顺序），风险高于本轮可验证范围。
- 因此：
  - ✅ GPU 探测、状态、基准、CPU 回退、两个内核一致性：已完成并实测；
  - ❌ “GPU 替换主编辑管线中的曝光与曲线”：**未接入**，不计为完成；
  - UI 只声明“GPU 可用/未启用”，不声称主预览已由 GPU 加速。
- 当前 Auto 在本机 release 下保持 CPU；即使用户强制选 GPU，主编辑管线也不会调用 GPU 内核。

## 6. 结论

1. Apple M4 Metal 上 `wgpu 30.0.2` 可运行，两个内核结果与 CPU 一致（<1 LSB）。
2. release 端到端 GPU 为 0.34×–0.38×，未达 1.25× 门槛；Auto 正确保持 CPU，GPU 不默认启用。
3. 任何失败均回退 CPU 且逐位一致。
4. 主编辑管线尚未接入 GPU，按未完成标注；不将“GPU 加速主编辑”写入已完成能力。
5. Windows / D3D12 / Vulkan 未实机验证。
