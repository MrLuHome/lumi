# ROUND 10 阶段 D：性能与稳定性

> 测量环境：macOS（Apple Silicon，本机开发机），Release 构建，测试二进制直接运行。
> 所有数字为现场实测；没有通过缩小导出分辨率或降低画质换数字。

## 1. 测量命令

```bash
cd src-tauri

# 24MP / 45MP / 50 张批量 / GPU 基准
LUMI_TEST_PHOTOS=/tmp/lumi-round10/photos \
  cargo test --release --test round10_performance -- --nocapture --test-threads=1

# 单场景峰值内存（RSS）
BIN=$(ls -t target/release/deps/round10_performance-* | grep -v '\.d$' | head -1)
LUMI_TEST_PHOTOS=/tmp/lumi-round10/photos /usr/bin/time -l "$BIN" --exact performance_24mp --nocapture
LUMI_TEST_PHOTOS=/tmp/lumi-round10/photos /usr/bin/time -l "$BIN" --exact performance_45mp --nocapture
LUMI_TEST_PHOTOS=/tmp/lumi-round10/photos /usr/bin/time -l "$BIN" --exact batch_export_50_jpegs --nocapture

# 打包应用冷启动（启动到主窗口出现）
open -n "src-tauri/target/release/bundle/macos/LUMI RETOUCH.app"
```

## 2. 冷启动与项目打开

| 指标 | 实测 |
| --- | --- |
| 冷启动（重新构建后首次） | 0.58 s |
| 冷启动（随后两次） | 0.22 s / 0.22 s |
| 打开 `round10.lumi`（3000×4495，1 张照片，含人脸合并） | 约 2.5 s（GUI 观测） |
| 重开后首次预览 | 109–197 ms |
| 单字段 ChangeSet 保存 | 10–26 ms（项目文件约 12 KB） |

## 3. 24MP（6000×4000）

| 操作 | 时间 |
| --- | --- |
| 解码 | 108–134 ms |
| 预览缩放（preview 长边） | 249–264 ms |
| 缩略图 640 | 220–296 ms |
| 参数渲染（preview） | 33–49 ms |
| 参数渲染（full） | 370–632 ms |
| JPEG 导出（q92） | 341–360 ms |
| PNG 导出（16-bit ICC） | 478–487 ms |
| TIFF 导出（16-bit） | 179–231 ms |
| ChangeSet 保存 | 10–26 ms |

峰值 RSS：**1.41 GB**（同进程内完成解码、预览、全尺寸渲染与三种导出）。
说明：这是最坏组合，实际编辑时预览渲染约 33–49 ms，交互流畅。

## 4. 45MP（9000×5000）

| 操作 | 时间 |
| --- | --- |
| 解码 | 167–191 ms |
| 预览缩放 | 438–476 ms |
| 参数渲染（preview） | 25–29 ms |
| JPEG 导出（q92） | 585–594 ms |

峰值 RSS：**933 MB**。
45MP 未做全尺寸参数渲染 + 三格式导出的组合测量；按 24MP 的比例推算峰值会高于 1.4 GB，列为风险。

## 5. 50 张批量导出（24MP JPEG）

| 指标 | 实测 |
| --- | --- |
| 总耗时 | 16.43 s |
| 平均 | 约 329 ms/张 |
| 最慢 | 350 ms |
| 峰值 RSS | 643 MB |
| 结果 | 50/50 文件生成，无失败 |

GUI 端 2 张批量导出：1.67 s，2/2 完成、0 失败。

## 6. GPU

独立 `ComputeEngine` 基准（同一台机器，Release）：

| 分辨率 | CPU | GPU 总耗时（含上传） | 倍率 |
| --- | --- | --- | --- |
| 1600×1200（1.9MP） | 10.0 ms | 24.1 ms | 0.41× |
| 3000×4000（12MP） | 60.5–63.1 ms | 142–175 ms | 0.36–0.42× |
| 6000×4000（24MP） | 121.7–167.6 ms | 625–660 ms | 0.20–0.25× |

`verified_speedup = 0.36–0.43`，GPU 明显慢于 CPU（上传/回读成本主导）。
结论：**不把 GPU 接入主管线**；保留独立基准入口，不宣传 GPU 加速。

## 7. CPU 占用与稳定性

- 50 张批量：real 16.62 s / user 16.26 s，单核跑满，几乎无线程空转。
- 24MP 单场景（含编译后测试进程）：real 2.07 s / user 2.86 s / sys 0.17 s，平均约 146% CPU，来自解码与编码的并行。
- 长任务（导出/批量）均有进度或完成提示；渲染有取消令牌；本轮未观察到崩溃、死锁或缓存失控。

## 8. 风险与结论

1. 24MP 最坏组合峰值 1.41 GB、45MP 预估更高；8 GB 内存机器同时编辑多张 45MP 可能吃紧。建议 Beta 发行说明标注「建议 16 GB 内存，45MP 单张编辑」。
2. 45MP 的 PNG/TIFF 导出未实测；编码时间按 24MP 线性推算约 1–1.4 s，但未作为结论。
3. GPU 不接入主管线有实测依据，不是未做。
4. 性能数据来自开发机，不代表所有 Mac 配置；发行说明中注明测量环境。
