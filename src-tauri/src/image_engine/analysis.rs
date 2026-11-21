use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai_engine::face_frame::FaceFrame;
use crate::image_engine::buffer::Image16;
use crate::image_engine::geometry::fit_within;
use crate::image_engine::params::{BeautyParams, EditParams};

/// 本地分析算法版本。规则或指标变化时必须递增，缓存与报告都依赖它。
pub const ANALYSIS_ALGORITHM_VERSION: &str = "local-analysis-1";
pub const ANALYSIS_RESULT_VERSION: u32 = 1;
const SAMPLE_MAX_EDGE: u32 = 720;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetouchMode {
    #[default]
    Natural,
    Light,
    Standard,
    Strong,
}

impl RetouchMode {
    pub fn label(self) -> &'static str {
        match self {
            RetouchMode::Natural => "自然",
            RetouchMode::Light => "轻度",
            RetouchMode::Standard => "标准",
            RetouchMode::Strong => "强化",
        }
    }

    /// 不同模式按参数语义分别约束，而不是只把全部滑块整体放大。
    fn tonal_factor(self) -> f32 {
        match self {
            RetouchMode::Natural => 0.55,
            RetouchMode::Light => 0.78,
            RetouchMode::Standard => 1.0,
            RetouchMode::Strong => 1.22,
        }
    }

    fn beauty_factor(self) -> f32 {
        match self {
            RetouchMode::Natural => 0.45,
            RetouchMode::Light => 0.7,
            RetouchMode::Standard => 1.0,
            RetouchMode::Strong => 1.15,
        }
    }

    fn detail_factor(self) -> f32 {
        match self {
            RetouchMode::Natural => 0.5,
            RetouchMode::Light => 0.8,
            RetouchMode::Standard => 1.0,
            RetouchMode::Strong => 1.12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricStatus {
    Ok,
    LowConfidence,
    NoData,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisMetric {
    pub key: String,
    pub label: String,
    pub value: Option<f32>,
    pub unit: String,
    pub status: MetricStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchSuggestion {
    pub id: String,
    pub group: String,
    pub label: String,
    /// `param` 表示普通编辑参数，`faceBeauty` 表示按人脸美肤参数合并。
    pub target: String,
    pub field: String,
    pub current: f32,
    pub suggested: f32,
    pub reason: String,
    pub confidence: f32,
    pub applicable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCapabilities {
    pub face_detection: bool,
    pub beauty_analysis: bool,
    pub manual_mask: bool,
    pub background: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisReport {
    pub image_hash: String,
    pub algorithm_version: String,
    pub result_version: u32,
    pub analyzed_at_ms: u64,
    pub mode: RetouchMode,
    pub faces: usize,
    pub metrics: Vec<AnalysisMetric>,
    pub suggestions: Vec<RetouchSuggestion>,
    pub warnings: Vec<String>,
    pub capabilities: AnalysisCapabilities,
}

#[derive(Debug, Clone)]
struct Stats {
    mean: f32,
    median: f32,
    p05: f32,
    p95: f32,
    p99: f32,
    std: f32,
    highlight_clip: f32,
    shadow_clip: f32,
    saturation: f32,
    r_mean: f32,
    g_mean: f32,
    b_mean: f32,
    local_contrast: f32,
    sharpness: f32,
    noise: f32,
}

pub fn image_hash(image: &Image16) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    let step_x = (image.width() / 64).max(1);
    let step_y = (image.height() / 64).max(1);
    for y in (0..image.height()).step_by(step_y as usize) {
        for x in (0..image.width()).step_by(step_x as usize) {
            let pixel = image.get_pixel(x, y).0;
            for channel in pixel {
                hash ^= (channel >> 8) as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash ^= (image.width() as u64) << 32;
    hash ^= image.height() as u64;
    format!("{hash:016x}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn luma(pixel: [u16; 4]) -> f32 {
    (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32) / 65535.0
}

fn stats(image: &Image16) -> Stats {
    let width = image.width();
    let height = image.height();
    let mut histogram = [0u64; 256];
    let mut count = 0u64;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut sat_sum = 0.0f64;
    let mut r_sum = 0.0f64;
    let mut g_sum = 0.0f64;
    let mut b_sum = 0.0f64;

    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(x, y).0;
            let value = luma(pixel);
            histogram[(value * 255.0).round() as usize & 255] += 1;
            count += 1;
            sum += value as f64;
            sum_sq += (value * value) as f64;
            let max = pixel[0].max(pixel[1]).max(pixel[2]) as f32;
            let min = pixel[0].min(pixel[1]).min(pixel[2]) as f32;
            sat_sum += if max > 0.0 { (max - min) / max } else { 0.0 } as f64;
            r_sum += pixel[0] as f64 / 65535.0;
            g_sum += pixel[1] as f64 / 65535.0;
            b_sum += pixel[2] as f64 / 65535.0;
        }
    }

    let count = count.max(1);
    let mean = (sum / count as f64) as f32;
    let variance = ((sum_sq / count as f64) - (mean as f64 * mean as f64)).max(0.0);
    let percentile = |target: f64| -> f32 {
        let need = (count as f64 * target).max(1.0);
        let mut seen = 0u64;
        for (index, value) in histogram.iter().enumerate() {
            seen += *value;
            if seen as f64 >= need {
                return index as f32 / 255.0;
            }
        }
        1.0
    };

    let mut laplacian_sum = 0.0f64;
    let mut laplacian_sq = 0.0f64;
    let mut laplacian_count = 0u64;
    let mut noise_sum = 0.0f64;
    let mut noise_count = 0u64;
    if width > 2 && height > 2 {
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let center = luma(image.get_pixel(x, y).0);
                let left = luma(image.get_pixel(x - 1, y).0);
                let right = luma(image.get_pixel(x + 1, y).0);
                let up = luma(image.get_pixel(x, y - 1).0);
                let down = luma(image.get_pixel(x, y + 1).0);
                let laplacian = (4.0 * center - left - right - up - down).abs();
                laplacian_sum += laplacian as f64;
                laplacian_sq += (laplacian * laplacian) as f64;
                laplacian_count += 1;
                // 平坦区域的微小残差更接近噪点而非真实细节。
                let local_mean = (left + right + up + down) * 0.25;
                if (center - local_mean).abs() < 0.02 {
                    noise_sum += laplacian as f64;
                    noise_count += 1;
                }
            }
        }
    }
    let lap_mean = if laplacian_count > 0 {
        (laplacian_sum / laplacian_count as f64) as f32
    } else {
        0.0
    };
    let lap_var = if laplacian_count > 0 {
        ((laplacian_sq / laplacian_count as f64) - (lap_mean as f64).powi(2)).max(0.0) as f32
    } else {
        0.0
    };
    let noise = if noise_count > 0 {
        (noise_sum / noise_count as f64) as f32
    } else {
        0.0
    };

    Stats {
        mean,
        median: percentile(0.5),
        p05: percentile(0.05),
        p95: percentile(0.95),
        p99: percentile(0.99),
        std: variance.sqrt() as f32,
        highlight_clip: histogram[252..].iter().sum::<u64>() as f32 / count as f32,
        shadow_clip: histogram[..4].iter().sum::<u64>() as f32 / count as f32,
        saturation: (sat_sum / count as f64) as f32,
        r_mean: (r_sum / count as f64) as f32,
        g_mean: (g_sum / count as f64) as f32,
        b_mean: (b_sum / count as f64) as f32,
        local_contrast: lap_mean,
        sharpness: lap_var.sqrt(),
        noise,
    }
}

fn metric(
    key: &str,
    label: &str,
    value: Option<f32>,
    unit: &str,
    status: MetricStatus,
    detail: impl Into<String>,
) -> AnalysisMetric {
    AnalysisMetric {
        key: key.to_string(),
        label: label.to_string(),
        value,
        unit: unit.to_string(),
        status,
        detail: detail.into(),
    }
}

fn clamp_suggestion(value: f32, min: f32, max: f32) -> f32 {
    value.clamp(min, max).round()
}

#[allow(clippy::too_many_arguments)]
fn scalar_suggestion(
    id: &str,
    group: &str,
    label: &str,
    field: &str,
    current: f32,
    raw: f32,
    reason: impl Into<String>,
    confidence: f32,
    min: f32,
    max: f32,
    factor: f32,
) -> Option<RetouchSuggestion> {
    let suggested = clamp_suggestion(raw * factor, min, max);
    if (suggested - current).abs() < 0.5 {
        return None;
    }
    Some(RetouchSuggestion {
        id: id.to_string(),
        group: group.to_string(),
        label: label.to_string(),
        target: "param".into(),
        field: field.to_string(),
        current,
        suggested,
        reason: reason.into(),
        confidence,
        applicable: true,
    })
}

fn beauty_current(params: &EditParams, field: &str) -> f32 {
    let Some(face) = params.faces.first() else {
        return 0.0;
    };
    let beauty = &face.beauty;
    match field {
        "skin_smooth" => beauty.skin_smooth,
        "texture" => beauty.texture,
        "detail" => beauty.detail,
        "blemish" => beauty.blemish,
        "spots" => beauty.spots,
        "blackheads" => beauty.blackheads,
        "tone_even" => beauty.tone_even,
        "whitening" => beauty.whitening,
        "redness" => beauty.redness,
        "dullness" => beauty.dullness,
        "dark_circles" => beauty.dark_circles,
        "eye_lines" => beauty.eye_lines,
        "forehead_lines" => beauty.forehead_lines,
        "nasolabial" => beauty.nasolabial,
        _ => 0.0,
    }
}

fn beauty_suggestions(
    params: &EditParams,
    faces: usize,
    mode: RetouchMode,
    out: &mut Vec<RetouchSuggestion>,
) {
    if faces == 0 {
        return;
    }
    let already = params
        .faces
        .iter()
        .any(|face| face.enabled && face.beauty.is_active());
    if already {
        return;
    }
    // 没有第三轮皮肤指标时不假装检测到具体皮肤问题，只给保守基础模板。
    let template: [(&str, &str, f32); 6] = [
        ("skin_smooth", "磨皮", 16.0),
        ("texture", "纹理保护", 18.0),
        ("tone_even", "肤色均匀", 12.0),
        ("blemish", "自动祛痘", 8.0),
        ("dark_circles", "黑眼圈淡化", 8.0),
        ("nasolabial", "法令纹", 6.0),
    ];
    let factor = mode.beauty_factor();
    for (field, label, raw) in template {
        let suggested = clamp_suggestion(raw * factor, 0.0, 35.0);
        if suggested < 1.0 {
            continue;
        }
        out.push(RetouchSuggestion {
            id: format!("beauty-{field}"),
            group: "人像".into(),
            label: format!("人脸{label}"),
            target: "faceBeauty".into(),
            field: field.into(),
            current: beauty_current(params, field),
            suggested,
            reason: format!(
                "检测到 {faces} 张人脸，当前未设置美肤参数；给出保守{}模板，不声称检测到具体皮肤问题",
                mode.label()
            ),
            confidence: 0.55,
            applicable: true,
        });
    }
}

pub fn analyze(
    image: &Image16,
    faces: &[FaceFrame],
    params: &EditParams,
    mode: RetouchMode,
) -> AnalysisReport {
    let started_hash = image_hash(image);
    let sample = if image.width().max(image.height()) > SAMPLE_MAX_EDGE {
        fit_within(image, SAMPLE_MAX_EDGE)
    } else {
        image.clone()
    };
    let stats = stats(&sample);
    let mut metrics = Vec::new();
    let mut suggestions = Vec::new();
    let mut warnings = Vec::new();
    let factor = mode.tonal_factor();

    metrics.push(metric(
        "exposure.mean",
        "平均亮度",
        Some(stats.mean),
        "",
        MetricStatus::Ok,
        format!("均值 {:.2}，中位数 {:.2}", stats.mean, stats.median),
    ));
    metrics.push(metric(
        "exposure.contrast",
        "明暗对比",
        Some(stats.std),
        "",
        if stats.std < 0.06 {
            MetricStatus::LowConfidence
        } else {
            MetricStatus::Ok
        },
        format!(
            "标准差 {:.3}，P5 {:.2} / P95 {:.2}",
            stats.std, stats.p05, stats.p95
        ),
    ));
    metrics.push(metric(
        "exposure.clipHighlights",
        "高光裁切",
        Some(stats.highlight_clip),
        "%",
        MetricStatus::Ok,
        format!("接近纯白的像素 {:.1}%", stats.highlight_clip * 100.0),
    ));
    metrics.push(metric(
        "exposure.clipShadows",
        "暗部压死",
        Some(stats.shadow_clip),
        "%",
        MetricStatus::Ok,
        format!("接近纯黑的像素 {:.1}%", stats.shadow_clip * 100.0),
    ));
    metrics.push(metric(
        "color.temperatureBias",
        "白平衡 R/B 偏差",
        Some(stats.r_mean - stats.b_mean),
        "",
        MetricStatus::Ok,
        format!(
            "R {:.3} / G {:.3} / B {:.3}",
            stats.r_mean, stats.g_mean, stats.b_mean
        ),
    ));
    metrics.push(metric(
        "color.saturation",
        "平均饱和度",
        Some(stats.saturation),
        "",
        MetricStatus::Ok,
        format!("{:.2}", stats.saturation),
    ));
    metrics.push(metric(
        "detail.localContrast",
        "局部对比度",
        Some(stats.local_contrast),
        "",
        MetricStatus::LowConfidence,
        "基于拉普拉斯残差的估计",
    ));
    metrics.push(metric(
        "detail.sharpness",
        "清晰度指标",
        Some(stats.sharpness),
        "",
        MetricStatus::LowConfidence,
        "高频方差，受拍摄内容影响",
    ));
    metrics.push(metric(
        "detail.noise",
        "噪点风险",
        Some(stats.noise),
        "",
        MetricStatus::LowConfidence,
        "平坦区域残差估计，仅作参考",
    ));

    // 曝光与光影
    let exposure_delta = (0.46 - stats.mean) * 90.0;
    if exposure_delta.abs() >= 3.0 {
        if let Some(item) = scalar_suggestion(
            "exposure",
            "基础光影",
            "曝光",
            "exposure",
            params.exposure,
            exposure_delta,
            format!("平均亮度 {:.2}，偏离人像常用中间调 0.46", stats.mean),
            0.8,
            -25.0,
            25.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    if stats.highlight_clip > 0.02 {
        if let Some(item) = scalar_suggestion(
            "highlights",
            "基础光影",
            "高光",
            "highlights",
            params.highlights,
            -(stats.highlight_clip * 100.0 * 9.0),
            format!(
                "高光裁切 {:.1}%，建议压回高光细节",
                stats.highlight_clip * 100.0
            ),
            0.78,
            -38.0,
            -4.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    if stats.shadow_clip > 0.035 {
        if let Some(item) = scalar_suggestion(
            "shadows",
            "基础光影",
            "阴影",
            "shadows",
            params.shadows,
            stats.shadow_clip * 100.0 * 7.0,
            format!(
                "暗部压死 {:.1}%，建议恢复暗部层次",
                stats.shadow_clip * 100.0
            ),
            0.75,
            4.0,
            36.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    if stats.p99 > 0.97 && stats.highlight_clip > 0.005 {
        if let Some(item) = scalar_suggestion(
            "whites",
            "基础光影",
            "白色色阶",
            "whites",
            params.whites,
            -8.0,
            "P99 接近纯白，白色色阶略收可保住高光层次".to_string(),
            0.62,
            -20.0,
            -3.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    if stats.std < 0.13 {
        if let Some(item) = scalar_suggestion(
            "contrast",
            "基础光影",
            "对比度",
            "contrast",
            params.contrast,
            (0.14 - stats.std) * 260.0,
            format!("明暗标准差 {:.3} 偏低，画面偏平", stats.std),
            0.78,
            4.0,
            22.0,
            factor,
        ) {
            suggestions.push(item);
        }
    } else if stats.std > 0.27 {
        if let Some(item) = scalar_suggestion(
            "contrast",
            "基础光影",
            "对比度",
            "contrast",
            params.contrast,
            -(stats.std - 0.24) * 150.0,
            format!("明暗标准差 {:.3} 偏高，建议柔和对比", stats.std),
            0.7,
            -20.0,
            -3.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }

    // 白平衡与色彩
    let green_ref = ((stats.r_mean + stats.b_mean) * 0.5).max(1e-4);
    let rb_bias = (stats.b_mean - stats.r_mean) / stats.g_mean.max(1e-4);
    if rb_bias.abs() > 0.035 {
        if let Some(item) = scalar_suggestion(
            "temperature",
            "白平衡",
            "色温",
            "temperature",
            params.temperature,
            rb_bias * 240.0,
            format!(
                "R/B 偏差 {:+.3}，画面{}",
                stats.r_mean - stats.b_mean,
                if rb_bias > 0.0 { "偏冷" } else { "偏暖" }
            ),
            0.65,
            -20.0,
            20.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    let tint_bias = (stats.g_mean - green_ref) / green_ref;
    if tint_bias.abs() > 0.025 {
        if let Some(item) = scalar_suggestion(
            "tint",
            "白平衡",
            "色调",
            "tint",
            params.tint,
            tint_bias * 220.0,
            format!("绿/品红偏差 {:+.3}", tint_bias),
            0.6,
            -12.0,
            12.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }
    if stats.saturation < 0.17 {
        if let Some(item) = scalar_suggestion(
            "vibrance",
            "色彩",
            "自然饱和度",
            "vibrance",
            params.vibrance,
            (0.18 - stats.saturation) * 150.0,
            format!("平均饱和度 {:.2} 偏低", stats.saturation),
            0.7,
            4.0,
            18.0,
            factor,
        ) {
            suggestions.push(item);
        }
    } else if stats.saturation > 0.48 {
        if let Some(item) = scalar_suggestion(
            "saturation",
            "色彩",
            "饱和度",
            "saturation",
            params.saturation,
            -(stats.saturation - 0.44) * 70.0,
            format!("平均饱和度 {:.2} 偏高，建议收敛", stats.saturation),
            0.66,
            -20.0,
            -3.0,
            factor,
        ) {
            suggestions.push(item);
        }
    }

    // 细节
    if stats.sharpness < 0.035 {
        if let Some(item) = scalar_suggestion(
            "sharpness",
            "细节",
            "锐化",
            "sharpness",
            params.sharpness,
            (0.045 - stats.sharpness) * 420.0,
            "高频方差偏低，画面清晰度不足".to_string(),
            0.55,
            5.0,
            22.0,
            mode.detail_factor(),
        ) {
            suggestions.push(item);
        }
    }
    if stats.local_contrast < 0.018 {
        if let Some(item) = scalar_suggestion(
            "clarity",
            "细节",
            "清晰度",
            "clarity",
            params.clarity,
            (0.022 - stats.local_contrast) * 520.0,
            "局部对比度偏低，建议轻量提升质感".to_string(),
            0.55,
            3.0,
            16.0,
            mode.detail_factor(),
        ) {
            suggestions.push(item);
        }
    }
    if stats.noise > 0.05 {
        warnings.push(format!(
            "噪点风险估计偏高（{:.3}），建议降低锐化或先做降噪",
            stats.noise
        ));
    }

    // 人像
    metrics.push(metric(
        "portrait.faces",
        "人脸数量",
        Some(faces.len() as f32),
        "张",
        if faces.is_empty() {
            MetricStatus::NoData
        } else {
            MetricStatus::Ok
        },
        if faces.is_empty() {
            "未检测到人脸；不生成人像参数建议".to_string()
        } else {
            format!("检测到 {} 张人脸", faces.len())
        },
    ));
    let mask_ready = params.subject_mask.enabled && !params.subject_mask.strokes.is_empty();
    metrics.push(metric(
        "portrait.manualMask",
        "手动人物蒙版",
        Some(if mask_ready { 1.0 } else { 0.0 }),
        "",
        MetricStatus::Ok,
        if mask_ready {
            "已绘制手动蒙版"
        } else {
            "未绘制手动蒙版"
        }
        .to_string(),
    ));
    metrics.push(metric(
        "portrait.beautyParams",
        "已有美肤参数",
        Some(
            params
                .faces
                .iter()
                .filter(|face| face.beauty.is_active())
                .count() as f32,
        ),
        "张人脸",
        MetricStatus::Ok,
        "来自当前编辑参数，不重新运行皮肤模型".to_string(),
    ));
    metrics.push(metric(
        "portrait.background",
        "背景分离",
        Some(if params.background.is_active() {
            1.0
        } else {
            0.0
        }),
        "",
        MetricStatus::Ok,
        if params.background.is_active() {
            "已启用背景处理"
        } else {
            "背景未处理"
        }
        .to_string(),
    ));
    if faces.is_empty() {
        warnings.push("未检测到人脸：人像相关指标标记为无数据，不生成虚假美肤建议".into());
    }
    beauty_suggestions(params, faces.len(), mode, &mut suggestions);

    AnalysisReport {
        image_hash: started_hash,
        algorithm_version: ANALYSIS_ALGORITHM_VERSION.to_string(),
        result_version: ANALYSIS_RESULT_VERSION,
        analyzed_at_ms: now_ms(),
        mode,
        faces: faces.len(),
        metrics,
        suggestions,
        warnings,
        capabilities: AnalysisCapabilities {
            face_detection: true,
            // 第三轮皮肤模型没有可商用权重，本轮不声称皮肤指标已启用。
            beauty_analysis: false,
            manual_mask: true,
            background: true,
        },
    }
}

/// 只应用被接受的建议，逐字段合并。人脸美肤只增不减，避免抹掉用户更强的设置。
pub fn apply_suggestions(
    params: &EditParams,
    suggestions: &[RetouchSuggestion],
    faces: &[FaceFrame],
) -> (EditParams, Vec<String>) {
    let mut next = params.clone();
    let mut warnings = Vec::new();
    for suggestion in suggestions {
        if !suggestion.applicable {
            continue;
        }
        if suggestion.target == "faceBeauty" {
            if next.faces.is_empty() {
                for face in faces {
                    let uuid = if face.uuid.is_empty() {
                        format!("face-{}", face.id)
                    } else {
                        face.uuid.clone()
                    };
                    next.faces.push(crate::image_engine::params::FaceBeauty {
                        face_id: face.id,
                        face_uuid: uuid,
                        enabled: true,
                        beauty: BeautyParams::default(),
                    });
                }
            }
            if next.faces.is_empty() {
                warnings.push("没有人脸条目，人脸美肤建议未应用".into());
                continue;
            }
            for face in next.faces.iter_mut() {
                let slot = match suggestion.field.as_str() {
                    "skin_smooth" => &mut face.beauty.skin_smooth,
                    "texture" => &mut face.beauty.texture,
                    "detail" => &mut face.beauty.detail,
                    "blemish" => &mut face.beauty.blemish,
                    "spots" => &mut face.beauty.spots,
                    "blackheads" => &mut face.beauty.blackheads,
                    "tone_even" => &mut face.beauty.tone_even,
                    "whitening" => &mut face.beauty.whitening,
                    "redness" => &mut face.beauty.redness,
                    "dullness" => &mut face.beauty.dullness,
                    "dark_circles" => &mut face.beauty.dark_circles,
                    "eye_lines" => &mut face.beauty.eye_lines,
                    "forehead_lines" => &mut face.beauty.forehead_lines,
                    "nasolabial" => &mut face.beauty.nasolabial,
                    _ => {
                        warnings.push(format!("未知美肤字段：{}", suggestion.field));
                        continue;
                    }
                };
                *slot = slot.max(suggestion.suggested).clamp(0.0, 100.0);
            }
            continue;
        }
        let value = suggestion.suggested;
        match suggestion.field.as_str() {
            "exposure" => next.exposure = value,
            "contrast" => next.contrast = value,
            "highlights" => next.highlights = value,
            "shadows" => next.shadows = value,
            "whites" => next.whites = value,
            "blacks" => next.blacks = value,
            "temperature" => next.temperature = value,
            "tint" => next.tint = value,
            "saturation" => next.saturation = value,
            "vibrance" => next.vibrance = value,
            "clarity" => next.clarity = value,
            "sharpness" => next.sharpness = value,
            "grain" => next.grain = value,
            "vignette" => next.vignette = value,
            _ => warnings.push(format!("未知参数：{}", suggestion.field)),
        }
    }
    (next.clamped(), warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn uniform(value: u16, width: u32, height: u32) -> Image16 {
        Image16::from_pixel(width, height, Rgba([value, value, value, 65535]))
    }

    fn params() -> EditParams {
        EditParams::default()
    }

    #[test]
    fn dark_image_suggests_positive_exposure() {
        let image = uniform(6000, 64, 64);
        let report = analyze(&image, &[], &params(), RetouchMode::Standard);
        let suggestion = report
            .suggestions
            .iter()
            .find(|item| item.field == "exposure")
            .expect("暗图应有曝光建议");
        assert!(suggestion.suggested > 0.0);
        assert!((suggestion.suggested as i32) <= 25);
    }

    #[test]
    fn bright_image_suggests_negative_exposure() {
        let image = uniform(60000, 64, 64);
        let report = analyze(&image, &[], &params(), RetouchMode::Standard);
        let suggestion = report
            .suggestions
            .iter()
            .find(|item| item.field == "exposure")
            .expect("亮图应有曝光建议");
        assert!(suggestion.suggested < 0.0);
    }

    #[test]
    fn deterministic_for_same_input() {
        let image = uniform(30000, 32, 32);
        let a = analyze(&image, &[], &params(), RetouchMode::Natural);
        let b = analyze(&image, &[], &params(), RetouchMode::Natural);
        assert_eq!(a.image_hash, b.image_hash);
        assert_eq!(a.metrics.len(), b.metrics.len());
        assert_eq!(a.suggestions.len(), b.suggestions.len());
        for (left, right) in a.suggestions.iter().zip(b.suggestions.iter()) {
            assert_eq!(left.field, right.field);
            assert_eq!(left.suggested, right.suggested);
        }
    }

    #[test]
    fn mode_changes_strength_but_keeps_natural_lowest() {
        let image = uniform(5000, 32, 32);
        let natural = analyze(&image, &[], &params(), RetouchMode::Natural);
        let strong = analyze(&image, &[], &params(), RetouchMode::Strong);
        let natural_exposure = natural
            .suggestions
            .iter()
            .find(|item| item.field == "exposure")
            .map(|item| item.suggested)
            .unwrap_or(0.0);
        let strong_exposure = strong
            .suggestions
            .iter()
            .find(|item| item.field == "exposure")
            .map(|item| item.suggested)
            .unwrap_or(0.0);
        assert!(strong_exposure >= natural_exposure);
    }

    #[test]
    fn no_face_produces_no_beauty_suggestions() {
        let image = uniform(40000, 32, 32);
        let report = analyze(&image, &[], &params(), RetouchMode::Standard);
        assert!(report
            .suggestions
            .iter()
            .all(|item| item.target != "faceBeauty"));
        assert!(report
            .warnings
            .iter()
            .any(|text| text.contains("未检测到人脸")));
    }

    #[test]
    fn apply_only_touches_suggested_fields() {
        let mut base = params();
        base.contrast = -7.0;
        let suggestions = vec![RetouchSuggestion {
            id: "exposure".into(),
            group: "基础光影".into(),
            label: "曝光".into(),
            target: "param".into(),
            field: "exposure".into(),
            current: 0.0,
            suggested: 12.0,
            reason: "测试".into(),
            confidence: 0.8,
            applicable: true,
        }];
        let (next, warnings) = apply_suggestions(&base, &suggestions, &[]);
        assert!(warnings.is_empty());
        assert_eq!(next.exposure, 12.0);
        assert_eq!(next.contrast, -7.0);
        assert!(next.reshape.is_empty());
    }

    fn colored(red: u16, green: u16, blue: u16) -> Image16 {
        Image16::from_pixel(48, 48, Rgba([red, green, blue, 65535]))
    }

    fn face(id: u32, uuid: &str) -> FaceFrame {
        FaceFrame {
            id,
            uuid: uuid.into(),
            score: 0.9,
            x: 0.2,
            y: 0.2,
            width: 0.2,
            height: 0.2,
            landmarks: Vec::new(),
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn overexposed_image_reports_clipping_and_recovers_highlights() {
        let image = uniform(65535, 48, 48);
        let report = analyze(&image, &[], &params(), RetouchMode::Standard);
        let clip = report
            .metrics
            .iter()
            .find(|item| item.key == "exposure.clipHighlights")
            .unwrap();
        assert!(clip.value.unwrap() > 0.9, "几乎整张图都是纯白");
        assert_eq!(clip.status as u8, MetricStatus::Ok as u8);
        let highlights = report
            .suggestions
            .iter()
            .find(|item| item.field == "highlights")
            .expect("应建议压回高光");
        assert!(highlights.suggested <= -4.0);
    }

    #[test]
    fn high_contrast_image_suggests_softer_contrast() {
        let image = Image16::from_fn(64, 64, |x, _| {
            if x < 32 {
                Rgba([2000, 2000, 2000, 65535])
            } else {
                Rgba([64000, 64000, 64000, 65535])
            }
        });
        let report = analyze(&image, &[], &params(), RetouchMode::Standard);
        let contrast = report
            .suggestions
            .iter()
            .find(|item| item.field == "contrast")
            .expect("高反差应建议降低对比");
        assert!(contrast.suggested < 0.0);
    }

    #[test]
    fn warm_and_cool_images_get_opposite_temperature_suggestions() {
        let warm = analyze(
            &colored(52000, 40000, 28000),
            &[],
            &params(),
            RetouchMode::Standard,
        );
        let cool = analyze(
            &colored(28000, 40000, 52000),
            &[],
            &params(),
            RetouchMode::Standard,
        );
        let warm_temperature = warm
            .suggestions
            .iter()
            .find(|item| item.field == "temperature")
            .map(|item| item.suggested)
            .unwrap_or(0.0);
        let cool_temperature = cool
            .suggestions
            .iter()
            .find(|item| item.field == "temperature")
            .map(|item| item.suggested)
            .unwrap_or(0.0);
        assert!(warm_temperature < 0.0, "偏暖照片应降低色温");
        assert!(cool_temperature > 0.0, "偏冷照片应提高色温");
    }

    #[test]
    fn saturation_extremes_are_handled_in_both_directions() {
        let gray = analyze(
            &uniform(30000, 48, 48),
            &[],
            &params(),
            RetouchMode::Standard,
        );
        assert!(
            gray.suggestions
                .iter()
                .any(|item| item.field == "vibrance" && item.suggested > 0.0),
            "低饱和应建议提高自然饱和度"
        );
        let vivid = analyze(&colored(60000, 0, 0), &[], &params(), RetouchMode::Standard);
        assert!(
            vivid
                .suggestions
                .iter()
                .any(|item| item.field == "saturation" && item.suggested < 0.0),
            "高饱和应建议降低饱和度"
        );
    }

    #[test]
    fn multi_face_uses_real_count_and_conservative_beauty_template() {
        let faces = [face(1, "face-a"), face(2, "face-b")];
        let report = analyze(
            &uniform(32000, 48, 48),
            &faces,
            &params(),
            RetouchMode::Standard,
        );
        assert_eq!(report.faces, 2);
        let count = report
            .metrics
            .iter()
            .find(|item| item.key == "portrait.faces")
            .unwrap();
        assert_eq!(count.value, Some(2.0));
        assert_eq!(count.status as u8, MetricStatus::Ok as u8);
        let beauty: Vec<_> = report
            .suggestions
            .iter()
            .filter(|item| item.target == "faceBeauty")
            .collect();
        assert!(!beauty.is_empty());
        for item in beauty {
            assert!(item.confidence <= 0.6, "没有皮肤模型时不能给出高置信度");
            assert!(item.reason.contains("不声称"));
            assert!((0.0..=35.0).contains(&item.suggested));
        }
    }

    #[test]
    fn all_suggestions_stay_inside_documented_ranges() {
        let cases = [
            uniform(2000, 32, 32),
            uniform(64000, 32, 32),
            colored(55000, 38000, 25000),
            colored(25000, 38000, 55000),
            uniform(30000, 32, 32),
        ];
        let limits: [(&str, f32, f32); 14] = [
            ("exposure", -25.0, 25.0),
            ("contrast", -25.0, 25.0),
            ("highlights", -40.0, 40.0),
            ("shadows", -10.0, 40.0),
            ("whites", -25.0, 25.0),
            ("blacks", -25.0, 25.0),
            ("temperature", -25.0, 25.0),
            ("tint", -15.0, 15.0),
            ("saturation", -25.0, 25.0),
            ("vibrance", 0.0, 25.0),
            ("clarity", 0.0, 20.0),
            ("sharpness", 0.0, 25.0),
            ("grain", 0.0, 20.0),
            ("vignette", 0.0, 20.0),
        ];
        for image in cases {
            for mode in [
                RetouchMode::Natural,
                RetouchMode::Light,
                RetouchMode::Standard,
                RetouchMode::Strong,
            ] {
                let report = analyze(&image, &[], &params(), mode);
                for item in &report.suggestions {
                    if let Some((_, min, max)) =
                        limits.iter().find(|(field, _, _)| *field == item.field)
                    {
                        assert!(
                            item.suggested >= *min && item.suggested <= *max,
                            "{} = {} 超出 [{} , {}]",
                            item.field,
                            item.suggested,
                            min,
                            max
                        );
                    }
                    assert!(item.confidence > 0.0 && item.confidence <= 1.0);
                    assert!(!item.reason.is_empty());
                }
            }
        }
    }

    #[test]
    fn every_no_data_metric_has_an_explicit_status_not_fake_ok() {
        let report = analyze(
            &uniform(40000, 32, 32),
            &[],
            &params(),
            RetouchMode::Standard,
        );
        let faces = report
            .metrics
            .iter()
            .find(|item| item.key == "portrait.faces")
            .unwrap();
        assert!(matches!(
            faces.status,
            MetricStatus::NoData | MetricStatus::LowConfidence
        ));
        for metric in &report.metrics {
            assert!(!metric.detail.is_empty(), "{} 缺少数据来源说明", metric.key);
        }
        for key in ["detail.localContrast", "detail.sharpness", "detail.noise"] {
            let metric = report.metrics.iter().find(|item| item.key == key).unwrap();
            assert!(matches!(
                metric.status,
                MetricStatus::LowConfidence | MetricStatus::NoData
            ));
        }
    }

    #[test]
    fn applying_face_beauty_creates_entries_and_keeps_higher_values() {
        let faces = [face(1, "face-a")];
        let mut base = params();
        base.faces = vec![crate::image_engine::params::FaceBeauty::new(1)];
        base.faces[0].face_uuid = "face-a".into();
        base.faces[0].beauty.skin_smooth = 30.0;
        let suggestions = vec![
            RetouchSuggestion {
                id: "beauty-skin_smooth".into(),
                group: "人像".into(),
                label: "人脸磨皮".into(),
                target: "faceBeauty".into(),
                field: "skin_smooth".into(),
                current: 30.0,
                suggested: 16.0,
                reason: "测试".into(),
                confidence: 0.5,
                applicable: true,
            },
            RetouchSuggestion {
                id: "beauty-tone_even".into(),
                group: "人像".into(),
                label: "人脸肤色均匀".into(),
                target: "faceBeauty".into(),
                field: "tone_even".into(),
                current: 0.0,
                suggested: 12.0,
                reason: "测试".into(),
                confidence: 0.5,
                applicable: true,
            },
        ];
        let (next, warnings) = apply_suggestions(&base, &suggestions, &faces);
        assert!(warnings.is_empty());
        assert_eq!(next.faces.len(), 1);
        assert_eq!(next.faces[0].face_uuid, "face-a");
        assert_eq!(next.faces[0].beauty.skin_smooth, 30.0, "不能降低已有强度");
        assert_eq!(next.faces[0].beauty.tone_even, 12.0);
    }
}
