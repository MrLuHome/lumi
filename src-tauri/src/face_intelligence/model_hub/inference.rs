//! 用户 ONNX 关键点模型的加载与单张裁剪推理。
//!
//! 模型在导入阶段已经通过结构与算子校验；这里仍对输出做有限性、数量与
//! 坐标范围检查。推理放在阻塞线程中，并用 `catch_unwind` 隔离 panic。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use tract_onnx::prelude::*;

use super::manifest::{ModelManifest, OutputCoordinate, OutputLayout};
use crate::face_intelligence::coordinate;
use crate::face_intelligence::types::BoundingBox;
use crate::image_engine::buffer::Image16;

pub struct LandmarkModel {
    manifest: ModelManifest,
    plan: Arc<TypedRunnableModel>,
}

impl LandmarkModel {
    pub fn load(path: &Path, manifest: &ModelManifest) -> Result<Self, String> {
        let plan = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|error| format!("模型解析失败：{error}"))?
            .into_optimized()
            .map_err(|error| format!("模型优化失败：{error}"))?
            .into_runnable()
            .map_err(|error| format!("模型编译失败：{error}"))?;
        Ok(Self {
            manifest: manifest.clone(),
            plan,
        })
    }

    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }

    /// 对单张人脸裁剪推理，返回原图归一化坐标 `(x, y, confidence)`，点数固定 68。
    pub fn infer(
        &self,
        image: &Image16,
        bbox: BoundingBox,
    ) -> Result<Vec<(f32, f32, f32)>, String> {
        let crop = coordinate::expand_crop(bbox, self.manifest.input.crop_margin);
        let (x, y, width, height) = coordinate::crop_rect(crop, image.width(), image.height());
        if width < 8 || height < 8 {
            return Err("人脸裁剪区域过小，无法推理".into());
        }
        let region = image::imageops::crop_imm(image, x, y, width, height).to_image();
        let resized = image::imageops::resize(
            &region,
            self.manifest.input.width,
            self.manifest.input.height,
            image::imageops::FilterType::Triangle,
        );
        let (input_w, input_h) = (resized.width() as usize, resized.height() as usize);
        let plane = input_w * input_h;
        let mut input = vec![0f32; 3 * plane];
        for (index, pixel) in resized.pixels().enumerate() {
            let values = [
                pixel.0[0] as f32 / 65535.0,
                pixel.0[1] as f32 / 65535.0,
                pixel.0[2] as f32 / 65535.0,
            ];
            for channel in 0..3 {
                input[channel * plane + index] =
                    self.manifest.input.normalization.apply(values[channel]);
            }
        }
        let tensor = tract_ndarray::Array4::from_shape_vec((1, 3, input_h, input_w), input)
            .map_err(|error| format!("输入张量构建失败：{error}"))?
            .into_tensor();

        let outputs = catch_unwind(AssertUnwindSafe(|| {
            self.plan.run(tvec!(tensor.into_tvalue()))
        }))
        .map_err(|_| "模型推理 panic，已隔离；请移除该模型".to_string())?
        .map_err(|error| format!("模型推理失败：{error}"))?;
        let output = outputs.first().ok_or_else(|| "模型没有输出".to_string())?;
        let view = output
            .to_plain_array_view::<f32>()
            .map_err(|error| format!("输出解析失败：{error}"))?;
        let shape: Vec<usize> = view.shape().to_vec();
        let data: Vec<f32> = view.iter().copied().collect();
        let layout = self.manifest.output.layout;
        let expected = match layout {
            OutputLayout::N2 | OutputLayout::N3 => {
                if shape.len() != 3 || shape[0] != 1 {
                    return Err(format!("输出形状 {:?} 与 manifest 不符", shape));
                }
                shape[1]
            }
            OutputLayout::C2N | OutputLayout::C3N => {
                if shape.len() != 3 || shape[0] != 1 {
                    return Err(format!("输出形状 {:?} 与 manifest 不符", shape));
                }
                shape[2]
            }
        };
        if expected != 68 {
            return Err(format!("模型输出 {expected} 点，face-68 需要 68 点"));
        }
        let channel_count = if layout.has_confidence() { 3 } else { 2 };
        if data.len() < expected * channel_count {
            return Err("输出元素数量不足".into());
        }

        let mut points = Vec::with_capacity(expected);
        for index in 0..expected {
            let (x_index, y_index, confidence_index) = match layout {
                OutputLayout::N2 | OutputLayout::N3 => (
                    index * channel_count,
                    index * channel_count + 1,
                    index * channel_count + 2,
                ),
                OutputLayout::C2N | OutputLayout::C3N => {
                    (index, expected + index, 2 * expected + index)
                }
            };
            let px = data[x_index];
            let py = data[y_index];
            let confidence = if layout.has_confidence() {
                data[confidence_index]
            } else {
                1.0
            };
            if !px.is_finite() || !py.is_finite() || !confidence.is_finite() {
                return Err(format!("第 {index} 个关键点包含 NaN/Inf"));
            }
            let (nx, ny) = match self.manifest.output.coordinate {
                OutputCoordinate::NormalizedCrop => {
                    if !(-0.3..=1.3).contains(&px) || !(-0.3..=1.3).contains(&py) {
                        return Err(format!(
                            "第 {index} 个关键点坐标越界（{px:.3}, {py:.3}），拒绝猜测"
                        ));
                    }
                    coordinate::crop_to_image(px, py, crop)
                }
                OutputCoordinate::PixelCrop => {
                    coordinate::crop_to_image(px / input_w as f32, py / input_h as f32, crop)
                }
            };
            points.push((nx, ny, coordinate::clamp_unit(confidence)));
        }
        Ok(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_fails_with_message() {
        let manifest = ModelManifest {
            schema_version: 1,
            id: "x".into(),
            name: "x".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            task: super::super::manifest::ModelTask::FaceLandmark,
            topology_id: "face-68".into(),
            input: super::super::manifest::InputSpec {
                width: 64,
                height: 64,
                layout: super::super::manifest::TensorLayout::Nchw,
                normalization: super::super::manifest::Normalization::ZeroToOne,
                crop_margin: 0.1,
            },
            output: super::super::manifest::OutputSpec {
                layout: OutputLayout::N2,
                coordinate: OutputCoordinate::NormalizedCrop,
            },
            license: Default::default(),
            sha256: String::new(),
            file_name: "model.onnx".into(),
            source_url: None,
            source_path: String::new(),
            imported_at: String::new(),
            validation: None,
        };
        assert!(LandmarkModel::load(Path::new("/tmp/missing-lumi.onnx"), &manifest).is_err());
    }
}
