//! Model Hub：用户自备 ONNX 关键点模型的导入、验证、存储与生命周期。
//!
//! 不打包任何权重；用户模型只存在本机应用数据目录。导入必须通过：
//! 路径/体积/伪装格式检查 → ONNX 结构与算子验证 → 隔离冒烟推理 → SHA256 记录。

pub mod inference;
pub mod manifest;
pub mod probe;
pub mod security;
pub mod storage;
pub mod validator;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use inference::LandmarkModel;
pub use manifest::{
    InputSpec, LicenseDeclaration, ModelManifest, ModelTask, Normalization, OutputCoordinate,
    OutputLayout, OutputSpec, TensorLayout, ValidationSummary,
};
pub use probe::{ProbeReport, SmokeResult};
pub use storage::{InstalledModel, ModelHubStore};
pub use validator::OnnxProbe;

use capability::LicenseState;
use security::{validate_import_path, validate_model_id, MAX_MODEL_BYTES};

use crate::face_intelligence::capability;

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub id: Option<String>,
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
    pub normalization: Normalization,
    pub output_coordinate: OutputCoordinate,
    pub crop_margin: f32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            id: None,
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
            normalization: Normalization::ZeroToOne,
            output_coordinate: OutputCoordinate::NormalizedCrop,
            crop_margin: 0.15,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelHub {
    store: ModelHubStore,
    executable: Option<PathBuf>,
    timeout: Duration,
}

impl ModelHub {
    pub fn new(root: PathBuf, executable: Option<PathBuf>) -> Self {
        Self {
            store: ModelHubStore::new(root),
            executable,
            timeout: Duration::from_millis(probe::DEFAULT_TIMEOUT_MS),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn store(&self) -> &ModelHubStore {
        &self.store
    }

    pub fn list(&self) -> Vec<InstalledModel> {
        self.store.list()
    }

    pub fn get(&self, id: &str) -> Result<InstalledModel, String> {
        self.store.load(id)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        self.store.remove(id)
    }

    fn run_probe(&self, path: &Path) -> Result<ProbeReport, String> {
        match &self.executable {
            Some(executable) => probe::probe_isolated(executable, path, self.timeout),
            None => Ok(probe::probe_sync(path)),
        }
    }

    /// 导入模型。所有失败都返回具体原因，不静默降级。
    pub fn import(&self, source: &Path, options: &ImportOptions) -> Result<InstalledModel, String> {
        let info = validate_import_path(source)?;
        if let Some(existing) = self.store.find_by_sha(&info.sha256) {
            return Err(format!(
                "相同 SHA256 的模型已安装：{}（{}）",
                existing.manifest.name, existing.manifest.id
            ));
        }
        let report = self.run_probe(&info.path)?;
        if !report.probe.ok {
            return Err(format!(
                "模型结构校验失败：{}",
                report.probe.errors.join("；")
            ));
        }
        if !report.smoke.ok {
            return Err(format!(
                "模型冒烟推理失败：{}",
                report.smoke.error.unwrap_or_else(|| "未知原因".into())
            ));
        }
        let (Some(width), Some(height), Some(layout)) = (
            report.probe.input_width,
            report.probe.input_height,
            report.probe.output_layout,
        ) else {
            return Err("模型输入/输出契约不完整".into());
        };
        if report.smoke.output_elements == 0 {
            return Err("模型没有输出元素".into());
        }
        let id = match &options.id {
            Some(id) => {
                validate_model_id(id)?;
                id.clone()
            }
            None => {
                let base = slug(&options.name);
                let suffix = &info.sha256[..8];
                format!("{base}-{suffix}")
            }
        };
        let manifest = ModelManifest {
            schema_version: manifest::SCHEMA_VERSION,
            id: id.clone(),
            name: options.name.clone(),
            version: options.version.clone(),
            author: options.author.clone(),
            description: options.description.clone(),
            task: ModelTask::FaceLandmark,
            topology_id: "face-68".into(),
            input: InputSpec {
                width,
                height,
                layout: TensorLayout::Nchw,
                normalization: options.normalization,
                crop_margin: options.crop_margin.clamp(0.0, 0.5),
            },
            output: OutputSpec {
                layout,
                coordinate: options.output_coordinate,
            },
            license: LicenseDeclaration {
                name: options.license_name.clone(),
                url: options.license_url.clone(),
                commercial_use_claimed: options.license_commercial_claimed,
                redistribution_claimed: options.license_redistribution_claimed,
                note: options.license_note.clone(),
            },
            sha256: info.sha256.clone(),
            file_name: "model.onnx".into(),
            source_url: options.source_url.clone(),
            source_path: info.path.to_string_lossy().into_owned(),
            imported_at: now_iso8601(),
            validation: Some(validator::summary(&report.probe)),
        };
        let issues = manifest.validate_schema();
        if !issues.is_empty() {
            return Err(issues.join("；"));
        }
        let bytes = security::read_limited(&info.path, MAX_MODEL_BYTES)?;
        if bytes.len() as u64 != info.size {
            return Err("模型文件在导入过程中被修改，请重试".into());
        }
        if security::sha256_bytes(&bytes) != info.sha256 {
            return Err("模型文件在导入过程中发生变化，请重试".into());
        }
        self.store.install(&manifest, &bytes)?;
        self.store.load(&id)
    }

    /// 重新验证已安装模型；SHA256 不一致或推理失败时保留记录但标记不可用。
    pub fn revalidate(&self, id: &str) -> Result<InstalledModel, String> {
        let installed = self.store.load(id)?;
        let path = PathBuf::from(&installed.model_path);
        if !path.is_file() {
            return Err("模型文件缺失".into());
        }
        let report = self.run_probe(&path)?;
        if !report
            .probe
            .sha256
            .eq_ignore_ascii_case(&installed.manifest.sha256)
        {
            return Err("模型 SHA256 与导入记录不一致，已拒绝启用".into());
        }
        if !report.probe.ok || !report.smoke.ok {
            let summary = validator::summary(&report.probe);
            self.store.write_validation(id, &summary)?;
            return Err(format!(
                "模型验证失败：{}",
                report
                    .probe
                    .errors
                    .iter()
                    .cloned()
                    .chain(report.smoke.error.clone())
                    .collect::<Vec<_>>()
                    .join("；")
            ));
        }
        self.store
            .write_validation(id, &validator::summary(&report.probe))?;
        self.store.load(id)
    }

    pub fn license_state(_model: &InstalledModel) -> LicenseState {
        // 用户声明不等于核实。Model Hub 永远不把用户模型标成官方商用。
        LicenseState::UserUnverified
    }
}

fn slug(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    let trimmed = result.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "user-model".into()
    } else {
        trimmed.chars().take(40).collect()
    }
}

fn now_iso8601() -> String {
    // 避免新增时间库：用 UNIX 秒 + 固定格式说明；展示层只要求可排序。
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix:{seconds}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_path_safe() {
        assert_eq!(slug("My  Model!!"), "my-model");
        assert_eq!(slug("!!!"), "user-model");
        assert!(validate_model_id(&slug("../../evil")).is_ok());
    }

    #[test]
    fn options_default_is_conservative() {
        let options = ImportOptions::default();
        assert!(!options.license_commercial_claimed);
        assert!(!options.license_redistribution_claimed);
        assert_eq!(options.license_name, "UNKNOWN");
    }
}
