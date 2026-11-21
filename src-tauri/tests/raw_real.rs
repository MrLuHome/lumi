//! 第七轮 T4：用真实 CC0 RAW 样本验证“只读原始像素与元数据”的能力。
//!
//! 样本目录：`/Users/lu/Library/Caches/lumi-raw-samples/`。
//! 找不到样本时打印提示并跳过，不制造失败；找到的样本必须满足：
//! 宽高 > 0、像素总数 > 0、CFA 来自文件、白平衡系数非空且不全为 0。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::image_engine::raw;

const SAMPLES: [&str; 5] = [
    "_MG_2233.CR3",
    "_DSC0812.ARW",
    "P1000951.RW2",
    "E_1__C106743_gredos.ORF",
    "_IGP2068.DNG",
];

/// LibRaw 0.21.1 明确不支持的样本（有损 DNG / JPEG-XL 压缩）。
/// 这里不断言“能读”，只在真被读到时大声提示，避免把不支持伪装成通过。
const UNSUPPORTED_SAMPLES: [&str; 1] = ["5G4A9395-compressed-lossy.DNG"];

fn sample_dir() -> PathBuf {
    PathBuf::from("/Users/lu/Library/Caches/lumi-raw-samples")
}

#[test]
fn raw_reads_real_sample() {
    let dir = sample_dir();
    let mut checked = 0usize;

    for name in SAMPLES {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("[跳过] 缺少样本：{}", path.display());
            continue;
        }
        checked += 1;
        probe(&path);
    }

    for name in UNSUPPORTED_SAMPLES {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("[跳过] 缺少已知不支持样本：{}", path.display());
            continue;
        }
        // 有损 DNG 依赖额外解码器，LibRaw 0.21.1 不支持。这里只记录真实结果：
        // 若哪天能读了，打印提示；若报错，必须是最清晰的“不支持”信息。
        match raw::raw_info(&path) {
            Ok(info) => eprintln!(
                "[意外可读] {} 现在能读了：{}（请更新能力矩阵）",
                path.display(),
                info.summary()
            ),
            Err(err) => eprintln!("[明确不支持] {}：{err}", path.display()),
        }
    }

    if checked == 0 {
        eprintln!("[跳过] 未找到任何 RAW 样本目录：{}", dir.display());
    }
    assert!(checked > 0, "样本目录存在但一个样本都没读到");
}

/// T5.5：预览与导出必须共用同一 develop 函数。
///
/// 导出路径 = `develop(max_edge=None)`；预览路径 = `develop(max_edge=900)`。
/// 二者应当等于“全尺寸显影后再缩放到 900px”，逐像素一致。
#[test]
fn raw_preview_and_export_share_develop() {
    use lumi_retouch_lib::image_engine::buffer::fit_within;
    use lumi_retouch_lib::image_engine::raw::RawDevelopParams;

    let candidates = ["E_1__C106743_gredos.ORF", "_IGP2068.DNG"];
    let Some(path) = candidates
        .iter()
        .map(|name| sample_dir().join(name))
        .find(|path| path.exists())
    else {
        eprintln!("[跳过] 没有可用于 develop 一致性的样本");
        return;
    };

    let before = std::fs::metadata(&path).unwrap().len();
    let full_params = RawDevelopParams {
        exposure_ev: 0.5,
        max_edge: None,
        ..Default::default()
    };
    let full = raw::develop_from_path(&path, &full_params)
        .unwrap_or_else(|err| panic!("{} 全尺寸显影失败：{err}", path.display()));

    let mut preview_params = full_params.clone();
    preview_params.max_edge = Some(900);
    let preview = raw::develop_from_path(&path, &preview_params)
        .unwrap_or_else(|err| panic!("{} 预览显影失败：{err}", path.display()));

    let expected = fit_within(&full.image, 900);
    assert_eq!(
        (preview.image.width(), preview.image.height()),
        (expected.width(), expected.height()),
        "预览尺寸必须由同一缩放规则决定"
    );
    assert!(
        preview.image.as_raw() == expected.as_raw(),
        "预览与全尺寸缩放的像素不一致，说明走了不同显影路径"
    );

    // 原图只读：显影不会修改文件。
    assert_eq!(
        before,
        std::fs::metadata(&path).unwrap().len(),
        "显影过程中原图大小发生变化"
    );

    // 参数必须真实改变像素。
    let mut zero_params = full_params.clone();
    zero_params.exposure_ev = 0.0;
    let zero = raw::develop_from_path(&path, &zero_params).unwrap();
    assert!(
        zero.image.as_raw() != full.image.as_raw(),
        "曝光参数没有改变任何像素"
    );
    eprintln!(
        "RAW 一致性：{}×{}，预览 {}×{}，曝光 +0.5EV 生效",
        full.image.width(),
        full.image.height(),
        preview.image.width(),
        preview.image.height()
    );
}

fn probe(path: &Path) {
    let info = raw::raw_info(path)
        .unwrap_or_else(|err| panic!("{} 元数据读取失败：{err}", path.display()));

    assert!(
        info.width > 0 && info.height > 0,
        "{} 有效宽高为 0",
        path.display()
    );
    assert!(
        info.raw_width > 0 && info.raw_height > 0,
        "{} 原始宽高为 0",
        path.display()
    );
    assert!(
        !info.cfa.pattern.is_empty(),
        "{} 没有从文件得到 CFA 排列",
        path.display()
    );
    assert!(
        matches!(
            info.cfa.pattern.as_str(),
            "RGGB" | "BGGR" | "GRBG" | "GBRG" | "X-Trans"
        ),
        "{} CFA 排列无法识别：{}",
        path.display(),
        info.cfa.pattern
    );
    let covered = info
        .white_balance
        .cam_mul
        .iter()
        .filter(|value| **value != 0.0)
        .count();
    assert!(covered > 0, "{} 白平衡系数全为 0", path.display());
    assert!(
        info.white_balance.cam_mul.iter().any(|value| *value != 0.0),
        "{} cam_mul 全为 0",
        path.display()
    );

    let linear = raw::decode_linear(path)
        .unwrap_or_else(|err| panic!("{} 线性解码失败：{err}", path.display()));
    assert_eq!((linear.width, linear.height), (info.width, info.height));
    assert!(
        linear.pixel_count() > 0,
        "{} 有效像素总数为 0",
        path.display()
    );
    assert_eq!(
        linear.pixel_count(),
        (linear.width * linear.height) as usize
    );
    assert_eq!(linear.cfa, info.cfa.pattern);

    let preview = raw::embedded_preview(path)
        .unwrap_or_else(|err| panic!("{} 内嵌预览读取失败：{err}", path.display()));

    eprintln!("──────── {} ────────", path.display());
    eprintln!("{}", info.summary());
    eprintln!(
        "文件 {} KB | 白点 {} | 相机矩阵 {:?} | EXIF 时间戳 {} | 内嵌预览 {} 张 ({:?}) → {}",
        info.file_size / 1024,
        info.white_level,
        info.camera_matrix,
        info.exif.timestamp,
        if info.has_embedded_preview {
            "有"
        } else {
            "无"
        },
        info.preview,
        preview
            .as_ref()
            .map(|p| format!(
                "{} {}×{} {} 字节",
                p.format,
                p.width,
                p.height,
                p.data.len()
            ))
            .unwrap_or_else(|| "不可用".to_string()),
    );
    eprintln!(
        "线性 CFA {}×{} = {} 像素 | 黑电平 {:?} | 白平衡 {:?}",
        linear.width,
        linear.height,
        linear.pixel_count(),
        linear.black_level,
        linear.white_balance
    );

    assert_eq!(
        linear.white_balance,
        [
            info.white_balance.cam_mul[0],
            info.white_balance.cam_mul[1],
            info.white_balance.cam_mul[2],
            info.white_balance.cam_mul[3],
        ]
    );
}
