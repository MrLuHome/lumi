//! 统一推理抽象层。
//!
//! 边界铁律：本模块**不依赖 Tauri**，也不做任何网络访问。
//! - 只读探测：[`InferenceBackend::inspect`]（不编译 runnable）
//! - 加载接口：[`InferenceBackend::load`]
//! - 统一张量 IO：[`TensorBatch`] / [`TensorSpec`]
//! - 统一错误类型：[`InferenceError`]（11 个稳定错误码）
//! - 取消：[`crate::image_engine::cancel::CancelToken`]（**复用，不新造**）
//!
//! 后端实现：CPU 一个真实实现（tract-onnx 0.23），GPU 只做 stub。

pub mod backend;
pub mod cpu_tract;
pub mod error;
pub mod gpu_stub;
pub mod tensor;

pub use backend::{
    BackendCapabilities, InferenceBackend, LoadedPlan, ModelIntrospection, PlanHandle,
    RunOptions,
};
pub use cpu_tract::{check_contract, inspect_contract, ContractSnapshot, CpuTractBackend};
pub use error::{InferenceError, ERROR_CODES};
pub use gpu_stub::GpuBackend;
pub use tensor::{ChannelOrder, Dtype, Layout, TensorBatch, TensorSpec, ValueRange};

/// 默认 CPU 后端实例（零状态，可自由复制）。
pub fn cpu_backend() -> CpuTractBackend {
    CpuTractBackend::new()
}
