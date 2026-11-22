//! RAW 引擎的元数据模型（只读）。
//!
//! 所有字段都来自 LibRaw 的 `identify()` 结果（文件自身），
//! 不包含任何按机型硬编码的参数。

use serde::Serialize;

/// CFA 排列来自文件的 `filters` / `xtrans`，未知时返回 `None`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CfaInfo {
    /// 有效区域左上角像素的 2×2 排列，如 `RGGB`；X-Trans 为 `X-Trans`。
    pub pattern: String,
    /// LibRaw `imgdata.idata.filters` 原值（0 = 无 CFA，9 = X-Trans）。
    pub filters: u32,
    /// 颜色通道数（通常 3，部分机型 4）。
    pub colors: u32,
    /// X-Trans 的 6×6 排列矩阵（仅 X-Trans 机型有值）。
    pub xtrans: Option<Vec<String>>,
}

/// 黑电平：单值 + 每通道 / 每像素模式覆盖。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlackLevel {
    /// `imgdata.color.black`：整体黑电平。
    pub black: u32,
    /// `imgdata.color.cblack` 前 6 项（前 4 项为每通道，第 5/6 项为模式宽高）。
    pub cblack: Vec<u32>,
}

/// 白平衡系数（相机 as-shot）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalance {
    /// 实际生效的 as-shot 系数；`cam_mul` 全 0 时回退到 `pre_mul`。
    pub cam_mul: Vec<f32>,
    /// LibRaw 的 `cam_mul` 原值。
    pub raw_cam_mul: Vec<f32>,
    /// LibRaw 的 `pre_mul`（机型/库默认系数）。
    pub pre_mul: Vec<f32>,
    /// `cam_mul` 或 `pre_mul`。
    pub source: String,
}

/// 由文件中的 `filters` / `xtrans` 推导 CFA 描述，不做任何机型推断。
///
/// LibRaw 的 2×2 排列以有效区域左上角（`top_margin, left_margin`）为基准，
/// 因此这里按该偏移取出 4 个色号，避免裁边后相位错位。
#[cfg(feature = "raw")]
pub(crate) fn cfa_pattern(
    filters: u32,
    colors: u32,
    xtrans: Option<[[u8; 6]; 6]>,
    top_margin: u32,
    left_margin: u32,
) -> CfaInfo {
    if filters == 9 {
        let rows = xtrans.map(|matrix| {
            matrix
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|value| match value {
                            0 => 'R',
                            1 => 'G',
                            2 => 'B',
                            _ => '?',
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        });
        return CfaInfo {
            pattern: "X-Trans".to_string(),
            filters,
            colors,
            xtrans: rows,
        };
    }

    if filters == 0 {
        return CfaInfo {
            pattern: if colors <= 1 { "MONO" } else { "NONE" }.to_string(),
            filters,
            colors,
            xtrans: None,
        };
    }

    let mut pattern = String::with_capacity(4);
    for (row, col) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let r = top_margin + row;
        let c = left_margin + col;
        // 与 LibRaw 的 FC(row,col) 编码一致。
        let shift = ((r & 7) * 2 + (c & 1)) * 2;
        let color = (filters >> shift) & 3;
        pattern.push(match color {
            0 => 'R',
            1 => 'G',
            2 => 'B',
            _ => 'G',
        });
    }

    CfaInfo {
        pattern,
        filters,
        colors,
        xtrans: None,
    }
}

/// 单个 CFA 色号（0..=3），用于黑电平与通道判定。
/// X-Trans 的 6×6 排列表不做通道细分，按绿通道处理。
#[cfg(feature = "raw")]
pub(crate) fn cfa_color(filters: u32, row: u32, col: u32) -> u32 {
    if filters == 0 || filters == 9 {
        return 1;
    }
    let shift = ((row & 7) * 2 + (col & 1)) * 2;
    (filters >> shift) & 3
}

/// EXIF 摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifSummary {
    pub iso: f32,
    pub shutter: f32,
    pub aperture: f32,
    pub focal_length: f32,
    /// Unix 时间戳（秒），0 表示文件未提供。
    pub timestamp: i64,
}

/// 内嵌预览（缩略图）摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
}

/// 单个 RAW 文件的只读元数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInfo {
    pub path: String,
    pub file_size: u64,
    /// 有效图像宽高（已去掉填充边）。
    pub width: u32,
    pub height: u32,
    /// 传感器原始宽高（含填充边）。
    pub raw_width: u32,
    pub raw_height: u32,
    pub top_margin: u32,
    pub left_margin: u32,
    pub cfa: CfaInfo,
    pub black_level: BlackLevel,
    pub white_balance: WhiteBalance,
    /// 相机 → XYZ 矩阵的 3×3 部分（来自 `cam_xyz`）。
    pub camera_matrix: Vec<Vec<f32>>,
    pub make: String,
    pub model: String,
    pub exif: ExifSummary,
    /// 线性化后的白点（`color.maximum`，0 表示未知）。
    pub white_level: u32,
    pub has_embedded_preview: bool,
    pub preview: Option<PreviewInfo>,
}

impl RawInfo {
    /// 测试与日志使用的单行摘要。
    pub fn summary(&self) -> String {
        format!(
            "{} {} | {}×{} (raw {}×{}, margin {},{}) | CFA {} (filters={}, colors={}) | black {} cblack={:?} | cam_mul {:?} [{}] | ISO {} 1/{} f/{} {}mm",
            self.make.trim(),
            self.model.trim(),
            self.width,
            self.height,
            self.raw_width,
            self.raw_height,
            self.left_margin,
            self.top_margin,
            self.cfa.pattern,
            self.cfa.filters,
            self.cfa.colors,
            self.black_level.black,
            self.black_level.cblack,
            self.white_balance.cam_mul,
            self.white_balance.source,
            self.exif.iso,
            if self.exif.shutter > 0.0 {
                (1.0 / self.exif.shutter).round() as i64
            } else {
                0
            },
            self.exif.aperture,
            self.exif.focal_length,
        )
    }
}
