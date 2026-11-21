# ROUND 8 — 审计

- 日期：2026-11-21
- 基线：第七轮 `3d90597` + 收尾 `09799e9`
- 结论：Provider 抽象、能力门控、用户模型安全路径、质量实测、LEVEL 3 门控、项目持久化均已落地并通过自动化测试；
  **不支持的能力（parsing / matting / mesh）保持禁用**，**未打包任何未经授权权重**，
  **商业 SDK 未集成**，**Windows 未验证**，**用户自备真实 ONNX 模型未验证**。

## 1. 交付物

### 1.1 后端

| 模块 | 内容 |
|---|---|
| `face_intelligence/provider.rs` | `FaceProvider` 统一 Trait、`FaceInput/AnalysisOptions/FaceError/ProviderHealth` |
| `face_intelligence/capability.rs` | 能力位、证据等级、`verified_only`、许可证状态 |
| `face_intelligence/types.rs` | 归一化人脸/关键点/区域/蒙版/网格统一结构 |
| `face_intelligence/topology.rs` | 拓扑注册（yunet-5 / apple-vision / face-68 / face-126 预留）与校验 |
| `face_intelligence/coordinate.rs` | 裁剪/原图坐标互转、非有限值处理 |
| `face_intelligence/anchors.rs` | 稠密结果 → 5 点变形锚点；稀疏兼容 |
| `face_intelligence/dense.rs` | 区域聚合、指纹、点数、边界 |
| `face_intelligence/quality.rs` | 结构校验、稳定性探针门槛、LEVEL 3 逐项门控 |
| `face_intelligence/registry.rs` | Provider 注册/切换、分析记录、质量验证缓存、未加载安全 |
| `providers/yunet.rs` | 5 点 Provider（随包） |
| `providers/apple_vision.rs` + `vision_bridge.m` | macOS 系统框架 85 点 Provider（左右语义归一化） |
| `providers/onnx.rs` | 用户自备 68 点 ONNX Provider |
| `model_hub/*` | 单文件导入、manifest、安全校验、子进程探针、存储 |
| `commands/face_intelligence.rs` | 12 个命令 |
| `face_reshape/face_mesh.rs` | 稠密控制点注入（按语义，不按编号） |
| `face_reshape/mod.rs` | LEVEL 3 逐组门控；未解锁不建稠密网格 |
| `face_reshape/identity.rs` | 项目人脸记录携带 dense/level3 |
| `state.rs` | `attach_analyses` 按 UUID 写回 |
| `build.rs` / `main.rs` | Objective-C 桥编译与探针子进程入口 |

### 1.2 前端

| 组件 | 内容 |
|---|---|
| `AiPortraitPanel.tsx` | Provider 卡片、能力 chips、证据表、分析、质量验证、逐脸门控 |
| `ModelHubPanel.tsx` | 导入表单、安全说明、已安装模型卡片、使用/重验/移除 |
| `ReshapePanel.tsx` | LEVEL 3 动态门控（按眼睛/鼻/唇/下巴），未解锁显示原因 |
| `editorStore.ts` | `updateFaceFeature` |
| `TopBar.tsx` / `App.tsx` | “AI 人像”“模型库”工作区 |
| `types.ts` / `backend.ts` | 全量类型与 invoke 封装 |

## 2. 能力矩阵（如实标注）

| 能力 | 状态 | 证据 |
|---|---|---|
| Provider 统一抽象 | 已验证 | 3 个实现 + 集成测试 |
| 能力声明与证据等级 | 已验证 | `capability.rs` 单测：`Unsupported` 不放行 |
| 5 点不冒充稠密 | 已验证 | `yunet_provider_stays_sparse_and_locks_level3` |
| Apple Vision 稠密 85 点 | 已验证（macOS） | 真实样张，5 张图，4 人合影 |
| 左右语义归一化 | 已验证 | `apple_vision_returns_real_dense_regions` 校验图像左侧 = 被摄者右眼 |
| 质量探针稳定性 | 已验证 | 3 次重复，最大偏差 0.000000，ratio 0% |
| LEVEL 3 质量门控 | 已验证 | 探针通过后才解锁；5 点永远锁定 |
| 解锁与锁定像素差异 | 已验证 | 27,494 像素；控制点 235 → 320 |
| 项目持久化 dense/level3 | 已验证 | `project_face_roundtrips_dense_analysis` |
| 用户模型路径安全校验 | 已验证（单元测试） | 伪装格式、路径、哈希、external_data |
| 用户模型真实端到端 | **未验证** | 无合法第三方 68 点 ONNX 样本 |
| Face Parsing | **不支持** | 无 Provider，UI 显示关闭 |
| Hair / Face Matting | **不支持** | 无 Provider，UI 显示关闭 |
| Face Mesh | **不支持** | Apple Vision 不提供，未接入其他模型 |
| 商业 SDK | **未集成** | 未采购、未下载、无二进制 |
| Windows / Linux | **未验证** | 本机只有 macOS |
| 45MP 输入 | **未验证** | 仅 24MP 降采样测试 |
| NME 定位精度 | **未验证** | 无标注数据集 |

## 3. 测试与构建（本轮实跑）

| 命令 | 结果 |
|---|---|
| `cargo test --lib face_intelligence` | 58 passed |
| `cargo test --test face_intelligence` | 5 passed（含真实 Apple Vision 推理） |
| `cargo test`（全量） | 通过 |
| `cargo clippy --all-targets` | 无警告（修正 1 处 `needless_borrow` 与类型复杂度） |
| `cargo test --test round8_benchmark -- --ignored --nocapture` | 1 passed，78.2 s，输出真实数字 |
| `npm run typecheck` | 通过 |
| `npm test -- --run` | 57 passed |
| `npm run build` | 通过 |
| `npm run app:build` | 成功，`.app` 15.85 MiB，dmg 7.70 MiB |
| 打包应用 GUI 验收 | PASS（5 屏，2 处真实 UI 缺陷已修复），见 `ROUND8_GUI_ACCEPTANCE.md` |

注：release 配置 `panic = "abort"`，测试与基准只能使用 dev profile；
基准中的 YuNet 耗时（~1.28 s）不代表 release 性能。

## 4. 对前七轮的影响

| 既有能力 | 是否保留 | 说明 |
|---|---|---|
| 手动变形（LEVEL 1/2） | 是 | 未改算法；`sanitize` 只在 LEVEL 3 未解锁时清零 LEVEL 3 字段 |
| YuNet 5 点检测与 faceUUID | 是 | 命令与数据结构不变 |
| RAW / ICC / GPU / 批量 | 是 | 未改动相关模块 |
| 项目 v1–v3 兼容 | 是 | 新字段 serde default；旧项目无 dense 时保持锁定 |
| 第三轮皮肤蒙版 | 是 | 不依赖新 Provider |

## 5. 诚实性检查

- 没有把 5 点/68 点插值成稠密网格。
- 没有把关键点轮廓称为 Parsing 或 Matting。
- 没有把 Apple Vision 的 `confidence=1.0` 说成逐点置信度。
- 没有把 debug 基准当成 release 性能。
- 没有宣称用户模型“已支持任意 ONNX”。
- 没有宣称商业 SDK 可用或价格可接受。
- 没有宣称 LGPL 发行合规已完成。
- 没有宣称 Windows 可用。

## 6. 独立检查（任务书第 18 节 12 项）

| # | 检查 | 结论 | 依据 |
|---|---|---|---|
| 1 | 是否存在假关键点 | 否 | 5 点不构造 regions；68 点不插值；`yunet_provider_stays_sparse_and_locks_level3` |
| 2 | 是否存在假分割 | 否 | 所有 Provider `face_parsing=false`；UI “分割”chip 为关闭；无蒙版填充 |
| 3 | 是否存在假 Matting | 否 | `face_matting=false`；无 Alpha 输出；文档明确不支持 |
| 4 | 是否打包未经授权模型 | 否 | bundle 仍只有 YuNet 2023mar（MIT + LICENSE）；用户模型不进包 |
| 5 | 是否把研究模型误标为商用 | 否 | 无研究模型接入；用户模型固定 `UserUnverified` |
| 6 | 是否真正支持静态照片 | 是 | 5 张静态样张（含 24MP 降采样、合影、明暗）真实推理 |
| 7 | 是否真正支持多人脸 | 是 | 合影实测 4 脸 / 340 点；GUI 与基准均验证 |
| 8 | 是否支持模型切换 | 是 | GUI 实测 YuNet ↔ Apple Vision，顶部/卡片/门控同步更新 |
| 9 | 是否破坏 faceUUID | 否 | `FaceFrame` 只新增可选字段；`stable_key()` 与匹配逻辑未改；持久化测试通过 |
| 10 | 是否破坏第四轮变形 | 否 | `face_reshape` 未重写；LEVEL 1/2 算法与参数不变；回归测试通过 |
| 11 | 是否影响前七轮功能 | 否 | RAW/ICC/GPU/批量/项目模块未改；全量 `cargo test` 与前端 57 项通过 |
| 12 | 是否存在未经授权的数据上传 | 否 | 全部分析在本机进程；无网络请求代码；无遥测；用户模型也不上传 |
| 13 | 是否存在模型导入安全风险 | 部分缓解，仍有缺口 | 文件头/路径/哈希/子进程隔离/超时已实现；无签名验证、无 OS 级沙盒，真实恶意模型未测试 |

（第 12 项是任务书列出的第 12 条；上表多列了导入安全，便于对照第 2 节。）

## 7. 遗留问题

1. 真实 68 点 ONNX 样本与端到端导入未验证（需合法权重）。
2. 分割边界准确性、Matting 质量、NME 精度均无数据（无模型/无标注集）。
3. 内存只测了进程 RSS 累计峰值，未拆分“模型加载 / 单次推理”。
4. Windows 构建与 Vision 替代方案未验证。
5. LibRaw 发行附件（源码包、重链接说明、许可页）未补齐。
6. 探针结果不跨进程持久化：重启应用后需重新验证才能解锁 LEVEL 3（有意保守，但需在 UI 说明）。
