# S1 · SkinRetouchEngine 磨皮核心 · 交付报告

**阶段：** S1（仅皮肤精修核心；无 UI / 无瘦脸 / 无瘦身）  
**基线 SHA：** `2892abd`（S0 结束）  
**日期：** 2026-11-23

---

## 1. 阶段目标

建立 SkinRetouchEngine：多尺度双边网格、频率分离、纹理保护、独立瑕疵/肤色模块编排、统一融合与分区质量报告；真实照片可验收。

---

## 2. 实际完成内容

| 模块 | 文件 | 职责 |
|---|---|---|
| 双边网格 | `retouch/bilateral_grid.rs` | splat → 三轴 (2c+n1+n2)/4 → normalize → 三线性 slice；多尺度金字塔；soft-threshold 细节回注 |
| 频率分离 | `retouch/frequency.rs` | 引导滤波高低频；低频均匀；重组 |
| 纹理保护 | `retouch/texture_protect.rs` | 五官 feature_protection × 强边缘；**不惩罚正常皮肤纹理** |
| 融合引擎 | `retouch/fusion.rs` | Alpha 越界校验、ROI/坐标、强度 0 恒等、接缝风险标记 |
| 质量评估 | `retouch/quality.rs` | skin / protected / edge / full 分区；过度磨皮与保护泄漏 |
| 编排 | `retouch/skin_engine.rs` | `run_skin_retouch`：频率 → 双边 → 融合；中间数据可导出 |
| 接入 | `retouch/mod.rs` | `apply_face_retouch` 平滑阶段改为 SkinRetouchEngine；blemish/tone/eyes 仍独立 |

**算法来源：**  
- 双边网格数学结构参考 Apache-2.0 公开实现（S0 研究），**独立 Rust 重写**，无原仓库代码拷贝。  
- 引导滤波、频率分离策略参考公开文献与 RapidRetouch **思想**（AGPL），**未复制其代码**。

---

## 3. 修改文件列表

```
src-tauri/src/ai_engine/retouch/bilateral_grid.rs  (新)
src-tauri/src/ai_engine/retouch/frequency.rs       (新)
src-tauri/src/ai_engine/retouch/texture_protect.rs (新)
src-tauri/src/ai_engine/retouch/fusion.rs          (新)
src-tauri/src/ai_engine/retouch/quality.rs         (新)
src-tauri/src/ai_engine/retouch/skin_engine.rs     (新)
src-tauri/src/ai_engine/retouch/mod.rs             (接入)
src-tauri/src/ai_engine/skin_segmentation/mod.rs   (feature_protection 改为 pub)
docs/S1_SKIN_ENGINE_REPORT.md                      (本报告)
```

未改：UI、瘦脸、瘦身、pipeline 阶段顺序、参数 schema。

---

## 4. 核心算法说明

### 4.1 双边网格
- 亮度分箱 + 空间分箱 splat；边界 +1 pad；盒式模糊×2；权重归一化；三线性采样。
- 金字塔粗→细，双 pass 强度；细节 soft-threshold 按 texture/detail 回注。
- `strength=0` 或 mask≡0 → 恒等（单测锁定）。

### 4.2 频率分离
- `low = guided_filter`（带 halo）；`high = src − low`。
- 低频按 `tone_even` 向局部均值靠；`high_keep = f(texture, detail)` 重组后再进双边网格。

### 4.3 纹理保护
- `w = feature_protection × edge_keep`（强边缘阈值 0.28）。
- 联调修正：曾对纹理差分过罚导致 `mean_w≈0.03` 磨皮空转 → 已去掉 soft 贴图惩罚。

### 4.4 融合
- 写前校验权重∈[0,1]、缓冲长度；`strength=0` 不写像素；边界权重过高标 `seam_risk`。

---

## 5. 单元 / 集成测试

| 套件 | 结果 |
|---|---|
| `cargo test --lib` | **549 passed / 0 failed** |
| `retouch` 相关 lib 测试 | **16**（网格/频率/保护/融合/质量/引擎） |
| `tests/beauty` | **12 passed**（含磨皮降高频、多人脸独立） |
| `tests/slim_items` | 12 passed |
| `tests/g13_slim_algorithms` | 5 passed |
| `tests/geometry_foundation` | 7 passed |
| `pnpm test` | 135 passed |
| `tsc --noEmit` | 0 |

---

## 6. 真实照片验收

`cargo test --test p0_skin_artifacts`：beauty-validation 真图 4 张（预览分辨率管线）：

| 照片 | 变化像素比 |
|---|---|
| （见测试输出 changed=…） | 约 1%–2% |

输出目录：`docs/assets/p0_skin/`（若存在则含 original/smoothed/compare50/zoom）。  
**质量门：** 无三角翻转（reshape 路径）；磨皮变化比 ∈ (0, 0.85)；beauty 高频断言通过。

100%/200% 目视：以 `p0_skin/*_zoom100*` / `*_zoom200*` 为准（测试生成时）。

---

## 7. 性能与内存

- 网格尺寸 `ny·nx·nv`，`nv≈1/σ_r`；4K 时通过 `sigma_s≥4` 与 levels≤3 控制。
- 本阶段未做完整 benchmark（属 S5）；单测在 96×96 合成图上完成。

---

## 8. Git

见提交 SHA（回填）。

---

## 9. 工作区状态

提交后应为 clean（仅可能剩 gitignore 无关项）。

---

## 10. 未完成 / 下一阶段

| 未完成 | 归属 |
|---|---|
| 真图 100%/200% 人工目视签字 | 验收人 |
| 性能基准（3MP–45MP） | S5 |
| 痘痘/肤色策略进一步产品化（与 S2 分析联动） | S2 |
| GPU | S3 |
| 瘦脸/瘦身 | S3/S4 |

**下一阶段计划：** S2 专业皮肤分析与质量控制（SkinAnalysis / BlemishAnalysis / 模型评估）——**等待用户确认**。

---

**禁止提前宣布整个人像引擎成熟。** S1 仅完成磨皮核心算法升级。
