# ROUND 8.1 打包应用 GUI 验收

> 应用：`src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`
> DMG：`src-tauri/target/release/bundle/dmg/LUMI RETOUCH_0.1.0_aarch64.dmg`
> 截图目录：`docs/assets/round8_1/`
> 说明：所有结论来自实际打包应用；未执行项标「未验证」，不写 PASS。

## 1. 验收矩阵

| # | 项目 | 结果 | 证据 / 观察 |
| --- | --- | --- | --- |
| 1 | macOS `.app` 启动 | PASS | 主窗口正常，9 个工作区、空图库、导入入口可见。`01-launch.jpg` |
| 2 | 导入照片 + AI 人像工作区 | PASS | portrait.jpg（1680×2240）导入成功；AI 人像面板显示 Provider 能力与「当前使用」。`02-ai-workspace.jpg` |
| 3 | AI 人像分析 + Provider 卡片 | PASS | Apple Vision 分析成功：1 张人脸、85 点、158 ms；「当前使用」正确指向 Apple Vision，YuNet 显示「切换」。`03-analyze.jpg` |
| 4 | LEVEL 3 分项门控 | PASS | 四列（眼/鼻/唇/下巴）均 ✓；稳定性 2 次重复、眼距占比 0.000%、通过；结构 4/4；定位精度「未验证」。`04-level3-gate.jpg` |
| 5 | Provider 切换 | 初次 FAIL → 修复后 PASS | 初次验收发现切换瞬间仍显示旧 Provider 的 85 点表格与「LEVEL 3 已解锁」（`05-provider-switch-stale.jpg`）。修复两处：作废 `lastAnalysis`、清空本地稳定性探针与状态胶囊。最终复验：切换后 LEVEL 3 立即锁定、精度证据显示「未运行重复分析」、状态胶囊为「人脸 · 未分析」。`05-provider-switch.jpg`、`05-provider-switch-final.jpg` |
| 6 | 模型库 | PASS | 「用户自备 ONNX · 本机校验 · 不随安装包分发」，导入入口完整，已安装 0。`06-model-hub.jpg` |
| 7 | 保存 / 重开 | PASS（有时序抖动） | 保存 `lumi-project-1.lumi`；人脸参数按 faceUuid 归属（skinSmooth 17 / texture 33 / spots 11），重开后恢复一致；提示旧稠密结论与当前环境不一致已锁定。首次重开曾短暂显示 0，第二次正常。应用内无「关闭项目」，用退出+重开替代。`07-reopen.jpg`、`07b-reopen-ai.jpg` |
| 8 | 导出 | PASS | 提示「已导出 portrait-lumi.jpg · 386 KB」；文件 395,760 字节，JPEG 1680×2240。`08-export.jpg` |
| 9 | DMG 挂载 | PASS | 挂载 `/Volumes/LUMI RETOUCH`，含 `.app` 与 Applications 链接，已 detach。`09-dmg.jpg` |

## 2. 发现的缺陷与修复

### 2.1 Provider 切换后旧分析记录残留

- **现象**：切换 Provider 后，通知已提示「旧 Provider 的稠密关键点与 LEVEL 3 门控已清空」，
  但 AI 面板仍显示 Apple Vision 的 85 点表格与「LEVEL 3 已解锁」，
  直到重新点「分析当前照片」才变成 YuNet 的 5 点 / 锁定。
- **根因**：切换命令清空了 `FaceRepository` 的人脸 `dense/level3`，但
  `FaceIntelligence.last` 中的 `FaceAnalysisRecord` 未作废，UI 的 LEVEL 3 表格直接读 `status.lastAnalysis`。
- **修复**：新增 `FaceIntelligence::clear_last_analysis()`，在 `select_face_provider`
  检测到 Provider 变化时调用；前端 `refresh()` 后 `lastAnalysis` 为空，旧表格立即消失。
  同时清空本地 `probe` 状态，并把人脸结果 `model` 置为「未分析」，避免状态栏继续显示旧 Provider。
- **复验**：重新打包后切换 Provider，三项同时成立：
  1. LEVEL 3 立即显示「尚未运行 AI 人像分析」；
  2. 精度证据-稳定性显示「未运行重复分析」；
  3. 底部状态胶囊显示「人脸 · 未分析」。

## 3. 未验证项

| 项目 | 状态 | 原因 |
| --- | --- | --- |
| 用户 ONNX 模型真实推理（导入→分析） | 未验证 | 无授权的真实第三方模型可用于验收；只验证了 Worker 限额与失败路径（单元测试） |
| 项目「关闭」菜单 | 未验证 | 应用当前无独立「关闭项目」入口，用退出应用替代 |
| 首次重开参数短暂显示 0 | 未验证（偶发） | 第二次重开未复现；疑似刷新时序，未定位到稳定复现路径 |
| 定位精度 | 未验证 | 无人工标注集与 NME（见 `ROUND9_NME_PREPARATION.md`） |
| LibRaw 合规 | 未验证 | 未完成源码/重链接材料与法律审核（见 `ROUND9_LIBRAW_ACTION_PLAN.md`） |

## 4. 截图清单

| 文件 | 内容 |
| --- | --- |
| `01-launch.jpg` | 应用启动主界面 |
| `02-ai-workspace.jpg` | 导入照片后的 AI 人像工作区 |
| `03-analyze.jpg` | Apple Vision 分析结果与 Provider 卡片 |
| `04-level3-gate.jpg` | LEVEL 3 四区门控与精度证据 |
| `05-provider-switch.jpg` | 修复后切换 Provider 的锁定状态 |
| `05-provider-switch-final.jpg` | 最终复验：LEVEL 3 锁定、探针清空、状态胶囊为「未分析」 |
| `05-provider-switch-stale.jpg` | 修复前残留旧数据的证据（保留） |
| `06-model-hub.jpg` | 模型库工作区 |
| `07-reopen.jpg` | 项目重开后的参数 |
| `07b-reopen-ai.jpg` | 重开后的 AI 人像工作区 |
| `08-export.jpg` | 导出文件与 Finder 位置 |
| `09-dmg.jpg` | DMG 挂载内容 |

## 5. 结论

- 打包应用的核心路径可用：启动、导入、分析、分项门控、模型库、保存/重开、导出、DMG。
- 验收中发现并修复了 1 个真实缺陷：Provider 切换后旧分析记录未作废。
- 用户 ONNX 真实推理与定位精度仍未验证，不作为本轮 PASS 项。
