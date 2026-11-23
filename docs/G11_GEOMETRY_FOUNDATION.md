# G11 · 几何变形引擎底座与保护蒙版设计

> 轨道：瘦脸 / 瘦身补充路线 **G11–G16**（与已关账的 AI 底座 R11 模型注册表无关）  
> 范围：**仅底座与设计**。本阶段不实现 AI 自动瘦脸 / 瘦身算法（那是 G13）。  
> 仓库：`/Users/lu/Documents/lumi人像` · 日期：2026-11-23

---

## 0. 结论摘要

1. **仓库已有两套可用内核**，不是从零开始：
   - 面部：`face_reshape` Delaunay 控制网格 + 分片仿射反向映射 + `quality::stabilize`；
   - 身材：`ai_engine/body` 规则 `GridField` 位移场 + 轮廓带 / 锚点 / 笔触 + `FaceGuard` + Jacobian 折叠检测。
2. **缺口是「统一底座」而不是「再写一个 warp」**：控制点模型、强度/范围、坐标映射、保护蒙版输入、多区域合并目前三套方言（face mesh / body grid / manual ROI），无法被 G13–G15 共用。
3. **技术选型（本阶段评估，不推倒重写）**：
   | 方案 | 结论 | 理由 |
   |---|---|---|
   | Mesh Warp（Delaunay 分片仿射） | **面部轮廓继续用** | 已有翻转/面积质量闸门；外圈 Fixed 结构性保背景 |
   | 局部位移场（规则网格双线性） | **通用局部变形主内核** | 分辨率无关、内存可控、易挂保护 alpha、已有人体折叠检测 |
   | Thin Plate Spline | **G11 只留评估与接口，不实现** | 控制点少时平滑好，但全图 TPS 在高分辨率上成本高、难做局部 range/protect；无现成依赖与测试基线 |
   | 保边缘变形 | **以保护蒙版 + 质量降强度表达** | 不引入新 PDE 求解器；边缘稳定靠 `protection` 乘子与 `stabilize` |
   | 多区域变形合并 | **G11 实现 `merge_ops`** | 按区域并集、位移取加权和后统一 clamp，禁止整图横压 |
4. **保护语义统一为「允许位移权重 w∈[0,1]」**：`w=0` 完全保护，`w=1` 允许变形。多层取 **min**（最严保护优先），与 `FaceGuard::weight` 一致。
5. **红线**：不得对整图做简单横向压缩；不得在无对应部位检测时强行变形（G13+ 强制）；G11 不得假称已完成瘦脸瘦身效果。

---

## 1. 现状盘点（file:line 可复核）

### 1.1 已有内核

| 内核 | 位置 | 能力 |
|---|---|---|
| 面部控制网格 | `face_reshape/mesh.rs`, `face_mesh.rs` | 眼距局部坐标、外圈 Fixed、Delaunay、signature |
| 面部 warp | `face_reshape/warp.rs` | 目标三角形重心坐标反向采样，只写 alpha 区域 |
| 面部质量 | `face_reshape/quality.rs` | 翻转 / 面积比 / 位移比，失败按 0.75 降强度 |
| 手动 ROI | `face_reshape/operations/manual.rs` | push/pull/zoom/protect/restore 笔触 |
| 身材位移场 | `ai_engine/body/mesh.rs`, `deformation.rs` | 轮廓带、锚点高斯、笔触、12% 位移 clamp |
| 人脸守卫 | `ai_engine/body/constraints.rs` | 脸框内 w=0，框外 ramp 恢复 |
| 折叠检测 | `constraints.rs::fold_report` | Jacobian ≤0.05 计数，超限降强度 |
| 人物蒙版 | `ai_engine/segmentation` | 背景/主体隔离，供 body 门控 |

### 1.2 管线顺序（不得在 G11 打乱）

```
subject mask → body warp → background → beauty → face reshape → grade → …
```

G11 **不改** `pipeline.rs` 阶段顺序；统一底座以库形式存在，供 G13+ 接入。

### 1.3 坐标系

| 空间 | 定义 | 使用方 |
|---|---|---|
| Normalized | 图像 0..1 | 项目持久化、笔触、锚点、FaceFrame |
| Pixel | 像素 | GridField、采样 |
| Face-local | 眼距单位 (u,v) | 面部控制点与脸型操作 |

G11 在 `geometry::coords` 固化互转，禁止调用方手写 `* width`。

---

## 2. 统一底座 API（G11 交付）

模块：`src-tauri/src/geometry/`

```
geometry/
  mod.rs          — 模块出口与能力说明
  types.rs        — ControlPointOp / LocalWarpOp / RegionBox / WarpStrength
  coords.rs       — NormalizedPoint ↔ pixel、区域盒
  protection.rs   — ProtectionLayer、compose（min）、apply_to_delta
  merge.rs        — 多区域 LocalWarpOp 合并（并集 + 加权位移 + clamp）
  quality.rs      — 统一 GeometryQualityReport（承接 flip/fold/位移比语义）
```

### 2.1 核心类型（语义）

```rust
/// 归一化控制点：pos 0..1，delta 为归一化位移（与 BodyAnchor.dx/dy 同量纲）。
pub struct ControlPointOp {
    pub pos: [f32; 2],
    pub delta: [f32; 2],
    pub radius: f32,   // 短边归一化影响半径
    pub strength: f32, // 0..100
}

/// 一次局部变形请求（非破坏：只描述，不直接改像素）。
pub struct LocalWarpOp {
    pub region: RegionBox,      // 归一化包围盒
    pub points: Vec<ControlPointOp>,
    pub strength: f32,          // 0..100 总强度
    pub feather: f32,           // 0..100 边界羽化
    pub protection: ProtectionMask, // 允许位移权重图（可与 op 分离合成）
}
```

**强度语义**：`effective_delta = delta * (strength/100) * w_protect * w_region_feather`。  
`strength=0` ⇒ 恒等（像素不变），有测试锁死。

### 2.2 保护蒙版

| 层 | 来源（现有） | G11 映射 |
|---|---|---|
| 背景 | subject mask 的 complement | `ProtectionLayer::Background` → w 低 |
| 五官 | FaceGuard / Fixed 控制点 / skin feature protect | `ProtectionLayer::Features` |
| 头发 | subject hairProtect / 脸椭圆外 | `ProtectionLayer::Hair` |
| 衣服 | G12+ 分割；现阶段可空 | `ProtectionLayer::Clothes`（预留） |
| 手部 | G12+ 姿态；现阶段可空 | `ProtectionLayer::Hands`（预留） |
| 人体边缘 | body alpha 轮廓带外侧 ramp | `ProtectionLayer::BodyEdge` |
| 用户保护笔 | `WarpMode::Protect` 笔触 | `ProtectionLayer::UserProtect` |

`compose_protection(layers) -> ProtectionMask`：同尺寸 `Vec<f32>`，逐点 `min`。  
`apply_to_delta(delta, mask, index)`：`delta * mask[index]`。

**禁止**：整图统一横向 `scale_x`；必须有 region + protection 才能写位移。

### 2.3 多区域合并

```rust
pub fn merge_ops(ops: &[LocalWarpOp], width: u32, height: u32) -> Option<MergedWarp>
```

- 区域取并集包围盒；
- 控制点按位置高斯核累加位移（同 `apply_anchors` 语义）；
- 合成后统一 `max_displacement_ratio` clamp（默认相对短边 12%，与 body 一致）；
- 空 op / 全零 strength → `None`（调用方 identity）。

### 2.4 质量控制占位

```rust
pub struct GeometryQualityReport {
    pub flipped_triangles: usize,
    pub folded_cells: u32,
    pub max_displacement_ratio: f32,
    pub applied_scale: f32,
    pub degraded: bool,
    pub notes: Vec<String>,
}
```

G11 提供类型与 `degrade_scale` 工具（按已有 0.75 递减策略）；**完整检查仍调用**  
`face_reshape::quality::stabilize` / `body::constraints::fold_report`（G13 接线）。

---

## 3. 方案评估详情（任务书要求项）

### 3.1 Mesh Warp
- **优点**：拓扑清晰、翻转可检测、面部外圈 Fixed 天然保背景。
- **缺点**：控制点由算法生成时用户不可自由编辑；高密度网格构建有成本。
- **G11 决策**：保留为 **face contour 内核**；`LocalWarpOp` 不直接暴露三角形，由适配器生成 mesh。

### 3.2 局部位移场
- **优点**：任意局部 range、易乘保护 alpha、多 op 可在场上合并。
- **缺点**：网格过粗会锯齿；过细内存大。现有 body 短边/160、6..24px 单元是合理默认。
- **G11 决策**：**通用主内核**；`MergedWarp` 输出场描述，G13 决定采样实现（复用 `GridField`）。

### 3.3 Thin Plate Spline
- **优点**：少量控制点全局 C² 平滑。
- **缺点**：① 难以严格限制在 `region` 内（尾部影响全图）；② 与 protection mask 逐像素相乘语义冲突；③ 24MP 全图求解/采样成本无 benchmark；④ 仓库零实现、零测试。
- **G11 决策**：**记录为候选，不实现**。若未来脸型大位移需要更平滑场，优先在 GridField 上做更强的平滑先验，而不是引入 TPS 依赖。

### 3.4 保边缘变形
- **G11 决策**：不引入 level-set/各向异性扩散。边缘稳定 =  
  `region feather × protection min × displacement clamp × quality degrade`。

### 3.5 多区域合并
- **G11 决策**：见 §2.3。测试覆盖：两区域不重叠时互不干扰；重叠时位移叠加后 clamp；与保护 mask 相乘。

---

## 4. 非破坏性与撤销

| 层 | 现状 | G11 |
|---|---|---|
| 参数存储 | `EditParams.reshape[] / body[]` | 不改 schema；`LocalWarpOp` 为库内中间结构 |
| 预览/导出 | 每次从原图重跑 pipeline | 保持 |
| 撤销 | history 100 步 + coalesce key | G15 手动 UI 继续用；底座不自带 undo 栈 |
| 项目文件 | `.lumi` JSON + unknown fields | G11 不新增持久化字段 |

**结论**：非破坏性已由「参数 + 全量重渲」保证；G11 不引入第二套像素缓存。

---

## 5. 明确不做（本阶段）

- ❌ AI 自动瘦脸 / 瘦身写入参数（G13）
- ❌ `FaceShapePlan` / `BodyShapePlan` 一键集成（G14）
- ❌ 新的手动瘦脸/瘦身滑条 UI（G15）
- ❌ 真实样张前后对比验收（G16；G11 只用合成图恒等/隔离测试）
- ❌ TPS / 新第三方依赖
- ❌ 修改 pipeline 阶段顺序
- ❌ 假算法或模拟效果宣布完成

---

## 6. 验收标准（G11）

| # | 条件 | 判据 |
|---|---|---|
| A1 | 模块编译进 lib | `cargo test --lib geometry` 全绿 |
| A2 | 强度 0 恒等 | `strength=0` 合并结果为 None 或零位移 |
| A3 | 保护优先 | 保护 w=0 处 delta 必为 0 |
| A4 | 多层 min | 两层保护合成后取更严者 |
| A5 | 多区域合并 | 并集、叠加、clamp 有断言 |
| A6 | 坐标映射 | normalized↔pixel 往返误差 < 1e-4 |
| A7 | 无整图横压 API | 类型上无 `global_scale_x` 之类入口 |
| A8 | 不回归 | `cargo test --lib`、`pnpm test`、`tsc --noEmit` 与基线一致全绿 |
| A9 | 无假完成 | 文档与 capability 不宣称已支持 AI 瘦脸瘦身 |

---

## 7. 与 G12–G16 的接口预告

| 阶段 | 依赖 G11 的部分 |
|---|---|
| G12 | 输出 `ProtectionLayer::Hands/Clothes/BodyEdge` 所需的人体区域与置信度；仍不写变形算法 |
| G13 | 用 `LocalWarpOp` + `compose_protection` + `merge_ops` 构图；调用现有 face/body 内核或 GridField；强制质量闸门 |
| G14 | 生成 plan → 填 `FaceReshape`/`BodyAdjust` 参数（仍走现有持久化） |
| G15 | UI 只产 `LocalWarpOp`/既有 params，不复制内核 |
| G16 | 真实照片对比 + 变形质量人工验收 |
