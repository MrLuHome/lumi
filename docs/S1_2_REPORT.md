# LUMI RETOUCH · S1.2 一键精修「无明显效果」专项排查与修复报告

**基线：** S1.1 结束（`e8b9e24`，工作区 clean）
**日期：** 2026-11-23
**范围：** 仅解决一键精修无明显效果问题；不进入 S2，不新增算法模块，不改 UI。

---

## 1. 完整根因分析

### 1.1 排查方法

不预设根因。搭建**三阶段端到端诊断**（真实代码、真实照片、逐环节留痕）：

```
阶段1  cargo test phase_dump      检测 → 逐脸美肤分析 → 整图分析 → {stem}.input.json
阶段2  vitest 计划桥              真实 buildOneClickPlan/applyOneClickPlan → 三档参数 JSON
阶段3  cargo test phase_render    真实 pipeline::render_with_report → 逐环节状态 + 像素统计 + 对比图
```

驱动：`scripts/s1_2_one_click_diagnostics.sh`。三态：默认 `ignored`=未执行；缺照片 **panic**「缺少照片导致未执行」；断言不过=失败。诊断数据只含数值与 face_id，不含照片内容，不上传。

### 1.2 执行链结论：12 步全部真实执行成功

| # | 环节 | 证据（诊断实测） |
|---|---|---|
| 1 | OneClickPanel 点击 → `run()` | 代码追踪（`OneClickPanel.tsx:120-210`） |
| 2 | `ensureFaces` → `detect_faces` → FaceRepository | 每张照片 facesDetected=1（group=4） |
| 3 | `ensureBeauty` → `analyze_beauty` → `analyze_face` | 每脸返回真实 suggested/metrics，桥接成功消费 |
| 4 | `ensureGlobalReport` → `analysis::analyze(standard)` | report JSON 完整落盘 |
| 5 | `buildOneClickPlan` | 真实 TS：**fields=16，conflicts=0，kept=0**（首跑） |
| 6 | `applyExternalParams` | 静态追踪：params 写入 + history + `paramsKey` 变化 |
| 7 | 重新渲染触发 | 静态追踪：`usePreviewRenderer` deps=[params] → `renderPreview` → 缓存键含序列化参数，必 miss |
| 8 | `pipeline` → `apply_beauty` | `applyBeauty=true`，无「跳过美肤」警告 |
| 9 | `apply_face_retouch` | 每脸 `skinEngineRan=true` |
| 10 | `run_skin_retouch` | 每脸 `fusedPixels>0`（16k–161k） |
| 11 | `bilateral_grid_smooth` | 每脸 `bilateralGridRan=true`，`processCoverage=0.33–0.46` |
| 12 | 融合进入预览/导出 | `fusedPixels>0`；预览 `encode_canvas(rendered)`；导出与预览共用 `render_with_report`（静态） |

**§五 故障清单逐项结论：** 人脸分析非空 ✓；face_id 对齐（测试链 UUID 为空按 id 对齐，应用内由 FaceTracker 分配——无错配证据）；无手动覆盖（首跑 conflicts=0）；plan 有效（16 字段）；fields≠0；蒙版覆盖 0.7%–22.8% 非空；skinSmooth=32.5–72.3 非 0；参数完整传到 Rust（反序列化后逐字段打印）；apply_beauty 未跳过；run_skin_retouch 未提前返回；process_w 覆盖 0.33+ 非全 0；fuse 实际写像素；预览缓存键含参数（静态）；导出接收当次参数（静态）。

### 1.3 真正的根因：强度被重复计效，效果被压成非线性小量

链路全部执行、参数全部写入，**但有效平滑量被三处叠加衰减**：

| # | 根因 | 位置 | 数学后果 |
|---|---|---|---|
| RC1 | **强度计算两次**：`grid_pass` 内部已按 `strength` 混合，最终权重回注又乘 `blend = m × strength` | `bilateral_grid.rs` 细节回注循环 | 有效强度 = **s²**（自然档 0.32 → 0.10） |
| RC2 | **回注地板第三次耦合 strength**：`keep_base = 0.15+0.75(1−s) = 0.9−0.75s` 把网格已做的平滑按 `(0.1+0.75s)` 回吐 | 同上 | 净平滑 = **s × (0.1+0.75s)**：自然档仅约满量的 **11%**；且违反 `slim.ts` 记载的「强度与写入值线性」产品契约 |
| RC3 | **观感被全图提亮掩盖**：tonal 建议（exposure ±4 档内）全图 +1.7%~4.1% 亮度，而美肤对全图均值仅 0.0002 | 分解实验实测 | 用户主观=「只是亮了点/没变化」，恰是任务禁止的「提亮冒充美颜」观感 |

**分解实验（阶段3，natural 档，5 张照片一致）：**

| 分支 | 全图 mean\|Δ\| | 全图亮度漂移 | 皮肤 HF 保留 | 皮肤 mean\|Δ\| |
|---|---|---|---|---|
| beauty-only（只留美肤） | **0.0001–0.0002** | **+0.0001（≈0）** | 0.73–0.75（修复前 0.80） | 0.018–0.022 |
| tonal-only（只留调色） | 0.017–0.041 | **+1.7%–4.1%** | ≈1.0（不变纹理） | 0.020–0.040 |

修复前 beauty-only HF 保留 0.795–0.808、retouch 内皮肤均差 0.0088–0.0116——**修皮肤的量级小到在 13% 适应视图下不可见**，可见的主要是提亮。

**为什么测试照片上链路正常、用户却觉得没效果：** 不是某一步没跑，而是跑出来的皮肤修改量被 RC1+RC2 压小了一个量级，剩余可见变化又被 RC3 的提亮盖过。

### 1.4 修复内容（修的是计效逻辑，不是默认参数）

1. **RC1**：回注采纳系数改为只用蒙版 `blend = m`（羽化），强度只在 `grid_pass` 内生效一次。
2. **RC2**：新增 `BilateralGridParams.detail_keep_floor`，由 `texture/detail` 决定（`0.15+0.45×texture`，clamp 0.10–0.75）——**纹理保护归纹理旋钮，strength 不再进入地板**；净平滑 ≈ `s × (1−floor)`，对 s 线性。
   - 实现过程中地板方向曾写成 `1−texture`（与语义相反），被既有测试 `multi_face_parameters_do_not_leak_across_faces`（texture=20 应强磨皮）当场抓住，已修正为 `0.15+0.45×texture`。
3. **RC3 不删调色**：⑤⑥ 调色是有上限的合法建议（exposure 一键限幅 4/6.5/8）；修复后 beauty-only 证明**皮肤纹理变化独立存在且不依赖亮度**（luma 漂移≈0.0001），提亮不再独自承担「修图效果」。

**明确未做：** 未改任何滑杆默认值/剖面常数（ITEM_PROFILE、MODE_FACTOR、high_keep 公式均未动）；未新增算法模块；未进 S2。

---

## 2. 参数实际写入报告（真实 buildOneClickPlan 输出）

完整数据：`docs/assets/s1_2/{stem}.plan.{mode}.json`。示例（DSCF0030 痘痘组，overall=100）：

| 档位 | skinSmooth | texture | detail | blemish | spots | blackheads | whitening | toneEven | darkCircles | eyeLines | exposure | fields |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 自然 | **31.9** | 55.7 | 32.2 | 41.6 | 27.1 | 15.4 | 23.8 | 33.6 | 42.6 | 32.0 | +2.8 | 16 |
| 精致 | **49.6** | 55.7 | 32.2 | 64.7 | 42.2 | 24.0 | 37.0 | 52.3 | 66.3 | 49.7 | +4.0 | 16 |
| 高级 | **70.9** | 55.7 | 32.2 | 92.4 | 60.3 | 34.3 | 52.9 | 74.8 | 94.7 | 70.0 | +5.2 | 16 |

- 五张照片三档全部 fields=16、conflicts=0；texture/detail 按设计不随档位缩放（纹理保护底线），skinSmooth/blemish/tone 组线性分档。
- 瑕疵密度过低的组不硬塞祛痘（DSCF0057 good-skin 等 blemish 组按门控写入较小值）。
- 瘦脸计划：`plan_face_shape` 合并写入；超出安全范围自动降强度并入 warnings（good_skin 42%、group 24%——保护生效，非静默）。

---

## 3. 真实照片验收（5 类，全部断言通过）

语料：`beauty-validation`（授权验证照片）+ 仓库多人素材 `tests/fixtures/images/group.jpg`（4 脸）。

| 类别 | 照片 | 脸数 | 蒙版覆盖 | beauty-only 皮肤 HF 保留 | 自然档皮肤 mean\|Δ\| | 高级档皮肤 mean\|Δ\| | retouch 内 HF 保留 | 融合像素 | 保护泄漏 | 过磨风险 |
|---|---|---|---|---|---|---|---|---|---|---|
| 痘印 | DSCF0030 | 1 | 0.8% | 0.744 | 0.0372 | **0.0763 (2.05×)** | 0.663 | 24 875 | 0 | 0 |
| 皮肤粗糙 | DSCF0053 | 1 | 0.7% | 0.733 | 0.0581 | **0.1196 (2.06×)** | 0.649 | 21 332 | 0 | 0 |
| 肤色不均 | DSCF0029 | 1 | 0.7% | 0.747 | 0.0434 | **0.0900 (2.07×)** | 0.663 | 19 623 | 0.5 | 0 |
| 光线好肤质好 | DSCF0057 | 1 | 1.0% | 0.749 | 0.0462 | **0.0978 (2.12×)** | 0.656 | 30 663 | 0 | 0 |
| 多人 | group | **4** | 22.8% | 0.668 | 0.0486 | **0.0981 (2.02×)** | 0.600 | 161 640×4 | 0 | 0 |

**不是提亮的证明：**
- beauty-only 亮度漂移 **+0.0001**（≈0），而皮肤 HF 保留 0.73–0.75、皮肤 mean\|Δ\| 0.018–0.022 —— 纹理确实被修改；
- tonal-only 分支皮肤 HF 保留 ≈1.0（提亮不削纹理）→ 皮肤变化只能来自美肤算法；
- beauty-only 与 tonal-only 的皮肤高频保留差 > 0.01（断言锁定）。

**修复前后对比（DSCF0030，natural，beauty-only）：**

| 指标 | 修复前 | 修复后 |
|---|---|---|
| 皮肤 HF 保留 | 0.805 | **0.744** |
| retouch 内皮肤 mean\|Δ\| | 0.0088 | **0.0141** |
| 网格净采纳（数学） | s²×(0.1+0.75s) ≈ 1% | s×(1−floor) ≈ 22%（线性） |

**三档可辨识：** 皮肤 mean\|Δ\| 自然→高级 ≈ **2.0–2.1×**（每档约 1.4× 递增），HF 保留单调下降（0.75→0.71 等）；`tier_compare.png` 四联原图|自然|精致|高级。

**产品标准对照（§七）：** 自然档 skin HF 保留 0.73–0.75（更干净且**保留真实纹理**，group 更强至 0.67）；过磨风险全 0（不塑料）；保护泄漏 0/0.5（五官保护生效，眼睛/眉毛/鼻翼/嘴唇放大图逐区导出）；身份不变（保护权重内变化受控）。

---

## 4. 交付物路径

```
docs/assets/s1_2/                          （gitignore，不入库）
├── manifest.json                          5 类照片清单
├── {stem}.input.json                      阶段1：检测+分析 dump
├── {stem}.plan.{mode}.json                阶段2：真实计划（三档）
├── {stem}.params.{mode}.json              阶段2：最终 EditParams（三档）
├── {stem}.diag.json                       阶段3：逐环节诊断+像素统计
└── {stem}/
    ├── original.png / oneclick_{natural,refined,advanced}.png
    ├── compare.png（原图|自然） / tier_compare.png（原图|自然|精致|高级）
    └── zoom_{eye_left,eye_right,brow_left,brow_right,
              nose_wing_left,nose_wing_right,lips}_{100,200}.png
         （100%=原像素、200%=放大；左原图|右结果）
```

---

## 5. 修复文件列表

```
src-tauri/src/ai_engine/retouch/bilateral_grid.rs   RC1 回注只按蒙版；RC2 detail_keep_floor 解耦 strength（含方向修正）
src-tauri/src/ai_engine/retouch/mod.rs              FaceRetouchReport +5 诊断字段（grid_ran/fused/coverage/keep/delta）
src-tauri/src/ai_engine/retouch/skin_engine.rs      SkinRetouchOutput.grid_ran；process_w 全 0 时跳过网格并如实上报
src-tauri/src/image_engine/pipeline.rs              PipelineReport.faces 逐脸明细；美肤未执行时写明 warning（不再静默）
src-tauri/tests/s1_2_diagnostics.rs                 新：三阶段诊断（dump/render，#[ignore]+缺照片 panic）
src/state/s1_2_plan_bridge.test.ts                  新：真实 buildOneClickPlan 桥（无 S1_2_DIR 时 skip 不假绿）
scripts/s1_2_one_click_diagnostics.sh               新：三阶段驱动
package.json / pnpm-lock.yaml                       devDep @types/node（桥接测试类型）
docs/S1_2_REPORT.md                                 本报告
```

未改：UI、默认参数剖面、high_keep 公式、S2、GPU。

---

## 6. 自动化测试结果

| 套件 | 结果 |
|---|---|
| `cargo test --lib` | **580 passed / 0 failed / 3 ignored**（含 retouch 47：新增 strength 线性/回注地板断言仍全绿） |
| `cargo test`（全部集成） | **全绿 exit=0**（beauty 12、beauty_real 4、engine 10、p0 1、slim 12、g13/g16、round8–10 等） |
| `beauty::multi_face_parameters_do_not_leak_across_faces` | 修复地板方向后通过（曾当场抓住方向 bug） |
| `pnpm test` | **135 passed / 1 skipped**（桥接无环境变量时 skipped，不冒充通过） |
| `pnpm exec tsc --noEmit` | **0 error** |
| `phase_dump`（--ignored） | ok：5 类照片，group 4 脸，suggestions 非空 |
| 计划桥（--ignored 环境） | ok：5×3 档参数生成 |
| `phase_render`（--ignored） | **ok：5/5 全过**（含三档、非提亮、多脸断言） |
| `s1_1_real_photos`（--ignored） | **7/7 通过**（S1.1 验收不回归，skin_keep 0.69→0.64 属效果增强） |
| `s1_1_performance`（release --ignored） | 通过；45MP 总耗时 2.50s、峰值 2.25 GiB（无性能回归） |

---

## 7. Git

1d346f135a1dda06a760b1d7143fb6901ba712eb

---

## 8. 工作区状态

提交后 clean；`docs/assets/`（s1_1/s1_2 对比图）与 `beauty-validation/` 按 .gitignore 不入库。

---

## 9. 未完成 / 观察项

| 项 | 说明 |
|---|---|
| 应用内 UI 段（步骤 6/7/14）为静态代码追踪 | 后端 12 步为动态实测；前端 store→渲染触发为代码审查（缓存键含参数、deps 含 params），未在运行中的 UI 里插桩 |
| 用户此前截图中的「方块伪影」本轮未复现 | 5 张真实照片三档渲染均无矩形伪影；与本轮「无效果」是不同症状，留待拿到可复现样张后单独立项 |
| 13% 适应视图下细微平滑仍不显眼 | 100%/200% 对比清晰可见（验收图）；如产品要求适应视图也可辨，需另行定义视图级验收标准（属产品决策，非本轮） |
| `protected_leak=0.5` 提示（2/5 张） | 部分保护权重仍按比例允许轻微变化（0.02–0.04 级），非阻断；与 S1.1 结论一致 |

**本轮只解决一键精修无明显效果问题。完成后停止，不进入 S2，等待人工验收。**
