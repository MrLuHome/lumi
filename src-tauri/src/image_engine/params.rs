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
    /// 旧字段：UUID 不可用时的回退匹配。
    pub face_id: u32,
    /// 稳定人脸 UUID，项目保存与重开匹配的主键。
    #[serde(default)]
    pub face_uuid: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub beauty: BeautyParams,
}

impl FaceBeauty {
    pub fn new(face_id: u32) -> Self {
        Self {
            face_id,
            face_uuid: String::new(),
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

/// 脸型塑形。全部 -100..100，0 表示不变；正值按字段语义放大，负值反向。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FaceShape {
    pub slim: f32,
    pub left: f32,
    pub right: f32,
    pub jaw: f32,
    pub cheekbone: f32,
    pub width: f32,
    pub length: f32,
    pub balance: f32,
}

impl FaceShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 8] {
        [
            self.slim,
            self.left,
            self.right,
            self.jaw,
            self.cheekbone,
            self.width,
            self.length,
            self.balance,
        ]
    }
}

/// 眼部塑形。尺寸类正值放大；距离正值拉远。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EyeShape {
    pub size: f32,
    pub left: f32,
    pub right: f32,
    pub width: f32,
    pub height: f32,
    pub distance: f32,
    pub corner: f32,
}

impl EyeShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 7] {
        [
            self.size,
            self.left,
            self.right,
            self.width,
            self.height,
            self.distance,
            self.corner,
        ]
    }
}

/// 鼻部塑形。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NoseShape {
    pub bridge: f32,
    pub alar: f32,
    pub tip: f32,
    pub height: f32,
    pub balance: f32,
}

impl NoseShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 5] {
        [self.bridge, self.alar, self.tip, self.height, self.balance]
    }
}

/// 唇部塑形。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MouthShape {
    pub width: f32,
    pub upper: f32,
    pub lower: f32,
    pub corner: f32,
    pub symmetry: f32,
    pub size: f32,
}

impl MouthShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 6] {
        [
            self.width,
            self.upper,
            self.lower,
            self.corner,
            self.symmetry,
            self.size,
        ]
    }
}

/// 下巴塑形。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ChinShape {
    pub length: f32,
    pub width: f32,
    pub point: f32,
    pub jawline: f32,
}

impl ChinShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 4] {
        [self.length, self.width, self.point, self.jawline]
    }
}

/// 手动局部变形模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WarpMode {
    #[default]
    Push,
    Pull,
    ZoomIn,
    ZoomOut,
    Protect,
    Restore,
}

/// 单次手动推拉笔触。坐标按图像归一化，缩放画布后仍然准确。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarpStroke {
    #[serde(default)]
    pub mode: WarpMode,
    /// 笔刷半径，按图像宽度归一化（0.004..0.25）。
    #[serde(default = "default_brush_radius")]
    pub radius: f32,
    /// 强度 0..100。
    #[serde(default = "default_opacity")]
    pub strength: f32,
    /// 羽化 0..100。
    #[serde(default = "default_feather")]
    pub feather: f32,
    #[serde(default)]
    pub points: Vec<MaskPoint>,
}

/// 单张人脸的手动变形记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ManualWarp {
    pub enabled: bool,
    pub strokes: Vec<WarpStroke>,
}

/// 单张人脸的塑形参数，按稳定人脸 ID 保存。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceReshape {
    pub face_id: u32,
    /// 稳定人脸 UUID；为空时回退到 face_id。
    #[serde(default)]
    pub face_uuid: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub face: FaceShape,
    #[serde(default)]
    pub eyes: EyeShape,
    #[serde(default)]
    pub nose: NoseShape,
    #[serde(default)]
    pub mouth: MouthShape,
    #[serde(default)]
    pub chin: ChinShape,
    #[serde(default)]
    pub manual: ManualWarp,
}

impl FaceReshape {
    pub fn new(face_id: u32) -> Self {
        Self {
            face_id,
            face_uuid: String::new(),
            enabled: true,
            face: FaceShape::default(),
            eyes: EyeShape::default(),
            nose: NoseShape::default(),
            mouth: MouthShape::default(),
            chin: ChinShape::default(),
            manual: ManualWarp::default(),
        }
    }

    /// 是否包含任何会改变像素的塑形参数。
    pub fn is_active(&self) -> bool {
        !self.face.is_neutral()
            || !self.eyes.is_neutral()
            || !self.nose.is_neutral()
            || !self.mouth.is_neutral()
            || !self.chin.is_neutral()
            || (self.manual.enabled && !self.manual.strokes.is_empty())
    }
}

/// 手动蒙版笔触类型。
/// - `Paint` 叠加前景，`Erase` 擦除；
/// - `Foreground` / `Background` 是显式标记，优先于普通笔触，用于修正局部误判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SubjectMaskStrokeKind {
    #[default]
    Paint,
    Erase,
    Foreground,
    Background,
}

/// 人物蒙版笔触。坐标按图像宽度/高度归一化；硬度与强度分离，
/// 同一套笔触在任意分辨率重建出相同形状的 Alpha。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectMaskStroke {
    #[serde(default)]
    pub kind: SubjectMaskStrokeKind,
    /// 画笔半径，按图像宽度归一化（0.002..0.5）。
    #[serde(default = "default_brush_radius")]
    pub radius: f32,
    /// 硬度 0..100：100 为硬边，0 为完全柔和过渡。
    #[serde(default = "default_hardness")]
    pub hardness: f32,
    /// 笔触强度 0..100，用于叠加时控制最大 Alpha。
    #[serde(default = "default_opacity")]
    pub strength: f32,
    #[serde(default)]
    pub points: Vec<MaskPoint>,
}

fn default_hardness() -> f32 {
    60.0
}

/// 手动人物蒙版（非 AI 分割）。流程：笔触 → 平滑 → 收缩/扩张 → 羽化 → 发丝细节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectMaskParams {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub invert: bool,
    /// 羽化 0..100（按图像短边换算像素半径）。
    #[serde(default = "default_feather")]
    pub feather: f32,
    /// 蒙版收缩/扩张 -100..100；正值扩张，负值收缩。
    #[serde(default)]
    pub expand: f32,
    /// 边缘平滑 0..100。
    #[serde(default)]
    pub smooth: f32,
    /// 发丝保护 0..100：在边缘带按原图高频细节保留细碎发丝。
    #[serde(default)]
    pub hair_protect: f32,
    /// 边缘去污染 0..100（背景替换时使用）。
    #[serde(default = "default_feather")]
    pub decontaminate: f32,
    #[serde(default)]
    pub strokes: Vec<SubjectMaskStroke>,
    /// 蒙版来源：`manual` 表示本地手动笔触，不是 AI 模型输出。
    #[serde(default = "default_manual_source")]
    pub source: String,
    #[serde(default = "default_manual_version")]
    pub model_version: String,
}

fn default_manual_source() -> String {
    "manual".into()
}

fn default_manual_version() -> String {
    "manual-alpha-1".into()
}

impl Default for SubjectMaskParams {
    fn default() -> Self {
        Self {
            enabled: false,
            invert: false,
            feather: default_feather(),
            expand: 0.0,
            smooth: 15.0,
            hair_protect: 0.0,
            decontaminate: default_feather(),
            strokes: Vec::new(),
            source: default_manual_source(),
            model_version: default_manual_version(),
        }
    }
}

impl SubjectMaskParams {
    pub fn has_content(&self) -> bool {
        self.enabled && !self.strokes.is_empty()
    }

    pub fn stroke_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.invert.hash(&mut hasher);
        self.feather.to_bits().hash(&mut hasher);
        self.expand.to_bits().hash(&mut hasher);
        self.smooth.to_bits().hash(&mut hasher);
        for stroke in &self.strokes {
            stroke.kind.hash(&mut hasher);
            stroke.radius.to_bits().hash(&mut hasher);
            stroke.hardness.to_bits().hash(&mut hasher);
            stroke.strength.to_bits().hash(&mut hasher);
            for point in &stroke.points {
                point.x.to_bits().hash(&mut hasher);
                point.y.to_bits().hash(&mut hasher);
            }
        }
        format!("{:016x}", hasher.finish())
    }
}

/// 身材参数。全部 -100..100，0 表示不变；正值按字段语义放大，负值收窄。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BodyShape {
    pub waist: f32,
    pub shoulders: f32,
    pub hips: f32,
    pub thighs: f32,
    pub calves: f32,
    pub arms: f32,
    pub legs: f32,
}

impl BodyShape {
    pub fn is_neutral(&self) -> bool {
        self.fields().iter().all(|value| *value == 0.0)
    }

    pub fn fields(&self) -> [f32; 7] {
        [
            self.waist,
            self.shoulders,
            self.hips,
            self.thighs,
            self.calves,
            self.arms,
            self.legs,
        ]
    }
}

/// 用户手动放置的身体控制点部位。它是编辑辅助标签，不是 AI 识别结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BodyAnchorPart {
    ShoulderLeft,
    ShoulderRight,
    WaistLeft,
    WaistRight,
    HipLeft,
    HipRight,
    ThighLeft,
    ThighRight,
    KneeLeft,
    KneeRight,
    CalfLeft,
    CalfRight,
    ArmLeft,
    ArmRight,
    #[default]
    Custom,
}

/// 手动身材锚点：把 (x,y) 处的身体边缘按 (dx,dy) 移动，周围平滑跟随。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyAnchor {
    #[serde(default)]
    pub part: BodyAnchorPart,
    pub x: f32,
    pub y: f32,
    /// 归一化位移（相对图像宽/高），受最大位移约束。
    pub dx: f32,
    pub dy: f32,
    /// 影响半径，按图像短边归一化。
    #[serde(default = "default_anchor_radius")]
    pub radius: f32,
    /// 锁定后画布拖动不会移动该锚点。
    #[serde(default)]
    pub locked: bool,
}

fn default_anchor_radius() -> f32 {
    0.12
}

/// 单个身材对象的参数。`subject_id` 是稳定 UUID，与人脸 UUID 不混用。
/// 条目可以携带独立蒙版笔触，实现多人照片中 A 的变形不影响 B。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyAdjust {
    pub subject_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub shape: BodyShape,
    #[serde(default)]
    pub anchors: Vec<BodyAnchor>,
    /// 手动推拉笔触（与第四轮同一套模式，但作用在身体蒙版内）。
    #[serde(default)]
    pub strokes: Vec<WarpStroke>,
    /// 该人物的独立蒙版笔触；为空时使用全局人物蒙版。
    #[serde(default)]
    pub mask_strokes: Vec<SubjectMaskStroke>,
}

impl BodyAdjust {
    pub fn new(subject_id: impl Into<String>) -> Self {
        Self {
            subject_id: subject_id.into(),
            enabled: true,
            shape: BodyShape::default(),
            anchors: Vec::new(),
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.shape.is_neutral() || !self.anchors.is_empty() || !self.strokes.is_empty()
    }

    pub fn has_own_mask(&self) -> bool {
        !self.mask_strokes.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundMode {
    #[default]
    Original,
    Blur,
    SolidColor,
    Gradient,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

/// 渐变色标，`position` 约定在 0..100。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    pub position: f32,
    /// `#RRGGBB`，非法颜色会被夹紧为黑色。
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundGradient {
    /// 角度 0..360，0 表示从左到右。
    pub angle: f32,
    pub stops: Vec<GradientStop>,
}

impl Default for BackgroundGradient {
    fn default() -> Self {
        Self {
            angle: 0.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: "#1c2733".into(),
                },
                GradientStop {
                    position: 100.0,
                    color: "#4a6076".into(),
                },
            ],
        }
    }
}

/// 背景处理参数。`mode` 为 Original 时不改变任何像素。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundParams {
    pub enabled: bool,
    pub mode: BackgroundMode,
    /// 背景虚化 0..100。
    pub blur: f32,
    /// 模拟景深 0..100：只改变背景虚化强度分布，不是真实 3D 深度。
    pub depth: f32,
    /// 背景亮度 -100..100。
    pub brightness: f32,
    pub temperature: f32,
    pub saturation: f32,
    /// 纯色背景 `#RRGGBB`。
    pub color: String,
    pub gradient: BackgroundGradient,
    /// 本地背景图片路径；不会上传到任何服务器。
    pub image_path: String,
    pub image_fit: BackgroundFit,
    /// 背景图片缩放 50..200。
    pub image_scale: f32,
    /// 背景图片偏移 -100..100（百分比）。
    pub image_offset_x: f32,
    pub image_offset_y: f32,
    pub image_blur: f32,
    /// 前景边缘去污染 0..100。
    pub decontaminate: f32,
}

impl Default for BackgroundParams {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: BackgroundMode::Original,
            blur: 45.0,
            depth: 30.0,
            brightness: 0.0,
            temperature: 0.0,
            saturation: 0.0,
            color: "#1f2933".into(),
            gradient: BackgroundGradient::default(),
            image_path: String::new(),
            image_fit: BackgroundFit::Cover,
            image_scale: 100.0,
            image_offset_x: 0.0,
            image_offset_y: 0.0,
            image_blur: 0.0,
            decontaminate: 60.0,
        }
    }
}

impl BackgroundParams {
    pub fn is_active(&self) -> bool {
        self.enabled && self.mode != BackgroundMode::Original
    }
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
    /// 按稳定人脸 ID 保存的独立塑形参数（第四轮）。
    #[serde(default)]
    pub reshape: Vec<FaceReshape>,
    /// 第五轮：手动人物蒙版（非 AI 分割）。
    #[serde(default)]
    pub subject_mask: SubjectMaskParams,
    /// 第五轮：按稳定人物 UUID 保存的身材参数。
    #[serde(default)]
    pub body: Vec<BodyAdjust>,
    /// 第五轮：背景处理参数。
    #[serde(default)]
    pub background: BackgroundParams,
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
            reshape: Vec::new(),
            subject_mask: SubjectMaskParams::default(),
            body: Vec::new(),
            background: BackgroundParams::default(),
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
        clamp_reshape(&mut self.reshape);
        clamp_subject_mask(&mut self.subject_mask);
        clamp_body(&mut self.body);
        clamp_background(&mut self.background);
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
            && clamped
                .reshape
                .iter()
                .all(|face| !face.enabled || !face.is_active())
            && clamped.body.iter().all(|entry| !entry.enabled || !entry.is_active())
            && !clamped.background.is_active()
            && clamped.hsl.is_neutral()
            && clamped.curves.is_neutral()
            && clamped.lut.is_none()
    }

    /// 找到某张脸的参数；不存在时返回默认值（不修改自身）。
    pub fn face_beauty(&self, face_id: u32) -> Option<&FaceBeauty> {
        self.faces.iter().find(|face| face.face_id == face_id)
    }

    pub fn face_reshape(&self, face_id: u32) -> Option<&FaceReshape> {
        self.reshape.iter().find(|face| face.face_id == face_id)
    }

    /// UUID 优先、face_id 回退的参数查找。
    pub fn face_beauty_match(&self, uuid: &str, face_id: u32) -> Option<&FaceBeauty> {
        if !uuid.is_empty() {
            if let Some(entry) = self.faces.iter().find(|entry| entry.face_uuid == uuid) {
                return Some(entry);
            }
        }
        self.faces.iter().find(|entry| entry.face_id == face_id)
    }

    pub fn face_reshape_match(&self, uuid: &str, face_id: u32) -> Option<&FaceReshape> {
        if !uuid.is_empty() {
            if let Some(entry) = self.reshape.iter().find(|entry| entry.face_uuid == uuid) {
                return Some(entry);
            }
        }
        self.reshape.iter().find(|entry| entry.face_id == face_id)
    }

    /// 是否存在任何启用的塑形参数。
    pub fn has_reshape(&self) -> bool {
        self.reshape.iter().any(|face| face.enabled && face.is_active())
    }

    /// 是否存在需要人物蒙版的工作（蒙版笔触 / 身材 / 背景）。
    pub fn has_subject_mask(&self) -> bool {
        self.subject_mask.has_content()
            || self.body.iter().any(|entry| entry.enabled && entry.has_own_mask())
    }

    pub fn has_body(&self) -> bool {
        self.body.iter().any(|entry| entry.enabled && entry.is_active())
    }

    pub fn has_background(&self) -> bool {
        self.background.is_active()
    }

    pub fn body_match(&self, subject_id: &str) -> Option<&BodyAdjust> {
        self.body.iter().find(|entry| entry.subject_id == subject_id)
    }

    /// 所有会参与渲染的身材对象（稳定 UUID 顺序）。
    pub fn active_body_subjects(&self) -> Vec<&BodyAdjust> {
        self.body
            .iter()
            .filter(|entry| entry.enabled && entry.is_active())
            .collect()
    }
}

fn clamp_subject_mask(mask: &mut SubjectMaskParams) {
    mask.feather = mask.feather.clamp(0.0, 100.0);
    mask.expand = mask.expand.clamp(-100.0, 100.0);
    mask.smooth = mask.smooth.clamp(0.0, 100.0);
    mask.hair_protect = mask.hair_protect.clamp(0.0, 100.0);
    mask.decontaminate = mask.decontaminate.clamp(0.0, 100.0);
    clamp_subject_strokes(&mut mask.strokes);
    if mask.source.trim().is_empty() {
        mask.source = default_manual_source();
    }
    if mask.model_version.trim().is_empty() {
        mask.model_version = default_manual_version();
    }
}

fn clamp_subject_strokes(strokes: &mut Vec<SubjectMaskStroke>) {
    strokes.retain(|stroke| !stroke.points.is_empty());
    strokes.truncate(400);
    for stroke in strokes.iter_mut() {
        stroke.radius = stroke.radius.clamp(0.002, 0.5);
        stroke.hardness = stroke.hardness.clamp(0.0, 100.0);
        stroke.strength = stroke.strength.clamp(0.0, 100.0);
        stroke.points.truncate(8000);
        stroke
            .points
            .retain(|point| point.x.is_finite() && point.y.is_finite());
        for point in stroke.points.iter_mut() {
            point.x = point.x.clamp(0.0, 1.0);
            point.y = point.y.clamp(0.0, 1.0);
        }
    }
    strokes.retain(|stroke| !stroke.points.is_empty());
}

/// 身材参数夹紧：所有数值 -100..100，笔触/锚点有数量上限，
/// 位移按 15% 上限预先收敛，防止异常项目文件产生不可控变形。
fn clamp_body(entries: &mut Vec<BodyAdjust>) {
    entries.retain(|entry| !entry.subject_id.trim().is_empty());
    entries.truncate(32);
    for entry in entries.iter_mut() {
        clamp_signed(&mut [
            &mut entry.shape.waist,
            &mut entry.shape.shoulders,
            &mut entry.shape.hips,
            &mut entry.shape.thighs,
            &mut entry.shape.calves,
            &mut entry.shape.arms,
            &mut entry.shape.legs,
        ]);
        entry.anchors.truncate(200);
        for anchor in entry.anchors.iter_mut() {
            anchor.x = anchor.x.clamp(0.0, 1.0);
            anchor.y = anchor.y.clamp(0.0, 1.0);
            anchor.dx = anchor.dx.clamp(-0.15, 0.15);
            anchor.dy = anchor.dy.clamp(-0.15, 0.15);
            anchor.radius = anchor.radius.clamp(0.02, 0.5);
        }
        entry.strokes.retain(|stroke| !stroke.points.is_empty());
        entry.strokes.truncate(400);
        for stroke in entry.strokes.iter_mut() {
            stroke.radius = stroke.radius.clamp(0.004, 0.25);
            stroke.strength = stroke.strength.clamp(0.0, 100.0);
            stroke.feather = stroke.feather.clamp(0.0, 100.0);
            stroke.points.truncate(4000);
            stroke
                .points
                .retain(|point| point.x.is_finite() && point.y.is_finite());
            for point in stroke.points.iter_mut() {
                point.x = point.x.clamp(0.0, 1.0);
                point.y = point.y.clamp(0.0, 1.0);
            }
        }
        entry.strokes.retain(|stroke| !stroke.points.is_empty());
        clamp_subject_strokes(&mut entry.mask_strokes);
    }
    entries.retain(|entry| entry.enabled || entry.is_active() || !entry.mask_strokes.is_empty());
    entries.sort_by(|a, b| a.subject_id.cmp(&b.subject_id));
    entries.dedup_by(|a, b| a.subject_id == b.subject_id);
}

fn clamp_background(bg: &mut BackgroundParams) {
    bg.blur = bg.blur.clamp(0.0, 100.0);
    bg.depth = bg.depth.clamp(0.0, 100.0);
    clamp_signed(&mut [
        &mut bg.brightness,
        &mut bg.temperature,
        &mut bg.saturation,
    ]);
    bg.image_scale = bg.image_scale.clamp(50.0, 200.0);
    bg.image_offset_x = bg.image_offset_x.clamp(-100.0, 100.0);
    bg.image_offset_y = bg.image_offset_y.clamp(-100.0, 100.0);
    bg.image_blur = bg.image_blur.clamp(0.0, 100.0);
    bg.decontaminate = bg.decontaminate.clamp(0.0, 100.0);
    bg.color = sanitize_hex_color(&bg.color, "#1f2933");
    bg.gradient.angle = bg.gradient.angle.clamp(0.0, 360.0);
    bg.gradient.stops.truncate(8);
    for stop in bg.gradient.stops.iter_mut() {
        stop.position = stop.position.clamp(0.0, 100.0);
        stop.color = sanitize_hex_color(&stop.color, "#000000");
    }
    bg.gradient
        .stops
        .sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
    if bg.gradient.stops.is_empty() {
        bg.gradient = BackgroundGradient::default();
    }
    if bg.mode == BackgroundMode::Image && bg.image_path.trim().is_empty() {
        bg.mode = BackgroundMode::Original;
    }
}

/// 只接受 `#RRGGBB`（可省略 `#`），其余一律回退，避免把任意字符串传给渲染器。
pub fn sanitize_hex_color(value: &str, fallback: &str) -> String {
    let normalize = |candidate: &str| {
        let trimmed = candidate.trim().trim_start_matches('#');
        if trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(format!("#{}", trimmed.to_ascii_uppercase()))
        } else {
            None
        }
    };
    normalize(value)
        .or_else(|| normalize(fallback))
        .unwrap_or_else(|| "#000000".to_string())
}

fn clamp_signed(values: &mut [&mut f32]) {
    for value in values {
        if value.is_finite() {
            **value = (**value).clamp(-100.0, 100.0);
        } else {
            **value = 0.0;
        }
    }
}

/// 塑形参数与笔触的深夹紧。数值范围与前端一致，笔触数量和点数有上限，
/// 防止异常项目文件把网格计算拖入不可控状态。
fn clamp_reshape(entries: &mut Vec<FaceReshape>) {
    entries.retain(|face| face.face_id < u32::MAX);
    for face in entries.iter_mut() {
        clamp_signed(&mut [
            &mut face.face.slim,
            &mut face.face.left,
            &mut face.face.right,
            &mut face.face.jaw,
            &mut face.face.cheekbone,
            &mut face.face.width,
            &mut face.face.length,
            &mut face.face.balance,
        ]);
        clamp_signed(&mut [
            &mut face.eyes.size,
            &mut face.eyes.left,
            &mut face.eyes.right,
            &mut face.eyes.width,
            &mut face.eyes.height,
            &mut face.eyes.distance,
            &mut face.eyes.corner,
        ]);
        clamp_signed(&mut [
            &mut face.nose.bridge,
            &mut face.nose.alar,
            &mut face.nose.tip,
            &mut face.nose.height,
            &mut face.nose.balance,
        ]);
        clamp_signed(&mut [
            &mut face.mouth.width,
            &mut face.mouth.upper,
            &mut face.mouth.lower,
            &mut face.mouth.corner,
            &mut face.mouth.symmetry,
            &mut face.mouth.size,
        ]);
        clamp_signed(&mut [
            &mut face.chin.length,
            &mut face.chin.width,
            &mut face.chin.point,
            &mut face.chin.jawline,
        ]);

        face.manual.strokes.retain(|stroke| !stroke.points.is_empty());
        face.manual.strokes.truncate(400);
        for stroke in face.manual.strokes.iter_mut() {
            stroke.radius = stroke.radius.clamp(0.004, 0.25);
            stroke.strength = stroke.strength.clamp(0.0, 100.0);
            stroke.feather = stroke.feather.clamp(0.0, 100.0);
            stroke.points.truncate(4000);
            stroke
                .points
                .retain(|point| point.x.is_finite() && point.y.is_finite());
            for point in stroke.points.iter_mut() {
                point.x = point.x.clamp(0.0, 1.0);
                point.y = point.y.clamp(0.0, 1.0);
            }
        }
        face.manual.strokes.retain(|stroke| !stroke.points.is_empty());
    }
    entries.retain(|face| face.enabled || face.is_active());
    entries.sort_by_key(|face| face.face_id);
    entries.dedup_by(|a, b| {
        (!a.face_uuid.is_empty() && a.face_uuid == b.face_uuid) || a.face_id == b.face_id
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beauty_params_clamp_and_detect_activity() {
        let mut params = EditParams::default();
        params.faces.push(FaceBeauty {
            face_id: 3,
            face_uuid: String::new(),
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

    #[test]
    fn body_params_clamp_and_keep_stable_subject_ids() {
        let params = EditParams {
            body: vec![
                BodyAdjust {
                    subject_id: "b-subject".into(),
                    enabled: true,
                    shape: BodyShape {
                        waist: 180.0,
                        shoulders: -300.0,
                        ..Default::default()
                    },
                    anchors: vec![BodyAnchor {
                        part: BodyAnchorPart::WaistLeft,
                        x: 2.0,
                        y: -1.0,
                        dx: 0.9,
                        dy: -0.9,
                        radius: 0.01,
                        locked: true,
                    }],
                    strokes: Vec::new(),
                    mask_strokes: Vec::new(),
                },
                BodyAdjust::new("a-subject"),
            ],
            ..Default::default()
        }
        .clamped();
        assert_eq!(params.body[0].subject_id, "a-subject");
        assert_eq!(params.body[1].shape.waist, 100.0);
        assert_eq!(params.body[1].shape.shoulders, -100.0);
        assert_eq!(params.body[1].anchors[0].x, 1.0);
        assert_eq!(params.body[1].anchors[0].y, 0.0);
        assert_eq!(params.body[1].anchors[0].dx, 0.15);
        assert!(params.has_body());
        assert!(!params.is_neutral());
    }

    #[test]
    fn background_params_validate_colors_and_mode() {
        let params = EditParams {
            background: BackgroundParams {
                enabled: true,
                mode: BackgroundMode::Image,
                image_path: "  ".into(),
                color: "not-a-color".into(),
                blur: 240.0,
                brightness: -300.0,
                gradient: BackgroundGradient {
                    angle: 999.0,
                    stops: vec![
                        GradientStop {
                            position: 220.0,
                            color: "#abcdef".into(),
                        },
                        GradientStop {
                            position: -10.0,
                            color: "zzz".into(),
                        },
                    ],
                },
                ..Default::default()
            },
            ..Default::default()
        }
        .clamped();
        assert_eq!(params.background.mode, BackgroundMode::Original);
        assert_eq!(params.background.color, "#1F2933");
        assert_eq!(params.background.blur, 100.0);
        assert_eq!(params.background.brightness, -100.0);
        assert_eq!(params.background.gradient.angle, 360.0);
        assert_eq!(params.background.gradient.stops[0].position, 0.0);
        assert_eq!(params.background.gradient.stops[0].color, "#000000");
        assert_eq!(params.background.gradient.stops[1].position, 100.0);
        assert!(!params.has_background());
        assert!(params.is_neutral());
    }

    #[test]
    fn subject_mask_hash_changes_with_strokes() {
        let mut mask = SubjectMaskParams {
            enabled: true,
            strokes: vec![SubjectMaskStroke {
                kind: SubjectMaskStrokeKind::Paint,
                radius: 0.1,
                hardness: 50.0,
                strength: 100.0,
                points: vec![MaskPoint { x: 0.2, y: 0.3 }],
            }],
            ..Default::default()
        };
        let first = mask.stroke_hash();
        mask.strokes[0].points[0].x = 0.4;
        assert_ne!(first, mask.stroke_hash());
        assert!(mask.has_content());
    }
}
