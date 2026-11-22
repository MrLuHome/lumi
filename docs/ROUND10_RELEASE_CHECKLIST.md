# ROUND 10 — 1.0 Beta 发行检查清单

- 日期：2026-11-22
- 产物：macOS 1.0 Beta 候选包（不含 LibRaw）
- 本文件在阶段 E 完成时落笔；阶段 F 完成后如数值变化，以 `ROUND10_REPORT.md` 为准。
- 当前候选判定：**BETA_READY_WITH_LIMITATIONS**（依据见第 8 节；最终判定在阶段 F 全量验收后写入 `ROUND10_REPORT.md`）

## 1. 发行物与构建信息

| 项 | 值 |
| --- | --- |
| 版本 | `0.1.0`（`package.json` / `Cargo.toml` / `tauri.conf.json` 一致） |
| 构建命令 | `npm run app:build:beta`（等价 `tauri build -- --no-default-features`） |
| 架构 | arm64（Apple Silicon，本机 macOS） |
| `.app` | `src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`（16.38 MiB） |
| `.app` 主程序 SHA256 | `02da73e2987a38ff0ac554ecb62aa9ee72caf0c3fc687a36c9f48d01b6dd68c4` |
| `.dmg` | `src-tauri/target/release/bundle/dmg/LUMI RETOUCH_0.1.0_aarch64.dmg`（8.24 MiB） |
| `.dmg` SHA256 | `f771d8c58fc8b3cad04276dd3d3f2803f74b3e3abdab9b3e03d415d8ef3a909f` |
| 签名 | ad-hoc（`codesign -dv`：`Signature=adhoc`），`TeamIdentifier=not set` |
| 公证 | **未公证**（无 Developer ID 证书），首次打开需右键“打开” |
| 二进制 LibRaw 符号 | `nm -a` 匹配 `libraw` = **0**；`otool -L` 无 libraw 动态依赖 |
| 二进制残留字符串 | 仅 UI/元数据文本（如 `LibRaw 许可审核中`、项目解码器版本号 `libraw-0.21.1+lumi-mhc-1`），不含 LibRaw 代码 |

构建日志摘要（真实输出）：

```
Finished `release` profile [optimized] target(s) in 3m 06s
Bundling LUMI RETOUCH.app (16.38 MiB)
Bundling LUMI RETOUCH_0.1.0_aarch64.dmg (8.24 MiB)
```

## 2. E1 — macOS 安装与运行

| 检查项 | 结果 | 证据 |
| --- | --- | --- |
| 正式 `.app` 生成 | PASS | 16.38 MiB，`Contents/MacOS/lumi-retouch` 存在 |
| `.dmg` 生成 | PASS | 8.24 MiB |
| 挂载 | PASS | Finder 显示 `LUMI RETOUCH.app` 与 Applications 快捷方式 |
| 复制安装 | PASS | 拖入 `/Applications` 并替换；安装副本与 DMG 内二进制 SHA256 均为 `02da73e2987a38ff0ac554ecb62aa9ee72caf0c3fc687a36c9f48d01b6dd68c4` |
| 启动 | PASS | 主界面「图库 00」「导入照片开始修图」 |
| 权限（系统弹窗） | PASS（无阻塞弹窗） | 文件访问走用户主动选择，未触发完全磁盘访问授权 |
| 文件访问 | PASS | 能打开仓库外 `/tmp/lumi-round10/gui/round10.lumi` 并读取同目录照片 |
| 模型目录 | PASS | 随包 YuNet 从 `Contents/Resources/resources/models/yunet/` 加载，人脸检测与塑形可用 |
| SQLite | N/A | 本项目不使用 SQLite；项目为单文件 JSON `.lumi`，用户数据目录无数据库 |
| 卸载 | PASS | 删除 `/Applications/LUMI RETOUCH.app` |
| 重新安装 | PASS | 从 DMG 再次复制并启动 |
| 旧项目迁移 | PASS | 旧版项目 `round10.lumi` 在 Beta 包中打开，曝光 30 / 对比度 20 / 饱和度 25 / 磨皮 50 / 瘦脸 50 全部恢复 |
| 升级后项目可打开 | PASS | 同上（阶段 C 保存的项目由阶段 E 包打开） |
| 退出 | PASS | ⌘Q 后进程消失（`pgrep` 无结果） |

截图（`/tmp/lumi-round10/gui/shots/`）：
`beta2-01-installed.png`、`beta2-02-launched.png`、`beta2-03-raw-rejected.png`、`beta2-04-project.png`、`beta2-05-quit.png`（最终 Beta 包，SHA256 `f771d8c5…`）；
早期同流程截图 `beta-01-dmg-mounted.png` 至 `beta-05-quit.png`；
阶段 C/D 的 `step30a-launch-installed-copy.png` 至 `step30h-quit-reinstalled.png` 记录安装、重装与退出。

RAW 拒绝提示原文（最终包实测）：

```
不支持的图片格式：sample-canon.cr2 是 RAW 文件，当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）
```

快捷键说明：⌘O 绑定“导入照片”，打开项目为 ⇧⌘O。该差异在 `ROUND10_UX_REVIEW.md` 中记录，本轮不调整键位。

> 说明：第 4 步打开项目时参数为 50（未显示 42%），因为该 `round10.lumi` 的瘦脸是在阶段 B 修复后保存的；42% 是阶段 B 修复前旧构建的降强度提示。两者都不影响“参数持久化正确”。

## 3. E2 — LibRaw 法律状态与 Beta 替代方案

- 法律状态：**LEGAL_REVIEW_PENDING**（截至本轮未取得法律审核结论）。
- 正式包含 LibRaw 的包：**不得发布**（不满足 LGPL-2.1 静态链接分发义务的审核结论）。
- 本轮替代方案：Cargo feature `raw`（默认开启）。Beta 发行包使用 `--no-default-features` 构建：
  - `libraw_rs_vendor` 不参与编译；
  - 二进制无 LibRaw 符号与动态依赖；
  - 所有 RAW 入口返回明确错误，不假装成功。
- 用户可见限制：
  - 导入 RAW 扩展名时提示「… 是 RAW 文件，当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）」；
  - `raw_capabilities` 返回 `enabled: false`。
- 包内仍包含 `Contents/Resources/resources/libraw/` 合规材料（源码包、LGPL/CDDL、COPYRIGHT、RELINK 等）。这些是**参考资料而非已链接组件**，因为 `tauri.conf.json` 的 `bundle.resources` 是静态列表；不构成对 LibRaw 的使用声明。
- 发布说明必须写明：RAW 导入在 1.0 Beta 中不可用。
- 解除条件：法律审核书面确认 + 恢复默认构建的 DMG 重新验收。

详见 `ROUND10_SECURITY_AND_LICENSE_REVIEW.md`。

## 4. E3 — Windows

- 本机无 Windows 环境，本轮**未构建、未安装、未实机测试**。
- `tauri.conf.json` 虽声明 `nsis` 目标，但没有产物，不得标记通过。
- 结论：**Windows 保持未验证，不随 1.0 Beta 发布**。
- 禁止将任何 macOS 结果外推为 Windows 兼容性结论。

## 5. E4 — AI 模型与能力清单

| 模型 | 来源 | 许可 | SHA256 | Provider | Topology | 能力等级 | 精度证据 | 安全限制 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| YuNet 2023mar（随包） | OpenCV Zoo `face_detection_yunet_2023mar.onnx` | MIT（`resources/models/yunet/LICENSE`） | `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4` | `yunet-5` | `yunet-5`（5 点，非稠密） | 检测/5 点：Verified；稠密/解析：Unsupported | 真实样张集成测试（阶段 B） | 只输出 5 点，不用于稠密五官；LEVEL 3 门控保持锁定 |
| Apple Vision（系统框架） | macOS `Vision.framework` | Apple SDK 条款（不随包权重） | N/A | `apple-vision` | `apple-vision-landmarks` | 检测：Verified；稠密：Declared | 仅结构可用，未做 NME 精度验证 | LEVEL 3 不因 Declared 解锁；不上传图像 |
| 用户导入 ONNX | 用户自选 | 用户自负 | 导入时计算并记录 | `onnx-user-*` | 依模型声明校验 | 未验证 | 无 | 单文件 ≤512 MB、拒绝符号链接/外部数据/伪装可执行文件；不随包分发 |
| 皮肤分割权重 | 无 | — | — | — | — | — | — | 未登记可商用权重，回退几何蒙版 |
| 人物分割 AI | 无 | — | — | — | — | — | — | 未启用，仅手动蒙版 + Alpha 处理 |

- `ProtocolTestOnly` 模型不参与任何“真实能力”宣传，也不作为精度依据。
- 没有人工标注评测的定位能力不宣传“精准五官识别”；UI 与文档只写“5 点几何锚点”。

## 6. 安装、卸载与重装步骤（可复现）

```bash
hdiutil attach "src-tauri/target/release/bundle/dmg/LUMI RETOUCH_0.1.0_aarch64.dmg"
cp -R "/Volumes/LUMI RETOUCH/LUMI RETOUCH.app" /Applications/
open "/Applications/LUMI RETOUCH.app"
# 验证项目与导出后退出
osascript -e 'quit app "LUMI RETOUCH"'
rm -rf "/Applications/LUMI RETOUCH.app"
hdiutil detach "/Volumes/LUMI RETOUCH"
```

## 7. 已知发行限制

1. RAW 导入关闭（LibRaw 审核中）。
2. 未签名公证；首次启动需右键打开，Gatekeeper 提示“无法验证开发者”。
3. Windows 未验证、不发布。
4. 无自动保存；关闭窗口/⌘Q 时若未保存会弹出确认（原生对话框按钮为英文 Yes/No）。
5. 无历史时间线 UI（撤销/重做存在，但无跨会话历史）。
6. 无独立“新建项目”入口（导入照片即开始新项目）。
7. 人物分割为手动蒙版，非 AI；皮肤分割为几何蒙版。
8. 稠密关键点依赖 Apple Vision 且为 Declared，不构成 LEVEL 3 解锁依据。

## 8. Beta 判定依据

满足 `BETA_READY_WITH_LIMITATIONS`：

- 核心修图工作流（导入 → 修图 → 保存 → 重开 → 导出）通过。
- 未完成功能（RAW）已关闭并明确标记。
- LibRaw 法律问题通过构建隔离（不含该组件）解决。
- Windows 不发布。
- NME / 稠密关键点不作为精度宣传依据。

不满足 `BETA_READY`：存在上述功能限制且未公证；数据保存可靠性需阶段 F 的保存/加载专项与并发测试全部通过后再确认。

不落入 `NOT_RELEASE_READY`：未发现静默数据丢失、保存恢复不可靠、预览/导出不一致、阻断性真实照片缺陷、安装启动失败或许可风险未隔离。
