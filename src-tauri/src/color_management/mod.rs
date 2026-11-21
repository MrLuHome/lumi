//! 色彩管理引擎：Profile、转换、嵌入、显示与软打样。
//!
//! 工作空间固定为 sRGB 编码（保持第六轮编辑算法语义）；
//! 输入 RAW/Raster 先转换到工作空间，输出时再一次性转换到目标 Profile，
//! 不存在重复转换。

pub mod display;
pub mod embed;
pub mod lab;
pub mod profiles;
pub mod tiff16;
pub mod transform;

pub use profiles::{ColorOutputSettings, ProfileKind, RenderingIntent};

use std::sync::RwLock;

use serde::Serialize;

/// 运行期色彩设置（输出 Profile / 意图 / BPC / 嵌入）。
pub struct ColorEngine {
    settings: RwLock<ColorOutputSettings>,
}

impl Default for ColorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorEngine {
    pub fn new() -> Self {
        Self {
            settings: RwLock::new(ColorOutputSettings::default()),
        }
    }

    pub fn settings(&self) -> ColorOutputSettings {
        self.settings
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set_output(&self, settings: ColorOutputSettings) {
        if let Ok(mut guard) = self.settings.write() {
            *guard = settings;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorStatus {
    pub working_space: String,
    pub output: ColorOutputSettings,
    pub output_label: String,
    pub display: display::DisplayStatus,
    pub display_transform: bool,
    pub cms: String,
    pub profiles: Vec<ProfileOption>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOption {
    pub kind: ProfileKind,
    pub label: String,
    pub builtin: bool,
}

pub fn status(engine: &ColorEngine) -> ColorStatus {
    let output = engine.settings();
    let display = display::status();
    let display_transform = display.available;
    ColorStatus {
        working_space: "sRGB (IEC 61966-2-1)".into(),
        output_label: output.profile.label().to_string(),
        output,
        display,
        display_transform,
        cms: format!("Little CMS {}", lcms2::version() >> 16),
        profiles: ProfileKind::all()
            .iter()
            .map(|kind| ProfileOption {
                kind: *kind,
                label: kind.label().to_string(),
                builtin: *kind != ProfileKind::Custom,
            })
            .collect(),
        notes: vec![
            "工作空间为 sRGB 编码；输出 Profile 转换只发生一次。".into(),
            "ICC 嵌入格式：JPEG APP2 / PNG iCCP / TIFF 34675。".into(),
            "CMYK 无 Profile 时使用通用公式（基础质量）。".into(),
            "主画布保持 sRGB 由系统做显示管理；显示器 Profile 预览需在色彩面板手动触发，避免二次转换。".into(),
            "多显示器切换需手动刷新显示 Profile。".into(),
        ],
    }
}
