# ROUND 8 — GUI 验收

- 日期：2026-11-21
- 对象：真实打包应用 `src-tauri/target/release/bundle/macos/LUMI RETOUCH.app`（15.85 MiB，dmg 7.70 MiB，14:08 构建）
- 样张：`src-tauri/tests/fixtures/images/portrait.jpg`
- 截图：`docs/assets/round8/`
- 环境：macOS（Apple Silicon M4），无签名分发，直接打开本地 bundle

## 1. 验收结果

| 项 | 结果 | 证据 |
|---|---|---|
| 应用可启动 | PASS | 实际打开并操作 |
| 导入真实照片 | PASS | 1350×1800 预览 |
| Provider 列表与“当前使用”一致 | PASS（修复后） | `ai-providers.png` |
| 切换到 Apple Vision | PASS | 顶部与卡片同步变为 `apple-vision` |
| 质量验证输出真实数字 | PASS（修复后） | `ai-probe.png` |
| 分析 85 点并逐脸门控 | PASS | `ai-analysis.png` |
| LEVEL 3 在面部塑形解锁 | PASS（修复后） | `reshape-level3.png` |
| 模型库导入页可用 | PASS | `model-hub.png` |
| LEVEL 1/2 仍可用 | PASS | 塑形面板滑块正常 |

## 2. 逐屏记录

### 2.1 Provider 列表（`ai-providers.png`）

- 切换前顶部：“Provider 能力解耦 · 当前 YuNet 2023mar（随包） · yunet-5”。
- YuNet 卡片：“当前使用”；Apple Vision 卡片：可点击的“切换”。
- 点击切换后：顶部变为“当前 Apple Vision（系统） · apple-vision-landmarks”；
  Apple Vision 显示“当前使用/就绪”，YuNet 变为“切换/空闲”；
  提示“Provider 已切换。LEVEL 3 参数按新 Provider 的实测门控重新判定，请确认五官参数。”
- 修复内容：此前 UI 把 `enabled`（可用）误当作“当前使用”，导致显示 Apple Vision
  而实际分析走 YuNet；现改为按 `status.activeProvider` 判定。

### 2.2 质量验证（`ai-probe.png`）

- 稳定性：“2 次 · 160 / 27 ms”；
- 最大偏差：“0.00000 px”；
- 眼距占比：“0.000%”；
- 结论：“通过”；
- 说明：“2 次重复，最大偏差 0.000% 眼距（阈值 2.00%）”；
- “稠密关键点与五官能力已升级为「已验证」”；
- LEVEL 3 行：“LEVEL 3 已解锁：眼部、鼻部、唇部、面部轮廓；未解锁：无”。
- 首轮 160 ms 含框架初始化，第二轮 27 ms，与基准文档的口径一致。

### 2.3 分析（`ai-analysis.png`）

- 最近分析：“Apple Vision（系统） · 50 ms · 85 点”；
- “1 张人脸 · 图像 1350×1800 · 拓扑 apple-vision-landmarks”；
- 人脸表：置信度 85（UI 按百分比展示为 100%）、稠密点 85，眼/鼻/唇/下巴均 ✓；
- 能力 chips：稠密关键点/眼睑/鼻部/唇部为 ✓，“分割”仍为 ×（不支持）。

### 2.4 面部塑形 LEVEL 3（`reshape-level3.png`）

- 标题：“精确五官（LEVEL 3 · 已解锁）”；
- 说明：“当前人脸使用 apple-vision 的稠密轮廓（apple-vision-landmarks）。参数只作用于实测通过的部位。”
- 四组滑块全部可调：眼部（整体大小/眼形宽度/眼形高度/眼距/眼角）、
  鼻部（鼻梁宽度/鼻翼宽度/鼻头大小/鼻部平衡）、
  唇部（唇宽/上唇厚度/下唇厚度/唇角/唇部大小）、下巴（下巴长度等）。
- 修复内容：此前 5 点 Provider 分析后仍显示“请先运行 AI 人像分析”，
  误导用户重复分析；现在稀疏 Provider 的门控原因明确写明
  “只输出 5 个稀疏关键点，不提供眼睑/鼻翼/唇轮廓”。

### 2.5 模型库（`model-hub.png`）

- 标题：“用户自备 ONNX · 本机校验 · 不随安装包分发”；
- 安全说明：“不打包任何第三方权重。导入的模型仅保存在本机应用数据目录，
  许可证由用户自行确认；LUMI 不把它包装成商业可用。”
- 表单：ONNX 文件、名称、归一化 0..1、输出坐标“裁剪内 0..1”、裁剪边距 0.15、
  许可证（用户声明）UNKNOWN、许可证链接、“校验并导入”；
- 导入要求：“单文件 .onnx、≤512 MB、静态输入 32..1024、输出 68 点、无 external_data；
  校验在独立子进程中运行，失败会给出具体原因。”
- 已安装（0）：未导入任何真实第三方模型，因此没有展示 SHA256 卡片。

## 3. 未覆盖

- 未导入真实第三方 68 点 ONNX（无合法样本），模型卡片的 SHA/形状展示未在真实模型上验收。
- 未测试水平翻转、旋转后重新分析、项目保存/重开后的 LEVEL 3 恢复的 GUI 表现
  （持久化由自动化测试覆盖，非 GUI）。
- 未在 Intel Mac / Windows 上验收。
