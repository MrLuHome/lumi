# ROUND 7 TASKS

> 依赖顺序执行；每项完成后必须留下可复核证据（命令输出 / 截图 / 测试名）。

## T0 — 空白窗口 P0 诊断与修复
- [x] T0.1 `npm run build` + `vite preview`，浏览器截图确认前端可渲染（`docs/assets/round7/`）。
- [x] T0.2 读取 Tauri 资源协议/CSP 证据：`log show`、WebKit 日志、`dist/index.html` 请求路径；定位根因。
- [x] T0.3 最小修复（预计 `vite.config.ts` 的 `base: "./"` 或 CSP 调整），`npm run app:build` 重建。
- [x] T0.4 重新启动 `.app`，截图 + AX 树证明界面元素存在。
- 验收：窗口内可看到工作区标签与导入入口，AX 树出现按钮/文本节点。

## T1 — 第六轮 GUI 人工验收
- [x] T1.1 按 spec §3.1 清单逐项执行，截图到 `docs/assets/round6-gui/`。
- [x] T1.2 记录无法执行项与原因（不得计为通过）。
- [x] T1.3 产出 `docs/ROUND6_GUI_ACCEPTANCE.md`。
- 验收：导入/参数/分析/预设/批量/项目恢复均有真实证据或明确“未执行”。

## T2 — 能力标注统一
- [x] T2.1 扫描 `src/components/**`、`src/state/**`、`src-tauri/src/**` 中能力文案与 capability 返回值。
- [x] T2.2 按 spec §3.3 表格修正文案与状态，不改算法。
- [x] T2.3 产出 `docs/ROUND6_CAPABILITY_LABELS.md`。
- 验收：无“AI 肤质诊断”类表述；皮肤深度分析显示“暂不支持”。

## T3 — 批量内存优化与测量
- [x] T3.1 `commands/batch.rs` / `state.rs`：批量专用缓存预算 ≤256MB，逐张作用域释放，`ImageCache` 不保留全尺寸。
- [x] T3.2 前端检查 objectURL 释放（`BatchWorkspace`/`SmartPanel`）；浏览器兜底导入的 `blob:` 预览在移除照片时 `revokeObjectURL`。
- [x] T3.3 新增批量内存基准（`commands/batch.rs::batch_memory_bench`，`#[ignore]`），50 张 1600×1200 + 100 张；`/usr/bin/time -l` 记录。
- [x] T3.4 产出 `docs/ROUND6_MEMORY_OPTIMIZATION.md`（原始输出）。
- 验收：50 张峰值 ≤600MB，或如实记录未达标原因与证据。

## T4 — RAW 依赖与最小验证
- [x] T4.1 `Cargo.toml` 增加 `libraw_rs_vendor = "1.0.0"`，`cargo build` 通过（记录 libclang 路径）。
- [x] T4.2 下载 CC0 样本到 `~/Library/Caches/lumi-raw-samples`，记录 SHA-256。
- [x] T4.3 `raw/libraw_ffi.rs` + `metadata.rs` + `linear.rs`：读取尺寸/CFA/WB/黑电平/相机矩阵/内嵌预览；不调用 `dcraw_process`。
- [x] T4.4 用真实样本跑通并打印元数据（测试 `raw_reads_real_sample`）。
- 验收：大于 0 的真实像素、CFA 与 WB 数值来自文件而非硬编码。

## T5 — RAW Engine
- [x] T5.1 `demosaic.rs`（Malvar-He-Cutler 5×5）+ 四种 Bayer 排列测试。
- [x] T5.2 `develop.rs`：曝光/高光/阴影/相机WB/自动WB/色温/色调，线性→sRGB 编码。
- [x] T5.3 命令 `raw_info` / `raw_preview` / `raw_develop`；内嵌预览快速路径。
- [ ] T5.4 项目 v3：`source/raw/color/compute`，旧 v2 可开；RAW 缺失提示重定位。
      **部分完成：v3 字段与 v2 兼容已完成；RAW 缺失时参数保留，但“重定位”UI 未实现。**
- [x] T5.5 全尺寸导出走同一 develop 函数；预览/导出一致性测试。
- [x] T5.6 `xtrans.rs` 6×6 基础插值（标注质量等级），有 RAF 样本则实测。
      **注：未获取 RAF 样本，只有合成测试；真实 RAF 未验证。**
- 验收：参数变化真实改变像素；原图 hash 不变；v2 项目打开无回归。

## T6 — ICC Engine
- [x] T6.1 `lcms2` 静态接入 + `color_management/{profiles,transform,embed,lab}.rs`。
- [x] T6.2 输入转换（v2/v4、无 Profile 策略）、输出转换（sRGB/AdobeRGB/P3/ProPhoto/用户）。
- [x] T6.3 嵌入：JPEG APP2、PNG iCCP、TIFF tag 34675；重新读取校验。
- [x] T6.4 软打样 + 色域警告（往返 ΔE00）。
- [x] T6.5 16-bit TIFF 自实现编码器（未压缩 RGB16）。
- [x] T6.6 色卡 ΔE00 测试与 `ROUND7_COLOR_PIPELINE.md` 草稿数据。
- [x] T6.7 `display.rs`：ColorSync 读取主显示器 ICC 并转换预览，失败回退 sRGB 并显示状态。
- [x] T6.8 CMYK Profile 读取与转 sRGB（标注质量等级）。
- 验收：转换前后像素值改变且方向正确；导出 Profile 可被重新识别；无重复转换。

## T7 — GPU Engine
- [x] T7.1 `wgpu` 30.0.2 + `pollster`，`rust-version` 升至 1.87，`cargo build` 通过。
- [x] T7.2 `device.rs` 探测 + `GpuCapabilities`；错误全捕获。
- [x] T7.3 `exposure.wgsl`（曝光/色彩）与 `curve.wgsl`（曲线/饱和）两项运算。
- [x] T7.4 CPU/GPU 一致性测试（平均/P95 ΔE00、NaN/Inf、边界、Alpha）。
      **注：使用逐通道 <0.01 断言，未单独输出 P95 ΔE00；NaN/Inf 不在 Image16 正常输入路径，未单独注入。**
- [x] T7.5 端到端基准（1600×1200 / 3000×4000 / 6000×4000），
      仅当 ≥1.25× 才默认启用；失败注入验证回退。
      **release 实测 0.34×–0.38×，Auto 保持 CPU；dev 的 1.45× 不作为依据。**
- [x] T7.6 `docs/ROUND7_GPU_BENCHMARK.md` 数据。
- 验收：截图/日志证明 Metal 设备被使用；回退到 CPU 有日志且功能完整。

## T8 — 统一管线、UI、批量
- [ ] T8.1 `pipeline::render` 接入 ingestion（RAW/Raster → Image16）与 GPU 可选后端，保持算法顺序。
      **未完成：RAW ingestion 在命令层完成；GPU 未接入主编辑管线（见 GPU 文档 §4）。**
- [x] T8.2 `RawPanel` / `ColorPanel` / `PerformancePanel` / 状态栏，全部连真实命令。
- [x] T8.3 批量支持 RAW/TIFF/输出 Profile，逐张解码释放，不预解码。
- [ ] T8.4 前端测试补齐（面板渲染、状态显示、禁用原因）。
      **未完成：仓库未引入 jsdom/RTL，仅有 store/项目序列化测试与真实 GUI 验收。**
- 验收：UI 无占位按钮；无 GPU 时显示 CPU 模式；RAW 解析失败不显示假元数据。

## T9 — 全量测试与构建
- [x] T9.1 `cargo test`、`cargo clippy --all-targets -- -D warnings`。
- [x] T9.2 `npm run typecheck`、`npm run test`、`npm run build`、`npm run app:build`。
- [x] T9.3 保存日志到 `docs/assets/round7/`；记录 rustc/node/依赖版本。
- 验收：全绿；修复新引入回归，不删除旧测试。

## T10 — 文档与提交
- [x] T10.1 七份 ROUND7 文档 + 三份 ROUND6 文档 + `THIRD_PARTY_LICENSES.md` 更新。
- [x] T10.2 文档内部数据与日志一致，未完成项显式标注。
- [ ] T10.3 `git add` 后提交（说明包含 RAW/ICC/GPU 与 6.1 收尾）。
- 验收：提交完成，工作区干净；停止，不进入第八轮。
