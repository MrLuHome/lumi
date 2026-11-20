//! LUMI RETOUCH 桌面端入口。
//!
//! 分层约定：
//! - `image_engine`：纯 Rust 统一图像引擎（16-bit 内部表示、调色、导出），不依赖 Tauri；
//! - `ai`：本地 ONNX 人脸检测（tract，纯 CPU）；
//! - `ai_engine`：皮肤区域、自然磨皮、瑕疵修复、肤色与眼周、一键分析；
//! - `face_reshape`：第四轮面部塑形（手动变形 + 轮廓级保守调整 + 质量保护）；
//! - `commands`：Tauri 命令边界，只做参数校验、线程调度与序列化；
//! - `state`：进程内图片缓存与人脸结果缓存。

pub mod ai;
pub mod ai_engine;
pub mod error;
pub mod face_reshape;
pub mod image_engine;
mod commands;
mod state;

pub use error::{AppError, AppResult};

use ai::FaceEngine;
use commands::render::{RenderCache, RenderRegistry};
use state::{FaceRepository, ImageCache};

pub fn run() {
    tauri::Builder::default()
        .manage(ImageCache::new(8))
        .manage(FaceRepository::default())
        .manage(RenderRegistry::default())
        .manage(RenderCache::default())
        .manage(FaceEngine::default())
        .manage(commands::analysis::AnalysisCache::default())
        .manage(commands::preset::PresetStore::new())
        .manage(commands::batch::BatchEngine::default())
        .invoke_handler(tauri::generate_handler![
            commands::library::pick_images,
            commands::library::load_images,
            commands::render::render_preview,
            commands::render::cancel_render,
            commands::render::export_image,
            commands::project::save_project,
            commands::project::open_project,
            commands::project::pick_lut,
            commands::project::auto_white_balance,
            commands::faces::face_model_status,
            commands::faces::detect_faces,
            commands::beauty::skin_model_status,
            commands::beauty::analyze_beauty,
            commands::beauty::preview_skin_mask,
            commands::reshape::reshape_capability,
            commands::reshape::reshape_mesh,
            commands::reshape::export_face_records,
            commands::reshape::restore_faces,
            commands::reshape::confirm_face_match,
            commands::segmentation::segmentation_capability,
            commands::segmentation::preview_subject_mask,
            commands::body::body_capability,
            commands::body::body_anchor_presets,
            commands::background::pick_background_image,
            commands::background::background_image_info,
            commands::analysis::analyze_retouch,
            commands::analysis::apply_retouch_suggestions,
            commands::preset::list_presets,
            commands::preset::apply_preset_to_photo,
            commands::preset::preset_previews,
            commands::preset::preset_preview,
            commands::preset::preset_params_from_edit,
            commands::preset::save_custom_preset,
            commands::preset::delete_custom_preset,
            commands::preset::duplicate_custom_preset,
            commands::preset::rename_custom_preset,
            commands::preset::export_preset,
            commands::preset::import_preset,
            commands::batch::batch_start,
            commands::batch::batch_status,
            commands::batch::batch_pause,
            commands::batch::batch_resume,
            commands::batch::batch_cancel_current,
            commands::batch::batch_cancel_all,
            commands::batch::batch_retry_failed,
            commands::batch::batch_clear_completed,
            commands::batch::batch_failures,
            commands::batch::pick_batch_output_dir,
        ])
        .run(tauri::generate_context!())
        .expect("LUMI RETOUCH 启动失败");
}
