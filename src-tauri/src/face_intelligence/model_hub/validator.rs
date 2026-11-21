//! ONNX 结构与能力校验。
//!
//! 只接受满足明确契约的模型：
//! - 单输入 `[1,3,H,W]` float32，H/W 静态；
//! - 单输出 3 维、单批次、N 个点（N∈5..2000）、2/3 通道；
//! - 所有算子能被当前 tract 后端编译；
//! - 无 external_data，文件体积受限。
//!
//! 不满足契约时返回具体原因，不尝试“猜一个后处理”。

use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

use super::manifest::OutputLayout;
use super::security::{read_limited, reject_external_data, sha256_bytes, MAX_MODEL_BYTES};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnnxProbe {
    pub ok: bool,
    pub file_size: u64,
    pub sha256: String,
    pub ir_version: Option<i64>,
    pub opset: Option<i64>,
    pub input_shape: Vec<Option<i64>>,
    pub output_shape: Vec<Option<i64>>,
    pub input_width: Option<u32>,
    pub input_height: Option<u32>,
    pub output_layout: Option<OutputLayout>,
    pub point_count: Option<usize>,
    pub operator_count: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub elapsed_ms: u64,
}

impl OnnxProbe {
    fn failed(sha256: String, file_size: u64, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            file_size,
            sha256,
            ir_version: None,
            opset: None,
            input_shape: Vec::new(),
            output_shape: Vec::new(),
            input_width: None,
            input_height: None,
            output_layout: None,
            point_count: None,
            operator_count: 0,
            warnings: Vec::new(),
            errors: vec![error.into()],
            elapsed_ms: 0,
        }
    }
}

fn shape_dims(fact: &TypedFact) -> Vec<Option<i64>> {
    match fact.shape.as_concrete() {
        Some(dims) => dims.iter().map(|dim| Some(*dim as i64)).collect(),
        None => vec![None; fact.shape.rank()],
    }
}

fn datum_is_f32(fact: &TypedFact) -> bool {
    fact.datum_type == DatumType::F32
}

/// 同步校验。Tauri 命令层会把它放进独立子进程并加硬超时。
pub fn validate_onnx(path: &Path) -> OnnxProbe {
    let started = Instant::now();
    let bytes = match read_limited(path, MAX_MODEL_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => return OnnxProbe::failed(String::new(), 0, error),
    };
    let file_size = bytes.len() as u64;
    let sha256 = sha256_bytes(&bytes);
    if let Err(error) = reject_external_data(&bytes) {
        return OnnxProbe::failed(sha256, file_size, error);
    }
    drop(bytes);

    let proto = match tract_onnx::onnx().proto_model_for_path(path) {
        Ok(proto) => proto,
        Err(error) => {
            return OnnxProbe::failed(
                sha256,
                file_size,
                format!("ONNX 解析失败（文件可能损坏或不是 ONNX）：{error}"),
            )
        }
    };
    let ir_version = Some(proto.ir_version);
    let opset = proto.opset_import.iter().map(|entry| entry.version).max();

    let model = match tract_onnx::onnx().model_for_path(path) {
        Ok(model) => model,
        Err(error) => {
            return OnnxProbe::failed(sha256, file_size, format!("ONNX 图构建失败：{error}"))
        }
    };
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if model.inputs.len() != 1 {
        errors.push(format!("需要 1 个输入，实际 {}", model.inputs.len()));
    }
    if model.outputs.len() != 1 {
        errors.push(format!("需要 1 个输出，实际 {}", model.outputs.len()));
    }
    // 只有形状完全静态且算子可推断时才允许导入，避免动态维度在推理时炸开。
    let typed = match model.clone().into_typed() {
        Ok(typed) => Some(typed),
        Err(error) => {
            errors.push(format!("输入/输出形状必须是静态且可推断的：{error}"));
            None
        }
    };
    let input_fact = typed.as_ref().and_then(|typed| typed.input_fact(0).ok());
    let output_fact = typed.as_ref().and_then(|typed| typed.output_fact(0).ok());
    let input_shape = input_fact.map(shape_dims).unwrap_or_default();
    let output_shape = output_fact.map(shape_dims).unwrap_or_default();

    let mut input_width = None;
    let mut input_height = None;
    if input_shape.len() != 4 {
        errors.push(format!("输入必须是 4 维 NCHW，实际 {:?}", input_shape));
    } else {
        if input_shape[0] != Some(1) {
            errors.push("输入批次必须固定为 1".into());
        }
        if input_shape[1] != Some(3) {
            errors.push(format!(
                "输入通道必须为 3（RGB），实际 {:?}",
                input_shape[1]
            ));
        }
        match (input_shape[2], input_shape[3]) {
            (Some(height), Some(width))
                if (32..=1024).contains(&height) && (32..=1024).contains(&width) =>
            {
                input_height = Some(height as u32);
                input_width = Some(width as u32);
            }
            _ => errors.push("输入高宽必须是 32..1024 的静态尺寸".into()),
        }
    }
    if let Some(fact) = input_fact {
        if !datum_is_f32(fact) {
            errors.push("输入数据类型必须是 float32".into());
        }
    }

    let mut output_layout = None;
    let mut point_count = None;
    match OutputLayout::infer(&output_shape) {
        Ok(layout) => {
            output_layout = Some(layout);
            point_count = layout.point_count(&output_shape);
            if let Some(count) = point_count {
                if count != 68 {
                    errors.push(format!(
                        "本轮只支持 face-68 拓扑（68 点），模型输出 {count} 点"
                    ));
                }
            }
        }
        Err(error) => errors.push(error),
    }
    if let Some(fact) = output_fact {
        if !datum_is_f32(fact) {
            errors.push("输出数据类型必须是 float32".into());
        }
    }

    let mut operator_count = 0usize;
    if errors.is_empty() {
        match model.into_optimized().and_then(|optimized| {
            operator_count = optimized.nodes().len();
            optimized.into_runnable()
        }) {
            Ok(_) => {}
            Err(error) => errors.push(format!(
                "算子不受支持或图无法编译（可能使用了自定义算子）：{error}"
            )),
        }
    } else {
        // 结构已经不合格时仍尝试优化，以便补充算子层面的诊断。
        match model.into_optimized() {
            Ok(optimized) => {
                operator_count = optimized.nodes().len();
                if let Err(error) = optimized.into_runnable() {
                    errors.push(format!("算子不受支持：{error}"));
                }
            }
            Err(error) => errors.push(format!("图优化失败：{error}")),
        }
    }
    if operator_count == 0 {
        warnings.push("优化后算子数为 0，请确认模型是否只含常量".into());
    }

    OnnxProbe {
        ok: errors.is_empty(),
        file_size,
        sha256,
        ir_version,
        opset,
        input_shape,
        output_shape,
        input_width,
        input_height,
        output_layout,
        point_count,
        operator_count,
        warnings,
        errors,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

/// 把同步探针结果转成存储用的摘要。
pub fn summary(probe: &OnnxProbe) -> super::manifest::ValidationSummary {
    super::manifest::ValidationSummary {
        ok: probe.ok,
        sha256: probe.sha256.clone(),
        file_size: probe.file_size,
        input_shape: probe.input_shape.clone(),
        output_shape: probe.output_shape.clone(),
        operator_count: probe.operator_count,
        elapsed_ms: probe.elapsed_ms,
        warnings: probe.warnings.clone(),
        errors: probe.errors.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_error_not_panic() {
        let probe = validate_onnx(Path::new("/tmp/definitely-missing-lumi.onnx"));
        assert!(!probe.ok);
        assert!(!probe.errors.is_empty());
    }

    #[test]
    fn text_file_is_rejected_as_invalid_onnx() {
        let path = std::env::temp_dir().join(format!("lumi-not-onnx-{}.onnx", std::process::id()));
        std::fs::write(&path, b"this is not protobuf").unwrap();
        let probe = validate_onnx(&path);
        assert!(!probe.ok);
        std::fs::remove_file(&path).ok();
    }
}
