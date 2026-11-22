//! 本地 YuNet 人脸检测引擎。
//!
//! R11 改动（全部是「转发」，不重写算法）：
//! 1. 加载走**统一准入链**（许可 → 存在 → 格式 → 契约 → SHA256 → 解析），
//!    由 `ai_engine::model_registry` 与 `ai_engine::inference` 提供；
//! 2. 张量 IO 走统一 `TensorSpec` / `TensorBatch`，不再手工拼裸 `Vec<f32>`；
//! 3. 新增 `detect_with_cancel`（**加法**）：旧 `detect` 转调空令牌，三处调用方零改动；
//! 4. `status()` 真正填写 `error`（区分“文件缺失”与“文件损坏”）。
//!
//! `plan.run()` 内部**无法中断**（tract 0.23 不暴露中断钩子），因此取消只在
//! 阶段边界生效：加载前后、预处理每 1/8、推理前后、后处理循环内。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::imageops::FilterType;
use serde::Serialize;

use crate::ai::model::{DETECT_SIZE, MODEL_OUTPUTS, NMS_THRESHOLD, SCORE_THRESHOLD};
use crate::ai::postprocess::{self, Candidate};
use crate::ai_engine::inference::{
    CpuTractBackend, InferenceBackend, InferenceError, LoadedPlan, RunOptions, TensorBatch,
};
use crate::ai_engine::model_registry::{builtin::YUNET, verify};
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;

/// 16 位内部表示 → 0..255：`u16::MAX / 257.0 == 255.0`。
/// 这个常量与 `YUNET.input.value_range == Raw255` 严格对应。
const PIXEL_SCALE: f32 = 257.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceModelStatus {
    pub available: bool,
    pub loaded: bool,
    pub name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub sha256: String,
    pub input: String,
    pub outputs: String,
    pub path: Option<String>,
    /// R11 起真正填写：缺失 / 体积不符 / 格式不符 都会给出可判别的说明。
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFace {
    pub id: usize,
    /// 稳定人脸 UUID，由跟踪器/项目恢复分配。
    #[serde(default)]
    pub uuid: String,
    pub score: f32,
    /// 原图像素坐标。
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub landmarks: Vec<FacePoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDetectionResult {
    pub faces: Vec<DetectedFace>,
    pub image_width: u32,
    pub image_height: u32,
    pub elapsed_ms: u64,
    pub inference_ms: u64,
    pub model: String,
}

/// 本地 YuNet 人脸检测引擎。模型只在首次检测时加载并缓存，
/// 不联网、不调用外部服务。
#[derive(Clone, Default)]
pub struct FaceEngine {
    inner: Arc<Mutex<Option<Arc<LoadedPlan>>>>,
}

impl FaceEngine {
    /// 查询模型状态。使用**浅检查**（存在性 + 体积 + 格式头），
    /// 不做整文件哈希 —— 该命令是同步 Tauri 命令，不能有毫秒级 IO。
    pub fn status(&self, model_path: Option<&Path>) -> FaceModelStatus {
        let loaded = self.is_loaded();
        let (available, error) = match model_path {
            Some(path) => {
                let health = verify::check_shallow(&YUNET, path);
                let error = health.blocking.as_ref().map(|failure| match failure.detail() {
                    Some(detail) => format!("{}：{detail}", failure.message()),
                    None => failure.message(),
                });
                (health.blocking.is_none(), error)
            }
            None => (false, Some("未能解析到模型资源目录".to_string())),
        };
        FaceModelStatus {
            available,
            loaded,
            name: YUNET.display_name.into(),
            version: YUNET.version.into(),
            license: YUNET.license.spdx.unwrap_or("未核实").into(),
            source: YUNET.provenance.weights_url.into(),
            sha256: YUNET.sha256.into(),
            input: YUNET.input.format_human(),
            outputs: MODEL_OUTPUTS.into(),
            path: model_path.map(|path| path.to_string_lossy().into_owned()),
            error,
        }
    }

    /// 是否已把 plan 载入内存。
    pub fn is_loaded(&self) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// 走完整准入链并加载（旧签名，错误降级为 `AppError`，保持调用方不变）。
    pub fn load(&self, model_path: &Path) -> AppResult<()> {
        self.load_checked(model_path).map_err(AppError::from)
    }

    /// 走完整准入链并加载，返回结构化错误（新命令层使用）。
    pub fn load_checked(&self, model_path: &Path) -> Result<(), InferenceError> {
        self.load_with_cancel(model_path, &CancelToken::default())
    }

    /// 带取消的加载。
    pub fn load_with_cancel(
        &self,
        model_path: &Path,
        cancel: &CancelToken,
    ) -> Result<(), InferenceError> {
        if self.is_loaded() {
            return Ok(());
        }
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        // ① 许可 → ② 存在 → ③ 体积/格式 → ⑤ SHA256。
        // 契约（④）由后端在解析后补验：这里若也解析一次，232 KB 权重会被解析两遍。
        let health = verify::check_at(&YUNET, model_path, false);
        if let Some(error) = health.error() {
            return Err(error);
        }
        cancel.check().map_err(|_| InferenceError::Cancelled)?;
        // ⑥ 解析 + 优化 + 编译（后端内部再校验一次契约）。
        let plan = CpuTractBackend::new().load(&YUNET, model_path, cancel)?;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| InferenceError::Runtime("人脸模型状态异常".into()))?;
        *guard = Some(Arc::new(plan));
        Ok(())
    }

    /// 释放已加载的 plan（GPU 后端未来需要显式释放显存；CPU 路径只是 drop）。
    pub fn unload(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    fn plan(&self) -> Result<Arc<LoadedPlan>, InferenceError> {
        self.inner
            .lock()
            .map_err(|_| InferenceError::Runtime("人脸模型状态异常".into()))?
            .clone()
            .ok_or_else(|| InferenceError::ModelLoadFailed {
                model_id: YUNET.id.to_string(),
                detail: "人脸模型尚未加载".into(),
            })
    }

    /// 在预览图上推理，结果映射回原图像素坐标。
    ///
    /// 签名与行为保持不变：内部转调 `detect_with_cancel` 并传入**空令牌**（永不取消）。
    pub fn detect(
        &self,
        model_path: &Path,
        preview: &Image16,
        original_width: u32,
        original_height: u32,
        progress: &mut dyn FnMut(&'static str, f32),
    ) -> AppResult<FaceDetectionResult> {
        self.detect_with_cancel(
            model_path,
            preview,
            original_width,
            original_height,
            progress,
            &CancelToken::default(),
        )
    }

    /// 可取消的推理。取消发生在阶段边界，**不能**中断已经在跑的 `plan.run()`。
    pub fn detect_with_cancel(
        &self,
        model_path: &Path,
        preview: &Image16,
        original_width: u32,
        original_height: u32,
        progress: &mut dyn FnMut(&'static str, f32),
        cancel: &CancelToken,
    ) -> AppResult<FaceDetectionResult> {
        self.detect_checked(
            model_path,
            preview,
            original_width,
            original_height,
            progress,
            cancel,
        )
        .map_err(AppError::from)
    }

    /// 与 [`Self::detect_with_cancel`] 相同，但返回结构化错误（`E_CANCELLED` 等）。
    pub fn detect_checked(
        &self,
        model_path: &Path,
        preview: &Image16,
        original_width: u32,
        original_height: u32,
        progress: &mut dyn FnMut(&'static str, f32),
        cancel: &CancelToken,
    ) -> Result<FaceDetectionResult, InferenceError> {
        self.detect_inner(
            model_path,
            preview,
            original_width,
            original_height,
            progress,
            cancel,
        )
    }

    fn detect_inner(
        &self,
        model_path: &Path,
        preview: &Image16,
        original_width: u32,
        original_height: u32,
        progress: &mut dyn FnMut(&'static str, f32),
        cancel: &CancelToken,
    ) -> Result<FaceDetectionResult, InferenceError> {
        let started = Instant::now();
        progress("加载模型", 0.05);
        self.load_with_cancel(model_path, cancel)?;
        let plan = self.plan()?;
        cancel.check().map_err(|_| InferenceError::Cancelled)?;

        progress("预处理", 0.25);
        let resized =
            image::imageops::resize(preview, DETECT_SIZE, DETECT_SIZE, FilterType::Triangle);
        let plane = (DETECT_SIZE * DETECT_SIZE) as usize;
        let mut input = vec![0f32; 3 * plane];
        // YuNet 使用 BGR、0..255 输入（OpenCV 训练口径）。
        // 16 位内部表示除以 257 后恰好落在 0..255，与登记表声明的
        // `ValueRange::Raw255` 一致 —— 这正是 R9 风险的反面证据。
        let checkpoint = (plane / 8).max(1);
        for (index, pixel) in resized.pixels().enumerate() {
            if index % checkpoint == 0 {
                cancel.check().map_err(|_| InferenceError::Cancelled)?;
            }
            let value = pixel.0;
            input[index] = value[2] as f32 / PIXEL_SCALE;
            input[plane + index] = value[1] as f32 / PIXEL_SCALE;
            input[2 * plane + index] = value[0] as f32 / PIXEL_SCALE;
        }
        let tensor = TensorBatch::new_for(YUNET.id, YUNET.input, input)?;

        progress("推理", 0.45);
        let inference_started = Instant::now();
        let outputs = CpuTractBackend::new().run(
            &plan,
            &tensor,
            &RunOptions::cpu_default(),
            cancel,
        )?;
        let inference_ms = inference_started.elapsed().as_millis() as u64;
        progress("后处理", 0.88);

        let vectors: Vec<Vec<f32>> = outputs.into_iter().map(|batch| batch.data).collect();
        let candidates = postprocess::decode_all(&vectors, SCORE_THRESHOLD, NMS_THRESHOLD);
        let scale_x = original_width as f32 / DETECT_SIZE as f32;
        let scale_y = original_height as f32 / DETECT_SIZE as f32;
        let mut faces: Vec<DetectedFace> = candidates
            .iter()
            .map(|candidate| to_face(&postprocess::scale_to_original(candidate, scale_x, scale_y)))
            .collect();
        // 从左到右排序，前端人脸切换顺序稳定。
        faces.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        for (index, face) in faces.iter_mut().enumerate() {
            face.id = index;
        }

        progress("完成", 1.0);
        Ok(FaceDetectionResult {
            faces,
            image_width: original_width,
            image_height: original_height,
            elapsed_ms: started.elapsed().as_millis() as u64,
            inference_ms,
            model: YUNET.display_name.into(),
        })
    }
}

fn to_face(candidate: &Candidate) -> DetectedFace {
    DetectedFace {
        id: 0,
        uuid: String::new(),
        score: candidate.score,
        x: candidate.left().max(0.0),
        y: candidate.top().max(0.0),
        width: candidate.width,
        height: candidate.height,
        landmarks: candidate
            .landmarks
            .iter()
            .map(|point| FacePoint {
                x: point[0],
                y: point[1],
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::buffer::Image16;
    use image::Rgba;
    use std::sync::atomic::AtomicBool;

    fn model_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(YUNET.dir)
            .join(YUNET.file_name)
    }

    fn blank_preview() -> Image16 {
        Image16::from_pixel(64, 64, Rgba([32768u16, 32768, 32768, 65535]))
    }

    #[test]
    fn status_reports_a_concrete_error_when_the_file_is_missing() {
        let engine = FaceEngine::default();
        let status = engine.status(Some(Path::new("/nope/face_detection_yunet_2023mar.onnx")));
        assert!(!status.available);
        assert!(!status.loaded);
        let error = status.error.expect("缺失时必须给出可判别的错误");
        assert!(error.contains("缺失"), "实际：{error}");
        assert_eq!(status.sha256, YUNET.sha256);
    }

    #[test]
    fn status_reports_available_for_the_real_model() {
        let engine = FaceEngine::default();
        let status = engine.status(Some(&model_path()));
        assert!(status.available, "error={:?}", status.error);
        assert!(status.error.is_none());
        assert_eq!(status.input, YUNET.input.format_human());
        assert_eq!(status.license, "MIT");
    }

    #[test]
    fn load_rejects_a_corrupted_file_with_a_model_error() {
        let engine = FaceEngine::default();
        let error = engine
            .load_checked(Path::new("/nope/face_detection_yunet_2023mar.onnx"))
            .unwrap_err();
        assert_eq!(error.code(), "E_MODEL_MISSING");
    }

    #[test]
    fn cancelled_token_stops_before_loading() {
        let engine = FaceEngine::default();
        let cancel = CancelToken::new(Arc::new(AtomicBool::new(true)));
        let error = engine.load_with_cancel(&model_path(), &cancel).unwrap_err();
        assert_eq!(error.code(), "E_CANCELLED");
        assert!(!engine.is_loaded());
    }

    /// 相对 epsilon 比较：两次推理走同一条确定性 CPU 路径，逐位相等是常态，
    /// 但不能假设编译器的浮点重排永远一致，故给一个按量级缩放的上界。
    fn close(left: f32, right: f32) -> bool {
        let epsilon = f32::EPSILON * 8.0;
        (left - right).abs() <= epsilon * left.abs().max(right.abs()).max(1.0)
    }

    /// 旧 `detect` 与 `detect_with_cancel` 必须完全等价。
    ///
    /// ⚠️ 必须用**真实人像**：旧版用 64×64 纯灰图，实测检出 0 张脸，
    /// 断言退化成 `0 == 0`，是一条第永远绿的空转测试（QA 指出，已修）。
    #[test]
    fn detect_and_detect_with_cancel_are_equivalent_on_a_real_portrait() {
        let engine = FaceEngine::default();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/images/portrait.jpg");
        let decoded = crate::image_engine::decoder::decode(&fixture).expect("fixture 缺失");
        let preview = crate::image_engine::buffer::fit_within(&decoded.image, 1800);
        let width = decoded.image.width();
        let height = decoded.image.height();

        let legacy = engine
            .detect(&model_path(), &preview, width, height, &mut |_, _| {})
            .unwrap();
        let explicit = engine
            .detect_with_cancel(
                &model_path(),
                &preview,
                width,
                height,
                &mut |_, _| {},
                &CancelToken::default(),
            )
            .unwrap();

        // 先证明这次比较有意义：真实人像必须检出人脸。
        assert!(
            !legacy.faces.is_empty(),
            "真实人像未检出人脸，本测试会退化成 0 == 0"
        );
        assert_eq!(legacy.faces.len(), explicit.faces.len());
        assert_eq!(legacy.model, explicit.model);
        assert_eq!(legacy.image_width, explicit.image_width);
        assert_eq!(legacy.image_height, explicit.image_height);

        // 逐字段比较（耗时字段按定义必然不同，不参与比较）。
        for (left, right) in legacy.faces.iter().zip(explicit.faces.iter()) {
            assert_eq!(left.id, right.id);
            assert_eq!(left.uuid, right.uuid);
            assert!(close(left.score, right.score), "score {} != {}", left.score, right.score);
            assert!(close(left.x, right.x), "x {} != {}", left.x, right.x);
            assert!(close(left.y, right.y), "y {} != {}", left.y, right.y);
            assert!(close(left.width, right.width));
            assert!(close(left.height, right.height));
            assert_eq!(left.landmarks.len(), right.landmarks.len());
            for (left, right) in left.landmarks.iter().zip(right.landmarks.iter()) {
                assert!(close(left.x, right.x));
                assert!(close(left.y, right.y));
            }
        }
    }

    #[test]
    fn cancelled_detection_returns_e_cancelled_without_panicking() {
        let engine = FaceEngine::default();
        let preview = blank_preview();
        let cancel = CancelToken::new(Arc::new(AtomicBool::new(true)));
        let error = engine
            .detect_checked(&model_path(), &preview, 64, 64, &mut |_, _| {}, &cancel)
            .unwrap_err();
        assert_eq!(error.code(), "E_CANCELLED");
        // 旧签名路径同样不 panic，只是降级为 AppError。
        let legacy = engine.detect_with_cancel(
            &model_path(),
            &preview,
            64,
            64,
            &mut |_, _| {},
            &cancel,
        );
        assert!(legacy.is_err());
    }
}
