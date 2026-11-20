# 第四轮修订报告 · FACE RESHAPE ENGINE 4.0（A 方案：分层降级交付）

日期：2026-11-20
基线提交：`91a8120`（第三轮 AI BEAUTY ENGINE 3.0）
交付定位：**LEVEL 1 手动变形 + LEVEL 2 轮廓级保守调整正式交付；LEVEL 3 稠密关键点五官精修仅保留接口与技术验证，明确禁用。**

---

## 1. A 方案实施结果

| 层级 | 内容 | 本轮状态 |
| --- | --- | --- |
| LEVEL 1 | 推动 / 拉动 / 局部放大 / 局部缩小 / 保护 / 恢复、半径、强度、羽化、网格显示、单笔撤销、全局撤销重做 | **已实现并真实改变像素** |
| LEVEL 2 | 整体瘦脸、面部宽度、下颌线、颧骨、面部长度、左右轮廓、左右平衡；基于真实检测框 + 5 点锚点的轮廓环保守调整 | **已实现，UI 标注“轮廓级近似调整”** |
| LEVEL 3 | 眼睑、眼角、鼻翼、唇线、下巴尖度等稠密五官塑形 | **未启用**：`LEVEL3_ENABLED = false`，能力查询返回禁用原因；即使项目文件携带参数，渲染前也会被清零 |

关键约束全部保持：

- 不推倒重写：在第三轮管线、参数系统、项目结构、前端工作台上增量实现。
- 不用 CSS 假装变形：所有效果在 Rust 内完成网格变形与像素重采样。
- 不只移动关键点：控制点位移最终通过分片仿射反向映射写入 16-bit 像素。
- 不做生成式整脸重绘：无任何生成模型。
- 不覆盖原图：只处理缓存中的 `Image16`，项目保存参数而非结果图。
- 不打包未授权权重：仓库内仍只有 YuNet 5 点（MIT，第三轮已存在）。
- 背景保护：变形只写面部椭圆 alpha 区域，外圈控制点固定，四角背景逐像素不变（有自动化断言）。

---

## 2. 审查结果摘要

完整审查见 [ROUND4_AUDIT.md](ROUND4_AUDIT.md)。关键结论：

1. **P0 阻塞（关键点）**：YuNet 仅 5 点，无法支撑眼睑、鼻翼、唇线网格。按要求未用 5 点硬凑稠密五官，改为 LEVEL 1/2 降级交付，并把稠密模型验证单独成文。
2. **P1 faceId 错配**：第三轮以检测数组下标作为持久 `faceId`，重开或重检测后可能把 A 的参数套到 B。本轮已改为稳定 UUID + 几何匹配，见第 5 节。
3. **P1 管线缺口**：第三轮没有面部变形阶段与质量出口。本轮在美肤之后、调色之前插入 `face_reshape`，并把逐脸质量报告透传到前端。
4. **P2**：参数越界、低置信度关键点、多人脸重叠区域归属、预览/导出一致性，均在本轮补齐。

---

## 3. 新增模块与文件

### 3.1 后端新增

`src-tauri/src/face_reshape/`（21 个文件，约 4300 行，含单元测试）：

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 能力描述、参数清洗、逐脸候选构建、质量保护、区域归属、管线入口、报告 |
| `landmarker.rs` | 统一 `LandmarkerProvider`：模型信息、许可证元数据、初始化、检测、坐标转换、置信度、卸载 |
| `landmark.rs` | 5 点几何校验、置信度评估、自动降强度 |
| `face_mesh.rs` | 轮廓环 + 眼部/鼻部/唇部同心环 + 外圈固定环；网格缓存与预览 |
| `mesh.rs` | 控制点、三角形、翻转/面积比/位移统计、版本号 |
| `triangulation.rs` | Bowyer–Watson Delaunay 三角剖分 |
| `deformation.rs` | 目标坐标计算、强度缩放、局部权重 |
| `warp.rs` | 分片仿射反向映射 + 双线性采样 + alpha 混合写回 |
| `boundary.rs` | 面部椭圆作用域、羽化、多人脸区域归属（`apply_ownership`） |
| `blending.rs` | 16-bit 像素混合 |
| `mask.rs` | 作用蒙版与边缘保护 |
| `quality.rs` | 三角翻转、面积比、位移上限；0.75 递减强度直至 0 |
| `identity.rs` | faceUUID、项目人脸记录、SHA-256 源图哈希、重开匹配、人工确认 |
| `presets.rs` | 仅含 LEVEL 1/2 范围的预设（自然 / 精致 / 立体） |
| `operations/` | `face`、`eyes`、`nose`、`mouth`、`chin`（LEVEL 3 接口保留、生产清零）、`manual`（手动笔触） |

其他新增：

- `src-tauri/src/commands/reshape.rs`：`reshape_capability`、`reshape_mesh`、`export_face_records`、`restore_faces`、`confirm_face_match`。
- `src-tauri/tests/reshape_persistence.rs`：三人合照重开、源图替换、同图重导入。
- `src-tauri/tests/reshape_pipeline.rs`：轮廓变形真实改像素 + 背景保护、LEVEL 3 参数忽略、手动笔触局部生效、12MP 性能基线（`#[ignore]`）。
- `src/components/ReshapePanel.tsx`：面部塑形面板。
- `docs/FACE_LANDMARK_MODEL_EVALUATION.md`：MediaPipe 478 点与商业 SDK 评估。
- `docs/ROUND4_AUDIT.md`：本轮审查。

### 3.2 修改文件

后端：`ai/face.rs`、`ai_engine/face_frame.rs`、`ai_engine/skin_segmentation/mod.rs`、`commands/{faces,mod,project,render}.rs`、`image_engine/{params,pipeline,project}.rs`、`lib.rs`、`state.rs`、测试 `tests/{beauty,beauty_real,engine}.rs`。

前端：`App.tsx`、`components/{CanvasStage,Inspector}.tsx`、`hooks/usePreviewRenderer.ts`、`lib/backend.ts`、`state/{editorStore,params,project}.ts`、`state/params.test.ts`、`styles/app.css`、`types.ts`。

---

## 4. 算法实现

### 4.1 关键点方案（LEVEL 1/2）

- 唯一来源：YuNet 5 点（双眼、鼻尖、左右嘴角），报告中 `source = "yunet-5"`、`denseLandmarks = false`。
- 坐标先转换到**原始图像素坐标系**，再做网格与变形；前端画笔也按原图坐标提交，缩放/平移画布不改变结果。
- 置信度检查鼻尖纵坐标、嘴角中心、嘴宽、双眼距离等几何关系；异常时 `autoScale < 1`，低置信度自动降低强度并提示，不直接输出高强度变形。
- 未使用人脸框推导眼睑、鼻翼、唇线。

### 4.2 网格

- 脸型轮廓 4 层同心椭圆环（12/16/20/24 点，权重 0→1 平滑衰减）+ 外圈固定环（24 点，权重 0）。
- 眼部、鼻部、唇部各 3 层同心环，中心锚定真实 5 点（眼中心、鼻尖、嘴角）。
- 全部控制点带 `from`（原位置）与 `to`（目标位置），网格有独立 `MESH_VERSION`，可缓存、可重新计算。
- Delaunay 三角剖分生成约 400–450 个三角形（视脸型环密度）。

### 4.3 变形与重采样

- 每个参数只移动对应控制点：轮廓参数移动脸缘环，权重由内到外按平滑曲线衰减。
- 质量合格后执行**分片仿射反向映射**：对每个目标三角形做重心坐标光栅化，从原图双线性采样 16-bit RGBA。
- 只写回面部椭圆 alpha 区域，并按 alpha 与原图混合；椭圆外 alpha 恒为 0。
- 多人脸：先计算所有脸的区域，再按 `apply_ownership` 处理重叠像素归属，A 的变形不会污染 B。

### 4.4 手动变形（LEVEL 1）

- 以归一化坐标保存笔触，构建逐像素位移场（`dx/dy`），双线性采样后反向映射。
- 推动/拉动沿拖动方向施加高斯衰减；放大/缩小以笔触中心做局部径向缩放；保护笔刷压低区域位移；恢复笔刷把区域混合回“本次塑形前”的 ROI，而非原图文件。
- 位移上限 `0.26 × 眼距`（至少 8px），半径上限为图像宽度的 25%。
- 每笔独立存储，支持单笔撤销、清除全部、全局撤销重做；保护/恢复不直接改写图像。

### 4.5 质量保护

`quality.rs` 在写像素前检查：

| 检查 | 阈值 |
| --- | --- |
| 三角翻转 | 必须为 0 |
| 三角形面积比 | 0.52 – 1.80 |
| 面部整体最大位移 | ≤ 0.26 × 眼距 |
| 局部五官最大位移 | ≤ 0.14 × 眼距（LEVEL 3 接口保留） |

不通过时按 0.75 递减强度重试，最多 9 次；仍失败则回退为不变形。报告包含 `flippedTriangles`、`min/maxAreaRatio`、`maxDisplacementRatio`、`appliedScale`、`degraded`、`notes`，前端显示“已自动降低强度”。

---

## 5. faceId 持久化（本轮阻塞项）

### 5.1 方案

- 每张脸创建稳定 UUID（`new_face_uuid()`），`id_from_uuid()` 用 FNV-1a 生成稳定的 u32 展示 ID；检测数组下标不再作为身份。
- 项目升级为 v2：`ProjectPhoto` 新增 `sourceHash` 与 `faces`。每条 `ProjectFace` 保存：
  - `uuid`
  - 原始检测框 `bbox`
  - 初始 5 点关键点（归一化）
  - 检测置信度 `score`
  - 检测模型版本 `model`（`yunet-5`）
  - `confidence`（匹配置信度）
  - `maskRef`（人脸蒙版引用）
  - 每脸编辑参数与变形参数仍在 `params.faces` / `params.reshape` 中按 UUID 关联。
- `source_hash` 使用流式 SHA-256，不把整图读入内存。
- 重开流程：
  1. 读取项目记录与保存的源图哈希；
  2. 重新检测；
  3. 若源图哈希变化 → 全部转人工确认；
  4. 否则按 `IoU × 0.55 + 关键点相似度 × 0.30 + 尺寸相似度 × 0.15` 做一一贪心匹配；
  5. 高置信（≥ 0.62）自动恢复 UUID；低置信（0.30–0.62）进入待确认；低于 0.30 视为未出现；
  6. 未匹配到的记录保留在项目中，不删除编辑参数；
  7. 人工确认后写回 UUID。
- 不保存任何可用于跨项目识别身份的 embedding。
- 参数解析优先按 `faceUuid`，旧数据回退 `faceId`。

### 5.2 测试覆盖

单元测试（`identity.rs`）：

- `three_faces_survive_detection_reorder`（检测顺序变化）
- `missing_face_keeps_its_record`（人脸暂时检测失败，记录保留）
- `new_person_does_not_steal_existing_parameters`（新增人物不抢参数）
- `changed_source_requires_manual_confirmation`（原图替换）
- `overlapping_faces_are_matched_one_to_one`（两张脸靠近/重叠，一一匹配）
- `source_hash_tracks_file_replacement`
- `project_face_roundtrip_keeps_uuid_and_geometry`（序列化往返）

端到端测试（`reshape_persistence.rs`）：

- `three_face_project_reopens_with_params_on_correct_people`：保存三人参数（25/55/10）→ 重开 → 检测顺序反转、ID 重排 → 全部高置信恢复 → 参数仍对应原人。
- `replaced_source_blocks_automatic_parameter_restore`：源图字节变化 → 全部转 pending，禁止自动套用。
- `reimporting_same_photo_restores_identity`：同一照片重新导入 → 哈希一致 → 恢复原 UUID。

删除人物、新增人物的行为由“记录保留 + 不抢参数”两条测试覆盖；UI 会提示“本次未检测到 N 张已保存的人脸，记录仍保留”。

---

## 6. MediaPipe 与商业 SDK 评估

完整内容见 [FACE_LANDMARK_MODEL_EVALUATION.md](FACE_LANDMARK_MODEL_EVALUATION.md)。

- 官方 `face_landmarker.task`（float16 v1，3,758,596 字节，SHA-256 `64184e22…4e0bc9ff`），包内含 BlazeFace、FaceMesh-V2、geometry pipeline、blendshape 四个资产。
- 模型与 MediaPipe Tasks 运行时均为 Apache-2.0。
- macOS（Python 1.2.3）实测：单脸 478 点，1680×2240 约 3.70 ms；3360×4480 约 4.25 ms；四人合照 10.42 ms；无脸图 0.66 ms；重复运行关键点偏差为 0；进程峰值 RSS 约 242 MiB。
- **结论：LEVEL 3 继续禁用。** 原因：未完成 Windows 实机验证、未确定桌面分发路径（Python sidecar / C++ FFI / WASM Web Tasks）、未完成真实侧脸与遮挡质量评估。
- 未为迁就 ONNX Runtime 转换 MediaPipe 模型，未打包任何未审查权重。
- 商业 SDK 评估表已建立（NVIDIA Maxine、Banuba、Luxand、Visage、ArcSoft、DeepAR）；未取得书面授权前不下载、不打包、不宣称可用。

---

## 7. 许可证审查

| 组件 | 许可证 | 本轮处置 |
| --- | --- | --- |
| YuNet 人脸检测权重 | MIT | 第三轮已随包，本轮未改动 |
| MediaPipe 模型与运行时 | Apache-2.0 | 仅评估，未打包 |
| 商业 SDK | 商业授权 | 仅调研，未下载、未打包 |
| Rust 依赖（image、serde、sha2 等） | MIT / Apache-2.0 | 无新增不合规依赖 |

安装包内未新增第三方权重；若将来启用 LEVEL 3，需附带 Apache-2.0 LICENSE/NOTICE 与模型版本、SHA-256。

---

## 8. 测试结果

执行命令与结果（本机 macOS）：

| 命令 | 结果 |
| --- | --- |
| `cargo test`（src-tauri） | **172 passed，0 failed，2 ignored** |
| 其中 lib 单元测试 | 136 passed |
| `tests/beauty.rs` | 12 passed，1 ignored |
| `tests/beauty_real.rs` | 4 passed |
| `tests/engine.rs` | 10 passed，1 ignored |
| `tests/face_detection.rs` | 4 passed |
| `tests/reshape_persistence.rs` | 3 passed |
| `tests/reshape_pipeline.rs` | 3 passed |
| `cargo test --release --test reshape_pipeline -- --ignored --nocapture` | 1 passed（性能基线） |
| `pnpm test` | **46 passed**（4 个测试文件） |
| `pnpm typecheck` | 通过 |
| `pnpm build` | 通过（主包 326.94 kB，gzip 100.96 kB） |

回归检查：第三轮美肤、皮肤蒙版、调色、项目保存/打开、导出、历史记录测试全部通过；管线新增阶段不影响无塑形参数时的像素（LEVEL 3 测试断言 `into_raw()` 完全一致）。

覆盖矩阵（如实）：

| 项目 | 覆盖情况 |
| --- | --- |
| 正面 / 多人合照 / 暗光 / 高分辨率检测 | 已有真实图片测试（`face_detection`、`beauty_real`） |
| 轮廓瘦脸真实改像素、背景保护 | 合成高对比人像端到端断言 |
| LEVEL 3 参数不生效 | 端到端断言像素完全一致 |
| 手动笔触局部生效、远处不变 | 端到端断言 |
| 多人脸参数独立、重开顺序变化 | 单元 + 端到端 |
| 撤销/重做、项目保存恢复 | 前端单测 + 后端项目测试 |
| 左侧脸 / 右侧脸 / 俯仰 / 眼镜 / 长发遮脸 / 遮挡 | **未完成系统评估**（见第 10 节） |

---

## 9. 性能数据

12MP（3000×4000，release，合成人像，瘦脸 50 / 下颌 25）：

| 指标 | 数值 |
| --- | --- |
| 管线总耗时 | 60.2 ms |
| 单脸网格构建 + 质量检查 + 变形写回 | 43.7 ms |
| 控制点 / 三角形 | 235 / 444 |
| 实际改写像素 | 1,133,752 |
| 三角翻转 | 0 |
| 自动降强度系数 | **0.56**（位移超过 0.26×眼距，连续两次 0.75 递减） |

说明：

- 该参数在 12MP 合成图上触发了质量保护，说明位移上限生效；真实使用中 UI 会提示已降低强度。
- 预览最长边固定 1800px（`PREVIEW_MAX_EDGE`），导出走原分辨率；预览与导出共用 `pipeline::render_with_report`，参数一致。
- 网格预览由 `reshape_mesh` 提供，前端 160ms 防抖；滑块连续变化由渲染任务合并与取消机制处理，旧结果不会覆盖新结果。
- 12MP RGBA16 单缓冲约 92 MiB（算术值）；未做独立 RSS 峰值测量。
- 未使用 GPU：当前为 CPU 实现，大图分块情况下网格按整脸区域计算，不存在分块边界不连续问题（变形按人脸 ROI 整体处理）。

---

## 10. 不同角度效果与已知问题

### 10.1 效果边界

- 正面、轻微侧脸：轮廓环与 5 点锚点可用，保守参数下效果自然；强参数会被位移上限自动削弱。
- 大角度侧脸、俯仰、遮挡、眼镜：5 点仍可能检出，但几何置信度下降，`autoScale` 会降低强度并在报告/UI 提示；**尚未做真实侧脸塑形质量的系统评估**，因此不宣称效果。
- 多人脸：参数按 UUID 独立，重叠区域有归属处理；端到端覆盖两人/三人参数不串。

### 10.2 已知问题

1. LEVEL 3 未启用：无精确眼睑、鼻翼、唇线、眼角、独立下巴尖度调整；UI 明确禁用并显示原因。
2. Windows 未实机验证；MediaPipe 桌面分发路径未定。
3. 真实照片（尤其侧脸、遮挡、长发）的瘦脸视觉质量缺少盲测与可复现参数集；当前自动化主要验证“像素真实改变、背景不变、质量保护生效”。
4. 质量保护对自动网格变形完整；手动笔触采用位移场 + 位移上限 + 区域约束，没有三角网格，因此不做三角翻转检测。
5. 源图哈希变化时全部转人工确认（安全优先），目前没有“同图不同格式”的感知哈希匹配。
6. 人脸检测失败时，旧记录保留但不显示参数恢复结果；需要用户重新检测后再匹配。

### 10.3 未完成事项

- 真实侧脸/俯仰/眼镜/遮挡/多人重叠塑形效果评测与阈值标定。
- MediaPipe 桌面运行时集成（macOS + Windows）与分发验证。
- 商业 SDK 商务与许可证确认。
- 真实照片的对前/后盲测与主观质量评分。
- Windows 安装包实机回归。

---

## 11. 结论

本轮按 A 方案完成 LEVEL 1 手动变形与 LEVEL 2 轮廓级保守调整：像素真实变化、背景受保护、质量保护可降强度、faceId 持久化与项目重开匹配通过测试、MediaPipe 评估给出“继续禁用 LEVEL 3”的明确结论。LEVEL 3 未被标记为已完成，未打包任何未审查权重。

等待审核后进入下一阶段。
