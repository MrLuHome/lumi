//! LibRaw 的最小 FFI 封装。
//!
//! 本文件是 RAW 模块中**唯一**允许出现 `unsafe` 的地方：
//! 所有指针解引用、C 字符串读取、原始像素拷贝都集中在这里，
//! 并逐处标注 SAFETY。上层只拿到拥有所有权的安全结构体。
//!
//! 约束：只调用 `libraw_init` / `libraw_open_file` / `libraw_unpack` /
//! `libraw_unpack_thumb` / `libraw_close`，**绝不调用 `libraw_dcraw_process`**，
//! 也不做去马赛克、色彩转换、gamma 或任何美化处理。

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use libraw_rs_vendor as lr;

use super::{RawError, RawResult};

const FILTERS_XTRANS: u32 = 9;

/// 从 LibRaw 读出的一份完整 RAW 数据（拥有所有权）。
pub(crate) struct RawFile {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) raw_width: u32,
    pub(crate) raw_height: u32,
    pub(crate) top_margin: u32,
    pub(crate) left_margin: u32,
    pub(crate) filters: u32,
    pub(crate) colors: u32,
    pub(crate) xtrans: Option<[[u8; 6]; 6]>,
    pub(crate) black: u32,
    pub(crate) cblack: Vec<u32>,
    pub(crate) maximum: u32,
    pub(crate) cam_mul: [f32; 4],
    pub(crate) pre_mul: [f32; 4],
    pub(crate) cam_xyz: [[f32; 3]; 4],
    pub(crate) make: String,
    pub(crate) model: String,
    pub(crate) iso: f32,
    pub(crate) shutter: f32,
    pub(crate) aperture: f32,
    pub(crate) focal_len: f32,
    pub(crate) timestamp: i64,
    pub(crate) thumb_format: u32,
    pub(crate) thumb_width: u32,
    pub(crate) thumb_height: u32,
    pub(crate) thumb_bytes: u32,
    pub(crate) thumb_count: i32,
    /// 仅在请求像素时存在：`raw_width * raw_height` 行主序 u16，含填充边。
    pub(crate) pixels: Option<Vec<u16>>,
}

/// 内嵌预览的原始字节（通常是 JPEG）。
pub(crate) struct RawPreview {
    pub(crate) format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data: Vec<u8>,
}

/// RAII 句柄：无论成功失败都保证 `libraw_close`。
struct Handle(*mut lr::libraw_data_t);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: `self.0` 由 `libraw_init` 返回且非空；`libraw_close` 成对释放，
        // 关闭后不再使用该指针。
        unsafe { lr::libraw_close(self.0) };
    }
}

fn c_path(path: &Path) -> RawResult<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| RawError::InvalidPath(path.display().to_string()))
}

fn error_text(code: i32) -> String {
    // SAFETY: `libraw_strerror` 返回指向 LibRaw 内部静态字符串的指针，
    // 在进程生命周期内有效且以 NUL 结尾。
    unsafe {
        let ptr = lr::libraw_strerror(code);
        if ptr.is_null() {
            format!("错误码 {code}")
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

/// 打开文件并执行 `identify()`（`libraw_open_file` 内部完成），不 unpack。
pub(crate) fn open_metadata(path: &Path) -> RawResult<RawFile> {
    let handle = open(path)?;
    // SAFETY: `handle.0` 是 `libraw_open_file` 成功后仍然有效的句柄。
    let file = unsafe { read_metadata(handle.0) };
    Ok(file)
}

/// 打开文件、unpack 原始像素，并连同元数据一起返回。
pub(crate) fn read_pixels(path: &Path) -> RawResult<RawFile> {
    let handle = open(path)?;
    let code = {
        // SAFETY: 句柄有效；`libraw_unpack` 只解析原始像素，不做任何后处理。
        unsafe { lr::libraw_unpack(handle.0) }
    };
    if code != 0 {
        return Err(RawError::Unpack {
            code,
            message: error_text(code),
        });
    }
    // SAFETY: unpack 成功后句柄内 `rawdata` 有效，读取见函数内部 SAFETY。
    let mut file = unsafe { read_metadata(handle.0) };
    // SAFETY: 同上，读取 `rawdata.raw_image` 指向的像素缓冲。
    file.pixels = Some(unsafe { copy_raw_pixels(handle.0) }?);
    Ok(file)
}

/// 解出内嵌预览（缩略图）的原始字节；没有缩略图时返回 `None`。
pub(crate) fn read_preview(path: &Path) -> RawResult<Option<RawPreview>> {
    let handle = open(path)?;
    let code = {
        // SAFETY: 句柄有效；`libraw_unpack_thumb` 只解缩略图。
        unsafe { lr::libraw_unpack_thumb(handle.0) }
    };
    if code != 0 {
        // 有些 RAW 的缩略图不可解析，这属于“没有可用预览”，不应中断流程。
        return Ok(None);
    }
    // SAFETY: unpack_thumb 成功后 `thumbnail.thumb` 指向 `tlength` 字节的有效缓冲。
    unsafe { Ok(copy_thumbnail(handle.0)) }
}

fn open(path: &Path) -> RawResult<Handle> {
    std::fs::metadata(path)?;
    let c_path = c_path(path)?;
    // SAFETY: `libraw_init` 返回独占句柄或空指针，后续所有调用都检查空指针。
    let ptr = unsafe { lr::libraw_init(0) };
    if ptr.is_null() {
        return Err(RawError::Open {
            code: -1,
            message: "LibRaw 初始化失败（libraw_init 返回空指针）".to_string(),
        });
    }
    let handle = Handle(ptr);
    let code = {
        // SAFETY: 句柄非空，路径为合法 C 字符串（不含 NUL）。
        unsafe { lr::libraw_open_file(handle.0, c_path.as_ptr()) }
    };
    if code != 0 {
        return Err(RawError::Open {
            code,
            message: error_text(code),
        });
    }
    Ok(handle)
}

/// # Safety
/// `data` 必须是 `libraw_init`/`libraw_open_file` 成功后仍然存活的句柄。
unsafe fn read_metadata(data: *mut lr::libraw_data_t) -> RawFile {
    let sizes = if (*data).rawdata.sizes.raw_width > 0 {
        (*data).rawdata.sizes
    } else {
        (*data).sizes
    };
    let idata = &(*data).idata;
    let color = &(*data).color;
    let other = &(*data).other;

    let mut xtrans = None;
    if idata.filters == FILTERS_XTRANS {
        let mut matrix = [[0u8; 6]; 6];
        let mut any = false;
        for (row, line) in idata.xtrans.iter().enumerate() {
            for (col, value) in line.iter().enumerate() {
                let v = *value as u8;
                matrix[row][col] = v;
                any |= v != 0;
            }
        }
        if any {
            xtrans = Some(matrix);
        }
    }

    RawFile {
        width: sizes.width as u32,
        height: sizes.height as u32,
        raw_width: sizes.raw_width as u32,
        raw_height: sizes.raw_height as u32,
        top_margin: sizes.top_margin as u32,
        left_margin: sizes.left_margin as u32,
        filters: idata.filters,
        colors: idata.colors.max(0) as u32,
        xtrans,
        black: color.black,
        cblack: color.cblack.to_vec(),
        maximum: color.maximum,
        cam_mul: color.cam_mul,
        pre_mul: color.pre_mul,
        cam_xyz: color.cam_xyz,
        make: c_array_string(idata.make.as_ptr(), idata.make.len()),
        model: c_array_string(idata.model.as_ptr(), idata.model.len()),
        iso: other.iso_speed,
        shutter: other.shutter,
        aperture: other.aperture,
        focal_len: other.focal_len,
        timestamp: other.timestamp,
        thumb_format: (*data).thumbnail.tformat,
        thumb_width: (*data).thumbnail.twidth as u32,
        thumb_height: (*data).thumbnail.theight as u32,
        thumb_bytes: (*data).thumbnail.tlength,
        thumb_count: (*data).thumbs_list.thumbcount,
        pixels: None,
    }
}

/// # Safety
/// `ptr` 指向至少 `len` 字节、以 NUL 结尾的 C 字符串（LibRaw 使用定长零填充数组）。
unsafe fn c_array_string(ptr: *const std::os::raw::c_char, len: usize) -> String {
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(len);
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

/// # Safety
/// 必须在 `libraw_unpack` 成功之后调用，句柄的 `rawdata.raw_image` 指向
/// `raw_width × raw_height` 的 u16 缓冲（行间可能带 `raw_pitch` 填充）。
unsafe fn copy_raw_pixels(data: *mut lr::libraw_data_t) -> RawResult<Vec<u16>> {
    let rawdata = &(*data).rawdata;
    let raw_image = rawdata.raw_image;
    if raw_image.is_null() {
        return Err(RawError::Unsupported(
            "该文件解包后没有单通道 Bayer 原始像素（可能是浮点 DNG 或多通道格式）".to_string(),
        ));
    }
    let sizes = &(*data).sizes;
    let width = sizes.raw_width as usize;
    let height = sizes.raw_height as usize;
    if width == 0 || height == 0 {
        return Err(RawError::Unsupported(
            "LibRaw 未提供有效的原始宽高".to_string(),
        ));
    }
    let pitch = if rawdata.sizes.raw_pitch > 0 {
        rawdata.sizes.raw_pitch as usize / std::mem::size_of::<u16>()
    } else {
        width
    };

    let mut pixels = vec![0u16; width * height];
    for row in 0..height {
        // SAFETY: 第 `row` 行起点位于 raw_image 缓冲内（共 height 行、每行至少
        // max(width, pitch) 个 u16），整行 copy 不越界。
        let src = raw_image.add(row * pitch);
        std::ptr::copy_nonoverlapping(src, pixels.as_mut_ptr().add(row * width), width);
    }
    Ok(pixels)
}

/// # Safety
/// 必须在 `libraw_unpack_thumb` 成功之后调用。
unsafe fn copy_thumbnail(data: *mut lr::libraw_data_t) -> Option<RawPreview> {
    let thumb = &(*data).thumbnail;
    let format = thumb.tformat;
    if thumb.thumb.is_null() || thumb.tlength == 0 || format == 0 {
        return None;
    }
    let bytes = std::slice::from_raw_parts(thumb.thumb as *const u8, thumb.tlength as usize);
    Some(RawPreview {
        format,
        width: thumb.twidth as u32,
        height: thumb.theight as u32,
        data: bytes.to_vec(),
    })
}

/// LibRaw 缩略图格式码 → 可读名称（见 `LibRaw_thumbnail_formats`）。
pub(crate) fn preview_format_name(format: u32) -> String {
    match format {
        1 => "jpeg".to_string(),
        2 => "bitmap".to_string(),
        3 => "bitmap16".to_string(),
        4 => "layer".to_string(),
        5 => "rollei".to_string(),
        6 => "h265".to_string(),
        other => format!("unknown({other})"),
    }
}
