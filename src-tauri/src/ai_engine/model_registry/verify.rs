//! 权重校验与准入链实现。
//!
//! 顺序固定（见 [`super::state`] 的文档）：许可 → 存在 → 格式 → 契约 → SHA256 → 解析。
//! 本文件负责前三项 + SHA；契约（第 ④ 步）复用 `inference::cpu_tract::inspect_contract`，
//! 解析（第 ⑥ 步）由 `InferenceBackend::load` 负责。
//!
//! **未通过许可时不读任何字节**：存在性判断只用 `is_file()`（一次 `metadata`），
//! 体积/头字节/SHA 全部跳过。这一点由
//! `tests/model_registry.rs::license_block_never_reads_the_file` 断言。

use std::path::{Path, PathBuf};

use crate::face_intelligence::model_hub::security::sha256_file;

use super::locate;
use super::spec::ModelSpec;
use super::state::{ModelFailure, ModelHealth};

/// 伪装格式的识别标记（与 `model_hub::security` 保持同一套判据）。
const FORBIDDEN_MARKERS: [(&[u8], &str); 3] = [
    (b"\x7fELF", "文件是可执行文件（ELF），不是 ONNX 模型"),
    (b"\x93NUMPY", "文件是 NumPy 数据，不是 ONNX 模型"),
    (b"%PDF", "文件是 PDF，不是 ONNX 模型"),
];

/// 校验一个**已知路径**的权重。`include_contract` 为 `false` 时把第 ④ 步
/// 推迟到 `InferenceBackend::load`（那里本来就要解析一次，避免重复解析 232 KB 权重）。
pub fn check_at(spec: &ModelSpec, path: &Path, include_contract: bool) -> ModelHealth {
    let mut health = ModelHealth::new();

    // ① 许可闸门：纯内存，先于一切文件 IO。
    if let Err(failure) = super::license::gate(spec.origin, &spec.license, spec.id) {
        health.push(failure);
        // 许可未过：连体积与哈希都不做，直接返回。
        // 存在性仍然检查一次（成本是一次 metadata），以便 findings 能同时暴露
        // “未授权 + 文件也不存在”，不让 Missing 掩盖 License。
        if !path.is_file() {
            health.push(ModelFailure::Missing {
                model_id: spec.id.to_string(),
                searched: vec![path.to_string_lossy().into_owned()],
            });
        }
        return health;
    }

    // ② 文件存在。
    if !path.is_file() {
        health.push(ModelFailure::Missing {
            model_id: spec.id.to_string(),
            searched: vec![path.to_string_lossy().into_owned()],
        });
        return health;
    }

    // ③ 体积 + 格式头。
    if let Some(failure) = check_size_and_header(spec, path) {
        health.push(failure);
        return health;
    }

    // ④ 契约（输入输出）。可选：热路径推迟到 load。
    if include_contract {
        match crate::ai_engine::inference::inspect_contract(path, spec.id) {
            Ok(snapshot) => {
                if let Err(error) = crate::ai_engine::inference::check_contract(spec, &snapshot) {
                    health.push(failure_from_inference(spec, error));
                    return health;
                }
            }
            Err(error) => {
                health.push(failure_from_inference(spec, error));
                return health;
            }
        }
    }

    // ⑤ SHA256（流式，不把权重整体读进内存）。
    match sha256_file(path) {
        Ok(actual) => {
            if !actual.eq_ignore_ascii_case(spec.sha256) {
                health.push(ModelFailure::Corrupt {
                    model_id: spec.id.to_string(),
                    detail: "SHA256 与登记值不符".into(),
                    expected_sha: spec.sha256.to_string(),
                    actual_sha: Some(actual),
                });
                return health;
            }
        }
        Err(error) => {
            health.push(ModelFailure::Corrupt {
                model_id: spec.id.to_string(),
                detail: format!("无法计算 SHA256：{error}"),
                expected_sha: spec.sha256.to_string(),
                actual_sha: None,
            });
            return health;
        }
    }

    health.resolved_path = Some(path.to_path_buf());
    health
}

/// 浅检查：只做 ① 许可 → ② 存在 → ③ 体积/格式头，**不读全文件、不算哈希**。
///
/// 供同步状态命令（`face_model_status` / `skin_model_status`）使用：
/// 这类命令会在面板挂载时调用，绝对不能出现整文件哈希（历史风险 R1）。
pub fn check_shallow(spec: &ModelSpec, path: &Path) -> ModelHealth {
    let mut health = ModelHealth::new();
    if let Err(failure) = super::license::gate(spec.origin, &spec.license, spec.id) {
        health.push(failure);
        if !path.is_file() {
            health.push(ModelFailure::Missing {
                model_id: spec.id.to_string(),
                searched: vec![path.to_string_lossy().into_owned()],
            });
        }
        return health;
    }
    if !path.is_file() {
        health.push(ModelFailure::Missing {
            model_id: spec.id.to_string(),
            searched: vec![path.to_string_lossy().into_owned()],
        });
        return health;
    }
    if let Some(failure) = check_size_and_header(spec, path) {
        health.push(failure);
        return health;
    }
    health.resolved_path = Some(path.to_path_buf());
    health
}

/// 按候选目录定位并校验。
pub fn check(spec: &ModelSpec, base_dirs: &[PathBuf], include_contract: bool) -> ModelHealth {
    let candidates = locate::candidates(base_dirs, spec.dir, spec.file_name);
    match locate::locate(base_dirs, spec.dir, spec.file_name) {
        Some(path) => check_at(spec, &path, include_contract),
        None => {
            // 许可是第一顺位：即使文件缺失，也要先暴露许可问题。
            let mut health = ModelHealth::new();
            if let Err(failure) = super::license::gate(spec.origin, &spec.license, spec.id) {
                health.push(failure);
            }
            health.push(ModelFailure::Missing {
                model_id: spec.id.to_string(),
                searched: locate::describe(&candidates),
            });
            health
        }
    }
}

fn failure_from_inference(
    spec: &ModelSpec,
    error: crate::ai_engine::inference::InferenceError,
) -> ModelFailure {
    use crate::ai_engine::inference::InferenceError;
    match error {
        InferenceError::ModelVersionMismatch {
            expected, actual, ..
        } => ModelFailure::VersionMismatch {
            model_id: spec.id.to_string(),
            expected,
            actual,
        },
        InferenceError::ContractMismatch { detail, .. } => ModelFailure::VersionMismatch {
            model_id: spec.id.to_string(),
            expected: format!("{} 个 head / {}", spec.outputs.len(), spec.input.format_human()),
            actual: detail,
        },
        other => ModelFailure::LoadFailed {
            model_id: spec.id.to_string(),
            detail: other.to_string(),
        },
    }
}

fn check_size_and_header(spec: &ModelSpec, path: &Path) -> Option<ModelFailure> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Some(ModelFailure::Corrupt {
                model_id: spec.id.to_string(),
                detail: format!("无法读取文件信息：{error}"),
                expected_sha: spec.sha256.to_string(),
                actual_sha: None,
            })
        }
    };
    if metadata.len() != spec.size_bytes {
        return Some(ModelFailure::Corrupt {
            model_id: spec.id.to_string(),
            detail: format!(
                "文件体积 {} 与登记值 {} 不符",
                metadata.len(),
                spec.size_bytes
            ),
            expected_sha: spec.sha256.to_string(),
            actual_sha: None,
        });
    }
    match header_is_onnx(path) {
        Ok(true) => None,
        Ok(false) => Some(ModelFailure::Corrupt {
            model_id: spec.id.to_string(),
            detail: "文件头不是 ONNX（疑似压缩包 / 可执行文件 / LFS 指针）".into(),
            expected_sha: spec.sha256.to_string(),
            actual_sha: None,
        }),
        Err(error) => Some(ModelFailure::Corrupt {
            model_id: spec.id.to_string(),
            detail: format!("无法读取文件头：{error}"),
            expected_sha: spec.sha256.to_string(),
            actual_sha: None,
        }),
    }
}

/// 只读文件头 8 字节判断是不是（或像不像）ONNX。
///
/// ONNX 是 protobuf，没有固定魔数，因此这里做的是**排除法**：
/// 命中已知伪装格式就拒；文件太小（读不到 8 字节）也拒。
pub fn header_is_onnx(path: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 8];
    let read = file.read(&mut header)?;
    let header = &header[..read];
    if header.len() < 8 {
        return Ok(false);
    }
    if header.starts_with(b"PK\x03\x04") {
        return Ok(false);
    }
    if header.starts_with(b"version https://git-lfs") {
        return Ok(false);
    }
    if header.len() >= 4
        && matches!(
            &header[..4],
            [0xcf, 0xfa, 0xed, 0xfe] | [0xce, 0xfa, 0xed, 0xfe]
        )
    {
        return Ok(false);
    }
    for (marker, _) in FORBIDDEN_MARKERS {
        if header.starts_with(marker) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::model_registry::builtin;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lumi-verify-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn real_model_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(builtin::YUNET.dir)
            .join(builtin::YUNET.file_name)
    }

    #[test]
    fn real_yunet_passes_the_full_chain() {
        let health = check_at(&builtin::YUNET, &real_model_path(), true);
        assert!(health.blocking.is_none(), "findings={:?}", health.findings);
        assert!(health.findings.is_empty());
        assert_eq!(health.state().as_str(), "verified");
    }

    #[test]
    fn missing_file_is_reported_with_searched_paths() {
        let health = check_at(
            &builtin::YUNET,
            Path::new("/nonexistent/face_detection_yunet_2023mar.onnx"),
            true,
        );
        assert_eq!(health.blocking.unwrap().kind(), "missing");
        assert_eq!(health.findings.len(), 1);
        assert!(!health.findings[0].searched().unwrap().is_empty());
    }

    #[test]
    fn sha_mismatch_is_corrupt_not_missing() {
        let dir = temp_dir("sha");
        let path = dir.join(builtin::YUNET.file_name);
        // 体积对齐、头合法但内容不同 → 必须落在 SHA 校验上。
        let mut bytes = vec![0x08u8; builtin::YUNET.size_bytes as usize];
        bytes[0] = 0x08;
        std::fs::write(&path, &bytes).unwrap();
        let health = check_at(&builtin::YUNET, &path, false);
        let failure = health.blocking.unwrap();
        assert_eq!(failure.kind(), "corrupt");
        assert!(failure.detail().unwrap().contains("SHA256"));
        assert_eq!(failure.expected(), Some(builtin::YUNET.sha256));
        assert!(failure.actual().is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wrong_size_is_corrupt_before_hashing() {
        let dir = temp_dir("size");
        let path = dir.join(builtin::YUNET.file_name);
        std::fs::write(&path, b"short").unwrap();
        let health = check_at(&builtin::YUNET, &path, false);
        let failure = health.blocking.unwrap();
        assert_eq!(failure.kind(), "corrupt");
        assert!(failure.detail().unwrap().contains("体积"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zip_disguised_as_onnx_is_rejected_by_header() {
        let dir = temp_dir("zip");
        let path = dir.join("evil.onnx");
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.resize(64, 0);
        std::fs::write(&path, &bytes).unwrap();
        assert!(!header_is_onnx(&path).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shallow_check_skips_hashing_but_still_rejects_a_wrong_size() {
        let dir = temp_dir("shallow");
        let path = dir.join(builtin::YUNET.file_name);
        std::fs::write(&path, b"short").unwrap();
        let health = check_shallow(&builtin::YUNET, &path);
        assert_eq!(health.blocking.unwrap().kind(), "corrupt");
        // 体积不对时连 SHA 都不该尝试，因此 actual_sha 必须为空。
        assert!(health.findings[0].actual().is_none());
        std::fs::remove_dir_all(&dir).ok();

        // 真实权重：浅检查通过（不读全文件）。
        let health = check_shallow(&builtin::YUNET, &real_model_path());
        assert!(health.blocking.is_none());
        assert!(health.resolved_path.is_some());
    }

    #[test]
    fn check_without_candidates_reports_every_searched_path() {
        let health = check(&builtin::YUNET, &[PathBuf::from("/nope")], false);
        let failure = health.blocking.unwrap();
        assert_eq!(failure.kind(), "missing");
        assert_eq!(failure.searched().unwrap().len(), 1);
    }
}
