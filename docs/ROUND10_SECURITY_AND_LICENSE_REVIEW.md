# ROUND 10 — 安全与许可审查

- 日期：2026-11-22
- 范围：macOS 1.0 Beta 候选包（`--no-default-features`，不含 LibRaw）
- 性质：工程审查记录，不构成法律意见

## 1. 数据与威胁模型

LUMI RETOUCH 是本地单机桌面应用：不登录、不上传、不联网推理、无遥测。

| 资产 | 风险 | 现有控制 |
| --- | --- | --- |
| 原始照片 | 被修改或覆盖 | 导入只读；导出目标不得与原图同路径；导出采用同目录临时文件 + 原子重命名 |
| 项目 `.lumi` | 半写入损坏 | `write_atomically` 临时文件 + rename；ChangeSet 整包事务式写入；失败保留旧文件 |
| 项目并发保存 | 旧数据覆盖新数据 | `base_revision` 冲突检测；幂等 `request_id`；共享保存锁；冲突时返回 `REVISION_CONFLICT` 而非强写 |
| 未保存修改 | 关窗丢失 | `UnsavedGuard` + 原生确认框（关闭窗口与 ⌘Q 均覆盖） |
| 用户导入 ONNX | 恶意文件、超大文件、伪装可执行文件 | 仅单文件；拒绝符号链接与外部数据；512 MB 上限；读取前检查大小；记录 SHA256；拒绝伪装/压缩/pickle |
| ICC / 色彩 | 畸形 profile | lcms2 解析失败返回错误，不写半成品；预览与导出同一转换 |
| 文件系统访问 | 越权读取 | 只访问用户通过系统选择器/拖拽给出的路径与项目所在目录；无后台扫描 |

## 2. 应用层安全

- CSP（`tauri.conf.json`）：`default-src 'self' tauri:`；`object-src 'none'`；`base-uri 'self'`；不允许远程脚本。
- 前端不拼接 shell 命令；Rust 侧无 `Command::new` 执行用户输入路径。
- 项目 JSON 解析：未知字段保留（`#[serde(flatten)] extra`）；缺失字段按版本默认；`null` 与缺失区分；不会因旧字段丢失导致参数被重置。
- 迁移：解码器版本、RAW/ICC 元数据在迁移中保留；失败保留原项目文件。
- 日志：不记录照片像素内容或人脸裁剪图；保存失败日志只含 `project_id`、实体 UUID、字段路径与 revision。

## 3. 第三方组件许可清单

| 组件 | 用途 | 许可 | 状态 | 随包 |
| --- | --- | --- | --- | --- |
| Tauri 2 / WRY / tao | 桌面壳与 IPC | MIT / Apache-2.0 | 通过 | 是 |
| React 19 / React DOM | UI | MIT | 通过 | 是（打包进 JS） |
| Zustand | 状态管理 | MIT | 通过 | 是 |
| lucide-react | 图标 | ISC | 通过 | 是 |
| Vite / TypeScript | 构建工具 | MIT / Apache-2.0 | 通过（不随包） | 否 |
| `image` | 解码/编码 | MIT / Apache-2.0 | 通过 | 是 |
| `tiff` / `flate2` / `bytemuck` / `base64` | TIFF、压缩、字节处理 | MIT / Apache-2.0 | 通过 | 是 |
| `rayon` / `thiserror` / `serde` / `sha2` | 并行、错误、序列化、哈希 | MIT / Apache-2.0 | 通过 | 是 |
| `lcms2` / `lcms2-sys` | ICC 转换 | MIT（Little CMS） | 通过 | 是 |
| `tract-onnx` | 本地 ONNX 推理 | MIT / Apache-2.0 | 通过 | 是 |
| `wgpu` / `naga` | GPU 基准（不在主管线） | MIT / Apache-2.0 | 通过 | 是 |
| `rfd` | 原生文件/确认对话框 | MIT | 通过 | 是 |
| YuNet 2023mar 权重 | 人脸检测与 5 点 | MIT（OpenCV Zoo） | 通过，SHA256 固定 | 是 |
| `libraw_rs_vendor`（LibRaw 0.21.1） | RAW 解码 | LGPL-2.1-only 或 CDDL-1.0 | **LEGAL_REVIEW_PENDING** | **否（Beta 构建排除）** |
| Apple Vision | 系统人脸关键点 | Apple SDK 条款 | 系统框架，不随包权重 | 系统提供 |

未发现 GPL / AGPL 组件进入 Beta 二进制；未包含 FFmpeg、Qt、商业 SDK。

## 4. LibRaw 专项

事实（默认构建，历史证据）：

- 静态链接，`otool -L` 无 libraw 动态依赖；`nm` 可见 LibRaw C++ 符号；字符串 `LibRaw 0.21.1`。
- 分发材料齐备：对应源码包（SHA256 `58317a2875f310a48cfbe03d92a178e148e32a3d032462eee559d8abcba924f9`）、COPYRIGHT、LGPL-2.1、CDDL-1.0、SOURCE、RELINK、MODIFICATIONS、构建脚本。
- 法律审核未完成，因此**不能声明合规**。

本轮处置：

1. 引入 Cargo feature：`default = ["raw"]`，`raw = ["dep:libraw_rs_vendor"]`。
2. Beta 包以 `--no-default-features` 构建：不编译、不链接 LibRaw。
3. 实测二进制 `nm -a` 中 `libraw` 匹配数 = 0，`otool -L` 无匹配。
4. RAW 路径统一返回「当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）」；导入过滤器也会命中该提示，不会静默跳过。
5. 包内 `Resources/resources/libraw/` 仍保留合规参考资料（静态资源列表无法按 feature 条件化）；其中不包含被链接的目标文件。发布说明应说明其为参考资料。

残余风险：

- 中文/英文“LibRaw”字样仍出现在 UI 与项目元数据中，可能被误解为已启用；发布说明与能力接口 `enabled: false` 已明确。
- 若未来恢复默认构建，必须重新执行 `check_release_compliance.sh` 并取得法律结论，不能沿用本轮结论。

## 5. 模型安全

- 随包模型只有 YuNet，SHA256 固定；加载前校验失败即拒绝。
- 用户模型导入：体积上限、拒绝符号链接/外部数据/伪装格式、记录 SHA256；推理使用 tract（无动态插件加载）。
- 皮肤分割与人物分割 AI 权重均未登记可商用来源，功能回退并明确提示，不使用未授权权重。
- `ProtocolTestOnly` 仅用于协议测试，不进入发行能力声明。

## 6. 隐私

- 所有推理与图像处理在本机完成。
- 无崩溃上报、无使用统计、无账户体系。
- 测试照片来自 Unsplash License 与 raw.pixls.us CC0，仅本地测试，未用于训练或公开发布（详见 `ROUND10_REAL_PHOTO_WORKFLOW.md`）。

## 7. 未解决风险

| 风险 | 等级 | 说明 |
| --- | --- | --- |
| LibRaw 法律审核未完成 | 高（已隔离） | Beta 不含该组件；GA 前必须完成审核 |
| 未签名/未公证 | 中 | Gatekeeper 需右键打开；发行体验受限 |
| Windows 未验证 | 中 | 不发布，不做兼容性声明 |
| Apple Vision 稠密点仅 Declared | 中 | 不解锁 LEVEL 3，不作为精度宣传 |
| ad-hoc 签名 | 中 | 无法证明包未被篡改；Beta 阶段接受并记录 SHA256 |
