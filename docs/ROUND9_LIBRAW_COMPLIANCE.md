# ROUND 9 — LibRaw 发行合规

- 日期：2026-11-21
- 对象：`libraw_rs_vendor 1.0.0`（内置 LibRaw 0.21.1），静态链接进 `lumi-retouch`
- 法律状态：**LEGAL_REVIEW_PENDING**
- 发行状态：**BLOCKED**（法律审核未完成；且当前 `.app` 为阶段 8 重建前的旧产物，未包含附件）

本文档只陈述事实与待办，不构成法律意见，也不宣布“已合规”。

## 1. 实际版本与来源

| 项 | 值 | 证据 |
| --- | --- | --- |
| Rust 绑定 crate | `libraw_rs_vendor 1.0.0` | `src-tauri/Cargo.lock` |
| crate checksum | `e7e8cce6adc7717b639654446091ede2b20aad58ce0cde8042ddf10f57303850` | `Cargo.lock` |
| 内置 LibRaw | 0.21.1 | crate 源码 `libraw/`；二进制内字符串 `LibRaw 0.21.1` |
| crate 上游 | https://github.com/mgolub2/libraw_rs_vendor | `SOURCE.md` |
| 源码包 | `libraw_rs_vendor-1.0.0-source.tar.gz`（600,419 B） | `src-tauri/resources/libraw/` |
| 源码包 SHA256 | `58317a2875f310a48cfbe03d92a178e148e32a3d032462eee559d8abcba924f9` | 实测 |
| 许可 | LGPL-2.1-only **或** CDDL-1.0（双许可） | crate 元数据与许可文本 |
| 本项目选择 | LGPL-2.1（静态链接路径） | 本文件 |
| 本地修改 | 无源码修改 | `MODIFICATIONS.md` |

## 2. 链接方式（实测）

- 静态库 `raw_r.a` 由 `build.rs` 用 `cc` 编译，经 `cargo:rustc-link-lib=static=raw_r` 编入可执行文件。
- `otool -L` 输出中**没有**任何 `libraw` 动态依赖（grep 无匹配）。
- 二进制内存在 LibRaw C++ 符号（如 `LibRaw::x3f_load_raw`）与版本字符串 `LibRaw 0.21.1`。
- 可执行文件约 15 MB，`.app` 约 16 MB。
- 结论：确认是**静态链接**，因此适用 LGPL-2.1 的“提供源码 + 允许重链接”义务，而不是动态库替换义务。

## 3. 材料清单与存放位置

仓库内：`src-tauri/resources/libraw/`；打包后：`LUMI RETOUCH.app/Contents/Resources/resources/libraw/`。

| # | 材料 | 文件 | 状态 |
| --- | --- | --- | --- |
| 1 | 完整对应源码 | `libraw_rs_vendor-1.0.0-source.tar.gz` | 已有，SHA256 已登记 |
| 2 | 版权声明 | `COPYRIGHT` | 已有 |
| 3 | LGPL-2.1 全文 | `LICENSE.LGPL-2.1` | 已有（24 KB） |
| 4 | CDDL-1.0 全文（另一许可选项） | `LICENSE.CDDL-1.0` | 已有（17 KB） |
| 5 | 版本/来源/许可/文件说明 | `SOURCE.md` | 已有 |
| 6 | 修改记录 | `MODIFICATIONS.md` | 已有（结论：无修改） |
| 7 | 重新链接说明 | `RELINK.md` | 已有 |
| 8 | 可复现构建脚本 | `libraw-build.sh` | 已有 |
| 9 | 打包声明 | `src-tauri/tauri.conf.json` → `bundle.resources` 含 `resources/libraw/*` | 已加入 |
| 10 | 目标文件 / 静态库（可选增强） | 构建时生成于 `target/release/build/libraw_rs_vendor-*/out/` | 未随包分发；由源码包 + 构建脚本替代 |

### 重链接路径

`RELINK.md` 给出用 `[patch.crates-io]` 指向修改后本地 vendor 目录、`cargo build --release` 的完整步骤；
`libraw-build.sh` 可按与 `build.rs` 相同的编译选项独立产出 `raw.a` / `raw_r.a`。
因此第三方可以在修改 LibRaw 后重新生成 `lumi-retouch`，满足 LGPL-2.1 §6 的重链接要求。
材料是否“充分”仍需法务确认（见第 6 节）。

## 4. 自动检查

脚本：[check_release_compliance.sh](../../scripts/check_release_compliance.sh)

```bash
./scripts/check_release_compliance.sh
# 可选：--app <path.app> --dmg <path.dmg> --nsis <path.exe>
```

检查项：Cargo.lock 版本、三份许可/版权文件、源码包存在且 SHA256 与登记一致、
`SOURCE/RELINK/MODIFICATIONS/libraw-build` 是否齐全、`.app` 内是否包含全部附件、
DMG 是否不早于 `.app`、Windows NSIS 是否存在。任何缺失 → `RESULT: BLOCKED` 且退出码 1；
即使全部通过，脚本仍输出 `LEGAL_REVIEW_PENDING`，永不打印 `Release Ready` / `COMPLIANT`。

本轮实测（阶段 8 重建产物后，仓库材料与 `.app`/DMG 全部 PASS）：

```
PASS  Cargo.lock 锁定 libraw_rs_vendor 1.0.0（内置 LibRaw 0.21.1）
PASS  许可/版权文件存在：LICENSE.LGPL-2.1 / LICENSE.CDDL-1.0 / COPYRIGHT
PASS  对应源码包存在且 SHA256 一致（58317a28…）
PASS  材料存在：SOURCE.md / RELINK.md / MODIFICATIONS.md / libraw-build.sh
PASS  找到 .app
PASS  .app 内包含全部 LibRaw 合规附件
PASS  DMG 存在且不早于 .app
SKIP  未提供 Windows NSIS 安装包：跨平台合规未验证
RESULT: BLOCKED（原因：法律审核未完成 LEGAL_REVIEW_PENDING）
```

首次运行时（加入 `resources/libraw/*` 之前的旧 `.app`）曾出现
`FAIL .app 内缺少合规附件`；执行 `npm run app:build` 重建后该项已 PASS，
说明打包配置生效。

## 5. 与其它依赖的边界

- Apple Vision 是系统框架，不打包其二进制，不涉及 LGPL。
- 用户自备 ONNX 模型不进入安装包，不改变 LibRaw 义务。
- 项目夹具与测试模型（CC0-1.0，`ProtocolTestOnly`）不进入发行包。

## 6. 未完成的法律事项（LEGAL_REVIEW_PENDING）

1. 由法务确认：所选 **LGPL-2.1 + 静态链接 + 源码包 + 重链接说明** 的材料组合是否足以满足 §6；
2. 确认是否需要额外随包提供 `.o`/`.a` 目标文件（当前以构建脚本 + 源码包替代）；
3. 确认应用内“关于 → 第三方许可”页面展示与实际包内材料一致（界面项待阶段 8 验收）；
4. 确认 EULA / 使用条款不含禁止反向工程以调试 LibRaw 的条款；
5. Windows 安装包（NSIS）内容未在 Windows 实机复核，跨平台义务未验证。

## 7. 结论

- 版本、链接方式、许可选择、材料与检查脚本均已落实并可审计；
- **法律审核未完成**，状态保持 `LEGAL_REVIEW_PENDING`，发行状态 `BLOCKED`；
- 阶段 8 重建产物后须重跑检查脚本并更新本文档的检查结果。
