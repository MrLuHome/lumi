# ROUND 6.1 — 批量内存优化与实测

- 日期：2026-11-21
- 基线：第六轮 `937b304` 的批量导出 + 第七轮增量
- 机器：macOS / Apple M4 / 16 GB
- 构建：**dev profile**（release 配置 `panic = "abort"` 与 Rust 测试 harness 冲突，无法运行 release 测试二进制）
- 测试：`commands::batch::tests::batch_memory_bench`（`#[ignore]`，`ImageCache` 预算 256 MB）
- 素材：程序生成的 1600×1200 JPEG，无外部照片

## 1. 优化内容

1. **批量专用缓存**：`BatchEngine::run` 使用 `ImageCache::with_budget(1, 256 MB)`，不保留全尺寸原图。
2. **逐张作用域释放**：每张任务开始时 `cache.clear()`，处理完立即释放全尺寸缓冲，下一张再解码。
3. **不预解码**：批量只持有任务元数据；文件在轮到该张时才读取/解码。
4. **前端 objectURL 释放**：移除照片时对 `blob:` 预览调用 `URL.revokeObjectURL`；
   批量导出使用后端路径，不创建整批 blob。
5. 预览与导出的缓存互不合并，避免批量占用编辑会话缓存。

## 2. 原始测量

命令：

```bash
BIN=target/debug/deps/lumi_retouch_lib-e6582dfbcdd83f4a
LUMI_BENCH_COUNT=50 /usr/bin/time -l "$BIN" batch_memory_bench --ignored --nocapture
LUMI_BENCH_COUNT=100 /usr/bin/time -l "$BIN" batch_memory_bench --ignored --nocapture
```

原始日志：`docs/assets/round7/memory-bench-50.log`、`docs/assets/round7/memory-bench-100.log`。

### 50 张 1600×1200

```text
BENCH count=50 completed=50 failed=0 elapsed_ms=119860 exported_bytes=15325151
test result: ok. 1 passed; 0 failed; 0 ignored

      145.16 real       138.78 user         2.10 sys
           124878848  maximum resident set size
           110625464  peak memory footprint
```

### 100 张 1600×1200

```text
BENCH count=100 completed=100 failed=0 elapsed_ms=236088 exported_bytes=30796112
test result: ok. 1 passed; 0 failed; 0 ignored

      289.95 real       271.91 user         5.40 sys
           118521856  maximum resident set size
           107774624  peak memory footprint
```

## 3. 结论

| 指标 | 50 张 | 100 张 | 门槛 |
|---|---:|---:|---:|
| 峰值 RSS | **124.9 MB**（119.1 MiB） | **118.5 MB**（113.0 MiB） | ≤600 MB |
| 处理耗时 | 119.9 s | 236.1 s | — |
| 失败 | 0 | 0 | 0 |
| 导出字节 | 15.3 MB | 30.8 MB | — |

- 50 张与 100 张的峰值 RSS 基本持平（100 张甚至略低），说明内存不随张数线性增长。
- 相比第六轮记录的 773 MB 峰值，本轮降到约 **125 MB**，满足 ≤600 MB 验收。
- 耗时是 dev 构建、单线程逐张处理的结果；不是性能承诺，只用于证明内存上限。
- 未测 release 构建的 RSS；应用发行版使用 release，理论上不会高于 dev（无调试符号/边界检查），
  但本轮不把“release RSS”标为已验证。
