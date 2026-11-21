//! Model Hub 本地存储。
//!
//! 目录结构：`<app_data>/face-models/<id>/{manifest.json, model.onnx, validation.json}`。
//! 写入使用临时目录 + 原子重命名，删除先改名到 .trash 再清理，避免半成品目录。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::{ModelManifest, ValidationSummary};
use super::security::{safe_join, validate_model_id, MAX_MANIFEST_BYTES};

const MANIFEST_FILE: &str = "manifest.json";
const MODEL_FILE: &str = "model.onnx";
const VALIDATION_FILE: &str = "validation.json";
const MAX_INSTALLED: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub manifest: ModelManifest,
    pub directory: String,
    pub model_path: String,
    pub present: bool,
    pub sha256_matches: bool,
    pub validation_ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelHubStore {
    root: PathBuf,
}

impl ModelHubStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root)
            .map_err(|error| format!("无法创建模型目录 {}：{error}", self.root.display()))
    }

    pub fn directory(&self, id: &str) -> Result<PathBuf, String> {
        validate_model_id(id)?;
        safe_join(&self.root, id)
    }

    pub fn model_path(&self, id: &str) -> Result<PathBuf, String> {
        Ok(self.directory(id)?.join(MODEL_FILE))
    }

    pub fn list(&self) -> Vec<InstalledModel> {
        let mut entries = Vec::new();
        let Ok(read_dir) = std::fs::read_dir(&self.root) else {
            return entries;
        };
        for entry in read_dir.flatten().take(MAX_INSTALLED * 4) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            match self.load(&name) {
                Ok(model) => entries.push(model),
                Err(_) => continue,
            }
            if entries.len() >= MAX_INSTALLED {
                break;
            }
        }
        entries.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        entries
    }

    pub fn load(&self, id: &str) -> Result<InstalledModel, String> {
        let directory = self.directory(id)?;
        let manifest_path = directory.join(MANIFEST_FILE);
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| format!("读取 manifest 失败：{error}"))?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("manifest 超过 1 MB".into());
        }
        let manifest: ModelManifest = serde_json::from_slice(&bytes)
            .map_err(|error| format!("manifest 解析失败：{error}"))?;
        if manifest.id != id {
            return Err(format!(
                "manifest ID `{}` 与目录 `{id}` 不一致",
                manifest.id
            ));
        }
        let model_path = directory.join(&manifest.file_name);
        let present = model_path.is_file();
        let current_sha = if present {
            super::security::sha256_file(&model_path).ok()
        } else {
            None
        };
        let sha256_matches = current_sha
            .as_deref()
            .map(|sha| sha.eq_ignore_ascii_case(&manifest.sha256))
            .unwrap_or(false);
        let validation_ok = manifest
            .validation
            .as_ref()
            .map(|summary| summary.ok)
            .unwrap_or(false);
        let error = if !present {
            Some("模型文件缺失".into())
        } else if !sha256_matches {
            Some("模型 SHA256 与 manifest 不一致".into())
        } else if !validation_ok {
            Some("模型尚未通过验证".into())
        } else {
            None
        };
        Ok(InstalledModel {
            manifest,
            directory: directory.to_string_lossy().into_owned(),
            model_path: model_path.to_string_lossy().into_owned(),
            present,
            sha256_matches,
            validation_ok,
            error,
        })
    }

    /// 原子安装：先写临时目录，再 rename。已存在同 ID 时拒绝覆盖。
    pub fn install(&self, manifest: &ModelManifest, model_bytes: &[u8]) -> Result<PathBuf, String> {
        self.ensure()?;
        validate_model_id(&manifest.id)?;
        let target = self.directory(&manifest.id)?;
        if target.exists() {
            return Err(format!("模型 ID `{}` 已存在，请先移除", manifest.id));
        }
        if self.list().len() >= MAX_INSTALLED {
            return Err(format!("最多只能安装 {MAX_INSTALLED} 个用户模型"));
        }
        let temp = self
            .root
            .join(format!(".tmp-{}-{}", manifest.id, std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).map_err(|error| format!("创建临时目录失败：{error}"))?;
        let write = || -> Result<(), String> {
            std::fs::write(temp.join(&manifest.file_name), model_bytes)
                .map_err(|error| format!("写入模型失败：{error}"))?;
            let json = serde_json::to_vec_pretty(manifest)
                .map_err(|error| format!("manifest 序列化失败：{error}"))?;
            std::fs::write(temp.join(MANIFEST_FILE), json)
                .map_err(|error| format!("写入 manifest 失败：{error}"))?;
            Ok(())
        };
        if let Err(error) = write() {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(error);
        }
        std::fs::rename(&temp, &target).map_err(|error| {
            let _ = std::fs::remove_dir_all(&temp);
            format!("安装模型失败：{error}")
        })?;
        Ok(target)
    }

    pub fn write_validation(&self, id: &str, summary: &ValidationSummary) -> Result<(), String> {
        let directory = self.directory(id)?;
        let json = serde_json::to_vec_pretty(summary)
            .map_err(|error| format!("验证结果序列化失败：{error}"))?;
        std::fs::write(directory.join(VALIDATION_FILE), json)
            .map_err(|error| format!("写入验证结果失败：{error}"))?;
        let mut manifest = self.load(id)?.manifest;
        manifest.validation = Some(summary.clone());
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("manifest 序列化失败：{error}"))?;
        std::fs::write(directory.join(MANIFEST_FILE), json)
            .map_err(|error| format!("写入 manifest 失败：{error}"))?;
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let directory = self.directory(id)?;
        if !directory.exists() {
            return Err(format!("模型 `{id}` 不存在"));
        }
        let trash = self
            .root
            .join(format!(".trash-{}-{}", id, std::process::id()));
        let _ = std::fs::remove_dir_all(&trash);
        std::fs::rename(&directory, &trash).map_err(|error| format!("移除模型失败：{error}"))?;
        std::fs::remove_dir_all(&trash).map_err(|error| format!("清理模型目录失败：{error}"))?;
        Ok(())
    }

    pub fn find_by_sha(&self, sha256: &str) -> Option<InstalledModel> {
        self.list()
            .into_iter()
            .find(|model| model.manifest.sha256.eq_ignore_ascii_case(sha256))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::model_hub::manifest::{
        InputSpec, LicenseDeclaration, ModelTask, Normalization, OutputCoordinate, OutputLayout,
        OutputSpec, TensorLayout,
    };

    fn temp_store(name: &str) -> (ModelHubStore, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("lumi-model-hub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        (ModelHubStore::new(root.clone()), root)
    }

    fn manifest(id: &str) -> ModelManifest {
        ModelManifest {
            schema_version: 1,
            id: id.into(),
            name: "Test Landmark".into(),
            version: "1.0".into(),
            author: "tester".into(),
            description: "synthetic".into(),
            task: ModelTask::FaceLandmark,
            topology_id: "face-68".into(),
            input: InputSpec {
                width: 64,
                height: 64,
                layout: TensorLayout::Nchw,
                normalization: Normalization::ZeroToOne,
                crop_margin: 0.15,
            },
            output: OutputSpec {
                layout: OutputLayout::N2,
                coordinate: OutputCoordinate::NormalizedCrop,
            },
            license: LicenseDeclaration::default(),
            sha256: crate::face_intelligence::model_hub::security::sha256_bytes(b"onnx"),
            file_name: "model.onnx".into(),
            source_url: None,
            source_path: "/tmp/model.onnx".into(),
            imported_at: "2026-11-21T00:00:00Z".into(),
            validation: Some(ValidationSummary {
                ok: true,
                sha256: crate::face_intelligence::model_hub::security::sha256_bytes(b"onnx"),
                file_size: 4,
                input_shape: vec![Some(1), Some(3), Some(64), Some(64)],
                output_shape: vec![Some(1), Some(68), Some(2)],
                operator_count: 1,
                elapsed_ms: 1,
                warnings: Vec::new(),
                errors: Vec::new(),
            }),
        }
    }

    #[test]
    fn install_list_and_remove() {
        let (store, root) = temp_store("install");
        store.install(&manifest("m1"), b"onnx").unwrap();
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].present && list[0].sha256_matches && list[0].validation_ok);
        assert!(store.install(&manifest("m1"), b"onnx").is_err());
        store.remove("m1").unwrap();
        assert!(store.list().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_file_is_reported_not_panicking() {
        let (store, root) = temp_store("missing");
        store.install(&manifest("m1"), b"onnx").unwrap();
        std::fs::remove_file(store.model_path("m1").unwrap()).unwrap();
        let model = store.load("m1").unwrap();
        assert!(!model.present);
        assert!(model.error.unwrap().contains("缺失"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tampered_file_is_detected() {
        let (store, root) = temp_store("tamper");
        store.install(&manifest("m1"), b"onnx").unwrap();
        std::fs::write(store.model_path("m1").unwrap(), b"evil").unwrap();
        let model = store.load("m1").unwrap();
        assert!(model.present && !model.sha256_matches);
        assert!(model.error.unwrap().contains("SHA256"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn path_traversal_id_is_rejected() {
        let (store, root) = temp_store("traversal");
        assert!(store.install(&manifest("../evil"), b"onnx").is_err());
        assert!(store.remove("../evil").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
