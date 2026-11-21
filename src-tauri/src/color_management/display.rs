//! macOS 显示器色彩管理：通过 ColorSync 读取主显示器 ICC，并把预览转换到显示 Profile。
//!
//! 读取失败时回退 sRGB，并在状态中明确标注“sRGB 回退”，不假装成功。
//! 多显示器切换的事件监听不在本模块实现，UI 提供手动刷新入口。

use lcms2::Profile;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

use super::profiles;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayStatus {
    /// `colorsync` 或 `srgb-fallback`。
    pub source: String,
    pub description: String,
    pub available: bool,
    /// 失败原因（成功时为空）。
    pub reason: String,
}

/// 读取主显示器 ICC 原始字节。
pub fn display_icc() -> Option<Vec<u8>> {
    imp::display_icc()
}

/// 当前显示器状态。失败时返回 sRGB 回退说明。
pub fn status() -> DisplayStatus {
    match display_icc() {
        Some(bytes) => match profiles::from_bytes(&bytes) {
            Ok(profile) => DisplayStatus {
                source: "colorsync".into(),
                description: profile
                    .info(lcms2::InfoType::Description, lcms2::Locale::none())
                    .unwrap_or_else(|| "主显示器 ICC".into()),
                available: true,
                reason: String::new(),
            },
            Err(error) => fallback(format!("ColorSync 数据无法解析：{error}")),
        },
        None => fallback("无法从 ColorSync 读取主显示器 ICC".into()),
    }
}

fn fallback(reason: String) -> DisplayStatus {
    DisplayStatus {
        source: "srgb-fallback".into(),
        description: "sRGB（回退）".into(),
        available: false,
        reason,
    }
}

/// 把 sRGB 预览转换到显示器 Profile；读取失败时原样返回并附带状态。
pub fn to_display(image: &Image16) -> AppResult<(Image16, DisplayStatus)> {
    let status = status();
    let Some(bytes) = display_icc() else {
        return Ok((image.clone(), status));
    };
    let display = profiles::from_bytes(&bytes)?;
    let srgb = Profile::new_srgb();
    let converted = convert_to_profile(image, &srgb, &display)?;
    Ok((converted, status))
}

fn convert_to_profile(
    image: &Image16,
    input: &Profile,
    output: &Profile,
) -> AppResult<Image16> {
    use image::{ImageBuffer, Rgba};
    use lcms2::{PixelFormat, Transform};

    let transform: Transform<[u16; 3], [u16; 3]> = Transform::new(
        input,
        PixelFormat::RGB_16,
        output,
        PixelFormat::RGB_16,
        lcms2::Intent::RelativeColorimetric,
    )
    .map_err(|error| AppError::Task(format!("显示器 Profile 转换失败：{error}")))?;
    let width = image.width();
    let src: Vec<[u16; 3]> = image
        .pixels()
        .map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2]])
        .collect();
    let mut dst = vec![[0u16; 3]; src.len()];
    transform.transform_pixels(&src, &mut dst);
    let mut out: Image16 = ImageBuffer::new(width, image.height());
    for (index, pixel) in dst.iter().enumerate() {
        let alpha = image.get_pixel(index as u32 % width, index as u32 / width).0[3];
        out.put_pixel(
            index as u32 % width,
            index as u32 / width,
            Rgba([pixel[0], pixel[1], pixel[2], alpha]),
        );
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::c_void;

    type CGDirectDisplayID = u32;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        fn CGDisplayCopyColorSpace(display: CGDirectDisplayID) -> *mut c_void;
        fn CGColorSpaceCopyICCData(space: *mut c_void) -> *const c_void;
        fn CGColorSpaceRelease(space: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetLength(data: *const c_void) -> isize;
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(cf: *const c_void);
    }

    pub fn display_icc() -> Option<Vec<u8>> {
        // SAFETY: 所有 CoreGraphics/CoreFoundation 对象都在本函数内释放；
        // 每个返回值都做空指针检查，只在数据有效期内拷贝。
        unsafe {
            let display = CGMainDisplayID();
            let space = CGDisplayCopyColorSpace(display);
            if space.is_null() {
                return None;
            }
            let data = CGColorSpaceCopyICCData(space);
            CGColorSpaceRelease(space);
            if data.is_null() {
                return None;
            }
            let length = CFDataGetLength(data);
            let ptr = CFDataGetBytePtr(data);
            if length <= 0 || ptr.is_null() {
                CFRelease(data);
                return None;
            }
            let bytes = std::slice::from_raw_parts(ptr, length as usize).to_vec();
            CFRelease(data);
            Some(bytes)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn display_icc() -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn status_is_reported_even_without_profile() {
        let status = status();
        assert!(status.source == "colorsync" || status.source == "srgb-fallback");
        if !status.available {
            assert!(!status.reason.is_empty(), "回退必须给出原因");
        }
    }

    #[test]
    fn preview_conversion_is_safe() {
        let image: Image16 = ImageBuffer::from_pixel(2, 2, Rgba([1000, 2000, 3000, 65535]));
        let (out, _) = to_display(&image).unwrap();
        assert_eq!(out.dimensions(), image.dimensions());
    }
}
