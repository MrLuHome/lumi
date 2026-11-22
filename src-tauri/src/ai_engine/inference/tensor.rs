//! 统一张量描述。
//!
//! 这里刻意不用“裸 `Vec<f32>` + 口头约定”的方式表达输入输出：
//! 每个张量都必须带显式的 dtype / layout / shape / 值域 / 通道顺序，
//! 让「输入口径」成为可被机器校验的数据，而不是注释里的一句话。
//!
//! 为什么重要（R9 的真实教训）：`ai/face.rs` 的预处理是
//! `u16 像素 / 257.0`，也就是把 16 位内部表示压回 **0..255**；
//! 而旧常量写的是「BGR，0..255」。两者其实一致，但常量是自由字符串，
//! 无法被任何代码校验，一旦未来的通用预处理按「0..1」实现，输入量纲
//! 就会差 255 倍，YuNet 分数全部低于 0.01，表现为“AI 突然检测不到人脸”。
//! 因此这里把量纲升级为枚举，并在 [`TensorBatch::new`] 中强制校验。

use std::borrow::Cow;

use serde::Serialize;

use super::error::InferenceError;

/// 数据类型。R11 只支持 F32，其余显式拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dtype {
    /// 32 位浮点。唯一被 CPU 后端接受的类型。
    F32,
}

impl Dtype {
    pub fn label(self) -> &'static str {
        match self {
            Dtype::F32 => "float32",
        }
    }
}

/// 内存布局。R11 只支持 NCHW。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// [N, C, H, W]
    Nchw,
}

impl Layout {
    pub fn label(self) -> &'static str {
        match self {
            Layout::Nchw => "NCHW",
        }
    }
}

/// 通道顺序。YuNet 用 BGR（OpenCV 训练口径），其余多为 RGB。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelOrder {
    Bgr,
    Rgb,
}

impl ChannelOrder {
    pub fn label(self) -> &'static str {
        match self {
            ChannelOrder::Bgr => "BGR",
            ChannelOrder::Rgb => "RGB",
        }
    }
}

/// 输入张量的值域。**必须与预处理实现严格一致**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueRange {
    /// 0..1
    ZeroOne,
    /// -1..1
    MinusOneToOne,
    /// 0..255（YuNet 实际口径：`u16 / 257.0`）
    Raw255,
    /// ImageNet mean/std
    Imagenet,
}

impl ValueRange {
    pub fn label(self) -> &'static str {
        match self {
            ValueRange::ZeroOne => "0..1",
            ValueRange::MinusOneToOne => "-1..1",
            ValueRange::Raw255 => "0..255",
            ValueRange::Imagenet => "ImageNet 均值/方差",
        }
    }

    /// 该值域允许的闭区间；`None` 表示不做范围校验（如 ImageNet 归一化）。
    pub fn bounds(self) -> Option<(f32, f32)> {
        match self {
            ValueRange::ZeroOne => Some((0.0, 1.0)),
            ValueRange::MinusOneToOne => Some((-1.0, 1.0)),
            ValueRange::Raw255 => Some((0.0, 255.0)),
            ValueRange::Imagenet => None,
        }
    }

    /// 把 0..1 的归一化像素映射到该值域。供未来的通用预处理使用，
    /// 保证「声明什么就按什么喂」。
    pub fn apply(self, unit: f32) -> f32 {
        match self {
            ValueRange::ZeroOne => unit,
            ValueRange::MinusOneToOne => unit * 2.0 - 1.0,
            ValueRange::Raw255 => unit * 255.0,
            ValueRange::Imagenet => (unit - 0.485) / 0.229,
        }
    }
}

/// 张量契约。
///
/// `shape` 用 `Cow<'static, [usize]>` 而不是 `Vec<usize>`：
/// - 登记表里必须是 **`const` 可构造**（`Cow::Borrowed(&[..])` 零堆分配，
///   且 `const YUNET` 能被 `const ALL: &[&ModelSpec] = &[&YUNET]` 取引用）；
/// - 而 `InferenceBackend::inspect()` 必须能表达**任意 ONNX 文件**的实际形状
///   （可能含符号维），这需要拥有所有权的 `Cow::Owned(Vec<..>)`。
///
/// 两者共用一个类型，避免"声明契约"与"观测契约"变成两套结构。
/// 符号维（动态维）统一记为 `0`，比对时充当通配符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorSpec {
    pub dtype: Dtype,
    pub layout: Layout,
    pub shape: Cow<'static, [usize]>,
    pub value_range: ValueRange,
    pub channel_order: ChannelOrder,
}

fn format_shape(shape: &[usize]) -> String {
    if shape.is_empty() {
        "标量".to_string()
    } else {
        shape
            .iter()
            .map(|dim| dim.to_string())
            .collect::<Vec<_>>()
            .join("×")
    }
}

impl TensorSpec {
    /// `const` 可用的静态契约构造器（无堆分配）。
    pub const fn declared(
        shape: &'static [usize],
        value_range: ValueRange,
        channel_order: ChannelOrder,
    ) -> Self {
        Self {
            dtype: Dtype::F32,
            layout: Layout::Nchw,
            shape: Cow::Borrowed(shape),
            value_range,
            channel_order,
        }
    }

    /// 观测到的契约（`inspect()` 用，形状可能来自任意 ONNX 文件）。
    pub fn observed(shape: Vec<usize>, value_range: ValueRange, channel_order: ChannelOrder) -> Self {
        Self {
            dtype: Dtype::F32,
            layout: Layout::Nchw,
            shape: Cow::Owned(shape),
            value_range,
            channel_order,
        }
    }

    /// 元素总数。符号维（0）在登记表里不会出现。
    pub fn element_count(&self) -> usize {
        self.shape.iter().product()
    }

    /// 是否是**占位契约**：形状全为 0（或空形状），没有真实描述任何张量。
    ///
    /// 为什么需要它：用户自备模型的 IO 契约由它自己的 manifest 描述，登记表里
    /// 只能用全 0 形状占位。如果这份占位契约被喂进 `TensorBatch`，`element_count() == 0`
    /// 会让"空数据"通过元素数校验，形成一个静默通过的口子。
    /// 因此 `validate()` 显式拒绝占位契约，而不是靠"调用方不会这么用"。
    pub fn is_placeholder(&self) -> bool {
        self.shape.iter().all(|dim| *dim == 0)
    }

    /// 结构化的人可读描述，用于替代旧版自由字符串常量。
    pub fn format_human(&self) -> String {
        let dims = self
            .shape
            .iter()
            .map(|dim| dim.to_string())
            .collect::<Vec<_>>()
            .join("×");
        format!(
            "{dims} {}（{}，{}）",
            self.dtype.label(),
            self.channel_order.label(),
            self.value_range.label()
        )
    }

    /// 与另一份契约是否一致（dtype / layout / shape / 值域 / 通道顺序全部相等）。
    pub fn same_as(&self, other: &TensorSpec) -> bool {
        self.dtype == other.dtype
            && self.layout == other.layout
            && self.shape.as_ref() == other.shape.as_ref()
            && self.value_range == other.value_range
            && self.channel_order == other.channel_order
    }

    /// 校验一个具体张量是否符合该契约。
    pub fn validate(&self, data: &[f32]) -> Result<(), InferenceError> {
        if self.is_placeholder() {
            return Err(InferenceError::ContractMismatch {
                model_id: String::new(),
                detail: format!(
                    "契约形状 {} 是占位值（全 0），未真实描述张量，拒绝推理",
                    format_shape(self.shape.as_ref())
                ),
            });
        }
        let expected = self.element_count();
        if data.len() != expected {
            return Err(InferenceError::ContractMismatch {
                model_id: String::new(),
                detail: format!("输入元素数 {} 与契约 {expected} 不符", data.len()),
            });
        }
        if let Some(index) = data.iter().position(|value| !value.is_finite()) {
            return Err(InferenceError::InvalidInput(format!(
                "输入第 {index} 个元素不是有限值（NaN/Inf）"
            )));
        }
        if let Some((low, high)) = self.value_range.bounds() {
            // 留 1e-3 的容差：16 位 → 0..255 的换算存在浮点舍入。
            let tolerance = 1e-3;
            if let Some((index, value)) = data
                .iter()
                .enumerate()
                .find(|(_, value)| **value < low - tolerance || **value > high + tolerance)
            {
                return Err(InferenceError::InvalidInput(format!(
                    "输入第 {index} 个元素 {value} 超出值域 {}",
                    self.value_range.label()
                )));
            }
        }
        Ok(())
    }
}

/// 统一输入输出载体：f32 平面数据 + 显式契约。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorBatch {
    pub spec: TensorSpec,
    pub data: Vec<f32>,
}

impl TensorBatch {
    /// 构造时立即校验元素数、有限值与量纲范围，错误带上下文。
    pub fn new(spec: TensorSpec, data: Vec<f32>) -> Result<Self, InferenceError> {
        spec.validate(&data)?;
        Ok(Self { spec, data })
    }

    /// 用模型 ID 补全错误上下文后校验（用于推理入口）。
    pub fn new_for(model_id: &str, spec: TensorSpec, data: Vec<f32>) -> Result<Self, InferenceError> {
        spec.validate(&data).map_err(|error| match error {
            InferenceError::ContractMismatch { detail, .. } => InferenceError::ContractMismatch {
                model_id: model_id.to_string(),
                detail,
            },
            other => other,
        })?;
        Ok(Self { spec, data })
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: TensorSpec =
        TensorSpec::declared(&[1, 3, 2, 2], ValueRange::ZeroOne, ChannelOrder::Rgb);

    #[test]
    fn element_count_multiplies_shape() {
        assert_eq!(UNIT.element_count(), 12);
    }

    #[test]
    fn new_rejects_wrong_element_count() {
        let error = TensorBatch::new(UNIT, vec![0.0; 11]).unwrap_err();
        assert_eq!(error.code(), "E_MODEL_CONTRACT");
        assert!(TensorBatch::new(UNIT, vec![0.0; 12]).is_ok());
    }

    #[test]
    fn new_rejects_nan_and_infinity() {
        let mut data = vec![0f32; 12];
        data[3] = f32::NAN;
        assert_eq!(
            TensorBatch::new(UNIT, data).unwrap_err().code(),
            "E_INPUT"
        );
        let mut data = vec![0f32; 12];
        data[0] = f32::INFINITY;
        assert_eq!(
            TensorBatch::new(UNIT, data).unwrap_err().code(),
            "E_INPUT"
        );
    }

    #[test]
    fn range_validation_matches_declared_value_range() {
        // 0..255 的输入不能被 0..1 契约接受，反过来的量纲差异正是 R9 的成因。
        let mut data = vec![0f32; 12];
        data[0] = 255.0;
        assert_eq!(
            TensorBatch::new(UNIT, data.clone()).unwrap_err().code(),
            "E_INPUT"
        );
        let raw = TensorSpec {
            value_range: ValueRange::Raw255,
            ..UNIT
        };
        assert!(TensorBatch::new(raw, data).is_ok());
    }

    #[test]
    fn raw255_is_the_yunet_scale_for_u16_pixels() {
        // 16 位内部表示 → 0..255：`u16::MAX as f32 / 257.0 == 255.0`。
        // 这一点决定了 YuNet 的值域必须登记为 Raw255 而不是 ZeroOne。
        let scaled = f32::from(u16::MAX) / 257.0;
        assert!((scaled - 255.0).abs() < 1e-3, "实际得到 {scaled}");
        assert_eq!(ValueRange::Raw255.bounds(), Some((0.0, 255.0)));
    }

    #[test]
    fn format_human_is_structured_not_free_text() {
        let spec = TensorSpec {
            shape: Cow::Borrowed(&[1, 3, 640, 640]),
            value_range: ValueRange::Raw255,
            channel_order: ChannelOrder::Bgr,
            ..UNIT
        };
        assert_eq!(spec.format_human(), "1×3×640×640 float32（BGR，0..255）");
    }

    #[test]
    fn placeholder_shape_is_rejected_instead_of_silently_passing() {
        // 全 0 形状的 element_count() == 0，若不显式拒绝，"空数据"会通过校验。
        let placeholder =
            TensorSpec::observed(vec![0, 0, 0, 0], ValueRange::ZeroOne, ChannelOrder::Rgb);
        assert!(placeholder.is_placeholder());
        assert_eq!(placeholder.element_count(), 0);
        let error = TensorBatch::new(placeholder, Vec::new()).unwrap_err();
        assert_eq!(error.code(), "E_MODEL_CONTRACT");
        assert!(error.to_string().contains("占位"));
        // 真实契约不是占位。
        assert!(!UNIT.is_placeholder());
    }

    #[test]
    fn same_as_compares_every_field() {
        assert!(UNIT.same_as(&UNIT));
        assert!(!UNIT.same_as(&TensorSpec {
            channel_order: ChannelOrder::Bgr,
            ..UNIT
        }));
        // 同一个形状，一份静态 / 一份拥有所有权：契约必须判等。
        let owned = TensorSpec::observed(vec![1, 3, 2, 2], ValueRange::ZeroOne, ChannelOrder::Rgb);
        assert!(UNIT.same_as(&owned));
    }
}
