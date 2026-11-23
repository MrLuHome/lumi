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

/// 查询皮肤解析能力。
///
/// R11 起**转发** `ai_engine::model_registry`：登记表的权威来源只有一处。
/// 面板挂载时会调用本函数，因此这里**只做浅检查**（许可 → 存在 → 体积/头），
/// 绝不算 SHA256；SHA256 与契约只在真正加载模型时于后台线程校验（历史风险 R1）。
/// 浅检查通过但推理层记录过失败时，也如实回退并说明原因。
pub fn status(resource_dir: Option<&Path>) -> SkinModelStatus {
    status_from_specs(
        resource_dir,
        crate::ai_engine::model_registry::builtin::SKIN_MODELS,
    )
}

/// 与 [`status`] 相同的判定，但允许注入登记表，便于失败注入测试。
fn status_from_specs(
    resource_dir: Option<&Path>,
    models: &[&crate::ai_engine::model_registry::spec::ModelSpec],
) -> SkinModelStatus {
    use crate::ai_engine::model_registry::state::{ModelFailure, ModelHealth};
    use crate::ai_engine::model_registry::{license, locate, verify};

    let base_dirs = skin_base_dirs(resource_dir);
    for spec in models {
        let located = locate::locate(&base_dirs, spec.dir, spec.file_name);
        let health = match &located {
            Some(path) => verify::check_shallow(spec, path),
            None => {
                let mut health = ModelHealth::new();
                // 许可是第一顺位：即使文件缺失，也要先暴露许可问题。
                if let Err(failure) = license::gate(spec.origin, &spec.license, spec.id) {
                    health.push(failure);
                }
                health.push(ModelFailure::Missing {
                    model_id: spec.id.to_string(),
                    searched: locate::describe(&locate::candidates(
                        &base_dirs, spec.dir, spec.file_name,
                    )),
                });
                health
            }
        };
        if health.blocking.is_none() {
            // 浅检查通过不代表推理可用：若加载/推理曾失败，如实回退并披露原因。
            if let Some(failure) = crate::ai_engine::skin_segmentation::onnx::last_failure() {
                return fallback(format!(
                    "皮肤解析权重加载失败：{failure}；已回退到几何蒙版，不会用未授权或不可用的权重冒充 AI 分割。"
                ));
            }
            let unverified = spec.license.unverified_fields();
            let reason = if unverified.contains(&"trainingData") {
                "已加载 Selfie Multiclass 皮肤解析（训练数据许可未披露，待法务复核）".into()
            } else {
                "已加载通过许可审查的皮肤分割权重".into()
            };
            return SkinModelStatus {
                available: true,
                method: format!("onnx:{}", spec.id),
                name: spec.display_name.into(),
                version: spec.version.into(),
                license: spec.license.spdx.unwrap_or("未核实").into(),
                commercial_use: true,
                sha256: spec.sha256.into(),
                input: spec.input.format_human(),
                outputs: spec.output_names().join("/"),
                path: health
                    .resolved_path
                    .map(|path| path.to_string_lossy().into_owned()),
                reason,
                registered_count: models.len(),
            };
        }
        let detail = health
            .blocking
            .as_ref()
            .map(|failure| {
                failure
                    .detail()
                    .map(str::to_string)
                    .unwrap_or_else(|| failure.message())
            })
            .unwrap_or_default();
        return fallback(format!(
            "皮肤解析权重未通过准入检查（{detail}）；已回退到几何蒙版（关键点 + 肤色似然），未使用 AI 分割。"
        ));
    }
    fallback("未登记可商用皮肤解析权重；已回退到几何蒙版（关键点 + 肤色似然），不会用未授权权重冒充 AI 分割。".into())
}

fn fallback(reason: String) -> SkinModelStatus {
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
        reason,
        registered_count: crate::ai_engine::model_registry::builtin::SKIN_MODELS.len(),
    }
}

/// 皮肤权重的候选基础目录（与 `commands` 层的候选顺序保持一致）。
fn skin_base_dirs(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = resource_dir {
        dirs.push(dir.join("resources"));
        dirs.push(dir.to_path_buf());
    }
    dirs.extend(crate::ai_engine::model_registry::locate::dev_base_dirs());
    dirs
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
        // 注入空登记表：没有权重时不得声称 AI 分割。
        let status = status_from_specs(None, &[]);
        assert!(!status.available);
        assert_eq!(status.method, "landmark-geometry");
        assert!(status.reason.contains("未登记可商用"));
    }

    #[test]
    fn status_reports_registered_model_and_training_data_gap() {
        let status = status(None);
        assert!(status.available, "随包权重已登记且浅检查应通过");
        assert_eq!(
            status.method,
            "onnx:skin-parse-mediapipe-selfie-multiclass-256"
        );
        assert_eq!(status.license, "Apache-2.0");
        assert!(
            status.reason.contains("训练数据许可未披露"),
            "训练数据 Unverified 必须在状态里如实披露：{}",
            status.reason
        );
        assert_eq!(status.registered_count, 1);
    }

    #[test]
    fn status_falls_back_when_license_is_not_cleared() {
        use crate::ai_engine::model_registry::license::{LicenseRecord, Verdict3};
        use crate::ai_engine::model_registry::spec::ModelSpec;
        let bad = ModelSpec {
            license: LicenseRecord {
                weights: Verdict3::NotAllowed,
                ..crate::ai_engine::model_registry::builtin::SELFIE_SKIN.license
            },
            ..crate::ai_engine::model_registry::builtin::SELFIE_SKIN.clone()
        };
        let status = status_from_specs(None, &[&bad]);
        assert!(!status.available);
        assert_eq!(status.method, "landmark-geometry");
        assert!(
            status.reason.contains("许可") || status.reason.contains("不允许"),
            "{}",
            status.reason
        );
        assert!(
            status.reason.contains("几何蒙版"),
            "回退路径必须诚实说明用的是几何方案：{}",
            status.reason
        );
    }
}
