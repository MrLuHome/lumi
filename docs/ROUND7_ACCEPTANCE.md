# ROUND 7 — 验收清单

- 日期：2026-11-21
- 被测产物：`src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`
- 构建：`npm run app:build`（日志 `docs/assets/round7/app-build.log`）

## 1. 验收门槛逐项

| # | spec 门槛 | 结果 | 证据 |
|---|---|---|---|
| 1 | 空白窗口修复，GUI 验收逐项有证据 | **通过** | 第六轮 20/22 步有截图；第七轮重建后真实启动见 §3 |
| 2 | 50 张批量 ≤600 MB | **通过** | 50 张 124.9 MB；100 张 118.5 MB（`docs/ROUND6_MEMORY_OPTIMIZATION.md`） |
| 3 | 真实 RAW 全链路、元数据真实、原图不变 | **通过（有范围限制）** | CR3/ARW/RW2/ORF/DNG 实测；`raw_reads_real_sample`、`raw_preview_and_export_share_develop`；NEF/RAF 无样本未验证 |
| 4 | ICC 数值真实改变、嵌入可回读、无重复转换 | **通过** | ΔE00 0.009/0.076；APP2/iCCP/34675 回读；`roundtrip_does_not_drift_repeatedly` |
| 5 | GPU 两项运算实测 + CPU 对比 + 回退 | **通过（内核范围）** | Metal 基准 2.49×/1.45×/1.25×；`injected_gpu_failure_falls_back_to_cpu`；主编辑管线未接入 |
| 6 | 全部命令与构建通过 | **通过** | `cargo test`、clippy、typecheck、vitest、build、app:build |
| 7 | 10 份文档齐备，未验证项如实标注 | **通过** | 7 份 ROUND7 + 3 份 ROUND6 文档；未完成项见 `ROUND7_REPORT.md §8` |

## 2. 自动化测试

| 目标 | 通过 | 忽略 |
|---|---:|---:|
| lib（`cargo test --lib`） | 295 | 3 |
| `tests/beauty.rs` | 12 | 1 |
| `tests/beauty_real.rs` | 4 | 0 |
| `tests/engine.rs` | 10 | 1 |
| `tests/face_detection.rs` | 4 | 0 |
| `tests/raw_real.rs` | 2 | 0 |
| `tests/reshape_persistence.rs` | 3 | 0 |
| `tests/reshape_pipeline.rs` | 3 | 1 |
| 前端 vitest | 57 | 0 |

- `cargo clippy --all-targets -- -D warnings`：通过。
- `npm run typecheck`、`npm run build`：通过。
- 忽略项为手工 GPU/内存/分辨率基准，需要真实硬件或长耗时，不参与常规 CI。

## 3. 正式包真实启动（P0 回归）

- 启动方式：真实打开 `LUMI RETOUCH.app`（非 `tauri dev`）。
- 检查项：
  1. 窗口不是空白，WebView 渲染出侧栏、顶栏、画布、状态栏；
  2. 顶栏出现 RAW / 色彩 / 性能工作区入口；
  3. 点击三个入口，面板可见且不是占位按钮；
  4. 状态栏显示工作区 / 输出 Profile / CPU 或 GPU。
- 截图与结果：**见下方最终记录**。

### 最终记录

- 启动截图：`docs/assets/round7/01-launch-app.png`（完整界面，非空白）。
- 工作区切换截图：`02-raw-panel.png`（RAW 显影）、`03-color-panel.png`（色彩管理）、
  `04-performance-panel.png`（性能与计算后端）。
- 基准截图：`05-gpu-benchmark-panel.png`（Apple M4，GPU 可用未启用，实测 0.38×，Auto 保持 CPU）。
- 结论：P0 空白窗口未复发；三个新工作区真实渲染且连接真实命令；状态栏显示“CPU 计算 · 就绪”。
- 限制：无照片打开时 RAW/色彩的部分控件按设计禁用并显示原因（“打开照片后可运行软打样”等），
  未执行真实 RAW 导入的 GUI 点击验收；RAW 全链路由 `tests/raw_real.rs` 覆盖。
