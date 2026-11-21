# ROUND 7 — ICC 色彩管理

- 依赖：`lcms2 6.3.0`（static，Little CMS，MIT）
- 原则：使用真实色彩空间转换，**不是只写元数据**；转换只发生一次，不做重复转换。
- 原始日志：`docs/assets/round7/color-delta-e.log`、`docs/assets/round7/cargo-test.log`

## 1. 架构

```text
输入（RAW 线性 / Raster + 内嵌 ICC）
  → 输入转换到 sRGB 工作空间（16-bit，lcms2）
  → 现有 CPU 编辑管线（美肤/塑形/调色/背景等，顺序不变）
  → 输出 Profile 转换（lcms2，16-bit，意图 + BPC）
  → 编码（JPEG / PNG / 16-bit TIFF）
  → 嵌入 ICC（JPEG APP2 / PNG iCCP / TIFF tag 34675）
```

- 工作空间：**sRGB 编码（IEC 61966-2-1）**，保持第六轮算法语义。
- 输出：sRGB / Adobe RGB (1998) / Display P3 / ProPhoto RGB / 用户 ICC。
- 内置 Profile 由 lcms2 构造并按 kind 进程内缓存，单张与批量导出嵌入字节一致。
- RAW 显影在 sRGB 工作空间内完成（曝光/高光/阴影/WB），不经过 ProPhoto 中间层；
  这是对 spec §4.2 的落地简化，已在 `ROUND7_RAW_ENGINE.md` 标注，不宣称使用 ProPhoto 中间层。

## 2. 输入

| 情况 | 行为 |
|---|---|
| JPEG/PNG 内嵌 RGB ICC | 解码后先用 lcms2 转到 sRGB，再进入编辑管线（`decoder.rs`） |
| 无 ICC | 按 sRGB 处理，色彩状态标注“未指定配置，按 sRGB” |
| 损坏/超长 ICC | 拒绝（长度 <128 或 >32 MB 直接报错；解析失败返回明确错误） |
| 非 RGB Profile（CMYK/灰度） | RGB 输入路径拒绝，不错误套用；CMYK 走专用转换 |
| ICC v2 / v4 | 均可读取（`v2_and_v4_profiles_are_both_accepted`） |

新增测试 `embedded_rgb_profile_is_converted_to_srgb_workspace`：把 ProPhoto Profile 写入 PNG，
解码结果与未标记 PNG 不同、Alpha 保留，证明输入转换真实发生。

## 3. 输出与嵌入

| 格式 | 转换 | 嵌入 |
|---|---|---|
| JPEG | 16-bit 转换后编码 | APP2，多段分割（`jpeg_app2_roundtrip_splits_large_profiles`） |
| PNG | 16-bit 转换后编码 | iCCP（zlib）（`png_iccp_roundtrip`） |
| TIFF | 自实现未压缩 RGB16 | tag 34675（`icc_tag_roundtrips`） |
| WebP | 转换后编码 | **不嵌入**（编码器限制，UI/文档标注） |

- 关闭“嵌入 ICC”只省略 Profile 标记，像素仍按目标空间转换。
- 导出路径拒绝与原图相同（`same_path`），原图永不被覆盖。

## 4. 实测数据

ColorChecker Classic 24 色块，sRGB → Adobe RGB → sRGB 往返：

```text
ColorChecker sRGB→AdobeRGB→sRGB：平均 ΔE00 0.009，最大 ΔE00 0.076
```

- 门槛：平均 <1.0、最大 <2.5；实测远低于门槛。
- `output_conversion_really_changes_pixels`：输出转换后像素值真实改变，`changed_pixels > 0`。
- `srgb_to_srgb_is_identity`：sRGB → sRGB 不产生漂移。
- `roundtrip_does_not_drift_repeatedly`：重复转换不累积偏移，验证不会二次转换。
- `soft_proof_flags_out_of_gamut_colors`：高饱和色在输出 Profile 下被标记超色域。

## 5. 软打样与色域警告

- `soft_proof`：真实执行 sRGB → 输出 Profile → sRGB 往返，逐像素计算 ΔE00。
- ΔE00 > 2.0 记为超色域，并在预览中用洋红覆盖。
- 返回超色域像素数、比例、最大 ΔE00；`ColorPanel` 已接入按钮与结果展示。
- 限制：RAW 软打样需先显影为栅格，当前对 RAW 禁用并显示原因。

## 6. 显示器 ColorSync

- `display.rs` 通过 `CGMainDisplayID` + `CGDisplayCopyColorSpace` + `CGColorSpaceCopyICCData`
  读取主显示器 ICC（raw FFI，全部释放）。
- 读取失败回退 sRGB，`DisplayStatus.reason` 给出原因；UI 提供手动刷新。
- `display_preview` 命令：渲染 → lcms2 转到显示器 Profile → 输出 PNG 并嵌入显示器 ICC，
  避免 WebView 再按 sRGB 二次转换。`ColorPanel` 已接入“显示器 Profile 预览”。
- 主画布仍保持 sRGB，由系统做显示管理；显示器转换预览需手动触发。
  因此 `ColorStatus.display_transform` 只在成功读取显示器 Profile 时为真，
  且不把“主画布已按显示器 Profile 转换”标为完成。
- 多显示器切换**未实现**事件监听，仅支持手动刷新（未验证项）。

## 7. CMYK

- `color_convert_cmyk`：读取 CMYK TIFF；优先文件内嵌 ICC，其次用户 ICC；
  有 Profile 时走 lcms2 真实转换，无 Profile 时用通用公式并在 `quality` 标注“基础质量”。
- 仅支持 8-bit CMYK TIFF；CMYK JPEG 未接入（标注未验证）。
- 质量等级：基础；不做黑场生成/总墨量控制。

## 8. 已知限制

1. RAW 的 ProPhoto 线性中间层未采用，直接在线性 sRGB 工作空间显影（不宣称已实现 ProPhoto 中间层）。
2. WebP 不嵌入 ICC。
3. CMYK 仅 TIFF 8-bit、基础质量。
4. 多显示器自动切换未监听。
5. 显示器 Profile 读取依赖 macOS ColorSync；Windows/Linux 未验证。
6. 输入 ICC 转换在解码阶段完成；若文件 ICC 损坏则按 sRGB 处理，但当前不把该降级原因回传到 UI
   （仅在色彩状态中标注“未指定配置，按 sRGB”）。
