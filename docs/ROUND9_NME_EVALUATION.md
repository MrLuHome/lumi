# ROUND 9 — Apple Vision NME 精度评测

- 日期：2026-11-21
- 对象：Apple Vision（`VNDetectFaceLandmarksRequest`）
- 数据状态：**BLOCKED** — 没有取得 ≥50 张具备明确授权的人脸数据集
- 精度状态：**AccuracyUnverified**（没有任何真实精度数字）
- 结论：本轮交付可执行的评测工具链与预置门槛，**不报告、不推测任何 NME 数值**

## 1. 为什么是 BLOCKED

任务书要求 ≥50 张有合法使用权的照片，并记录授权与来源。当前仓库内
`docs/assets/` 只有应用界面截图与一张项目自有素材，不足以构成评测集，
也没有被摄者授权记录。为了避免伪造指标，本轮不生成数据集、不填写 NME。

未执行项（不会写成通过）：

- 50 张以上多姿态/遮挡/光照/多人脸样本采集与授权；
- 双人独立标注与一致性复核；
- Apple Vision 语义映射在 dev 集上的冻结验证；
- 任何 Overall / 分区 NME、P50 / P95、Failure Rate 数值。

## 2. 本轮交付

| 产物 | 路径 | 作用 |
| --- | --- | --- |
| 标注规范 | `docs/ROUND9_LANDMARK_ANNOTATION_GUIDE.md` | 8 区域语义 ID、字段、划分、防泄漏、映射规则 |
| 预测导出 | `src-tauri/src/face_intelligence/eval_export.rs` | `lumi-retouch --nme-predict <dataset.json> <out.json>` |
| 评测脚本 | `scripts/nme_evaluate.py` | 授权校验 → 语义映射 → NME / P50 / P95 / 失败率 / 分组 |
| 预置门槛 | 同脚本 `THRESHOLDS` | 在上述结果产生之前固定 |

### 评测流程（数据到位后可直接执行）

```bash
# 1) 导出预测（Apple Vision，带语义 ID）
./src-tauri/target/release/lumi-retouch \
  --nme-predict data/nme/dataset.json data/nme/predictions.json

# 2) 评测（mapping 为在 dev 集冻结的语义映射）
python3 scripts/nme_evaluate.py \
  --dataset data/nme/dataset.json \
  --predictions data/nme/predictions.json \
  --mapping data/nme/apple-vision-map.json \
  --output reports/nme
```

输出：`report.json`（总体、分区、姿态/遮挡/光照分组、失败明细）与 `points.csv`（逐点）。

### 度量定义

- 点误差：预测点与标注点的欧氏距离（归一化坐标）；
- 归一化分母：标注的双眼中心距离，缺失时退化为人脸框对角线（报告内记录计数）；
- 只统计 `visible=true` 且能按 `semanticId` 配对的点；
- 点失败率：GT 可见点中，Provider 无对应 `semanticId` 或无法归一化的比例；
- 样本失败：解码失败、无人脸、Provider 报错，单独计数，不混入 NME。

### 预置门槛（2026-11-21，看到任何结果之前固定）

| 指标 | 门槛 |
| --- | --- |
| Overall NME | ≤ 0.080 |
| Eye NME | ≤ 0.050 |
| Eyebrow NME | ≤ 0.080 |
| Nose NME | ≤ 0.080 |
| Mouth NME | ≤ 0.080 |
| Contour NME | ≤ 0.100 |
| P95 | ≤ 0.150 |
| 点失败率 | ≤ 5% |
| 语义映射 | 必须 `verified` |

未达标时脚本输出 `AccuracyInsufficient`，不得下调门槛或更换分母。

## 3. 工具链验证（不是精度证据）

### 3.1 度量实现自检

```
$ python3 scripts/nme_evaluate.py --self-test
PASS 相同标注与预测 → NME=0.000000
PASS 已知偏移 0.01（分母 0.1500）→ NME=0.066667
PASS 缺失语义点计为失败而不是静默跳过
PASS 语义 ID 错位 → 计为失败，未按下标配对
PASS 不可见点被排除（既不算正确也不算失败）
PASS 缺授权/同意字段 → BLOCKED
```

说明：合成数据验证的是**计算与配对逻辑**，不涉及任何真实照片。

### 3.2 预测导出链路冒烟

在项目自有素材（`docs/assets/round8_1/portrait-lumi.jpg`，非评测集）上运行导出：

```
provider: apple-vision VNDetectFaceLandmarksRequest
smoke-1 ok=True faces=1 1680x2240
points: 85
regions: face.contour 17, eye.left.lid 6, eye.right.lid 6,
         eyebrow.left 6, eyebrow.right 6, nose.contour 8, nose.crest 6,
         mouth.outer 14, mouth.inner 6, face.median 10
```

说明：这证明「解码 → Apple Vision → 语义 ID → 归一化坐标 → JSON」链路可用，
**不构成任何精度结论**，该素材也没有 Ground Truth。

## 4. 已知限制

1. Apple Vision 各区域点数与 iBUG-68 不同（例如外唇 14 点、内唇 6 点），
   必须依赖 §5 映射，不能直接与 68 点标注按下标比较；
2. Vision 的点顺序未由 Apple 公开文档逐点规定，映射必须在 dev 集上实测冻结；
3. 不同 macOS 版本的 Vision revision 可能改变点数与顺序，映射需随系统版本复核；
4. 本轮未验证侧脸/遮挡下的点是否存在系统性偏移——没有数据就不下结论。

## 5. 解除 BLOCKED 所需

- 取得 ≥50 张授权样本，覆盖正面、左右侧脸、轻微/明显俯仰、闭眼眯眼、眼镜、
  头发遮挡、口罩、不同光照、不同肤色、多人脸、不同分辨率；
- 按 `ROUND9_LANDMARK_ANNOTATION_GUIDE.md` 完成双人标注与复核；
- 在 dev 集冻结 Apple Vision 映射；
- 重新运行脚本并把 `report.json` 结果写入本文件，替换 BLOCKED。
