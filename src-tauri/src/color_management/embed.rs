//! ICC Profile 嵌入与回读：JPEG APP2、PNG iCCP、TIFF tag 34675。
//!
//! 全部为自有字节级实现，不依赖图像编码器的私有扩展；写入后可通过
//! `extract_icc` 重新读出并交给 Little CMS 校验。

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::error::{AppError, AppResult};

const ICC_TAG: u16 = 34675;
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const ICC_APP2: &[u8] = b"ICC_PROFILE\0";

/// 把 ICC 写入图片字节；不支持的格式返回原数据。
pub fn embed(data: Vec<u8>, icc: &[u8]) -> AppResult<Vec<u8>> {
    if icc.is_empty() {
        return Ok(data);
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        return embed_jpeg(data, icc);
    }
    if data.starts_with(&PNG_SIGNATURE) {
        return embed_png(data, icc);
    }
    if data.starts_with(b"II") || data.starts_with(b"MM") {
        return Err(AppError::Unsupported(
            "TIFF 的 ICC 由 16-bit 编码器直接写入；不支持对已有 TIFF 追加".into(),
        ));
    }
    Ok(data)
}

/// 从图片字节中读出嵌入的 ICC。
pub fn extract_icc(data: &[u8]) -> AppResult<Option<Vec<u8>>> {
    if data.starts_with(&[0xFF, 0xD8]) {
        return Ok(extract_jpeg(data));
    }
    if data.starts_with(&PNG_SIGNATURE) {
        return extract_png(data);
    }
    if data.starts_with(b"II") || data.starts_with(b"MM") {
        return Ok(extract_tiff(data));
    }
    Ok(None)
}

fn embed_jpeg(mut data: Vec<u8>, icc: &[u8]) -> AppResult<Vec<u8>> {
    // 单段上限：65533 长度 - 2 长度字节 - 14 ICC 头 = 65517 字节数据。
    const CHUNK: usize = 65517;
    let chunks: Vec<&[u8]> = icc.chunks(CHUNK).collect();
    if chunks.len() > 255 {
        return Err(AppError::Unsupported(
            "ICC 过大，超过 JPEG APP2 上限".into(),
        ));
    }
    let mut payload = Vec::with_capacity(icc.len() + chunks.len() * 16);
    for (index, chunk) in chunks.iter().enumerate() {
        let length = (chunk.len() + 16) as u16;
        payload.extend_from_slice(&[0xFF, 0xE2, (length >> 8) as u8, length as u8]);
        payload.extend_from_slice(ICC_APP2);
        payload.push(index as u8 + 1);
        payload.push(chunks.len() as u8);
        payload.extend_from_slice(chunk);
    }
    data.splice(2..2, payload);
    Ok(data)
}

fn extract_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut index = 2usize;
    while index + 4 <= data.len() {
        if data[index] != 0xFF {
            break;
        }
        let marker = data[index + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let length = u16::from_be_bytes([data[index + 2], data[index + 3]]) as usize;
        if length < 2 || index + 2 + length > data.len() {
            break;
        }
        let segment = &data[index + 4..index + 2 + length];
        if marker == 0xE2 && segment.len() > 14 && segment.starts_with(ICC_APP2) {
            let sequence = segment[12];
            chunks.push((sequence, segment[14..].to_vec()));
        }
        index += 2 + length;
    }
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|(sequence, _)| *sequence);
    Some(chunks.into_iter().flat_map(|(_, bytes)| bytes).collect())
}

fn embed_png(mut data: Vec<u8>, icc: &[u8]) -> AppResult<Vec<u8>> {
    // 在 IHDR 之后插入 iCCP。
    if data.len() < 33 {
        return Err(AppError::Decode("PNG 数据不完整".into()));
    }
    let ihdr_length = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let insert_at = 8 + 12 + ihdr_length;

    let mut compressed = Vec::new();
    {
        let mut encoder = ZlibEncoder::new(&mut compressed, Compression::default());
        encoder
            .write_all(icc)
            .map_err(|error| AppError::Task(format!("ICC 压缩失败：{error}")))?;
        encoder
            .finish()
            .map_err(|error| AppError::Task(format!("ICC 压缩失败：{error}")))?;
    }
    let mut chunk_data = Vec::with_capacity(compressed.len() + 8);
    chunk_data.extend_from_slice(b"LUMI\0");
    chunk_data.push(0);
    chunk_data.extend_from_slice(&compressed);

    let mut chunk = Vec::with_capacity(chunk_data.len() + 12);
    chunk.extend_from_slice(&(chunk_data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"iCCP");
    chunk.extend_from_slice(&chunk_data);
    let mut crc_input = Vec::with_capacity(chunk_data.len() + 4);
    crc_input.extend_from_slice(b"iCCP");
    crc_input.extend_from_slice(&chunk_data);
    chunk.extend_from_slice(&crc32(&crc_input).to_be_bytes());

    data.splice(insert_at..insert_at, chunk);
    Ok(data)
}

fn extract_png(data: &[u8]) -> AppResult<Option<Vec<u8>>> {
    let mut index = 8usize;
    while index + 12 <= data.len() {
        let length = u32::from_be_bytes([
            data[index],
            data[index + 1],
            data[index + 2],
            data[index + 3],
        ]) as usize;
        if index + 12 + length > data.len() {
            break;
        }
        let kind = &data[index + 4..index + 8];
        if kind == b"iCCP" {
            let body = &data[index + 8..index + 8 + length];
            let name_end = body.iter().position(|byte| *byte == 0).unwrap_or(0);
            if name_end + 2 > body.len() {
                return Err(AppError::Decode("PNG iCCP 数据异常".into()));
            }
            let compressed = &body[name_end + 2..];
            let mut out = Vec::new();
            let mut decoder = flate2::read::ZlibDecoder::new(compressed);
            std::io::Read::read_to_end(&mut decoder, &mut out)
                .map_err(|error| AppError::Decode(format!("PNG ICC 解压失败：{error}")))?;
            return Ok(Some(out));
        }
        if kind == b"IEND" {
            break;
        }
        index += 12 + length;
    }
    Ok(None)
}

/// 从 TIFF 中读取 tag 34675。
fn extract_tiff(data: &[u8]) -> Option<Vec<u8>> {
    let little = data.starts_with(b"II");
    let read_u16 = |offset: usize| -> Option<u16> {
        let bytes = data.get(offset..offset + 2)?;
        Some(if little {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        })
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        let bytes = data.get(offset..offset + 4)?;
        Some(if little {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    };
    if read_u16(2)? != 42 {
        return None;
    }
    let mut ifd = read_u32(4)? as usize;
    let count = read_u16(ifd)? as usize;
    ifd += 2;
    for entry in 0..count {
        let offset = ifd + entry * 12;
        let tag = read_u16(offset)?;
        if tag == ICC_TAG {
            let size = read_u32(offset + 4)? as usize;
            let value = read_u32(offset + 8)? as usize;
            return data.get(value..value + size).map(|bytes| bytes.to_vec());
        }
    }
    None
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB_88320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jpeg() -> Vec<u8> {
        // SOI + APP0(JFIF) + SOS + 假数据。
        let mut data = vec![0xFF, 0xD8];
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        data.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        data.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 2, 3, 4, 5, 6]);
        data
    }

    fn fake_png() -> Vec<u8> {
        let mut data = PNG_SIGNATURE.to_vec();
        let ihdr = [0u8, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0, 0, 0, 0, 0];
        data.extend_from_slice(&(13u32).to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&ihdr[..13]);
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(b"IEND");
        data.extend_from_slice(&[0, 0, 0, 0]);
        data
    }

    #[test]
    fn jpeg_app2_roundtrip_splits_large_profiles() {
        let icc = vec![0xABu8; 140_000];
        let embedded = embed(fake_jpeg(), &icc).unwrap();
        let extracted = extract_icc(&embedded).unwrap().unwrap();
        assert_eq!(extracted, icc);
    }

    #[test]
    fn png_iccp_roundtrip() {
        let icc = vec![0x5Au8; 4096];
        let embedded = embed(fake_png(), &icc).unwrap();
        let extracted = extract_icc(&embedded).unwrap().unwrap();
        assert_eq!(extracted, icc);
    }

    #[test]
    fn non_icc_files_are_untouched() {
        let raw = vec![1u8, 2, 3];
        assert!(extract_icc(&raw).unwrap().is_none());
        assert_eq!(embed(raw.clone(), &[]).unwrap(), raw);
    }
}
