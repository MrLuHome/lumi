# ROUND 10 — 打包应用 GUI 验收

- 日期：2026-11-22
- 验收对象：`/Applications/LUMI RETOUCH.app`（由 `LUMI RETOUCH_0.1.0_aarch64.dmg` 安装）
- 包 SHA256：DMG `f771d8c58fc8b3cad04276dd3d3f2803f74b3e3abdab9b3e03d415d8ef3a909f`；主程序 `02da73e2987a38ff0ac554ecb62aa9ee72caf0c3fc687a36c9f48d01b6dd68c4`
- 截图目录：`/tmp/lumi-round10/gui/shots`
- 测试项目：`/tmp/lumi-round10/gui/round10.lumi`、`round10-final.lumi`
- 测试照片：`/tmp/lumi-round10/photos/portrait-a.jpg`（3000×4495）、`portrait-group.jpg`

## 1. 安装与启动

| 步骤 | 结果 | 证据 |
| --- | --- | --- |
| 挂载 DMG | PASS | `beta2-01-installed.png`（可见 App 与 Applications 快捷方式） |
| 覆盖安装到 `/Applications` | PASS | 替换确认；安装副本二进制 SHA256 与 DMG 内一致 |
| 首次启动 | PASS | `beta2-02-launched.png`，空图库「导入照片开始修图」 |
| 启动后无崩溃/白屏 | PASS | 主界面完整渲染，顶部「工程底座 v0.1」 |

## 2. 真实项目打开与参数恢复

| 步骤 | 结果 | 证据 |
| --- | --- | --- |
| 打开 `round10.lumi` | PASS | `beta2-04-project.png`，图库 02 |
| 参数恢复 | PASS | 曝光 30、对比度 20、饱和度 25、磨皮 50、瘦脸 50 |
| 照片可显示 | PASS | 画布显示 portrait-a.jpg |
| 状态栏项目名 | PASS | `项目：round10.lumi` |

## 3. RAW 导入拒绝（Beta 限制）

- 操作：导入 `sample-canon.cr2`。
- 提示原文：

```
不支持的图片格式：sample-canon.cr2 是 RAW 文件，当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）
```

- 结果：PASS（明确失败，图库仍为 02，未产生任何导入项）。
- 证据：`beta2-03-raw-rejected.png`。

## 4. 编辑 → 保存 → 退出 → 重开 → 导出

| 步骤 | 结果 | 证据 |
| --- | --- | --- |
| 打开 round10.lumi | PASS | `final-01-open.png`，曝光 30 / 对比度 20 |
| 修改曝光 30 → 40 | PASS | `final-02-edit.png`，出现「未保存」提示 |
| 保存到新项目文件 | PASS | Finder 复制为 `round10-final.lumi` 后应用内 ⌘S；`final-03-saved.png`，状态栏「项目：round10-final.lumi」且未保存提示消失 |
| ⌘Q 退出 | PASS | `final-04-quit.png`，进程退出，无未保存确认（已落盘） |
| 重启并打开新项目 | PASS | `final-05-reopened.png`，曝光 40、对比度 20、饱和度 25、「5 项已调整」 |
| 导出 JPEG | PASS | `final-06-export-jpeg.png`，提示 `已导出 final-jpeg.jpg · 5.2 MB · 732 ms` |
| 导出 PNG | PASS | `final-07-export-png.png`，提示 `已导出 final-png.png · 55.7 MB · 1.04 s` |
| 导出 TIFF | PASS | `final-08-export-tiff.png`，提示 `已导出 final-tiff.tiff · 77.2 MB · 557 ms` |

导出文件实测（`/tmp/lumi-round10/export/final/`）：

| 文件 | 大小 | 分辨率 | 位深/编码 | ICC |
| --- | --- | --- | --- | --- |
| `final-jpeg.jpg` | 5,438,818 B | 3000×4495 | JPEG 8-bit baseline | sRGB built-in |
| `final-png.png` | 58,416,071 B | 3000×4495 | PNG 16-bit RGBA | sRGB built-in |
| `final-tiff.tiff` | 80,910,780 B | 3000×4495 | TIFF RGB 16-bit 无损 | sRGB built-in |

- 三种格式均为原始分辨率，未被缩小。
- 导出后源图哈希不变：`portrait-a.jpg` = `a29a7077…`，`portrait-group.jpg` = `b9f99f52…`。
- 文件存在证据：`final-09-export-files.png`。

## 5. 关窗保护

- 阶段 C 已验证：存在未保存修改时关闭窗口弹出原生确认，取消则保留窗口。
- 证据：`step20-*` 系列（见 `ROUND10_UX_REVIEW.md`）。

## 6. 批量导出、人脸与塑形

- 阶段 B/C 已完成：导入多图、人脸检测、美肤、瘦脸 50（实际 42%）、批量导出。
- 证据：`step01`–`step14`、`step30c-reshape-slim50.png`。
- 身材局部变形与背景处理由引擎测试覆盖（`ai_engine::body` 22 个测试、背景模块测试），本轮未做新的 GUI 拖拽蒙版截图，列为限制。

## 7. 验收中发现的问题

1. **没有“另存为”**：⌘S 覆盖当前项目；新项目副本通过 Finder 复制完成。影响：用户可以保存，但不能在应用内另存。记入 1.1，不阻塞 Beta。
2. **⌘O 与 ⇧⌘O 语义不同**：⌘O 是导入照片，打开项目是 ⇧⌘O；验收时按 ⌘O 未打开项目。已记入 `ROUND10_UX_REVIEW.md`。
3. **导出成功提示约 1–2 秒消失**：截图未捕获 toast，但同一时刻无障碍树与文件落盘均可验证。
4. 首次导出时文件名输入未生效，产生 `portrait-a-lumi.jpg`；重新导出后 `final-jpeg.jpg` 正确。属于操作流程问题，导出功能本身正常。

## 8. 结论

- 打包应用可以完成：安装 → 启动 → 打开项目 → 编辑 → 保存 → 退出 → 重开 → 继续编辑 → 导出三种格式。
- 没有观察到数据丢失、项目损坏、预览/导出分辨率不一致或静默覆写原图。
- 限制清楚且不阻塞 Beta 使用。
