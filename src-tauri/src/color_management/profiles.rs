//! ICC Profile 的创建与加载。
//!
//! 内置 sRGB / Adobe RGB / Display P3 / ProPhoto，并支持用户 ICC 文件。
//! 所有内置 Profile 都由 Little CMS 按标准原色与传递函数生成，
//! 不是“写个名字”的假 Profile。

use std::path::{Path, PathBuf};

use lcms2::{CIExyY, CIExyYTRIPLE, Intent, Profile, ToneCurve};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// 输出 Profile 种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProfileKind {
    #[default]
    Srgb,
    AdobeRgb,
    DisplayP3,
    ProPhoto,
    Custom,
}

impl ProfileKind {
    pub fn label(self) -> &'static str {
        match self {
            ProfileKind::Srgb => "sRGB",
            ProfileKind::AdobeRgb => "Adobe RGB (1998)",
            ProfileKind::DisplayP3 => "Display P3",
            ProfileKind::ProPhoto => "ProPhoto RGB",
            ProfileKind::Custom => "自定义 ICC",
        }
    }

    pub fn all() -> [ProfileKind; 5] {
        [
            ProfileKind::Srgb,
            ProfileKind::AdobeRgb,
            ProfileKind::DisplayP3,
            ProfileKind::ProPhoto,
            ProfileKind::Custom,
        ]
    }
}

/// 渲染意图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RenderingIntent {
    Perceptual,
    #[default]
    Relative,
    Saturation,
    Absolute,
}

impl RenderingIntent {
    pub fn to_lcms(self) -> Intent {
        match self {
            RenderingIntent::Perceptual => Intent::Perceptual,
            RenderingIntent::Relative => Intent::RelativeColorimetric,
            RenderingIntent::Saturation => Intent::Saturation,
            RenderingIntent::Absolute => Intent::AbsoluteColorimetric,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RenderingIntent::Perceptual => "感知",
            RenderingIntent::Relative => "相对比色",
            RenderingIntent::Saturation => "饱和度",
            RenderingIntent::Absolute => "绝对比色",
        }
    }
}

/// 输出色彩设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorOutputSettings {
    pub profile: ProfileKind,
    /// `ProfileKind::Custom` 时必填。
    pub custom_path: Option<String>,
    pub intent: RenderingIntent,
    /// 黑点补偿。
    pub black_point_compensation: bool,
    /// 是否把 Profile 嵌入输出文件。
    pub embed: bool,
}

impl Default for ColorOutputSettings {
    fn default() -> Self {
        Self {
            profile: ProfileKind::Srgb,
            custom_path: None,
            intent: RenderingIntent::Relative,
            black_point_compensation: true,
            embed: true,
        }
    }
}

/// 按设置加载一个可用的 ICC Profile。
pub fn load(settings: &ColorOutputSettings) -> AppResult<Profile> {
    match settings.profile {
        ProfileKind::Srgb => Ok(Profile::new_srgb()),
        ProfileKind::AdobeRgb => adobe_rgb(),
        ProfileKind::DisplayP3 => display_p3(),
        ProfileKind::ProPhoto => prophoto(),
        ProfileKind::Custom => {
            let path = settings
                .custom_path
                .as_ref()
                .ok_or_else(|| AppError::Unsupported("自定义 ICC 未指定文件路径".into()))?;
            load_file(Path::new(path))
        }
    }
}

pub fn load_file(path: &Path) -> AppResult<Profile> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err(AppError::Unsupported("ICC 文件超过 32MB，已拒绝".into()));
    }
    Profile::new_file(path).map_err(|error| AppError::Decode(format!("读取 ICC 失败：{error}")))
}

pub fn from_bytes(bytes: &[u8]) -> AppResult<Profile> {
    if bytes.len() < 128 || bytes.len() > 32 * 1024 * 1024 {
        return Err(AppError::Decode("ICC 数据长度异常".into()));
    }
    Profile::new_icc(bytes).map_err(|error| AppError::Decode(format!("ICC 解析失败：{error}")))
}

/// Profile 的原始 ICC 字节，用于嵌入与回读校验。
pub fn icc_bytes(profile: &Profile) -> AppResult<Vec<u8>> {
    profile
        .icc()
        .map_err(|error| AppError::Task(format!("导出 ICC 数据失败：{error}")))
}

fn d65() -> CIExyY {
    CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    }
}

fn d50() -> CIExyY {
    CIExyY {
        x: 0.3457,
        y: 0.3585,
        Y: 1.0,
    }
}

fn gamma(value: f64) -> ToneCurve {
    ToneCurve::new(value)
}

fn srgb_curve() -> ToneCurve {
    // ICC parametric type 4: Y=(aX+b)^g，X<d 时 Y=cX。
    ToneCurve::new_parametric(4, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045])
        .unwrap_or_else(|_| ToneCurve::new(2.2))
}

fn adobe_rgb() -> AppResult<Profile> {
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.64,
            y: 0.33,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.21,
            y: 0.71,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.15,
            y: 0.06,
            Y: 1.0,
        },
    };
    let curve = gamma(2.2);
    Profile::new_rgb(&d65(), &primaries, &[&curve, &curve, &curve])
        .map_err(|error| AppError::Task(format!("创建 Adobe RGB Profile 失败：{error}")))
}

fn display_p3() -> AppResult<Profile> {
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.68,
            y: 0.32,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.265,
            y: 0.69,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.15,
            y: 0.06,
            Y: 1.0,
        },
    };
    let curve = srgb_curve();
    Profile::new_rgb(&d65(), &primaries, &[&curve, &curve, &curve])
        .map_err(|error| AppError::Task(format!("创建 Display P3 Profile 失败：{error}")))
}

fn prophoto() -> AppResult<Profile> {
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.7347,
            y: 0.2653,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.1596,
            y: 0.8404,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.0366,
            y: 0.0001,
            Y: 1.0,
        },
    };
    let curve = gamma(1.8);
    Profile::new_rgb(&d50(), &primaries, &[&curve, &curve, &curve])
        .map_err(|error| AppError::Task(format!("创建 ProPhoto Profile 失败：{error}")))
}

/// 内置 Profile 的字节缓存（供列表与嵌入使用）。
pub fn builtin_bytes(kind: ProfileKind) -> AppResult<Vec<u8>> {
    if kind == ProfileKind::Custom {
        return Err(AppError::Unsupported("自定义 ICC 需要提供文件路径".into()));
    }
    let slot = cache_slot(kind);
    if let Some(bytes) = slot.get() {
        return Ok(bytes.clone());
    }
    let profile = load(&ColorOutputSettings {
        profile: kind,
        ..Default::default()
    })?;
    let bytes = icc_bytes(&profile)?;
    Ok(slot.get_or_init(|| bytes).clone())
}

/// 内置 Profile 的进程内缓存。ICC 头部包含生成时间，缓存可保证同一次会话内
/// 单张与批量导出得到完全相同的字节。
fn cache_slot(kind: ProfileKind) -> &'static std::sync::OnceLock<Vec<u8>> {
    static SRGB: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    static ADOBE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    static P3: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    static PROPHOTO: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    match kind {
        ProfileKind::Srgb => &SRGB,
        ProfileKind::AdobeRgb => &ADOBE,
        ProfileKind::DisplayP3 => &P3,
        ProfileKind::ProPhoto => &PROPHOTO,
        ProfileKind::Custom => &SRGB,
    }
}

/// 输出设置对应的嵌入 ICC 字节。内置 Profile 走缓存，用户 Profile 实时读取。
pub fn embedding_bytes(settings: &ColorOutputSettings) -> AppResult<Vec<u8>> {
    if settings.profile == ProfileKind::Custom {
        let profile = load(settings)?;
        icc_bytes(&profile)
    } else {
        builtin_bytes(settings.profile)
    }
}

/// 用户 ICC 文件校验：能否被 Little CMS 打开，以及色彩空间。
pub fn describe_file(path: &Path) -> AppResult<ProfileSummary> {
    let profile = load_file(path)?;
    Ok(ProfileSummary {
        path: path.display().to_string(),
        description: profile
            .info(lcms2::InfoType::Description, lcms2::Locale::none())
            .unwrap_or_else(|| file_name(path)),
        color_space: format!("{:?}", profile.color_space()),
        channels: lcms2::ColorSpaceSignatureExt::channels(profile.color_space()),
        version: profile.version(),
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub path: String,
    pub description: String,
    pub color_space: String,
    pub channels: u32,
    pub version: f64,
}

/// 自定义 Profile 的默认搜索目录（仅用于 UI 提示，不自动加载）。
pub fn custom_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        for name in ["Library/ColorSync/Profiles", "Documents"] {
            let dir = PathBuf::from(&home).join(name);
            if dir.exists() {
                out.push(dir);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_have_valid_icc_bytes() {
        for kind in ProfileKind::all() {
            if kind == ProfileKind::Custom {
                continue;
            }
            let bytes = builtin_bytes(kind).unwrap();
            assert!(bytes.len() > 128, "{:?} 的 ICC 数据过短", kind);
            assert_eq!(&bytes[36..40], b"acsp", "{:?} 不是 ICC Profile", kind);
            // 能被重新解析。
            let profile = from_bytes(&bytes).unwrap();
            assert!(profile.version() >= 2.0);
        }
    }

    #[test]
    fn custom_without_path_is_rejected() {
        let settings = ColorOutputSettings {
            profile: ProfileKind::Custom,
            ..Default::default()
        };
        assert!(load(&settings).is_err());
    }

    #[test]
    fn corrupt_and_oversized_icc_are_rejected() {
        assert!(from_bytes(&[0u8; 16]).is_err(), "过短数据必须拒绝");
        assert!(from_bytes(&[0u8; 256]).is_err(), "损坏数据必须拒绝");
        assert!(
            from_bytes(&vec![0u8; 33 * 1024 * 1024]).is_err(),
            "超过 32MB 必须拒绝"
        );
    }

    #[test]
    fn v2_and_v4_profiles_are_both_accepted() {
        for version in [2.1f64, 4.4] {
            let mut profile = adobe_rgb().unwrap();
            profile.set_version(version);
            let bytes = profile.icc().unwrap();
            let parsed = from_bytes(&bytes).unwrap();
            assert!(parsed.version() >= 2.0);
        }
    }
}
