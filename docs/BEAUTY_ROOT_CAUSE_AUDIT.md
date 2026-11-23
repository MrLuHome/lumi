# LUMI RETOUCH 美颜链路根因审计

> 审计人：高见远（架构师）· 只读审计，**未修改任何源码**
> 仓库：`/Users/lu/Documents/lumi人像` · HEAD `88e949a`（含 slim 实现 `cffaa19` + 修复 `88e949a`）
> 目标：定位「照片修完和原图几乎没区别、只是稍微变亮」的真实根因。

**置信度口径**
- **已确证**：有直接代码/数值证据，可逐行复核。
- **高度怀疑**：代码路径支持，但缺真实样张或运行时数据，不能下最终结论。
- **待验证**：需要新增实验/外部核验才能定论。

---

## 0. 结论摘要（先看这一段）

| 排名 | 根因 | 类别 | 置信度 |
|---|---|---|---|
| 1 | **一键精修对皮肤的驱动量只有手动满档的 20%（自然）/36%（高级）**：AI 建议值本身保守（skinSmooth 典型 22–33、上限 55），TS 又乘档位系数 0.55/0.8/1.0，最终写入 13.8/20/25，而手动满档是 70。引擎没坏，是**喂给引擎的量太小**。 | 算法空转（驱动不足） | **已确证** |
| 2 | **全图调光压过局部美肤**：⑤⑥ 未经二次削弱直接写入 exposure 等参数，`grade` 对全图像素生效；皮肤算法只作用在几何蒙版内（QA 实测磨皮只动整图 11.6% 像素）。视觉上就剩「稍微变亮」。 | 算法空转（权重失衡） | **已确证** |
| 3 | **导入路径与批量路径不检测人脸**，`faces=[]` 时 `pipeline.rs:194-197` 静默跳过全部美肤。新 slim 面板已自带预热检测，但批量/其它路径仍然漏。 | 链路断点 | **已确证** |
| 4 | **皮肤蒙版是「5 点几何 + YCbCr 肤色似然」，不是语义分割**；`REGISTERED_SKIN_MODELS` 与 `SKIN_MODELS` 两张表都为空。这是**已知且如实标注**的能力缺口，不是冒牌 AI。 | 蒙版缺陷 + 模型缺口 | **已确证** |
| 5 | **测试通过 ≠ 效果可见**：`pnpm test` 128 例、`cargo test --lib` 478 例、`slim_items` 8 例全绿，但断言的是「参数确实改像素」，不是「幅度够用户看见」。 | 验证缺口 | **已确证** |
| 6 | 深肤色 / 暗光下 YCbCr 似然偏低，`alpha<0.12` 被清零 → 参数写了但几乎不生效。 | 蒙版缺陷 | **高度怀疑** |
| 7 | 老项目里 `enabled=false` 的人脸条目不会被一键重新启用 → 那个人脸永远不受精修影响。 | 链路断点 | **高度怀疑** |
| 8 | 合法可商用皮肤语义分割模型**当前一个都没有登记**；有候选（MediaPipe selfie-multiclass / MODNet），但都未完成许可四字段、SHA 与 tract 算子验证。 | 模型缺口 | **已确证**（缺口）／**待验证**（候选） |

**一句话**：不是引擎假、不是按钮假、不是预览显示原图；是**一键把引擎的旋钮只拧到 20%，却把全图曝光拧到 60–100%，所以用户只看见变亮**。

---

## 1. 一键精修端到端链路逐环查证

### 1.1 链路全景

```
[点击] OneClickPanel.run()                       src/components/slim/OneClickPanel.tsx:110-162
  ├─ ensureFaces(photo)                           src/components/slim/analysis.ts:16-29
  │    └─ detectFaces(path) → invoke("detect_faces")           src/lib/backend.ts:231
  │         └─ commands/faces.rs:53-89 → engine.detect → repo.store  ← 关键前置
  ├─ ensureBeauty(photo, force)                   analysis.ts:32-48
  │    └─ analyzeBeauty(path) → invoke("analyze_beauty")       backend.ts:244
  │         └─ commands/beauty.rs:74-151 → build_skin_mask → analyze_face（逐脸 suggested）
  ├─ ensureGlobalReport(photo, force)             analysis.ts:51-76
  │    └─ analyzeRetouch(path, baseline, "standard")           backend.ts:403
  │         └─ commands/analysis.rs:65-141 → analysis::analyze（整图 suggestions）
  ├─ buildOneClickPlan({...})                     src/state/slim.ts:640-712
  ├─ applyOneClickPlan(current, plan)             slim.ts:726-751
  ├─ applyExternalParams(photoId, next, label)    editorStore.ts:945-959
  │    └─ clampParams(cloneParams(next)) → pushHistory（一条撤销）
  └─ setOneClickApplied / setSlimItems / notify
      ↓ params 变化
usePreviewRenderer effect                         src/hooks/usePreviewRenderer.ts:50-119
  └─ renderPreview(path, params, requestId, ...) → invoke("render_preview")   backend.ts:74-89
       └─ commands/render.rs:160-251
            ├─ faces = repo.get(source)            render.rs:215-218
            ├─ pipeline::render_with_report(...)   render.rs:220-228
            ├─ encode_canvas(rendered)             render.rs:229
            └─ render_cache.put(key, result)       render.rs:245
      ↓ markRendered(url, key)
selectEffectUrl → CanvasStage <img className="stage-effect" src={effectUrl}>   CanvasStage.tsx:497-506
```

### 1.2 逐环证据

| 环节 | 证据（file:line + 关键代码） | 判定 |
|---|---|---|
| 点击 → 三个真实后端调用 | `OneClickPanel.tsx:119-123`：`const faces = await ensureFaces(photo); const analysis = await ensureBeauty(photo, force); const report = await ensureGlobalReport(photo, force);` | **已确证** 真调用，无空壳 |
| 进度条真实 | `OneClickPanel.tsx:236-255` 用 `faceProgress`/`beautyProgress`；事件源 `commands/beauty.rs:40-48` `app.emit("lumi://beauty-progress", …)` | **已确证** 非假进度 |
| plan 合成 | `slim.ts:640-712` 逐字段读 `suggestion.suggested[key]`，乘 `MODE_FACTOR` 与整体强度，比较 `applied` 记录决定保留/覆盖 | **已确证** 纯函数且有单测 |
| 写回 store | `OneClickPanel.tsx:82-84` `const next = applyOneClickPlan(current, plan); applyExternalParams(photo.id, next, …)`；`editorStore.ts:945-959` `clampParams(cloneParams(next))` + `pushHistory` | **已确证** |
| IPC 序列化 | `backend.ts:74-89` `invoke("render_preview", { requestId, path, params, maxEdge, rawParams })`；Rust 端 `commands/render.rs:160-170` 参数含 `params: EditParams` | **已确证** 字段一一对应 |
| 强度字段不丢 | `slim.ts:178-187` `ITEM_PROFILE` → `slim.ts:649-661` `capBeauty(key, peak*factor*overall)`；TS `clampParams` 只夹紧 0..100（`params.ts:948-962`），Rust `pipeline.rs:77` 再 `clamped()`。**无中途覆盖**；`withAnchors` 只作用于送进分析的参数副本（`analysis.ts:63-66`），不写回真实参数 | **已确证** |
| Rust 渲染 | `render.rs:220-228` `pipeline::render_with_report(&working, &params, &faces, …)` → `pipeline.rs:264-342` `apply_face_retouch` → 瑕疵/平滑/肤色/眼周 → `pipeline.rs:235-237` `grade::GradePlan` | **已确证** |
| 显示 render 而非原图 | `editorStore.ts:2239-2240`：`if (isNeutral(params)) return photo.previewUrl; return state.renderedPreviews[photo.id]?.url ?? photo.previewUrl;`；`CanvasStage.tsx:493-506` 原图在下、effect 在上 | **已确证** |
| 侦测晚到会不会被前端缓存挡住 | 曾**高度怀疑**：`usePreviewRenderer.ts:64` 的 `cacheKey` 只含 params/maxEdge/raw，不含 faces。复核后**排除**：`setFaces` 在 `editorStore.ts:1445-1478` 会重建 `params[photoId].faces`，`paramsKey` 变化 → effect 重跑。Rust 侧缓存键也含 `repo.signature`（`render.rs:146-155,178-187`） | 已排除 |

### 1.3 一个真实的链路断点：**导入/批量不检测人脸**

```ts
// src/App.tsx:160-169
const handleImport = useCallback(async () => {
  const items = await pickImages();
  if (items.length > 0) useEditorStore.getState().addPhotos(items);   // ← 没有 detectFaces
  ...
});
```
```ts
// src/state/editorStore.ts:961-968 addPhotos：只有去重/激活，没有检测
```
```rust
// src-tauri/src/commands/batch.rs:449-454
let known_faces: Vec<FaceFrame> = repo.get(...).map(|e| e.faces).unwrap_or_default();  // 只读，不检测
```
`pipeline.rs:193-197`：
```rust
if wants_beauty {
    if faces.is_empty() {
        progress(0.06, "未检测到人脸，已跳过美肤");
        report.warnings.push("未检测到人脸，已跳过美肤".into());
    } else { apply_beauty(...)?; }
}
```
→ **批量导出的照片只要没被 UI 面板检测过，美肤参数全部静默失效，只有调色生效。**（**已确证**）

现有检测调用点全量（`grep`）：`App.tsx:336`（项目重开）、`OneClickPanel.tsx:59,119`、`ManualPanel.tsx:144,213`、`FacePanel.tsx:53`、`BeautyPanel.tsx:130,167`。导入与批量均不在其中。

---

## 2. 算法空转：**引擎没坏，但一键只拧了 20%**

### 2.1 AI 建议值本身就保守

```rust
// src-tauri/src/ai_engine/retouch/analyze.rs:216-232
let mut suggested = BeautyParams {
    skin_smooth: (20.0 + 220.0 * texture + 25.0 * dark_ratio).clamp(10.0, 55.0),
    texture:     (38.0 + 260.0 * (texture - 0.035).max(0.0)).clamp(30.0, 70.0),
    detail: 25.0,
    blemish: (blemish_density * 900.0).clamp(0.0, 55.0),
    spots:   (blemish_density * 500.0).clamp(0.0, 35.0),
    ...
};
```
典型纹理能量 0.01–0.06 → `skin_smooth` 只有 **22.4–33.5**（上限才 55）。`tests/beauty.rs:591` 还专门断言 `skin_smooth <= 55`，这是刻意的安全阀。

### 2.2 TS 再乘一次档位系数，双重保守

```ts
// src/state/slim.ts:215-221
MODE_FACTOR = {
  natural: { skin: 0.55, blemish: 0.5, eyes: 0.4, tone: 0.55, light: 0.6, color: 0.5 },
  refined: { skin: 0.8,  blemish: 0.75, ... },
  advanced:{ skin: 1,    blemish: 1,    ... },
};
// slim.ts:649-651
const target = capBeauty(key, peak * factor[id] * overall);
```

以典型建议 `skinSmooth=25`、整体强度默认 100 计算（可复算）：

| 档位 | 写入 `skinSmooth` | 占手动满档（`ITEM_PROFILE.skin.skinSmooth=70`，`slim.ts:180`） |
|---|---|---|
| 自然 | **13.8** | **20%** |
| 精致 | **20.0** | 29% |
| 高级 | **25.0** | **36%** |

`smoothing.rs:20` 的 `strength = skin_smooth/100`，即蒙版内的混合系数只有 0.138/0.20/0.25；`smoothing.rs:32,57` 的 eps/retention 还进一步保留高频。**三档确实产生不同输出（1.0 : 1.45 : 1.81），但全都远低于用户从「高级」期待的力度。**

对照 QA 独立 harness（`docs/SLIMMING_ACCEPTANCE.md:124-160`）在合成人像上的实测：

| 参数组合 | 右脸颊/色块 `mean\|Δ\|` | 说明 |
|---|---|---|
| 手动磨皮满档 `skinSmooth=70` | **0.02144** | 86.5% 像素变化，脸外严格 0 |
| 手动肤色满档组合 | **0.01705** | 55.4% 像素变化 |
| 手动眼周满档 | 0.00892（整图仅 0.5%） | 90% 来自 darkCircles |
| 一键自然（约 20% 驱动） | 预计 ≈ 0.004 量级 | 肉眼接近不可见 |

→ **根因 #1：已确证。**

### 2.3 全图调光压过局部美肤

```rust
// src-tauri/src/image_engine/analysis.rs:506-523
let exposure_delta = (0.46 - stats.mean) * 90.0;
scalar_suggestion("exposure", ..., exposure_delta, ..., -25.0, 25.0, factor)
// factor = mode.tonal_factor()，standard = 1.0（analysis.rs:35-42）
```
`slim.ts:685-706` 对 ⑤⑥ 的处理是 `target = anchor + delta * factor * overall`，**没有再按 profile 降权**（light=0.6/color=0.5 是相对 AI 建议的 50–60%，而皮肤只有相对手动满档的 20%）。

`grade.rs:67` `exposure_gain = 2^(exposure/100*2)`，**对全图每个像素生效**（`grade.rs:89` `par_pixels_mut`）；皮肤算法只作用于几何蒙版。QA 实测磨皮整图仅 11.6% 像素变化（`SLIMMING_ACCEPTANCE.md:157`）。

→ 偏暗人像（mean≈0.35）得到 exposure ≈ +9.9，档位系数 0.6 → +6；全图提亮 6/100 档是**全局可见**的，而脸颊磨皮只有手动满档的 1/5。用户看到的自然是「稍微变亮」。**已确证。**

### 2.4 三档「真实不同」但感知差异不足

| 档位 | skinSmooth | blemish | darkCircles | exposure（假设建议 +10） |
|---|---|---|---|---|
| 自然 | 13.8 | 7.5 | 4.0 | +6.0 |
| 精致 | 20.0 | 11.3 | 7.0 | +8.5 |
| 高级 | 25.0 | 15.0 | 10.0 | +10.0 |

三档数值确实不同（不是同一结果套三个文案），但全落在引擎建议值的低段；**「三档必须产生真实不同输出」在技术上成立，在感知上不达标**。（**已确证**）

### 2.5 测试为什么没抓到

- `pnpm test`：10 files / **128 passed**（含 slim 新增 47 例）；`pnpm typecheck` exit 0。
- `cargo test --lib`：**478 passed / 0 failed / 3 ignored**；`cargo test --test slim_items`：**8 passed**。
- 但 `slim_items.rs` 断言的是「满档参数 → ROI `mean|Δ| > 阈值`」，测的是 `ITEM_PROFILE` 的 70/90/90/50，**没有一条测试走 `buildOneClickPlan` → `applyExternalParams` → `renderPreview`**。QA 在 `SLIMMING_ACCEPTANCE.md:236` 也明确写「真实样张上的三档观感未评估」。

→ 机制测试全绿与观感失效并不矛盾。（**已确证**）

---

## 3. 蒙版缺陷：几何 + 肤色规则，不是语义分割

### 3.1 蒙版生成代码（真实、可解释、不是 AI 分割）

```rust
// src-tauri/src/ai_engine/skin_segmentation/mod.rs:1-9（模块自述）
// 没有可确认商用许可的皮肤解析权重…实际生效的是可解释、可测试的几何方案：
// 5 个关键点建立脸部坐标系 → 额头/双颊/鼻部/下巴/颈部候选区 →
// 挖除眼睛、眉毛、鼻孔、嘴唇 → YCbCr 肤色似然降权 → 羽化。
// 该蒙版不是模型推理结果，报告中标注为 `landmark-geometry`，不冒充 AI 分割。
```
关键步骤：
- 椭圆候选区 `mod.rs:112-137`（`radius = ((u/0.92)^2 + (v/1.58)^2).sqrt()`，`face = 1 - smoothstep(0.80,1.0,radius)`）
- 五官挖除 `mod.rs:139-160`（眼/眉/口/鼻孔椭圆 protect）
- YCbCr 肤色似然 `mod.rs:265-285`（`cb≈0.402, cr≈0.600` 的高斯，`luma_gate`）
- 羽化 `mod.rs:166-168`、低置信清零 `mod.rs:203-207`（`alpha < 0.12 → 0`）
- 多人脸归属 `mod.rs:591-616`（`apply_ownership` 按最近脸中心）

**这是真实算法，会改像素**（QA 复现脸颊 0.02144、脸外 0），但：
- 依赖 **5 点关键点**质量（侧脸/遮挡/戴口罩会退化）；
- 依赖 **YCbCr 肤色中心**，深肤色/强阴影下可能整片低 alpha（**高度怀疑**，需真实样张）；
- 无法区分「面部皮肤 vs 颈部 vs 手部」，只能靠几何范围限制。

### 3.2 `status()` 走的是 `check`（深检）不是 `check_shallow`

```rust
// src/ai_engine/skin_segmentation/model.rs:70-72
let health = crate::ai_engine::model_registry::verify::check(spec, &base_dirs, false);
```
`verify.rs:146-163` `check(spec, base_dirs, include_contract)` 定位文件后调 `check_at`；
`verify.rs:37-97` `check_at` 依次做：许可闸门 → 文件存在 → 体积/格式头 → （可选）契约 → **SHA256 流式校验**。
第三个参数 `false` 只跳过第 ④ 步「输入输出契约」（`verify.rs:70-84`，注释写明"热路径推迟到 load"），**SHA256 在第 ⑤ 步仍然执行**。
`check_shallow`（`verify.rs:118-143`）在本路径**未被调用**。→ 不存在「浅检冒充深检」。（**已确证**）

不过：`SKIN_MODELS` 为空 ⇒ `for` 循环一次都不执行 ⇒ 这段代码实际永远走 fallback 分支（`model.rs:88-104`）。深检逻辑正确但空转。

### 3.3 没有测试实现冒充真实能力

| 组件 | 状态 | 证据 |
|---|---|---|
| 皮肤蒙版 | 明确声明几何方案，`method="landmark-geometry"` | `skin_segmentation/model.rs:90-104` |
| AI 人物分割 Provider | `available() == false`，`segment()` 返回错误；文案「没有经过再分发审查的权重」 | `segmentation/inference.rs:1-12,36-48` |
| GPU 推理后端 | `available == false`，原因「缺少 benchmark 依据」；不接 wgpu | `inference/gpu_stub.rs:1-12,37-50` |
| 真实推理后端 | 只有 CPU `CpuTractBackend`（tract-onnx 0.23） | `inference/mod.rs:10,22-24`；`Cargo.toml:31` |

→ **未发现 `ProtocolTestOnly` 之类测试实现被当真实能力对外声明。**（**已确证**）

---

## 4. 模型缺口：登记表与许可证

### 4.1 登记了什么

```rust
// src-tauri/src/ai_engine/model_registry/builtin.rs:157-164
pub const SKIN_MODELS: &[&ModelSpec] = &[];
pub const SEGMENTATION_MODELS: &[&ModelSpec] = &[];
pub const ALL: &[&ModelSpec] = &[&YUNET];
```
旧表同样为空：`skin_segmentation/model.rs:30` `pub const REGISTERED_SKIN_MODELS: &[RegisteredSkinModel] = &[];`

`ai_engine/segmentation/model.rs:34-65` 的 `CATALOG` 是**调研目录**，4 个条目（BiRefNet / SAM2 / MediaPipe Pose / RVM）**全部 `Disabled`**，且 `builtin.rs` 明确不引用它（`:159-161`）。不会「候选被误当已授权」。（**已确证**）

### 4.2 许可证四字段（唯一随包模型 YuNet）

```rust
// builtin.rs:112-137
license: LicenseRecord {
    weights: Verdict3::Allowed,
    training_data: Verdict3::Unverified { what_to_check: "需人工核对 WIDER FACE 数据集使用条款…" },
    commercial_use: Verdict3::Allowed,
    redistribution: Verdict3::Allowed,
    spdx: Some("MIT"),
    ...
}
```
闸门 `license.rs:141-146`：`GATED_FIELDS = weights / commercialUse / redistribution` **三项**；`training_data` 只做披露、不阻塞（`license.rs:141` 注释）。→ YuNet 可随包，合规口径一致。（**已确证**）

任何皮肤分割模型若要登记，必须同时补齐四字段证据（权重/训练数据/商业使用/再分发）+ SPDX + 官方页面引用 + SHA256 + 打包资源清单（`builtin.rs:10-11` 明确提醒同步 `tauri.conf.json`）。

---

## 5. 合法、离线、可商用的皮肤分割方案评估

> 结论先行：**仓库当前没有任何已登记、可加载的皮肤语义分割模型（已确证）。**下面是候选，**均未通过本项目的四字段/SHA/tract 验证**，不得在 UI 宣称 AI 皮肤识别。

| 候选 | 许可证与出处 | 与 tract-onnx 0.23 兼容 | 与「皮肤区域」的相关性 | 集成成本 |
|---|---|---|---|---|
| **A. MediaPipe Selfie Multiclass 256×256** | Google MediaPipe，模型卡 Apache-2.0；第三方项目记录 `selfie_multiclass_256x256.tflite`（含 `background/hair/body-skin/face-skin/clothes/others` 六类）已用 tf2onnx 转 ONNX opset 18（`Pedrojok01/linux-broadcast/models/README.md`） | **待验证**：需实测转换后 ONNX 的算子集；tf2onnx 转换产物常有 `Resize/NonZero` 等 tract 支持度不一 | **最高**：直接给出 `face-skin`/`body-skin` 语义，可把现有几何蒙版与该通道求交/并集 | 高：下载+转换+SHA+四字段登记+资源打包+契约测试+集成进 `build_skin_mask` |
| **B. MODNet（portrait matting, ONNX 官方提供）** | ZHKKKe/MODNet README 原文："The code, models, and demos in this repository … are released under the Apache License 2.0"，提供 `onnx/` 转换脚本 | **待验证**：现成 ONNX，但需 `inspect_contract` + `infer` 实测；输入约 512×512 | 中：输出人物 alpha，不是皮肤语义；可与 5 点几何取交集，改善发际/下巴边缘，脸部皮肤仍靠 YCbCr | 中：无需 TFLite 转换，25MB 级权重；仍需四字段/SHA/打包 |
| **C. MediaPipe Selfie Segmentation landscape** | Apache-2.0（model card 明示 "Licensed under Apache 2.0"，第三方 vendored 记录含 SHA `490e9ea7…`，`infernoxc/ixc-camera/third_party/.../README.IXC.md`） | **待验证**：仅 TFLite，需转换 | 低：只有「人 vs 背景」，只能排除背景，不能定位脸部皮肤 | 中高 |
| **D. Apple Vision `VNGeneratePersonSegmentationRequest`** | 系统内建，无权重分发问题；macOS only | 不适用（非 ONNX） | 低：同 C，且不能跨 Windows | 中（平台分支，不能作为主方案） |

**明确排除**（与商业发行或现有策略冲突）：
- BiSeNet / CelebAMask-HQ 系：CelebAMask-HQ 数据集非商用条款，现仓库注释已据此拒绝（`skin_segmentation/model.rs:30-33`）。
- InsightFace 多数模型：仅限研究（同上）。
- RVM：GPL-3.0，`CATALOG:60-64` 已判定不适合当前商业发行。
- SAM2 / BiRefNet：需额外运行时或未取得可再分发权重，`CATALOG:36-58` 为 Disabled。

**诚实缺口报告**：想要「真正的 AI 皮肤区域识别」，最短路径是候选 A，但必须先完成：①取得官方权重文件并记录 SHA；②人工核对四字段并写入 `LicenseRecord` + 证据 URL；③TFLite→ONNX 转换并用 `inference::inspect_contract` / `check_contract` 验证；④用 `CpuTractBackend` 跑通推理并做精度基线；⑤同步 `tauri.conf.json` resources。**在此之前，产品不得对外声称使用 AI 语义分割。**

---

## 6. 五项核心能力修复设计（只设计，不改代码）

> 共同原则：**不动引擎数值**（保住 478+8 个 Rust 测试契约），只改 TS 侧 plan 合成与 UI 驱动；所有方案必须让三档在真实像素上拉开可测差距。

### 6.1 磨皮美肤：从「AI 建议驱动」改成「满档剖面 × 档位 × 问题分析调节」

```ts
// 设计伪码，放 src/state/slim.ts 新纯函数
blendItemWithAnalysis(id, analysis, gain) {
  const peak = ITEM_PROFILE[id];                      // skin: 70/55/28（已验证的满档）
  const severity = problemScore(analysis.metrics);   // 0..1：纹理/瑕疵/暗沉的实际程度
  return mapValues(peak, v => capBeauty(v * gain * (0.6 + 0.4 * severity)));
}
```
- 档位 gain 基于**满档剖面**：自然 0.45 / 精致 0.70 / 高级 1.00 → `skinSmooth` 31.5 / 49 / 70（现在是 13.8 / 20 / 25，提升 2.3–2.8×）。
- `texture` 跟随 profile 55（现在自然档只有 20.9，反而保护不足），`detail` 28。
- `problemScore` 只用 `analyze_face.metrics` 的既有字段（`texture/blemishDensity/chromaVariation/darkRatio`），无新问题需要新后端。
- **验收**：同一人像三档在脸颊 ROI 的 `mean|Δ|` 分别 ≥ 0.008 / 0.015 / 0.020，且两两组间比值 ≥ 1.4；脸外仍严格 0（对齐 `slim_items.rs` 的蒙版外断言）。

### 6.2 祛痘祛斑：`blemish` 用满档剖面，按「有没有痘」门控

- `blemish = 90 × gain × (0.4 + 0.6·min(1, blemishDensity/0.02))`，`spots = 0.66×blemish`（硬约束 `spots ≤ blemish`，规避 `blemish.rs:45` 的 `max()` 吞并），`blackheads = 35 × gain`（独立 pass，只在对黑头有证据时写）。
- `blemishDensity < 0.003` 时**不写**这一组（避免对干净皮肤做无谓修复，也减少「怎么没变化」的误判——此时 UI 应显示「没找到需要淡掉的斑点」）。
- **验收**：用带暗斑的合成人像，三档在 `SPOTS` ROI 的 repaired 像素数单调增；痣仍被保护（`tests/beauty.rs:288-326` 不回归）。

### 6.3 肤色均匀：主推 `toneEven`，限制 `whitening`

- `toneEven = 65 × gain × severity(chromaVariation)`；`whitening = min(35, 50 × gain)` 且仅当 `meanLuma < 0.55` 才写；`redness`/`dullness` 按 `redness`/`darkRatio` 门控。
- 目的：消除「一键 = 变白」的观感来源之一；`tone.rs:90-106` 的四种操作里，真正该主导的是 `tone_even` 而不是 `whitening`。
- **验收**：肤色色块 ROI 的 Lab-a 标准差下降（均匀），而全图平均亮度提升 ≤ 肤色 ROI 提亮幅度；高光钳位 `tone.rs:109-111` 不回归。

### 6.4 眼周：`darkCircles` 主导、`eyeLines` 单独验证

- `darkCircles = 90 × gain × min(1, darkCircleDelta/0.08)`；`eyeLines = 70 × gain`（只在 `eyeTexture > 0` 时写）。
- QA 发现 `eye_lines=70` 单独在合成 fixture 上只有 0.00035（`SLIMMING_ACCEPTANCE.md:141`）——fixture 没皱纹，不是代码错；需补一组「带横向细纹条纹」的 fixture 再断言，否则文案「和眼下细纹」缺证据。
- 绝对不碰 `eyeShape.size` / `face_reshape` 的眼部形变（暂缓项）。
- **验收**：眼下 ROI `mean|Δ|` 三档单调增且整图变化占比 < 2%（QA 当前 0.5% 是合理上界）。

### 6.5 一键整合：重定权重 + 老条目修复 + 端到端观感测试

1. `buildOneClickPlan` 的 **beauty 分支**改用 6.1–6.4 的 `blendItemWithAnalysis`；**tonal 分支**降权：light gain 0.35 / color gain 0.25，并把 exposure 上限在**一键路径**收到 ±8（手动路径不动）。
2. 加一条**输出平衡硬约束**：计算 `tonalMeanDelta`（全图亮度均差）与 `skinMeanDelta`（脸颊 ROI 均差）；若 `tonalMeanDelta > skinMeanDelta`，本次 plan 判为「失衡」并把 tonal 系数再乘 0.5。可由后端 `pipeline` 的 report 或前端在 `renderPreview` 结果上做（前端更简单，但要用真实像素，不能用估算）。
3. `applyOneClickPlan` 对**本次有写入**的人脸条目强制 `enabled = true`（修 `slim.ts:726-751` 对老项目 `enabled=false` 不处理的漏洞）。
4. 检测门控：一键在 `ensureFaces` 后检查 `faces.length > 0`；为 0 时明确「只调光影色调，不修皮肤」（UI 已有 `OneClickPanel.tsx:258-261`），但**禁止**把这种情况计入「一键精修成功」的观感验收。
5. 批量路径：在 `batchStart` 之前对未检测照片补跑 `detect_faces`（或在 `commands/batch.rs:449` 检测并 `repo.store`），否则批量导出的美肤一律无效。
6. **新增端到端测试**（当前最大验证缺口）：用真实样张 + 三档各跑一次 `analyze → plan → render`，断言：①三档输出两两 `mean|Δ| ≥ 0.003`；②「高级」在脸颊 ROI 的 `mean|Δ| ≥ 0.015`；③脸外像素不变；④`noise` 指标不劣化（`analysis.rs` 的 `detail.noise` 已算但不参与建议）。

---

## 7. 待验证清单（给主理人/QA 的具体动作）

| # | 事项 | 方法 | 当前状态 |
|---|---|---|---|
| 1 | 真实人像上三档观感与幅度 | 取 3 张不同肤色/光照人像，导出三档 PNG，按 §6.5 指标量测 | 待验证 |
| 2 | 深肤色/暗光下几何蒙版是否漏 | 导出 `preview_skin_mask`，统计 `skinRatio` 与脸颊 alpha；对照浅肤色 | 高度怀疑 |
| 3 | MediaPipe multiclass ONNX 是否被 tract 0.23 支持 | `inference::inspect_contract` + `check_contract` + 实际推理 | 待验证 |
| 4 | 候选模型四字段与 SHA | 人工核对官方 model card/LICENSE，登记 `LicenseRecord` | 待验证 |
| 5 | 批量导出是否真的没检测 | 用未打开过 UI 面板的照片直接批量导出，对比导出图与原图 | 已确证（代码路径），待运行时复现 |
| 6 | 老项目 `enabled=false` 人脸是否不生效 | 构造 `.lumi` 复现 | 高度怀疑 |

---

## 8. 本轮确认**不是**根因的几件事（避免误伤）

1. **不是假按钮**：三个分析命令、渲染命令、进度事件全部真实（`§1.2`）。
2. **不是预览显示原图**：`selectEffectUrl` 正确选择 render 结果，缓存键含参数与人脸签名（`§1.2`）。
3. **不是 TS→Rust 序列化丢字段**：camelCase 一一对应，双方都 `clamp`，无中途覆盖（`§1.2`）。
4. **不是皮肤算法空实现**：磨皮/祛痘/肤色/眼周都有像素写入与自动化断言；QA 独立 harness 复现了同样的数值（`§2.5`、`docs/SLIMMING_ACCEPTANCE.md:105-160`）。
5. **不是用测试模型冒充 AI 能力**：`AiMaskProvider` / GPU 后端都明确 `available=false`（`§3.3`）。
6. **不是 `status()` 走浅检**：走 `verify::check(..., false)`，SHA256 仍校验（`§3.2`）。

---

## 附：审计时的基线（只读执行）

| 命令 | 结果 |
|---|---|
| `pnpm test` | 10 files / **128 passed** |
| `pnpm typecheck` | exit 0 |
| `cargo test --lib` | **478 passed / 0 failed / 3 ignored** |
| `cargo test --test slim_items` | **8 passed / 0 failed** |

**测试全绿但效果不可见，本身就说明当前测试体系缺少「感知幅度」这一层断言（§2.5 / §6.5）。**
