# ROUND 9 准备：LibRaw 发行合规行动清单

> 状态：**未完成合规审核。不得对外宣称「LibRaw 已合规」。**
> 当前构建仍静态链接 LibRaw 0.21.1（经 `libraw_rs_vendor 1.0.0`），
> 尚未随安装包提供完整对应源码、补丁与可重链接材料。

## 1. 事实基线（已核实）

| 项 | 当前值 | 证据 |
| --- | --- | --- |
| Rust crate | `libraw_rs_vendor 1.0.0` | `src-tauri/Cargo.toml:27` |
| 上游 LibRaw | 0.21.1 | `libraw/libraw/libraw_version.h`：MAJOR 0 / MINOR 21 / PATCH 1 |
| 链接方式 | 静态链接进主二进制 | crate `build.rs` + `lcms2` 静态特性 |
| 许可证 | LGPL-2.1 或 CDDL-1.0（双许可，由使用者选择） | `libraw/COPYRIGHT`、`LICENSE.LGPL`、`LICENSE.CDDL` |
| 分发方式 | macOS `.app` + DMG | `src-tauri/target/release/bundle/` |
| 源码可得性 | 仅在本机 cargo registry 缓存中 | 未随 DMG 提供 |
| 法律审核 | **未进行** | 无审核记录 |

## 2. LGPL-2.1 静态链接的核心义务

选择 LGPL-2.1 路径时，发行静态链接二进制必须满足：

1. **提供完整对应源码**（Corresponding Source）：LibRaw 0.21.1 全部源码 + 本项目的修改/补丁 + 构建脚本。
2. **允许用户重新链接**：提供目标文件或等效机制，使用户能修改 LibRaw 并重新生成可运行程序。
3. **许可证文本随附**：LGPL-2.1 全文、版权声明、修改说明。
4. **不得附加限制**：不能阻止用户反向工程用于调试其修改后的 LibRaw。

> 若无法满足 2（重链接材料），则应改为**动态链接** LibRaw 动态库，
> 或改为 CDDL-1.0 路径（仍需提供源码与许可证，且 CDDL 与 GPL 不兼容，需单独评估）。

## 3. 交付物清单（第九轮必须补齐）

| # | 交付物 | 目标路径 | 当前状态 |
| --- | --- | --- | --- |
| L1 | LibRaw 0.21.1 完整源码归档 | `compliance/libraw/libraw-0.21.1.tar.xz` | 未提供 |
| L2 | 本项目补丁集（如有） | `compliance/libraw/patches/*.patch` | 未整理 |
| L3 | 构建脚本与编译参数 | `compliance/libraw/build.md` + 实际脚本 | 未整理 |
| L4 | 静态链接重链接材料 | `compliance/libraw/relink/`（`.o`/`.a` + 链接命令） | 未提供 |
| L5 | 许可证全文与版权声明 | `compliance/libraw/LICENSE.LGPL`、`COPYRIGHT` | 未随包 |
| L6 | 安装包内合规附件 | DMG 内 `Licenses/LibRaw/` | 未加入 |
| L7 | 第三方许可总表更新 | `THIRD_PARTY_LICENSES.md` | 已有行，缺源码指针 |
| L8 | 打包后检查脚本 | 校验 DMG 内附件齐全、可解压 | 未编写 |
| L9 | 法律审核记录 | `compliance/libraw/legal-review.md` | 未进行 |

## 4. 执行步骤

1. **锁定版本**：确认 `libraw_rs_vendor 1.0.0` 内嵌源码与 LibRaw 0.21.1 官方发布一致（SHA256 对比）。
2. **导出源码**：从 vendor 目录复制完整 `libraw/` 树，生成归档并记录 SHA256。
3. **整理补丁**：对比官方 0.21.1 与 vendor 源码，输出 diff；无修改也要写明「无补丁」。
4. **构建记录**：记录 `build.rs` 实际编译参数、编译器版本、目标三元组、静态库路径。
5. **重链接材料**：
   - 保留 `libraw.a` 与所有参与链接的目标文件；
   - 提供可复现的链接命令（含 `lcms2`、系统框架）；
   - 在文档中说明用户如何替换 `libraw.a` 并重新链接。
6. **打包**：将 L1/L2/L3/L5 放入 DMG 的 `Licenses/LibRaw/`，并在「关于」或文档中给出获取路径。
7. **检查**：挂载 DMG，逐项确认附件存在、许可证可读、源码归档可解压。
8. **法律审核**：由法务确认 LGPL-2.1 静态链接方案是否可接受；若否，切换动态链接或 CDDL 路径。
9. **状态更新**：只有 L1–L9 全部完成后，才能把 `ROUND8_LIBRAW_DISTRIBUTION_COMPLIANCE.md` 的状态改为「合规」。

## 5. 打包检查脚本（建议）

```bash
#!/usr/bin/env bash
set -euo pipefail
DMG="$1"
MNT=$(mktemp -d)
hdiutil attach "$DMG" -mountpoint "$MNT" -nobrowse -quiet
trap 'hdiutil detach "$MNT" -quiet || true; rmdir "$MNT"' EXIT
test -f "$MNT/Licenses/LibRaw/LICENSE.LGPL"
test -f "$MNT/Licenses/LibRaw/COPYRIGHT"
test -f "$MNT/Licenses/LibRaw/libraw-0.21.1.tar.xz"
test -f "$MNT/Licenses/LibRaw/build.md"
tar -tf "$MNT/Licenses/LibRaw/libraw-0.21.1.tar.xz" >/dev/null
echo "LibRaw 合规附件检查通过（不代表法律审核通过）"
```

## 6. 当前结论

- **未合规**：缺少随包源码、补丁说明、重链接材料与法律审核。
- 现有 `THIRD_PARTY_LICENSES.md` 只登记了依赖与许可证名称，不构成 LGPL 履行。
- 第九轮如继续静态链接，必须完成 L1–L9；否则应改为动态链接或移除 LibRaw 路径。
- 在此之前，对外材料不得出现「LibRaw 已合规」「已满足 LGPL」等表述。
