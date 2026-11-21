//! 第四轮面部塑形预设。前端按钮使用同一组数值，默认全部温和。

use crate::image_engine::params::{FaceReshape, FaceShape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Natural,
    Delicate,
    Defined,
}

impl Preset {
    pub fn id(&self) -> &'static str {
        match self {
            Preset::Natural => "natural",
            Preset::Delicate => "delicate",
            Preset::Defined => "defined",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Preset::Natural => "自然",
            Preset::Delicate => "精致",
            Preset::Defined => "立体",
        }
    }
}

pub fn params(face_id: u32, preset: Preset) -> FaceReshape {
    let mut output = FaceReshape::new(face_id);
    match preset {
        Preset::Natural => {
            output.face = FaceShape {
                slim: 18.0,
                jaw: 10.0,
                length: 4.0,
                ..Default::default()
            };
        }
        Preset::Delicate => {
            output.face = FaceShape {
                slim: 28.0,
                cheekbone: 10.0,
                jaw: 18.0,
                length: 6.0,
                ..Default::default()
            };
        }
        Preset::Defined => {
            output.face = FaceShape {
                slim: 38.0,
                cheekbone: 18.0,
                jaw: 26.0,
                length: 8.0,
                ..Default::default()
            };
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_active_and_ordered() {
        for preset in [Preset::Natural, Preset::Delicate, Preset::Defined] {
            let params = params(3, preset);
            assert_eq!(params.face_id, 3);
            assert!(params.is_active());
            assert!(!preset.label().is_empty());
        }
        assert!(params(1, Preset::Defined).face.slim > params(1, Preset::Natural).face.slim);
    }

    #[test]
    fn presets_never_exceed_ui_range() {
        for preset in [Preset::Natural, Preset::Delicate, Preset::Defined] {
            let params = params(1, preset);
            for value in params.face.fields() {
                assert!(value.abs() <= 100.0);
            }
            assert!(!params.manual.enabled);
            // 预设只使用 LEVEL 1/2 能力，不得夹带 LEVEL 3 参数。
            assert_eq!(params.eyes.size, 0.0);
            assert_eq!(params.nose.alar, 0.0);
            assert_eq!(params.mouth.width, 0.0);
            assert_eq!(params.chin.length, 0.0);
        }
    }
}
