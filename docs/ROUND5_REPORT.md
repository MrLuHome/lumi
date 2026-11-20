# ROUND 5 报告 — BODY & BACKGROUND ENGINE 1.0

日期：2026-11-20
分支：`main`
第五轮基线提交：`ba183f7`（第四轮面部塑形引擎、faceId 持久化、LandmarkerProvider、项目 v2）
本轮范围：手动人物蒙版、身材调整、背景处理、项目与渲染集成

> 说明：正式修订版任务书在「整体平衡 | 调整多个控制」处中断，背景引擎细则、项目集成与验收标准的后续章节未收到。本报告只声明已实现且可验证的能力，不臆造缺失章节的验收结论；收到补充后会按新增要求继续对齐。

---

## 1. 执行前核对

- `git branch --show-current`：`main`
- 基线 `git log`：`ba183f7` 及其后续合法提交，未回退、未 `reset --hard`
- 第四轮成果全部保留：`face_reshape/`、人脸 UUID 持久化、Face ID 恢复、`LandmarkerProvider`、手动变形、项目结构与报告
- 本轮为增量接入，未删除既有模块、未删除既有测试、未降低旧功能标准

---

## 2. 四个模块交付

### 2.1 MANUAL SUBJECT MASK ENGINE（手动人物蒙版）

定位：**手动人物蒙版**，不是 AI 自动分割、智能抠图或 AI 发丝识别。UI 文案与能力接口均按此标注。

实现：

- 数据结构：`SubjectMaskParams`（`enabled`、`invert`、`feather`、`expand`、`smooth`、`hair_protect`、`decontaminate`、`strokes`、`source="manual"`、`model_version="manual-alpha-1"`）。
- 笔触类型：`paint` / `erase` / `foreground` / `background`；坐标归一化，缩放、平移后与原图严格对应。
- Alpha：连续 `f32`（比 0/0.25/0.5/0.75/1.0 更细），不是二值蒙版。
- 处理顺序：普通笔触 → 前景标记取最大 → 背景标记取最小 → 平滑 → 收缩/扩张 → 羽化 → 发丝辅助 → 反转。
- 羽化使用可分离盒式模糊多 pass 近似边缘过渡，不做整张大半径高斯，避免轮廓丢失。
- 区分三件事：蒙版平滑（`smooth`）、Alpha 过渡（`feather`）、前景颜色去污染（`decontaminate`）。
- 去污染：用邻近可信前景像素估计边缘颜色，不做固定色涂抹。
- 发丝：`hair.rs` 用亮度高频调制边缘 Alpha；复杂发丝不生成虚假细节，保留手动笔刷入口。
- 缓存：`MaskCache` 容量 4，按蒙版参数哈希命中。
- 画布：红色遮罩 / 黑白 Alpha 两种显示；预览由后端 `preview_subject_mask` 生成，和最终渲染共用同一套 Alpha 算法。

### 2.2 BODY SHAPE ENGINE（身材调整）

路线：**手动蒙版轮廓 + 用户手动锚点 + 位移上限 + 局部网格变形**，不依赖 AI 姿态。

实现：

- 轮廓：`landmarks.rs` 逐行扫描 Alpha 阈值 0.4 的左右边界，覆盖行低于 8% 判定不可用；没有可靠人物区域时不产生自动身材结果。
- 网格：`GridField` 粗网格（6..24px 控制点）双线性插值，反向采样；全分辨率不存储逐像素位移场。
- 形状项：收腰、肩宽、胯部、大腿、小腿、腿部比例（受控）、手臂；按高度高斯权重只作用轮廓带（半宽 55%），内部细节保持不动。
- 锚点：左右肩/腰/髋/大腿/膝/小腿/手臂与自定义；归一化坐标持久化，重开项目恢复；支持建议、添加、删除、拖动、半径、锁定、复位、撤销/重做。
- 推拉：推动、拉动、局部放大、局部缩小、保护、恢复六种笔触。
- 质量保护：人脸框 `FaceGuard` 逐像素保护；位移上限 12%；40×40 Jacobian 折叠检测，发现折叠自动降强度最多 3 次，仍折叠则跳过并记录原因。
- 多人：`BodyAdjust.subject_id` 隔离，每人可使用独立 `mask_strokes`；Alpha 与图像同步变形，互不影响。

### 2.3 BACKGROUND ENGINE（背景处理）

- 模式：原始、背景虚化、纯色、渐变、本地图片。
- 虚化：盒式模糊多 pass 近似高斯；景深按纵向位置模拟（明确不是 3D 深度重建）。
- 替换：纯色、角度渐变（多色标）、本地图片（Cover/Contain/Stretch、缩放、偏移、图片模糊）。
- 独立处理：背景只作用在 Alpha 之外；前景像素完全不被背景颜色影响。
- 颜色：背景亮度、色温、饱和度；边缘去污染在合成前执行。
- 合成：`fg * alpha + bg * (1 - alpha)`；没有蒙版时给出警告并跳过，不偷偷处理整图。

### 2.4 PROJECT & RENDER INTEGRATION

- 管线顺序：原图 → 手动 Alpha → 身材位移场（同步 Alpha）→ 背景 → Alpha 合成 → 第四轮面部塑形 → 第三轮美肤 → 调色/细节 → 输出。
- 没有第五轮参数时，像素与旧版一致（`neutral_render_is_identity` 保持通过）。
- 预览与导出共用 `pipeline::render_with_report`；`RenderResult` 新增 `body`、`background`、`mask` 报告。
- 项目版本升级到 v3：新增 `subjects`、`mask_meta`；Alpha 不入 JSON，只保存可重放笔触与哈希；v1/v2 向后兼容。
- 非破坏性：原始照片只读，所有编辑以参数保存，可撤销、保存、恢复、重新编辑。

---

## 3. 前端

- `src/state/params.ts`：新增 `SubjectMaskParams`、`BodyAdjust`、`BodyAnchor`、`BackgroundParams` 及默认值、夹紧、激活计数。
- `src/state/editorStore.ts`：新增人物蒙版画笔、目标（公共/当前人物）、显示模式、身材工具与锚点、背景更新与重置；`markRendered` 保存三份报告。
- `CanvasStage.tsx`：原图坐标画笔、红色遮罩/黑白 Alpha、锚点显示与拖动、推拉笔刷；缩放平移后坐标一致。
- 新增 `SubjectMaskPanel.tsx`、`BodyPanel.tsx`、`BackgroundPanel.tsx`，在 `Inspector` 中以折叠分组接入；展示「手动蒙版（非 AI 分割）」与模型禁用原因。
- `src/lib/backend.ts` 接入 `segmentation_capability`、`preview_subject_mask`、`body_capability`、`body_anchor_presets`、`pick_background_image`、`background_image_info`。
- `src/state/project.ts` 升级到 version 3。

---

## 4. 验证结果

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --lib` | 211 passed / 0 failed |
| Rust 静态检查 | `cargo clippy --lib --tests` | 通过，无新增警告 |
| 前端类型 | `pnpm typecheck` | 通过 |
| 前端测试 | `pnpm test` | 4 个文件，50 passed |
| 前端构建 | `pnpm build` | 通过 |
| 桌面启动 | `pnpm tauri dev` | 本轮未做长时间交互验证；命令注册与构建通过，需在审核环境补一次手工冒烟 |

关键回归测试：

- `neutral_render_is_identity`：无参数像素不变
- `background_replaces_only_outside_manual_mask`
- `body_edit_changes_subject_not_corner`
- `background_without_mask_warns_and_keeps_pixels`
- `project_roundtrip_keeps_face_identity_records`（第四轮回归）
- `v2_project_without_new_fields_loads_with_defaults`
- `v3_roundtrip_keeps_subject_body_and_mask_meta`

---

## 5. 已知限制与未完成事项

1. 背景引擎细则、项目集成细则与验收标准章节未收到，当前实现只覆盖已确认部分，待补充后对齐。
2. 没有经过授权的人物分割权重，`ai_available=false`；手动蒙版是唯一可用路径。
3. 没有姿态模型，身材锚点是用户放置的几何控制点，不是 AI 识别结果。
4. 发丝边缘为局部笔刷 + 亮度高频辅助，不承诺复杂发丝自动恢复。
5. 腿部比例为受控范围位移，不是解剖学意义上的精确比例调整。
6. 景深为纵向模拟，不是 3D 深度重建。
7. 多人场景需要用户先建立独立蒙版，再分别为每个对象调整。
8. 尚未做桌面端长时间手工冒烟与高分辨率导出实测。

---

## 6. 模型与许可

- 本轮未新增模型权重。
- BiRefNet、SAM 2、MediaPipe Pose、MediaPipe `.task`、RVM 均不进入正式版本，详见 `docs/ROUND5_MODEL_LICENSES.md`。
- 第三方资产登记同步更新在 `THIRD_PARTY_LICENSES.md`。

---

## 7. 本轮停止点

第五轮到此停止，不进入第六轮，等待审核。
