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

/// 单张人脸的 AI 美肤参数。所有字段 0..100，0 表示不做任何处理。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BeautyParams {
    /// 皮肤质感：低频平滑强度。
    pub skin_smooth: f32,
    /// 纹理保护：越大越保留毛孔与皮肤纹理。
    pub texture: f32,
    /// 细节恢复：在磨皮基础上回注高频。
    pub detail: f32,
    /// 自动祛痘。
    pub blemish: f32,
    /// 祛斑（小面积）。
    pub spots: f32,
    /// 黑头。
    pub blackheads: f32,
    /// 肤色均匀。
    pub tone_even: f32,
    /// 美白。
    pub whitening: f32,
    /// 泛红改善。
    pub redness: f32,
    /// 暗沉改善。
    pub dullness: f32,
    /// 黑眼圈淡化。
    pub dark_circles: f32,
    /// 眼下细纹。
    pub eye_lines: f32,
    /// 额头细纹。
    pub forehead_lines: f32,
    /// 法令纹。
    pub nasolabial: f32,
}

impl BeautyParams {
    pub fn fields(&self) -> [f32; 14] {
        [
            self.skin_smooth,
            self.texture,
            self.detail,
            self.blemish,
            self.spots,
            self.blackheads,
            self.tone_even,
            self.whitening,
            self.redness,
            self.dullness,
            self.dark_circles,
            self.eye_lines,
            self.forehead_lines,
            self.nasolabial,
        ]
    }

    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn is_active(&self) -> bool {
        !self.is_neutral()
    }
}

/// 单张人脸的独立美肤参数。`face_id` 由后端稳定 ID 跟踪分配，
/// 检测顺序变化不会改变已有参数归属。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceBeauty {
    pub face_id: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub beauty: BeautyParams,
}

impl FaceBeauty {
    pub fn new(face_id: u32) -> Self {
        Self {
            face_id,
            enabled: true,
            beauty: BeautyParams::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// 归一化图像坐标（0..1）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaskPoint {
    pub x: f32,
    pub y: f32,
}

/// 手动蒙版笔触：坐标按图像归一化保存，任意分辨率重建结果一致。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskStroke {
    /// true 为橡皮擦，false 为画笔。
    #[serde(default)]
    pub erase: bool,
    /// 画笔半径，按图像宽度归一化（0.005..0.25）。
    #[serde(default = "default_brush_radius")]
    pub radius: f32,
    #[serde(default)]
    pub points: Vec<MaskPoint>,
}

fn default_brush_radius() -> f32 {
    0.03
}

/// 手动蒙版：在自动皮肤蒙版之上叠加画笔区域，橡皮擦减去区域。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualMask {
    #[serde(default)]
    pub enabled: bool,
    /// 反转最终蒙版。
    #[serde(default)]
    pub invert: bool,
    /// 羽化半径，按图像宽度归一化的百分比（0..100）。
    #[serde(default = "default_feather")]
    pub feather: f32,
    /// 画笔/橡皮擦强度 0..100。
    #[serde(default = "default_opacity")]
    pub strength: f32,
    #[serde(default)]
    pub strokes: Vec<MaskStroke>,
}

fn default_feather() -> f32 {
    40.0
}

fn default_opacity() -> f32 {
    100.0
}

impl Default for ManualMask {
    fn default() -> Self {
        Self {
            enabled: false,
            invert: false,
            feather: default_feather(),
            strength: default_opacity(),
            strokes: Vec::new(),
        }
    }
}

impl ManualMask {
    pub fn has_content(&self) -> bool {
        self.enabled && !self.strokes.is_empty()
    }
}

/// 手动修复点：点击瑕疵位置，只修复该点半径内的候选瑕疵，可单独撤销。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairPoint {
    pub x: f32,
    pub y: f32,
    /// 作用半径，按图像宽度归一化。
    #[serde(default = "default_brush_radius")]
    pub radius: f32,
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
    /// 第二轮旧字段：作为所有已启用人脸的额外磨皮强度保留兼容。
    #[serde(default)]
    pub skin_smooth: f32,
    /// 按稳定人脸 ID 保存的独立美肤参数。
    pub faces: Vec<FaceBeauty>,
    pub manual_mask: ManualMask,
    /// 手动点选的局部修复点（可撤销、可再编辑）。
    pub repair_points: Vec<RepairPoint>,
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
            faces: Vec::new(),
            manual_mask: ManualMask::default(),
            repair_points: Vec::new(),
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
        self.faces.retain(|face| face.beauty.is_active() || face.enabled);
        for face in self.faces.iter_mut() {
            for value in [
                &mut face.beauty.skin_smooth,
                &mut face.beauty.texture,
                &mut face.beauty.detail,
                &mut face.beauty.blemish,
                &mut face.beauty.spots,
                &mut face.beauty.blackheads,
                &mut face.beauty.tone_even,
                &mut face.beauty.whitening,
                &mut face.beauty.redness,
                &mut face.beauty.dullness,
                &mut face.beauty.dark_circles,
                &mut face.beauty.eye_lines,
                &mut face.beauty.forehead_lines,
                &mut face.beauty.nasolabial,
            ] {
                *value = value.clamp(0.0, 100.0);
            }
        }
        self.faces.sort_by_key(|face| face.face_id);
        self.manual_mask.feather = self.manual_mask.feather.clamp(0.0, 100.0);
        self.manual_mask.strength = self.manual_mask.strength.clamp(0.0, 100.0);
        for stroke in self.manual_mask.strokes.iter_mut() {
            stroke.radius = stroke.radius.clamp(0.002, 0.25);
            stroke.points.retain(|point| point.x.is_finite() && point.y.is_finite());
            for point in stroke.points.iter_mut() {
                point.x = point.x.clamp(0.0, 1.0);
                point.y = point.y.clamp(0.0, 1.0);
            }
        }
        self.manual_mask.strokes.retain(|stroke| !stroke.points.is_empty());
        self.repair_points.retain(|point| point.x.is_finite() && point.y.is_finite());
        self.repair_points.truncate(200);
        for point in self.repair_points.iter_mut() {
            point.x = point.x.clamp(0.0, 1.0);
            point.y = point.y.clamp(0.0, 1.0);
            point.radius = point.radius.clamp(0.004, 0.25);
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
            && clamped.faces.iter().all(|face| !face.enabled || face.beauty.is_neutral())
            && clamped.hsl.is_neutral()
            && clamped.curves.is_neutral()
            && clamped.lut.is_none()
    }

    /// 找到某张脸的参数；不存在时返回默认值（不修改自身）。
    pub fn face_beauty(&self, face_id: u32) -> Option<&FaceBeauty> {
        self.faces.iter().find(|face| face.face_id == face_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beauty_params_clamp_and_detect_activity() {
        let mut params = EditParams::default();
        params.faces.push(FaceBeauty {
            face_id: 3,
            enabled: true,
            beauty: BeautyParams {
                skin_smooth: 150.0,
                texture: -20.0,
                ..Default::default()
            },
        });
        let clamped = params.clone().clamped();
        assert_eq!(clamped.faces[0].beauty.skin_smooth, 100.0);
        assert_eq!(clamped.faces[0].beauty.texture, 0.0);
        assert!(!params.is_neutral());
    }

    #[test]
    fn manual_mask_normalizes_strokes_and_points() {
        let mut mask = ManualMask {
            enabled: true,
            invert: false,
            feather: 300.0,
            strength: 130.0,
            strokes: vec![MaskStroke {
                erase: false,
                radius: 0.9,
                points: vec![MaskPoint { x: -1.0, y: 2.0 }],
            }],
        };
        let params = EditParams {
            manual_mask: std::mem::take(&mut mask),
            ..Default::default()
        };
        let clamped = params.clamped();
        assert_eq!(clamped.manual_mask.feather, 100.0);
        assert_eq!(clamped.manual_mask.strength, 100.0);
        assert_eq!(clamped.manual_mask.strokes[0].radius, 0.25);
        assert_eq!(clamped.manual_mask.strokes[0].points[0].x, 0.0);
        assert_eq!(clamped.manual_mask.strokes[0].points[0].y, 1.0);
    }

    #[test]
    fn face_entries_are_sorted_by_stable_id() {
        let params = EditParams {
            faces: vec![FaceBeauty::new(5), FaceBeauty::new(1)],
            ..Default::default()
        }
        .clamped();
        assert_eq!(params.faces[0].face_id, 1);
        assert_eq!(params.faces[1].face_id, 5);
        assert!(params.is_neutral());
    }
}
