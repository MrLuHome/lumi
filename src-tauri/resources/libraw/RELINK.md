# 使用修改后的 LibRaw 重新链接

LUMI 以静态方式链接 LibRaw。LGPL-2.1 允许你修改 LibRaw 并用修改后的版本重新链接本应用。
以下是可复现步骤（macOS arm64 / x86_64，Rust 1.95.0）。

## 1. 获取材料

- 应用包内：`LUMI.app/Contents/Resources/libraw/`
- 仓库内：`src-tauri/resources/libraw/`
- 源码包：`libraw_rs_vendor-1.0.0-source.tar.gz`

## 2. 解压并替换

```bash
tar -xzf libraw_rs_vendor-1.0.0-source.tar.gz
# 将解压目录放到本地 vendor 路径，例如：
mkdir -p ~/lumi-vendor
mv libraw_rs_vendor-1.0.0 ~/lumi-vendor/
```

在 `src-tauri/.cargo/config.toml` 加入（或合并）：

```toml
[patch.crates-io]
libraw_rs_vendor = { path = "/Users/<you>/lumi-vendor/libraw_rs_vendor-1.0.0" }
```

## 3. 修改与构建

在 `~/lumi-vendor/libraw_rs_vendor-1.0.0/libraw/` 中修改源码后：

```bash
cd src-tauri
cargo clean -p libraw_rs_vendor
cargo build --release
```

`libraw_rs_vendor/build.rs` 会：
- 用 `cc` 编译 75 个 C++ 源文件为 `raw` 与 `raw_r` 静态库；
- 追加 `-pthread`、`-Wno-deprecated-declarations`；
- 通过 bindgen 生成绑定。

## 4. 重新打包

```bash
npm run app:build
```

## 5. 链接顺序说明

- 目标文件/静态库由 `cc` 生成在 `target/release/build/libraw_rs_vendor-*/out/`。
- 最终链接由 `rustc` 驱动，LibRaw 静态库位于应用依赖之后；
- 系统框架（CoreFoundation、Security 等）由 Tauri 链路负责，无需手工追加。
- 如需完全手工重链接，可使用 `cargo rustc --release -- -C link-arg=...`，但不建议绕过 Cargo。

## 6. 法律说明

本说明不构成法律意见。分发修改版时，你仍需遵守 LGPL-2.1 的源码提供与声明义务。
