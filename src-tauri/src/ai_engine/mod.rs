//! AI 人像引擎：人脸几何、皮肤区域、美肤操作、皮肤分析与一键建议。
//! 第五轮新增：手动人物蒙版、身材轮廓调整、背景处理。

pub mod background;
pub mod body;
pub mod face_frame;
pub mod inference;
pub mod model_registry;
pub mod retouch;
pub mod segmentation;
pub mod skin_segmentation;

pub use face_frame::{FaceFrame, FaceGeometry, FaceTracker};
pub use skin_segmentation::{build_skin_mask, SkinMask};
