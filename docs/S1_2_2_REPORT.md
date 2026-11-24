# LUMI RETOUCH · S1.2.2 人像精修方块/模糊/色块/接缝伪影专项报告

**基线：** S1.2 结束（`9106c48`，工作区 clean）
**首要回归样本：** `/Users/lu/Desktop/微信图片_2026-11-22_210852_641.png`（3744×5616）
及其问题导出 `…-lumi.jpg`（2026-11-24 09:18，S1.2 构建产生）
**日期：** 2026-11-24　**范围：** 只解决伪影；不进 S2、不改 UI、不动瘦脸产品参数、不靠降强度掩盖。

---

## 1. 伪影根因报告（先定位，后修改）

### 1.1 定位方法

1. **逐阶段输出**（15 张，同一照片/参数/坐标）：`docs/assets/s1_2_2/user_wechat/`
   `01_original → 02/03/04 三种蒙版 → 05/06 频率低/高 → C 仅 even → 08 重组 →
   09/10/11(2/3) 各尺度网格 → 12 金字塔 → 13 纹理保护 → 14 最终融合 → G 美肤引擎 → H 完整一键`。
2. **消融 A–H**（含带/不带 reshape 的专项对照）。
3. **伪影检测器**（`detect_artifacts`，均以原图同位置为基线）：ROI 边缘差异、横/纵新边缘、
   纹理损失、网格周期自相关、64px 相邻块台阶、模糊块比例、保护边缘比。
4. **文本字符热力图**（|Δ| 分箱）：矩形块结构一目了然（read_image 工具本轮持续串图，
   目视改由字符图 + 数值完成，见 §11 观察项）。

### 1.2 根因

**根因：`face_reshape::warp::warp_mesh` 反向映射从「正在写回的同一缓冲」采样（自污染）。**

```rust
// 修复前（warp.rs）
let warped = sample_bilinear(image, …);   // image 同时是写目标！
let original = image.get_pixel(x, y).0;   // original 也可能已被前序三角写脏
```

- 行序自上而下：目标像素的逆映射采样点若落在**已写入的行** → 读到变形像素 →
  **梳状条纹（comb）**；
- 三角形按序处理：后一个三角的采样点落入前一个三角的**目标区** → 读到被别人写过的像素
  → **鬼影/重影（眼睛双重影像）**；
- 两种反馈叠加在三角边界处形成不连续 → **矩形阶梯状色块**；
- 手动笔触路径 `operations::manual::apply` 一直使用 pristine 的 `original_roi`，唯独
  `warp_mesh` 漏了快照——这是确凿的实现缺陷，而非调参问题。

**为什么 S1.2 一键必现：** 一键精修固定写入瘦脸计划（`plan_face_slim`），本照片全分辨率
下位移量大（安全阀降至 56% 仍显著），自污染被放大；此前小图/小位移下不明显。

### 1.3 阶段判定（回答“哪一步首次出现”）

| 问题 | 结论 | 证据 |
|---|---|---|
| 首次出现方块？ | **H（完整一键）中的 face_reshape**；A–G 全程无 | 消融：`no_reshape new_step=0.0006`（干净）；`only_reshape=0.0122`（最重）；14/08 差异图无矩形结构 |
| 首次出现矩形边界？ | 同上（warp 三角边界 × 自污染） | 字符热力图：lumi 11.33% 像素 \|d\|>27 呈 @@@ 矩形簇 |
| 首次出现局部模糊？ | 无独立“突然变糊”阶段；H 的梳纹是反馈伪影 | 各阶段 blurTiles 无阶跃异常 |
| 首次出现色彩断层？ | 无（色彩断层即方块的亮度台阶，同源） | 08/09/12/13/14 无色块 |
| 造成毛孔丢失？ | 正常平滑范围（beauty-only HF keep≈0.65–0.74），非伪影源 | 分解实验数据（S1.2） |

**网格彻底排除：** §4.2 实验24 组（点/三线性 Splat × σ_s{4,8,14} × σ_r{0.08,0.15} ×
levels{0,3}）全部 **blocks=0**；三线性 Splat 与点 Splat 的 HF keep 差 ≤0.001 且慢 5–10%
→ **正式算法保持单节点 Splat**，三线性仅存于实验分支 `bilateral_grid_smooth_splat_experiment`
（未经质量验证不替换，符合 §八）。

### 1.4 顺带发现并修复的两个 §四 强制项

| 项 | 发现 | 修复 |
|---|---|---|
| §4.1 `even_low` 无 Halo | 直接对裁剪 ROI `box_mean`，与 S1.1 Halo 设计不一致（ROI 边缘窗口截断） | 新增 `even_low_frequency_padded`：在 split 的 padded 低频上做盒滤（真实 Halo），引擎已接入；ROI 版 `even_low_frequency_wh` 同步改为边缘复制外扩，消除截断。测试：内部/边缘与全图参考一致 <1e-4；贴角 ROI 与边缘复制策略一致；旧截断路径误差 >10× |
| §4.3 保护蒙版硬跳变 | 连续性测试当场测得 **max_jump=0.91**：小半径椭圆（鼻/眉 rv≈0.12）1px 跨完 smoothstep 区间；`edge_keep` 基于锐利亮度近似二值 | feature 图两遍3×3（摊开椭圆边界、内部保护不变）+ `edge_keep` 改用3×3 模糊亮度 + smoothstep(0.08,0.30)。**max_jump<0.35**（测试锁定），睫毛保护深度保留（S1.1 keep 回到0.59–0.63） |
| §4.4 融合偏移/接缝 | 增加恒等往返测试（ROI 内逐位等于原图=无一像素偏移）与半权重带连续性测试 | 均通过；检测器 ROI 边缘比 0.63–0.98（<1，无边缘突变） |

---

## 2. 修复前后对比（问题照片）

### 2.1 数值（伪影带 (980,950)-(2100,2300)，以原图为基线）

| 指标 | 修复前（lumi 导出） | 修复后（H 复现） |
|---|---|---|
| \|d\|>27 像素占比 | **11.33%** | **2.75%** |
| \|d\|>40 像素占比 | **5.31%** | **0.51%**（↓10×） |
| 字符热力图 | 大片 `@@@@` 矩形簇 | 无 @ 块，仅平滑梯度 |
| 消融 new_step（带内） | lumi0.0080 / 仅瘦脸0.0122 | 无瘦脸0.0006；修复后 H steps 268→60 |
| lumi ↔ 修复后 H 全图 mean\|Δ\| | — | 1.10（同一管线，仅伪影差异） |

### 2.2 修复前后中间图

```
docs/assets/s1_2_2/user_wechat/           ← 修复后（全部15阶段+消融A–H+差异图+detect.json）
  01_original.jpg … 14_after_final_fusion.jpg
  G_full_beauty_engine.jpg / H_full_oneclick.jpg + *_diff.png
  Abl_no_reshape.jpg / Abl_only_reshape.jpg / C_ablation_even_only.jpg / D_ablation_grid_level0.jpg
  stages.json / detect.json
docs/assets/s1_2_2/grid_experiment.json   ← §4.224组实验
docs/assets/s1_2_prefix/{stem}/           ← 5张验收照片的“修复前”输出备份
docs/assets/s1_2/{stem}/                  ← 同5张的“修复后”输出（回归重跑）
（问题照片“修复前”原件：桌面 …-lumi.jpg，不入库）
```

字符热力图（伪影带，` .:-=+*#%@` = |Δ| 递增）：

- 修复前 lumi：多行 `@@@@@@@@` 矩形簇（如第1–5行、第17–25行成片）。
- 修复后 H：无 `@`；左上为平滑位移/提亮梯度（有机形态）。

---

## 3. 关键代码修改

```
src-tauri/src/face_reshape/warp.rs           ★根因：warp_mesh 增加 pristine source 快照参数，
                                              采样与 original 均取自快照；src_pts 遮蔽更名；
                                              新增回归测试 warp_samples_pristine_snapshot_without_feedback
src-tauri/src/face_reshape/mod.rs            每个候选变形前 image.clone() 作快照传入
src-tauri/src/ai_engine/retouch/frequency.rs §4.1 even_low_frequency_padded（真实 Halo）；
                                              even_low_frequency_wh 边缘复制外扩（去截断）；
                                              3个新一致性测试
src-tauri/src/ai_engine/retouch/skin_engine.rs split_planes 在 padded 低频上执行 even 后再裁回；
                                              移除对裁剪 ROI 的独立 even 调用
src-tauri/src/ai_engine/retouch/texture_protect.rs §4.3 feature 两遍3×3 + 模糊亮度 edge_keep
                                              + smoothstep；连续性测试（max_jump<0.35）
src-tauri/src/ai_engine/retouch/bilateral_grid.rs §4.2 SplatMode 实验分支（Point 正式/Bilinear 实验）
                                              + bilateral_grid_smooth_splat_experiment 入口；
                                              GridDebugCapture 逐尺度捕获（只读，不改行为）
src-tauri/src/ai_engine/retouch/fusion.rs    §4.4 恒等往返无偏移测试 + 半权重带连续性测试
src-tauri/tests/s1_2_2_artifact_localize.rs  新：dump/localize/gridexp 三阶段定位与实验
scripts/s1_2_2_artifact.sh                   新：四阶段驱动（缺照片 panic，三态）
```

未改：UI、瘦脸参数/计划逻辑（只修采样正确性）、双边网格正式 Splat、默认强度。

---

## 4. 真实照片回归

| 样本 | 结果 |
|---|---|
| **问题照片**（首要） | 伪影带 \|d\|>40：5.31%→0.51%；字符图无矩形块；H 全链断言通过 |
| 痘印 DSCF0030/0031 | s1_1 + s1_2 断言通过；skin_keep 0.60/0.59 |
| 粗糙 DSCF0053/0054 | 通过；keep 0.60/0.63 |
| 肤色不均 DSCF0029 | 通过；keep 0.60 |
| 正常肤质 DSCF0057/0055 | 通过；keep 0.62 |
| 多人 group（4脸） | s1_2 通过；四脸 fused≈16万/脸 |
| S1.1 真图套件 | **7/7 PASS**（`--ignored` 显式执行） |
| S1.2 五类诊断 | **5/5 PASS**（三档 Δ≈2×、非提亮分解断言保持） |

导出物：每张含原图/修复前（`s1_2_prefix`）/修复后（`s1_2`）/compare/tier_compare/
眼·眉·鼻翼·唇 ×100%/200% 放大；问题照片另含15 阶段图与 A–H 消融、差异热力图。

**产品质量（§七）自查：** 方块/网格边界——修复后字符图与检测器均无；局部突糊——无独立
阶段、blurTiles 无阶跃；色块/光晕——protect 连续性 <0.35、边缘比<1；纹理保留——S1.1
keep0.59–0.63；三档差异——Δ2.0–2.1× 保持；**未通过关闭美肤或降强度实现**（参数与 S1.2
完全一致，只改采样正确性）；预览/导出共用 `render_with_report`（静态确认，见 §11）。

---

## 5. 自动化测试结果

| 套件 | 结果 |
|---|---|
| `cargo test`（全量） | **29 套件全部 ok，0 failed**（lib 含 warp 回归/连续性/Halo/融合新测试） |
| retouch 相关 lib | **53 passed**（+warp 自污染回归、protect 连续性、even Halo×3、fusion×2） |
| `s1_2_diagnostics phase_render` | 5/5 通过 |
| `s1_2_2` dump→桥→localize→gridexp | 4/4 阶段通过（缺照片会 panic，三态） |
| `s1_1_real_photos --ignored` | 7/7 通过 |
| `s1_1_performance --release --ignored` | 通过 |
| `pnpm test` | 135 passed + 1 skipped（桥无环境变量不冒充通过） |
| `pnpm exec tsc --noEmit` | 0 error |

---

## 6. 性能对比（release，45MP）

| 指标 | S1.2 基线 | S1.2.2 修复后 |
|---|---|---|
| 总耗时 | 2.50 s | **2.49 s**（持平） |
| 峰值 RSS | 2.25 GiB | **1.65 GiB**（更低，分配器波动内） |
| 网格缓冲 | 3.4 MiB | 3.4 MiB |
| Splat/Blur/Slice | 8/28/1237 ms | 6/26/1218 ms |

额外成本：每张脸变形前 +1 次全图快照（45MP≈168MB 瞬时）；protect 模糊与 even padded 盒滤
在 O(N) 内，未进入耗时瓶颈。

---

## 7. Git

（提交后回填）

---

## 8. 工作区状态

提交后 clean；`docs/assets/`、`beauty-validation/`、桌面问题照片按 gitignore/隐私不入库。

---

## 9. 观察项 / BLOCKED

| 项 | 状态 |
|---|---|
| read_image 工具本轮持续返回缓存旧图 | **BLOCKED（工具故障）**：修复前后目视对比改用字符热力图 + 数值（§2）完成；此前成功读取的三联图曾确认修复前伪影形态。建议人工按 §2.2 路径自行目视复核 |
| 检测器 gridCorrH/V 自相关 | 弱指标（平滑差分场天然高相关），已由 blockSteps/new_steps/字符图替代；保留输出供后续校准 |
| 预览与导出一致性 | 二者共用 `render_with_report`（代码审查）；本轮未在运行中的 UI 内做像素级双通道比对 |
| `protected_leak=0.5`（部分样本） | 与 S1.1/S1.2 相同的0.5级提示，非阻断 |

**本轮只解决伪影问题。修复完成立即停止，不进入 S2，等待用户人工验收。**
