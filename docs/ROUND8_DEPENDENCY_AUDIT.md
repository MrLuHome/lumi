# ROUND 8 — 依赖审计

- 日期：2026-11-21
- 工具：`cargo tree --depth 1` + `Cargo.lock`（离线）；未运行 `cargo audit` / `npm audit`（见“未验证”）
- 结论：第八轮**没有新增第三方 crate**。人脸 Provider 抽象使用已有 `tract-onnx`；
  Apple Vision 走系统框架（编译期链接，不新增包）；用户 ONNX 解析沿用 tract。

## 1. 直接依赖（锁定版本）

| crate | 版本 | 用途 | 许可证（公开信息） | 本轮变化 |
|---|---|---|---|---|
| `tauri` | 2.12.7 | 桌面壳 / 命令 | MIT 或 Apache-2.0 | 无 |
| `tauri-build` | 2.7.6 | 构建脚本 | MIT 或 Apache-2.0 | 使用，无版本变化 |
| `serde` / `serde_json` | 1.0.229 / 1.0.154 | 序列化 | MIT 或 Apache-2.0 | 无 |
| `image` | 0.25.10 | JPEG/PNG/WebP 解码 | MIT 或 Apache-2.0 | 无 |
| `base64` | 0.22.1 | 预览传输 | MIT 或 Apache-2.0 | 无 |
| `rfd` | 0.15.4 | 文件选择对话框 | MIT | 新增调用（模型导入），无新 crate |
| `rayon` | 1.12.0 | 并行 | MIT 或 Apache-2.0 | 无 |
| `thiserror` | 2.0.21 | 错误类型 | MIT 或 Apache-2.0 | 无 |
| `tract-onnx` | 0.23.10 | ONNX 解析/推理/校验 | MIT 或 Apache-2.0（tract 项目） | 新增用途（模型校验），无版本变化 |
| `sha2` | 0.10.9 | SHA256 | MIT 或 Apache-2.0 | 新增用途（模型哈希） |
| `libraw_rs_vendor` | 1.0.0 | RAW 解码（内置 LibRaw） | 包装层 MIT；LibRaw 为 LGPL-2.1 / CDDL-1.0 | 无 |
| `lcms2` | 6.3.0（`static`） | ICC 转换 | MIT | 无 |
| `wgpu` | 30.0.2 | GPU 计算 | MIT 或 Apache-2.0 | 无 |
| `pollster` | 0.4.0 | 阻塞等待 async | MIT 或 Apache-2.0 | 无 |
| `flate2` | 1.1.11 | 压缩 | MIT 或 Apache-2.0 | 无 |
| `tiff` | 0.9.1 | 16-bit TIFF 导出 | MIT | 无 |
| `bytemuck` | 1.25.2 | GPU 缓冲转换 | MIT 或 Apache-2.0 / Zlib | 无 |

前端：`@tauri-apps/api 2.x`、`react 19`、`react-dom 19`、`zustand 5`、`lucide-react`（ISC）、
`vite 8`、`vitest 5`、`typescript 5.9`。第八轮未新增 npm 包。

## 2. 构建期要求

- macOS 需要 Xcode Command Line Tools（`clang`、`ar`）编译 `vision_bridge.m`；
  缺失时 `build.rs` 直接失败并给出原因，不会静默跳过。
- 链接系统框架：`Vision`、`Foundation`、`CoreGraphics`、`CoreFoundation`、`objc`。
- `tract-onnx` 默认特性即可，无额外原生库。

## 3. 许可证与分发风险

| 风险 | 说明 | 处理 |
|---|---|---|
| LibRaw LGPL-2.1 | 当前 `libraw_rs_vendor` 静态链接进应用 | 见 `ROUND8_LIBRAW_DISTRIBUTION_COMPLIANCE.md`；发行前必须满足 LGPL 义务，否则不得发布 |
| `lcms2 static` | MIT，静态链接可接受 | 保留版权声明 |
| 系统框架 | Apple SDK 条款 | 不打包框架本体；遵守 macOS 应用条款 |
| 用户模型 | 许可证未知 | 固定 `UserUnverified`，不随包分发，用户自担 |

## 4. 未验证项

- **没有运行 `cargo audit` / `npm audit`**：本机未安装/未执行，不能声称“无已知漏洞”。
- 没有做 SBOM 导出（CycloneDX/SPDX）。
- 没有复核每个传递依赖的许可证文本；上表是公开信息与项目惯例，未经法务确认。
- 没有测量依赖升级对包体积的影响（release 包 15.85 MiB，dmg 7.70 MiB，为本轮构建实测）。

## 5. 变更清单

第八轮新增的代码依赖面：

1. `tract-onnx`：仅用于用户模型结构校验与冒烟推理（已有依赖）。
2. `sha2`：用户模型 SHA256（已有依赖）。
3. `rfd`：模型文件选择（已有依赖）。
4. Objective-C 桥接：系统框架，无第三方二进制。

没有引入 Banuba、NVIDIA Maxine、ONNX Runtime、MediaPipe 或任何预编译推理引擎。
