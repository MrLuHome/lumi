use crate::error::AppResult;
use crate::image_engine::buffer::{fit_within, Image16};
use crate::image_engine::decoder::BitDepth;
use crate::image_engine::encoder;

/// 交互预览长边。超过这个尺寸的缩放对屏幕没有意义，
/// 却会线性拉长每次滑杆回调的渲染时间。
pub const PREVIEW_MAX_EDGE: u32 = 1800;
pub const THUMB_MAX_EDGE: u32 = 400;
/// 100% / 200% 查看时的高清预览上限；导出不受此限制。
pub const DETAIL_MAX_EDGE: u32 = 3200;

pub struct CanvasPreview {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

pub fn build_preview(image: &Image16, max_edge: u32) -> Image16 {
    fit_within(image, max_edge)
}

pub fn encode_canvas(image: &Image16, has_alpha: bool) -> AppResult<CanvasPreview> {
    Ok(CanvasPreview {
        url: encoder::preview_data_url(image, has_alpha)?,
        width: image.width(),
        height: image.height(),
    })
}

pub fn encode_thumb(image: &Image16, has_alpha: bool) -> AppResult<String> {
    encoder::thumb_data_url(image, has_alpha)
}

/// 判断是否需要按原始精度导出（PNG 16-bit 源 → PNG 16-bit 输出）。
pub fn supports_sixteen_bit_export(depth: BitDepth) -> bool {
    depth == BitDepth::Sixteen
}
