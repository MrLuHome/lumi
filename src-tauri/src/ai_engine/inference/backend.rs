//! 推理后端抽象。
//!
//! R11 只有 CPU 一个真实实现（[`super::cpu_tract::CpuTractBackend`]），
//! GPU 后端只留形状（[`super::gpu_stub::GpuBackend`]），**不接 wgpu、
//! 不替换 `tract-onnx 0.23`** —— 没有 benchmark 依据前不做运行时替换。

use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use tract_onnx::prelude::TypedRunnableModel;

use crate::ai_engine::model_registry::spec::ModelSpec;
use crate::image_engine::cancel::CancelToken;

use super::error::InferenceError;
use super::tensor::{Dtype, TensorBatch, TensorSpec};

/// 一个已解析 + 已优化 + 已编译的模型句柄。
///
/// 由调用方以 `(model_id, sha256)` 为键缓存（键必须含 sha256：
/// 同一个 ID 换了文件时旧 plan 必须失效）。
pub struct LoadedPlan {
    pub model_id: String,
    pub sha256: String,
    pub backend: &'static str,
    pub input_spec: TensorSpec,
    pub output_spec: Vec<TensorSpec>,
    pub handle: PlanHandle,
}

impl LoadedPlan {
    /// 输出 head 数量。
    pub fn output_count(&self) -> usize {
        self.output_spec.len()
    }
}

impl std::fmt::Debug for LoadedPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedPlan")
            .field("model_id", &self.model_id)
            .field("sha256", &self.sha256)
            .field("backend", &self.backend)
            .field("input_spec", &self.input_spec)
            .field("output_spec", &self.output_spec)
            .finish_non_exhaustive()
    }
}

/// 后端私有句柄。CPU = tract 的 runnable model；GPU 为预留。
pub enum PlanHandle {
    Tract(Arc<TypedRunnableModel>),
    /// 预留：GPU 句柄。R11 不会构造该分支。
    Gpu(Box<dyn Any + Send + Sync>),
}

/// 只读结构视图。**不编译 runnable**，只报告 ONNX 文件声明的结构。
///
/// 为什么要有 `inspect()` 而不是只靠 `load()`：
/// `face_intelligence/model_hub/validator.rs:87` 用 `proto_model_for_path`
/// （只读 ir_version / opset / 形状，不编译），`:100` 才用 `model_for_path`，
/// 是两个不同 API。`load()` 覆盖不了前者（它一定会编译 runnable，
/// 对"只想看一眼这个文件长什么样"的导入校验是过度开销，也不能容忍编译失败）。
/// 把 `inspect()` 放进 trait，未来把 Model Hub 校验迁到统一后端时不用改 trait 形状。
///
/// 符号维（动态维）在 `inputs/outputs[].shape` 里记为 `0`（通配），
/// 原始可读描述保存在 `declarations` 里，避免"零维标量"与"未知维"被混为一谈。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelIntrospection {
    pub ir_version: Option<i64>,
    pub opset: Option<i64>,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
    pub operator_count: usize,
    /// 未能表达成 `TensorSpec` 的原始描述（符号维、非 F32 dtype、未知值域等）。
    pub declarations: Vec<String>,
}

impl ModelIntrospection {
    /// 空视图（解析失败前的占位）。
    pub fn empty() -> Self {
        Self {
            ir_version: None,
            opset: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            operator_count: 0,
            declarations: Vec::new(),
        }
    }

    pub fn input_shapes(&self) -> Vec<Vec<usize>> {
        self.inputs.iter().map(|spec| spec.shape.to_vec()).collect()
    }

    pub fn output_shapes(&self) -> Vec<Vec<usize>> {
        self.outputs
            .iter()
            .map(|spec| spec.shape.to_vec())
            .collect()
    }
}

/// 后端能力声明。
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub dtypes: &'static [Dtype],
    /// 单次推理支持的最大批大小。R11 恒为 1。
    pub max_batch: usize,
    pub gpu: bool,
}

/// 单次推理选项。
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// 推理超时。CPU 路径在推理返回后检查（tract 0.23 不暴露中断钩子，
    /// 因此**无法硬杀正在执行的算子**）。
    pub timeout: Option<Duration>,
    /// 是否允许降级到更低精度。R11 恒为 false。
    pub allow_quantized: bool,
}

impl RunOptions {
    /// R11 的默认选项：不超时、不降精度。
    pub fn cpu_default() -> Self {
        Self {
            timeout: None,
            allow_quantized: false,
        }
    }
}

/// 推理后端。所有方法都不触碰 Tauri。
pub trait InferenceBackend: Send + Sync {
    /// 稳定标识："cpu-tract" / "gpu-reserved"。
    fn id(&self) -> &'static str;

    fn capabilities(&self) -> BackendCapabilities;

    /// 只读结构探测：不编译 runnable，只返回 ir_version / opset / 输入输出形状。
    /// 对应 `model_hub/validator.rs` 的 `proto_model_for_path` 语义。
    fn inspect(
        &self,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<ModelIntrospection, InferenceError>;

    /// 解析 + 优化 + 编译。返回值里的 `handle` 可直接并发 `&self` 调用。
    fn load(
        &self,
        spec: &ModelSpec,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<LoadedPlan, InferenceError>;

    /// 单批推理。输入输出都用统一张量描述。
    ///
    /// 注意：返回的是**多输出**的 `Vec<TensorBatch>`（YuNet 有 12 个 head），
    /// 而不是设计稿里的单个 `TensorBatch` —— 单张量无法表达多 head 契约。
    fn run(
        &self,
        plan: &LoadedPlan,
        input: &TensorBatch,
        options: &RunOptions,
        cancel: &CancelToken,
    ) -> Result<Vec<TensorBatch>, InferenceError>;

    /// 释放。CPU 后端无操作（drop 即释放），GPU 后端释放显存。
    fn unload(&self, plan: LoadedPlan) -> Result<(), InferenceError>;
}
