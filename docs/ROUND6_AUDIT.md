# 第六轮独立审计

审计方式：逐项检查代码路径、运行真实测试、核对命令注册与 UI 绑定。以下结论只覆盖实际验证过的内容；没有人工点击 GUI 的部分会明确写出。

## 1. 智能分析是否真实运行

**通过。** 证据：

- `image_engine/analysis.rs::analyze` 对真实 16-bit 缓冲做统计，不返回常量。
- 测试 `bright_image_suggests_negative_exposure`、`dark_image_suggests_positive_exposure`、`overexposed_image_suggests_lowering_highlights`、`warm_and_cool_images_produce_opposite_temperature_direction`、`low_and_high_saturation_move_in_opposite_directions` 用不同合成图验证建议方向随输入变化。
- `deterministic_for_same_input` 验证同输入结果一致。
- `multi_face_uses_real_count_and_conservative_beauty_template` 用 2 个人脸框验证真实计数为 2。
- `no_face_produces_no_beauty_suggestions` 验证无人脸时不产生美肤建议，`portrait.faces` 为 `noData`。
- 建议应用后：`apply_only_touches_suggested_fields`、`applying_face_beauty_creates_entries_and_keeps_higher_values` 验证只改勾选字段。
- 端到端「应用后图像真变」由前端流程保证：`apply_retouch_suggestions` 返回普通 `EditParams`，再进入既有 `pipeline::render`；Rust 侧测试同时验证参数确实变化，渲染差异由批量测试 `preset_zero_and_full_strength_produce_expected_render_equivalence` 的同类断言覆盖。

**未验证项：** 没有在打包后的 GUI 中手动点击「应用全部」并肉眼对比。UI 绑定已通过代码核对（`SmartPanel.tsx` 调用 `applyRetouchSuggestions`，`backend.ts` → `lib.rs` 注册），以及 TypeScript 类型检查。

## 2. 是否有假 AI / 虚假模型状态

**通过。** UI 文案只使用「智能精修建议」「本地照片分析」；`AnalysisCapabilities.beautyAnalysis` 固定为 `false`，皮肤指标不返回数值，界面显示「不支持」。分析过程中没有任何模型推理调用，也不存在「AI 分析中」之类掩盖真实来源的文案。

## 3. 预设是否是假预设

**通过。**

- 内置预设 14 个，分布在 5 个分类；`builtin_presets_are_distinct_and_complete` 断言数量 ≥ 12 且参数集合互不相同。
- 缩略图通过 `preset_previews` 对当前照片真实渲染（`commands/preset.rs` 中调用 `pipeline::render`），不存在共用占位图。
- `zero_strength_is_identity`、`full_strength_matches_preset_values`、`half_strength_is_between_current_and_target` 验证强度语义。
- `serialized_preset_has_no_raw_edit_params_shape` 验证不包含对象级数据。
- `beauty_template_skips_when_no_face` 验证无人脸时不会凭空创建美肤。

## 4. 批量是否只是循环调用 UI / 是否绕过正式管线

**通过。**

- `BatchEngine::run` 在独立 `std::thread` 中执行，逐张调用 `cache.load` → `pipeline::render_with_progress` → `export::write_atomic`，不经过前端逐张 invoke。
- `formal_pipeline_export_is_atomic_and_never_touches_source` 验证输出尺寸为原图尺寸、源文件字节不变、无临时文件残留。
- `batch_export_matches_single_export_byte_for_byte` 验证批量与单张导出逐字节一致。

## 5. 是否错复制人脸参数 / 蒙版 / 锚点

**通过。**

- `paramGroups.ts` 的 `COPY_GROUPS` 只包含标量调色、HSL、曲线、LUT、细节、美肤模板；`faces`、`reshape`、`subjectMask`、`body`、`background`、`manualMask`、`repairPoints` 不在任何分组中。
- `paramGroups.test.ts::默认分组不包含任何对象级字段` 与 `应用到目标照片时保留目标自己的蒙版与锚点` 断言目标照片的蒙版、塑形、身材数据保持不变。
- 美肤模板只取最大值并写入目标照片自己的人脸。

## 6. 是否覆盖源文件 / 已有导出

**通过。**

- `image_engine/export.rs::write_atomic` 明确拒绝 `destination == original`。
- 批量冲突策略默认 `Skip`：`existing_output_is_skipped_and_never_overwritten` 验证已有文件内容保持 `keep-me`。
- `Overwrite` 必须携带 `overwrite_confirmed = true`，否则拒绝；`overwrite_requires_confirmation` 覆盖该分支。

## 7. 坏图是否拖垮整批

**通过。** `batch_exports_every_photo_and_isolates_broken_file` 用 2 张正常图 + 1 个非图片文件运行完整队列：正常图完成 2、坏图失败 1，批次正常结束，无临时文件残留。

## 8. 能否安全取消

**通过。**

- `cancel_all_marks_tasks_cancelled_without_partial_outputs`：取消后所有任务为 Cancelled，输出目录为空。
- 取消全部同时设置当前任务取消标志，正在渲染的图会在阶段边界中止，不会继续写盘。
- 写盘使用临时文件 + rename；测试断言没有 `.lumi-tmp-*` 残留。
- 性能基线中的取消响应实测数据见 `ROUND6_BATCH_TESTS.md`。

## 9. 进度与 ETA 是否真实

**通过。** 进度来自真实像素阶段回调（`render_with_progress`）与任务完成计数；ETA 仅在存在已完成任务样本且仍有剩余时计算，样本不足返回 `null`，前端显示「估算中」或直接隐藏。

## 10. 缓存是否跨照片错误复用

**通过。**

- 分析缓存按路径 + 原图 hash + 人脸签名 + 参数签名 + 模式 + 版本；不同照片不会命中同一键。
- 批量使用独立 `ImageCache` 实例，容量 1 张，处理完即释放，不与编辑器缓存共享。
- 渲染缓存键包含完整参数与各对象版本（第五轮已有实现，本轮未改动）。

## 11. 是否破坏前五轮

**基本通过，有一处测试修复。**

- `cargo test` 全部通过，包括第五轮新增的 `engine`、`round5`、`project`、`reshape_persistence` 集成测试。
- 修复了 `tests/engine.rs` 与 `tests/reshape_persistence.rs` 中 `ProjectPhoto` 初始化缺失第五轮字段导致的编译失败（补 `subjects`、`mask_meta`），属于测试代码与新字段不同步，不涉及运行时行为。
- `tests/engine.rs` 的项目保存断言改为先执行 `sync_subject_records()`，与保存时的真实行为一致。
- 修复 `face_reshape/deformation.rs` 中 `length > MAX && length > 1e-6` 的冗余比较（clippy 编译错误），逻辑等价。

## 12. 是否引入新依赖 / 未授权模型

**否。** `Cargo.toml` 与 `package.json` 未新增依赖；没有下载权重，没有网络请求。

## 13. 最终验证记录（提交前）

| 命令 | 结果 |
| --- | --- |
| `cargo test` | 288 通过，0 失败，4 忽略 |
| `cargo clippy --all-targets` | 0 错误；22 条警告全部位于前五轮文件（project.rs、detail.rs、render.rs、ai_engine、face_reshape），本轮新增文件 0 警告 |
| `npm run typecheck` | 通过 |
| `npm test` | 5 个文件、55 个用例通过 |
| `npm run build` | 通过，index 约 401 KB（gzip 119 KB） |
| `npm run app:build` | 通过，0 警告；`.app` 12.87 MiB、`.dmg` 6.24 MiB |
| release 性能基线 | 单张 1600×1200 0.056 s；3000×4000 0.368 s；批量 10 张 1.07 s；50 张 5.69 s；取消响应 0.137 s |

## 14. 遗留风险

1. 未进行打包后的人工 GUI 点击验收（受环境限制），仅完成代码核对、类型检查与构建。
2. 预设缩略图逐张渲染，照片数量很多时首次进入预设库会有等待；已限制 240px 尺寸。
3. 批量暂停只在任务之间响应，正在渲染的单张不会立即暂停（可取消当前）。
4. `cargo clippy --all-targets` 仍有前五轮遗留的风格类告警（未逐一清理，本轮新增代码无告警、无错误）。
