//! 自实现的 16-bit 未压缩 RGB TIFF 编码器。
//!
//! 不是“把扩展名改成 .tif”：文件头、IFD、tag、strip 全部按 TIFF 6.0 写出，
//! 并支持 tag 34675 嵌入 ICC Profile。

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

const ENTRY_WIDTH: usize = 12;
const ICC_TAG: u16 = 34675;

/// 写出未压缩 RGB16 TIFF；`icc` 为空时不写 34675。
pub fn encode(image: &Image16, icc: Option<&[u8]>) -> AppResult<Vec<u8>> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err(AppError::Unsupported("TIFF 尺寸为 0".into()));
    }
    let has_icc = icc.map(|bytes| !bytes.is_empty()).unwrap_or(false);
    let entries = if has_icc { 13 } else { 12 };
    let ifd_size = 2 + entries * ENTRY_WIDTH + 4;
    let external = 8 + ifd_size;

    let bits_offset = external;
    let xres_offset = bits_offset + 6;
    let yres_offset = xres_offset + 8;
    let icc_offset = yres_offset + 8;
    let pixel_offset = if has_icc {
        let end = icc_offset + icc.unwrap().len();
        (end + 1) & !1
    } else {
        icc_offset
    };
    let pixel_bytes = width as usize * height as usize * 6;

    let mut out = Vec::with_capacity(pixel_offset + pixel_bytes + 16);
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());

    out.extend_from_slice(&(entries as u16).to_le_bytes());
    let mut entry = |tag: u16, kind: u16, count: u32, value: [u8; 4]| {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value);
    };
    let long = |value: u32| value.to_le_bytes();
    let short_inline = |value: u16| {
        let mut bytes = [0u8; 4];
        bytes[..2].copy_from_slice(&value.to_le_bytes());
        bytes
    };

    entry(256, 4, 1, long(width));
    entry(257, 4, 1, long(height));
    entry(258, 3, 3, long(bits_offset as u32));
    entry(259, 3, 1, short_inline(1));
    entry(262, 3, 1, short_inline(2));
    entry(273, 4, 1, long(pixel_offset as u32));
    entry(277, 3, 1, short_inline(3));
    entry(278, 4, 1, long(height));
    entry(279, 4, 1, long(pixel_bytes as u32));
    entry(282, 5, 1, long(xres_offset as u32));
    entry(283, 5, 1, long(yres_offset as u32));
    entry(296, 3, 1, short_inline(2));
    if let Some(icc) = icc.filter(|bytes| !bytes.is_empty()) {
        entry(ICC_TAG, 7, icc.len() as u32, long(icc_offset as u32));
    }
    out.extend_from_slice(&0u32.to_le_bytes());

    // 外部数据区
    for value in [16u16, 16, 16] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&72u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&72u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    if let Some(icc) = icc.filter(|bytes| !bytes.is_empty()) {
        out.extend_from_slice(icc);
        if out.len() % 2 != 0 {
            out.push(0);
        }
    }
    debug_assert_eq!(out.len(), pixel_offset);

    for pixel in image.pixels() {
        out.extend_from_slice(&pixel.0[0].to_le_bytes());
        out.extend_from_slice(&pixel.0[1].to_le_bytes());
        out.extend_from_slice(&pixel.0[2].to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn header_and_ifd_are_valid() {
        let image: Image16 = ImageBuffer::from_fn(4, 3, |x, y| {
            Rgba([(x * 1000) as u16, (y * 2000) as u16, 42, 65535])
        });
        let bytes = encode(&image, None).unwrap();
        assert_eq!(&bytes[..4], b"II*\0");
        let ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let count = u16::from_le_bytes([bytes[ifd], bytes[ifd + 1]]) as usize;
        assert_eq!(count, 12);
        let mut decoder = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(decoder.dimensions().unwrap(), (4, 3));
    }

    #[test]
    fn icc_tag_roundtrips() {
        let image: Image16 = ImageBuffer::from_pixel(2, 2, Rgba([1, 2, 3, 65535]));
        let icc = vec![7u8; 512];
        let bytes = encode(&image, Some(&icc)).unwrap();
        let extracted = super::super::embed::extract_icc(&bytes).unwrap().unwrap();
        assert_eq!(extracted, icc);
    }

    #[test]
    fn pixel_bytes_are_little_endian_rgb16() {
        let image: Image16 = ImageBuffer::from_pixel(1, 1, Rgba([0x1234, 0x5678, 0x9ABC, 0xFFFF]));
        let bytes = encode(&image, None).unwrap();
        let pixel = &bytes[bytes.len() - 6..];
        assert_eq!(pixel, &[0x34, 0x12, 0x78, 0x56, 0xBC, 0x9A]);
    }
}
