# G13 · AI 瘦脸与瘦身算法 · 本轮报告

> 前置：G11 `a60f71d` · G12 `27658a0`  
> 范围：由分析生成参数 + 质量门控 + 像素证明；**不做**一键集成（G14）与手动 UI（G15）

---

## 1. 交付

| 文件 | 职责 |
|---|---|
| `face_reshape/ai_slim.rs` | `plan_face_slim(contour, style, strength) → FaceShape`；低置信封顶、禁止激进 |
| `ai_engine/body/ai_slim.rs` | `plan_body_slim(analysis, …) → BodyShape`（瘦身=负值）；无部位不写；遮挡降级 |
| `tests/g13_slim_algorithms.rs` | 5 条像素契约 |

### 算法要点

- **脸**：`slim/jaw/cheekbone/width` 主写，`length=0`；`allow_aggressive=false` 时 cap 30–40。
- **身**：`waist/hips/thighs/calves/arms` 负值收窄；分区盒缺失 → 对应字段 0；`BODY_MIN_CONFIDENCE=0.25`。
- **质量**：渲染仍走 `quality::stabilize` / `fold_report`；测试断言 `flipped_triangles=0`、`folded_cells=0`。
- **非整图横压**：只写轮廓参数，无 `global_scale`。

---

## 2. 测试

| 套件 | 结果 |
|---|---|
| `cargo test --lib ai_slim` | 10 passed |
| `cargo test --test g13_slim_algorithms` | **5 passed** |
| `cargo test --lib` | 见全量 |
| `pnpm test` / `tsc` | 不回归 |

像素证据：

1. 高级瘦脸 → 脸区大量变化、背景角不变、0 翻转  
2. 强度 0 → 全图恒等  
3. 低置信 → `slim≤31` 且 notes 含降级  
4. 瘦身 → 轮廓带变化、远角不变、0 折叠  
5. 无人体 → 中性  

**真图对比仍归 G16。**

---

## 3. Git

见回填 SHA。
