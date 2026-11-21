# ROUND 7 — RAW ENGINE

- 基线：`main` @ `937b304` + 第七轮增量
- 依赖：`libraw_rs_vendor 1.0.0`（LibRaw 0.21.1，静态链接）
- 原则：只读取原始像素与元数据，**不调用 `dcraw_process`**；预览与导出共用同一 `develop` 函数；
  原图永不被写入。

## 1. 模块

| 文件 | 职责 |
|---|---|
| `src-tauri/src/image_engine/raw/libraw_ffi.rs` | 唯一 `unsafe` 边界：打开/解包/内嵌预览/释放；返回码映射；尺寸与字节上限；`catch_unwind` |
| `raw/metadata.rs` | 品牌/型号/镜头/ISO/快门/光圈/焦距/时间/相机 WB；缺失值显式 `Unknown` |
| `raw/linear.rs` | `LinearImage`（f32 相机原生线性值，已减黑电平，保留 >1 高光与负值） |
| `raw/demosaic.rs` | Malvar-He-Cutler 5×5 Bayer 插值（RGGB/BGGR/GRBG/GBRG）与双线性 |
| `raw/xtrans.rs` | X-Trans 6×6 基础插值（质量等级低于 AHD，已在能力矩阵标注） |
| `raw/develop.rs` | 曝光/高光/阴影/相机 WB/自动 WB/色温/色调、相机矩阵 → 线性 sRGB、sRGB 编码 |
| `raw/mod.rs` | 门面、`RawDevelopParams`、`is_raw_path`（`tif/tiff` 明确排除） |

## 2. 处理链路

```text
LibRaw unpack（线性 CFA，单通道）
  → 黑电平减法 / 白电平归一化（1/white）
  → Bayer Malvar-He-Cutler 或 X-Trans 6×6 → 线性 RGB
  → 相机矩阵（行归一化，适配 D65）→ 线性 sRGB
  → 白平衡 / 曝光 / 高光 / 阴影
  → sRGB 编码 → Image16（编辑管线输入）
```

- 显影参数独立为 `RawDevelopParams`，不写入 `EditParams`，随项目 `raw.params` 保存。
- 预览：`raw_preview` 优先返回相机内嵌 JPEG（秒开），没有可用内嵌预览时按受控长边显影。
- 编辑预览：`render_preview` 接收 `rawParams`，按受控尺寸显影后进入现有 CPU 编辑管线。
- 导出：`export_image` / 批量导出对 RAW 做全尺寸显影，使用同一 `develop_from_path`，
  位深固定为 16-bit，输出路径使用原图路径（不覆盖原文件）。

## 3. 实测样本（CC0，仓库外缓存）

缓存目录：`~/Library/Caches/lumi-raw-samples`

| 文件 | 相机 | 结果 |
|---|---|---|
| `_MG_2233.CR3` | Canon EOS M200 | 可读，真实 CFA/WB/黑电平 |
| `_DSC0812.ARW` | Sony DSC-RX1 | 可读 |
| `P1000951.RW2` | Panasonic DC-G9 | 可读 |
| `E_1__C106743_gredos.ORF` | Olympus E-1 | 可读 |
| `_IGP2068.DNG` | Pentax K-01 | 可读（非压缩 DNG） |
| `5G4A9395-compressed-lossy.DNG` | Canon 5D Mark III DNG | **明确不支持**：LibRaw 0.21.1 无法解包有损压缩 DNG；测试中按“已知不支持”记录，不计为通过 |

补采 Nikon NEF 与 Fujifilm RAF：**未获取到可复现的 CC0 样本**，因此：

- NEF / RAF 代码路径存在（按扩展名与 CFA/X-Trans 分支），但**没有实机样本实测**；
- X-Trans 只有合成平坦场/边界测试（`xtrans_flat_field_is_reconstructed`），未做真实 RAF 验收；
- 报告中不把 NEF / RAF 标为“已验证”。

## 4. 测试与证据

| 测试 | 覆盖 |
|---|---|
| `raw_reads_real_sample`（`tests/raw_real.rs`） | 真实样本尺寸、CFA、黑电平、白电平、相机 WB、相机矩阵、内嵌预览 |
| `raw_preview_and_export_share_develop`（`tests/raw_real.rs`） | 预览尺寸显影与全尺寸显影在缩放到同尺寸后逐位一致；显影前后原图文件长度不变；曝光 +0.5 EV 改变像素 |
| `bayer_orders_reconstruct_flat_colors` | 四种 Bayer 排列平坦场重建 |
| `bayer_phase_is_preserved` / `unknown_pattern_is_rejected` | 相位保持与非法 CFA |
| `bilinear_runs_and_matches_flat_field` | 双线性基线 |
| `exposure_changes_pixels_and_never_touches_source` | 参数生效与源数据不变 |
| `camera_white_balance_normalizes_green` / `custom_temperature_direction_is_correct` | WB 方向 |
| `shadows_and_highlights_move_in_expected_direction` | 阴影/高光方向 |
| `extreme_params_are_sanitized` | NaN/Inf/越界参数 |
| `xtrans_flat_field_is_reconstructed` | X-Trans 合成平坦场 |

此外 `cargo test --lib` 覆盖 RAW 参数校验、项目 v3 往返、缺失字段兼容。

## 5. 已知限制（如实标注）

1. **有损压缩 DNG**：LibRaw 0.21.1 不支持，导入时返回明确错误，不显示假元数据。
2. **Nikon HE/HE\***：0.21.1 不支持。
3. **NEF / RAF**：无真实样本，未完成实机验收；X-Trans 质量等级为“基础”。
4. **去马赛克**：Bayer 使用 MHC 5×5；没有 AHD/AMaZE，极高对比细线可能出现拉链伪影。
5. **镜头校正/色差/暗角**：未实现。
6. **显示器色彩管理**：RAW 显影输出进入 sRGB 工作空间，显示器 Profile 预览对栅格输入启用；
   RAW 的显示器预览当前禁用并标注（需先显影为栅格），未计为完成。
7. **GPU**：RAW 显影的去马赛克与矩阵运算仍为 CPU；GPU 仅覆盖已验证的曝光/曲线内核，
   主编辑管线尚未接入（见 `ROUND7_GPU_BENCHMARK.md`）。

## 6. 原图不变

- 读取路径只使用 `open_file/unpack/open_thumb/close`，不写回源文件。
- 导出始终写入用户选择的输出目录；文件名冲突策略独立。
- `tests/raw_real.rs` 在显影前后比较文件长度，断言原图未被修改。
