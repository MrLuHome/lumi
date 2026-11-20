//! 统一图像处理引擎。
//!
//! 外部只通过本模块的 `pipeline::render` 得到结果，内部细节
//! （解码、16-bit 缓冲、调色阶段、编码、导出）对命令层透明。

pub mod analysis;
pub mod buffer;
pub mod cancel;
pub mod color;
pub mod decoder;
pub mod encoder;
pub mod export;
pub mod geometry;
pub mod operations;
pub mod params;
pub mod pipeline;
pub mod preset;
pub mod preview;
pub mod project;

pub use buffer::Image16;
pub use cancel::CancelToken;
pub use decoder::{BitDepth, DecodedImage};
pub use export::{ExportFormat, ExportSettings};
pub use params::EditParams;
pub use pipeline::{render, render_with_progress};
pub use preview::{CanvasPreview, DETAIL_MAX_EDGE, PREVIEW_MAX_EDGE, THUMB_MAX_EDGE};
pub use project::{MaskKind, MaskLayer, MaskPoint, ProjectDoc, ProjectPhoto};
