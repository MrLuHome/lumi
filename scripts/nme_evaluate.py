#!/usr/bin/env python3
"""LUMI 人脸关键点 NME 评测（可重复执行）。

输入：
  dataset.json       样本清单（含授权字段、分组标签、标注文件路径）
  predictions.json   由 `lumi-retouch --nme-predict dataset.json out.json` 导出
输出：
  <out>/report.json  总体 / 分区 / 分组指标与失败明细
  <out>/points.csv   逐点明细

设计原则：
  - 只按 **semanticId** 配对（例如 `eye.left.lid.2`），绝不按数组下标；
  - 分母（归一化基准）在运行前固定：优先用标注的双眼中心距离，缺失时退化为人脸框对角线；
  - 不可见点不计入 NME，也不计入失败率（无法判断的点不能算“正确”）；
  - 样本缺少授权/同意字段 → 整体 BLOCKED，不产出指标；
  - `--self-test` 用合成数据验证度量实现本身正确，不代表任何真实精度。

用法：
  python3 scripts/nme_evaluate.py --dataset data/dataset.json \
      --predictions data/predictions.json --output reports/nme
  python3 scripts/nme_evaluate.py --self-test
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import sys

# 预置门槛：在看到任何真实结果之前固定，不得事后下调。
THRESHOLDS = {
    "overallNme": 0.080,
    "eyeNme": 0.050,
    "eyebrowNme": 0.080,
    "noseNme": 0.080,
    "mouthNme": 0.080,
    "contourNme": 0.100,
    "p95": 0.150,
    "pointFailureRate": 0.050,
}

# 8 个评分区域：语义前缀 → 报告分组名
SCORED_REGIONS = {
    "face.contour": "contour",
    "eye.left.lid": "eye",
    "eye.right.lid": "eye",
    "eyebrow.left": "eyebrow",
    "eyebrow.right": "eyebrow",
    "nose.contour": "nose",
    "mouth.outer": "mouth",
    "mouth.inner": "mouth",
}
# 只用于归一化的区域，不参与误差统计
AUX_REGIONS = {"nose.crest", "face.median", "eye.iris"}


def region_of(semantic_id: str) -> str | None:
    for prefix in sorted(SCORED_REGIONS, key=len, reverse=True):
        if semantic_id == prefix or semantic_id.startswith(prefix + "."):
            return SCORED_REGIONS[prefix]
    return None


def load_json(path: str):
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def percentile(values: list[float], q: float) -> float:
    if not values:
        return float("nan")
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * q
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def eye_center(points: dict[str, dict], prefix: str) -> tuple[float, float] | None:
    xs = [p["x"] for key, p in points.items() if key.startswith(prefix + ".")]
    ys = [p["y"] for key, p in points.items() if key.startswith(prefix + ".")]
    if not xs:
        return None
    return sum(xs) / len(xs), sum(ys) / len(ys)


def normalization(points: dict[str, dict], bbox: dict | None) -> tuple[float, str]:
    """归一化分母：双眼中心欧氏距离；缺失时退化为人脸框对角线。"""
    left = eye_center(points, "eye.left.lid")
    right = eye_center(points, "eye.right.lid")
    if left and right:
        distance = math.dist(left, right)
        if distance > 1e-6:
            return distance, "interocular"
    if bbox:
        diagonal = math.hypot(bbox.get("width", 0.0), bbox.get("height", 0.0))
        if diagonal > 1e-6:
            return diagonal, "bboxDiagonal"
    return float("nan"), "missing"


def authorization_issues(dataset: dict) -> list[str]:
    issues = []
    for sample in dataset.get("samples", []):
        sid = sample.get("id", "<无 id>")
        for field in ("license", "consent"):
            if not str(sample.get(field, "")).strip():
                issues.append(f"{sid}: 缺少 {field}")
        if not sample.get("annotations"):
            issues.append(f"{sid}: 缺少 annotations 路径")
    return issues


def collect_predictions(predictions: dict) -> dict[str, dict]:
    return {sample["id"]: sample for sample in predictions.get("samples", [])}


def apply_mapping(predictions: dict, mapping: dict | None) -> dict:
    """把 Provider 的 semanticId 翻译成标注使用的规范 ID。

    没有映射文件时不猜测：原样使用并在报告里标记 `unverified`，
    由门槛判定为不可验收（不同拓扑的编号不能直接当同一语义）。
    """
    if not mapping:
        return predictions
    table = mapping.get("map", {})
    for sample in predictions.get("samples", []):
        for face in sample.get("faces", []):
            points = []
            for point in face.get("points", []):
                canonical = table.get(point["semanticId"])
                if canonical is None:
                    continue
                points.append({**point, "semanticId": canonical})
            face["points"] = points
    return predictions


def match_faces(gt_faces: list[dict], pred_faces: list[dict]) -> list[tuple[dict, dict | None]]:
    """按人脸框中心距离做一对一匹配；不做跨人脸的点级配对。"""
    remaining = list(pred_faces)
    pairs = []
    for face in gt_faces:
        bbox = face.get("boundingBox")
        if bbox and remaining:
            center = (bbox.get("x", 0) + bbox.get("width", 0) / 2,
                      bbox.get("y", 0) + bbox.get("height", 0) / 2)

            def distance(item):
                b = item.get("boundingBox", {})
                c = (b.get("x", 0) + b.get("width", 0) / 2,
                     b.get("y", 0) + b.get("height", 0) / 2)
                return math.dist(center, c)

            best = min(remaining, key=distance)
            remaining.remove(best)
            pairs.append((face, best))
        else:
            pairs.append((face, None))
    return pairs


def evaluate(dataset: dict, predictions: dict) -> dict:
    pred_by_id = collect_predictions(predictions)
    point_rows = []
    sample_rows = []

    for sample in dataset.get("samples", []):
        sid = sample["id"]
        base = os.path.dirname(os.path.abspath(sample.get("_dataset_path", ".")))
        ann_path = sample.get("annotations")
        if ann_path and not os.path.isabs(ann_path):
            ann_path = os.path.join(base, ann_path)
        annotation = load_json(ann_path) if ann_path else {"faces": []}
        pred = pred_by_id.get(sid)

        gt_faces = annotation.get("faces", [])
        pred_faces = [face for face in (pred or {}).get("faces", [])]

        if pred is None or not pred.get("ok") or not pred_faces:
            reason = "predictionMissing" if pred is None else (pred.get("error") or "noFace")
            sample_rows.append({
                "id": sid, "split": sample.get("split"), "pose": sample.get("pose"),
                "occlusion": sample.get("occlusion"), "lighting": sample.get("lighting"),
                "status": "failed", "reason": reason, "matchedFaces": 0,
            })
            continue

        matched = match_faces(gt_faces, pred_faces)
        for face_index, (gt_face, pred_face) in enumerate(matched):
            gt_points = {p["semanticId"]: p for p in gt_face.get("points", [])}
            pred_points = {p["semanticId"]: p for p in (pred_face or {}).get("points", [])}
            norm, norm_kind = normalization(gt_points, gt_face.get("boundingBox")
                                            or (pred_face or {}).get("boundingBox"))
            for semantic_id, gt in gt_points.items():
                if not gt.get("visible", True):
                    continue
                region = region_of(semantic_id)
                if region is None and semantic_id.split(".")[0:2] and \
                        any(semantic_id.startswith(prefix + ".") for prefix in AUX_REGIONS):
                    continue
                pred_point = pred_points.get(semantic_id)
                if region is None:
                    continue
                if pred_point is None or math.isnan(norm):
                    point_rows.append({
                        "sampleId": sid, "faceIndex": face_index, "semanticId": semantic_id,
                        "region": region, "status": "failed",
                        "reason": "missingPrediction" if pred_point is None else "missingNorm",
                        "nme": "", "norm": norm_kind,
                        "pose": sample.get("pose"), "occlusion": sample.get("occlusion"),
                        "lighting": sample.get("lighting"),
                    })
                    continue
                error = math.dist((pred_point["x"], pred_point["y"]), (gt["x"], gt["y"])) / norm
                point_rows.append({
                    "sampleId": sid, "faceIndex": face_index, "semanticId": semantic_id,
                    "region": region, "status": "scored", "reason": "",
                    "nme": error, "norm": norm_kind,
                    "pose": sample.get("pose"), "occlusion": sample.get("occlusion"),
                    "lighting": sample.get("lighting"),
                })
        sample_rows.append({
            "id": sid, "split": sample.get("split"), "pose": sample.get("pose"),
            "occlusion": sample.get("occlusion"), "lighting": sample.get("lighting"),
            "status": "ok", "reason": "", "matchedFaces": len(matched),
        })

    scored = [row for row in point_rows if row["status"] == "scored"]
    errors = [row["nme"] for row in scored]
    report = {
        "schemaVersion": 1,
        "thresholds": THRESHOLDS,
        "normalization": {
            "rule": "interocular 优先，缺失时 bboxDiagonal",
            "counts": {
                kind: sum(1 for row in point_rows if row["norm"] == kind)
                for kind in ("interocular", "bboxDiagonal", "missing")
            },
        },
        "samples": {
            "total": len(sample_rows),
            "ok": sum(1 for row in sample_rows if row["status"] == "ok"),
            "failed": sum(1 for row in sample_rows if row["status"] == "failed"),
        },
        "points": {
            "scored": len(scored),
            "failed": len(point_rows) - len(scored),
            "pointFailureRate": (len(point_rows) - len(scored)) / len(point_rows)
            if point_rows else float("nan"),
        },
        "overall": {
            "nme": sum(errors) / len(errors) if errors else float("nan"),
            "p50": percentile(errors, 0.50),
            "p95": percentile(errors, 0.95),
        },
        "byRegion": {},
        "groups": {},
        "failures": [row for row in point_rows if row["status"] != "scored"],
        "sampleRows": sample_rows,
    }
    for region in sorted(set(SCORED_REGIONS.values())):
        values = [row["nme"] for row in scored if row["region"] == region]
        report["byRegion"][region] = {
            "points": len(values),
            "nme": sum(values) / len(values) if values else float("nan"),
            "p50": percentile(values, 0.50),
            "p95": percentile(values, 0.95),
        }
    for label in ("pose", "occlusion", "lighting"):
        groups = {}
        for row in scored:
            key = str(row.get(label) or "unknown")
            groups.setdefault(key, []).append(row["nme"])
        report["groups"][label] = {
            key: {"points": len(values), "nme": sum(values) / len(values),
                  "p95": percentile(values, 0.95)}
            for key, values in sorted(groups.items())
        }
    return report, point_rows


def gate(report: dict) -> list[str]:
    """按预置门槛判定；未达标只说明未达标，不修改门槛。"""
    reasons = []
    if report.get("mapping", {}).get("status") != "verified":
        reasons.append("语义映射未验证（缺少 verifiedOn 的映射文件）")
    overall = report["overall"]["nme"]
    if math.isnan(overall) or overall > THRESHOLDS["overallNme"]:
        reasons.append(f"overall NME {overall:.4f} > {THRESHOLDS['overallNme']}")
    p95 = report["overall"]["p95"]
    if math.isnan(p95) or p95 > THRESHOLDS["p95"]:
        reasons.append(f"P95 {p95:.4f} > {THRESHOLDS['p95']}")
    mapping = {
        "eye": "eyeNme", "eyebrow": "eyebrowNme", "nose": "noseNme",
        "mouth": "mouthNme", "contour": "contourNme",
    }
    for region, key in mapping.items():
        value = report["byRegion"].get(region, {}).get("nme", float("nan"))
        if math.isnan(value) or value > THRESHOLDS[key]:
            reasons.append(f"{region} NME {value:.4f} > {THRESHOLDS[key]}")
    rate = report["points"]["pointFailureRate"]
    if math.isnan(rate) or rate > THRESHOLDS["pointFailureRate"]:
        reasons.append(f"point failure rate {rate:.4f} > {THRESHOLDS['pointFailureRate']}")
    return reasons


def write_outputs(report: dict, rows: list[dict], out_dir: str) -> None:
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "report.json"), "w", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)
    fields = ["sampleId", "faceIndex", "semanticId", "region", "status", "reason",
              "nme", "norm", "pose", "occlusion", "lighting"]
    with open(os.path.join(out_dir, "points.csv"), "w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def self_test() -> int:
    """度量实现自检：合成数据，不涉及任何真实照片或精度结论。"""
    ok = True

    def face(points):
        return {"boundingBox": {"x": 0.2, "y": 0.2, "width": 0.4, "height": 0.4},
                "points": points}

    def points_with(dx=0.0, dy=0.0, skip=()):
        offsets = {
            "eye.left.lid": (0.30, 0.40), "eye.right.lid": (0.45, 0.40),
            "eyebrow.left": (0.30, 0.34), "eyebrow.right": (0.45, 0.34),
            "nose.contour": (0.38, 0.48), "face.contour": (0.28, 0.30),
            "mouth.outer": (0.36, 0.58), "mouth.inner": (0.37, 0.59),
        }
        counts = {
            "eye.left.lid": 6, "eye.right.lid": 6, "eyebrow.left": 5,
            "eyebrow.right": 5, "nose.contour": 9, "face.contour": 17,
            "mouth.outer": 12, "mouth.inner": 8,
        }
        result = []
        for prefix, count in counts.items():
            bx, by = offsets[prefix]
            for index in range(count):
                sid = f"{prefix}.{index}"
                if sid in skip:
                    continue
                result.append({"semanticId": sid, "x": bx + index * 0.004 + dx,
                               "y": by + (index % 3) * 0.003 + dy, "visible": True})
        return result

    base = points_with()
    base_map = {p["semanticId"]: p for p in base}
    norm, _ = normalization(base_map, None)

    dataset = {"samples": [{"id": "s1", "license": "test", "consent": "test",
                            "annotations": "in.json", "pose": "frontal"}]}

    def run(gt_points, pred_points):
        annotation = {"faces": [face(gt_points)]}
        prediction = {"samples": [{"id": "s1", "ok": True, "error": None,
                                   "faces": [{"boundingBox": face([])["boundingBox"],
                                              "points": pred_points}]}]}
        original = load_json
        globals()["load_json"] = lambda path: annotation
        try:
            report, rows = evaluate(dataset, prediction)
        finally:
            globals()["load_json"] = original
        return report, rows

    report, _ = run(base, base)
    if abs(report["overall"]["nme"]) > 1e-9:
        print("FAIL 相同标注与预测应得到 NME=0")
        ok = False
    else:
        print("PASS 相同标注与预测 → NME=0.000000")

    shifted = points_with(dx=0.01)
    report, _ = run(base, shifted)
    expected = 0.01 / norm
    actual = report["overall"]["nme"]
    if math.isnan(actual) or abs(actual - expected) > 1e-6:
        print(f"FAIL 已知偏移：期望 {expected:.6f}，实际 {actual:.6f}")
        ok = False
    else:
        print(f"PASS 已知偏移 0.01（分母 {norm:.4f}）→ NME={expected:.6f}")

    partial = [p for p in base if p["semanticId"] != "mouth.inner.0"]
    report, _ = run(base, partial)
    if report["points"]["failed"] != 1:
        print(f"FAIL 缺失点统计：期望 1，实际 {report['points']['failed']}")
        ok = False
    else:
        print("PASS 缺失语义点计为失败而不是静默跳过")

    renamed = [{"semanticId": p["semanticId"].replace("eye.left", "eye.right"),
                "x": p["x"], "y": p["y"]} for p in base]
    report, _ = run(base, renamed)
    if report["points"]["failed"] == 0:
        print("FAIL 语义错位应按 semanticId 判失败")
        ok = False
    else:
        print("PASS 语义 ID 错位 → 计为失败，未按下标配对")

    hidden = points_with(skip=("nose.contour.0",))
    for point in hidden:
        if point["semanticId"] == "nose.contour.0":
            point["visible"] = False
    report, _ = run(hidden, base)
    if report["points"]["failed"] != 0:
        print("FAIL 不可见点不应计入失败率")
        ok = False
    else:
        print("PASS 不可见点被排除（既不算正确也不算失败）")

    bad = {"samples": [{"id": "x", "annotations": "a.json"}]}
    if not authorization_issues(bad):
        print("FAIL 缺授权字段应判 BLOCKED")
        ok = False
    else:
        print("PASS 缺授权/同意字段 → BLOCKED")

    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--dataset")
    parser.add_argument("--predictions")
    parser.add_argument("--mapping", help="语义映射文件（含 provider 与 verifiedOn）")
    parser.add_argument("--output", default="reports/nme")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.dataset or not args.predictions:
        parser.error("需要 --dataset 与 --predictions，或使用 --self-test")

    dataset = load_json(args.dataset)
    dataset["_dataset_path"] = os.path.abspath(args.dataset)
    for sample in dataset.get("samples", []):
        sample["_dataset_path"] = os.path.abspath(args.dataset)
    predictions = load_json(args.predictions)

    issues = authorization_issues(dataset)
    if issues:
        print("RESULT: BLOCKED")
        for issue in issues:
            print(f"  - {issue}")
        return 4

    mapping = load_json(args.mapping) if args.mapping else None
    mapping_status = "unverified"
    if mapping:
        if mapping.get("verifiedOn") and mapping.get("map"):
            mapping_status = "verified"
        else:
            print("RESULT: BLOCKED")
            print("  - 映射文件缺少 verifiedOn 或 map，不能用于验收")
            return 4
        predictions = apply_mapping(predictions, mapping)

    report, rows = evaluate(dataset, predictions)
    report["mapping"] = {
        "status": mapping_status,
        "provider": (mapping or {}).get("provider"),
        "verifiedOn": (mapping or {}).get("verifiedOn"),
        "file": args.mapping,
    }
    write_outputs(report, rows, args.output)
    reasons = gate(report)
    report["accuracyStatus"] = "AccuracyVerified" if not reasons else "AccuracyInsufficient"
    report["gateReasons"] = reasons
    with open(os.path.join(args.output, "report.json"), "w", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)

    print(f"样本 {report['samples']['ok']}/{report['samples']['total']} 成功")
    print(f"Overall NME {report['overall']['nme']:.4f} "
          f"P50 {report['overall']['p50']:.4f} P95 {report['overall']['p95']:.4f}")
    print(f"点失败率 {report['points']['pointFailureRate']:.4f}")
    print(f"精度状态：{report['accuracyStatus']}")
    for reason in reasons:
        print(f"  未达标：{reason}")
    return 0 if not reasons else 5


if __name__ == "__main__":
    sys.exit(main())
