# ROUND 9 — 正式打包与 GUI 验收

- 日期：2026-11-21
- 平台：macOS 27.2.1 (26B131)，Apple M4，16 GB，arm64
- 产物：`LUMI RETOUCH.app` 16.52 MiB；`LUMI RETOUCH_0.1.0_aarch64.dmg` 8.31 MiB
- 构建命令：`npm run app:build`（= `tauri build`，前置 `pnpm build`）
- 验收方式：真实启动 release 包，不使用 Vite dev server

## 1. 构建与产物

| 项 | 值 |
| --- | --- |
| 主可执行文件 | 16,017,840 B，SHA256 前缀 `8b886eda82b034b7` |
| .app 内 LibRaw 合规附件 | 全部存在（`check_release_compliance.sh` PASS） |
| DMG | 存在且不早于 .app |
| Windows NSIS | **未构建、未验证**（本机无 Windows 环境） |

## 2. 验收结果（macOS）

| # | 场景 | 结果 | 证据 |
| --- | --- | --- | --- |
| 1 | 冷启动（无项目） | PASS：显示空图库「工程底座 v0.1」 | `01-cold-launch.png` |
| 2 | 首次打开项目（冷启动后） | PASS：磨皮 17 / 纹理 33 / 祛斑 11，非 0 | `02-project-opened.png`、`03-face-params.png` |
| 3 | Apple Vision 分析 | PASS：1 张人脸、置信度 100%、85 点、117 ms | `04-ai-analysis.png` |
| 4 | 分析后参数保持 | PASS：17/33/11 未被重置 | `05-params-after-analysis.png` |
| 5 | 模型库初始状态 | PASS：已安装（0），提示「校验在独立子进程中运行」 | `06-model-hub.png` |
| 6 | 导入用户 ONNX（协议模型） | PASS：校验通过，输入 64×64、输出 `[1,N,3]`、拓扑 `face-68`、状态「可用」 | `07-model-imported.png` |
| 7 | 保存 → 退出 → 冷启动 → 重开 | PASS：17/33/11 一致 | `08-reopen-after-save.png`（修复前）、`13-final-reopen.png`（修复后） |
| 8 | 模型库跨重启持久化 | PASS：`ProtocolTestOnly-68` 仍在，SHA256 一致 | `09-model-hub-after-restart.png` |
| 9 | 导出 | PASS：`已导出 … 386 KB`，1680×2240 JPEG，395,760 B | `/tmp/lumi-round9-export3.jpg` |
| 10 | 最终「打开→保存→重启→重开」 | PASS：无「需重新分析」提示，dense 10 区域/85 点、`denseLandmarks=true` | `12-final-open.png`、`13-final-reopen.png` |
| 11 | Windows 启动/Worker/异常 | **未验证**（无环境，不计为通过） | — |

### 能力等级检查

- 模型库导入后只显示「可用」，**没有**出现正式 LEVEL 3。
- AI 面板明确提示「LEVEL 3 未解锁（稠密点未通过门控）、结构验证 0/4 区域」。
- 未把 `ProtocolTestOnly` 模型写成真实人脸模型通过。

## 3. 验收中发现并修复的缺陷

### 3.1 保存删除已有稠密关键点（P0 级数据丢失）

- 现象：打开带 Apple Vision 分析结果的项目（默认 Provider 为 YuNet）后直接保存，
  项目文件中 `faces[0].dense` 整段消失，文件从 27,281 B 降到 8,070 B。
- 根因与修复：见 `ROUND9_PROJECT_RECOVERY_TESTS.md` §6.1。
- 复验：修复后保存文件 27,356 B，`dense` 10 区域 / 85 点、`level3.denseLandmarks=true`，
  冷启动重开无「需重新分析」提示。

### 3.2 导出耗时包含文件对话框停留时间

- 现象：在保存对话框停留约 136 秒，提示「已导出 … 137.50 s」。
- 修复：`export_image` 的计时从「对话框确认之后」开始。
- 复验：在对话框停留约 5 秒，提示「已导出 lumi-round9-export3.jpg · 386 KB · 258 ms」。

## 4. 性能记录

| 项 | 数值 | 说明 |
| --- | --- | --- |
| 冷启动 → 首屏可交互 | 约 2 s（目测） | 未做精确埋点 |
| 项目加载（状态栏「就绪」） | 158 ms（首次）、149 / 145 ms（后续） | 含参数、人脸、蒙版恢复 |
| 双击 → 画面稳定 | 0.79–0.95 s | 含文件对话框关闭与首帧渲染 |
| 主进程内存（空图库） | 99.5 MB | RSS |
| 打开项目后 | 283.8 MB | RSS |
| 导出后 | 386.0 MB | RSS |
| 连续 3 次重开 | 376.7 → 292.7 → 272.2 MB | 无单调增长 |
| 导出（全尺寸 1680×2240） | 258 ms | 不含对话框 |
| Apple Vision 分析 | 117 ms | 85 点 |
| ONNX Worker 首次加载+推理 | 13–14 ms | 每次推理重新加载模型（协议模型，64×64） |
| Worker 崩溃/超时恢复 | 立即返回错误，主进程存活 | 见 `ROUND9_ONNX_RUNTIME_VALIDATION.md` |

冷/热差异：项目加载首次 158 ms、后续 145–149 ms；差异来自页面缓存与解码缓存，
未出现成倍波动。以上均为单次实测，不代表统计分布。

## 5. 未测项（不得视为通过）

- Windows 安装包、启动、Worker、异常处理；
- Intel Mac / 其他 macOS 版本；
- 长时间（>1 小时）连续编辑的内存曲线；
- 多显示器 / 不同缩放比下的界面检查；
- DMG 挂载安装流程的逐屏截图（仅确认文件存在）。
