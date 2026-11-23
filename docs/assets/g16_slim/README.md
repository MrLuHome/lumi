# G16 真图瘦脸对比

- `{stem}_original.png` 原图
- `{stem}_faceslim.png` 瘦脸后
- `{stem}_compare.png` 左右对比

素材来自 `beauty-validation/`（不入库）。质量门：无三角翻转、变化比例 ∈ (0, 0.5)。
身体瘦身真图需人物蒙版；无已验证分割时不伪造，像素契约见 `tests/g13_slim_algorithms.rs`。
