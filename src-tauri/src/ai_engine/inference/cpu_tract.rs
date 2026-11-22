//! CPU 推理后端（唯一真实实现）。
//!
//! 继续使用 `tract-onnx 0.23`：**不替换运行时**。这里只做三件事：
//! 1) 把「解析 / 优化 / 编译」收拢到一个入口，让 plan 可以被缓存复用；
//! 2) 把 tract 的 `TValue` 与统一张量描述互转；
//! 3) 用 `catch_unwind` 把算子级 panic 隔离成 `E_INFER`，而不是让整个进程挂掉。

use std::borrow::Cow;
use std::path::Path;
use std::time::Instant;

use tract_onnx::prelude::*;

use crate::ai_engine::model_registry::spec::ModelSpec;
use crate::image_engine::cancel::CancelToken;

use super::backend::{
    BackendCapabilities, InferenceBackend, LoadedPlan, ModelIntrospection, PlanHandle, RunOptions,
};
use super::error::InferenceError;
use super::tensor::{ChannelOrder, Dtype, TensorBatch, TensorSpec, ValueRange};

/// 只读契约快照：描述一个 ONNX 文件**实际**声明的输入输出形状。
///
/// 符号维（动态维）记为 `0`，在比对时充当通配符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSnapshot {
    pub inputs: Vec<Vec<usize>>,
    pub outputs: Vec<Vec<usize>>,
}

/// 只解析、不优化、不编译地读出模型契约。
///
/// 这是准入判定链第 ④ 步「契约」的实现：成本是纯解析（232 KB 的 YuNet 在毫秒级），
/// 与第 ⑥ 步「解析」的区别是不做常量折叠与算子替换。
pub fn inspect_contract(path: &Path, model_id: &str) -> Result<ContractSnapshot, InferenceError> {
    let backend = CpuTractBackend::new();
    let introspection = backend
        .inspect(path, &CancelToken::default())
        .map_err(|error| attach_model_id(error, model_id))?;
    Ok(ContractSnapshot {
        inputs: introspection.input_shapes(),
        outputs: introspection.output_shapes(),
    })
}

/// `inspect()` 不知道模型 ID（它可以被用在任意文件上），准入链需要一个带 ID 的错误。
fn attach_model_id(error: InferenceError, model_id: &str) -> InferenceError {
    match error {
        InferenceError::ModelLoadFailed { detail, .. } => InferenceError::ModelLoadFailed {
            model_id: model_id.to_string(),
            detail,
        },
        InferenceError::ContractMismatch { detail, .. } => InferenceError::ContractMismatch {
            model_id: model_id.to_string(),
            detail,
        },
        other => other,
    }
}

/// 比对实际契约与登记契约。
pub fn check_contract(
    spec: &ModelSpec,
    snapshot: &ContractSnapshot,
) -> Result<(), InferenceError> {
    if snapshot.inputs.len() != 1 {
        return Err(InferenceError::ModelVersionMismatch {
            model_id: spec.id.to_string(),
            expected: "1 个输入张量".into(),
            actual: format!("{} 个输入张量", snapshot.inputs.len()),
        });
    }
    if !shape_matches(&snapshot.inputs[0], &spec.input.shape) {
        return Err(InferenceError::ModelVersionMismatch {
            model_id: spec.id.to_string(),
            expected: format_shape(&spec.input.shape),
            actual: format_shape(&snapshot.inputs[0]),
        });
    }
    if snapshot.outputs.len() != spec.outputs.len() {
        return Err(InferenceError::ModelVersionMismatch {
            model_id: spec.id.to_string(),
            expected: format!("{} 个输出 head", spec.outputs.len()),
            actual: format!("{} 个输出 head", snapshot.outputs.len()),
        });
    }
    for (index, head) in spec.outputs.iter().enumerate() {
        if !shape_matches(&snapshot.outputs[index], head.shape) {
            return Err(InferenceError::ModelVersionMismatch {
                model_id: spec.id.to_string(),
                expected: format!("{}:{}", head.name, format_shape(head.shape)),
                actual: format!("{}:{}", head.name, format_shape(&snapshot.outputs[index])),
            });
        }
    }
    Ok(())
}

/// `0` 是通配符，其余维度必须逐一相等。
fn shape_matches(actual: &[usize], expected: &[usize]) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .iter()
        .zip(expected.iter())
        .all(|(actual, expected)| *actual == 0 || actual == expected)
}

fn format_shape(shape: &[usize]) -> String {
    if shape.is_empty() {
        "标量".into()
    } else {
        shape
            .iter()
            .map(|dim| {
                if *dim == 0 {
                    "?".to_string()
                } else {
                    dim.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("×")
    }
}

fn parse_typed(path: &Path, model_id: &str) -> Result<TypedModel, InferenceError> {
    tract_onnx::onnx()
        .model_for_path(path)
        .map_err(|error| InferenceError::ModelLoadFailed {
            model_id: model_id.to_string(),
            detail: format!("ONNX 解析失败：{error}"),
        })?
        .into_typed()
        .map_err(|error| InferenceError::ModelLoadFailed {
            model_id: model_id.to_string(),
            detail: format!("ONNX 类型推导失败：{error}"),
        })
}

/// 把 tract 的类型推导结果转成 `ModelIntrospection`（不含 ir_version / opset，
/// 那两项来自 proto 解析，见 `inspect()`）。
fn introspect_typed(typed: &TypedModel, model_id: &str) -> Result<ModelIntrospection, InferenceError> {
    let mut introspection = ModelIntrospection::empty();
    for (index, outlet) in typed.inputs.iter().enumerate() {
        let fact = typed
            .outlet_fact(*outlet)
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: model_id.to_string(),
                detail: format!("读取输入契约失败：{error}"),
            })?;
        push_io(&mut introspection, fact, "input", index, true);
    }
    for (index, outlet) in typed.outputs.iter().enumerate() {
        let fact = typed
            .outlet_fact(*outlet)
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: model_id.to_string(),
                detail: format!("读取输出契约失败：{error}"),
            })?;
        push_io(&mut introspection, fact, "output", index, false);
    }
    Ok(introspection)
}

/// ONNX 不声明"值域 / 通道顺序"（那是训练侧约定），因此 `inspect()` 只能给中性占位，
/// 并把它显式记进 `declarations`，绝不假装知道。
fn push_io(
    introspection: &mut ModelIntrospection,
    fact: &TypedFact,
    role: &str,
    index: usize,
    is_input: bool,
) {
    let (shape, declaration) = describe_shape(fact);
    if fact.datum_type != DatumType::F32 {
        introspection.declarations.push(format!(
            "{role}[{index}] {declaration}：dtype {:?} 不是 F32，TensorSpec 只支持 F32",
            fact.datum_type
        ));
        return;
    }
    introspection.declarations.push(format!(
        "{role}[{index}] {declaration}：值域与通道顺序未由 ONNX 声明，需模型卡补充"
    ));
    let spec = TensorSpec::observed(shape, ValueRange::ZeroOne, ChannelOrder::Rgb);
    if is_input {
        introspection.inputs.push(spec);
    } else {
        introspection.outputs.push(spec);
    }
}

/// 返回 (形状, 人可读描述)。符号维记为 `0` 并在描述里显示 `?`。
fn describe_shape(fact: &TypedFact) -> (Vec<usize>, String) {
    match fact.shape.as_concrete() {
        Some(dims) => (dims.to_vec(), format_shape(dims)),
        None => {
            // 符号维：内部用 0 通配，描述里保留符号信息（`?` / `-1` / 名称）。
            let rank = fact.shape.rank();
            (vec![0; rank], format!("{:?}（含符号维）", fact.shape))
        }
    }
}

/// 唯一的真实推理后端。
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuTractBackend;

impl CpuTractBackend {
    pub fn new() -> Self {
        Self
    }

    /// 从静态登记表构造输出契约（形状来自登记表，实际形状在 `run` 中校验）。
    fn output_specs(spec: &ModelSpec) -> Vec<TensorSpec> {
        spec.outputs
            .iter()
            .map(|head| TensorSpec {
                dtype: Dtype::F32,
                layout: spec.input.layout,
                shape: Cow::Borrowed(head.shape),
                // 输出 head 是概率 / 偏移量，值域不做强制校验，这里只作声明。
                value_range: ValueRange::ZeroOne,
                channel_order: spec.input.channel_order,
            })
            .collect()
    }
}

impl InferenceBackend for CpuTractBackend {
    fn id(&self) -> &'static str {
        "cpu-tract"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            available: true,
            unavailable_reason: None,
            dtypes: &[Dtype::F32],
            max_batch: 1,
            gpu: false,
        }
    }

    fn inspect(
        &self,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<ModelIntrospection, InferenceError> {
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        // 第一段：proto 解析（只读结构，不建图、不编译）—— 与
        // `model_hub/validator.rs:87` 的 `proto_model_for_path` 同一 API。
        let proto = tract_onnx::onnx()
            .proto_model_for_path(path)
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: String::new(),
                detail: format!("ONNX 解析失败（文件可能损坏或不是 ONNX）：{error}"),
            })?;
        let ir_version = Some(proto.ir_version).filter(|version| *version != 0);
        let opset = proto.opset_import.iter().map(|entry| entry.version).max();
        let operator_count = proto.graph.as_ref().map(|graph| graph.node.len()).unwrap_or(0);
        cancel.check().map_err(|_| InferenceError::Cancelled)?;

        // 第二段：类型推导拿形状。符号维记为 0 并保留可读描述。
        let typed = parse_typed(path, "")?;
        let shapes = introspect_typed(&typed, "")?;

        Ok(ModelIntrospection {
            ir_version,
            opset,
            inputs: shapes.inputs,
            outputs: shapes.outputs,
            operator_count,
            declarations: shapes.declarations,
        })
    }

    fn load(
        &self,
        spec: &ModelSpec,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<LoadedPlan, InferenceError> {
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        let typed = parse_typed(path, spec.id)?;
        // 契约在第 ⑥ 步再验一次：即使调用方跳过了准入链，也不能拿错模型硬跑。
        let introspection = introspect_typed(&typed, spec.id)?;
        check_contract(
            spec,
            &ContractSnapshot {
                inputs: introspection.input_shapes(),
                outputs: introspection.output_shapes(),
            },
        )?;
        cancel.check().map_err(|_| InferenceError::Cancelled)?;

        let runnable = typed
            .into_optimized()
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: spec.id.to_string(),
                detail: format!("ONNX 优化失败：{error}"),
            })?
            .into_runnable()
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: spec.id.to_string(),
                detail: format!("ONNX 编译失败：{error}"),
            })?;

        Ok(LoadedPlan {
            model_id: spec.id.to_string(),
            sha256: spec.sha256.to_string(),
            backend: <Self as InferenceBackend>::id(self),
            input_spec: spec.input.clone(),
            output_spec: Self::output_specs(spec),
            handle: PlanHandle::Tract(runnable),
        })
    }

    fn run(
        &self,
        plan: &LoadedPlan,
        input: &TensorBatch,
        options: &RunOptions,
        cancel: &CancelToken,
    ) -> Result<Vec<TensorBatch>, InferenceError> {
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        // R11 不支持低精度降级：显式声明而不是静默忽略。
        debug_assert!(
            !options.allow_quantized,
            "R11 不支持量化降级，allow_quantized 必须为 false"
        );
        if !input.spec.same_as(&plan.input_spec) {
            return Err(InferenceError::ContractMismatch {
                model_id: plan.model_id.clone(),
                detail: format!(
                    "输入契约 {} 与模型契约 {} 不一致",
                    input.spec.format_human(),
                    plan.input_spec.format_human()
                ),
            });
        }
        let PlanHandle::Tract(runnable) = &plan.handle else {
            return Err(InferenceError::BackendUnsupported(plan.backend.to_string()));
        };

        let array = tract_ndarray::ArrayD::from_shape_vec(
            tract_ndarray::IxDyn(&input.spec.shape),
            input.data.clone(),
        )
        .map_err(|error| InferenceError::ContractMismatch {
            model_id: plan.model_id.clone(),
            detail: format!("输入张量构建失败：{error}"),
        })?;
        if let Some((low, high)) = input.spec.value_range.bounds() {
            let _ = (low, high); // 值域已在 TensorBatch::new 校验，这里只作说明。
        }
        let tensor = array.into_tensor();

        let started = Instant::now();
        // 算子级 panic 隔离：宁可返回 E_INFER，也不能让推理线程把进程带走。
        let outputs = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runnable.run(tvec!(tensor.into_tvalue()))
        }))
        .map_err(|_| InferenceError::Runtime("推理过程发生 panic，已隔离".into()))?
        .map_err(|error| InferenceError::Runtime(format!("tract 推理失败：{error}")))?;
        let elapsed = started.elapsed();

        if let Some(timeout) = options.timeout {
            if elapsed > timeout {
                return Err(InferenceError::Timeout {
                    ms: timeout.as_millis() as u64,
                });
            }
        }
        cancel.check().map_err(|_| InferenceError::Cancelled)?;

        if outputs.len() != plan.output_spec.len() {
            return Err(InferenceError::ContractMismatch {
                model_id: plan.model_id.clone(),
                detail: format!(
                    "实际输出 {} 个 head，契约声明 {} 个",
                    outputs.len(),
                    plan.output_spec.len()
                ),
            });
        }

        let mut batches = Vec::with_capacity(outputs.len());
        for (index, output) in outputs.iter().enumerate() {
            let view = output
                .to_plain_array_view::<f32>()
                .map_err(|error| InferenceError::ContractMismatch {
                    model_id: plan.model_id.clone(),
                    detail: format!("输出 {index} 解析失败：{error}"),
                })?;
            let actual: Vec<usize> = view.shape().to_vec();
            let declared = &plan.output_spec[index];
            if !shape_matches(&actual, &declared.shape) {
                return Err(InferenceError::ContractMismatch {
                    model_id: plan.model_id.clone(),
                    detail: format!(
                        "输出 {index} 实际形状 {} 与契约 {} 不符",
                        format_shape(&actual),
                        format_shape(&declared.shape)
                    ),
                });
            }
            batches.push(TensorBatch {
                spec: declared.clone(),
                data: view.iter().copied().collect::<Vec<f32>>(),
            });
        }
        Ok(batches)
    }

    fn unload(&self, plan: LoadedPlan) -> Result<(), InferenceError> {
        // CPU 后端没有显存：drop 释放句柄即可。
        drop(plan);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::model_registry::builtin;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn model_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/models/yunet")
            .join(builtin::YUNET.file_name)
    }

    #[test]
    fn parses_real_yunet_and_matches_declared_contract() {
        let path = model_path();
        assert!(path.is_file(), "缺少随包模型：{}", path.display());
        let snapshot = inspect_contract(&path, builtin::YUNET.id).unwrap();
        assert_eq!(snapshot.inputs, vec![vec![1, 3, 640, 640]]);
        assert_eq!(snapshot.outputs.len(), 12);
        check_contract(&builtin::YUNET, &snapshot).unwrap();
    }

    #[test]
    fn inspect_reports_ir_version_opset_and_io_without_compiling() {
        // 设计 §8.6 验收点 ⑨：inspect() 必须能对真实 YuNet 给出
        // ir_version / opset / inputs / outputs，证明 trait 覆盖
        // model_hub/validator.rs 的「只读结构」语义（不编译 runnable）。
        let backend = CpuTractBackend::new();
        let introspection = backend
            .inspect(&model_path(), &CancelToken::default())
            .unwrap();
        assert!(introspection.ir_version.unwrap_or(0) > 0, "{introspection:?}");
        assert!(introspection.opset.unwrap_or(0) > 0, "{introspection:?}");
        assert_eq!(introspection.input_shapes(), vec![vec![1, 3, 640, 640]]);
        assert_eq!(introspection.outputs.len(), 12);
        assert_eq!(introspection.outputs[0].shape.as_ref(), &[1, 6400, 1]);
        assert!(introspection.operator_count > 0);
        assert!(
            !introspection.declarations.is_empty(),
            "值域/通道顺序未由 ONNX 声明，必须显式登记为待补充"
        );
    }

    #[test]
    fn inspect_is_cancellable() {
        let backend = CpuTractBackend::new();
        let cancel = CancelToken::new(Arc::new(std::sync::atomic::AtomicBool::new(true)));
        let error = backend.inspect(&model_path(), &cancel).unwrap_err();
        assert_eq!(error.code(), "E_CANCELLED");
    }

    #[test]
    fn inspect_rejects_a_non_onnx_file() {
        let backend = CpuTractBackend::new();
        let dir = std::env::temp_dir().join(format!("lumi-inspect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-onnx.onnx");
        std::fs::write(&path, b"definitely not a protobuf onnx model").unwrap();
        let error = backend.inspect(&path, &CancelToken::default()).unwrap_err();
        assert_eq!(error.code(), "E_MODEL_LOAD");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn contract_mismatch_is_reported_as_version_error() {
        let path = model_path();
        let snapshot = inspect_contract(&path, builtin::YUNET.id).unwrap();
        let wrong = crate::ai_engine::model_registry::spec::ModelSpec {
            input: TensorSpec {
                shape: Cow::Borrowed(&[1, 3, 320, 320]),
                ..builtin::YUNET.input
            },
            ..builtin::YUNET
        };
        let error = check_contract(&wrong, &snapshot).unwrap_err();
        assert_eq!(error.code(), "E_MODEL_VERSION");
        assert!(error.to_string().contains(builtin::YUNET.id));
    }

    #[test]
    fn load_produces_a_runnable_plan_and_reports_capabilities() {
        let backend = CpuTractBackend::new();
        let capabilities = backend.capabilities();
        assert!(capabilities.available);
        assert!(!capabilities.gpu);
        assert_eq!(capabilities.max_batch, 1);
        let plan = backend
            .load(&builtin::YUNET, &model_path(), &CancelToken::default())
            .unwrap();
        assert_eq!(plan.backend, "cpu-tract");
        assert_eq!(plan.output_count(), 12);
        backend.unload(plan).unwrap();
    }

    #[test]
    fn cancelled_token_aborts_before_parsing() {
        let backend = CpuTractBackend::new();
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let cancel = CancelToken::new(flag);
        let error = backend
            .load(&builtin::YUNET, &model_path(), &cancel)
            .unwrap_err();
        assert_eq!(error.code(), "E_CANCELLED");
    }

    #[test]
    fn run_rejects_input_with_wrong_contract() {
        let backend = CpuTractBackend::new();
        let plan = backend
            .load(&builtin::YUNET, &model_path(), &CancelToken::default())
            .unwrap();
        let wrong_spec = TensorSpec {
            shape: Cow::Borrowed(&[1, 3, 320, 320]),
            ..builtin::YUNET.input
        };
        let element_count = wrong_spec.element_count();
        let input = TensorBatch {
            spec: wrong_spec,
            data: vec![0.0; element_count],
        };
        let error = backend
            .run(
                &plan,
                &input,
                &RunOptions::cpu_default(),
                &CancelToken::default(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "E_MODEL_CONTRACT");
        backend.unload(plan).unwrap();
    }
}

