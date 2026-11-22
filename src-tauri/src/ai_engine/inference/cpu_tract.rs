//! CPU 推理后端（唯一真实实现）。
//!
//! 继续使用 `tract-onnx 0.23`：**不替换运行时**。这里只做三件事：
//! 1) 把「解析 / 优化 / 编译」收拢到一个入口，让 plan 可以被缓存复用；
//! 2) 把 tract 的 `TValue` 与统一张量描述互转；
//! 3) 用 `catch_unwind` 把算子级 panic 隔离成 `E_INFER`，而不是让整个进程挂掉。

use std::path::Path;
use std::time::Instant;

use tract_onnx::prelude::*;

use crate::ai_engine::model_registry::spec::ModelSpec;
use crate::image_engine::cancel::CancelToken;

use super::backend::{BackendCapabilities, InferenceBackend, LoadedPlan, PlanHandle, RunOptions};
use super::error::InferenceError;
use super::tensor::{Dtype, TensorBatch, TensorSpec, ValueRange};

/// 只读契约快照：描述一个 ONNX 文件**实际**声明的输入输出形状。
///
/// 符号维（动态维）记为 `0`，在比对时充当通配符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSnapshot {
    pub inputs: Vec<Vec<usize>>,
    pub outputs: Vec<Vec<usize>>,
    /// 输入张量的数据类型（`DatumType` 的调试名），仅用于诊断。
    pub dtype: Option<String>,
}

/// 只解析、不优化、不编译地读出模型契约。
///
/// 这是准入判定链第 ④ 步「契约」的实现：成本是纯解析（232 KB 的 YuNet 在毫秒级），
/// 与第 ⑥ 步「解析」的区别是不做常量折叠与算子替换。
pub fn inspect_contract(path: &Path, model_id: &str) -> Result<ContractSnapshot, InferenceError> {
    let typed = parse_typed(path, model_id)?;
    snapshot_of(&typed, model_id)
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
    if !shape_matches(&snapshot.inputs[0], spec.input.shape) {
        return Err(InferenceError::ModelVersionMismatch {
            model_id: spec.id.to_string(),
            expected: format_shape(spec.input.shape),
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

fn snapshot_of(
    typed: &TypedModel,
    model_id: &str,
) -> Result<ContractSnapshot, InferenceError> {
    let mut inputs = Vec::with_capacity(typed.inputs.len());
    for outlet in &typed.inputs {
        let fact = typed
            .outlet_fact(*outlet)
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: model_id.to_string(),
                detail: format!("读取输入契约失败：{error}"),
            })?;
        inputs.push(shape_of(fact));
    }
    let mut outputs = Vec::with_capacity(typed.outputs.len());
    for outlet in &typed.outputs {
        let fact = typed
            .outlet_fact(*outlet)
            .map_err(|error| InferenceError::ModelLoadFailed {
                model_id: model_id.to_string(),
                detail: format!("读取输出契约失败：{error}"),
            })?;
        outputs.push(shape_of(fact));
    }
    let dtype = typed
        .inputs
        .first()
        .and_then(|outlet| typed.outlet_fact(*outlet).ok())
        .map(|fact| format!("{:?}", fact.datum_type));
    Ok(ContractSnapshot {
        inputs,
        outputs,
        dtype,
    })
}

fn shape_of(fact: &TypedFact) -> Vec<usize> {
    match fact.shape.as_concrete() {
        Some(dims) => dims.to_vec(),
        // 动态维：用 0 占位，比对时当通配符。
        None => vec![0; fact.shape.rank()],
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
                shape: head.shape,
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

    fn load(
        &self,
        spec: &ModelSpec,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<LoadedPlan, InferenceError> {
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        let typed = parse_typed(path, spec.id)?;
        // 契约在第 ⑥ 步再验一次：即使调用方跳过了准入链，也不能拿错模型硬跑。
        let snapshot = snapshot_of(&typed, spec.id)?;
        check_contract(spec, &snapshot)?;
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
            input_spec: spec.input,
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
            tract_ndarray::IxDyn(input.spec.shape),
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
            let declared = plan.output_spec[index].shape;
            if !shape_matches(&actual, declared) {
                return Err(InferenceError::ContractMismatch {
                    model_id: plan.model_id.clone(),
                    detail: format!(
                        "输出 {index} 实际形状 {} 与契约 {} 不符",
                        format_shape(&actual),
                        format_shape(declared)
                    ),
                });
            }
            batches.push(TensorBatch {
                spec: plan.output_spec[index],
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
    fn contract_mismatch_is_reported_as_version_error() {
        let path = model_path();
        let snapshot = inspect_contract(&path, builtin::YUNET.id).unwrap();
        let wrong = crate::ai_engine::model_registry::spec::ModelSpec {
            input: TensorSpec {
                shape: &[1, 3, 320, 320],
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
            shape: &[1, 3, 320, 320],
            ..builtin::YUNET.input
        };
        let input = TensorBatch {
            spec: wrong_spec,
            data: vec![0.0; wrong_spec.element_count()],
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

