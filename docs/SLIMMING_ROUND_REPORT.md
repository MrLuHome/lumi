# LUMI RETOUCH 双入口精简 · 本轮交付报告

> 角色：工程师（寇豆码）· 任务 #2 · 仓库 `/Users/lu/Documents/lumi人像` · 基线 `32efc79` → 本轮提交见文末
> 设计依据：`docs/SLIMMING_AUDIT_AND_DESIGN.md`（架构师 583 行）+ 主理人裁决 Q1–Q4

---

## 1. 修改文件清单

### 新增（11 个源文件 + 本报告）

| 文件 | 作用 |
|---|---|
| `src/state/slim.ts` | 六项映射、三档系数、安全上限、强度反解、一键计划合成与「不静默覆盖」判定、旧调整审计（**全部纯函数**） |
| `src/state/slim.test.ts` | 32 例：映射/单调/反解/三档/计划/覆盖保护/旧项目往返 |
| `src/components/slim/featureFlags.ts` | `SLIM_UI_ENABLED`、`LEGACY_PANELS_ENABLED`、文件菜单里保留的旧入口 |
| `src/components/slim/analysis.ts` | 三个真实后端动作的统一入口（预热检测、逐脸指标、整图分析）+ 错误白话化 |
| `src/components/slim/OneClickPanel.tsx` | 一键精修：三档 / 整体强度 / 对比 / 应用 / 撤销 / 重新精修 / 空态 / 无人脸 / 失败态 |
| `src/components/slim/ManualPanel.tsx` | 手动调整：六项 + 人脸态提示 + ⑤⑥ 首次拖动真实触发分析 |
| `src/components/slim/SixItemRow.tsx` | 单项行：开关 + 0–100 滑块 + 单项重置 + 方向说明 + 「微调过」标记 |
| `src/components/slim/AdvancedSettings.tsx` | 高级设置（折叠）：逐人脸开关 / 修复画笔 / **旧调整清理** / 导出设置 |
| `src/components/slim/render.test.ts` | 5 例界面冒烟（SSR 渲染 + 禁用词 + 错误白话化） |
| `src/components/ui/Switch.tsx` | 无新依赖的开关 |
| `src-tauri/tests/slim_items.rs` | 7 例**像素级**契约测试：六项各自改像素（新增③④首次断言） |
| `docs/SLIMMING_ROUND_REPORT.md` | 本文件 |

### 修改（6 个）

| 文件 | 改动 |
|---|---|
| `src/App.tsx` | 右栏按 `slimEntry` 只渲染当前入口；旧面板走 `LegacyHost`（带「返回修图」）；两条通知去掉工程术语；首次进入提示批量/RAW 已移到「文件」 |
| `src/components/TopBar.tsx` | 9 个 Tab → 1 个双段控件；保留 导入/对比/撤销/重做/保存/导出；`文件·编辑·视图` 三个菜单（批量/RAW 进「文件」） |
| `src/components/StatusBar.tsx` | 去掉算法名、耗时、GPU、人脸组件名与「工作区 sRGB」 |
| `src/components/ui/Slider.tsx` | 新增可选 `disabled`（无破坏性改动） |
| `src/state/editorStore.ts` | 新增 slim slice：`slimEntry`、`slimItems`、`oneClick`、`slimReports` + 写入/重置/清理动作 |
| `src/styles/app.css` | 追加 `.slim-* / .six-item / .switch / .advanced / .legacy-*` 约 480 行 |

### 明确未动

- `src-tauri/**` 业务代码：**0 行**（仅新增一条集成测试）；无参数 schema 变更，`.lumi` 零迁移。
- 未新增任何第三方依赖（`package.json` 未改）。
- 旧面板组件文件全部保留（`Inspector.tsx`、`SmartPanel.tsx`、`BeautyPanel.tsx`、`AiPortraitPanel.tsx`、`ModelHubPanel.tsx`、`PerformancePanel.tsx`、`ColorPanel.tsx`、`BatchWorkspace.tsx`、`RawPanel.tsx`、`PresetPanel.tsx` 等）。
- 未触碰 `.workbuddy/`、`docs/REVIEW_PERF_SECURITY.md`、`docs/UX_EXPERIENCE_AUDIT_2026-11-22.md`。

---

## 2. 测试结果（本机实测）

| 命令 | 结果 |
|---|---|
| `cargo test --lib` | **478 passed / 0 failed / 3 ignored**（6.87s） |
| `cargo test --test beauty` | **12 passed / 0 failed / 1 ignored**（1.10s） |
| `cargo test --test slim_items` | **7 passed / 0 failed**（0.21s，本轮新增） |
| `pnpm test` | **10 files / 118 tests passed**（基线 81 + 新增 37） |
| `pnpm typecheck` | **exit 0** |
| `pnpm build` | **成功**（dist 产物 426.87 kB / gzip 129.51 kB） |

### 禁用词检查

```
grep -rn "Provider|LEVEL|置信度|关键点|SHA256|ONNX|语义分割|Tensor|蒙版算法|许可证" \
  src/components/slim src/components/TopBar.tsx src/components/StatusBar.tsx \
  src/components/ui/Switch.tsx src/components/ui/Slider.tsx src/state/slim.ts src/App.tsx
```

**0 命中**（仅 `src/state/slim.test.ts` 的**否定断言词表**里出现这些字符串，用于断言 UI 文案不含它们）。
另加一道运行时防线：`plainMessage()` 把后端错误串里的 `模型/Provider/ONNX/Tensor/蒙版/关键点/置信度/LEVEL/SHA256/推理/算法/Rust/ms` 统一替换为白话（`llm` 类错误也不会漏词），并有测试锁定。

---

## 3. 六项逐项证据（「点一下像素必须变」）

### 3.1 引擎级（自动化，`cargo test --test slim_items -- --nocapture`）

探针为合成人像，全部走 `pipeline::render`（预览与导出同一条管线），`faces` 非空。
`mean |Δ|` = 指定区域内逐像素三通道平均绝对差（单位 0..1，×255 即灰阶差）。

| 项 | 写入参数（强度 100 剖面） | 断言区域 | 实测 `mean |Δ|` | 判据 |
|---|---|---|---|---|
| ① 磨皮美肤 | `skinSmooth=70, texture=55, detail=28` | 脸颊 32×28 | **0.02293**（≈5.85/255） | > 0.002 |
| ② 祛痘祛斑 | `blemish=90, spots=60, blackheads=35` | 痘印 34×46 | **0.01062**（≈2.71/255） | > 0.002 |
| ③ 眼睛美化 | `darkCircles=90, eyeLines=70` | 眼下 120×26 | **0.00892**（≈2.27/255） | > 0.002（仓库首次断言） |
| ④ 肤色提亮 | `whitening=50, toneEven=65, dullness=45, redness=40` | 肤色不均 76×66 | **0.01705**（≈4.35/255） | > 0.002 |
| ④ 其中的 `dullness` | 单独 `dullness=55` | 同上 | **0.00484**（≈1.23/255） | > 0.0005（仓库首次断言） |
| ⑤ 光影调整 | `exposure=18, contrast=-8, highlights=-12, shadows=20` | 整图 | **0.12425**（≈31.7/255） | > 0.005 |
| ⑥ 照片色调 | `temperature=15, tint=8, saturation=-10, vibrance=12` | 整图 | **0.01011**（≈2.58/255） | > 0.002 |

另外两条：
- `neutral_params_leave_pixels_untouched`：中性参数渲染后 `mean |Δ| == 0`（回到中性 → 预览字节级复用原图的前提）。
- ① 与 ② 额外断言「蒙版外（画面左侧 60px 竖条）`mean |Δ| < 0.0001`」，证明不是整图糊一层。
- `forehead_lines_and_nasolabial_are_not_part_of_the_six_items`：Q1 裁决落地后，这两项为 0 时整图 `mean |Δ| == 0`。

### 3.2 界面级（自动化，`pnpm test`）

| 断言 | 位置 |
|---|---|
| 强度 0 六项全部空写入；强度 100 等于剖面 | `slim.test.ts › 六项映射` |
| 任意强度 `spots ≤ blemish`（36 档逐点检查，规避 `blemish.rs:45` 的 `max()` 吞并） | 同上 |
| 强度递增时每个字段不下降（单调） | 同上 |
| ③ 只写 `darkCircles/eyeLines`，`reshape` 不动 | 同上 |
| ⑤⑥ 无建议 → 空写入（不是写 0 偏移） | 同上 |
| 写入后反解回同一强度（0/17/40/60/88/100） | `反解强度` |
| 正负建议混合（`exposure +18 / highlights -12`）反解仍得 60 | 同上 |
| 一键：自然 < 精致 < 高级，高级不超过建议值 | `一键精修计划` |
| 一键：`applied == null` 且当前非 0 → `needsConfirm`（不静默覆盖） | 同上 |
| 一键：用户手改字段被保留并计入 `kept` | 同上 |
| 一次拖动只产生**一条**撤销记录；撤销后 `selectEffectUrl` 回到 `photo.previewUrl` | `store 契约` |
| 单项重置只影响该项 | 同上 |
| 关闭开关写 0 但记住强度，再打开回到原力度 | 同上 |

### 3.3 桌面端手工验收（③ ④ 也想亲自看一遍时）

1. `pnpm app:dev`，导入一张正面人像，等右栏提示消失（检测完成，六项由灰变亮）。
2. 切到「手动调整」，把 ⑥ 照片色调 从 0 拉到 60：第一次拖动会先出现「正在看这张照片的光影和色调…」，随后出现一句方向说明（如「AI 判断照片偏冷，向右会加暖」），画布颜色必须变。
3. 逐项把滑块从 0 拉到 60，观察 ① 毛孔变淡、② 痘印变淡（痣保留）、③ 黑眼圈变淡、④ 肤色更均匀。
4. 每项拉回 0 → 画布回到原图；按 `\` 按住可随时看原图；⌘Z 一步撤掉该项。
5. 导出一次，用任意看图工具与中性导出对比，肉眼可见差异（自动化侧已用 `mean |Δ|` 量化，见 3.1）。

---

## 4. 隐藏清单（入口去掉了，能力在哪还能用）

| 原入口 | 现在 | 原能力还能在哪里用到 |
|---|---|---|
| 顶栏 9 个 Tab（编辑/智能精修/预设库/RAW/色彩/性能/AI 人像/模型库/批量处理） | **移除**，改为「一键精修 ｜ 手动调整」双段控件 | — |
| 编辑（`Inspector` 20 个分组） | **不再渲染**（文件保留） | 六项覆盖了其中的六组；其余（细节/HSL/曲线/LUT/色阶/塑形/身材/背景/人脸信息）进「高级设置 → 旧调整清理」可发现可清除 |
| 智能精修（`SmartPanel`） | 隐藏 | 能力并入「一键精修」（改用更强的 `analyze_beauty` 双向分析） |
| 预设库（`PresetPanel`） | 隐藏（**不在菜单**，属任务书「复杂高级参数」暂缓） | 代码保留；已应用的 LUT 会被旧调整清理发现并清除 |
| RAW（`RawPanel`） | 隐藏 | **文件 → RAW 显影…**（右栏临时打开，带「返回修图」） |
| 批量处理（`BatchWorkspace`） | 隐藏 | **文件 → 批量处理…**（全宽临时打开，带「返回修图」） |
| 色彩（`ColorPanel`）/ 性能（`PerformancePanel`） | 隐藏 | 代码保留；`LEGACY_PANELS_ENABLED` 可整体回滚 |
| AI 人像（`AiPortraitPanel`）/ 模型库（`ModelHubPanel`） | 隐藏（禁用词重灾区，无法只靠文案清洗） | 代码保留；不提供菜单入口 |
| 瘦脸/大眼/鼻唇形变/瘦身/身材/换脸/AI 重绘 | **UI 入口全部移除** | 参数与渲染逻辑保留，老项目仍生效；可被旧调整清理一次性清除 |

旧调整清理的判定与清除有自动化保障（含一条**经项目层真实往返**的老项目 fixture 测试：
写入 `.lumi` 结构 → 读回 → 审计出 `reshape/body/background/hsl/lut/legacySkinSmooth/legacyLines` → 一键清除后审计为空）。

---

## 5. 关键实现决策

1. **预热检测是硬前提**。`pipeline.rs:187-197` 在 `faces` 为空时静默跳过美肤。两个入口在挂载时都调用 `ensureFaces()`（真实 `detect_faces`），并在检测完成前把六项置灰。`render.test.ts` 里有断言：没有检测结果时六项必须是 `disabled`，不能是可点的假按钮。
2. **一键走 `analyze_beauty`，不走 `applyRetouchSuggestions`**。后者 `slot.max()` 只增不减、无皮肤模型时只给固定模板（max 35）。
3. **`analyze_retouch` 的 `mode` 固定 `"standard"`**，档位系数只在 TS 侧乘一次，避免自然档 0.55×0.55≈0.30。
4. **⑤⑥ 的语义**：滑块 = 采用 AI 建议的强度。写入 `anchor + (suggested - current) × 档位系数 × K/100`，并受 §4.3 上限约束；无建议时该项禁用并给真实原因，不伪造方向。
5. **分析前把 ⑤⑥ 恢复到锚点**（`withAnchors`）：否则第二次精修时「已经采纳的值」会把建议吞掉，导致重复点「重新精修」越修越弱。这条同时消掉了「重复精修系数叠加」的隐患。
6. **两个入口只有一份数字**：滑块值一律由 `invertItemStrength(params)` 反解（最小二乘投影，兼容老项目的手工值），切换入口不丢编辑。
7. **零新增 Rust 命令**：`detect_faces` + `analyze_beauty` + `analyze_retouch` → TS `buildOneClickPlan()` → `applyExternalParams` 一次写入 = **一条撤销记录**。
8. **不给 `EditParams` 加字段**：`AppliedPlan` 只活在内存（`oneClick.applied`，按 `photoId` 校验），`.lumi` 零迁移；撤销后主动清掉记录，下一次精修先确认而不是静默覆盖。

---

## 6. 已知限制

1. **⑤⑥ 的方向不可手动反转（Q4 既定取舍）**。滑块只缩放幅度，方向来自真实分析。若 AI 判断「这张照片偏亮」，向右就是压暗；用户想反向只能自己去别处调——本版**没有**「50 = 不变、左右双向」的替代模式。无建议时该项禁用并说明「当前光影已在合理范围，无需调整」（真实结论，不是假按钮）。
2. **逐人脸参数被单滑块压缩的表达损失**。一张脸 14 个字段、N 张脸 N 套值；六项滑块只能表达「对所有人脸同时施加同一强度」。缓解措施：写入覆盖全部已检测人脸；高级设置保留逐人脸开关；老项目的差异化值在反解时显示「微调过」而不是被静默抹平。仍然无法用滑块表达「第 1 张脸磨皮重、第 2 张脸轻」这种组合。
3. **六项之下仍有不可单独关闭的子参数**（纹理保护、细节恢复、黑头、额头细纹、法令纹）。用户说「毛孔被磨没了」时只能整项关掉①。映射设计为随强度同向上调 `texture`，并有 Rust 断言纹理保留率不会被压到 0。
4. **额头细纹 / 法令纹彻底不可见**（Q1）。一键计划不写、手动六项不含；老项目里已有的值只能通过「高级设置 → 旧调整清理」看到并整体清除，**不能单独下调**。
5. **`AppliedPlan` 不持久化**。重启应用或重新打开项目后，「哪些字段是用户手改的」判定退化为一律先弹确认（不静默，但会多一次点击）。要持久化必须同时改 TS 类型 + `clampParams` + Rust 结构体（schema 变更），本轮明确不做。
6. **多人脸 / 侧脸 / 深肤色**：本轮按架构师 §4.4 只落地「写入全部人脸 + 显示真实人脸数量」，**未**实现「侧脸系数 ×0.7」与「深肤色 whitening 上限 50→35」这两条自适应降级（引擎自身的 Lab 局部均值 + 色度约束已在保底）。原因是需要把 `FaceMetrics` 的判据搬到 TS 侧再与 `analyze_face` 的阈值对齐，属于可独立迭代的增强项，本轮为避免与既有限制冲突而暂缓。
7. **浏览器预览模式**（非 Tauri 窗口）下没有后端，检测/分析/渲染都会失败：两个入口会显示「刚才没成功：…」并保持六项禁用。这是既有行为（旧面板同样如此），非本轮回归。
8. **旧面板仍含工程术语**。`SmartPanel`/`AiPortraitPanel`/`ModelHubPanel`/`ReshapePanel`/`FacePanel`/`SubjectMaskPanel`/`Inspector` 的文案里仍有「Provider / LEVEL / 关键点 / 置信度 / SHA256 / 语义分割」等词——按「不删底层代码、只隐藏入口」的约束保留。它们从两个主入口**不可达**；批量/RAW 两个菜单入口涉及的组件（`BatchWorkspace`/`RawPanel`/`PresetPanel`/`ColorPanel`/`PerformancePanel`）已确认**不含**禁用词。若验收要求「全仓零命中」，需要单独一轮文案清洗（会与 478 个 lib 测试的既有契约无关，但属于新范围）。
9. **`dullness` 的效果天然很弱**：只抬升 L\* < 62 的暗部，且按 `(62-l)` 线性收敛，因此在正常曝光的皮肤上几乎无变化（这正是它「安全」的原因）。自动化用的是暗皮肤合成图，实测 ≈1.23/255。

---

## 7. 与设计的偏离（逐条说明原因）

| # | 设计原文 | 实际实现 | 原因 |
|---|---|---|---|
| 1 | §4.1 ① 写 `texture=40+0.15S`、`detail=20+0.08S` | 改为线性 `剖面 × S/100`（S=0 → 0，S=100 → 55/28） | 文档同一节的验收要求「S=0 全 0」。若 S=0 时 `texture=40`，参数就不中性（`beautyIsActive` 为真），预览会走后端而不是字节级复用原图，破坏「回到中性 → url 回到 `photo.previewUrl`」，也会让「关掉磨皮」看起来还在磨 |
| 2 | §5.5 内存 `AppliedPlan` + 「applied == null 时先确认」 | 完全照做；额外：撤销后主动清空 `applied` | 撤销后参数已不等于记录值，若不清空，下一次「重新精修」会误判成「用户手改了 N 项」。清空后退化为「先确认」，仍然不静默 |
| 3 | §4.2 「锚点 = 该项开关打开瞬间的当前值」 | 锚点统一取「分析报告里该字段的 `current`」，并在每次分析前把 ⑤⑥ 恢复到锚点 | 分析本身就是用当前参数跑的，`current` 就是分析时的值；恢复锚点再分析可以避免「重复精修时档位系数叠加 / 建议被自己吞掉」 |
| 4 | §6.4 文案「请在『文件 → 检查环境』里修复」 | 未采用 | 文件菜单里没有「检查环境」这一项，写出来就是假入口。改为在失败态显示真实原因（已白话化）+「点『重试』再来一次」 |
| 5 | §2.1「预设库」隐藏但「菜单可回」 | 预设库**不进菜单**，彻底隐藏（代码保留） | 主理人 Q3 只裁定了批量与 RAW；LUT/曲线属于任务书点名的「复杂高级参数」暂缓项，留菜单入口与「隐藏 UI 入口」要求冲突 |
| 6 | §5.5 可选加固：把 `AppliedPlan` 持久化到项目 | 不做 | 需同时改 TS 类型 + `clampParams` + Rust 结构体 = schema 变更，与「`.lumi` 零迁移」冲突 |
| 7 | §4.4 侧脸 ×0.7、深肤色 `whitening` 上限下调 | 未实现（见已知限制 6） | 判据在 Rust 的 `analyze_face` 内部，搬到 TS 需要重新对齐阈值；本轮不动引擎数值（避免撞 `all_suggestions_stay_inside_documented_ranges`） |
| 8 | §9 T05 交付 `docs/slim-acceptance.md` | 人工验收步骤并入本报告 §3.3 | 避免两份清单漂移；验收方只需要看一份文件 |
| 9 | §9 T01 文件清单含 `src/state/editorStore.ts` 只加 `slimEntry` 两处 | 实际新增了 `slimItems` / `oneClick` / `slimReports` 与 6 个动作 | 六项状态与一键状态需要一个归属；放在 store 里可同时被两个面板与撤销栈复用，且不写进项目文件 |

---

## 8. 提交

见 `git log -1 --stat`（message：`feat(slim): 双入口界面与六项手动调整`）。
`git add` 只加了本轮相关路径：`src/**`、`docs/SLIMMING_*.md`、`src-tauri/tests/slim_items.rs`。

---

# 第二轮：QA 验收缺陷修复（D1–D6）

> 触发：`docs/SLIMMING_ACCEPTANCE.md`（QA 严过关独立复验，六项像素证据全部复现一致）
> 结论先行：**D1 已修（含 error 路径自动化测试）、D5 已修（清理范围收窄 + 手绘内容改为显式勾选）、D2 选「补证据」路线、D3/D4/D6 已处理。**

## 9.1 修改文件

| 文件 | 改动 |
|---|---|
| `src/state/editorStore.ts` | `SlimReport.report` 改为**可空**；`setSlimReportStatus` **不再虚构占位报告**；`applySlimItem` 在「没真正写入」时不更新记忆强度；删除死代码 `clearSlimReport`；`clearLegacyAdjustments(includeBrushwork?)` 增加参数 |
| `src/state/slim.ts` | 新增 `auditBrushwork()`；`auditLegacyAdjustments()` **把手绘内容摘出**；`clearLegacyAdjustments(params, { includeBrushwork })`；新增 `needsGlobalAnalysis()`（D1 判定表的唯一事实来源）；删除死代码 `hiddenBeautyFields` |
| `src/components/slim/ManualPanel.tsx` | 提示区抽成纯组件 `ManualNotice`；补 **error 分支 + 「重试」按钮**；⑤⑥ 的**开关**也走「先分析再写」；写入判定改用 `needsGlobalAnalysis` |
| `src/components/slim/OneClickPanel.tsx` | 失败态聚合 `slimReports[].error`；重试文案指向真实存在的按钮 |
| `src/components/slim/AdvancedSettings.tsx` | 手绘内容单列区块 + 「同时清除手绘内容（N 处）」复选框，**默认不勾** |
| `src/components/StatusBar.tsx` | 「参数已保存」→「参数还在（记得保存）」 |
| `src/styles/app.css` | `.advanced-check`、`.slim-error .mini-btn` |
| `src/state/slim.test.ts` | +9 例（D1 判定表 / 失败后无假写入 / 重试后生效 / D5 清理范围 / D6 合并窗口） |
| `src/components/slim/render.test.ts` | +2 例（失败态 UI 白话 + 重试；分析中/未分析提示） |
| `src-tauri/tests/slim_items.rs` | +1 例（`eye_lines` 单独作用的细纹证据）+ 细纹 fixture + `vertical_energy` |

## 9.2 D1（严重）：分析失败后 ⑤⑥ 永久失效且静默 —— 已修

**根因**：`setSlimReportStatus` 用空占位报告兜底 → `slimReports[p].report` 从「分析中」起恒为真值 → 面板以为已分析 → 空 `suggestions` → 全链路静默 no-op。

**修法（三处一起，任何一处单独都不够）**：

1. **数据结构**：`SlimReport.report: AnalysisReport | null`。`setSlimReportStatus` 只写 `report: previous?.report ?? null` —— **没有真实结论就一定是 null**，占位报告彻底删除。已有旧结论时保留（重分析失败不会把可用结论也搞丢）。
2. **判定收口**：新增 `needsGlobalAnalysis(id, slimReport)`（`slim.ts`）：「未分析 / 分析中 / 上次失败」一律返回 true。`ManualPanel` 的拖动与**开关**都用它决定「先分析再写」。
3. **UI 分支**：`ManualNotice` 补 `reportStatus === "error"` 分支 —— 白话原因 + **「重试」按钮**（`ensureGlobalReport(photo, true)`，真实重新分析）。

**用户现在看到什么（这是验收最关心的）**：

| 场景 | 现象 |
|---|---|
| 第一次拖 ⑤⑥ | 立刻出现「正在看这张照片的光影和色调… 一般几秒钟」，分析成功后按强度写入 |
| 分析失败 | 出现红色提示：「『光影调整』『照片色调』这次没看成：<白话原因>。其余四项不受影响，可以照常调整。」+ **「重试」按钮** |
| 失败后再拖 ⑤⑥ | **会真的重新发起分析**（不是静默 no-op）；再次失败会再弹一次提示 |
| 失败期间会不会写出脏参数 | 不会。写入前先分析，失败即 return；`applySlimItem` 也加了「没真正写入就不更新记忆强度」的护栏 |
| 重试成功 | 「重试」消失，滑块立即生效（一条撤销记录） |
| 一键入口 | 同一个失败原因也会显示在「一键精修」面板，主按钮/「重新精修」都会重新分析 |
| 人脸检测失败（同类死路） | 也补了「重试」按钮（`handleRetryFace` → 真实 `detect_faces`），不再是只能靠切入口重挂载 |

**测试**（此前 118 例零覆盖 error 路径）：
- `slim.test.ts › ⑤⑥ 分析失败的恢复路径（D1）` 4 例：`loading/error` 时 `report === null`；失败期间拖动**参数与撤销栈都不动**且不留假开关记忆；`setSlimReport` 回来之后同一滑块写入 10.8（exposure 18×0.6）且恰好一条撤销；placeholder 不会被当成「已分析」。
- `slim.test.ts › ⑤⑥ 在「未分析 / 分析中 / 上次失败」时都必须先重新分析（D1 判定表）`：6 条断言覆盖 `needsGlobalAnalysis`。
- `render.test.ts › 分析失败时给出白话原因与「重试」`：断言 error 提示含「重试」「其余四项不受影响」且**不**同时出现「正在看…」。

## 9.3 D2（一般）：③「眼下细纹」无像素证据 —— 选路线 (a) 补证据

**为什么不选改文案**：`eyes.rs:139-160` 的细纹分支代码是完整的（`line_smoothing` + 环带权重），只是原 fixture 里没有皱纹可平。改文案等于把引擎已有能力从 UI 上抹掉。

**新证据**（`item_eye_lines_alone_smooths_under_eye_wrinkles`，`eye_lines=70` 单独作用）：

| 指标 | 数值 | 断言 |
|---|---|---|
| 细纹环带 `mean |Δ|` | **0.00183**（≈0.47/255） | > 0.0015 |
| 竖向高频能量 | **0.02335 → 0.02194（ratio 0.940，降 6%）** | < 0.97（「细纹对比度被压下去」） |
| 脸外 / 下巴 | 0.00000 / < 0.002 | 不外溢 |

**fixture 的关键坑（值得记录）**：一开始用「1 行深 3 行浅」的硬条纹，`eye_lines` 只有 0.00035——因为 `line_smoothing` 走的是**边缘保持**的引导滤波（`eps=0.0012`，半径 2），硬边被当成结构原样保留（能量只降 1.5%）。改成**平滑余弦**（周期 6、幅度 15%）后才被识别成细纹，能量降 6%。这也解释了为什么这条证据天然比黑眼圈弱：`analyze_face` 对 `eye_lines` 只敢建议 ≤25，产品侧最大 70，本来就比 `dark_circles` 温和。

原 `item_eye_area_changes_under_eye_pixels` **未改动**，实测仍是 QA 复现过的 **0.00892**。

## 9.4 D3（一般）：状态条措辞 —— 已改

`StatusBar.tsx` 预览失败：「预览没出来，参数已保存」→ **「预览没出来，参数还在（记得保存）」**。本项目没有自动保存，这句不再误导。

## 9.5 D4（一般）：死代码 —— 已清

- `clearSlimReport`：**删除**。D1 用「`report` 可空」解决后，不再需要额外的「清空」动作；留着就是新的死代码。（QA 提示它可作 D1 的药，但两处都做会产生「清空后又立刻写 error」的冗余写入。）
- `hiddenBeautyFields`：**删除**。`advancedLegacy` 清单已经覆盖「额头细纹 / 法令纹」，这个导出没有任何调用点。

## 9.6 D5（一般，数据丢失风险）：清理范围收窄 —— 已修

- `auditLegacyAdjustments()` 现在**只含隐藏的旧调整**：面部塑形 / 身材 / 背景 / 分色 / 曲线 / 风格滤镜 / 旧全图磨皮 / 额头细纹·法令纹 / 清晰度类 / 色阶类。
- 手绘内容（`manualMask`、`subjectMask` 笔触、`repairPoints`）改用 `auditBrushwork()` **单独披露**，并在高级设置里给一个**默认不勾**的复选框：「同时清除手绘内容（笔触 N 段、点选修复 N 个，共 N 处）」。
- `clearLegacyAdjustments(params, { includeBrushwork })`：默认 `false` 时**原样保留**手绘内容；确认按钮的 toast 也会区分「旧调整已清除（手绘内容保留）」与「旧调整与手绘内容已清除」。
- 测试：`旧调整清理不碰手绘内容：默认保留笔触与点选修复点（D5）` —— 先断言审计里没有 `manualMask/subjectMask/repairPoints`、`auditBrushwork` 计数为 2/1，再断言默认清理后三样都还在、`clarity` 归零，最后显式 `clearLegacyAdjustments(true)` 才清空。

## 9.7 D6（观察）：撤销粒度写成显式契约 —— 已补

`连发写入在 600ms 合并窗口内只留一条撤销记录，超过窗口才开新条目（D6）`：
500ms 后第二次写入 → 仍 1 条；再过 700ms 写入 → 2 条；换到另一项（合并键 `slim:<id>` 不同）→ 3 条。
**明确契约：一次连续拖动＝一条撤销；中途停顿 >600ms 会切成两条**（与旧界面一致，属既有 `COALESCE_WINDOW_MS` 机制，本轮不改）。

## 9.8 第二轮测试结果（本机实测）

| 命令 | 结果 |
|---|---|
| `cargo test --lib` | **478 passed / 0 failed / 3 ignored**（6.69s） |
| `cargo test --test beauty` | **12 passed / 0 failed / 1 ignored**（1.10s） |
| `cargo test --test slim_items` | **8 passed / 0 failed**（0.20s，+1 例细纹证据） |
| `pnpm test` | **10 files / 128 tests passed**（第一轮 118 + 新增 10） |
| `pnpm typecheck` | **exit 0** |
| `pnpm build` | **成功**（428.32 kB / gzip 130.04 kB） |

禁用词：新增/改动的 UI 源文件仍 **0 命中**（仅 `analysis.ts` 的替换词表与测试的否定词表里出现）。

## 9.9 本轮新增的已知限制

1. **⑤⑥ 的「上次失败」状态会跨照片保留**（按 `photoId` 存），切回来仍显示失败提示 —— 这是有意的：让用户知道这张照片还没分析成功，而不是假装可以调。
2. **重试是「整条分析重跑」**，没有做「只重试某一步」的细粒度控制（`analyze_retouch` 是单次命令）。
3. **细纹证据仍比黑眼圈弱一个量级**（0.00183 vs 0.00892），这是引擎的安全阀（`eye_lines` 建议上限 25），不是缺陷；文案保留「黑眼圈和眼下细纹」，两半边现在都有像素断言。
