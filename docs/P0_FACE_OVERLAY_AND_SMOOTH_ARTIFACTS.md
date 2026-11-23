# P0 · 人脸调试框泄漏与磨皮伪影修复报告

日期：2026-11-23 · 范围：仅两个 P0，无其它功能

---

## 问题一：正常预览中的调试图层

### 根因

| # | 根因 | 位置 |
|---|---|---|
| 1 | `showFaceOverlay` **默认 true**，检测完成后再强制 `true` | `editorStore.ts:699,1476,1492` |
| 2 | `CanvasStage` 无条件按该开关绘制 SVG 金框/黄点/`A · ID …` | `CanvasStage.tsx:567-607` |
| 3 | `showBodyAnchors` 默认 true，锚点也画在预览上 | `editorStore.ts:745` |
| 4 | 无开发者总开关 | 原先仅有 FacePanel 眼睛按钮 |

**图层类型**：DOM/SVG 叠加（`<svg class="face-overlay">`），**不是** Canvas/WebGL，**不进入** Rust 导出管线（`export_image` 只吃 params+路径）。导出 JPG/PNG/WebP 本就不含调试信息；泄漏只发生在预览 UI。

### 修复

1. `DEBUG_OVERLAYS_ENABLED`（`featureFlags.ts`）：`DEV` 或 `VITE_DEBUG_OVERLAYS=1` 或 `localStorage["lumi.debugOverlays"]="1"`。
2. `showFaceOverlay` / `showBodyAnchors` **默认 false**；检测完成不再强制打开。
3. `CanvasStage` 人脸框、网格、身体锚点均 `&& DEBUG_OVERLAYS_ENABLED`。
4. FacePanel 眼睛按钮在非调试模式下 `disabled`。

### 正常预览逻辑（后）

```
检测完成 → showFaceOverlay 保持 false → SVG 不渲染
开发者：localStorage.lumi.debugOverlays=1 或 DEV 构建 → 可手动打开
导出：始终无叠加层（未改导出路径）
```

---

## 问题二：磨皮多边形色块 / 横条 / 拉伸

### 根因（逐项对照任务书）

| # | 根因 | 位置 | 症状 |
|---|---|---|---|
| 1 | 蒙版羽化后 **硬切 `alpha<0.12→0`** | `skin_segmentation/mod.rs` finalize_mask | 多边形硬边、色块边界 |
| 2 | 多脸 **Voronoi 归属直接置 0**，无再羽化 | `apply_ownership` | 直线接缝 |
| 3 | **ROI 边界 box/guided 窗口截断**（无 halo） | `filter.rs` / `smoothing.rs` | 横向条纹、矩形接缝 |
| 4 | 滤波输入 **0 填充**（蒙版外/未激活像素） | `eyes.rs` plane、`tone.rs` LAB、`blemish.rs` luma | 条纹、错误均值、色偏 |
| 5 | 祛痘 **连通域平色填充 + 1px 硬边** | `blemish.rs` | 多边形色块 |

**已排除**：缓存双次融合（每次 render 重建蒙版）；导出混入调试图（无）。

### 修复（改算法，非降强度）

1. **finalize_mask**：弱响应软清除 `v≤0.05→0`，`0.05..0.16` 平滑，**不把高光置信度抬到 1**。
2. **apply_ownership**：切割后再 `box_blur_alpha` + 同款软清除。
3. **RgbPlanes::from_image_padded / crop_center**：滤波带 halo（边缘复制，禁止 0 填充）。
4. **smooth_skin**：在 padded 平面上做双尺度 guided，再裁回融合。
5. **eyes / tone / blemish**：先写满 ROI 真实采样再 `box_mean`；祛痘用到边界的 smoothstep 衰减 + 降低平色强度。

---

## 真图对比

目录：`docs/assets/p0_skin/`

| 照片 | 变化像素比 |
|---|---|
| DSCF0030 | 1.04% |
| DSCF0053 | 0.88% |
| DSCF0055 | 1.32% |
| DSCF0029 | 0.82% |

每张含：`_original` / `_smoothed` / `_compare50` / `_zoom100(_smoothed)` / `_zoom200(_smoothed)`。

---

## 测试与编译

| 命令 | 结果 |
|---|---|
| `cargo test --lib` | **534 passed / 0 failed** |
| `cargo test --test beauty` | 12 passed |
| `cargo test --test beauty_skin_model` | 8 passed |
| `cargo test --test slim_items` | 12 passed |
| `cargo test --test p0_skin_artifacts` | 1 passed（4 真图） |
| `pnpm test` | 135 passed |
| `tsc --noEmit` | 0 |
| `pnpm run app:build` | 成功（见构建日志） |

---

## Git

见回填 SHA。工作区状态见提交时 `git status`。

---

## 未解决 / 边界

1. **未改** 导出编码与色彩管线（本就无调试层）。
2. 磨皮在 **预览分辨率 1200** 上生成对比图；全尺寸导出共用同一 `pipeline::render`。
3. 未做大规模重构；未删除 AI 美肤/分割能力。
4. 真图「无色块」依赖人工目视 zoom 图；自动化仅锁变化比例与既有 beauty 断言。
5. 本地 ahead origin 的历史提交仍待 push（本轮独立 commit）。
