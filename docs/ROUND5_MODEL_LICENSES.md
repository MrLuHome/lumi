# 第五轮模型授权结论

日期：2026-11-20
分支：`main`
基线：`ba183f7`

本文件只记录「本轮是否引入模型权重」的结论与依据。第五轮的产品路线是「手动精修优先、本地算法驱动、AI 能力可插拔」，因此模型授权不是主线交付的前置条件。

## 1. 结论

| 候选 | 用途 | 本轮状态 | 原因 |
| --- | --- | --- | --- |
| BiRefNet 系列 | 人像/通用分割 | 不引入 | 权重授权与训练数据条款未完整核验，无法确认商业使用与再分发权 |
| SAM 2 | 可提示分割 | 不引入 | 权重许可、依赖体量与桌面分发成本未验证 |
| MediaPipe Pose | 人体姿态关键点 | 不引入 | 桌面端分发方案未确定，`.task` 格式与 tract 不兼容 |
| MediaPipe Face/Subject `.task` | 分割/姿态 | 不引入 | tract 不支持 `.task`（TFLite + bundle） |
| RVM | 视频/人像抠图 | 不引入 | GPL-3.0，与闭源分发不兼容 |
| 其他第三方人像分割权重 | 蒙版 | 不下载、不打包、不启用 | 未确认商业授权 |

以上候选不进入正式版本，本轮不重复调研；如出现新的官方授权文件、明确商业合同或实质性新技术证据，再单独评审。

## 2. 本轮实际使用的模型资产

| 名称 | 用途 | 许可证 | 状态 |
| --- | --- | --- | --- |
| YuNet 2023mar ONNX | 人脸检测（第四轮已存在） | MIT | 继续使用，SHA256 与路径见 `THIRD_PARTY_LICENSES.md` |

第五轮新增的人物蒙版、身材变形、背景处理全部是本地确定性算法，不新增任何模型权重。

## 3. 能力接口与禁用原因

- `ai_engine/segmentation`：`SegmentationCapability.ai_available = false`，`AiMaskProvider` 的 `segment()` 返回 `AppError::Unsupported`，原因固定为「未找到经过商用授权与再分发许可校验的人物分割权重」。
- `ai_engine/body/pose`：`PoseCapability.pose_available = false`，`DisabledPoseProvider` 明确说明姿态模型未启用；身材调整使用手动蒙版轮廓 + 用户锚点。
- 前端 `SubjectMaskPanel` / `BodyPanel` 直接展示禁用原因，不把空实现包装成 AI 功能。

## 4. 合规边界

- 本地推理，不上传照片；背景图片只读取本地路径。
- 原始照片只读，所有编辑以非破坏性参数保存。
- 预览与导出共用 `pipeline::render_with_report`，不存在两套算法。
- 未实现的功能不标记完成；AI 能力保持「接口存在、默认禁用、原因可见」。
