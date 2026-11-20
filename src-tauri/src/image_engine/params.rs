use serde::{Deserialize, Serialize};

/// HSL 八色通道。色相偏移单位为度（-30..30 由 -100..100 映射），
/// 饱和度与明度是百分比增益（-100..100）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HslChannel {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HslParams {
    pub red: HslChannel,
    pub orange: HslChannel,
    pub yellow: HslChannel,
    pub green: HslChannel,
    pub aqua: HslChannel,
    pub blue: HslChannel,
    pub purple: HslChannel,
    pub magenta: HslChannel,
}

impl HslParams {
    pub const CHANNELS: [(&'static str, f32); 8] = [
        ("red", 0.0),
        ("orange", 30.0),
        ("yellow", 60.0),
        ("green", 120.0),
        ("aqua", 180.0),
        ("blue", 240.0),
        ("purple", 280.0),
        ("magenta", 330.0),
    ];

    pub fn is_neutral(&self) -> bool {
        self.channels().all(|channel| {
            channel.hue == 0.0 && channel.saturation == 0.0 && channel.luminance == 0.0
        })
    }

    pub fn channel_mut(&mut self, name: &str) -> Option<&mut HslChannel> {
        Some(match name {
            "red" => &mut self.red,
            "orange" => &mut self.orange,
            "yellow" => &mut self.yellow,
            "green" => &mut self.green,
            "aqua" => &mut self.aqua,
            "blue" => &mut self.blue,
            "purple" => &mut self.purple,
            "magenta" => &mut self.magenta,
            _ => return None,
        })
    }

    pub fn channels(&self) -> impl Iterator<Item = &HslChannel> {
        [
            &self.red,
            &self.orange,
            &self.yellow,
            &self.green,
            &self.aqua,
            &self.blue,
            &self.purple,
            &self.magenta,
        ]
        .into_iter()
    }
}

/// 曲线上的一个控制点，坐标归一化到 0..1。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

impl CurvePoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CurveSet {
    /// 主 RGB 曲线。
    pub rgb: Vec<CurvePoint>,
    pub red: Vec<CurvePoint>,
    pub green: Vec<CurvePoint>,
    pub blue: Vec<CurvePoint>,
}

impl CurveSet {
    pub fn is_neutral(&self) -> bool {
        self.rgb.is_empty() && self.red.is_empty() && self.green.is_empty() && self.blue.is_empty()
    }
}

/// 基础 3D LUT（.cube 文件）。路径由用户显式选择，跟着项目保存。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LutSettings {
    pub path: String,
    pub name: String,
    /// 0..100，100 表示完全套用 LUT。
    pub strength: f32,
}

/// 统一编辑参数。前后端共用同一套字段名（camelCase），
/// 滑杆、撤销快照、Rust 渲染管线、项目文件都直接使用这个结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditParams {
    /// 曝光，-100..100
    pub exposure: f32,
    /// 对比度，-100..100
    pub contrast: f32,
    /// 高光，-100..100
    pub highlights: f32,
    /// 阴影，-100..100
    pub shadows: f32,
    /// 白色色阶，-100..100
    pub whites: f32,
    /// 黑色色阶，-100..100
    pub blacks: f32,
    /// 色温，-100..100（负冷正暖）
    pub temperature: f32,
    /// 色调，-100..100（负绿正品红）
    pub tint: f32,
    /// 饱和度，-100..100
    pub saturation: f32,
    /// 自然饱和度，-100..100
    pub vibrance: f32,
    /// 清晰度（大半径局部对比），0..100
    pub clarity: f32,
    /// 锐化，0..100
    pub sharpness: f32,
    /// 颗粒，0..100
    pub grain: f32,
    /// 暗角，0..100
    pub vignette: f32,
    /// 磨皮强度，0..100
    pub skin_smooth: f32,
    pub hsl: HslParams,
    pub curves: CurveSet,
    pub lut: Option<LutSettings>,
}

impl Default for EditParams {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            tint: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            clarity: 0.0,
            sharpness: 0.0,
            grain: 0.0,
            vignette: 0.0,
            skin_smooth: 0.0,
            hsl: HslParams::default(),
            curves: CurveSet::default(),
            lut: None,
        }
    }
}

impl EditParams {
    /// 把越界值收敛到 UI 允许的区间，防止前端异常数据把管线算到无效区间。
    pub fn clamped(mut self) -> Self {
        for value in [
            &mut self.exposure,
            &mut self.contrast,
            &mut self.highlights,
            &mut self.shadows,
            &mut self.whites,
            &mut self.blacks,
            &mut self.temperature,
            &mut self.tint,
            &mut self.saturation,
            &mut self.vibrance,
        ] {
            *value = value.clamp(-100.0, 100.0);
        }
        for value in [
            &mut self.clarity,
            &mut self.sharpness,
            &mut self.grain,
            &mut self.vignette,
            &mut self.skin_smooth,
        ] {
            *value = value.clamp(0.0, 100.0);
        }
        for channel in [
            &mut self.hsl.red,
            &mut self.hsl.orange,
            &mut self.hsl.yellow,
            &mut self.hsl.green,
            &mut self.hsl.aqua,
            &mut self.hsl.blue,
            &mut self.hsl.purple,
            &mut self.hsl.magenta,
        ] {
            channel.hue = channel.hue.clamp(-100.0, 100.0);
            channel.saturation = channel.saturation.clamp(-100.0, 100.0);
            channel.luminance = channel.luminance.clamp(-100.0, 100.0);
        }
        for curve in [
            &mut self.curves.rgb,
            &mut self.curves.red,
            &mut self.curves.green,
            &mut self.curves.blue,
        ] {
            curve.retain(|point| point.x.is_finite() && point.y.is_finite());
            for point in curve.iter_mut() {
                point.x = point.x.clamp(0.0, 1.0);
                point.y = point.y.clamp(0.0, 1.0);
            }
            curve.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        }
        if let Some(lut) = &mut self.lut {
            lut.strength = lut.strength.clamp(0.0, 100.0);
            if lut.path.trim().is_empty() {
                self.lut = None;
            }
        }
        self
    }

    pub fn is_neutral(&self) -> bool {
        let clamped = self.clone().clamped();
        clamped.exposure == 0.0
            && clamped.contrast == 0.0
            && clamped.highlights == 0.0
            && clamped.shadows == 0.0
            && clamped.whites == 0.0
            && clamped.blacks == 0.0
            && clamped.temperature == 0.0
            && clamped.tint == 0.0
            && clamped.saturation == 0.0
            && clamped.vibrance == 0.0
            && clamped.clarity == 0.0
            && clamped.sharpness == 0.0
            && clamped.grain == 0.0
            && clamped.vignette == 0.0
            && clamped.skin_smooth == 0.0
            && clamped.hsl.is_neutral()
            && clamped.curves.is_neutral()
            && clamped.lut.is_none()
    }
}
