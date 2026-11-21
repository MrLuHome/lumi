# ROUND 7 — 依赖与许可证审计

- 审计日期：2026-11-21
- 环境：Rust 1.95.0 / Node v22.15.0 / macOS（Apple M4）
- 范围：第七轮新增的 RAW、ICC、GPU 依赖；其余依赖沿用前六轮登记，见 `THIRD_PARTY_LICENSES.md`。

## 1. 新增直接依赖

| 名称 | 版本 | 用途 | 许可证（上游） | 分发方式 | 备注 |
|---|---|---|---|---|---|
| `libraw_rs_vendor` | 1.0.0（LibRaw 0.21.1） | RAW 识别、`unpack` 原始像素、元数据、内嵌预览 | LibRaw：LGPL-2.1-only OR CDDL-1.0；Rust 绑定按其仓库声明 | 静态链接进二进制 | 不调用 `dcraw_process`；发行包需履行 LGPL 静态链接义务（见 §2） |
| `lcms2` | 6.3.0（`static` feature） | ICC v2/v4 读取、16-bit 色彩转换、软打样 | Little CMS：MIT | 静态链接 `liblcms2.a` | 本机取自 `/opt/homebrew/opt/little-cms2` |
| `wgpu` | 30.0.2 | Metal 计算后端 | MIT OR Apache-2.0 | 编译进二进制 | 仅 macOS Metal 实机验证；其他平台未验证 |
| `pollster` | 0.4 | GPU 异步等待 | MIT OR Apache-2.0 | 编译进二进制 | — |
| `bytemuck` | 1 | GPU buffer 字节转换 | Zlib OR MIT OR Apache-2.0 | 编译进二进制 | — |
| `flate2` | 1 | PNG iCCP 压缩（ICC 嵌入） | MIT OR Apache-2.0 | 编译进二进制 | 已有传递依赖，本轮显式使用 |
| `tiff` | 0.9 | CMYK TIFF 读取与 16-bit TIFF 结构验证 | MIT | 编译进二进制 | 16-bit 导出编码器为自实现，不依赖此 crate 写文件 |

## 2. LibRaw 静态链接义务（未完成，发行前必须落实）

LibRaw 为 LGPL-2.1-only 或 CDDL-1.0 双许可，本项目按 LGPL-2.1 静态链接。发行包需要同时提供：

1. 使用的 LibRaw 0.21.1 **完整对应源码**（含任何本地修改；当前无修改）；
2. 可复现的构建脚本与编译选项；
3. 允许用户用修改后的 LibRaw **重新链接**本应用的说明（目标文件或静态库形式）。

当前仓库只保留 Cargo 依赖声明与 `Cargo.lock`，**尚未**在 `bundle.resources` 或发行附件中放入
LibRaw 源码包与重链接说明。该项在正式对外发行前必须补齐；本轮不宣称已完成发行合规打包。

## 3. 版本与构建验证

- `rust-version` 由 1.80 提升至 **1.87**（`wgpu 30` 要求），本机 1.95.0 编译通过。
- `cargo build` 与 `cargo test --lib` 通过；GPU 在 Apple M4 Metal 上实际运行（见 `ROUND7_GPU_BENCHMARK.md`）。
- release 配置 `panic = "abort"` 与 Rust 测试 harness 冲突，因此测试/基准使用 dev profile；
  应用本身仍以 release 打包。

## 4. 已知限制

| 项 | 状态 |
|---|---|
| Windows NSIS 实机安装 | **未验证**（无 Windows 机器） |
| Windows GPU（D3D12/Vulkan） | **未验证**；探测失败时回退 CPU |
| 其他 Linux 发行版 | **未验证** |
| LibRaw 0.22.3（Nikon HE/HE\*） | 未采用；0.21.1 明确不支持 HE/HE\* |
| 有损压缩 DNG | LibRaw 0.21.1 不支持，导入报明确错误 |
| ColorSync 多显示器监听 | 未实现事件监听，仅提供手动刷新；主显示器读取已实现 |
