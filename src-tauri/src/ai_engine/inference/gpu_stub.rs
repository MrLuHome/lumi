//! GPU 后端**预留形状**。R11 不接 wgpu、不做推理运行时替换。
//!
//! 为什么保留一个永远不可用的实现：
//! - 先把 `InferenceBackend` 的方法签名固定下来，未来接 GPU 时调用方零改动；
//! - 让 `capabilities().available == false` + 明确理由 可以被前端与测试断言，
//!   避免“能力 advertised 但实际不可用”。
//!
//! 已有的 `crate::gpu::ComputeEngine`（wgpu 30）是**图像算子**（曲线/曝光/缩放），
//! 与推理无关，两者不复用也不冲突。

use std::path::Path;

use crate::ai_engine::model_registry::spec::ModelSpec;
use crate::image_engine::cancel::CancelToken;

use super::backend::{
    BackendCapabilities, InferenceBackend, LoadedPlan, ModelIntrospection, RunOptions,
};
use super::error::InferenceError;
use super::tensor::{Dtype, TensorBatch};

/// GPU 推理后端（未启用）。
#[derive(Debug, Default, Clone, Copy)]
pub struct GpuBackend;

impl GpuBackend {
    pub fn new() -> Self {
        Self
    }

    fn reason() -> String {
        "GPU 推理已预留接口但未启用：缺少 tract GPU 后端在 640×640 人脸检测上的 benchmark 依据"
            .to_string()
    }
}

impl InferenceBackend for GpuBackend {
    fn id(&self) -> &'static str {
        "gpu-reserved"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            available: false,
            unavailable_reason: Some(Self::reason()),
            dtypes: &[Dtype::F32],
            max_batch: 1,
            gpu: true,
        }
    }

    fn inspect(
        &self,
        _path: &Path,
        _cancel: &CancelToken,
    ) -> Result<ModelIntrospection, InferenceError> {
        Err(InferenceError::BackendUnsupported(Self::reason()))
    }

    fn load(
        &self,
        _spec: &ModelSpec,
        _path: &Path,
        _cancel: &CancelToken,
    ) -> Result<LoadedPlan, InferenceError> {
        Err(InferenceError::BackendUnsupported(Self::reason()))
    }

    fn run(
        &self,
        _plan: &LoadedPlan,
        _input: &TensorBatch,
        _options: &RunOptions,
        _cancel: &CancelToken,
    ) -> Result<Vec<TensorBatch>, InferenceError> {
        Err(InferenceError::BackendUnsupported(Self::reason()))
    }

    fn unload(&self, _plan: LoadedPlan) -> Result<(), InferenceError> {
        Err(InferenceError::BackendUnsupported(Self::reason()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::model_registry::builtin;

    #[test]
    fn gpu_backend_is_unavailable_with_a_reason() {
        let capabilities = GpuBackend::new().capabilities();
        assert!(!capabilities.available);
        assert!(capabilities.gpu);
        assert!(capabilities
            .unavailable_reason
            .as_deref()
            .unwrap_or_default()
            .contains("benchmark"));
    }

    #[test]
    fn gpu_backend_rejects_inspect_with_backend_code() {
        let error = GpuBackend::new()
            .inspect(Path::new("/nonexistent.onnx"), &CancelToken::default())
            .unwrap_err();
        assert_eq!(error.code(), "E_BACKEND");
    }

    #[test]
    fn gpu_backend_rejects_load_with_backend_code() {
        let error = GpuBackend::new()
            .load(
                &builtin::YUNET,
                Path::new("/nonexistent.onnx"),
                &CancelToken::default(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "E_BACKEND");
    }
}
