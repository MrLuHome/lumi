//! ROUND 8：人脸智能与 Model Hub 的 Tauri 命令层。
//!
//! 所有命令都不绕过 Provider 抽象：命令只负责加载图片、调用注册表、
//! 把结果写入 `FaceRepository`，以及把“未验证 / 未解锁”的原因原样返回给前端。

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
use crate::error::{AppError, AppResult};
use crate::face_intelligence::anchors;
use crate::face_intelligence::capability::ProviderDescriptor;
use crate::face_intelligence::model_hub::{
    ImportOptions, InstalledModel, Normalization, OutputCoordinate,
};
use crate::face_intelligence::quality::QualityThresholds;
use crate::face_intelligence::registry::{
    FaceAnalysisRecord, FaceIntelligenceStatus, QualityReport,
};
use crate::face_intelligence::{dense, FaceIntelligence};
use crate::state::{FaceRepository, ImageCache};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeFacesReport {
    pub provider_id: String,
    pub provider_name: String,
    pub processing_time_ms: u64,
    pub face_count: usize,
    pub dense_face_count: usize,
    pub total_landmarks: usize,
    pub gate_summary: String,
    pub warnings: Vec<String>,
    /// 分析后的人脸（含 dense 与 level3），前端直接刷新，不再重跑检测，
    /// 避免二次检测把稠密结果覆盖成 5 点。
    pub faces: Vec<FaceFrame>,
    pub record: FaceAnalysisRecord,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSwitchReport {
    pub from: String,
    pub to: String,
    pub previous_gate_summary: Option<String>,
    /// true 表示切换后 LEVEL 3 的可用性可能变化，需要用户重新确认参数。
    pub requires_review: bool,
    /// 被清空稠密关键点/门控的人脸数；这些脸必须按新 Provider 重新分析。
    pub cleared_faces: usize,
    pub message: String,
}

/// Provider 列表（含证据等级与许可）。
#[tauri::command]
pub fn list_face_providers(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
) -> Vec<ProviderDescriptor> {
    intelligence.descriptors()
}

#[tauri::command]
pub fn face_intelligence_status(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
) -> FaceIntelligenceStatus {
    intelligence.status()
}

/// 切换 Provider：明确报告切换前后 LEVEL 3 能力差异，不静默沿用旧参数。
#[tauri::command]
pub fn select_face_provider(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    repo: State<'_, FaceRepository>,
    provider_id: String,
) -> AppResult<ProviderSwitchReport> {
    let previous = intelligence.last_analysis();
    let from = previous
        .as_ref()
        .map(|record| record.provider_id.clone())
        .unwrap_or_else(|| intelligence.active_id());
    let descriptor = intelligence.select(&provider_id)?;
    let changed =
        previous.as_ref().map(|record| record.provider_id.clone()) != Some(descriptor.id.clone());
    // 旧 Provider 的稠密关键点与门控结论不能继续使用：切换后一律清空，
    // 必须按新 Provider + 拓扑 + 模型版本重新分析并重新确认。
    let cleared_faces = if changed { repo.clear_analyses() } else { 0 };
    if changed {
        // 旧 Provider 的分析记录同样作废，否则 AI 面板会继续显示旧的 85 点表格。
        intelligence.clear_last_analysis();
    }
    let message = if changed {
        format!(
            "Provider 已切换到 {}。旧 Provider 的稠密关键点与 LEVEL 3 门控已清空（{cleared_faces} 张人脸），请重新分析并按新 Provider 的实测门控确认五官参数。",
            descriptor.display_name
        )
    } else {
        "Provider 未变化。".to_string()
    };
    Ok(ProviderSwitchReport {
        from,
        to: descriptor.id,
        previous_gate_summary: previous.map(|record| record.gate_summary()),
        requires_review: changed,
        cleared_faces,
        message,
    })
}

#[tauri::command]
pub fn face_quality_thresholds(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
) -> QualityThresholds {
    intelligence.thresholds().clone()
}

#[tauri::command]
pub fn last_face_analysis(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
) -> Option<FaceAnalysisRecord> {
    intelligence.last_analysis()
}

/// 用当前 Provider 分析照片，并把稠密关键点与 LEVEL 3 门控写入人脸缓存。
///
/// 只有 `gate.any_enabled()` 的人脸才携带 `dense`；5 点结果永远没有稠密区域。
#[tauri::command]
pub async fn analyze_photo_faces(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    path: String,
) -> AppResult<AnalyzeFacesReport> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let intelligence = intelligence.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let cancel = crate::image_engine::cancel::CancelToken::default();
        let (result, record) = intelligence.analyze(&path, &cached.preview, &cancel)?;

        let mut frames = Vec::with_capacity(result.faces.len());
        for face in &result.faces {
            let Some(anchors) =
                anchors::five_point_anchors(face).or_else(|| anchors::sparse_anchors(face))
            else {
                continue;
            };
            let Some(analyzed) = record
                .faces
                .iter()
                .find(|item| item.face_uuid == face.face_uuid)
            else {
                continue;
            };
            let dense_face = if analyzed.gate.any_enabled() {
                let dense = dense::assemble_dense(face, &result);
                (!dense.regions.is_empty()).then_some(dense)
            } else {
                None
            };
            frames.push(FaceFrame {
                id: 0,
                uuid: face.face_uuid.clone(),
                score: face.confidence,
                x: face.bounding_box.x,
                y: face.bounding_box.y,
                width: face.bounding_box.width,
                height: face.bounding_box.height,
                landmarks: anchors
                    .iter()
                    .map(|(x, y)| NormPoint { x: *x, y: *y })
                    .collect(),
                dense: dense_face,
                level3: Some(analyzed.gate.clone()),
            });
        }
        let stored = repo.store(Path::new(&path), frames);
        let dense_face_count = stored
            .faces
            .iter()
            .filter(|face| face.dense.is_some())
            .count();
        Ok(AnalyzeFacesReport {
            provider_id: record.provider_id.clone(),
            provider_name: record.provider_name.clone(),
            processing_time_ms: record.processing_time_ms,
            face_count: record.faces.len(),
            dense_face_count,
            total_landmarks: record.total_landmarks,
            gate_summary: record.gate_summary(),
            warnings: record.warnings.clone(),
            faces: stored.faces,
            record,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 质量探针：固定分析同一张照片 2..5 次，测量稳定性并升级/降级能力证据。
#[tauri::command]
pub async fn probe_face_quality(
    state: State<'_, ImageCache>,
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    path: String,
    runs: Option<usize>,
) -> AppResult<QualityReport> {
    let cache = state.inner().clone();
    let intelligence = intelligence.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let cancel = crate::image_engine::cancel::CancelToken::default();
        let report = intelligence.probe_quality(&cached.preview, &cancel, runs.unwrap_or(2))?;
        Ok(report)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub fn list_face_models(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
) -> Vec<InstalledModel> {
    intelligence.installed_models()
}

/// 选择用户自备的 ONNX 文件；只返回路径，不做任何加载。
#[tauri::command]
pub async fn pick_face_model_file() -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("选择用户自备 ONNX 关键点模型")
            .add_filter("ONNX 模型", &["onnx"])
            .pick_file()
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 导入请求。默认值刻意保守：未知许可证、不声明商用、不声明可再分发。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportFaceModelRequest {
    pub source: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub source_url: Option<String>,
    pub license_name: String,
    pub license_url: Option<String>,
    pub license_commercial_claimed: bool,
    pub license_redistribution_claimed: bool,
    pub license_note: String,
    pub normalization: String,
    pub output_coordinate: String,
    pub crop_margin: f32,
}

impl Default for ImportFaceModelRequest {
    fn default() -> Self {
        Self {
            source: String::new(),
            name: "用户关键点模型".into(),
            version: "1.0".into(),
            author: String::new(),
            description: String::new(),
            source_url: None,
            license_name: "UNKNOWN".into(),
            license_url: None,
            license_commercial_claimed: false,
            license_redistribution_claimed: false,
            license_note: "用户自备模型；LUMI 未核实许可证，不随安装包分发".into(),
            normalization: "zeroToOne".into(),
            output_coordinate: "normalizedCrop".into(),
            crop_margin: 0.15,
        }
    }
}

impl ImportFaceModelRequest {
    fn into_options(self) -> AppResult<ImportOptions> {
        if self.source.trim().is_empty() {
            return Err(AppError::InvalidInput("请选择 .onnx 模型文件".into()));
        }
        let normalization = match self.normalization.as_str() {
            "zeroToOne" => Normalization::ZeroToOne,
            "minusOneToOne" => Normalization::MinusOneToOne,
            "imagenet" => Normalization::Imagenet,
            "raw255" => Normalization::Raw255,
            other => return Err(AppError::InvalidInput(format!("未知归一化方式：{other}"))),
        };
        let output_coordinate = match self.output_coordinate.as_str() {
            "normalizedCrop" => OutputCoordinate::NormalizedCrop,
            "pixelCrop" => OutputCoordinate::PixelCrop,
            other => return Err(AppError::InvalidInput(format!("未知输出坐标系：{other}"))),
        };
        Ok(ImportOptions {
            id: None,
            name: self.name,
            version: self.version,
            author: self.author,
            description: self.description,
            source_url: self.source_url,
            license_name: self.license_name,
            license_url: self.license_url,
            license_commercial_claimed: self.license_commercial_claimed,
            license_redistribution_claimed: self.license_redistribution_claimed,
            license_note: self.license_note,
            normalization,
            output_coordinate,
            crop_margin: self.crop_margin,
        })
    }
}

/// 导入用户自备 ONNX。所有校验失败都带具体原因返回。
#[tauri::command]
pub async fn import_face_model(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    request: ImportFaceModelRequest,
) -> AppResult<InstalledModel> {
    let intelligence = intelligence.inner().clone();
    let source = request.source.clone();
    let options = request.into_options()?;
    tauri::async_runtime::spawn_blocking(move || {
        let installed = intelligence
            .model_hub()
            .import(Path::new(&source), &options)
            .map_err(AppError::Unsupported)?;
        intelligence.reload_models();
        Ok(installed)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub async fn revalidate_face_model(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    model_id: String,
) -> AppResult<InstalledModel> {
    let intelligence = intelligence.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let installed = intelligence
            .model_hub()
            .revalidate(&model_id)
            .map_err(AppError::Unsupported)?;
        intelligence.reload_models();
        Ok(installed)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub fn remove_face_model(
    intelligence: State<'_, std::sync::Arc<FaceIntelligence>>,
    model_id: String,
) -> AppResult<()> {
    intelligence
        .model_hub()
        .remove(&model_id)
        .map_err(AppError::Unsupported)?;
    intelligence.reload_models();
    Ok(())
}
