//! LUMI RETOUCH 桌面端入口。
//!
//! 分层约定：
//! - `imaging`：纯 Rust 图像管线，不依赖 Tauri，可独立单元测试；
//! - `commands`：Tauri 命令边界，只做参数校验、线程调度与序列化；
//! - `state`：进程内图片缓存，避免滑杆调整时反复解码原图。

mod commands;
mod error;
mod imaging;
mod state;

pub use error::{AppError, AppResult};

use state::ImageCache;

pub fn run() {
    tauri::Builder::default()
        .manage(ImageCache::new(6))
        .invoke_handler(tauri::generate_handler![
            commands::library::pick_images,
            commands::library::load_images,
            commands::render::render_preview,
            commands::render::export_image,
        ])
        .run(tauri::generate_context!())
        .expect("LUMI RETOUCH 启动失败");
}
