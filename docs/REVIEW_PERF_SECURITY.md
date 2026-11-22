# LUMI RETOUCH 代码审查：性能与安全

审查日期：2026-11-22
审查范围：`src/`（React 19 前端，~14k 行）、`src-tauri/src/`（Rust，~39k 行）
方法：静态阅读 + 数据流追踪（滑杆一次回调 → IPC → Rust 管线 → 回写渲染结果）；未做运行时 profiling，未执行构建/测试。

结论摘要：**没有发现可直接远程利用的漏洞**（无 XSS sink、无 `localStorage`、CSP 严格、ONNX 推理在独立进程 + rlimit 下跑、写盘全部走"临时文件 + rename"）。真正需要动手的是两类：

1. **参数更新链路的结构性性能问题**（P1/P2）——滑杆每动一格，整份参数文档（含所有笔刷点）要被 JSON 序列化 6 次；且因为深拷贝产生新对象身份，**滑杆拖动会触发后端蒙版重算**。这是当前最可能让用户感到"越修越卡"的根因。
2. **几处"无上限读取"**（S1/S2/S3/S4）——大文件/畸形文件能把内存顶到 GB 级。修法都很小，且项目里已经有 `read_limited` 这类现成工具没用上。

---

## 一、性能问题

### P1（高）参数每次微调都要全量 JSON 序列化 ~6 次

| 位置 | 现象 |
| --- | --- |
| `src/state/editorStore.ts:567` | `clampParams(updater(cloneParams(current)))` |
| `src/state/params.ts:1092-1094` | `cloneParams` = `JSON.parse(JSON.stringify())` |
| `src/state/params.ts:1140-1142` | `paramsEqual` = 两次 `paramsKey` = 两次 `JSON.stringify` |
| `src/hooks/usePreviewRenderer.ts:53` | `paramsKey(params)` 又一次 stringify |
| `src/components/ui/Slider.tsx:111` | range input 的 `onChange` 按指针频率触发（120Hz 触控板会更高） |

`EditParams` **不只是几个标量**：它内含 `subjectMask.strokes[].points[]`、`repairPoints[]`、`faces[].warpStrokes`、`body[].maskStrokes`（见 `src/state/params.ts:510-516`）。一次修图会话笔刷画满后，参数文档轻松到几百 KB。

于是**拖一次滑杆的每一格**都要付出：

1. `JSON.stringify(params)` → 2. `JSON.parse` → 3. `clampParams` 重建整棵树 → 4. `clampParams` 再来一遍（第 567 行的外层） → 5. 两次 `JSON.stringify` 做相等比较 → 6. effect 里 `paramsKey` → 7. IPC 序列化 → 8. Rust 端 `cache_key` 里再 `serde_json::to_string(params)`（`src-tauri/src/commands/render.rs:146-157`）。

按 300KB 文档、60 次/秒估算，约 **100MB/s 的字符串分配**，全部落在主线程。这也解释了为什么"画完蒙版之后滑杆变卡"——代价随笔刷数据线性增长。

**建议（按性价比排序）**

- `paramsEqual` 换成廉价的版本号/脏标记比较：`apply` 内部本来就知道改了什么，不必序列化整个文档；或在 store 里维护 `paramsRev`，history/compare 只比较 rev。
- 笔刷类数据（`subjectMask.strokes`、`repairPoints`、`warpStrokes`、`body.maskStrokes`）**从 `EditParams` 拆出去**，放到独立 slice（`brushByPhoto`）。滑杆只碰标量文档，就不再为笔刷数据付序列化成本；Rust 侧 `EditParams` 反序列化也会变快。
- 若暂不拆结构，至少把 `apply` 改成"手工构造 next"：只拷贝被改的那一层（`{...current, exposure: value}`），标量钳制就地做，不做整树 clone。`clampParams` 目前是"从 `defaultParams()` 重建全树"，作为脏数据入口很稳，但不适合当每次交互的必经路径。

### P2（高）滑杆拖动会触发后端"人物蒙版重算"

```
src/components/CanvasStage.tsx:108-138  useEffect(..., [photo, params.subjectMask, ...])
src/state/params.ts:1024 / 800-818    clampSubjectMask 每次都返回新对象
```

`apply` 深拷贝整棵树 → `params.subjectMask` **每次参数变化都是新身份** → 第 138 行的依赖判定"变了" → 180ms 后真的调 `previewSubjectMask(photo.path, ...)`（`:122`）跑一遍后端蒙版合成。

也就是说：用户只要开着蒙版预览/画笔，拖动**曝光**滑杆也会持续触发蒙版重算，且结果毫无变化。

**建议**：依赖项换成内容签名（`paramsKey(params.subjectMask)` 太贵 → 用 `strokes.length + 单调整数值 + 一个 strokesRev`），或按 P1 建议把笔刷数据拆出 `EditParams`，依赖自然稳定。

同类问题：`CanvasStage.tsx:105` 的网格 effect 依赖整个 `params`，改**颜色**参数也会去后端取一次 `reshapeMesh`。建议收窄到 reshape 相关字段。

### P3（中）撤销栈存整份参数快照，上限 100

`src/state/history.ts:26-40`（`pushHistory` 保存 `before` 的引用）+ `src/state/editorStore.ts:584`（`before` 来自深拷贝后的 `current`，每次都是独立对象）+ `history.ts:13`（`limit = 100`）。

笔刷数据同样在快照里 → 100 份 × 每份几百 KB，内存占用可达几十到上百 MB，且 GC 压力大。

**建议**：结构化共享（只记录 patch/反操作），或按字节预算而非条数淘汰（和 `ImageCache` 的 `budget_bytes` 思路一致，见 `src-tauri/src/state.rs:50`）。

### P4（中）预览图走 base64 data URL

`src-tauri/src/image_engine/preview.rs:23-29` → `encoder.rs:73-79` → `src-tauri/src/commands/render.rs:236`（`preview_url`）→ `src/components/CanvasStage.tsx:491-508`（`<img src="data:image/jpeg;base64,...">`）。

代价有三层：IPC 传大字符串（1800px JPEG 编码后 ~300KB–800KB）、前端 JS 堆常驻、浏览器**主线程**解码 JPEG（3200px 高清预览时单次解码 30ms 量级）。导入 100 张时 `commands/library.rs:34-35` 会给**每张**都编码一份 1800px 预览并塞进一次响应。

**建议**：改用 Tauri 自定义协议/`convertFileSrc` 提供二进制流（写临时文件或走 protocol handler），前端 `createImageBitmap` 离主线程解码；列表里的 `previewUrl` 改为按需加载（选中才取全预览，列表只用 thumb）。注意：改协议后 CSP 的 `img-src` 要同步放开对应 scheme（当前是 `data: blob:`）。

### P5（中）渲染缓存的 key 是"整份参数 JSON"，且线性扫描比较

`src-tauri/src/commands/render.rs:73-80`（`position(|stored| stored == key)`）、`:183-189`（拼 key）、`:146-157`（`cache_key` 里 `serde_json::to_string(params)`）。

key 长度 = 参数文档大小（可达数百 KB），缓存里最多 12 条 → 每次未命中要走 12 次长字符串比较；命中时也要比到匹配为止。同时 `RENDER_CACHE_BUDGET` 只统计 `preview_url.len()`（`:84`），key 本身占的体积没算进去。

**建议**：key 取哈希（`blake3`/`sha2` 后 32 字节，项目里 `sha2` 已有依赖），比较变成定长字节比较；预算里把 key 长度也计入。

### P6（中）画布指针移动：每次事件 setState + 布局读取 + 877 行组件整体重渲染

`src/components/CanvasStage.tsx:269-330`（`handlePointerMove`）、`:427-438`（`toImagePoint` 每次都 `getBoundingClientRect()`）。

画笔/蒙版/变形模式下，每个 pointermove 都 `setCursor` + `setDraft`，触发整个 `CanvasStage` 重渲染——包括网格（`mesh.triangles` 可能上千个 `<polygon>`）、人脸关键点、锚点等全部 SVG 重建。指针 120Hz 时这会直接吃掉一帧预算。

**建议**：① `getBoundingClientRect` 结果缓存到 ref，只在 resize/photo 变化时刷新；② 笔刷游标 + 草稿路径抽到独立子组件（或直接操作一个 `<canvas>`/`<path>` 的 `d` 属性），不要让父组件跟着重渲染；③ 指针事件用 `requestAnimationFrame` 合并。

### P7（中）草稿笔迹用闭包里的 `draft`，高指针频率下会丢点

`src/components/CanvasStage.tsx:296-298`、`306-308`、`317-319`：

```ts
setDraft({ points: [...draft.points, point] });   // draft 来自上一次渲染
```

指针事件密度高于 React 渲染频率时，两次 move 会基于同一个旧 `draft` 计算，**后一次覆盖前一次 → 笔画丢点、曲线出现折角**。

**建议**：改函数式更新 `setDraft(prev => prev ? { points: [...prev.points, point] } : prev)`。

### P8（中，RAW 开关打开时）RAW 预览每格滑杆都重新解一遍 RAW

`src-tauri/src/commands/render.rs:203` → `image_engine/raw/mod.rs:162-165` → `decode_linear(path)`，**没有任何缓存**；`ImageCache` 只覆盖非 RAW 路径。

当前 `default` feature 下 RAW 被关掉（`decoder.rs:73-81`），`--no-default-features` 的 Beta 构建里同样会返回"未开放"，所以这条暂时打不到用户。但一旦放开 RAW，50MP 文件每格滑杆 = 一次 LibRaw unpack + 线性化，交互会明显卡顿。

**建议**：给 `decode_linear` 的结果也加一层 LRU（复用 `ImageCache` 的预算机制，或单独 `RawCache` 按像素预算淘汰）。

### P9（低）其余可顺手处理项

| 位置 | 问题 | 建议 |
| --- | --- | --- |
| `image_engine/pipeline.rs:77`、`:85` | 每次渲染 `params.clone().clamped()` + `image.clone()`（1800px ≈ 17MB 拷贝） | 参数改 `&EditParams` 就地钳制；图像用 `Cow` 或只在真有改动时 clone |
| `src/state/editorStore.ts:2051-2057` | `selectEffectUrl` 内部调 `isNeutral`（遍历全部参数含笔刷） | selector 只读 `renderedPreviews` / 预存一个 `hasEdits` 标记 |
| `src/hooks/usePreviewRenderer.ts:78-79` | 每次参数变化**同步**写 `renderStatus`/`renderProgress`，而真正的请求在 70ms 防抖之后 | 状态切换挪进 debounce 回调，避免每格两次额外全量重渲染 |
| `src-tauri/src/state.rs:68-82` | 同一路径并发 `load` 没有 single-flight，会重复解码 | 加 `Mutex<HashMap<PathBuf, Arc<OnceLock>>>` 之类合并请求 |

---

## 二、安全问题

### S1（中）预设导入：无体积上限 + 无结构上限

`src-tauri/src/commands/preset.rs:564`：

```rust
let data = std::fs::read(&path)?;      // 按文件大小一次性分配
let preset = store.import_json(&data)?; // 238-267：只校验 params 非空
```

预设是可外发/可交换的文件（`export_preset`/`import_preset`）。一个 2GB 的 `.json`，或一个 `params` 里塞几百万个数组元素的 JSON，会直接把内存顶爆；`serde_json` 只限递归深度，不限数组长度/总字节。

**修法很小**：项目里已经有 `model_hub/security.rs:48` 的 `read_limited`（先看 metadata，再 `take(limit+1)`），直接复用即可（建议 1MB，和 `MAX_MANIFEST_BYTES` 同量级）；再对 `params` 的条目数/字符串长度做个上限。

### S2（中）图片解码：整个文件读进内存，尺寸校验在解码之后

`src-tauri/src/image_engine/decoder.rs:90`（`std::fs::read(path)`）、`:107`（`DynamicImage::from_decoder`）、`:125-129`（`ensure_reasonable_size` **在解码完成之后**才调用）。

- 文件本体没有上限：一个 3GB 的 TIFF 会先整块读进来。
- 像素上限（`buffer.rs:75-83`，1.2 亿像素）判得太晚：解码阶段的峰值内存已经发生了。而且 `image` crate 默认 `Limits::default()` 的 `max_alloc` 是 512MB（已确认 `into_decoder` 会 `set_limits`），所以实际效果是"超过约 64MP 就先报 `内存分配失败`"，用户看到的不是那条友好的"超过 1.2 亿像素上限"——即 120MP 这条上限对大多数 8-bit 源图实际不可达，错误提示会错位。

**建议**：① `fs::read` 前用 `metadata.len()` 卡上限（比如 256MB）；② 尺寸判断前移到 `decoder.dimensions()`（`from_decoder` 之前），这样超限图不会先分配再拒绝；③ 顺手显式 `set_limits`，让错误信息可读。

### S3（低）ChangeSet 应用路径缺少 64MB 上限

`src-tauri/src/image_engine/changeset.rs:208-210` 直接 `std::fs::read(path)` + `from_slice`；而整包加载路径 `image_engine/project.rs:469-474` 是有 64MB 检查的。同一份项目文件走两条不同入口，安全基线不一致。

**建议**：把 `project.rs` 的读取+限额抽成 `read_project_bytes(path)`，两处共用。

### S4（低，依赖 `raw` feature）LibRaw 像素缓冲无尺寸闸门

`src-tauri/src/image_engine/raw/libraw_ffi.rs:252`：

```rust
let mut pixels = vec![0u16; width * height];
```

`width/height` 取自文件头解析结果，没有 `ensure_reasonable_size` 之类的上限。畸形/超大 DNG 可以让这里申请数 GB。当前发行版不含 `raw` feature（`mod.rs:183-193`），风险被 feature 挡住；放开 RAW 前建议在这里加一道像素上限。

### S5（低）模型导入是 TOCTOU 式的"先校验后打开"

`model_hub/security.rs:80-115`（`symlink_metadata` 判符号链接 + 读文件头）与 `:137-144`（`sha256_file` + `canonicalize`）是**分三次打开同一路径**。校验与最终读取之间路径被替换，理论上可以绕过"不接受符号链接"。

同样，`:149-155` 的 `reject_external_data` 是子串扫描，属于启发式（ONNX 用不同编码/分片写外部数据时可能漏），文档里应写明"这是降级防线，不是保证"。

考虑到这是本地单用户应用、模型只能由用户主动选择，实际风险不高；加固成本也很低：`open` 一次拿 `File`，后续 `sha256`/读头都用同一个句柄（`File::try_clone`），并在 macOS 上检查 `O_NOFOLLOW`。

### S6（提示）CSP 的必要让步与推理隔离

`src-tauri/tauri.conf.json` 的 CSP 里 `style-src 'unsafe-inline'`、`img-src data: blob:` 是当前 base64 预览方案的必然结果（见 P4）。一旦切到自定义协议提供图片，这两条可以收紧，`connect-src` 只有 `ipc:`/`tauri:` 这点很好——webview 没有任何外部网络出口。

`face_intelligence/model_hub/probe.rs:129-147` 用 `setrlimit(RLIMIT_AS / RLIMIT_CPU)` + 独立进程跑推理，是本项目最扎实的一处安全设计（恶意/畸形 ONNX 只能打死 Worker）。唯一小瑕疵：`infer_isolated` 在 `temp_dir()` 下用固定文件名（`:161-162`），同进程并发推理会互相覆盖——建议在目录名里带 request 序号。

---

## 三、已核实为良好的部分

避免只提问题，这些点我特意反向查过，**不需要改**：

- **写盘安全**：导出走 `.lumi-tmp-{nanos}-{name}` + `rename`（`image_engine/export.rs:166-179`），并显式拒绝覆盖原图（`:131-135`，含 canonicalize 比较）；项目保存同样走临时文件（`project.rs:404-444`）并有保存锁（`changeset.rs:206`）。
- **文件名注入**：批量导出 `sanitize`（`commands/batch.rs:669-712`）、预设名 `sanitize`（`commands/preset.rs:569-585`）都会剥掉分隔符，落盘路径不会逃出用户选的目录。
- **模型路径穿越**：`safe_join` + `validate_model_id`（`model_hub/security.rs:158-182`，含 `starts_with` 复核）挡住了 `../`；`MAX_MODEL_BYTES` 512MB、拒绝 ZIP/ELF/Mach-O/pickle/LFS 指针，覆盖到位。
- **内存预算**：`ImageCache` 同时按条目数和字节预算淘汰（`state.rs:50-130`），`RenderCache` 也有 160MB 预算——比"只看条数"的实现靠谱。
- **前端 XSS 面**：全仓无 `dangerouslySetInnerHTML` / `innerHTML` / `eval` / `new Function`，无 `localStorage`；所有用户可见字符串都经 React 转义。
- **权限面**：`capabilities/default.json` 只开了 `core:default` 加三个窗口能力，没开 `fs`/`shell`/`http` 插件——命令面完全由 `invoke_handler` 白名单控制（`lib.rs:96-175`），这个收敛程度是对的。
- **完整性校验**：项目加载有 64MB 上限、迁移有报告（`migration_log`）、ChangeSet 有白名单式字段校验（`changeset.rs:366-383` 的 `ensure_patch` + `struct_keys`）和幂等 `requestId`。

---

## 四、建议的修复顺序

1. **P2 + P1 前半**（改 `CanvasStage.tsx:138` 依赖 + `apply` 不做整树 clone）——改动最小，能立刻消掉"越修越卡"。
2. **P1 后半**（笔刷数据拆 slice / `paramsEqual` 去序列化）——收益最大，但要动 `EditParams` 与 Rust 侧结构，建议单独一轮做，配 `engine.rs` / `persistence.test.ts` 回归。
3. **S1 + S2 + S3**（复用 `read_limited`、尺寸判断前移）——都是十来行，风险低，建议一起做掉。
4. **P4 + P5**（数据 URL → 自定义协议、缓存 key 改哈希）——需要同步改 CSP 和 `CanvasStage` 的 `<img>` 用法。
5. P3 / P6 / P7 与其余低优先项，随轮次顺手处理。

## 五、本次审查的边界（对抗性自检）

- 只做静态分析，**没有跑 profiling 或基准**；P1 的"6 次序列化"和"100MB/s"是按代码路径与 300KB 文档量级估算的，方向确定、绝对值需实测（`docs/ROUND10_PERFORMANCE.md` 里的基线可以用来对比）。
- **没有执行构建与测试**，因此不排除存在编译/测试层面的问题；行号基于当前工作区状态（未提交改动可能影响）。
- RAW 相关结论（P8/S4）依赖 `raw` feature；当前 `default` 与 `--no-default-features` 构建下均不生效，标为"放开前必须先修"。
- 未审计 `scripts/*.py`、`face_intelligence/providers/vision_bridge.m`（Objective-C）与 vendored `libraw_rs_vendor` 源码本身。
