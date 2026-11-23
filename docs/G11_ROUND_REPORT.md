# G11 · 几何变形底座 · 本轮报告

> 轨道：**G11–G16** 瘦脸瘦身补充（非 AI 底座 R11）  
> 范围：统一局部变形 API + 保护蒙版 + 坐标映射 + 多区域合并 + 设计评估  
> **不含** AI 自动瘦脸/瘦身算法（G13）、一键集成（G14）、手动 UI（G15）、真图验收（G16）

---

## 1. 交付物

### 1.1 文档

| 文件 | 内容 |
|---|---|
| `docs/G11_GEOMETRY_FOUNDATION.md` | Mesh Warp / 位移场 / TPS / 保边缘 / 多区域选型评估；保护层语义；G12–G16 接口预告；验收 A1–A9 |

### 1.2 代码（新增模块 `src-tauri/src/geometry/`）

| 文件 | 职责 |
|---|---|
| `mod.rs` | 出口、`GeometryFoundationCapability`（`ai_face_body_slim=false`） |
| `types.rs` | `ControlPointOp` / `LocalWarpOp` / `RegionBox` / `WarpStrength` |
| `coords.rs` | 归一化↔像素、`region_to_pixels`（浮点边界 epsilon） |
| `protection.rs` | `ProtectionMask` / `ProtectionLayer` / `compose`（min）/ `apply_to_delta` |
| `merge.rs` | `merge_ops`：区域并集、同点位移累加、保护采样、相对短边 12% clamp |
| `quality.rs` | `GeometryQualityReport` + `degrade_scale`（0.75）+ `stabilize_scale` |

### 1.3 测试

| 文件 | 例数 |
|---|---|
| 模块内 `mod tests`（types/coords/protection/merge/quality/mod） | 35（含既有 geometry 过滤命中） |
| `tests/geometry_foundation.rs` | 7 |

### 1.4 顺带修复（非 G11 功能，但门禁需要）

- `skin_segmentation/model.rs`：`status_from_specs` 多模型循环原先每次失败都 `return`（clippy `never_loop` + 多模型逻辑错误）→ 改为记录 `last_detail` 后继续，全部失败才 fallback。
- `skin_segmentation/onnx.rs` 测试：NCHW 索引改用 `nchw()` 辅助，消除 `identity_op`/`erasing_op`。

---

## 2. 测试结果（本机实测）

| 命令 | 结果 |
|---|---|
| `cargo test --lib` | **516 passed / 0 failed / 3 ignored** |
| `cargo test --test geometry_foundation` | **7 passed / 0 failed** |
| `cargo test --test beauty_skin_model` | **8 passed / 0 failed** |
| `cargo test --test slim_items` | **12 passed / 0 failed** |
| `cargo clippy --all-targets` | **0 error**（仍有既有 style warning，不阻断） |
| `pnpm test` | **10 files / 134 passed** |
| `pnpm typecheck`（`tsc --noEmit`） | **exit 0** |

### 验收对照

| # | 条件 | 结果 |
|---|---|---|
| A1 | geometry 进 lib 且测试绿 | PASS |
| A2 | strength=0 恒等 → merge None | PASS |
| A3 | 保护 w=0 → delta 0 | PASS |
| A4 | 多层保护 min | PASS |
| A5 | 多区域合并/叠加/clamp | PASS |
| A6 | 坐标往返 <1e-4 | PASS |
| A7 | 无整图横压 API | PASS（类型仅 region+points） |
| A8 | 不回归 | PASS（上表全绿） |
| A9 | 不宣称 AI 瘦脸瘦身 | PASS（`ai_face_body_slim=false`） |

### 关于「真实照片对比」

任务书要求阶段交付含真实照片对比。**G11 按任务书只做底座、不做瘦脸瘦身算法**，故本阶段以：

- 合成网格恒等（strength=0）
- 保护 mask 阻断位移
- 多区域 clamp

作为像素级证据；**真实样张原图/处理对比固定在 G16**，不在 G11 伪造「瘦脸效果图」。

---

## 3. 明确未做

- ❌ 未改 `pipeline.rs` 阶段顺序  
- ❌ 未接线 face/body 内核到 `LocalWarpOp`（G13）  
- ❌ 未新增 TPS / 第三方依赖  
- ❌ 未改 `.lumi` 持久化 schema  
- ❌ 未做 AI 瘦脸/瘦身参数写入  

---

## 4. Git

| 项 | 值 |
|---|---|
| Commit | **`a60f71d99bf467390a848e6cece87773616f446c`** |
| 标题 | `feat(geometry): G11 几何变形底座与保护蒙版设计` |
| 范围 | 12 files / +1444 / −6 |

后续阶段（G12–G16）在本 SHA 之上继续。
