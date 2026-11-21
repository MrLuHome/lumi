//! 皮肤分割模型注册与许可证闸门。
//!
//! 原则：仓库代码许可不能推断权重可商用。只有同时满足
//! 1) 人工登记为允许商用；2) 文件 SHA256 与登记值一致 的权重才会被加载。
//!
//! 当前没有任何通过审查的皮肤分割权重，因此引擎明确回退到几何蒙版，
//! 并在状态下如实说明原因，不打包、不冒充。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ai::model::sha256_hex;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy)]
pub struct RegisteredSkinModel {
    pub id: &'static str,
    pub file: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub commercial_use: bool,
    pub sha256: &'static str,
    pub input: &'static str,
    pub outputs: &'static str,
    pub source: &'static str,
}

/// 已人工审查、允许商用的权重清单。当前为空：
/// BiSeNet/CelebAMask-HQ 系权重受数据集非商用条款限制，InsightFace 多数模型仅限研究，
/// 均不满足打包条件。
pub const REGISTERED_SKIN_MODELS: &[RegisteredSkinModel] = &[];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinModelStatus {
    /// 是否有可用的、许可通过的模型权重。
    pub available: bool,
    /// 实际生效的蒙版方法。
    pub method: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub commercial_use: bool,
    pub sha256: String,
    pub input: String,
    pub outputs: String,
    pub path: Option<String>,
    pub reason: String,
    pub registered_count: usize,
}

pub fn status(resource_dir: Option<&Path>) -> SkinModelStatus {
    for spec in REGISTERED_SKIN_MODELS {
        if !spec.commercial_use || !is_commercial_license(spec.license) {
            continue;
        }
        if let Some(path) = locate(resource_dir, spec.file) {
            if verify(&path, spec).is_ok() {
                return SkinModelStatus {
                    available: true,
                    method: format!("onnx:{}", spec.id),
                    name: spec.name.into(),
                    version: spec.version.into(),
                    license: spec.license.into(),
                    commercial_use: true,
                    sha256: spec.sha256.into(),
                    input: spec.input.into(),
                    outputs: spec.outputs.into(),
                    path: Some(path.to_string_lossy().into_owned()),
                    reason: "已加载通过许可审查的皮肤分割权重".into(),
                    registered_count: REGISTERED_SKIN_MODELS.len(),
                };
            }
        }
    }

    SkinModelStatus {
        available: false,
        method: "landmark-geometry".into(),
        name: "几何皮肤区域（关键点 + 肤色似然）".into(),
        version: "3.0".into(),
        license: "本项目实现".into(),
        commercial_use: true,
        sha256: String::new(),
        input: "5 关键点 + 图像像素".into(),
        outputs: "额头/双颊/鼻部/下巴/颈部皮肤 alpha".into(),
        path: None,
        reason: "未登记可商用皮肤解析权重；BiSeNet/CelebAMask-HQ 系受非商用数据集限制，InsightFace 多数模型仅限研究。已回退到几何蒙版，不会用未授权权重冒充 AI 分割。".into(),
        registered_count: REGISTERED_SKIN_MODELS.len(),
    }
}

pub fn locate(resource_dir: Option<&Path>, file: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("resources/models/skin").join(file));
        candidates.push(dir.join("models/skin").join(file));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/models/skin")
            .join(file),
    );
    candidates.into_iter().find(|path| path.is_file())
}

/// 加载前的强制校验：许可证必须允许商用，SHA256 必须匹配。
pub fn verify(path: &Path, spec: &RegisteredSkinModel) -> AppResult<()> {
    if !spec.commercial_use {
        return Err(AppError::Unsupported(format!(
            "皮肤分割权重 {} 未获商用许可，拒绝加载",
            spec.id
        )));
    }
    if !is_commercial_license(spec.license) {
        return Err(AppError::Unsupported(format!(
            "皮肤分割权重 {} 的许可证 {} 不在允许清单内，拒绝加载",
            spec.id, spec.license
        )));
    }
    let bytes = std::fs::read(path)?;
    let digest = sha256_hex(&bytes);
    if !digest.eq_ignore_ascii_case(spec.sha256) {
        return Err(AppError::Unsupported(format!(
            "皮肤分割权重 SHA256 校验失败（期望 {}，实际 {digest}）",
            spec.sha256
        )));
    }
    Ok(())
}

pub fn is_commercial_license(license: &str) -> bool {
    matches!(
        license.trim().to_ascii_lowercase().as_str(),
        "mit" | "apache-2.0" | "apache 2.0" | "bsd-2-clause" | "bsd-3-clause"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(commercial: bool, license: &'static str, sha: &'static str) -> RegisteredSkinModel {
        RegisteredSkinModel {
            id: "test",
            file: "test.onnx",
            name: "test",
            version: "1",
            license,
            commercial_use: commercial,
            sha256: sha,
            input: "",
            outputs: "",
            source: "",
        }
    }

    #[test]
    fn rejects_non_commercial_weights() {
        let path = std::env::temp_dir().join("lumi_skin_gate_test.bin");
        std::fs::write(&path, b"dummy").unwrap();
        let error = verify(&path, &spec(false, "MIT", "")).unwrap_err();
        assert!(error.to_string().contains("未获商用许可"));
    }

    #[test]
    fn rejects_research_only_license_even_if_flagged_commercial() {
        let path = std::env::temp_dir().join("lumi_skin_gate_test.bin");
        std::fs::write(&path, b"dummy").unwrap();
        let error = verify(&path, &spec(true, "CC-BY-NC-4.0", "")).unwrap_err();
        assert!(error.to_string().contains("不在允许清单"));
    }

    #[test]
    fn rejects_sha_mismatch_and_accepts_matching_hash() {
        let path = std::env::temp_dir().join("lumi_skin_gate_test.bin");
        std::fs::write(&path, b"dummy").unwrap();
        let digest = sha256_hex(b"dummy");
        let error = verify(&path, &spec(true, "MIT", "deadbeef")).unwrap_err();
        assert!(error.to_string().contains("SHA256"));
        verify(
            &path,
            &spec(true, "MIT", Box::leak(digest.into_boxed_str())),
        )
        .unwrap();
    }

    #[test]
    fn status_falls_back_to_geometry_without_weights() {
        let status = status(None);
        assert!(!status.available);
        assert_eq!(status.method, "landmark-geometry");
        assert!(status.reason.contains("未登记可商用"));
    }
}
