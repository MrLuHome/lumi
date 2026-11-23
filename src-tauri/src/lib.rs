//! LUMI RETOUCH 桌面端入口。
//!
//! 分层约定：
//! - `image_engine`：纯 Rust 统一图像引擎（16-bit 内部表示、调色、导出），不依赖 Tauri；
//! - `ai`：本地 ONNX 人脸检测（tract，纯 CPU）；
//! - `ai_engine`：皮肤区域、自然磨皮、瑕疵修复、肤色与眼周、一键分析；
//! - `face_reshape`：第四轮面部塑形（手动变形 + 轮廓级保守调整 + 质量保护）；
//! - `commands`：Tauri 命令边界，只做参数校验、线程调度与序列化；
//! - `state`：进程内图片缓存与人脸结果缓存。

// 测试里用 `json!` 构造完整项目文档，默认 128 层展开不够。
#![recursion_limit = "256"]
// 图像处理与 Tauri 命令中，显式索引/多参数签名可读性更好；这些风格 lint
// 由 Rust 1.95 的 clippy 新增，沿用既有代码约定，按 lint 名称显式放行。
#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::excessive_precision,
    clippy::field_reassign_with_default,
    clippy::useless_vec,
    clippy::derivable_impls,
    clippy::default_constructed_unit_structs,
    clippy::explicit_into_iter_loop,
    clippy::redundant_clone,
    clippy::manual_is_multiple_of
)]

pub mod ai;
pub mod ai_engine;
pub mod color_management;
mod commands;
pub mod error;
pub mod face_intelligence;
pub mod face_reshape;
pub mod gpu;
pub mod image_engine;
mod state;

pub use error::{AppError, AppResult};

use ai::FaceEngine;
use commands::render::{RenderCache, RenderRegistry};
use state::{FaceRepository, ImageCache};
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 人脸智能层：Provider 注册表 + 模型目录 + 隔离探针入口。
            // 模型目录放在应用数据目录，避免写安装包内部；不存在时自动创建。
            let yunet = commands::faces::model_path(app.handle());
            let yunet = yunet.is_file().then_some(yunet);
            let models_root = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("face-models"))
                .unwrap_or_else(|_| std::env::temp_dir().join("lumi-face-models"));
            let executable = std::env::current_exe().ok();
            app.manage(std::sync::Arc::new(
                face_intelligence::FaceIntelligence::new(yunet, models_root.clone(), executable),
            ));
            // R11 统一模型注册表：权威登记层。用户自备模型的 manifest 目录就是
            // model_hub 的根目录，registry 只读它（单向依赖，不反向）。
            app.manage(std::sync::Arc::new(ai_engine::model_registry::Registry::new(
                Some(models_root),
            )));
            // 皮肤语义分割权重的打包资源目录：只在命令层取 `resource_dir()`，
            // 推理层保持零 Tauri 依赖。首张人脸推理时才真正加载（后台线程）。
            if let Ok(resource) = app.path().resource_dir() {
                ai_engine::skin_segmentation::onnx::configure_base_dirs(vec![
                    resource.join("resources"),
                    resource,
                ]);
            }
            // 关闭窗口前检查未保存修改：确认后才真正退出。
            let guard = app
                .state::<commands::project::UnsavedGuard>()
                .0
                .clone();
            if let Some(window) = app.get_webview_window("main") {
                let handle = window.clone();
                let flag = guard.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if !flag.load(std::sync::atomic::Ordering::Relaxed) {
                            return;
                        }
                        api.prevent_close();
                        if commands::project::confirm_discard() {
                            flag.store(false, std::sync::atomic::Ordering::Relaxed);
                            let _ = handle.close();
                        }
                    }
                });
            }
            Ok(())
        })
        .manage(commands::project::UnsavedGuard::default())
        .manage(ImageCache::new(8))
        .manage(FaceRepository::default())
        .manage(RenderRegistry::default())
        .manage(RenderCache::default())
        .manage(FaceEngine::default())
        .manage(commands::analysis::AnalysisCache::default())
        .manage(commands::preset::PresetStore::new())
        .manage(commands::batch::BatchEngine::default())
        .manage(color_management::ColorEngine::new())
        .manage(gpu::ComputeEngine::new())
        .invoke_handler(tauri::generate_handler![
            commands::library::pick_images,
            commands::library::load_images,
            commands::render::render_preview,
            commands::render::cancel_render,
            commands::render::export_image,
            commands::project::save_project,
            commands::project::set_unsaved_changes,
            commands::project::apply_project_changeset,
            commands::project::open_project,
            commands::project::pick_lut,
            commands::project::auto_white_balance,
            commands::faces::face_model_status,
            commands::faces::detect_faces,
            commands::beauty::skin_model_status,
            commands::beauty::analyze_beauty,
            commands::beauty::preview_skin_mask,
            commands::model_status::model_registry_snapshot,
            commands::model_status::model_registry_check,
            commands::model_status::model_load,
            commands::model_status::model_unload,
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
            commands::raw::raw_info,
            commands::raw::raw_preview,
            commands::raw::raw_develop,
            commands::raw::raw_capabilities,
            commands::raw::raw_probe,
            commands::color::color_status,
            commands::color::color_profiles,
            commands::color::color_set_output,
            commands::color::color_check_profile,
            commands::color::soft_proof,
            commands::color::display_preview,
            commands::color::color_convert_cmyk,
            commands::color::display_profile_refresh,
            commands::gpu::gpu_status,
            commands::gpu::gpu_benchmark,
            commands::gpu::set_compute_backend,
            commands::face_intelligence::list_face_providers,
            commands::face_intelligence::face_intelligence_status,
            commands::face_intelligence::select_face_provider,
            commands::face_intelligence::face_quality_thresholds,
            commands::face_intelligence::last_face_analysis,
            commands::face_intelligence::analyze_photo_faces,
            commands::face_intelligence::probe_face_quality,
            commands::face_intelligence::list_face_models,
            commands::face_intelligence::pick_face_model_file,
            commands::face_intelligence::import_face_model,
            commands::face_intelligence::revalidate_face_model,
            commands::face_intelligence::remove_face_model,
        ])
        .build(tauri::generate_context!())
        .expect("LUMI RETOUCH 启动失败")
        .run(|app_handle, event| {
            // ⌘Q 或系统退出不经过窗口 CloseRequested，这里再保护一次。
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_some() {
                    return;
                }
                let guard = app_handle
                    .state::<commands::project::UnsavedGuard>()
                    .0
                    .clone();
                if !guard.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                api.prevent_exit();
                if commands::project::confirm_discard() {
                    guard.store(false, std::sync::atomic::Ordering::Relaxed);
                    app_handle.exit(0);
                }
            }
        });
}
