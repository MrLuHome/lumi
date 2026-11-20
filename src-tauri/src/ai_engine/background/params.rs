//! 背景模式辅助函数。真正的参数结构与夹紧在 `image_engine::params`，
//! 这里只做模式标签与「是否替换背景」的判定，避免前后端各写一套。

use crate::image_engine::params::{BackgroundMode, BackgroundParams};

pub fn mode_label(mode: BackgroundMode) -> &'static str {
    match mode {
        BackgroundMode::Original => "original",
        BackgroundMode::Blur => "blur",
        BackgroundMode::SolidColor => "solidColor",
        BackgroundMode::Gradient => "gradient",
        BackgroundMode::Image => "image",
    }
}

pub fn is_replacement(settings: &BackgroundParams) -> bool {
    matches!(
        settings.mode,
        BackgroundMode::SolidColor | BackgroundMode::Gradient | BackgroundMode::Image
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_have_stable_labels() {
        assert_eq!(mode_label(BackgroundMode::SolidColor), "solidColor");
        assert_eq!(mode_label(BackgroundMode::Blur), "blur");
    }

    #[test]
    fn replacement_modes_exclude_blur() {
        let mut settings = BackgroundParams::default();
        settings.mode = BackgroundMode::Blur;
        assert!(!is_replacement(&settings));
        settings.mode = BackgroundMode::Gradient;
        assert!(is_replacement(&settings));
    }
}
