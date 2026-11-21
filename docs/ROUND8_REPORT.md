# ROUND 8 — 报告

## 1. 一句话

把“人脸智能”从具体模型里拆出来：统一 Provider 契约 + 证据等级 + 质量门控，
让 Apple Vision 的实测稠密结果解锁 LEVEL 3，让 5 点模型永远保持稀疏，
让用户自备 ONNX 在安全边界内导入；没有打包任何未授权权重，没有集成商业 SDK。

## 2. 本轮完成

1. **Provider 抽象**
   - `FaceProvider` Trait：`analyze / descriptor / health_check / unload`；
   - 统一输入 `FaceInput`（16-bit RGBA + 原始尺寸）与统一输出
     `FaceAnalysisResult`（归一化坐标、语义区域、置信度、耗时、警告）；
   - 错误类型 `FaceError` 覆盖不可用、未初始化、推理失败、超时、取消等。
2. **能力声明与证据**
   - 12 个能力位 + 4 级证据（Verified / Declared / Unverified / Unsupported）；
   - 只有 Verified 能放行 LEVEL 3；
   - `LicenseState` 区分系统框架、已验证商用、研究用途、用户自备、禁止等。
3. **三个 Provider**
   - `yunet-5`：随包 MIT，5 点，不产生稠密区域；
   - `apple-vision`：macOS 系统框架，85 点，左右语义按被摄者归一化；
   - `onnx:<id>`：用户自备 face-68 单文件模型，固定 `UserUnverified`。
4. **用户模型安全路径**
   - 单文件、≤512 MB、拒绝伪装格式/符号链接/`external_data`；
   - SHA256、ID 白名单、路径越界校验；
   - 独立子进程冒烟（20 s 超时 kill），失败原因具体到检查项。
5. **质量驱动解锁**
   - 重复分析稳定性（阈值 2% 眼距）+ 逐脸结构校验（置信度、点数、眼/唇宽度）；
   - 逐项门控 eyes/nose/mouth/chin，未通过显示原因；
   - 未解锁时稠密点不进入网格；解锁后控制点 235 → 320。
6. **持久化**
   - `ProjectFace.dense/level3` 随项目保存；重开按 UUID 写回；
   - 旧项目（v1–v3）无新字段时保持 5 点并锁定。
7. **UI**
   - 新增“AI 人像”“模型库”工作区；
   - 面部塑形 LEVEL 3 改为动态门控；
   - 修复两处真实缺陷：Provider “当前使用”误判、稀疏 Provider 门控原因误导。

## 3. 实测数字（本机 Apple M4 / macOS / debug 测试构建）

| 项 | Apple Vision | YuNet 5 点 |
|---|---|---|
| 每脸稠密点 | 85 | 0（不伪造） |
| 冷启动 | 313 ms | 1527 ms |
| 热推理 p50 | 63 ms | 1278 ms |
| 4 人合影 | 4 脸 / 340 点 / 65–76 ms | 4 脸 / 0 点 / ~1.28 s |
| 重复分析（3 次） | 最大偏差 0.000000，ratio 0% | 探针不适用 |
| 置信度 | 24/24 均为 1.000（检测级，非逐点） | 0.926–0.940 |
| GUI 分析 | 50 ms / 85 点 / 全门控 ✓ | — |

内存：测试进程 RSS 从 10.86 MB 增至 733.22 MB（累计峰值，debug，含 5 张图全部缓冲）。

限制：无标注集，**没有 NME**；Parsing / Matting / Mesh 不支持；Windows 未验证；
用户真实 ONNX 未验证；LibRaw LGPL 发行附件仍未补齐。

## 4. 文件改动

| 区域 | 文件 |
|---|---|
| 新增后端 | `src-tauri/src/face_intelligence/**`（provider/capability/types/topology/coordinate/anchors/dense/quality/registry/providers/model_hub） |
| 新增命令 | `src-tauri/src/commands/face_intelligence.rs` |
| 构建/入口 | `src-tauri/build.rs`、`src-tauri/src/main.rs`、`src-tauri/src/lib.rs` |
| 变形与持久化 | `face_reshape/mod.rs`、`face_mesh.rs`、`identity.rs`、`state.rs`、`commands/reshape.rs`、`image_engine/project.rs` |
| 前端新增 | `src/components/AiPortraitPanel.tsx`、`ModelHubPanel.tsx` |
| 前端修改 | `App.tsx`、`TopBar.tsx`、`StatusBar.tsx`、`ReshapePanel.tsx`、`editorStore.ts`、`types.ts`、`lib/backend.ts`、`styles/app.css` |
| 测试 | `src-tauri/tests/face_intelligence.rs`、`src-tauri/tests/round8_benchmark.rs` |
| 文档 | `docs/ROUND8_*.md`、`docs/assets/round8/*.png`、`THIRD_PARTY_LICENSES.md` |

## 5. 测试结果

| 命令 | 结果 |
|---|---|
| `cargo test --lib face_intelligence` | 58 passed |
| `cargo test --test face_intelligence` | 5 passed |
| `cargo test`（全量） | 通过 |
| `cargo clippy --all-targets` | 无警告 |
| `cargo test --test round8_benchmark -- --ignored --nocapture` | 1 passed（真实数字） |
| `npm run typecheck` / `npm test -- --run` | 通过 / 57 passed |
| `npm run app:build` | 成功（.app 15.85 MiB，dmg 7.70 MiB） |
| GUI 验收 | PASS（5 屏；修复 Provider 归属与门控原因 2 个缺陷） |

## 6. 文档

- `ROUND8_BASELINE_AUDIT.md`（已有）
- `ROUND8_ARCHITECTURE.md`
- `ROUND8_PROVIDER_API.md`
- `ROUND8_TOPOLOGY.md`
- `ROUND8_MODEL_HUB.md`
- `ROUND8_MODEL_SECURITY.md`
- `ROUND8_COMMERCIAL_SDK_COMPARISON.md`
- `ROUND8_LANDMARK_BENCHMARK.md`
- `ROUND8_PARSING_BENCHMARK.md`
- `ROUND8_MATTING_BENCHMARK.md`
- `ROUND8_GUI_ACCEPTANCE.md`
- `ROUND8_DEPENDENCY_AUDIT.md`
- `ROUND8_LIBRAW_DISTRIBUTION_COMPLIANCE.md`
- `ROUND8_AUDIT.md`
- `ROUND8_REPORT.md`（本文）
- 截图：`docs/assets/round8/*.png`

## 7. 商业 SDK POC

- Apple Vision：完成真实 POC（85 点、多人脸、静态照片、离线、耗时与稳定性实测）。
- Banuba：未申请试用、未采购、未下载、未集成；能力与价格均“待验证”。
- NVIDIA Maxine：未安装 SDK、未引入 CUDA/TensorRT；公开资料不面向 macOS，未实测。
- 发行包内无任何商业 SDK 二进制。

## 8. 用户模型导入

- 已实现：单文件 `.onnx`、≤512 MB、静态输入 32..1024、输出 68 点、拒绝
  `external_data`、SHA256、路径/ID 校验、独立子进程冒烟与 20 s 超时。
- 许可证固定 `UserUnverified`，即使 manifest 声明商用也不升级。
- 未验证：真实第三方 68 点模型的端到端导入与推理（无合法样本）。

## 9. 已知问题

1. 用户模型端到端未验证；Windows 未验证。
2. Parsing / Matting / Mesh 不支持；无 NME 与分割边界数据。
3. 内存只测累计 RSS 峰值；YuNet 的 release 性能未测。
4. 探针结论不跨重启缓存，重启后需重新验证才能解锁。
5. LibRaw LGPL 发行附件未补齐。

## 10. 下一轮建议（不进入第九轮）

1. 找一个许可证明确的 68 点 ONNX，走通真实导入/推理并补 SHA 与形状展示验收。
2. 引入 300W 等标注集做 NME 与遮挡/侧脸分层评测，替换“只有稳定性”的证据。
3. 若商业需求成立，再启动 Banuba 试用与条款审查；在此之前不集成。
4. 补齐 LibRaw 发行附件，或评估动态链接/CDDL 路径。
5. 修复探针结果跨重启缓存（将实测结论写入 model-hub/validation 或项目）。
