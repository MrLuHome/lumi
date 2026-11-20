use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::imageops::FilterType;
use serde::Serialize;
use tract_onnx::prelude::*;

use crate::ai::model::{
    sha256_hex, DETECT_SIZE, MODEL_INPUT, MODEL_LICENSE, MODEL_NAME, MODEL_OUTPUTS, MODEL_SHA256,
    MODEL_SOURCE, MODEL_VERSION, NMS_THRESHOLD, SCORE_THRESHOLD,
};
use crate::ai::postprocess::{self, Candidate};
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

struct LoadedModel {
    plan: Arc<TypedRunnableModel>,
}

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
    inner: Arc<Mutex<Option<Arc<LoadedModel>>>>,
}

impl FaceEngine {
    pub fn status(&self, model_path: Option<&Path>) -> FaceModelStatus {
        let loaded = self.inner.lock().map(|guard| guard.is_some()).unwrap_or(false);
        FaceModelStatus {
            available: model_path.map(|path| path.exists()).unwrap_or(false),
            loaded,
            name: MODEL_NAME.into(),
            version: MODEL_VERSION.into(),
            license: MODEL_LICENSE.into(),
            source: MODEL_SOURCE.into(),
            sha256: MODEL_SHA256.into(),
            input: MODEL_INPUT.into(),
            outputs: MODEL_OUTPUTS.into(),
            path: model_path.map(|path| path.to_string_lossy().into_owned()),
            error: None,
        }
    }

    pub fn load(&self, model_path: &Path) -> AppResult<()> {
        {
            if let Ok(guard) = self.inner.lock() {
                if guard.is_some() {
                    return Ok(());
                }
            }
        }
        let bytes = std::fs::read(model_path).map_err(|error| {
            AppError::Task(format!(
                "未找到人脸模型 {}：{error}。请确认安装包 resources 中包含模型文件",
                model_path.display()
            ))
        })?;
        let digest = sha256_hex(&bytes);
        if !digest.eq_ignore_ascii_case(MODEL_SHA256) {
            return Err(AppError::Unsupported(format!(
                "人脸模型 SHA256 校验失败（期望 {MODEL_SHA256}，实际 {digest}）"
            )));
        }

        let plan = tract_onnx::onnx()
            .model_for_path(model_path)
            .map_err(|error| AppError::Task(format!("人脸模型解析失败：{error}")))?
            .into_optimized()
            .map_err(|error| AppError::Task(format!("人脸模型优化失败：{error}")))?
            .into_runnable()
            .map_err(|error| AppError::Task(format!("人脸模型编译失败：{error}")))?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AppError::Task("人脸模型状态异常".into()))?;
        *guard = Some(Arc::new(LoadedModel { plan }));
        Ok(())
    }

    fn model(&self) -> AppResult<Arc<LoadedModel>> {
        self.inner
            .lock()
            .map_err(|_| AppError::Task("人脸模型状态异常".into()))?
            .clone()
            .ok_or_else(|| AppError::Task("人脸模型尚未加载".into()))
    }

    /// 在预览图上推理，结果映射回原图像素坐标。
    pub fn detect(
        &self,
        model_path: &Path,
        preview: &Image16,
        original_width: u32,
        original_height: u32,
        progress: &mut dyn FnMut(&'static str, f32),
    ) -> AppResult<FaceDetectionResult> {
        let started = Instant::now();
        progress("加载模型", 0.05);
        self.load(model_path)?;
        let model = self.model()?;
        progress("预处理", 0.25);

        let resized = image::imageops::resize(
            preview,
            DETECT_SIZE,
            DETECT_SIZE,
            FilterType::Triangle,
        );
        let mut input = vec![0f32; 3 * (DETECT_SIZE * DETECT_SIZE) as usize];
        let plane = (DETECT_SIZE * DETECT_SIZE) as usize;
        // YuNet 使用 BGR、0..255 输入（OpenCV 训练口径），这里按同一口径喂入。
        for (index, pixel) in resized.pixels().enumerate() {
            let value = pixel.0;
            input[index] = value[2] as f32 / 257.0;
            input[plane + index] = value[1] as f32 / 257.0;
            input[2 * plane + index] = value[0] as f32 / 257.0;
        }

        let tensor = tract_ndarray::Array4::from_shape_vec(
            (1, 3, DETECT_SIZE as usize, DETECT_SIZE as usize),
            input,
        )
        .map_err(|error| AppError::Task(format!("人脸输入张量构建失败：{error}")))?
        .into_tensor();

        progress("推理", 0.45);
        let inference_started = Instant::now();
        let outputs = model
            .plan
            .run(tvec!(tensor.into_tvalue()))
            .map_err(|error| AppError::Task(format!("人脸推理失败：{error}")))?;
        let inference_ms = inference_started.elapsed().as_millis() as u64;
        progress("后处理", 0.88);

        let mut vectors = Vec::with_capacity(outputs.len());
        for output in outputs.iter() {
            let view = output
                .to_plain_array_view::<f32>()
                .map_err(|error| AppError::Task(format!("人脸输出解析失败：{error}")))?;
            vectors.push(view.iter().copied().collect::<Vec<f32>>());
        }

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
            model: MODEL_NAME.into(),
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

// 占位常量引用，避免不同编译配置下的未使用警告。
