//! 精度评测的预测导出（开发用 CLI）。
//!
//! 用法：`lumi-retouch --nme-predict <dataset.json> <predictions.json>`
//!
//! 只在本机读取数据集清单里的照片，调用 Apple Vision Provider，把**带语义 ID**
//! 的归一化坐标写成 JSON，供 `scripts/nme_evaluate.py` 与人工标注比较。
//! 不打包任何照片、不上传、不做任何训练。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::face_intelligence::provider::{AnalysisOptions, FaceInput, FaceProvider};
use crate::face_intelligence::providers::apple_vision::AppleVisionProvider;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::decoder;

pub const PREDICT_FLAG: &str = "--nme-predict";

#[derive(Debug, Deserialize)]
struct Dataset {
    samples: Vec<Sample>,
}

#[derive(Debug, Deserialize)]
struct Sample {
    id: String,
    image: String,
    #[serde(default)]
    split: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Predictions {
    schema_version: u32,
    provider_id: String,
    provider_version: String,
    samples: Vec<SamplePrediction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SamplePrediction {
    id: String,
    image: String,
    split: Option<String>,
    ok: bool,
    error: Option<String>,
    image_width: u32,
    image_height: u32,
    faces: Vec<FacePrediction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacePrediction {
    confidence: f32,
    bounding_box: crate::face_intelligence::types::BoundingBox,
    points: Vec<PointPrediction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PointPrediction {
    semantic_id: String,
    x: f32,
    y: f32,
    confidence: f32,
}

pub fn base_dir(dataset_path: &Path) -> PathBuf {
    dataset_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 返回进程退出码。清单无法读取或整体无效时非 0；
/// 单张照片失败只写进该样本的 `error`，不中断整个导出。
pub fn run_cli(argv: &[String]) -> i32 {
    let (Some(dataset_arg), Some(output_arg)) = (argv.get(2), argv.get(3)) else {
        eprintln!("用法：lumi-retouch {PREDICT_FLAG} <dataset.json> <predictions.json>");
        return 2;
    };
    let dataset_path = PathBuf::from(dataset_arg);
    let text = match std::fs::read_to_string(&dataset_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("读取数据集清单失败：{error}");
            return 2;
        }
    };
    let dataset: Dataset = match serde_json::from_str(&text) {
        Ok(dataset) => dataset,
        Err(error) => {
            eprintln!("数据集清单不是合法 JSON：{error}");
            return 2;
        }
    };
    if dataset.samples.is_empty() {
        eprintln!("数据集清单没有样本");
        return 2;
    }
    let provider = AppleVisionProvider::new();
    if !AppleVisionProvider::available() {
        eprintln!("当前平台没有 Apple Vision，无法导出预测");
        return 3;
    }
    let options = AnalysisOptions::default();
    let cancel = CancelToken::default();
    let root = base_dir(&dataset_path);
    let mut samples = Vec::with_capacity(dataset.samples.len());
    for sample in &dataset.samples {
        let resolved = if Path::new(&sample.image).is_absolute() {
            PathBuf::from(&sample.image)
        } else {
            root.join(&sample.image)
        };
        let predicted = match decoder::decode(&resolved) {
            Ok(decoded) => {
                let image = decoded.image;
                match provider.analyze(&FaceInput::new(&image), &options, &cancel) {
                    Ok(result) => SamplePrediction {
                        id: sample.id.clone(),
                        image: sample.image.clone(),
                        split: sample.split.clone(),
                        ok: true,
                        error: None,
                        image_width: image.width(),
                        image_height: image.height(),
                        faces: result
                            .faces
                            .into_iter()
                            .map(|face| FacePrediction {
                                confidence: face.confidence,
                                bounding_box: face.bounding_box,
                                points: face
                                    .landmarks
                                    .into_iter()
                                    .map(|point| PointPrediction {
                                        semantic_id: point.semantic_id,
                                        x: point.x,
                                        y: point.y,
                                        confidence: point.confidence,
                                    })
                                    .collect(),
                            })
                            .collect(),
                    },
                    Err(error) => failed_sample(sample, 0, 0, error.to_string()),
                }
            }
            Err(error) => failed_sample(sample, 0, 0, error.to_string()),
        };
        samples.push(predicted);
    }
    let predictions = Predictions {
        schema_version: 1,
        provider_id: "apple-vision".into(),
        provider_version: "VNDetectFaceLandmarksRequest".into(),
        samples,
    };
    match serde_json::to_string_pretty(&predictions) {
        Ok(json) => match std::fs::write(output_arg, json) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("写入预测失败：{error}");
                2
            }
        },
        Err(error) => {
            eprintln!("序列化预测失败：{error}");
            2
        }
    }
}

fn failed_sample(sample: &Sample, width: u32, height: u32, error: String) -> SamplePrediction {
    SamplePrediction {
        id: sample.id.clone(),
        image: sample.image.clone(),
        split: sample.split.clone(),
        ok: false,
        error: Some(error),
        image_width: width,
        image_height: height,
        faces: Vec::new(),
    }
}
