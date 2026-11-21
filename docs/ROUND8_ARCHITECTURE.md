# ROUND 8 — 架构

- 日期：2026-11-21
- 范围：`src-tauri/src/face_intelligence/**`、`src-tauri/src/commands/face_intelligence.rs`、`src/face_reshape/**`（门控与稠密输入）、前端 AI 人像 / 模型库工作区
- 约束：不推翻前七轮；不重写手动变形；评估结论只用本机实测数字或公开资料，无法确认的字段写“待验证”。

## 1. 分层

```text
┌──────────────────────────── 前端（React）────────────────────────────┐
│ AiPortraitPanel   ModelHubPanel   ReshapePanel(LEVEL3 门控)          │
│ backend.ts invoke → list/select/analyze/probe/import/…               │
└──────────────────────────────────▲───────────────────────────────────┘
                                   │ Tauri commands（commands/face_intelligence.rs）
┌──────────────────────────────────┴───────────────────────────────────┐
│ FaceIntelligence（registry.rs，Arc 托管在 Tauri State）               │
│  ├── providers: RwLock<BTreeMap<String, Arc<dyn FaceProvider>>>      │
│  ├── verifications: RwLock<…>   质量实测缓存                          │
│  ├── active: RwLock<String>     当前 Provider                        │
│  └── ModelHub                   用户自备模型目录与校验               │
├──────────────────────────────────────────────────────────────────────┤
│ Provider 实现（providers/）                                           │
│  ├── yunet-5         随包 YuNet，5 点                                 │
│  ├── apple-vision    macOS 系统框架，85 个稠密点（ObjC 桥）            │
│  └── onnx:<id>       用户自备单文件 ONNX，face-68 契约                │
├──────────────────────────────────────────────────────────────────────┤
│ 统一数据（types.rs / topology.rs / coordinate.rs / dense.rs）         │
│ FaceAnalysisResult → FaceResult → Landmark / DenseRegion / Mask       │
├──────────────────────────────────────────────────────────────────────┤
│ 质量与门控（quality.rs）                                              │
│ 稳定性 → verified 能力 → Level3Gate（eyes/nose/mouth/chin 逐项）     │
├──────────────────────────────────────────────────────────────────────┤
│ 变形引擎（face_reshape，未重写）                                      │
│ FaceFrame.dense → face_mesh::push_dense_points → 现有 Warp             │
└──────────────────────────────────────────────────────────────────────┘
```

关键点：变形引擎不认识任何厂商的具体关键点编号。它只消费
`DenseFace.regions[].kind`（`RegionKind` 语义枚举）与归一化坐标。

## 2. 数据流

1. `commands::faces::detect_faces`（第四轮链路）继续提供 5 点与 faceUUID，保持不变。
2. `analyze_photo_faces` 用当前 Provider 分析同一张预览图：
   - Provider 输出 `FaceAnalysisResult`（坐标统一归一化到原图 0..1）；
   - `dense::assemble_dense` 把 `DenseRegion` 汇总为 `DenseFace`（指纹、点数、边界）；
   - `quality::evaluate_face_quality` 做结构校验；
   - `quality::level3_gate` 结合实测稳定性给出逐项门控；
   - 结果写入 `FaceRepository`（`state.rs`）并返回 `AnalyzeFacesReport`。
3. 变形时读取 `FaceFrame.dense/level3`：
   - `level3_unlocked` 为假时，`dense` 根本不进入网格构建；
   - 为真时，`push_dense_points` 按 `RegionKind` 映射到 `Face/Eye/Nose/Mouth/Fixed` 控制点；
   - LEVEL 1/2 参数在任何情况下都保留。
4. 保存项目：`ProjectFace.dense/level3` 随项目 JSON 落盘；重开时
   `restore_faces` → `attach_analyses` 按 UUID 写回，避免“重新检测后 LEVEL 3 悄悄失效”。

## 3. 失败隔离

- Provider 构造/加载失败只会让该 Provider 缺席，不影响 `yunet-5`、Apple Vision 与手动修图。
- 所有 Provider 错误统一为 `FaceError`（Unavailable / NotInitialized / InvalidInput /
  Inference / Timeout / Unsupported / Cancelled），命令层转成用户可读的中文错误，不做静默降级。
- 用户 ONNX 的校验与冒烟推理在应用自身子进程（`--face-model-probe`）执行，20 s 超时 kill，
  主进程不会被人为构造的死循环模型拖死。

## 4. 平台

| 平台 | yunet-5 | apple-vision | 用户 ONNX |
|---|---|---|---|
| macOS Apple Silicon | 已验证 | 已验证（真实推理） | 代码路径已验证；真实第三方模型未验证 |
| macOS Intel | 未验证 | 未验证 | 未验证 |
| Windows | 未验证 | 不支持（系统无 Vision） | 未验证 |
| Linux | 未验证 | 不支持 | 未验证 |

## 5. 明确非目标

- 不把 5 点/68 点重采样成“稠密网格”。
- 不把面部网格当作语义分割，不把二值蒙版当作 Alpha Matting。
- 不集成 Banuba / NVIDIA Maxine 二进制，不引入其数据结构。
- 不联网上传照片：全部分析在本机进程内完成。
