# LibRaw 源码与许可材料（发行附件）

本目录随 macOS/Windows 安装包一起分发，用于满足 LGPL-2.1 静态链接义务。
状态：**LEGAL_REVIEW_PENDING**（材料齐备待法务审核，不得标记 COMPLIANT）。

## 实际版本

| 项 | 值 |
| --- | --- |
| Rust 绑定 crate | `libraw_rs_vendor 1.0.0`（`Cargo.lock` 锁定） |
| 内置 LibRaw | 0.21.1 Release |
| crate 上游 | https://github.com/mgolub2/libraw_rs_vendor |
| crate 包 SHA256 | `e7e8cce6adc7717b639654446091ede2b20aad58ce0cde8042ddf10f57303850` |
| 许可 | LGPL-2.1-only **或** CDDL-1.0（本项目选择 LGPL-2.1） |
| 链接方式 | 静态库编入 `lumi-retouch` 可执行文件 |
| 本地修改 | 无源码修改；仅通过 crate 的编译配置构建 |

## 文件清单

| 文件 | 用途 |
| --- | --- |
| `libraw_rs_vendor-1.0.0-source.tar.gz` | 完整对应源码（含 LibRaw 0.21.1 与 Rust 绑定、build.rs） |
| `libraw_rs_vendor-1.0.0-source.tar.gz` SHA256 | `58317a2875f310a48cfbe03d92a178e148e32a3d032462eee559d8abcba924f9` |
| `LICENSE.LGPL-2.1` | LGPL 2.1 全文 |
| `LICENSE.CDDL-1.0` | CDDL 1.0 全文（双许可另一选项） |
| `COPYRIGHT` | LibRaw 版权声明 |
| `RELINK.md` | 第三方重新构建与重链接说明 |
| `libraw-build.sh` | 可复现构建脚本 |

## 修改记录

当前 **无修改**。若后续为适配新相机或修复缺陷而修改 LibRaw 源码，必须：
1. 在 `MODIFICATIONS.md` 记录文件、原因、补丁；
2. 重新生成源码包并更新 SHA256；
3. 在发行说明中声明修改。
