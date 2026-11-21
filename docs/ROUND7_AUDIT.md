# ROUND 7 — 审计与偏差说明

- 审计对象：第七轮增量（基于 `937b304`）
- 结论：核心链路有真实实现与测试；下列偏差均已在任务清单与报告中标注，不以标注代替实现。

## 1. 与 spec 的偏差

| spec 项 | 实际实现 | 影响 | 标注位置 |
|---|---|---|---|
| §4.2 RAW 在“线性 ProPhoto RGB”中显影 | 在**线性 sRGB** 中显影，输出 sRGB 编码 | 与 spec 的色彩架构不同；不影响 RAW 读取与参数真实性，但高光/色域策略与 ProPhoto 中间层不同 | `ROUND7_COLOR_PIPELINE.md §1` |
| §3.4 RAW 缺失提示重定位 | 项目保留 `source/raw` 参数，但无重定位 UI | 用户需手动重新导入；不会静默低清导出 | `tasks.md T5.4` |
| §5 GPU 替换主编辑管线曝光/曲线 | 仅实现独立内核、状态、基准、回退；主编辑管线纯 CPU | 主预览/导出未获得 GPU 加速；性能面板不声称已加速 | `ROUND7_GPU_BENCHMARK.md §4` |
| §7 GPU P95 ΔE00 / NaN/Inf | 逐通道 <0.01 一致性；未输出 P95，未注入 NaN/Inf | 无法给出 P95 指标；Image16 输入本身有限，NaN/Inf 不在正常路径 | `tasks.md T7.4` |
| §4.2 显示 Profile 转换预览 | 主画布保持 sRGB；新增手动“显示器 Profile 预览”并内嵌显示 ICC | 主画布未按显示器 Profile 重转；避免 WebView 二次转换 | `ROUND7_COLOR_PIPELINE.md §6` |
| §4.1 输入转换 | 已接入 JPEG/PNG 解码；损坏/非 RGB Profile 静默回退 sRGB | 降级原因未回传 UI | `ROUND7_COLOR_PIPELINE.md §8` |
| §7 Windows 构建 | 未执行 | 无 Windows 实机 | `ROUND7_DEPENDENCY_AUDIT.md §4` |
| §3.1 LibRaw 源码与重链接说明 | 未随包提供 | 发行合规缺口 | `ROUND7_DEPENDENCY_AUDIT.md §2` |

## 2. 风险与质量

1. **RAW 去马赛克**：MHC 5×5 已用平坦场与相位测试校验；缺少 AHD，极高对比边缘可能有伪影。
2. **X-Trans**：仅合成测试，真实 RAF 未验证；质量等级明确低于 Bayer。
3. **有损 DNG**：明确报错，不会返回假像素。
4. **ICC 输入回退**：损坏 ICC 时按 sRGB 处理，UI 只显示通用“按 sRGB”说明。
5. **显示器预览**：内嵌显示 ICC 的 PNG 在 WebView 中是否完全按 Profile 渲染，未做像素级截图验证；
   只验证了命令成功与 PNG 可生成。
6. **批量 RAW**：逐张全尺寸显影，内存受控，但比栅格慢；未单独做 RAW 批量内存基准。
7. **clippy**：为通过 `-D warnings`，在 crate 级显式 allow 了 10 类 Rust 1.95 新增风格 lint
   （`needless_range_loop`、`too_many_arguments` 等），未改写既有算法代码。
8. **release 测试**：`panic = "abort"` 使 release 测试二进制不可用；测试/基准均为 dev。
9. **GPU 24MP**：1.25× 刚好达到门槛，余量很小；大图收益有限。

## 3. 未引入的内容

- 未引入任何新模型权重。
- 未上传照片、未调用云服务；RAW/ICC/GPU 全部本地处理。
- 未修改第六轮的编辑算法语义；新增能力通过命令边界与独立模块接入。
- 未把 LibRaw 的 JPEG 预览当作 RAW 显影结果；编辑/导出走真实 `develop`。

## 4. 证据索引

| 证据 | 路径 |
|---|---|
| 后端正则测试 | `docs/assets/round7/cargo-test.log` |
| clippy | `docs/assets/round7/cargo-clippy.log` |
| GPU 基准 | `docs/assets/round7/gpu-benchmark.log`、`docs/ROUND7_GPU_BENCHMARK.md` |
| 色卡 ΔE00 | `docs/assets/round7/color-delta-e.log` |
| 批量内存 | `docs/assets/round7/memory-bench-50.log`、`-100.log` |
| 打包日志 | `docs/assets/round7/app-build.log` |
| GUI 截图 | `docs/assets/round6-gui/`、`docs/assets/round7/`（最终启动截图） |
