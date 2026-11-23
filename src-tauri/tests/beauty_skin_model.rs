//! Selfie Multiclass 皮肤语义分割接入验收。
//!
//! 关键目标：证明 **tract 在应用代码路径（CpuTractBackend::load/run）真跑通**，
//! 而不是只有 /tmp 探针能跑；同时验证契约、降级与两路蒙版都可用。

use std::path::{Path, PathBuf};

use image::Rgba;
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry, NormPoint};
use lumi_retouch_lib::ai_engine::inference::{
    check_contract, inspect_contract, CpuTractBackend, InferenceBackend,
};
use lumi_retouch_lib::ai_engine::model_registry::builtin;
use lumi_retouch_lib::ai_engine::model_registry::locate;
use lumi_retouch_lib::ai_engine::skin_segmentation::onnx;
use lumi_retouch_lib::ai_engine::skin_segmentation::{build_skin_mask, model};
use lumi_retouch_lib::image_engine::buffer::Image16;
use lumi_retouch_lib::image_engine::params::ManualMask;

fn skin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(builtin::SELFIE_SKIN.dir)
        .join(builtin::SELFIE_SKIN.file_name)
}

fn skin_image(width: u32, height: u32) -> Image16 {
    Image16::from_pixel(
        width,
        height,
        Rgba([214 * 257, 170 * 257, 140 * 257, 65535]),
    )
}

fn frame(width: u32, height: u32) -> FaceFrame {
    let d = width as f32 * 0.2;
    let center_x = width as f32 * 0.5;
    let center_y = height as f32 * 0.4;
    let to_norm = |x: f32, y: f32| NormPoint {
        x: x / width as f32,
        y: y / height as f32,
    };
    FaceFrame {
        id: 0,
        uuid: String::new(),
        score: 0.95,
        x: (center_x - d * 1.6) / width as f32,
        y: (center_y - d * 1.4) / height as f32,
        width: d * 3.2 / width as f32,
        height: d * 3.6 / height as f32,
        landmarks: vec![
            to_norm(center_x - d * 0.5, center_y),
            to_norm(center_x + d * 0.5, center_y),
            to_norm(center_x, center_y + d * 0.55),
            to_norm(center_x - d * 0.35, center_y + d * 0.95),
            to_norm(center_x + d * 0.35, center_y + d * 0.95),
        ],
        dense: None,
        level3: None,
    }
}

#[test]
fn selfie_skin_contract_matches_the_actual_onnx_file() {
    let path = skin_path();
    assert!(path.is_file(), "随包 Selfie Multiclass ONNX 缺失");
    let snapshot = inspect_contract(&path, builtin::SELFIE_SKIN.id).unwrap();
    assert_eq!(snapshot.inputs, vec![vec![1, 3, 256, 256]]);
    assert_eq!(snapshot.outputs, vec![vec![1, 6, 256, 256]]);
    check_contract(&builtin::SELFIE_SKIN, &snapshot).unwrap();

    // inspect() 只读结构、不编译 runnable。
    let backend = CpuTractBackend::new();
    let introspection = backend
        .inspect(&path, &lumi_retouch_lib::image_engine::cancel::CancelToken::default())
        .unwrap();
    assert!(introspection.ir_version.unwrap_or(0) > 0);
    assert!(introspection.opset.unwrap_or(0) > 0);
    assert!(introspection.operator_count >= 200, "算子数量异常");
}

#[test]
fn app_path_loads_real_onnx_and_runs_inference() {
    // 这是应用路径实测：准入链 + tract load + tract run 全部走生产代码。
    let plan = onnx::load_from_spec(&builtin::SELFIE_SKIN, &locate::dev_base_dirs())
        .expect("应用路径加载 Selfie Multiclass 失败");
    assert_eq!(plan.model_id, builtin::SELFIE_SKIN.id);
    assert_eq!(plan.backend, "cpu-tract");
    assert_eq!(plan.output_count(), 1);

    let image = skin_image(400, 400);
    let face = frame(400, 400);
    let geometry = FaceGeometry::from_face(&face, 400, 400).unwrap();
    let mask = onnx::infer(&plan, &image, &geometry, &ManualMask::default())
        .expect("应用路径推理失败");
    assert!(mask.width > 0 && mask.height > 0);
    assert!(!mask.alpha.is_empty());
    assert!(
        mask.alpha
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)),
        "皮肤 alpha 必须是 [0,1] 内的有限值"
    );
    assert!(mask.active_pixels() > 0, "皮肤蒙版不应为空");
}

#[test]
fn build_skin_mask_dispatches_to_the_model_branch_without_changing_signature() {
    let image = skin_image(360, 360);
    let face = frame(360, 360);
    let status = model::status(None);
    assert!(status.available, "随包权重已登记，浅检查应通过");
    assert_eq!(
        status.method,
        "onnx:skin-parse-mediapipe-selfie-multiclass-256"
    );
    let mask = build_skin_mask(&image, &face, &ManualMask::default())
        .expect("分派到模型分支后不得失败");
    assert_eq!(mask.width > 0, true);
    assert_eq!(mask.height > 0, true);
}

#[test]
fn softmax_output_is_a_probability_distribution_not_raw_logits() {
    // 论文/模型卡类别顺序：0 bg / 1 hair / 2 body-skin / 3 face-skin / 4 clothes / 5 others。
    let pixels = 256 * 256;
    let mut logits = vec![0f32; 6 * pixels];
    logits[0] = -1.4928; // 负数不能当概率使用
    logits[1] = 3.0486;
    let probabilities = onnx::softmax_classes(&logits).unwrap();
    assert!(probabilities.iter().all(|value| *value >= 0.0 && *value <= 1.0));
    // tract 输出是 NCHW [1,6,256,256]：同一像素的 6 类概率在 class*pixels + pixel，
    // 不是相邻 6 个 flat 元素（后者是 6 个不同像素的同一类别）。
    for pixel in [0usize, 7, pixels - 1] {
        let sum: f32 = (0..6).map(|class| probabilities[class * pixels + pixel]).sum();
        assert!((sum - 1.0).abs() < 1e-5, "像素 {pixel} 概率和 {sum}");
    }
    let skin = onnx::skin_alpha_from_probs(&probabilities);
    assert_eq!(skin.len(), pixels);
    assert!(skin.iter().all(|value| *value >= 0.0 && *value <= 1.0));
}

/// 按 NCHW `[6,256,256]` 造 logits：通道 c 的第 p 个像素在 `c*PIXELS + p`。
const PIXELS: usize = 256 * 256;
const CLASSES: usize = 6;
const BODY_SKIN: usize = 2;
const FACE_SKIN: usize = 3;
fn nchw(class: usize, pixel: usize) -> usize {
    class * PIXELS + pixel
}

#[test]
fn skin_mask_tracks_image_content_left_vs_right() {
    // 内容相关 fixture：皮肤类(2/3)左半 logit=+5、右半=-5，其余类保持 0。
    // 旧的错误 stride（pixel*6+class）只会读到 class0 全 0 的通道，得到恒定 1/3 → 本测试变红。
    let mut logits = vec![0f32; CLASSES * PIXELS];
    for y in 0..256usize {
        for x in 0..256usize {
            let p = y * 256 + x;
            let value = if x < 128 { 5.0 } else { -5.0 };
            logits[nchw(BODY_SKIN, p)] = value;
            logits[nchw(FACE_SKIN, p)] = value;
        }
    }
    let probabilities = onnx::softmax_classes(&logits).unwrap();
    let skin = onnx::skin_alpha_from_probs(&probabilities);

    let column = |x: usize| -> f32 {
        (0..256usize).map(|y| skin[y * 256 + x]).sum::<f32>() / 256.0
    };
    let left = column(64);
    let right = column(192);
    assert!(
        left - right > 0.3,
        "左半 p(skin)={left}，右半 p(skin)={right}：蒙版必须跟内容走，不能是恒定 1/3 的伪蒙版"
    );
}

#[test]
fn permuting_class_channels_changes_the_mask() {
    // 基线：皮肤信号全图放在真皮肤通道 2/3。
    let mut base = vec![0f32; CLASSES * PIXELS];
    for p in 0..PIXELS {
        base[nchw(BODY_SKIN, p)] = 5.0;
        base[nchw(FACE_SKIN, p)] = 5.0;
    }
    let base_skin = onnx::skin_alpha_from_probs(&onnx::softmax_classes(&base).unwrap());

    // 轮转类别通道：old c -> new perm[c]，把皮肤信号从 2/3 搬到 4/5，
    // 且 class0 通道仍是全 0。正确实现读 2/3 → 蒙版剧变；
    // 错误实现读 class0 交错位置 → 蒙版不变（证明它读串了通道）。
    let perm = [0usize, 1, 4, 5, 2, 3];
    let mut shuffled = vec![0f32; CLASSES * PIXELS];
    for p in 0..PIXELS {
        for class in 0..CLASSES {
            shuffled[nchw(perm[class], p)] = base[nchw(class, p)];
        }
    }
    let shuffled_skin = onnx::skin_alpha_from_probs(&onnx::softmax_classes(&shuffled).unwrap());

    let mean = |map: &[f32]| -> f32 { map.iter().sum::<f32>() / map.len() as f32 };
    let base_mean = mean(&base_skin);
    let shuffled_mean = mean(&shuffled_skin);
    assert!(
        (base_mean - shuffled_mean).abs() > 0.3,
        "通道打乱后蒙版应显著变化：base={base_mean}，shuffled={shuffled_mean}；\
         若两者相等说明实现读串了类别通道"
    );
}

#[test]
fn failure_injection_never_panics_and_never_loads_untrusted_bytes() {
    use lumi_retouch_lib::ai_engine::model_registry::license::{LicenseRecord, Verdict3};
    use lumi_retouch_lib::ai_engine::model_registry::spec::ModelSpec;

    // ① 许可未过：纯内存闸门，先于文件读取。
    let bad_license = ModelSpec {
        license: LicenseRecord {
            weights: Verdict3::NotAllowed,
            ..builtin::SELFIE_SKIN.license
        },
        ..builtin::SELFIE_SKIN.clone()
    };
    let error = onnx::load_from_spec(&bad_license, &locate::dev_base_dirs()).unwrap_err();
    assert!(error.contains("不允许") || error.contains("许可"), "{error}");

    // ② SHA 不符：即使文件是真实 ONNX，也必须拒绝。
    let bad_sha = ModelSpec {
        sha256: "deadbeef",
        ..builtin::SELFIE_SKIN.clone()
    };
    let error = onnx::load_from_spec(&bad_sha, &locate::dev_base_dirs()).unwrap_err();
    assert!(error.contains("SHA256 不符"), "{error}");

    // ③ 权重缺失：不得 panic，报缺失。
    let error =
        onnx::load_from_spec(&builtin::SELFIE_SKIN, &[PathBuf::from("/nonexistent/lumi")])
            .unwrap_err();
    assert!(error.contains("缺失"), "{error}");

    // ④ 非 ONNX 内容塞进正确路径：体积对齐、伪装头合法，也必须被 SHA 挡下。
    let dir = std::env::temp_dir().join(format!("lumi-skin-fake-{}", std::process::id()));
    let nested = dir.join(builtin::SELFIE_SKIN.dir);
    std::fs::create_dir_all(&nested).unwrap();
    let fake = nested.join(builtin::SELFIE_SKIN.file_name);
    std::fs::write(&fake, vec![0x08u8; builtin::SELFIE_SKIN.size_bytes as usize]).unwrap();
    let error = onnx::load_from_spec(&builtin::SELFIE_SKIN, &[dir.clone()]).unwrap_err();
    assert!(
        error.contains("SHA256 不符") || error.contains("文件头"),
        "{error}"
    );
    std::fs::remove_dir_all(&dir).ok();

    // 所有注入之后，几何蒙版仍可用（不崩溃）。
    let mask = build_skin_mask(&skin_image(300, 300), &frame(300, 300), &ManualMask::default())
        .expect("模型失败必须回退几何蒙版而不是报错");
    assert!(mask.active_pixels() > 0);
}
