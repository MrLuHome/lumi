//! ROUND 8：统一人脸智能层。
//!
//! 目标是把“人脸分析”从具体模型实现中解耦：
//! - `Provider` 只输出统一语义结构，不把某个 SDK 的数组下标泄漏到变形引擎；
//! - 能力声明必须附带证据等级（实测 / 声明 / 未验证 / 不支持）；
//! - LEVEL 3 五官精修由质量门控逐项解锁，没有合格数据就保持禁用并说明原因；
//! - 用户自备 ONNX 模型经过安全校验与隔离验证后才可能成为 Provider。
//!
//! 本模块不修改 RAW / ICC / GPU / 批量处理核心。

pub mod anchors;
pub mod capability;
pub mod coordinate;
pub mod dense;
pub mod model_hub;
pub mod provider;
pub mod providers;
pub mod quality;
pub mod registry;
pub mod topology;
pub mod types;

pub use capability::ProviderState;
pub use provider::{AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth};
pub use registry::FaceIntelligence;
pub use types::{
    BoundingBox, DenseFace, DenseRegion, FaceAnalysisResult, FaceMesh, FaceResult, Landmark,
    RegionKind, SemanticMask,
};
