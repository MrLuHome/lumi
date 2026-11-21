# ROUND 9 阶段 4-5：ONNX 正常推理链路与 Worker 安全验证

> 结论先行：**协议链路已在真实 Worker 子进程中跑通，覆盖正常推理与 9 类异常。**
> 本轮没有找到可合法分发、且与本项目 68 点协议一致的第三方人脸关键点模型，
> 因此使用自建夹具模型，能力等级为 **ProtocolTestOnly**，**不是真实人脸模型推理**。
> 真实模型集成保持未完成，用户模型不会因 RuntimeTested 自动解锁 LEVEL 3。

## 1. 模型登记

| 项 | 值 |
| --- | --- |
| 模型名 | LUMI Protocol 68（ProtocolTestOnly） |
| 用途 | 验证导入/加载/预处理/推理/输出校验/坐标转换协议，不作人脸定位 |
| 来源 | 本项目自建，`scripts/make_protocol_models.py` 可重复生成 |
| 许可 | CC0-1.0（公共领域贡献） |
| 输入 | `float32[1,3,64,64]`，NCHW，`zeroToOne` 归一化，cropMargin 0.1 |
| 输出 | `float32[1,68,3]`（N3：x, y, confidence），`normalizedCrop` 坐标 |
| 语义拓扑 | `face-68`（与现有 Provider 协议一致），但数值为常量，不具备语义正确性 |
| SHA256 | `a0b1c9c15fbc1e1af48fb1347a680a1b464b55016be9ff82b388c0bf46808e7b` |
| 异常夹具 | `protocol68_nan.onnx` `664f2673…`、`protocol68_out_of_range.onnx` `afa130a9…`、`protocol68_bad_op.onnx` `ee5594cf…` |

## 2. 链路与真实日志

执行命令：

```bash
cd src-tauri
cargo test --test onnx_worker -- --nocapture
```

真实输出（2026-11-21，Apple M4 / macOS 27.2.1）：

```
running 7 tests
test worker_timeout_and_start_failure_are_contained ... ok
worker 推理成功：68 点，耗时 14 ms
test protocol_model_runs_real_inference_in_worker ... ok
test worker_rejects_wrong_input_length_and_nan_input ... ok
test worker_reports_corrupt_and_unsupported_models_without_crashing ... ok
test worker_rejects_nan_and_out_of_range_outputs ... ok
probe: 输入 64x64，输出 204 元素，有限值比例 1，冒烟 1 ms
test probe_chain_validates_structure_and_smoke_in_worker ... ok
test consecutive_inference_model_switch_and_multi_face_inputs ... ok

test result: ok. 7 passed; 0 failed
```

逐阶段对应：

| 阶段 | 验证方式 | 结果 |
| --- | --- | --- |
| 模型文件读取 + SHA256 | `probe::probe_isolated` 返回 `sha256` 与 `file_size` | 通过 |
| Manifest 验证 | `manifest` 解析 + 输入/输出契约完整性 | 通过（夹具 manifest 完整） |
| 结构与算子检查 | `validate_onnx`（tract 优化期捕获未知算子） | 通过；未知算子返回 `Unimplemented(LumiUnknownOp)` |
| Worker 启动 | `infer_isolated` / `probe_isolated` 启动应用自身 `--face-model-infer` | 通过（独立子进程） |
| 模型加载 | Worker 内 `tract_onnx` 加载 | 通过 |
| 输入预处理 | `inference::prepare_input`（crop + NCHW + 归一化） | 由 Worker 输入长度/数值断言覆盖 |
| 推理 | tract runnable | 通过，单次 13–14 ms |
| 输出长度与数值 | 68 点、有限值、confidence≈0.9 | 通过 |
| 语义拓扑映射 | `topology::group_face68`（Provider 路径） | 既有单测覆盖；本轮未构造图像做端到端像素路径 |
| 坐标转换 | `coordinate::crop_to_image` | 同上 |
| Provider 输出 → 前端 | Provider 契约不变 | 阶段 8 打包应用验收 |

## 3. Worker 安全边界

| 限制 | 实际值 | 位置 |
| --- | --- | --- |
| 输入边长 | 32 – 1024 | `probe::MAX_INFER_EDGE` |
| 输入张量元素数 | ≤ 3,145,728（3×1024×1024） | `probe::MAX_INFER_ELEMENTS` |
| 输出点数 | 必须为 68 | `infer_face` / Worker |
| 推理超时 | 默认 10,000 ms | `DEFAULT_INFER_TIMEOUT_MS` |
| 模型文件大小 | ≤ 512 MiB | `security::MAX_MODEL_BYTES` |
| 地址空间 | RLIMIT_AS 1.5 GiB（Unix） | `apply_worker_limits` |
| CPU 时间 | RLIMIT_CPU 10 s（Unix） | `apply_worker_limits` |
| 文件访问 | 仅 manifest 所在目录 + 请求指定输入/输出文件 | Worker 请求协议 |
| IPC 数据 | 仅 JSON 请求/响应，点数为固定 68 | `infer_isolated` |
| 退出码/错误分类 | 超时、启动失败、进程崩溃、非法输出均转为可读错误 | `InferResponse.error` |

平台差异（如实记录）：
- macOS/Linux 使用 `setrlimit`；**Windows 无等价 RLIMIT**，仅依赖超时与进程退出，不得宣传为完整沙盒。
- Worker 以当前用户权限运行，未做 seccomp/App Sandbox 级隔离。
- 模型路径由用户选择，Worker 可读取该目录，限制范围不等于系统级只读。

## 4. 异常矩阵结果

| 异常 | 用例 | 结果 |
| --- | --- | --- |
| 文件损坏 | 写入非 ONNX 字节 | Worker 返回失败，主进程存活 |
| 不支持算子 | `LumiUnknownOp` | `模型优化失败：…Unimplemented` |
| 输入维度错误 | 12 字节输入 | `输入张量长度不符` |
| 输入 NaN | 第 7 个元素 NaN | 拒绝，错误含 `NaN` |
| 输出 NaN | 夹具第 3 点 confidence=NaN | 拒绝，错误含 `NaN` |
| 越界坐标 | x=5.0 | 拒绝，错误含 `越界` |
| Worker 启动失败 | 不存在的可执行文件 | `无法启动`，不 panic |
| 超时 | 1 ns 截止时间 | `超时`，子进程被终止 |
| 连续推理 | 同模型 2 次不同输入 | 全部成功 |
| 模型切换 | 正常 → NaN → 正常 | 异常被拒绝，切回后恢复 |
| 多人脸输入 | 两张不同张量 | 均成功（协议层串行） |
| 重复加载 | 每次推理重新加载模型 | 成功，耗时 13–14 ms |

## 5. 能力等级

| 等级 | 状态 | 说明 |
| --- | --- | --- |
| Imported | 是（夹具） | 文件可被探针读取 |
| StructurallyValidated | 是 | 输入/输出契约通过 |
| ProtocolTested | 是 | 68 点协议、坐标、异常处理 |
| RuntimeTested | 是（仅夹具） | 真实 Worker 推理执行 |
| AccuracyUnverified | 是 | 常量输出，无语义正确性 |
| AccuracyVerified | 否 | 需要阶段 7 的人工标注 NME |

用户模型 UI 仍显示“实验性/精度未验证”，RuntimeTested 不改变 LEVEL 3 门控。

## 6. 剩余限制

1. 未集成真实第三方关键点模型：未找到来源/许可清晰且拓扑为 68 点的可分发模型。
2. Provider 的“图像 → 裁剪 → 坐标回映 → 前端展示”像素级链路未在本阶段自动化，留待阶段 8 打包应用实测。
3. Windows 资源限制不完整，需在发行说明中标注。
4. 模型导入 UI 的完整交互（对话框、许可证确认）未在本阶段自动化；探针与 manifest 校验函数已被真实调用。
