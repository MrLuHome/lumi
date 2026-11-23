# S1.1 · 磨皮核心算法审查修复 · 交付报告

**阶段：** S1.1（仅算法修复与重新验收；无 S2 / 无新模型 / 无 UI / 无 GPU / 无新功能）
**基线 SHA：** `71e6d7b6e8fcaf0038d49138ce3b52e3cd183edd`（S1 提交；其后 `eb7ad0d` 仅为 S1 报告回填）
**日期：** 2026-11-23

---

## 1. 修改文件列表

```
src-tauri/src/ai_engine/retouch/bilateral_grid.rs   (P0 trilinear/蒙版Splat；P1 采样与内存)
src-tauri/src/ai_engine/retouch/quality.rs          (P0 质量评估器重写)
src-tauri/src/ai_engine/retouch/skin_engine.rs      (P1 频率分离真实 Halo；质量调用点修正)
src-tauri/src/ai_engine/retouch/frequency.rs        (P1 新增 Halo 与全图一致性锁定测试)
src-tauri/src/ai_engine/retouch/filter.rs           (P1 新增边缘复制（非黑填充）锁定测试)
src-tauri/tests/s1_1_performance.rs                 (新：3/12/24/45MP 性能内存测试)
src-tauri/tests/s1_1_real_photos.rs                 (新：真实照片重新验收 + 对比/放大图导出)
docs/S1_1_REPORT.md                                 (本报告)
```

未改：UI、S2 分析、GPU、参数 schema、管线阶段顺序、`texture_protect` / `fusion` 逻辑（仅被调用方传参修正）。

---

## 2. P0/P1 修复前后逻辑

### 2.1 P0 · 双边网格三线性插值（`bilateral_grid.rs::trilinear`）

| | 修复前 | 修复后 |
|---|---|---|
| 权重 | 只累加 `wgt>0.5` 角点的 `acc×w`，**不除以权重和** → 部分角点无效时结果按比例变暗 | 计算全部有效角点的插值权重；RGB 加权和 ÷ 有效权重和（归一化） |
| 失败判据 | `r+g+b==0` 判失败 → **合法黑色像素被误判** | 黑色节点（权重有效、颜色 0）正常返回黑色；失败只看有效权重/节点存在性 |
| 回退 | 固定返回 `(x0,y0,z0)` 颜色（可能本身无效） | 权重和过小但角点有有效节点 → 取最大权有效角点；无有效角点 → Chebyshev 外壳 r=1..3 最近有效节点；仍无 → `None`，slice 保持输入像素 |
| 边界/防护 | 无 NaN 防护 | 非有限坐标 → `None`；索引 clamp 到 `[0,n-1]`；nx/ny/nv=0 → `None` |
| 连续性 | 权重和随节点有效性跳变 | 归一化权重在采样坐标上连续；节点有效性固定，边界采样无亮度断层（测试锁定） |

### 2.2 P0 · 皮肤蒙版参与 Splat（`grid_pass` / `bilateral_grid_smooth`）

| | 修复前 | 修复后 |
|---|---|---|
| Splat | 不接收 mask；每像素 `weight+=1`，蒙版外像素以**完整权重**进网格，blur 后污染皮肤节点 | 每像素按蒙版置信度 `m` 参与：`Σc·m` 与 `Σm` **分离累加**；`m≤1e-4` 跳过；蒙版外像素不进网格 |
| Blur | 4 平面同步模糊（本身线性） | 不变；颜色累加与权重累加同步传播 → 稀疏网格下权重正确（被 mask-Splat + 归一化测试锁定） |
| Normalize | 无效节点写颜色 0（依赖标志位才不致误用） | `w>1e-6`：颜色÷w、flag=1；否则 **flag=0（无效≠有效黑色）** |
| Slice | 聚合不归一化 + `RGB和==0` 回退 | 只聚合有效节点并按权重和归一化；近零权重走最近有效节点；`None` 保持输入 |
| 粗层蒙版 | 各尺度无蒙版 | mask 与 RGB 同一覆盖模型做面积降采样后参与该尺度 Splat |
| 最终融合 | 细节回注 `blend=m×strength`；fuse 用 process_w | **不变**：仍用羽化皮肤蒙版 × 保护权重（`process_w`）做细节回注与 `fusion::fuse`；mask 全 0 提前恒等返回 |
| 禁止黑填充 | — | 不把蒙版外像素填黑后参与滤波（蒙版外直接不 Splat） |

### 2.3 P1 · 多尺度降采样（`downsample_rgb` / 上采样 / 融合）

| | 修复前 | 修复后 |
|---|---|---|
| 降采样 | 单点最近邻（中心 floor 取样）→ 混叠/摩尔纹 | **面积平均**（源区域按重叠加权），正确处理非整数缩放比；测试：常量保持、DC 不漂移、棋盘→0.5（低通） |
| 上采样 | 双线性 | **Catmull-Rom 双三次**（权重分区和=1 → 常量/DC 保持；结果 clamp 0..1 防光晕） |
| 融合权重 | 注释与代码矛盾，`level_images` 每尺度各存一份全分辨率 | 权重显式固定：`level0 w=1`；粗层 `L`：`w=0.35/(levels−L+1)`（最粗 0.35…最细粗层 0.35/levels）；`combined=Σw·up(I_i)/Σw`；每尺度输入=原图面积降采样，处理顺序=最细入累加器 → 粗到细依次融合，小图随算随弃 |
| 实验证明 | — | `multi_scale_beats_single_scale_on_large_scale_unevenness`：同参数下 levels=3 的大尺度不均匀（块均值 σ）显著低于 levels=0，且高频保留 ≥ 单尺度的 80% → **保留金字塔混合优于单尺度** |
| 尺度一致性 | — | 常量图任意层数融合后仍为常量（`multi_scale_fusion_weights_documented_and_dc_preserved`） |

### 2.4 P0 · 质量评估器（`quality.rs`）

| | 修复前 | 修复后 |
|---|---|---|
| 高频计算 | 含**恒等于零**的 `x−min(x,x)` 项；仅水平邻差 | 水平+垂直双向梯度 `( |Ix|+|Iy| )/2` 三通道平均（边界回退可用方向） |
| 分区 | 硬阈值 membership；`skin_w` 传 `process_w` → protect=0 的五官区被排除，保护泄漏永远测不到 | **软权重**：skin=`skin_w`；protected=`skin_w×(1−allow)`；edge=`4s(1−s)`；full=1；`skin_engine` 改传 `mask.alpha` |
| 多尺度 | 无 | 2× 盒式降采样后的同型梯度指标（`high_freq2_*` / `texture_keep_ratio2`） |
| 低频偏移 | 与纹理混在一起 | 新增 `mean_color_delta`（区域均值偏移，0..1） |
| 空区域 | 无处理 | `weight_sum≈0` → `pixels=0`、比率=1、差值=0，`summarize` 写明 note |
| NaN/Inf | 无防护 | 中心/邻居任一非有限 → 像素剔除；输出字段有限性防线；尺寸不匹配返回带 note 的默认报告 |
| 量纲 | 无说明 | 模块文档明确：梯度/差值 = 0..1 强度（每像素），比率为无量纲，附计算式与权重表 |

### 2.5 P1 · 频率分离真实 Halo（`skin_engine.rs::split_planes` 等）

| | 修复前 | 修复后 |
|---|---|---|
| 滤波域 | 对裁剪 ROI 直接 `guided_filter`（box 窗口在 ROI 边界截断 → 脸颊/额头/下巴矩形接缝） | 从**原图**按 mask 包围盒外扩 `pad=min(2r+4,40)≥2r` 读取**真实像素** Halo → 在含 Halo 平面上引导滤波 → 裁回 ROI → `high=roi−low` |
| 四边界 | — | 统一边缘复制（`RgbPlanes::from_image_padded`），**禁止 0/黑色填充**（`filter.rs` 测试锁定） |
| 验证 | — | 内部 ROI：与全图引导滤波裁剪一致（误差 <1e-4）；纯斜坡 ROI 无线性断层；角 ROI 渐趋势保持、常量通道不被黑填充 |

### 2.6 P1 · 高分辨率内存（`bilateral_grid_smooth` 结构）

- **前**：`original`+`current` 两份全图副本；每个尺度的上采样结果各存一份全分辨率（levels=3 → 4 份）；slice 再复制一份 → 45MP 量级可达数 GB 冗余。
- **后**：输入 `rgb` 兼作原图（细节回注同址先读后写）；**仅 1 份**全分辨率累加器 `combined`；粗层小图与 mask 随算随弃；上采样**直接融合进累加器**（不分配全分辨率临时缓冲）；slice 同址写回；blur 临时缓冲为网格尺寸（非图像尺寸）。

---

## 3. 新增测试列表

### 3.1 lib 单测（`cargo test --lib retouch`：**47 passed**，基线 16 → +31）

**bilateral_grid.rs（22，净增 18）**
- 原有：`zero_strength_is_identity` / `smooth_reduces_high_freq_but_keeps_dc` / `mask_zero_leaves_region_unchanged` / `wrong_mask_len_errors`
- 新增：
  1. `all_black_image_runs_without_anomaly`（全黑无异常）
  2. `constant_color_image_unchanged`（常量色不变，含近黑/白）
  3. `sparse_grid_does_not_darken`（稀疏网格不变暗）
  4. `trilinear_normalizes_valid_weights`（插值权重归一化）
  5. `trilinear_black_pixel_is_valid_not_failure`（合法黑色不误判）
  6. `trilinear_falls_back_to_nearest_valid_node`（最近有效节点回退）
  7. `trilinear_rejects_non_finite_coords`（NaN/Inf 防护）
  8. `mask_one_actively_smooths`（mask=1 正确处理）
  9. `black_background_does_not_darken_skin`（黑背景不使皮肤变暗）
  10. `white_background_does_not_brighten_skin`（白背景不使皮肤变亮）
  11. `saturated_red_background_does_not_bleed_into_skin`（高饱和红不污染）
  12. `eye_and_lip_regions_do_not_pollute_cheek`（眼/唇不污染脸颊）
  13. `distinct_luma_regions_keep_their_colors`（不同亮度区无颜色污染）
  14. `image_boundary_has_no_brightness_step`（图像边界无亮度断层）
  15. `multi_scale_fusion_weights_documented_and_dc_preserved`（融合权重+DC）
  16. `downsample_area_average_preserves_constant_and_dc`（面积平均/非整数比）
  17. `downsample_is_lowpass_not_nearest_alias`（低通无混叠）
  18. `multi_scale_beats_single_scale_on_large_scale_unevenness`（多尺度 vs 单尺度实验）

**quality.rs（10，替换原 2 个合成测试）**
1. `identical_before_after_is_perfect`（处理前后完全一致）
2. `detects_total_texture_loss`（高频全部消失）
3. `detects_low_freq_color_shift`（低频颜色偏移）
4. `detects_protected_region_modification`（保护区被修改）
5. `detects_mask_edge_discontinuity`（蒙版边缘断层）
6. `only_skin_changes_reports_skin_not_protected`（仅皮肤区域变化）
7. `vertical_stripes_detected_by_horizontal_gradient`（垂直条纹）
8. `horizontal_stripes_detected_by_vertical_gradient`（水平条纹）
9. `empty_region_and_invalid_input_are_explicit`（空区域/非法输入）
10. `nan_and_infinity_inputs_are_guarded`（NaN/Infinity 防护）

**skin_engine.rs（+3）**
- `split_planes_halo_matches_full_frame_guided_filter`
- `split_planes_no_rectangular_seam_on_ramp`
- `split_planes_corner_roi_edge_policy_consistent`

**frequency.rs（+1）**：`split_halo_matches_full_frame_guided_filter`
**filter.rs（+1）**：`padded_sampling_uses_edge_replication_not_black`

### 3.2 集成测试（默认 `ignored` = 未执行态）

- `tests/s1_1_performance.rs::highres_memory_and_performance_3_12_24_45mp`
- `tests/s1_1_real_photos.rs::s1_1_real_photo_reacceptance`

**三态约定（禁止缺照片假通过）：**
| 状态 | 表现 |
|---|---|
| 未执行 | 默认 `cargo test` 显示 **ignored** |
| 通过 | `-- --ignored` 且照片齐全、断言全过 → **ok** |
| 失败 | `-- --ignored` 且断言不过 → **FAILED**（消息含 `[FAIL] {stem}`） |
| 缺照片 | `-- --ignored` 且无照片 → **panic「缺少照片导致未执行」**，绝不 return 假绿 |

---

## 4. 完整测试结果

| 套件 | 结果 |
|---|---|
| `cargo test --lib` | **580 passed / 0 failed / 3 ignored**（3 ignored 为基线已有） |
| 其中 `retouch` 相关 | **47 passed / 0 failed** |
| `tests/beauty` | 12 passed |
| `tests/beauty_real` | 4 passed |
| `tests/beauty_skin_model` | 8 passed |
| `tests/engine` | 10 passed |
| `tests/p0_skin_artifacts` | 1 passed |
| `tests/slim_items` | 12 passed |
| `tests/g13_slim_algorithms` | 5 passed |
| `tests/g16_real_slim_acceptance` | 2 passed |
| `tests/geometry_foundation` | 7 passed |
| `tests/face_detection` / `face_intelligence` | 4 / 5 passed |
| `tests/model_registry` / `onnx_worker` | 18 / 7 passed |
| `tests/raw_real` / `reshape_*` | 2 / 3+3 passed |
| `tests/round8~10_*` | 11+5+5+9 passed（round8_benchmark 1 ignored 为基线已有） |
| `tests/s1_1_performance` | **1 ignored（未执行态，见 §6 显式运行结果）** |
| `tests/s1_1_real_photos` | **1 ignored（未执行态，见 §7 显式运行结果）** |
| `pnpm test` | **135 passed / 0 failed** |
| `pnpm exec tsc --noEmit` | **0 error** |

显式执行（照片齐全）：
- `cargo test --release --test s1_1_performance -- --ignored --nocapture` → **1 passed**
- `cargo test --test s1_1_real_photos -- --ignored --nocapture` → **1 passed（7 张照片）**

---

## 5. 真实照片前后对比图

输出目录：`docs/assets/s1_1/{stem}/`（gitignore 隐私素材，不进版本库）

照片（7 张，覆盖任务书要求的“原伪影照片 + 真实痘痘/肤色不均授权照片”）：

| 照片 | 组 | 变化像素比 | skin_keep | 备注 |
|---|---|---|---|---|
| DSCF0030 | 01-acne（痘痘） | 0.0111 | 0.693 | 授权验证语料 |
| DSCF0031 | 01-acne（痘痘） | 0.0109 | 0.688 | |
| DSCF0053 | 02-rough-tone（肤色不均） | 0.0095 | 0.694 | |
| DSCF0054 | 02-rough-tone | 0.0121 | 0.712 | |
| DSCF0055 | 03-good-skin | 0.0141 | 0.709 | |
| DSCF0029 | 04-skin-tone | 0.0087 | 0.690 | |
| DSCF0060 | 05-lighting | 0.0064 | 0.702 | |

每张导出：
- `original.png`（原图）
- `skin_mask.png`（皮肤蒙版）
- `protect_mask.png`（保护蒙版）
- `frequency_output.png`（频率分离输出）
- `bilateral_grid_output.png`（双边网格输出）
- `final.png`（最终融合输出）
- `compare.png`（原图 | 最终 并排）

自动断言（全部通过）：变化比 ∈ (0.0001, 0.85)；ROI 外像素零改动；`over_smooth_risk<1`；`protected_leak<1`；皮肤纹理保留率 ≥0.25；输出全有限值。

---

## 6. 100% / 200% 放大图

每张照片 × 每个区域：

```
docs/assets/s1_1/{stem}/zoom_{region}_{100|200}.png
```

- factor=1 → 100%（原像素），factor=2 → 200%（最近邻放大），左原图 | 右处理后。
- region ∈ `forehead`（额头）、`eye_left` / `eye_right`（眼睛周围）、`nose_wing_left` / `nose_wing_right`（鼻翼）、`lips`（嘴唇）、`cheek_left` / `cheek_right`（左右脸颊）、`chin`（下巴）、`skin_edge`（皮肤边缘）。
- 裁窗尺寸 = `clamp(眼距×2.4, 64, 160)`，随脸在画面中的大小自适应。

**100%/200% 目视要点（供人工验收）：** 皮肤过渡无矩形接缝；眼线/睫毛/唇线无涂抹；鼻翼无色块；脸颊纹理保留（skin_keep≈0.69）；无黑/白/红色污染。

---

## 7. 性能与内存数据

命令：`cargo test --release --test s1_1_performance -- --ignored --nocapture`
源图：`beauty-validation/01-acne/DSCF0030.JPG`（39MP 真实照片，内容缩放到目标分辨率）；管线 = 检测 → 皮肤蒙版 → 纹理保护 → 频率分离（Halo）→ 双边网格 → 质量 → 融合。

| 目标 | 尺寸 | ROI | detect | mask | protect | freq | grid | quality | fuse | **总耗时** |
|---|---|---|---|---|---|---|---|---|---|---|
| 3MP | 2000×1500 | 246×430 | 107ms | 216ms | 2ms | 7ms | 28ms | 4ms | 0ms | **366ms** |
| 12MP | 4000×3000 | 491×859 | 196ms | 71ms | 8ms | 28ms | 145ms | 19ms | 1ms | **474ms** |
| 24MP | 6000×4000 | 737×1289 | 335ms | 106ms | 20ms | 69ms | 530ms | 43ms | 3ms | **1111ms** |
| 45MP | 9000×5000 | 1099×1923 | 500ms | 193ms | 49ms | 177ms | 1522ms | 95ms | 7ms | **2553ms** |

**网格分阶段（Splat / Blur / Slice）与单网格缓冲：**

| 目标 | Splat | Blur(含normalize) | Slice | 单网格缓冲(max) | blur 临时缓冲 |
|---|---|---|---|---|---|
| 3MP | 0ms | 0ms | 11ms | 0.2 MiB | 0.2 MiB |
| 12MP | 0ms | 1ms | 87ms | 0.7 MiB | 0.7 MiB |
| 24MP | 4ms | 8ms | 398ms | 1.5 MiB | 1.5 MiB |
| 45MP | 8ms | 28ms | 1237ms | **3.4 MiB** | 3.4 MiB |

**每尺度耗时（downsample / process / blend）：**

| 目标 | L0(全分辨率) | L1 | L2 | L3(最粗) |
|---|---|---|---|---|
| 3MP | 0 / 10 / — ms | 0 / 3 / 3 | 0 / 1 / 3 | 0 / 0 / 4 |
| 12MP | 0 / 71 / — | 2 / 19 / 13 | 1 / 6 / 13 | 1 / 1 / 13 |
| 24MP | 0 / 311 / — | 5 / 81 / 31 | 3 / 23 / 30 | 2 / 4 / 32 |
| 45MP | 0 / 947 / — | 12 / 249 / 69 | 6 / 70 / 69 | 4 / 15 / 70 |

**总峰值内存（进程 RSS，含解码/缩放等全流程，逐目标单调累计）与 CPU：**

| 目标 | 峰值 RSS | CPU 时间 |
|---|---|---|
| 3MP | 0.99 GiB | 0.362 s |
| 12MP | 1.35 GiB | 0.472 s |
| 24MP | 1.72 GiB | 1.108 s |
| 45MP | **2.25 GiB** | **2.546 s** |

结论：
- 单网格缓冲最大 3.4 MiB（ROI 分箱，远低于 512 MiB 门限）；
- 45MP 峰值 2.25 GiB —— 单副本累加器优化后，未出现按尺度份数放大的内存增长；
- Slice 是网格主要耗时（三线性 ×2 pass ×多尺度），Splat/Blur 可忽略；
- 测试断言：峰值 <12 GiB、单网格缓冲 <512 MiB、管线非空转、输出全有限。
- 未通过缩小导出分辨率掩盖内存问题（导出仍为处理分辨率原样）。

---

## 8. 编译结果

- `cargo build --lib`（dev）：**成功**；剩余 2 条警告均为基线已有（`texture_protect.rs` 未用 `box_mean` 导入、闭包多余括号），非本轮引入。
- `cargo test`（dev，全部 lib+集成目标）：**编译并执行成功**。
- `cargo test --release`（性能目标）：**编译并执行成功**。
- `pnpm exec tsc --noEmit`：**0 error**。

---

## 9. Git

76c62eece8d9f1f39b4339321bcc7d14952d3eed

---

## 10. 工作区状态

提交后应为 clean；`beauty-validation/` 与 `docs/assets/`（含 `s1_1/` 对比图）按 `.gitignore` 不入库。

---

## 11. 未完成问题

| 未完成 | 归属 / 说明 |
|---|---|
| 100%/200% 真图人工目视签字 | 验收人（自动断言已全过，§6 路径） |
| `protected_leak=0.5` 提示（7 张照片均报“保护区域有可见变化”，mean_abs_delta≈0.02–0.04） | 非阻断（<1）；源于部分保护权重（allow∈(0,0.35)）仍按比例融合，属可解释行为；如需压到 0 可在后续阶段收紧保护映射 |
| 整应用级性能基准（RAW 解码、全管线） | S5（本轮仅磨皮核心管线基准） |
| 旧 `p0_skin_artifacts` 缺照片时仍 `return` 显示通过 | 历史测试，非本轮交付物；本轮新测试已按三态实现（如需可下轮一并改造） |
| GPU 加速 Slice（当前 CPU 瓶颈） | S3 |

**本轮只执行 S1.1，未进入 S2。修复完成，等待人工验收。**
