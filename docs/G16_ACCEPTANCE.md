# G16 · 真实照片验收 · 本轮报告

> 前置：G11–G15 · 素材：`beauty-validation/`（gitignore）

---

## 1. 交付

| 项 | 说明 |
|---|---|
| 测试 | `src-tauri/tests/g16_real_slim_acceptance.rs`（2 例） |
| 对比图 | `docs/assets/g16_slim/{stem}_original.png` / `_faceslim.png` / `_compare.png` |
| 覆盖 | 每类目录最多 1 张、整包最多 4 张；预览分辨率 1200 长边跑完整管线 |

### 实测（本机）

```
G16 DSCF0030.JPG face=0 slim=42
G16 DSCF0053.JPG face=0 slim=42
G16 DSCF0055.JPG face=0 slim=42
G16 DSCF0029.JPG face=0 slim=42
G16 通过 4 张真图瘦脸
test result: ok. 2 passed
```

### 质量门

- `flipped_triangles == 0`
- 全图变化比例 ∈ (0, 0.5)
- 无检测/无计划时 skip，不伪造

### 身体瘦身

真图无已验证人物分割权重 → **不强行瘦身**（任务书）。像素契约见 `g13_slim_algorithms.rs`。

---

## 2. 全量回归（G16 提交前）

| 命令 | 结果 |
|---|---|
| `cargo test --lib` | 534+（含 G12–G13） |
| `cargo test --test g13_slim_algorithms` | 5 passed |
| `cargo test --test g16_real_slim_acceptance` | 2 passed |
| `pnpm test` | 135 passed |
| `tsc --noEmit` | 0 |

---

## 3. Git

见回填 SHA。
