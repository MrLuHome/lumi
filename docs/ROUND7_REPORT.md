# ROUND 7 — 交付报告

- 日期：2026-11-21
- 基线：`main` @ `937b304` + 第七轮增量（未推翻第六轮）
- 目标：ROUND 6.1 收尾（空白窗口 P0、GUI 验收、能力标注、批量内存）+ RAW / ICC / GPU 三块
- 结论：RAW 与 ICC 主链路真实可用并通过测试；GPU 两个内核实测并具备 CPU 回退；
  **GPU 未接入主编辑管线、RAW 重定位 UI 未实现、Windows 未实机验证**，均不计为完成。

## 1. 阶段 0 / 6.1 收尾

| 项 | 结果 | 证据 |
|---|---|---|
| 空白窗口 P0 | 已修复：CSP 显式允许 `tauri:` / `http://tauri.localhost` / `ipc:`；重建后界面渲染 | `docs/ROUND6_GUI_ACCEPTANCE.md`、`docs/assets/round6-gui/00-blank-before-fix.jpg`、`01-launch-fixed.jpg` |
| 正式包 GUI 人工验收 | 22 步清单中 20 步有真实截图；第 20/21 项因辅助功能限制未执行，未计为通过 | `docs/ROUND6_GUI_ACCEPTANCE.md` |
| 能力标注 | 删除“AI 肤质诊断”类表述；皮肤深度分析标注“暂不支持” | `docs/ROUND6_CAPABILITY_LABELS.md` |
| 批量内存 | 50 张峰值 124.9 MB、100 张 118.5 MB，均 ≤600 MB | `docs/ROUND6_MEMORY_OPTIMIZATION.md`、`docs/assets/round7/memory-bench-50.log` / `-100.log` |

本轮最后一次重建后的真实 GUI 启动验证记录见 §7。

## 2. RAW Engine

- 依赖 `libraw_rs_vendor 1.0.0`（LibRaw 0.21.1），仅 `open/unpack/thumb/close`，**不调用 `dcraw_process`**。
- 线性 CFA → 黑电平 → MHC 5×5 / X-Trans 6×6 → 相机矩阵 → 线性 sRGB → WB/曝光/高光/阴影 → sRGB → `Image16`。
- 预览与导出共用 `develop_from_path`；RAW 导出固定 16-bit，原图不被覆盖。
- 项目 v3 保存 `source/raw/color/compute`；v2 项目可打开。
- 真实样本：CR3 / ARW / RW2 / ORF / 非压缩 DNG 可读；有损 DNG 明确不支持；
  NEF / RAF 未取得样本，真实机型未验证。
- 详见 `docs/ROUND7_RAW_ENGINE.md`。

## 3. ICC 色彩管理

- `lcms2 6.3.0` 静态接入；输入 ICC → sRGB 工作空间，输出时一次性转到目标 Profile。
- 输出：sRGB / Adobe RGB / Display P3 / ProPhoto / 用户 ICC；意图 + BPC + 可选嵌入。
- 嵌入：JPEG APP2 / PNG iCCP / TIFF 34675；WebP 不嵌入（标注）。
- 16-bit TIFF：自实现未压缩 RGB16 编码器。
- ColorChecker 24 色块 sRGB→AdobeRGB→sRGB：**平均 ΔE00 0.009，最大 0.076**。
- 输入 ICC 已在解码器真实生效（ProPhoto 标记 PNG 测试像素改变）；CMYK TIFF 支持内嵌/用户 Profile，质量标注“基础”。
- ColorSync 主显示器 ICC 读取 + 手动显示器 Profile 预览（PNG 内嵌显示 ICC）；多显示器自动切换未实现。
- 详见 `docs/ROUND7_COLOR_PIPELINE.md`。

## 4. GPU

- `wgpu 30.0.2`，Apple M4 Metal 实机；两个内核（曝光/色彩矩阵、曲线/饱和度）与 CPU 一致性 <0.01。
- **release 打包应用实测（权威）**：1.9MP **0.38×**、12MP **0.38×**、24MP **0.34×**；
  Auto 按 12MP 档 0.38× < 1.25× 的门槛**保持 CPU**，状态栏显示“CPU 计算”。
- dev 测试二进制曾测得 2.49×/1.45×/1.25×，但 dev CPU 未优化、倍率虚高，不作为启用依据。
- 注入失败后逐位回退 CPU 并给出原因；主编辑管线仍为 CPU，GPU 未接入。
- 详见 `docs/ROUND7_GPU_BENCHMARK.md`。

## 5. UI 与批量

- 新增 `RAW` / `色彩` / `性能` 三个工作区面板，均调用真实命令：
  RAW 元数据/矩阵/CFA/WB/内嵌预览、显影参数；色彩输出/意图/BPC/嵌入/软打样/显示器 Profile；
  计算后端策略/设备/实测倍率/基准表格。
- 状态栏显示文件类型、16-bit、工作区 sRGB、输出 Profile、GPU/CPU 实际状态。
- 批量导出支持 RAW 全尺寸显影、TIFF、输出 Profile，逐张解码释放；单张与批量字节一致测试通过。
- 软打样 UI 已接入，RAW 软打样禁用并说明原因。

## 6. 测试与构建

| 命令 | 结果 |
|---|---|
| `cargo test` | 通过：lib 295、集成 38；6 个 `#[ignore]` 手工基准 |
| `cargo clippy --all-targets -- -D warnings` | 通过（Rust 1.95 新增风格 lint 按名称显式 allow，见 §8） |
| `npm run typecheck` | 通过 |
| `npm run test -- --run` | 57 通过 |
| `npm run build` | 通过 |
| `npm run app:build` | 见 §7 |
| 真实 GPU 基准 | `docs/assets/round7/gpu-benchmark.log` |
| 色卡 ΔE00 | `docs/assets/round7/color-delta-e.log` |

日志目录：`docs/assets/round7/`。

## 7. 正式包真实启动验证

- 产物：`src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`（15.53 MiB）+ DMG（7.53 MiB）
- 方式：真实 `open` 打包应用，非 dev、非仅自动化；用系统截图核验。
- 结果：
  1. 界面完整渲染：侧栏、画布、右侧面板、状态栏均可见，非空白（`docs/assets/round7/01-launch-app.png`）。
  2. 顶栏出现 RAW / 色彩 / 性能工作区；三个面板均可点击切换
     （`02-raw-panel.png`、`03-color-panel.png`、`04-performance-panel.png`）。
  3. RAW 面板显示“RAW 显影 / 尚未打开照片”；色彩面板显示输出 Profile、意图、BPC、嵌入、软打样、
     显示器 ColorSync（LCD Color）；性能面板显示设备 `Apple M4`、后端 `cpu · integratedGpu`。
  4. 性能面板运行基准后显示真实表格，Auto 保持 CPU（`05-gpu-benchmark-panel.png`）。
- **P0 空白窗口在第七轮正式包上确认未复发。**

## 8. 未完成 / 未验证（不得计为完成）

1. GPU 未接入主编辑管线（T8.1 未完成）。
2. RAW 缺失时的“重定位”UI 未实现（参数保留已实现，T5.4 部分完成）。
3. 面板级前端渲染测试未引入 jsdom/RTL（T8.4 未完成）。
4. 有损压缩 DNG、Nikon HE/HE\* 不支持。
5. NEF / RAF 无真实样本，未实机验证。
6. CMYK 仅 8-bit TIFF、基础质量；CMYK JPEG 未接入。
7. WebP 不嵌入 ICC。
8. 多显示器 ColorSync 自动切换未监听。
9. Windows / D3D12 / Vulkan 未实机验证。
10. LibRaw LGPL 静态链接的源码包与重链接说明尚未随发行包提供（发行前必须补齐）。
11. P95 ΔE00、GPU NaN/Inf 注入未单独测试；GPU 一致性使用逐通道 <0.01。
12. Rust 1.95 新增的 10 类风格 lint 以显式 allow 放行，未逐一改写既有算法代码。

## 9. 结论

第七轮完成了 RAW 读取/显影、ICC 真实转换与嵌入、GPU 内核实测与 CPU 回退、批量 RAW/ICC 接入、
三个前端工作区与状态栏，以及 6.1 的 P0 修复、GUI 验收、能力标注和内存优化。
对未完成项按实际状态标注，不进入第八轮。
