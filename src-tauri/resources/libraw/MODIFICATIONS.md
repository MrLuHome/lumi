# LibRaw 修改记录

- 对应版本：LibRaw 0.21.1（`libraw_rs_vendor 1.0.0` 内置）
- 记录日期：2026-11-21

## 结论

自 LUMI 引入 LibRaw 至本轮（第九轮）为止，**未修改 LibRaw 源码**。

- 未应用补丁；
- 未改动 `libraw/src/**`、`libraw/libraw/**` 任何文件；
- 仅通过 `libraw_rs_vendor/build.rs` 的编译选项构建（`-pthread`、
  `-Wno-deprecated-declarations` 等警告抑制），这些是构建配置而非源码修改。

## 未来修改时的义务

若为适配新相机或修复缺陷而修改源码，发布前必须：

1. 在本文件登记：文件路径、修改原因、补丁内容、修改日期；
2. 重新生成源码包并更新 `SOURCE.md` 中的 SHA256；
3. 在发行说明与“关于 → 第三方许可”中声明已修改；
4. 保持 `RELINK.md` 的重链接说明可用。
