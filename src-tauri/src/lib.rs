//! LUMI RETOUCH 桌面端入口。
//!
//! 分层约定：
//! - `image_engine`：纯 Rust 统一图像引擎（16-bit 内部表示、调色、导出），不依赖 Tauri；
//! - `ai`：本地 ONNX 人脸检测（tract，纯 CPU）；
//! - `ai_engine`：皮肤区域、自然磨皮、瑕疵修复、肤色与眼周、一键分析；
//! - `commands`：Tauri 命令边界，只做参数校验、线程调度与序列化；
//! - `state`：进程内图片缓存与人脸结果缓存。

pub mod ai;
pub mod ai_engine;
pub mod error;
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
        ])
        .run(tauri::generate_context!())
        .expect("LUMI RETOUCH 启动失败");
}
