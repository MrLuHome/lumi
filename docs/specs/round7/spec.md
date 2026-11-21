# ROUND 7 SPEC — RAW ENGINE & ICC COLOR MANAGEMENT & GPU ACCELERATION 1.0

状态：待批准（修订版 2）
基线：`main` @ `937b304`，工作区干净；第六轮 `.app` / `.dmg` 已存在。
依据：第七轮任务书全文（1600 行）。

本规格不缩减任务书范围：任务书 P0 全部列为交付项；仅对“本机物理不可验证”的事项
（Windows 构建、真实相机全型号覆盖）在报告中标注验证状态，不以标注代替实现。

---

## 1. 已核实的基线事实

| 项 | 实测 |
|---|---|
| HEAD | `937b304`，`git status` 干净 |
| Rust | 1.95.0；`Cargo.toml` 现声明 `rust-version = 1.80` |
| Node | v22.15.0；脚本 `typecheck/test/build/app:build` |
| 机器 | macOS 27.2.1 / Apple M4 / 16GB |
| 原生库 | `/opt/homebrew/opt/little-cms2`（含 `liblcms2.a`）、`libtiff` |
| 第六轮包 | `src-tauri/target/release/bundle/macos/LUMI RETOUCH.app` |
| RAW 样本 | 已下载 6 个 CC0 样本（见 §4.2），Nikon NEF 链接 404，需换源 |

**实测 P0（阶段 0 首先修复）**：启动正式 `.app` 后窗口为空白（`#0b0c0e`），
AX 树只有 1 个空 `AXGroup`，无 HTML 节点；内嵌资源名与 `dist/index.html` 一致，
排除哈希失配，需定位 WebView 未执行脚本的原因。

---

## 2. 阶段 0：ROUND 6.1 收尾

### 2.1 空白窗口修复（P0）

1. `npm run build` + `vite preview` 浏览器验证前端本身。
2. 重建 `.app` 时开启 WebView 远程调试（`tauri.conf.json` 临时 `devtools: true` 或 debug 构建）
   抓取 WKWebView 控制台错误。
3. 核查 Tauri v2 自定义协议下绝对路径 `/assets/*`、CSP `script-src 'self'`、
   `titleBarStyle: Overlay` 与非 ASCII 路径（`lumi人像`）是否导致资源 404。
4. 修复（优先配置层：`base: "./"` / CSP / 路径处理），`app:build` 重建并复测。
5. 记录：现象、日志、根因、修复、复测截图，写进 `ROUND6_GUI_ACCEPTANCE.md`。

### 2.2 GUI 人工验收

启动正式包 → 截图（`docs/assets/round6-gui/`）→ System Events 读取 AX 树作为控件存在证据 →
对可点击控件用 AppleScript `click` 执行；不可点击的步骤如实写“未执行：辅助功能限制”，
不得计入通过。覆盖任务书 §2.1 的 22 个步骤（启动、导入、切图、参数、智能分析、应用建议、
撤销重做、14 预设、强度、自定义预设重启保留、多选、复制、批量预设、批量导出、同名跳过、
取消、单项重试、保存项目、退出、重开核对）。

### 2.3 能力标注

按任务书 §2.2 表格统一 UI/菜单/提示/capability 返回值；删除任何“AI 肤质诊断”类表述；
皮肤深度分析显示“暂不支持”。产出 `docs/ROUND6_CAPABILITY_LABELS.md`。

### 2.4 批量内存

- 批量专用缓存预算 ≤256MB；每张完成后作用域释放全尺寸缓冲；前端 objectURL 成对释放。
- 新增 `src-tauri/tests/memory_bench.rs`（`#[ignore]`）：50 张 1600×1200、100 张；
  `/usr/bin/time -l` 记录峰值 RSS 与耗时，对照第六轮 773MB。
- 未达 ≤600MB 时保留原始输出与原因分析，不改报告数字。
- 产出 `docs/ROUND6_MEMORY_OPTIMIZATION.md`。

---

## 3. RAW ENGINE

### 3.1 依赖与许可证

`libraw_rs_vendor = "1.0.0"`（LibRaw 0.21.1，LGPL-2.1-only OR CDDL-1.0，静态构建）。
仅用 LibRaw 做：识别、`unpack` 原始像素、CFA/黑电平/白电平/相机 WB/相机矩阵、元数据、内嵌预览；
**不调用 `dcraw_process`**。选择 0.21.1 而非 0.22.3 的理由（绑定成熟度/内存/构建稳定性）
与限制（无 Nikon HE/HE*）写入依赖审计与相机矩阵。
许可证路径：LGPL-2.1 静态链接 → 发行包提供 LibRaw 完整源码、构建脚本与重链接说明。

### 3.2 样本（CC0，仓库外 `~/Library/Caches/lumi-raw-samples`）

| 文件 | 字节 | SHA-256 前 16 位 |
|---|---|---|
| `_MG_2233.CR3`（Canon EOS M200） | 17,606,963 | `0585a468143c6606` |
| `_DSC0812.ARW`（Sony DSC-RX1） | 25,144,576 | `e17c13bdff09558e` |
| `P1000951.RW2`（Panasonic DC-G9） | 24,256,000 | `16282357ce0143ff` |
| `E_1__C106743_gredos.ORF`（Olympus E-1） | 10,661,632 | `042286653fbae5b0` |
| `_IGP2068.DNG`（Pentax K-01） | 16,853,508 | `4c9aac2ce2720154` |
| `5G4A9395-compressed-lossy.DNG`（5D3 DNG） | 2,484,836 | `b22f1e36331f679a` |

补采 Nikon NEF 与 Fujifilm RAF 各 1 个；若源不可得，矩阵如实写“无样本/未验证”。

### 3.3 模块

```text
src-tauri/src/image_engine/raw/
├── mod.rs          # 门面 + 能力/限制声明
├── libraw_ffi.rs   # 唯一 unsafe 边界（生命周期、返回码、panic 隔离、上限）
├── metadata.rs     # 品牌/型号/镜头/ISO/快门/光圈/焦距/时间/曝光补偿/相机WB（缺失=Unknown）
├── linear.rs       # LinearImageF32 + 线性范围与传递函数说明
├── demosaic.rs     # Malvar-He-Cutler 5×5；Bayer RGGB/BGGR/GRBG/GBRG
├── xtrans.rs       # X-Trans 6×6 基础插值（明确标注质量等级低于 AHD）
├── develop.rs      # 曝光/高光/阴影/相机WB/自动WB/色温/色调
└── preview.rs      # 内嵌预览提取
```

- 安全：最大输入 512MB、最大像素 200MP、解码线程可取消、`catch_unwind`、
  所有返回码映射 `AppError`；截断/损坏/异常 EXIF/TIFF Tag/非法 CFA/超大图 全部测试。
- 线性语义：`LinearImageF32` 为相机原生线性值（`/white` 后约 0..1，保留 >1 高光与负值），
  相机矩阵 → XYZ(D50 适配) → 线性 sRGB → 现有 `Image16`。
- 开发参数独立 `RawDevelopParams`，不进 `EditParams`。
- 预览：内嵌预览秒开；编辑用受控尺寸解码；导出全尺寸重新解码，同一 `develop` 函数。

### 3.4 项目格式

`v2 → v3` 增 `source{type,path,hash,metadata}`、`raw`、`color`、`compute`；
v2 项目零改动可打开；RAW 缺失时保留参数并提示重定位，禁止静默低清导出；
解码器版本变化时提示可能需要重新渲染。

---

## 4. ICC COLOR MANAGEMENT

### 4.1 依赖

`lcms2 = { version = "6.3.0", features = ["static"] }`（Little CMS，MIT）。
读取 ICC v2/v4、RGB/灰度/CMYK、用户 Profile；损坏与超大 Profile 有上限与错误路径。

### 4.2 色彩架构（写入 `ROUND7_COLOR_PIPELINE.md`）

```text
输入（RAW 线性 / Raster+ICC）
  → 输入转换到工作空间
  → 线性 ProPhoto RGB（RAW 开发：曝光/高光恢复在此完成）
  → 线性 sRGB → sRGB 编码 Image16（编辑层，保持现有算法语义）
  → 现有调色/美肤/塑形
  → 输出 Profile 转换（lcms2，16-bit）
  → 编码 + 嵌入 Profile
```

- **工作空间定义**：ProPhoto RGB 原色，D50 白点，RAW 阶段线性（gamma 1.0），
  编辑层为 sRGB 编码（IEC 61966-2-1），数值范围 0..1（高光在 RAW 阶段已处理）。
  两者边界处的转换由 lcms2 完成，不手写近似矩阵。
- 无 ICC 的 JPEG/PNG：按 sRGB 处理并在 UI 标注“未指定配置，按 sRGB”。
- 显示：读取 macOS 主显示器 ICC（`CGDisplayCopyColorSpace` + `CGColorSpaceCopyICCData`，
  raw FFI），lcms2 转显示 Profile 后输出预览；读取失败回退 sRGB 并在状态栏显示。
  多显示器切换监听 `NSApplicationDidChangeScreenParametersNotification`，
  失败则提供手动刷新按钮。
- 输出：sRGB / AdobeRGB / Display P3 / ProPhoto / 用户 ICC；意图 + BPC + 是否嵌入。
- 软打样：真实 lcms2 转换；色域警告 = 工作像素经输出 Profile 往返后 ΔE00 > 2.0。
- 16-bit TIFF：自实现未压缩 RGB16 TIFF（II、strip、tag 34675 嵌入 ICC），非改后缀。
- CMYK：读取 Profile 并把 CMYK TIFF/JPEG 转 sRGB（转换质量标注为“基础”）。

### 4.3 模块与嵌入

```text
src-tauri/src/color_management/
├── mod.rs        # ColorEngine 状态与能力
├── profiles.rs   # 内置/文件/显示 Profile
├── transform.rs  # 输入/输出/软打样/意图/BPC/色域
├── embed.rs      # JPEG APP2 / PNG iCCP / TIFF 34675 读写
├── display.rs    # ColorSync 显示 Profile（macOS FFI）
└── lab.rs        # RGB↔Lab、ΔE00
```

`export::write_atomic` 增加 `ColorOutput`，批量导出复用同一函数；
测试断言不会二次转换（转换后再读回数值不再偏移）。

---

## 5. GPU ACCELERATION

- `wgpu = "30.0.2"` + `pollster = "0.4"`；`rust-version` 提升至 1.87（本机 1.95 已验证）。
- 模块：`gpu/{mod,device,ops}.rs` + `shaders/exposure.wgsl`（曝光+色彩矩阵+sRGB 编解码）、
  `shaders/curve.wgsl`（曲线 LUT + 饱和度）。
- 探测/初始化失败、Shader 编译失败、纹理超限、OOM、DeviceLost、取消 → 全部回退 CPU 并记日志。
- 一致性：平均 ΔE00 ≤0.5、P95 ≤1.0；NaN/Inf/Alpha/边界像素检查；不达标则记录真实数值且不默认启用。
- 基准：1600×1200 / 3000×4000 / 6000×4000（模拟 24/45/60MP），记录 CPU、上传、运算、读回、总耗时、
  RSS；端到端 ≥1.25× 才默认启用。
- UI/状态栏如实显示设备、后端、启用状态与失败原因。

---

## 6. 统一管线、UI 与批量

- `pipeline::render` 前置 ingestion：文件类型识别 → RAW/Raster → 输入 ICC → `Image16`；算法顺序不变。
- GPU 仅替换曝光与曲线两处，数学定义与 CPU 相同，其余保持 CPU。
- 新增命令：`raw_info`、`raw_preview`、`raw_develop`、`color_status`、`color_profiles`、
  `color_set_output`、`soft_proof`、`gpu_status`、`set_compute_backend`。
- UI：`RawPanel`（信息/曝光/高光/阴影/WB/色温色调/去马赛克/默认方案）、
  `ColorPanel`（输入/工作/显示/输出/意图/BPC/嵌入/软打样/色域）、
  `PerformancePanel`（设备/GPU 状态/CPU-GPU 切换/显存/性能）、状态栏（文件类型/工作空间/输出空间/设备/状态）。
  不可用能力 disabled 并显示原因。
- 批量：`BatchPhoto` 增 `raw`、`output`；逐张“读取→解码→线性→转换→渲染→输出→释放”，
  RAW 不预解码；支持 TIFF/输出 Profile。

---

## 7. 测试与验收

执行并保存日志：

```bash
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
npm run typecheck && npm run test && npm run build && npm run app:build
```

新增测试：RAW（真实样本、CFA/WB/黑电平、四种 Bayer、细线/彩色边缘、截断/损坏/超大/非法 CFA、
取消、内存上限、参数生效、项目往返）、ICC（v2/v4、sRGB/AdobeRGB/P3/ProPhoto、无/损坏/超大 Profile、
ΔE00、嵌入回读、软打样/色域、TIFF 16bit、CMYK）、GPU（一致性、极端值、回退注入、大图）、
一致性（单张 vs 批量 vs 预设 vs 复制参数）、内存基准。

验收门槛：
1. 空白窗口修复，GUI 验收逐项有证据。
2. 50 张批量 ≤600MB（原始 `time -l` 输出）。
3. 真实 RAW 全链路可用、元数据真实、原图不变。
4. ICC 数值真实改变、嵌入可回读、无重复转换。
5. GPU 两项运算实测 + CPU 对比 + 回退验证。
6. 全部命令与构建通过。
7. 10 份文档齐备，未验证项（Windows 构建、未获取样本的机型）如实标注。

---

## 8. 执行顺序

T0 空白窗口修复 → T1 GUI 验收 → T2 能力标注 → T3 内存优化 →
T4 RAW 依赖与最小验证 → T5 RAW Engine → T6 ICC → T7 GPU →
T8 统一管线/UI/批量 → T9 全量测试构建 → T10 文档与提交（完成即停止，不进第八轮）。

批准后从 T0 开始执行，每个里程碑按用户要求上报一次。
