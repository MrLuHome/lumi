//! 变形保护蒙版：允许位移权重 w ∈ [0,1]。
//!
//! - `w = 0`：完全保护（背景 / 五官 / 用户保护笔…）
//! - `w = 1`：允许变形
//!
//! 多层合成取 **min**（最严保护优先），与 `FaceGuard::weight` 语义一致。
//! **禁止**用保护蒙版实现「整图横向压缩」——保护只做减法，不提供全局缩放。

/// 完全允许位移。
pub const PROTECTION_FULL_ALLOW: f32 = 1.0;
/// 完全保护。
pub const PROTECTION_FULL_BLOCK: f32 = 0.0;

/// 与图像同尺寸的允许位移权重图（行优先，长度 = width * height）。
#[derive(Debug, Clone, PartialEq)]
pub struct ProtectionMask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<f32>,
}

impl ProtectionMask {
    pub fn full_allow(width: u32, height: u32) -> Self {
        let len = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            alpha: vec![PROTECTION_FULL_ALLOW; len],
        }
    }

    pub fn full_block(width: u32, height: u32) -> Self {
        let len = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            alpha: vec![PROTECTION_FULL_BLOCK; len],
        }
    }

    pub fn from_alpha(width: u32, height: u32, alpha: Vec<f32>) -> Option<Self> {
        let expected = (width as usize).saturating_mul(height as usize);
        if width == 0 || height == 0 || alpha.len() != expected {
            return None;
        }
        Some(Self {
            width,
            height,
            alpha: alpha.into_iter().map(|v| v.clamp(0.0, 1.0)).collect(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.alpha.is_empty()
    }

    pub fn at(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return PROTECTION_FULL_BLOCK;
        }
        self.alpha[(y as usize) * (self.width as usize) + (x as usize)].clamp(0.0, 1.0)
    }

    /// 与另一蒙版逐点取 min（最严保护优先）。尺寸必须一致。
    pub fn intersect(&self, other: &ProtectionMask) -> ProtectionMask {
        if self.width != other.width || self.height != other.height || self.is_empty() {
            return if self.is_empty() {
                other.clone()
            } else {
                self.clone()
            };
        }
        let alpha = self
            .alpha
            .iter()
            .zip(other.alpha.iter())
            .map(|(a, b)| (*a).min(*b).clamp(0.0, 1.0))
            .collect();
        ProtectionMask {
            width: self.width,
            height: self.height,
            alpha,
        }
    }
}

/// 保护层语义标签（用于日志 / 报告 / 分层构建）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectionLayer {
    /// 背景（人物蒙版外）。
    Background,
    /// 五官（眼/眉/鼻/口）。
    Features,
    /// 头发。
    Hair,
    /// 衣服（G12+；可为空层）。
    Clothes,
    /// 手部（G12+；可为空层）。
    Hands,
    /// 人体边缘 ramp。
    BodyEdge,
    /// 用户「保护」笔触。
    UserProtect,
}

impl ProtectionLayer {
    pub fn id(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Features => "features",
            Self::Hair => "hair",
            Self::Clothes => "clothes",
            Self::Hands => "hands",
            Self::BodyEdge => "bodyEdge",
            Self::UserProtect => "userProtect",
        }
    }
}

/// 逐层合成：全部层取 min。无层时返回全允许。
pub fn compose_protection(width: u32, height: u32, layers: &[ProtectionMask]) -> ProtectionMask {
    if layers.is_empty() {
        return ProtectionMask::full_allow(width, height);
    }
    let mut result = layers[0].clone();
    for layer in &layers[1..] {
        result = result.intersect(layer);
    }
    // 若首层尺寸异常，intersect 已兜底；再保证尺寸。
    if result.width != width || result.height != height {
        if let Some(fixed) = ProtectionMask::from_alpha(width, height, result.alpha.clone()) {
            // 长度不匹配时 from_alpha 会失败，保持原样由调用方校验。
            if fixed.width == width {
                return fixed;
            }
        }
    }
    result
}

/// 把保护权重乘到位移上。`index = y * width + x`。
pub fn apply_to_delta(delta: [f32; 2], weight: f32) -> [f32; 2] {
    let w = weight.clamp(0.0, 1.0);
    [delta[0] * w, delta[1] * w]
}

/// 由现有语义快速构造一层（测试与 G13 适配用）：
/// - `block_rects`：完全保护的归一化矩形 (x,y,w,h)
/// - 其余像素默认 allow
pub fn mask_from_blocked_rects(
    width: u32,
    height: u32,
    block_rects: &[(f32, f32, f32, f32)],
) -> ProtectionMask {
    let mut mask = ProtectionMask::full_allow(width, height);
    if width == 0 || height == 0 {
        return mask;
    }
    for &(x, y, w, h) in block_rects {
        let x0 = (x * width as f32).floor().max(0.0) as u32;
        let y0 = (y * height as f32).floor().max(0.0) as u32;
        let x1 = ((x + w) * width as f32).ceil().min(width as f32) as u32;
        let y1 = ((y + h) * height as f32).ceil().min(height as f32) as u32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let idx = (yy as usize) * (width as usize) + (xx as usize);
                if let Some(slot) = mask.alpha.get_mut(idx) {
                    *slot = PROTECTION_FULL_BLOCK;
                }
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_takes_stricter_min() {
        let mut a = ProtectionMask::full_allow(4, 4);
        let mut b = ProtectionMask::full_allow(4, 4);
        // a 保护 (0,0)，b 保护 (1,0)；合成后两处都应为 0，其余为 1。
        a.alpha[0] = 0.0;
        b.alpha[1] = 0.0;
        let c = compose_protection(4, 4, &[a, b]);
        assert_eq!(c.at(0, 0), 0.0);
        assert_eq!(c.at(1, 0), 0.0);
        assert_eq!(c.at(2, 0), 1.0);
    }

    #[test]
    fn apply_to_delta_zero_weight_blocks() {
        assert_eq!(apply_to_delta([0.1, -0.2], 0.0), [0.0, 0.0]);
        let d = apply_to_delta([0.1, -0.2], 0.5);
        assert!((d[0] - 0.05).abs() < 1e-6);
        assert!((d[1] + 0.1).abs() < 1e-6);
    }

    #[test]
    fn blocked_rects_create_hard_protection() {
        let mask = mask_from_blocked_rects(10, 10, &[(0.0, 0.0, 0.2, 0.2)]);
        assert_eq!(mask.at(0, 0), 0.0);
        assert_eq!(mask.at(1, 1), 0.0);
        assert_eq!(mask.at(5, 5), 1.0);
    }

    #[test]
    fn empty_layers_means_full_allow() {
        let m = compose_protection(2, 2, &[]);
        assert_eq!(m.at(0, 0), 1.0);
        assert_eq!(m.at(1, 1), 1.0);
    }

    #[test]
    fn from_alpha_rejects_bad_len() {
        assert!(ProtectionMask::from_alpha(2, 2, vec![1.0; 3]).is_none());
        assert!(ProtectionMask::from_alpha(2, 2, vec![1.0; 4]).is_some());
    }
}
