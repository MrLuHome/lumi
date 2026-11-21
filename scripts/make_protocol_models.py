#!/usr/bin/env python3
"""生成 ROUND 9 的协议测试模型（ProtocolTestOnly）。

这些模型不是人脸关键点模型，只用于验证 LUMI 的 ONNX Worker 协议：
导入 → manifest → 输入张量 → 推理 → 输出长度/数值/坐标校验。

模型由本项目自行生成，许可：CC0-1.0（公共领域贡献）。
运行：python3 scripts/make_protocol_models.py
"""

from pathlib import Path

import onnx
from onnx import TensorProto, helper

ROOT = Path(__file__).resolve().parent.parent / "src-tauri" / "tests" / "fixtures" / "onnx"
POINTS = 68


def landmark_values(mutate=None):
    values = []
    for index in range(POINTS):
        x = 0.08 + 0.84 * ((index % 10) / 9.0)
        y = 0.18 + 0.64 * ((index // 10) / 6.0)
        confidence = 0.9
        if mutate:
            x, y, confidence = mutate(index, x, y, confidence)
        values.extend([x, y, confidence])
    return values


def build_model(values, op_type="Constant", domain=""):
    image = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 3, 64, 64])
    output = helper.make_tensor_value_info("landmarks", TensorProto.FLOAT, [1, POINTS, 3])
    if op_type == "Constant":
        tensor = helper.make_tensor("points", TensorProto.FLOAT, [1, POINTS, 3], values)
        node = helper.make_node("Constant", [], ["landmarks"], value=tensor)
    else:
        # 故意使用不存在的算子，验证结构/算子检查会拒绝。
        node = helper.make_node(op_type, ["input"], ["landmarks"], domain=domain)
    graph = helper.make_graph([node], "lumi_protocol68", [image], [output])
    model = helper.make_model(
        graph,
        producer_name="lumi-round9-protocol-fixture",
        opset_imports=[helper.make_opsetid("", 13)],
    )
    model.ir_version = 8
    return model


def write(name, model, check=True):
    ROOT.mkdir(parents=True, exist_ok=True)
    path = ROOT / name
    if check:
        onnx.checker.check_model(model)
    onnx.save(model, path)
    print(f"{path}  {path.stat().st_size} bytes")


def main():
    write("protocol68.onnx", build_model(landmark_values()))
    write(
        "protocol68_nan.onnx",
        build_model(landmark_values(lambda i, x, y, c: (x, y, float("nan")) if i == 3 else (x, y, c))),
    )
    write(
        "protocol68_out_of_range.onnx",
        build_model(landmark_values(lambda i, x, y, c: (5.0, y, c) if i == 0 else (x, y, c))),
    )
    write(
        "protocol68_bad_op.onnx",
        build_model(landmark_values(), op_type="LumiUnknownOp"),
        check=False,
    )


if __name__ == "__main__":
    main()
