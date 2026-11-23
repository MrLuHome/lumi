# 精简版验收报告（SLIMMING ACCEPTANCE）

- **验收对象**：`cffaa19` `feat(slim): 双入口界面与六项手动调整`（19 files, +4541/−112）
- **基线**：`452278e`
- **验收人**：严过关（QA）
- **验收方式**：全部数字由本人在本机**独立复跑**，不采信工程师自报；六项像素证据另写独立 harness 复核（`/tmp/qa_slim_check.rs`，不依赖仓库内测试代码）
- **仓库改动**：本报告只新增本文件，未修改 `src/**`、`src-tauri/**`

---

## 0. 一句话结论

**可以交付用户试用（YES），但有一个「严重」缺陷必须在放量前修**：任何一次光影/色调分析失败后，⑤⑥ 两个滑块会**永久失效且不报错**，界面还提示「点一下再试」——这与「禁止假按钮」直接冲突。详见 D1。

---

## 1. 复跑数字 vs 自报数字对照

| 命令 | 工程师自报 | 本人复跑 | 结论 |
|---|---|---|---|
| `cargo test --lib` | 478 passed / 0 failed | **478 passed / 0 failed / 3 ignored**（7.67s） | 一致 |
| `cargo test --test beauty` | 12 / 1 ignored | **12 passed / 0 failed / 1 ignored** | 一致 |
| `cargo test --test slim_items` | 7 / 0 | **7 passed / 0 failed**（0.20s） | 一致 |
| `pnpm test` | 10 files / 118 tests | **10 files / 118 tests passed** | 一致 |
| `pnpm typecheck` | 通过 | **0 error** | 一致 |
| `pnpm build` | 通过 | **通过**（index 426.87 kB / gzip 129.51 kB，452ms） | 一致 |
| `cargo test --test beauty_real` | 未自报 | **4 passed / 0 failed**（27.45s，真实样张） | 补跑通过 |
| `cargo test --test engine` | 未自报 | **10 passed / 0 failed / 1 ignored**（导出/渲染管线） | 补跑通过 |
| `cargo test --test reshape_persistence` | 未自报 | **3 / 0** | 补跑通过 |
| `cargo test --test round9_1_persistence` | 未自报 | **9 / 0** | 补跑通过 |
| `cargo test --test round10_changeset` | 未自报 | **11 / 0** | 补跑通过 |
| `git diff 452278e cffaa19 --stat -- src-tauri/src` | 0 行改动 | **输出为空**（`src-tauri/` 下唯一新增是 `tests/slim_items.rs`） | 一致 |

**结论：自报数字全部属实，无夸大。**

---

## 2. 逐条验收表

### 验收项 1：两个入口 + 功能清单 —— **PASS**

| 产品要求 | 证据 | 判定 |
|---|---|---|
| 一键精修入口：一键按钮 | `OneClickPanel.tsx:290-298`（`slim-primary`） | PASS |
| 自然/精致/高级，默认自然 | `slim.ts:51-57` `ONE_CLICK_MODES` + `DEFAULT_ONE_CLICK_MODE="natural"`；`render.test.ts:63-65` 断言三档文案 | PASS |
| 一个整体强度滑块 | `OneClickPanel.tsx:219-228`，0–100，默认 100（`slim.ts:101`） | PASS |
| 原图对比 | `TopBar.tsx:176-184` + `OneClickPanel.tsx:312-320` → `toggleCompare`（split） | PASS |
| 应用/撤销/重新精修 | `OneClickPanel.tsx:289-325`：主按钮 + 「重新精修」(force) + 「撤销」(`canUndo` 绑定真实历史栈) | PASS |
| 手动入口只有六项 | `SIX_ITEM_ORDER`（`slim.ts:46`）、`ITEM_META` 六条；`ManualPanel.tsx:125` 只遍历这六项 | PASS |
| 六项名称完全一致 | 磨皮美肤 / 祛痘祛斑 / 眼睛美化 / 肤色提亮 / 光影调整 / 照片色调（`slim.ts:124-173`） | PASS |
| 每项：开关 + 0–100 + 单项重置 | `SixItemRow.tsx:36-66`（Switch / Slider min0 max100 step1 / 恢复默认按钮） | PASS |
| 高级设置折叠 | `AdvancedSettings.tsx:26` `useState(false)`，默认收起 | PASS |
| 右栏只显示当前入口 | `App.tsx:583-586`：`slimEntry === "oneclick" ? OneClickPanel : ManualPanel` 二选一 | PASS |

### 验收项 2：暗色 / 照片为主 / 中文白话 / 禁用词 —— **PASS（视觉目视未做，见 §6）**

- 暗色：新增 479 行 CSS 全部走 `--bg-app / --bg-panel / --line` 等深色 token，`git diff` 未引入任何 `#fff`/`white` 硬编码背景。
- 照片为主：三栏结构 `LibraryRail + CanvasStage + 右栏(340px 级窄面板)`，右栏不再是旧的 `Inspector` 多分组长表单。
- 中文白话：六项 hint 全部白话（"让皮肤更细腻，同时尽量留住毛孔" / "淡掉小痘印和小斑点，痣会保留"），`slim.test.ts:509` 有禁用词断言。
- **禁用词 grep（可达路径：`src/components/slim/**`、`TopBar.tsx`、`StatusBar.tsx`、`App.tsx`）**：`置信度 / 语义分割 / 关键点 / Tensor / LEVEL / Provider / SHA256 / ONNX / 模型 / 蒙版 / 算法 / 推理` **零命中**（仅命中 `analysis.ts` 的替换词表与注释、`render.test.ts` 的 BANNED 列表、`App.tsx:444` 一条开发注释——都不是用户可见文案）。
- 关键清理已验证：`StatusBar.tsx` 删掉了 `人脸 · {faceResult.model}` 和 `GPU · {device}`，`App.tsx:362-370` 把 "Provider/模型版本/LEVEL 3/置信度" 三条提示改写成白话。
- `plainMessage()` **没有掩盖可操作错误**：词表只替换工程术语（`蒙版/关键点/置信度/ONNX/Tensor/模型/推理/算法`）和耗时单位，`文件不存在 / 磁盘已满 / 权限不足 / 路径` 这类用户该看的词一条都没被吃掉。`render.test.ts:124` 用真实样例（"人脸模型不可用：ONNX 推理失败，Provider level 3 置信度 0.2，用时 812 ms"）验证替换后仍保留"真实原因"。
- **记录（不计 FAIL）**：仅 `src/state/paramGroups.ts:36-43` 的 `OBJECT_LEVEL_GROUPS` 标签含「手动人物蒙版 / 手动蒙版与修复点」，该文案会渲染在**批量处理面板**的「对象级参数」折叠区里。批量面板按裁决 Q3 保留给专业用户，且不在主入口，故只记录。工程师报告里把该风险写成"BatchWorkspace/RawPanel/**PresetPanel/ColorPanel/PerformancePanel** 已确认不含禁用词"——后三个面板其实**已完全不可达**，这句表述不准确（无害）。

### 验收项 3：对比/撤销重做 + 禁止假按钮/假进度/模拟结果 —— **部分通过（D1 例外）**

| 检查 | 证据 | 判定 |
|---|---|---|
| 对比、撤销、重做 | `TopBar.tsx:186-201` 绑定真实 `canUndo/canRedo`；`OneClickPanel` 的撤销调 `undo()` | PASS |
| 一切真实调用后端 | `analysis.ts:20/39/68` → `detectFaces / analyzeBeauty / analyzeRetouch`（`invoke` 真命令）；无新增 Rust 命令 | PASS |
| 无 `setTimeout` 伪造进度 | 全仓 grep：`src/components/slim/**`、`src/state/slim.ts`、`App.tsx`、`TopBar.tsx`、`StatusBar.tsx` **零** `setTimeout/setInterval/模拟/fake`；`App.tsx:64` 的 `Math.random()` 是保存请求 id（原有代码） | PASS |
| 进度条是真实事件 | `OneClickPanel.tsx:239-250` 用 `beautyProgress ?? faceProgress`（后端 `emit("人脸检测",0.08)`/`emit("皮肤分析",..)` 驱动）；无进度时用 `is-indeterminate` 条纹，**不编造百分比** | PASS |
| 六项在检测完成前 disabled | `ManualPanel.tsx:128` `unavailable = noFace \|\| !detection` → `SixItemRow disabled`；`detection` 只由 `setFaces(detect_faces 结果)` 写入（`analysis.ts:22`），**不是定时器也不是默认值** | PASS |
| 无照片/无人脸给真实原因 | 无照片：`panel-empty` "先拖一张照片进来"；无人脸：`OneClickPanel.tsx:254-258` "这张照片里没找到人脸，一键精修只会调整光影和色调"，且该承诺是真的（`commands/beauty.rs:75-140` 零人脸时返回空 `faces`，不报错，后续整图分析照跑） | PASS |
| **分析失败后 ⑤⑥ 仍可用** | **FAIL —— 见 D1** | **FAIL** |

### 验收项 4：暂缓功能 UI 隐藏但底层不删 —— **PASS**

- 9 个工作区 Tab 全删：`TopBar.tsx` 原 `workspace-switch` 的 9 个按钮被替换为 2 个入口按钮，全仓 `setWorkspace` 的调用点只剩 `TopBar` 的菜单回调与 `SmartPanel.tsx:312`（面板本身已不可达）。
- 可达性实测：只有 **2 个**旧面板可达（批量 / RAW，经「文件」菜单）；`smart / presets / color / performance / ai / models` 六个已**完全不可达**（App 里保留了渲染分支，但没有 UI 能把 `workspace` 设成这些值）。
- 旧面板文件全部仍在仓库且仍被 `App.tsx` import（`SmartPanel/PresetPanel/RawPanel/ColorPanel/PerformancePanel/AiPortraitPanel/ModelHubPanel/BatchWorkspace`），`Inspector.tsx` 也仍在，只是不再渲染 → 底层未删。
- 瘦脸/大眼/鼻唇形变等塑形入口随 `Inspector` 一起不可达；`foreheadLines / nasolabial` 底层仍在：本人实测 `forehead_lines=80, nasolabial=80` 时整图 `mean|Δ| = 0.00006`（**>0，底层还在渲染**），而 UI 无任何可调入口。
- 「批量/RAW 菜单项不是死菜单」：`TopBar.tsx:105-113` 生成菜单项 → `onLegacyWorkspace(setWorkspace)` → `App.tsx:583` 渲染 `LegacyHost` 包住真面板，并带「返回修图」按钮（`App.tsx:76-101`）。唯一限制是 `disabled: !photo`（没照片时灰掉，合理）。

### 验收项 5：不破坏保存 / 撤销重做 / 导出 —— **PASS**

- `EditParams` schema 未动：`src/state/params.ts`、`src-tauri/src/image_engine/params.rs` 均**不在**本次 19 个改动文件里。
- 项目保存/恢复：`src/state/persistence.test.ts` 10 tests 全绿；`cargo test --test reshape_persistence` 3/0、`round9_1_persistence` 9/0、`round10_changeset` 11/0 全绿。
- 导出：`cargo test --test engine` 10/0（含导出管线），`TopBar` 只是把「导出」按钮换了位置，实现与命令未改。
- 撤销重做：`applyExternalParams`（一键，`editorStore.ts` 内）与 `apply(..., "slim:<id>")`（手动）都走原有的 `pushHistory`，没有绕开历史栈。
- 「重置全部参数」没有丢：顶栏图标删了，但「编辑」菜单里仍有 `{ label: "重置全部参数" }`（`TopBar.tsx:129`）。

### 验收项 6：主理人裁决 ①–⑥ —— **PASS**

| 裁决 | 证据 | 判定 |
|---|---|---|
| ① 额头细纹/法令纹不展示、一键 plan 不写入 | `buildOneClickPlan` 只遍历 `SIX_ITEM_ORDER`+`ITEM_META[*].beauty`，而这两项不在任何 `beauty` 列表里；`slim.test.ts:366` 专门断言；`slim_items.rs:282-294` 断言值为 0 时整图 `mean|Δ| == 0` | PASS |
| ②「高级」档不突破引擎建议上限 | `MODE_FACTOR.advanced` 全为 1（`slim.ts:220`），`buildOneClickPlan` 里 `capBeauty(key, peak*factor*overall)` 上限是 `BEAUTY_CAP`；`slim.test.ts:348/356` 断言三档单调且高级档 == 建议值（`advanced ≤ 1`） | PASS |
| ③ 批量/RAW 保留在「文件」菜单 | 见验收项 4 | PASS |
| ④⑤⑥ 滑块=采用建议强度 | `buildAdjustWrite: 值 = 锚点 + 建议增量 × S/100`（`slim.ts:290-306`） | PASS |
| ④⑤⑥ 未分析时首次拖动真实触发分析 | `ManualPanel.tsx:59-78`：`itemNeedsAnalysis(id) && !slimReport?.report` 时 `await ensureGlobalReport(photo)` 再写；**但失败后不再重试 —— 见 D1** | 部分通过 |

---

## 3. 六项像素证据：独立复核

### 3.1 先审「这些断言能不能失败」

| 疑点 | 复核结论 |
|---|---|
| fixture 是否 git 跟踪 / 会不会被改图骗过 | **不存在图片文件**。`slim_items.rs:52-115` 全部在内存里合成（`Image16::from_pixel` + 确定性噪声 `((x*13+y*7)%9-4)*2600` + 硬编码斑点/暗沉/色块坐标），**不可被外部改图影响**，可重复 |
| ROI 是否真落在目标区域 | 是。脸框 = x∈[72,328], y∈[68,356]。痘印画在 (238,212)(252,228)(236,240)，`SPOTS=(228,204,262,250)` 完全包住；眼下椭圆中心 (160,204)/(240,204) rx16/ry9，`UNDER_EYE=(140,192,260,218)` 包住；色块 210–290 × 210–280，`TONE_PATCH=(212,212,288,278)` 包住。**不是整图变化冒充局部** |
| 有没有「中性参数 == 0」对照 | 有，`neutral_params_leave_pixels_untouched`（`slim_items.rs:273-279`）断言 `EditParams::default()` 整图 `mean|Δ| == 0.0`。本人独立复现：**0.00000000**。这条是整个测试文件的噪声地板 |
| ①② 是否断言蒙版外 < 0.0001 | 有，但**只测了画面左侧 60px 竖条**（`(0,0,60,HEIGHT)`）。我补测了 `(340,400,400,500)` 与额头，也是 0 → 「糊全脸」风险确实不存在 |
| 阈值是否形同虚设 | 实测值 vs 阈值：① 0.02293/0.002（11×）、② 0.01062/0.002（5.3×）、③ 0.00892/0.002（4.5×）、④ 0.01705/0.002（8.5×）、⑤ 0.12425/0.005（25×）、⑥ 0.01011/0.002（5×）。**都有 4 倍以上余量，断言能失败**。唯一偏弱的是 `dullness` 单独项 0.00484 / 0.0005（9.7×，可接受） |

### 3.2 我自己写的 harness 复现结果

工具：`/tmp/qa_slim_check.rs`（自己重写 fixture 与测量函数，仅通过 `rustc --extern lumi_retouch_lib` 链接被测 rlib，**不 import 仓库测试代码**）。
`mean|Δ|` = 区域内三通道绝对差 / `CHANNEL_MAX` 的均值；`signed_luma` = 带符号亮度差（>0 = 变亮），这是仓库测试**没有**断言的方向性证据。

**③ 眼睛美化（工程师称「仓库首次断言」，最可疑）—— 已复现，结论成立**

| 区域 | mean\|Δ\| | signed_luma | 变化像素占比 |
|---|---|---|---|
| 眼下整体（复刻仓库 fixture，暗沉 ×0.78） | **0.00892** | **+0.00828** | 29.0% |
| 眼下整体（我自己写得更浅的 fixture，×0.86） | 0.00528 | +0.00458 | 29.0% |
| 左/右眼下单侧 | 0.01301 | +0.01152 | 72.1% |
| 额头（对照） | 0.00000 | 0 | 0% |
| 画面左缘 / 右下（对照） | 0.00000 | 0 | 0% |
| **整图** | 0.00008 | +0.00007 | **0.5%** |
| 仓库断言的「下巴区」(300,300,340,340) | **0.00000** | 0 | 0% |

→ **我复现出与仓库完全相同的 0.00892**，且方向正确（黑眼圈被提亮，不是随机噪声），且变化被限制在眼周（整图只有 0.5% 像素动）。

**③ 拆开看是谁在起作用（仓库没做，这是本次新发现）**

| 参数 | 眼下 mean\|Δ\| |
|---|---|
| 只 `dark_circles=90` | 0.00492 |
| 只 `eye_lines=70` | **0.00035**（几乎为 0） |

→ 「眼睛美化」这一项 90% 以上的像素变化来自 **dark_circles**；`eye_lines`（眼下细纹）在合成 fixture 上几乎测不出来，**因为 fixture 里根本没有皱纹**。测试名 `item_eye_area_changes_under_eye_pixels` 实际只验证了黑眼圈。这不是代码缺陷（真实样张有 `beauty_real::real_portrait_retouch_changes_skin_but_not_eyes` 兜底），但「淡一点黑眼圈**和眼下细纹**」这句文案里的细纹半边，本轮没有任何像素断言支持（见 D2）。

**④ 肤色提亮（含 dullness）—— 已复现，数字与仓库完全一致**

| 区域 | mean\|Δ\| | signed_luma | 变化像素占比 |
|---|---|---|---|
| 色块内（组合参数） | **0.01705**（与仓库一字不差） | +0.01678 | 55.4% |
| 色块外 | 0.00107 | +0.00103 | 7.2%（美白/泛红是全脸肤色，合理） |
| 只 `dullness=55`，色块内 | **0.00484**（与仓库一字不差） | +0.00483 | 49.7% |
| 只 `dullness=55`，色块外 | **0.00000** | 0 | 0% |

→ dullness 确实只提亮暗沉区域，方向正确（变亮），且不外溢。

**① 磨皮是否糊全脸 —— 没糊**

| 区域 | mean\|Δ\| | 变化像素占比 |
|---|---|---|
| 右脸颊 | 0.02144 | 86.5% |
| 额头 | 0.02264 | 88.9% |
| 画面左缘（脸外） | **0.00000** | 0% |
| 整图 | 0.00234 | 11.6% |

→ 磨皮覆盖整张脸的皮肤（额头也动，属正常皮肤蒙版范围），脸外严格 0。

**补充验证：`faces` 为空时美肤被静默跳过（假按钮风险的根源属实）**
`pipeline::render(..., &[])` 且 `skinSmooth=70` 时整图 `mean|Δ| = 0.00000000`。所以 `ManualPanel` 那条「检测完成前 disabled」的防线是**必须的、也是真的起作用**的。

### 3.3 关于「跨语言锁」

`ITEM_PROFILE`（`slim.ts:180-187`）与 `slim_items.rs` 里手写的 `BeautyParams` 数值目前**完全一致**（skin 70/55/28、blemish 90/60/35、eyes 90/70、tone 50/65/45/40）。但 Rust 测试是**手抄**这份剖面的，两边没有任何自动化断言把 TS 常量与 Rust 测试参数绑在一起——改 `ITEM_PROFILE` 不会让 `slim_items.rs` 变红。这是残留的漂移风险（审计文档也提过 N7 跨语言锁缺口），本轮未引入新问题。

---

## 4. 撤销粒度

| 检查 | 证据 | 判定 |
|---|---|---|
| 一次拖动 = 一条撤销记录 | `applySlimItem` → `apply(next, "手动调整·X", "slim:<id>")`，`coalesceKey` 相同 + `COALESCE_WINDOW_MS=600` 内不新开条目；同一机制的 `updateParam` 已有 2 条测试（`editorStore.test.ts:70-110`，含"超过窗口后开新条目"） | PASS（有前提，见 D6） |
| 「应用一键 plan」= 一条 | `applyExternalParams` 内部只 `pushHistory` 一次，并把 `lastEdit` 置 null，保证下一次手动拖动一定开新条目 | PASS |
| `applied == null` 时重新精修会确认 | `needsConfirm = applied === null && conflicts > 0`（`slim.ts:696`）；`slim.test.ts:386` "没有 applied 记录且当前已有调整时要先确认，不静默覆盖" 断言为 true；`OneClickPanel.tsx:267-285` 弹出「继续覆盖 / 取消」 | PASS |
| 撤销后不会静默再覆盖 | `OneClickPanel.tsx:164-169`：`undo()` 后立刻 `setOneClickApplied(null)` | PASS |

---

## 5. 缺陷清单

### D1（严重 / 阻断放量）：⑤⑥ 分析失败一次后滑块永久失效，且提示"点一下再试"永远无效

- **位置**：`src/components/slim/ManualPanel.tsx:62`、`src/components/slim/ManualPanel.tsx:72`、`src/components/slim/ManualPanel.tsx:120-122`、`src/state/editorStore.ts:816-846`（`setSlimReportStatus`）、`src/state/editorStore.ts:848`（`clearSlimReport` **全仓无调用点**）
- **复现**：
  1. 打开一张有人脸的照片 → 切「手动调整」→ 拖「光影调整」；
  2. 让 `analyze_retouch` 失败一次（模型缺失 / 照片被移走 / 后端报错）；
  3. 界面弹出「刚才没成功：…。点一下再试。」；
  4. **再拖一次 → 滑块弹回 0，什么都不发生，也没有任何提示**；此后该照片的「光影调整」「照片色调」永久失效，只有重启应用能恢复。
- **根因**：`analysis.ts:66` 在请求**之前**就调 `setSlimReportStatus(photo.id, "loading")`，而 `editorStore.ts:823` 用 `report: previous?.report ?? <空占位报告>` 兜底，于是 `slimReports[photo.id].report` 从"分析中"开始就恒为**真值对象**。`ManualPanel.tsx:62` 用 `if (!itemNeedsAnalysis(id) || slimReport?.report)` 判断"已经分析过"，失败后照样走"直接写入"分支；而空占位报告里 `suggestions: []` → `tonalDeltas()` 返回 `{}` → `buildAdjustWrite()` 返回 `{}` → `writeSixItem()` 原样返回 → `apply()` 里 `paramsEqual` 直接 return。**整条链路静默无操作**。
- **期望 vs 实际**：期望「点一下再试」能重试，或至少明确告知这次失败；实际是**点了永远无效，且不再报错**（`reportStatus === "error"` 在 ManualPanel 里没有任何渲染分支，只有 `"loading"` 有）。
- **加重情节**：`hasReport = Boolean(slimReport?.report)`（`OneClickPanel.tsx:64`）同理会把占位报告当成"已分析"，失败后仍显示「已经看过这张照片的光影和色调」。
- **建议修法（不动架构）**：① 占位报告不要写进 `report` 字段（改成 `report: null`，`ManualPanel` 判 `reportStatus === "ready"`）；或 ② 在 `analysis.ts` 的 catch 里调 `clearSlimReport(photo.id)`；同时给 `reportStatus === "error"` 补一条可见提示。三处任选其一即可让"点一下再试"成立。
- **必须有测试**：`setSlimReportStatus(photo.id,"error")` 之后再拖 ⑤⑥ 应重新触发一次分析（当前 118 个用例里**没有任何** error 路径覆盖，`grep setSlimReportStatus` 在测试里 0 命中）。

### D2（一般）：③ 眼睛美化的像素断言只覆盖了黑眼圈，"眼下细纹"无证据

- 位置：`src-tauri/tests/slim_items.rs:196-211`
- `eye_lines=70` 单独作用时眼下 mean|Δ| 仅 **0.00035**（低于该测试自身的 0.002 阈值 6 倍），测试名与断言实际只验证了 `dark_circles`。fixture 里没有皱纹纹理，所以测不出来是必然的。
- 影响：UI 文案承诺"淡一点黑眼圈**和眼下细纹**"，细纹那半边在合成样张上无像素证据（真实样张由 `beauty_real` 兜底）。**不建议阻断**，建议后续给 fixture 加一组横向细纹条纹再断言一次。

### D3（一般）：状态条在预览失败时声称"参数已保存"

- 位置：`src/components/StatusBar.tsx:113` `预览没出来，参数已保存`
- 事实：本项目**没有自动保存**，参数只在用户 ⌘S 后才落盘。用户看到"参数已保存"直接关窗口，虽有 `setUnsavedChanges` 的原生确认兜底，但这句话本身是错的。
- 建议：改为"预览没出来，参数还在（记得保存）"或直接"预览没出来"。

### D4（一般）：两处死代码

- `src/state/editorStore.ts:848` `clearSlimReport` 声明 + 实现，**全仓无调用点**（正是 D1 的修法之一，说明作者本来想用但漏了）。
- `src/state/slim.ts:883` `hiddenBeautyFields` 导出后**全仓无引用**（注释说是"给高级设置做诚实提示"，实际 `AdvancedSettings` 走的是 `auditLegacyAdjustments`）。

### D5（一般）：一键「清除旧调整」会连用户手绘的笔触/点选修复一起删掉

- 位置：`src/state/slim.ts:824-829`（把 `manualMask.strokes`、`subjectMask.strokes`、`repairPoints` 计入"旧调整"）→ `clearLegacyAdjustments`（`slim.ts:846-880`）把它们全部清空。
- 矛盾点：同一个「高级设置」面板里既提供画笔/橡皮/点选修复（`AdvancedSettings.tsx:84-159`），又提供一个把这些内容一键抹掉的按钮。虽然列表里披露了"皮肤选区笔触 N 处 / 点选修复 N 处"，且可 ⌘Z 撤销，但语义上"手动笔触"不是"看不见的旧调整"，容易被误清。
- 建议：把 `manualMask / subjectMask / repairPoints` 从"旧调整清理"里摘出来（它们本来就有各自的"清掉笔触/清掉点选"按钮）。

### D6（观察）：撤销粒度依赖 600ms 合并窗口

- `slim.test.ts:552` 的用例名叫"一次拖动只产生一条撤销记录"，但它只调用了**一次** `applySlimItem`，实际验证的是"一次调用一条"，并没有模拟拖动产生的连发事件。
- 真实的"一次拖动一条"依赖 `COALESCE_WINDOW_MS = 600`（所有滑杆共用，属原有机制，`editorStore.test.ts` 已覆盖）。**慢速拖动、中途停顿 >600ms 会切成多条撤销记录**——这与旧界面行为一致，不算回归，但"一次拖动只有一条"的说法要打折。

---

## 6. 未执行 / 覆盖盲区（如实声明）

1. **GUI 目视未做**：本轮是 CLI 验收，没有启动 `pnpm app:dev` 实机看暗色与排版。暗色/照片为主是按 CSS token 与结构判定（+`render.test.ts` 的 `renderToStaticMarkup` 断言无禁用词、组件可渲染）。**建议放量前人工看一眼 1280×800 与 1920×1080 两档布局。**
2. **真实样张上的六项观感未评估**：像素契约只证明"参数确实改像素、方向正确、不外溢"，不证明"效果好看"。`beauty_real` 4 个用例证明真实人像上皮肤变化且不动眼睛，但"自然/精致/高级三档观感是否可接受"属主观验收。
3. **多脸 / 侧脸 / 遮挡场景**：`slim_items.rs` 只有单张合成正脸；`beauty.rs` 有 `multi_face_parameters_do_not_leak_across_faces` 兜底，但精简版 UI 的多脸路径（`invertItemStrength` 取"最偏离"的那张脸、禁用脸也参与反解）没有专门断言。
4. **老项目往返**：`slim.test.ts:597-649` 走的是 `buildProjectDoc → buildProjectSnapshot → commitProjectSnapshot`，即**真实的序列化 → 反序列化**（不是内存里手搓参数），但**不落磁盘**（vitest 无 Tauri 环境）。足以证明 `clampParams` 没吞掉 reshape/body/background/hsl/lut，不足以证明真机写盘无碍——后者由 `round9_1_persistence`（9/0）与 `persistence.test.ts`（10/0）覆盖。

---

## 7. 结论

**能否交付用户试用：YES，带一个前置条件。**

- **可以放的**：界面结构、双入口、六项像素契约、禁用词清理、暂缓功能隐藏、老项目兼容、保存/撤销/导出回归，全部有可复现的证据，自报数字无一虚报。六项「真改像素」这件事我用独立 harness 逐项复现，③④ 数字与仓库完全一致（0.00892 / 0.01705 / 0.00484），方向正确、局部生效、不外溢——**这轮最可疑的部分反而是最扎实的部分**。
- **必须先修的**：**D1**。它让 ⑤⑥ 在"分析失败"这一常见场景下退化成假按钮，而且界面还骗用户"点一下再试"。修法很小（占位报告不写 `report`，或 catch 里调 `clearSlimReport`），加一条 error 路径的测试即可。
- **建议同批修**：D3（"参数已保存"这句不实）、D4（顺手清死代码，`clearSlimReport` 正好是 D1 的药）。
- **不阻断**：D2 / D5 / D6 可进下一轮。

---

*本报告所有复跑命令、独立 harness（`/tmp/qa_slim_check.rs`）与测量口径均可在本机重放。*
