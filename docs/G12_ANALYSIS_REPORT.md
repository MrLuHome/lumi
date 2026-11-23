# G12 · 人脸轮廓与人体分析 · 本轮报告

> 轨道：G11–G16 · 前置：G11 `a60f71d`  
> 范围：分析数据结构与保护蒙版接口；**不写瘦脸/瘦身变形算法**（G13）  
> 诚实约束：姿态模型未启用；衣服分割无已验证权重 → **不伪造**

---

## 1. 交付

### 1.1 人脸轮廓 `face_reshape/contour.rs`

| 项 | 说明 |
|---|---|
| 方法标签 | `yunet5-bbox-contour`（UI 禁止写成精准 AI 轮廓） |
| 输出 | 中心线、左右脸颊、颧骨、下颌线、下巴、太阳穴、耳朵（框级近似） |
| 置信度 | 检测分 × 侧脸惩罚 × 点健全性 |
| 激进变形门 | `allow_aggressive = confidence >= 0.55`（G13 强制） |
| 测试 | 4：正脸 / 低分禁激进 / 侧脸降置信 / 无点报错 |

### 1.2 人体分析 `ai_engine/body/analysis.rs`

| 项 | 说明 |
|---|---|
| 方法标签 | `alpha-silhouette-contour` |
| `BodyAnalysis` | detected、confidence、bbox、structure、regions、subject_alpha、occlusion、notes |
| 结构点 | 肩/胸/腰/胯/膝/踝 + 肘腕近似；**全部 `source=silhouette`** |
| `pose_model_available` | **false**（与 `pose.rs` 一致） |
| 分区 | arms×2 / waist / hips / legs 归一化盒 |
| `clothes_protect` | **恒空**（无已验证权重，notes 说明） |
| 保护蒙版 | `background_protection`（人外=0）、`body_edge_protection` |
| 测试 | 5：检出、空 alpha、衣服为空、背景保护、分区归一化 |

### 1.3 文档

- 本报告；设计约束沿用 `docs/G11_GEOMETRY_FOUNDATION.md` §G12 预告。

---

## 2. 测试结果

| 命令 | 结果 |
|---|---|
| `cargo test --lib face_reshape::contour` | 4 passed |
| `cargo test --lib body::analysis` | 5 passed |
| `cargo test --lib geometry` | 35 passed |
| 全量 `cargo test --lib` | 见提交前复跑 |
| `pnpm test` / `tsc` | 不回归（本阶段无前端改动） |

---

## 3. 明确未做

- ❌ 未接入 MediaPipe 姿态推理（无许可权重）
- ❌ 未导出衣服/手部分割蒙版
- ❌ 未改 pipeline、未写 G13 变形
- ❌ 未在 UI 宣称「AI 姿态/精准轮廓」

---

## 4. Git

**6a113dec5dcf1f40e421eed328602085be13020a**
