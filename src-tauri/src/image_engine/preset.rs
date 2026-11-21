use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ai_engine::face_frame::FaceFrame;
use crate::image_engine::params::{
    BeautyParams, CurvePoint, CurveSet, EditParams, FaceBeauty, HslChannel,
};

/// 预设结构版本。字段或语义变化时递增，项目快照与导入文件都依赖它。
pub const PRESET_SCHEMA_VERSION: u32 = 1;
pub const BUILTIN_PRESET_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetCategory {
    PortraitNatural,
    SkinTone,
    Color,
    Mood,
    Custom,
}

impl PresetCategory {
    pub fn label(self) -> &'static str {
        match self {
            PresetCategory::PortraitNatural => "人像自然",
            PresetCategory::SkinTone => "肤色",
            PresetCategory::Color => "色彩",
            PresetCategory::Mood => "氛围",
            PresetCategory::Custom => "我的预设",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PresetCategory::PortraitNatural => "portraitNatural",
            PresetCategory::SkinTone => "skinTone",
            PresetCategory::Color => "color",
            PresetCategory::Mood => "mood",
            PresetCategory::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetSource {
    Builtin,
    Custom,
}

/// 预设只保存“可调色参数子集”。这里刻意没有任何对象级字段：
/// 不保存人脸 UUID、蒙版笔触、变形锚点、背景图片路径、原图路径或缓存。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PresetParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadows: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whites: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blacks: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibrance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vignette: Option<f32>,
    pub hsl: BTreeMap<String, HslChannel>,
    #[serde(skip_serializing_if = "CurveSet::is_neutral")]
    pub curves: CurveSet,
    /// 只保存 LUT 名称，绝不保存绝对路径；应用时要求当前照片选择同名 LUT。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lut_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lut_strength: Option<f32>,
    /// 美肤模板：字段名到 0..100 的目标值，不含任何人脸身份信息。
    pub beauty: BTreeMap<String, f32>,
}

impl PresetParams {
    pub fn is_empty(&self) -> bool {
        self.scalar_list().iter().all(|(_, value)| value.is_none())
            && self.hsl.is_empty()
            && self.curves.is_neutral()
            && self.lut_name.is_none()
            && self.beauty.is_empty()
    }

    fn scalar_list(&self) -> [(&'static str, Option<f32>); 14] {
        [
            ("exposure", self.exposure),
            ("contrast", self.contrast),
            ("highlights", self.highlights),
            ("shadows", self.shadows),
            ("whites", self.whites),
            ("blacks", self.blacks),
            ("temperature", self.temperature),
            ("tint", self.tint),
            ("saturation", self.saturation),
            ("vibrance", self.vibrance),
            ("clarity", self.clarity),
            ("sharpness", self.sharpness),
            ("grain", self.grain),
            ("vignette", self.vignette),
        ]
    }

    /// 从当前照片提取预设：只抓非中性标量、非中性 HSL、非空曲线、LUT 名称与美肤值。
    pub fn from_edit_params(params: &EditParams) -> Self {
        let scalar = |value: f32| if value == 0.0 { None } else { Some(value) };
        let mut hsl = BTreeMap::new();
        for (name, channel) in [
            ("red", params.hsl.red),
            ("orange", params.hsl.orange),
            ("yellow", params.hsl.yellow),
            ("green", params.hsl.green),
            ("aqua", params.hsl.aqua),
            ("blue", params.hsl.blue),
            ("purple", params.hsl.purple),
            ("magenta", params.hsl.magenta),
        ] {
            if channel != HslChannel::default() {
                hsl.insert(name.to_string(), channel);
            }
        }
        // 美肤模板取所有启用人脸同一字段的最大值，不携带任何身份信息。
        let mut beauty = BTreeMap::new();
        let mut fields = BeautyParams::default().fields();
        let names = [
            "skin_smooth",
            "texture",
            "detail",
            "blemish",
            "spots",
            "blackheads",
            "tone_even",
            "whitening",
            "redness",
            "dullness",
            "dark_circles",
            "eye_lines",
            "forehead_lines",
            "nasolabial",
        ];
        for face in params.faces.iter().filter(|face| face.enabled) {
            for (index, value) in face.beauty.fields().iter().enumerate() {
                if *value > fields[index] {
                    fields[index] = *value;
                }
            }
        }
        for (index, value) in fields.iter().enumerate() {
            if *value > 0.0 {
                beauty.insert(names[index].to_string(), *value);
            }
        }
        Self {
            exposure: scalar(params.exposure),
            contrast: scalar(params.contrast),
            highlights: scalar(params.highlights),
            shadows: scalar(params.shadows),
            whites: scalar(params.whites),
            blacks: scalar(params.blacks),
            temperature: scalar(params.temperature),
            tint: scalar(params.tint),
            saturation: scalar(params.saturation),
            vibrance: scalar(params.vibrance),
            clarity: scalar(params.clarity),
            sharpness: scalar(params.sharpness),
            grain: scalar(params.grain),
            vignette: scalar(params.vignette),
            hsl,
            curves: params.curves.clone(),
            lut_name: params.lut.as_ref().map(|lut| lut.name.clone()),
            lut_strength: params.lut.as_ref().map(|lut| lut.strength),
            beauty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchPreset {
    pub id: String,
    pub name: String,
    pub category: PresetCategory,
    pub description: String,
    pub version: String,
    pub source: PresetSource,
    pub params: PresetParams,
    /// 适用场景，展示给用户看。
    #[serde(default)]
    pub suitable_for: String,
    /// 不适用的场景。预设不能承诺所有照片都好看。
    #[serde(default)]
    pub avoid: String,
    /// 真实渲染缩略图（data URL），由预检命令按当前照片生成，不存盘。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

fn preset(
    id: &str,
    name: &str,
    category: PresetCategory,
    description: &str,
    suitable_for: &str,
    avoid: &str,
    params: PresetParams,
) -> RetouchPreset {
    RetouchPreset {
        id: id.to_string(),
        name: name.to_string(),
        category,
        description: description.to_string(),
        version: BUILTIN_PRESET_VERSION.to_string(),
        source: PresetSource::Builtin,
        params,
        suitable_for: suitable_for.to_string(),
        avoid: avoid.to_string(),
        preview: None,
    }
}

fn hsl(channels: &[(&str, f32, f32, f32)]) -> BTreeMap<String, HslChannel> {
    channels
        .iter()
        .map(|(name, hue, saturation, luminance)| {
            (
                name.to_string(),
                HslChannel {
                    hue: *hue,
                    saturation: *saturation,
                    luminance: *luminance,
                },
            )
        })
        .collect()
}

fn beauty(values: &[(&str, f32)]) -> BTreeMap<String, f32> {
    values
        .iter()
        .map(|(name, value)| (name.to_string(), *value))
        .collect()
}

fn curve(points: &[(f32, f32)]) -> CurveSet {
    CurveSet {
        rgb: points
            .iter()
            .map(|(x, y)| CurvePoint::new(*x, *y))
            .collect(),
        ..Default::default()
    }
}

/// 内置预设：14 个，参数经过人工设计与数值校验，互不相同。
pub fn builtin_presets() -> Vec<RetouchPreset> {
    vec![
        preset(
            "builtin-natural-portrait",
            "自然通透",
            PresetCategory::PortraitNatural,
            "轻微提亮加一点点阴影恢复，保留原本肤色与质感。",
            "日常人像、室内外自然光",
            "极暗或严重偏色的原片",
            PresetParams {
                exposure: Some(4.0),
                contrast: Some(6.0),
                highlights: Some(-8.0),
                shadows: Some(10.0),
                vibrance: Some(8.0),
                clarity: Some(4.0),
                sharpness: Some(12.0),
                curves: curve(&[(0.0, 0.0), (0.25, 0.27), (0.75, 0.76), (1.0, 1.0)]),
                beauty: beauty(&[("skin_smooth", 12.0), ("tone_even", 8.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-portrait-soft",
            "柔光人像",
            PresetCategory::PortraitNatural,
            "压低高光、抬起阴影，肤色柔和但保留轮廓。",
            "顺光、窗光人像",
            "需要强烈立体感的硬光棚拍",
            PresetParams {
                exposure: Some(2.0),
                contrast: Some(-4.0),
                highlights: Some(-18.0),
                shadows: Some(16.0),
                whites: Some(-6.0),
                temperature: Some(3.0),
                vibrance: Some(5.0),
                clarity: Some(2.0),
                grain: Some(4.0),
                vignette: Some(8.0),
                beauty: beauty(&[
                    ("skin_smooth", 20.0),
                    ("texture", 14.0),
                    ("tone_even", 12.0),
                ]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-portrait-strong",
            "精修人像",
            PresetCategory::PortraitNatural,
            "更明确的对比与美肤模板，适合交付级人像。",
            "单人肖像、证件照之外的人像交付",
            "纪实、新闻类不允许修饰的场景",
            PresetParams {
                exposure: Some(3.0),
                contrast: Some(14.0),
                highlights: Some(-22.0),
                shadows: Some(12.0),
                whites: Some(-4.0),
                blacks: Some(-6.0),
                vibrance: Some(10.0),
                clarity: Some(8.0),
                sharpness: Some(18.0),
                vignette: Some(10.0),
                curves: curve(&[(0.0, 0.0), (0.3, 0.29), (0.7, 0.72), (1.0, 1.0)]),
                beauty: beauty(&[
                    ("skin_smooth", 24.0),
                    ("texture", 18.0),
                    ("tone_even", 16.0),
                    ("blemish", 12.0),
                    ("dark_circles", 10.0),
                ]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-clean-skin",
            "干净肤色",
            PresetCategory::SkinTone,
            "中性白平衡与低饱和调整，让肤色回归干净。",
            "办公室灯光、偏黄环境光下的人像",
            "已经有强烈风格化的照片",
            PresetParams {
                temperature: Some(-6.0),
                tint: Some(2.0),
                contrast: Some(4.0),
                saturation: Some(-4.0),
                vibrance: Some(4.0),
                hsl: hsl(&[("orange", 0.0, -6.0, 3.0), ("red", 0.0, -3.0, 2.0)]),
                beauty: beauty(&[("tone_even", 18.0), ("dullness", 12.0), ("redness", 10.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-warm-skin",
            "暖调肤色",
            PresetCategory::SkinTone,
            "略暖的色温与橙色通道提亮，肤色更有血色。",
            "秋冬、日落前后的人像",
            "白平衡本就偏暖的室内照片",
            PresetParams {
                temperature: Some(12.0),
                tint: Some(4.0),
                exposure: Some(2.0),
                saturation: Some(6.0),
                vibrance: Some(10.0),
                hsl: hsl(&[("orange", 2.0, 8.0, 6.0), ("red", 0.0, 6.0, 3.0)]),
                beauty: beauty(&[("whitening", 6.0), ("redness", 8.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-cool-skin",
            "冷白肤色",
            PresetCategory::SkinTone,
            "降低色温、轻微提亮，呈现清冷肤色。",
            "室内白炽灯、偏暖环境",
            "需要保留暖调氛围的日落照片",
            PresetParams {
                temperature: Some(-12.0),
                tint: Some(-2.0),
                exposure: Some(3.0),
                contrast: Some(5.0),
                vibrance: Some(6.0),
                hsl: hsl(&[("blue", 0.0, 6.0, 4.0), ("orange", 0.0, -4.0, 5.0)]),
                beauty: beauty(&[("whitening", 12.0), ("tone_even", 10.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-cinematic-teal",
            "青橙电影",
            PresetCategory::Color,
            "暗部偏青、肤色偏橙的经典电影配色。",
            "城市夜景、有明确肤色主体的人像",
            "大面积绿色或纯白背景",
            PresetParams {
                contrast: Some(16.0),
                highlights: Some(-14.0),
                shadows: Some(8.0),
                blacks: Some(-8.0),
                saturation: Some(-6.0),
                vibrance: Some(12.0),
                hsl: hsl(&[
                    ("aqua", -8.0, 14.0, -4.0),
                    ("blue", -6.0, 10.0, -6.0),
                    ("orange", 4.0, 10.0, 2.0),
                ]),
                curves: curve(&[(0.0, 0.03), (0.28, 0.25), (0.72, 0.76), (1.0, 0.98)]),
                vignette: Some(16.0),
                ..Default::default()
            },
        ),
        preset(
            "builtin-urban-cool",
            "城市冷灰",
            PresetCategory::Color,
            "整体降低饱和、偏冷灰调，适合街头与建筑。",
            "街头人像、阴天、水泥质感场景",
            "暖色调食物与秋叶",
            PresetParams {
                temperature: Some(-8.0),
                contrast: Some(12.0),
                highlights: Some(-10.0),
                shadows: Some(6.0),
                saturation: Some(-14.0),
                vibrance: Some(6.0),
                clarity: Some(10.0),
                grain: Some(7.0),
                hsl: hsl(&[("blue", 0.0, 8.0, -4.0), ("yellow", 0.0, -10.0, 0.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-classic-bw",
            "经典黑白",
            PresetCategory::Color,
            "去色后提高对比，并加入少量颗粒模拟胶片层次。",
            "结构感强的人像、建筑与纪实",
            "需要保留服装或环境色彩信息",
            PresetParams {
                saturation: Some(-100.0),
                contrast: Some(18.0),
                highlights: Some(-8.0),
                shadows: Some(10.0),
                whites: Some(6.0),
                blacks: Some(-10.0),
                grain: Some(14.0),
                vignette: Some(12.0),
                curves: curve(&[(0.0, 0.01), (0.5, 0.53), (1.0, 0.99)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-vivid-pop",
            "鲜明色彩",
            PresetCategory::Color,
            "提升自然饱和度与清晰度，画面更有冲击力。",
            "旅行、户外、色彩丰富的场景",
            "肤色已经很红或高饱和的原始素材",
            PresetParams {
                exposure: Some(2.0),
                contrast: Some(12.0),
                saturation: Some(10.0),
                vibrance: Some(22.0),
                clarity: Some(12.0),
                sharpness: Some(20.0),
                hsl: hsl(&[("green", 0.0, 12.0, 2.0), ("aqua", 0.0, 10.0, 0.0)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-film-soft",
            "柔胶片",
            PresetCategory::Mood,
            "低对比、轻颗粒、高光柔化，接近负片观感。",
            "日常记录、室内弱光人像",
            "需要锐利商业质感的照片",
            PresetParams {
                exposure: Some(2.0),
                contrast: Some(-8.0),
                highlights: Some(-16.0),
                shadows: Some(14.0),
                blacks: Some(6.0),
                saturation: Some(-8.0),
                vibrance: Some(6.0),
                grain: Some(16.0),
                vignette: Some(10.0),
                curves: curve(&[(0.0, 0.05), (0.3, 0.31), (0.7, 0.71), (1.0, 0.95)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-vintage",
            "复古褪色",
            PresetCategory::Mood,
            "抬黑、压低高光并偏暖，做旧照片的褪色感。",
            "怀旧主题、老镜头素材",
            "现代商业广告与高保真记录",
            PresetParams {
                contrast: Some(-10.0),
                highlights: Some(-12.0),
                shadows: Some(18.0),
                blacks: Some(14.0),
                temperature: Some(10.0),
                tint: Some(4.0),
                saturation: Some(-14.0),
                grain: Some(20.0),
                vignette: Some(22.0),
                curves: curve(&[(0.0, 0.12), (0.4, 0.45), (0.8, 0.78), (1.0, 0.92)]),
                ..Default::default()
            },
        ),
        preset(
            "builtin-sunset",
            "日落暖光",
            PresetCategory::Mood,
            "加强暖色与高光，模拟黄金时刻的光线方向感。",
            "傍晚、逆光、海邊人像",
            "正午强光或已经偏橙的照片",
            PresetParams {
                exposure: Some(3.0),
                contrast: Some(10.0),
                highlights: Some(-10.0),
                shadows: Some(6.0),
                temperature: Some(18.0),
                tint: Some(6.0),
                vibrance: Some(14.0),
                hsl: hsl(&[
                    ("orange", 3.0, 12.0, 6.0),
                    ("yellow", 2.0, 10.0, 4.0),
                    ("red", 0.0, 6.0, 2.0),
                ]),
                grain: Some(5.0),
                vignette: Some(14.0),
                ..Default::default()
            },
        ),
        preset(
            "builtin-soft-matte",
            "柔哑光",
            PresetCategory::Mood,
            "压低高光与饱和度，暗部抬升形成哑光质感。",
            "极简、棚拍、浅色背景人像",
            "需要高反差与亮部细节的素材",
            PresetParams {
                contrast: Some(-6.0),
                highlights: Some(-20.0),
                shadows: Some(12.0),
                whites: Some(-8.0),
                blacks: Some(10.0),
                saturation: Some(-12.0),
                vibrance: Some(4.0),
                clarity: Some(3.0),
                grain: Some(6.0),
                ..Default::default()
            },
        ),
    ]
}

fn normalized(points: &[CurvePoint]) -> Vec<CurvePoint> {
    let mut list: Vec<CurvePoint> = points
        .iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| CurvePoint::new(point.x, point.y))
        .collect();
    list.sort_by(|left, right| left.x.partial_cmp(&right.x).unwrap_or(Ordering::Equal));
    list.dedup_by(|left, right| (left.x - right.x).abs() < 1e-4);
    if list.is_empty() {
        return list;
    }
    if list[0].x > 1e-4 {
        let y = list[0].y;
        list.insert(0, CurvePoint::new(0.0, y));
    }
    if list.last().map(|point| point.x).unwrap_or(1.0) < 1.0 - 1e-4 {
        let y = list.last().map(|point| point.y).unwrap_or(1.0);
        list.push(CurvePoint::new(1.0, y));
    }
    list
}

fn evaluate(points: &[CurvePoint], x: f32) -> f32 {
    if points.is_empty() {
        return x;
    }
    let x = x.clamp(0.0, 1.0);
    if x <= points[0].x {
        return points[0].y;
    }
    for pair in points.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        if x <= right.x {
            let span = (right.x - left.x).max(1e-6);
            let t = (x - left.x) / span;
            return left.y + (right.y - left.y) * t;
        }
    }
    points.last().map(|point| point.y).unwrap_or(x)
}

/// 曲线混合：先把两侧控制点规范化并补齐端点，在 x 并集上线性求值后按强度混合。
fn mix_curve(current: &[CurvePoint], target: &[CurvePoint], t: f32) -> Vec<CurvePoint> {
    if target.is_empty() {
        return current.to_vec();
    }
    let current = normalized(current);
    let target = normalized(target);
    let identity = current.is_empty();
    let mut xs: Vec<f32> = current
        .iter()
        .chain(target.iter())
        .map(|point| point.x)
        .collect();
    xs.push(0.0);
    xs.push(1.0);
    xs.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    xs.dedup_by(|left, right| (*left - *right).abs() < 1e-4);

    let mut mixed: Vec<CurvePoint> = xs
        .iter()
        .map(|&x| {
            let base = if identity { x } else { evaluate(&current, x) };
            let goal = evaluate(&target, x);
            CurvePoint::new(x, base + (goal - base) * t)
        })
        .collect();

    if mixed.len() > 32 {
        let step = (mixed.len() - 1) as f32 / 31.0;
        let mut thinned = Vec::with_capacity(32);
        for index in 0..32 {
            let pick = (index as f32 * step).round() as usize;
            thinned.push(mixed[pick]);
        }
        mixed = thinned;
    }
    mixed
}

fn mix_scalar(current: f32, target: Option<f32>, t: f32) -> f32 {
    match target {
        Some(value) => current + (value - current) * t,
        None => current,
    }
}

fn beauty_names() -> [&'static str; 14] {
    [
        "skin_smooth",
        "texture",
        "detail",
        "blemish",
        "spots",
        "blackheads",
        "tone_even",
        "whitening",
        "redness",
        "dullness",
        "dark_circles",
        "eye_lines",
        "forehead_lines",
        "nasolabial",
    ]
}

fn beauty_slot<'a>(face: &'a mut FaceBeauty, field: &str) -> Option<&'a mut f32> {
    Some(match field {
        "skin_smooth" => &mut face.beauty.skin_smooth,
        "texture" => &mut face.beauty.texture,
        "detail" => &mut face.beauty.detail,
        "blemish" => &mut face.beauty.blemish,
        "spots" => &mut face.beauty.spots,
        "blackheads" => &mut face.beauty.blackheads,
        "tone_even" => &mut face.beauty.tone_even,
        "whitening" => &mut face.beauty.whitening,
        "redness" => &mut face.beauty.redness,
        "dullness" => &mut face.beauty.dullness,
        "dark_circles" => &mut face.beauty.dark_circles,
        "eye_lines" => &mut face.beauty.eye_lines,
        "forehead_lines" => &mut face.beauty.forehead_lines,
        "nasolabial" => &mut face.beauty.nasolabial,
        _ => return None,
    })
}

/// 应用预设。强度 s（0..100）只影响参数值，不会整份 JSON 相乘；
/// 对象级数据（人脸 UUID、蒙版、锚点、背景引用）永远不在预设里，也就不会被覆盖。
pub fn apply_preset(
    params: &EditParams,
    preset: &RetouchPreset,
    strength: f32,
    faces: &[FaceFrame],
) -> (EditParams, Vec<String>) {
    let t = (strength / 100.0).clamp(0.0, 1.0);
    if t <= 0.0 {
        return (params.clone(), Vec::new());
    }
    let mut next = params.clone();
    let mut warnings = Vec::new();
    let values = &preset.params;

    next.exposure = mix_scalar(next.exposure, values.exposure, t);
    next.contrast = mix_scalar(next.contrast, values.contrast, t);
    next.highlights = mix_scalar(next.highlights, values.highlights, t);
    next.shadows = mix_scalar(next.shadows, values.shadows, t);
    next.whites = mix_scalar(next.whites, values.whites, t);
    next.blacks = mix_scalar(next.blacks, values.blacks, t);
    next.temperature = mix_scalar(next.temperature, values.temperature, t);
    next.tint = mix_scalar(next.tint, values.tint, t);
    next.saturation = mix_scalar(next.saturation, values.saturation, t);
    next.vibrance = mix_scalar(next.vibrance, values.vibrance, t);
    next.clarity = mix_scalar(next.clarity, values.clarity, t);
    next.sharpness = mix_scalar(next.sharpness, values.sharpness, t);
    next.grain = mix_scalar(next.grain, values.grain, t);
    next.vignette = mix_scalar(next.vignette, values.vignette, t);

    for (name, target) in values.hsl.iter() {
        if let Some(channel) = next.hsl.channel_mut(name) {
            channel.hue += (target.hue - channel.hue) * t;
            channel.saturation += (target.saturation - channel.saturation) * t;
            channel.luminance += (target.luminance - channel.luminance) * t;
        }
    }

    if !values.curves.is_neutral() {
        next.curves.rgb = mix_curve(&next.curves.rgb, &values.curves.rgb, t);
        next.curves.red = mix_curve(&next.curves.red, &values.curves.red, t);
        next.curves.green = mix_curve(&next.curves.green, &values.curves.green, t);
        next.curves.blue = mix_curve(&next.curves.blue, &values.curves.blue, t);
    }

    if let Some(name) = &values.lut_name {
        let target_strength = values.lut_strength.unwrap_or(100.0);
        match next.lut.as_mut() {
            Some(lut) if lut.name.eq_ignore_ascii_case(name) => {
                lut.strength = mix_scalar(lut.strength, Some(target_strength), t);
            }
            _ => warnings.push(format!(
                "预设引用的 LUT「{name}」当前照片未选择，请在调色面板手动选择同名 LUT 后再应用强度"
            )),
        }
    }

    if !values.beauty.is_empty() {
        if next.faces.is_empty() {
            if faces.is_empty() {
                warnings.push("照片未检测到人脸，预设中的美肤模板已跳过".into());
            } else {
                for face in faces {
                    next.faces.push(FaceBeauty {
                        face_id: face.id,
                        face_uuid: if face.uuid.is_empty() {
                            format!("face-{}", face.id)
                        } else {
                            face.uuid.clone()
                        },
                        enabled: true,
                        beauty: BeautyParams::default(),
                    });
                }
            }
        }
        if next.faces.is_empty() {
            warnings.push("没有可应用的人脸，预设中的美肤模板已跳过".into());
        } else {
            for face in next.faces.iter_mut() {
                for (field, target) in values.beauty.iter() {
                    if !beauty_names().contains(&field.as_str()) {
                        warnings.push(format!("预设包含未识别的美肤字段：{field}"));
                        continue;
                    }
                    if let Some(slot) = beauty_slot(face, field) {
                        *slot = *slot + (*target - *slot) * t;
                    }
                }
            }
        }
    }

    (next.clamped(), warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use serde_json::Value;

    use crate::image_engine::buffer::Image16;

    fn faces() -> Vec<FaceFrame> {
        vec![FaceFrame {
            id: 1,
            uuid: "face-a".into(),
            score: 0.9,
            x: 0.2,
            y: 0.2,
            width: 0.2,
            height: 0.2,
            landmarks: Vec::new(),
            dense: None,
            level3: None,
        }]
    }

    #[test]
    fn builtin_presets_are_distinct_and_complete() {
        let presets = builtin_presets();
        assert!(presets.len() >= 12, "至少 12 个内置预设");
        for (index, preset) in presets.iter().enumerate() {
            assert!(!preset.params.is_empty(), "{} 不能是空参数", preset.id);
            assert!(!preset.description.is_empty());
            assert!(!preset.suitable_for.is_empty());
            for other in presets.iter().skip(index + 1) {
                assert_ne!(
                    serde_json::to_string(&preset.params).unwrap(),
                    serde_json::to_string(&other.params).unwrap(),
                    "{} 与 {} 参数雷同",
                    preset.id,
                    other.id
                );
            }
        }
    }

    #[test]
    fn preset_params_never_contain_object_data() {
        let params = EditParams::default();
        let preset = &builtin_presets()[0];
        let json = serde_json::to_string(&preset.params).unwrap();
        for forbidden in [
            "faces",
            "faceUuid",
            "manualMask",
            "reshape",
            "subjectMask",
            "background",
            "repairPoints",
            "path",
            "imageHash",
            "projectId",
        ] {
            assert!(!json.contains(forbidden), "预设不应包含 {forbidden}");
        }
        let stored = PresetParams::from_edit_params(&params);
        assert!(stored.is_empty());
    }

    #[test]
    fn zero_strength_is_identity() {
        let params = EditParams {
            exposure: 12.0,
            faces: vec![FaceBeauty::new(1)],
            ..EditParams::default()
        };
        let preset = &builtin_presets()[0];
        let (next, _) = apply_preset(&params, preset, 0.0, &[]);
        assert_eq!(next, params);
    }

    #[test]
    fn full_strength_matches_preset_values() {
        let params = EditParams::default();
        let preset = &builtin_presets()[0];
        let (next, _) = apply_preset(&params, preset, 100.0, &faces());
        assert_eq!(next.exposure, preset.params.exposure.unwrap());
        assert_eq!(next.contrast, preset.params.contrast.unwrap());
        assert_eq!(next.sharpness, preset.params.sharpness.unwrap());
        let target = preset.params.beauty.get("skin_smooth").copied().unwrap();
        assert_eq!(next.faces[0].beauty.skin_smooth, target);
    }

    #[test]
    fn half_strength_is_between_current_and_target() {
        let params = EditParams {
            exposure: 10.0,
            ..EditParams::default()
        };
        let preset = &builtin_presets()[0];
        let target = preset.params.exposure.unwrap();
        let (next, _) = apply_preset(&params, preset, 50.0, &[]);
        let expected = 10.0 + (target - 10.0) * 0.5;
        assert!((next.exposure - expected).abs() < 0.01);
    }

    #[test]
    fn curve_mixing_keeps_endpoints_and_stays_bounded() {
        let current = vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)];
        let target = vec![
            CurvePoint::new(0.0, 0.1),
            CurvePoint::new(0.5, 0.6),
            CurvePoint::new(1.0, 0.9),
        ];
        let mixed = mix_curve(&current, &target, 0.5);
        assert!(mixed.first().unwrap().x == 0.0);
        assert!(mixed.last().unwrap().x == 1.0);
        for point in mixed {
            assert!((0.0..=1.0).contains(&point.x));
            assert!((0.0..=1.0).contains(&point.y));
        }
    }

    #[test]
    fn missing_lut_does_not_set_absolute_path() {
        let params = EditParams::default();
        let mut preset = builtin_presets()[0].clone();
        preset.params.lut_name = Some("demo.cube".into());
        preset.params.lut_strength = Some(80.0);
        let (next, warnings) = apply_preset(&params, &preset, 100.0, &[]);
        assert!(next.lut.is_none());
        assert!(warnings.iter().any(|item| item.contains("LUT")));
    }

    #[test]
    fn beauty_template_skips_when_no_face() {
        let params = EditParams::default();
        let preset = builtin_presets()[0].clone();
        let (next, warnings) = apply_preset(&params, &preset, 100.0, &[]);
        assert!(next.faces.is_empty());
        assert!(warnings.iter().any(|item| item.contains("未检测到人脸")));
    }

    #[test]
    fn serialized_preset_has_no_raw_edit_params_shape() {
        let preset = builtin_presets()[0].clone();
        let value: Value = serde_json::to_value(&preset).unwrap();
        assert!(value.get("params").is_some());
        assert!(value.get("source").is_some());
        // 结构上不可能保存完整 EditParams：参数对象仅包含允许字段。
        let params = value.get("params").unwrap().as_object().unwrap();
        for key in params.keys() {
            assert!(
                [
                    "exposure",
                    "contrast",
                    "highlights",
                    "shadows",
                    "whites",
                    "blacks",
                    "temperature",
                    "tint",
                    "saturation",
                    "vibrance",
                    "clarity",
                    "sharpness",
                    "grain",
                    "vignette",
                    "hsl",
                    "curves",
                    "lutName",
                    "lutStrength",
                    "beauty"
                ]
                .contains(&key.as_str()),
                "未预期字段 {key}"
            );
        }
        let _ = Image16::from_pixel(2, 2, Rgba([0, 0, 0, 65535]));
    }
}
