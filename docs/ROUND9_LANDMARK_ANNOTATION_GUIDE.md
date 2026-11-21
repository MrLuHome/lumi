# ROUND 9 — 人脸关键点标注规范

- 版本：v1（2026-11-21）
- 适用：Apple Vision NME 评测的 Ground Truth，以及任何后续 Provider 的精度比较
- 配套脚本：[nme_evaluate.py](../../scripts/nme_evaluate.py)

标注与评测的**唯一配对键是 `semanticId`**。任何按数组下标的比较都视为无效。

## 1. 数据来源与授权

每个样本必须在数据集清单里记录：

| 字段 | 说明 |
| --- | --- |
| `id` | 样本唯一 ID（与照片文件名解耦） |
| `image` | 照片路径（相对清单，或绝对路径） |
| `license` | 授权方式，例如「本人书面授权」「项目自有拍摄」「模特合同 #123」 |
| `consent` | 被摄者同意范围，例如 `internal-evaluation`、`commercial-marketing` |
| `sourceUrl` | 若来自第三方，必须给出原始页面与下载日期；**禁止**来源不明的网络人像 |
| `subjectHash` | 同一被摄者的不可逆标识（用于防泄漏，不用姓名） |
| `split` | `dev` / `val` / `test` |
| `pose` | `frontal` / `left30` / `right30` / `left60` / `right60` / `pitchUp` / `pitchDown` |
| `occlusion` | `none` / `glasses` / `hair` / `mask` / `hand` / `partial` |
| `lighting` | `daylight` / `studio` / `lowlight` / `backlit` / `harsh` |
| `faceCount` | 画面中需要标注的人脸数 |
| `annotations` | 标注 JSON 路径 |

脚本会校验 `license` 与 `consent`，任一为空即整体 `BLOCKED`，不产生指标。

### 划分与防泄漏

- 开发集（`dev`）：调映射、调参数；
- 验证集（`val`）：内部检查；
- 独立测试集（`test`）：只用于最终报告。
- **同一 `subjectHash` 的近似照片不得跨集**；脚本按清单人工核对，发现跨集即该批数据作废。

## 2. 区域与语义 ID

8 个评分区域，编号顺序固定如下（左右均按**被摄者**的左右，图像左侧为右眼）：

| 区域 | 语义前缀 | 规范点数 | 说明 |
| --- | --- | --- | --- |
| Face Contour | `face.contour` | 17 | 从右侧下颌角沿下颌到左侧下颌角 |
| Left Eye | `eye.left.lid` | 6 | 外眼角 → 上眼睑 2 点 → 内眼角 → 下眼睑 2 点 |
| Right Eye | `eye.right.lid` | 6 | 同上，镜像 |
| Left Eyebrow | `eyebrow.left` | 6 | 眉头 → 眉峰 → 眉尾 |
| Right Eyebrow | `eyebrow.right` | 6 | 同上，镜像 |
| Nose | `nose.contour` | 8 | 鼻根 → 两侧鼻翼 → 鼻底 |
| Outer Lip | `mouth.outer` | 14 | 上唇 7 点 + 下唇 7 点，顺时针 |
| Inner Lip | `mouth.inner` | 6 | 内唇闭合轮廓，顺时针 |

辅助区域（可用于归一化或质量检查，但不计入 8 区域 NME）：
`nose.crest`（鼻梁中线）、`face.median`（面部中线）。

语义 ID 形如 `eye.left.lid.0`。**允许 Provider 的点数与本表不同**，此时必须通过
§5 的映射文件把 Provider ID 翻译成上表 ID；无法翻译的点按缺失处理，不按位置猜测。

## 3. 单点字段

```json
{
  "semanticId": "eye.left.lid.2",
  "x": 0.4123,
  "y": 0.3871,
  "visible": true,
  "occlusion": "none",
  "confidence": 0.98,
  "region": "eye",
  "annotator": "A1"
}
```

| 字段 | 规则 |
| --- | --- |
| `x` / `y` | 原图左上角原点，按图像宽高归一化到 0..1（与 Provider 输出同一坐标系） |
| `visible` | 该点在照片上是否可判断位置。不可判断必须 `false` |
| `occlusion` | `none` / `partial` / `full` / `unknown` |
| `confidence` | 标注者对该点位置的主观把握，0..1 |
| `annotator` | 标注者代号，用于一致性统计 |

**不可判断的点不得写坐标**。`visible=false` 的点不参与 NME，也不计入失败率——
“无法判断”既不算正确也不算错误。

## 4. 标注流程

1. 以 100% 比例逐点标注，先轮廓后五官；每个区域按上表顺序，不得跳点；
2. 越过鼻子中线、发际线遮挡、镜框遮挡时，先判断可见性，再决定是否落点；
3. 标注完成后自检：区域点数为规范点数、左右未互换、坐标均在 0..1；
4. 至少 20% 的样本由第二名标注者独立完成；
5. 两名标注者对同一批点计算归一化距离，**> 0.05（相对双眼距离）** 的点进入复核，
   由第三人裁决后写入最终 Ground Truth，并保留两份原始标注。

## 5. Provider 语义映射

不同 Provider 的区域点数与编号顺序不同，必须提供映射文件：

```json
{
  "schemaVersion": 1,
  "provider": "apple-vision",
  "providerVersion": "VNDetectorRevision2",
  "verifiedOn": "2026-XX-XX / dev-set N=15 / 方法：几何顺序检验 + 双人复核",
  "map": {
    "eye.left.lid.0": "eye.left.lid.0",
    "nose.contour.5": "nose.contour.4"
  }
}
```

- 映射必须在 **dev 集**上验证：检查每个区域点的几何顺序单调性、左右不互换、
  与标注的均值误差合理；
- 验证完成后冻结，并记录 macOS / Vision revision；
- 没有 `verifiedOn` 的映射文件视为未验证，脚本直接 `BLOCKED`；
- 未提供映射文件时，脚本按 `unverified` 记录，并在门槛判定中拒绝验收。

## 6. 一致性检查（交付前必做）

- [ ] 同一 `subjectHash` 未跨集
- [ ] 每张照片的 `faceCount` 与标注人脸数一致
- [ ] 每个区域的点数等于规范点数
- [ ] 所有坐标在 0..1 且为有限值
- [ ] 双人标注覆盖率 ≥ 20%，复核记录齐全
- [ ] 映射文件已冻结并记录验证方法
- [ ] 数据集清单中的授权字段全部非空
