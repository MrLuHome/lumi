//! MediaPipe Selfie Multiclass 256×256 皮肤语义分割（ONNX / tract）。
//!
//! 职责单一：加载随包权重、跑推理、把 6 类 logits 转成皮肤 alpha，
//! 再与几何蒙版融合。几何蒙版的五官保护 / 高光补偿 / 手绘后处理
//! 全部复用 `super` 里的同一套实现，模型分支不另起一套。
//!
//! **绝不静默吞掉失败**：加载与推理失败都记录到 `LAST_FAILURE`，
//! 由 `model::status()` 在 UI 状态里如实披露；`infer()` 返回 `Err` 后
//! `build_skin_mask` 回退几何蒙版，进程不挂。

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use image::imageops::FilterType;
use image::{Rgb, RgbImage};

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::inference::{
    CpuTractBackend, InferenceBackend, LoadedPlan, RunOptions, TensorBatch,
};
use crate::ai_engine::model_registry::builtin;
use crate::ai_engine::model_registry::locate;
use crate::ai_engine::model_registry::spec::ModelSpec;
use crate::ai_engine::model_registry::state::{ModelFailure, ModelHealth};
use crate::ai_engine::model_registry::{license, verify};
use crate::error::{AppError, AppResult};
use crate::face_intelligence::model_hub::security::sha256_file;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::ManualMask;

use super::SkinMask;

const INPUT_SIDE: u32 = 256;
const CLASSES: usize = 6;
const BODY_SKIN: usize = 2;
const FACE_SKIN: usize = 3;
/// 单张推理上限。tract 不暴露算子级中断钩子，超时只在 run 返回后检查
/// （`inference/backend.rs::RunOptions` 的既有语义）。
const INFER_TIMEOUT: Duration = Duration::from_secs(12);

/// 已编译 plan 只加载一次；失败不缓存，便于权重被修复后重试。
static PLAN: OnceLock<Arc<LoadedPlan>> = OnceLock::new();
/// 由 `commands` 层在启动时注入的打包资源目录（不在这里调用 `resource_dir()`）。
static BASE_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// 最近一次加载 / 推理失败原因，供 `SkinModelStatus.reason` 显示。
static LAST_FAILURE: Mutex<Option<String>> = Mutex::new(None);

/// 命令层在启动时注入资源目录；`ai_engine` 保持零 Tauri 依赖。
pub fn configure_base_dirs(dirs: Vec<PathBuf>) {
    let _ = BASE_DIRS.set(dirs);
}

/// 供 `model::status()` 读取；成功加载后自动清空。
pub fn last_failure() -> Option<String> {
    LAST_FAILURE
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
}

fn record_failure(reason: impl Into<String>) {
    if let Ok(mut guard) = LAST_FAILURE.lock() {
        *guard = Some(reason.into());
    }
}

fn clear_failure() {
    if let Ok(mut guard) = LAST_FAILURE.lock() {
        *guard = None;
    }
}

/// 加载（或取回缓存的）plan。首次调用会做浅检查 + SHA256 + 契约 + 编译，
/// 必须在后台线程调用。失败返回 `None` 并记录原因。
pub fn load() -> Option<Arc<LoadedPlan>> {
    if let Some(plan) = PLAN.get() {
        return Some(Arc::clone(plan));
    }
    let mut dirs = BASE_DIRS.get().cloned().unwrap_or_default();
    dirs.extend(locate::dev_base_dirs());
    match load_from_spec(&builtin::SELFIE_SKIN, &dirs) {
        Ok(plan) => {
            let plan = Arc::new(plan);
            let _ = PLAN.set(Arc::clone(&plan));
            clear_failure();
            PLAN.get().cloned().or(Some(plan))
        }
        Err(reason) => {
            record_failure(reason);
            None
        }
    }
}

/// 真正的准入链，供 `load()` 与失败注入测试使用。
///
/// 顺序：① 许可（纯内存，先于任何文件字节读取）→ ② 存在 → ③ 体积/头 →
/// ④ SHA256（流式，后台线程）→ ⑤ 契约 + 优化 + 编译。
pub fn load_from_spec(spec: &ModelSpec, base_dirs: &[PathBuf]) -> Result<LoadedPlan, String> {
    let located = locate::locate(base_dirs, spec.dir, spec.file_name);
    let health = match &located {
        Some(path) => verify::check_shallow(spec, path),
        None => {
            let mut health = ModelHealth::new();
            if let Err(failure) = license::gate(spec.origin, &spec.license, spec.id) {
                health.push(failure);
            }
            health.push(ModelFailure::Missing {
                model_id: spec.id.to_string(),
                searched: locate::describe(&locate::candidates(
                    base_dirs,
                    spec.dir,
                    spec.file_name,
                )),
            });
            health
        }
    };
    if let Some(failure) = &health.blocking {
        let detail = failure
            .detail()
            .map(str::to_string)
            .unwrap_or_else(|| failure.message());
        return Err(format!("{}：{detail}", failure.message()));
    }
    let path = health
        .resolved_path
        .ok_or_else(|| "权重路径未解析".to_string())?;

    // SHA256 只在真正加载时算一次（绝不放在状态查询里，历史风险 R1）。
    let actual = sha256_file(&path).map_err(|error| format!("无法计算权重 SHA256：{error}"))?;
    if !actual.eq_ignore_ascii_case(spec.sha256) {
        return Err(format!(
            "权重 SHA256 不符（期望 {}，实际 {actual}）",
            spec.sha256
        ));
    }

    CpuTractBackend::new()
        .load(spec, &path, &CancelToken::default())
        .map_err(|error| format!("权重加载失败：{error}"))
}

/// 对单张人脸跑模型分支，返回与几何蒙版同结构的 `SkinMask`。
///
/// 任一步失败都返回 `Err`（由调用方回退几何蒙版），并写入 `LAST_FAILURE`。
pub fn infer(
    plan: &LoadedPlan,
    image: &Image16,
    geometry: &FaceGeometry,
    manual: &ManualMask,
) -> AppResult<SkinMask> {
    let bbox = super::face_bbox(geometry, image.width(), image.height());
    let (x0, y0, width, height) = bbox;
    let (crop, crop_x, crop_y, crop_side) = build_crop(image, bbox);
    let skin_map = run_segmentation(plan, &crop)?;

    let mut model_alpha = vec![0f32; (width * height) as usize];
    let protect = super::feature_protection(geometry, width, height, x0, y0);
    for (index, value) in model_alpha.iter_mut().enumerate() {
        let local_x = index as u32 % width;
        let local_y = index as u32 / width;
        let source_x = ((x0 + local_x as i32) as f32 - crop_x) / crop_side * INPUT_SIDE as f32;
        let source_y = ((y0 + local_y as i32) as f32 - crop_y) / crop_side * INPUT_SIDE as f32;
        let skin = bilinear(&skin_map, source_x, source_y);
        *value = (skin * protect[index]).clamp(0.0, 1.0);
    }

    // 安全网：模型对眼睛/嘴唇并非逐像素完美，几何置信度取 max 保留其保护。
    let geometry_alpha = super::geometry_alpha(image, geometry, x0, y0, width, height);
    let fused = fuse_alpha(&model_alpha, &geometry_alpha);

    Ok(super::finalize_mask(
        fused,
        geometry,
        manual,
        x0,
        y0,
        width,
        height,
        image.width(),
        image.height(),
    ))
}

/// 从包围盒取方形裁剪并缩放到 256×256 RGB。
/// 返回 (裁剪图, 裁剪左上角 x, 裁剪左上角 y, 裁剪边长)。
fn build_crop(
    image: &Image16,
    bbox: (i32, i32, u32, u32),
) -> (RgbImage, f32, f32, f32) {
    let (x0, y0, width, height) = bbox;
    let center_x = x0 as f32 + width as f32 * 0.5;
    let center_y = y0 as f32 + height as f32 * 0.5;
    // 1.5× 包围盒较长边，覆盖全脸与少量上下文，同时不引入过多背景。
    let side = ((width.max(height) as f32 * 1.5).round() as u32).max(16);
    let crop_x = (center_x - side as f32 * 0.5).round();
    let crop_y = (center_y - side as f32 * 0.5).round();

    let mut raw = RgbImage::new(side, side);
    let max_x = image.width().saturating_sub(1) as i32;
    let max_y = image.height().saturating_sub(1) as i32;
    for local_y in 0..side {
        for local_x in 0..side {
            let source_x = (crop_x as i32 + local_x as i32).clamp(0, max_x) as u32;
            let source_y = (crop_y as i32 + local_y as i32).clamp(0, max_y) as u32;
            let pixel = image.get_pixel(source_x, source_y).0;
            raw.put_pixel(
                local_x,
                local_y,
                Rgb([
                    (pixel[0] as f32 / 257.0).round() as u8,
                    (pixel[1] as f32 / 257.0).round() as u8,
                    (pixel[2] as f32 / 257.0).round() as u8,
                ]),
            );
        }
    }
    let resized = image::imageops::resize(&raw, INPUT_SIDE, INPUT_SIDE, FilterType::Triangle);
    (resized, crop_x, crop_y, side as f32)
}

/// 预处理 `[1,3,256,256]` F32 RGB [-1,1] → run → axis=1 softmax → 皮肤概率图。
fn run_segmentation(plan: &LoadedPlan, crop: &RgbImage) -> AppResult<Vec<f32>> {
    let spec = &builtin::SELFIE_SKIN;
    let mut data = vec![0f32; 3 * (INPUT_SIDE as usize) * (INPUT_SIDE as usize)];
    let plane = (INPUT_SIDE as usize) * (INPUT_SIDE as usize);
    for y in 0..INPUT_SIDE {
        for x in 0..INPUT_SIDE {
            let pixel = crop.get_pixel(x, y).0;
            for channel in 0..3 {
                // (x/127.5)-1 == (x/255)*2-1，与 ValueRange::MinusOneToOne 一致。
                data[channel * plane + y as usize * INPUT_SIDE as usize + x as usize] =
                    pixel[channel] as f32 / 127.5 - 1.0;
            }
        }
    }
    let input = TensorBatch::new_for(spec.id, spec.input.clone(), data)
        .map_err(|error| AppError::Unsupported(format!("皮肤解析输入契约错误：{error}")))?;
    let outputs = CpuTractBackend::new()
        .run(
            plan,
            &input,
            &RunOptions {
                timeout: Some(INFER_TIMEOUT),
                allow_quantized: false,
            },
            &CancelToken::default(),
        )
        .map_err(|error| {
            let reason = format!("皮肤解析推理失败：{error}");
            record_failure(reason.clone());
            AppError::Unsupported(reason)
        })?;
    let logits = outputs
        .first()
        .map(|batch| batch.data.as_slice())
        .ok_or_else(|| {
            let reason = "皮肤解析模型没有返回输出";
            record_failure(reason);
            AppError::Unsupported(reason.into())
        })?;

    let probabilities = softmax_classes(logits)?;
    Ok(skin_alpha_from_probs(&probabilities))
}

/// 沿类别维（NCHW 的 axis=1，步长 = 256×256）做**数值稳定**的 softmax。
/// 图内 6 个 Softmax 都在 ViT 注意力内部，不是类别维，必须在这里自行做。
///
/// tract 输出布局是 `[1,6,256,256]`（NCHW），flat 缓冲为 `c * 65536 + y*256 + x`：
/// 同一像素 p 的 6 个类别 logits **不相邻**，而是位于 `class*pixel_count + p`。
/// 曾错误地按 `p*CLASSES + class`（逐像素交错 NHWC）索引，导致输出恒定 1/3、
/// 与图像内容无关的伪蒙版；这里必须按 NCHW stride 取数。
pub fn softmax_classes(logits: &[f32]) -> AppResult<Vec<f32>> {
    let pixel_count = (INPUT_SIDE as usize) * (INPUT_SIDE as usize);
    if logits.len() != CLASSES * pixel_count {
        let reason = format!(
            "皮肤解析输出形状异常：期望 {} 个 logits，实际 {}",
            CLASSES * pixel_count,
            logits.len()
        );
        record_failure(reason.clone());
        return Err(AppError::Unsupported(reason));
    }
    if let Some(index) = logits.iter().position(|value| !value.is_finite()) {
        let reason = format!("皮肤解析输出第 {index} 个元素不是有限值（NaN/Inf）");
        record_failure(reason.clone());
        return Err(AppError::Unsupported(reason));
    }

    // 每个像素的最大值必须跨 6 个类别取：max_c logits[c*pixel_count + p]。
    let mut maxima = vec![f32::NEG_INFINITY; pixel_count];
    for class in 0..CLASSES {
        let offset = class * pixel_count;
        for (pixel, max) in maxima.iter_mut().enumerate() {
            *max = (*max).max(logits[offset + pixel]);
        }
    }

    // exp 后按类累加分母，probabilities 与 logits 保持同布局（NCHW）。
    let mut probabilities = vec![0f32; logits.len()];
    let mut sums = vec![0f32; pixel_count];
    for class in 0..CLASSES {
        let offset = class * pixel_count;
        for pixel in 0..pixel_count {
            let idx = offset + pixel;
            let exponent = (logits[idx] - maxima[pixel]).exp();
            probabilities[idx] = exponent;
            sums[pixel] += exponent;
        }
    }

    for pixel in 0..pixel_count {
        let sum = sums[pixel];
        if !sum.is_finite() || sum <= 0.0 {
            let reason = format!("皮肤解析 softmax 在第 {pixel} 个像素上无效（sum={sum}）");
            record_failure(reason.clone());
            return Err(AppError::Unsupported(reason));
        }
        for class in 0..CLASSES {
            probabilities[class * pixel_count + pixel] /= sum;
        }
    }
    Ok(probabilities)
}

/// 整脸平均皮肤概率低于此值才认为模型整体失效，需要几何兜底。
/// 正常推理（椭圆脸 + 少量上下文）均值约 0.3~0.6，远高于此阈值。
const LOW_CONF_MEAN: f32 = 0.12;
/// 模型失效时几何兜底的最大权重，保留上限避免退回纯几何矩形。
const MAX_GEOMETRY_GATE: f32 = 0.9;

/// 融合模型 alpha 与几何 alpha：**模型主导**，几何只在模型整体失效时受限兜底。
///
/// 旧实现是逐元素 `max(model, geometry)`：几何蒙版本身覆盖额头-下巴的矩形区域，
/// 脸颊处 alpha 本就高，`max` 会把模型判定为非皮肤的区域（发际、下颌角、背景）
/// 顶成高值，最终蒙版退化成与脸型无关的硬矩形——真人取证已证实这一缺陷。
///
/// 五官保护已在 `infer` 里乘进 `model_alpha`，无需几何再参与。因此这里按整脸平均
/// 皮肤置信度算一个全局 gate：模型健康（均值 ≥ `LOW_CONF_MEAN`）时 gate=0，
/// 蒙版形状完全由模型决定；只有模型近乎检测不到皮肤时才按比例抬高 gate，
/// 混入几何作安全网，避免完全丢失蒙版。这样「真跑通推理」才是名副其实的 model 分支。
pub fn fuse_alpha(model_alpha: &[f32], geometry_alpha: &[f32]) -> Vec<f32> {
    let n = model_alpha.len().min(geometry_alpha.len());
    if n == 0 {
        return Vec::new();
    }
    let mean = model_alpha[..n].iter().sum::<f32>() / n as f32;
    let gate = if mean >= LOW_CONF_MEAN {
        0.0
    } else {
        (1.0 - (mean / LOW_CONF_MEAN).max(0.0)) * MAX_GEOMETRY_GATE
    };
    model_alpha[..n]
        .iter()
        .zip(geometry_alpha[..n].iter())
        .map(|(model, geometry_value)| (model.max(*geometry_value * gate)).clamp(0.0, 1.0))
        .collect()
}

/// 取 `p(body-skin=2) + p(face-skin=3)` 作为皮肤置信度。
/// 概率缓冲与 tract 输出同为 NCHW：通道 c 的第 pixel 个像素在 `c*pixel_count + pixel`。
pub fn skin_alpha_from_probs(probabilities: &[f32]) -> Vec<f32> {
    let pixel_count = probabilities.len() / CLASSES;
    (0..pixel_count)
        .map(|pixel| {
            (probabilities[BODY_SKIN * pixel_count + pixel]
                + probabilities[FACE_SKIN * pixel_count + pixel])
                .clamp(0.0, 1.0)
        })
        .collect()
}

/// 双线性采样 256×256 概率图（坐标可落在像素中心之间）。
fn bilinear(map: &[f32], x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (INPUT_SIDE - 1) as f32);
    let y = y.clamp(0.0, (INPUT_SIDE - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min((INPUT_SIDE - 1) as usize);
    let y1 = (y0 + 1).min((INPUT_SIDE - 1) as usize);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let row0 = map[y0 * INPUT_SIDE as usize + x0] * (1.0 - fx)
        + map[y0 * INPUT_SIDE as usize + x1] * fx;
    let row1 = map[y1 * INPUT_SIDE as usize + x0] * (1.0 - fx)
        + map[y1 * INPUT_SIDE as usize + x1] * fx;
    (row0 * (1.0 - fy) + row1 * fy).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::model_registry::license::LicenseRecord;

    #[test]
    fn softmax_classes_is_stable_and_sums_to_one() {
        let pixels = INPUT_SIDE as usize * INPUT_SIDE as usize;
        let mut logits = vec![0f32; CLASSES * pixels];
        // 极端 logit 必须放在**同一像素的不同类别**（NCHW: class*pixels + pixel），
        // 才能检验最大值是跨 6 个类别取的；负 logits 也不能当概率直接用。
        fn nchw(class: usize, pixel: usize, pixels: usize) -> usize {
            class * pixels + pixel
        }
        logits[nchw(0, 0, pixels)] = -1.49;
        logits[nchw(1, 0, pixels)] = 3.05;
        logits[nchw(0, 1, pixels)] = -100.0;
        logits[nchw(1, 1, pixels)] = 100.0;
        let probs = softmax_classes(&logits).unwrap();
        assert_eq!(probs.len() / CLASSES, pixels);
        assert!(probs.iter().all(|value| *value >= 0.0 && value.is_finite()));
        // 一个像素的 6 类概率位于 class*pixels + pixel（不是相邻 6 个）。
        for pixel in [0usize, 1, pixels - 1] {
            let sum: f32 = (0..CLASSES).map(|class| probs[class * pixels + pixel]).sum();
            assert!((sum - 1.0).abs() < 1e-5, "像素 {pixel} 概率和 {sum}");
        }
        let skin = skin_alpha_from_probs(&probs);
        assert!(skin.iter().all(|value| *value >= 0.0 && *value <= 1.0));
    }

    #[test]
    fn softmax_rejects_nan_output() {
        let mut logits = vec![0f32; CLASSES * INPUT_SIDE as usize * INPUT_SIDE as usize];
        logits[10] = f32::NAN;
        let error = softmax_classes(&logits).unwrap_err();
        assert!(error.to_string().contains("NaN"));
    }

    #[test]
    fn wrong_output_size_is_rejected() {
        let error = softmax_classes(&[0.0; 6]).unwrap_err();
        assert!(error.to_string().contains("输出形状异常"));
    }

    #[test]
    fn load_from_spec_rejects_bad_license_before_reading_bytes() {
        let bad = ModelSpec {
            license: LicenseRecord {
                weights: crate::ai_engine::model_registry::license::Verdict3::NotAllowed,
                ..builtin::SELFIE_SKIN.license
            },
            ..builtin::SELFIE_SKIN.clone()
        };
        let dirs = locate::dev_base_dirs();
        let error = load_from_spec(&bad, &dirs).unwrap_err();
        assert!(error.contains("许可") || error.contains("不允许"), "{error}");
    }

    #[test]
    fn load_from_spec_rejects_wrong_sha() {
        let bad = ModelSpec {
            sha256: "deadbeef",
            ..builtin::SELFIE_SKIN.clone()
        };
        let error = load_from_spec(&bad, &locate::dev_base_dirs()).unwrap_err();
        assert!(error.contains("SHA256 不符"), "{error}");
    }

    #[test]
    fn load_from_spec_rejects_missing_weight() {
        let error = load_from_spec(&builtin::SELFIE_SKIN, &[PathBuf::from("/nonexistent")])
            .unwrap_err();
        assert!(error.contains("缺失"), "{error}");
    }

    #[test]
    fn shallow_check_skips_the_full_file_hash() {
        // 体积对、头合法、内容错误：浅检查必须通过（说明没算 SHA），
        // 全检查必须报 SHA 不符（说明真的算了）。
        let dir = std::env::temp_dir().join(format!("lumi-skin-shallow-{}", std::process::id()));
        let fake = dir.join(builtin::SELFIE_SKIN.file_name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&fake, vec![0x08u8; builtin::SELFIE_SKIN.size_bytes as usize]).unwrap();

        let shallow = verify::check_shallow(&builtin::SELFIE_SKIN, &fake);
        assert!(
            shallow.blocking.is_none(),
            "浅检查不应计算 SHA：{:?}",
            shallow.findings
        );
        let full = verify::check_at(&builtin::SELFIE_SKIN, &fake, false);
        let failure = full.blocking.expect("全检查必须发现内容不符");
        assert_eq!(failure.kind(), "corrupt");
        assert!(failure.actual().is_some(), "全检查必须给出实际 SHA");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn license_failure_never_reads_an_unreadable_weight() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("lumi-skin-noread-{}", std::process::id()));
        let fake = dir.join(builtin::SELFIE_SKIN.file_name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&fake, vec![0x08u8; builtin::SELFIE_SKIN.size_bytes as usize]).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&fake).is_ok() {
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o644)).ok();
            std::fs::remove_dir_all(&dir).ok();
            eprintln!("[skip] 当前用户可绕过 0o000，无法证明\"没读文件\"");
            return;
        }
        let bad = ModelSpec {
            license: LicenseRecord {
                weights: crate::ai_engine::model_registry::license::Verdict3::NotAllowed,
                ..builtin::SELFIE_SKIN.license
            },
            ..builtin::SELFIE_SKIN.clone()
        };
        let health = verify::check_shallow(&bad, &fake);
        assert_eq!(health.findings.len(), 1);
        assert_eq!(health.findings[0].kind(), "licenseBlocked");
        // 对照组：许可通过后必须真的去读，且读不到。
        let allowed = verify::check_shallow(&builtin::SELFIE_SKIN, &fake);
        let failure = allowed.blocking.expect("许可通过后必须尝试读取");
        assert_eq!(failure.kind(), "corrupt");
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o644)).ok();
        std::fs::remove_dir_all(&dir).ok();
    }
}
