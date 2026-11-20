use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::fit_within;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::EditParams;
use crate::image_engine::pipeline;
use crate::image_engine::preset::{
    apply_preset, builtin_presets, PresetCategory, PresetParams, PresetSource, RetouchPreset,
    PRESET_SCHEMA_VERSION,
};
use crate::image_engine::preview::encode_thumb;
use crate::state::{FaceRepository, ImageCache};

const PRESET_EXTENSION: &str = "lumi-preset.json";

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PresetStoreFile {
    schema_version: u32,
    presets: Vec<RetouchPreset>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

/// 全局自定义预设库。只存用户预设，内置预设永远由代码提供，
/// 因此升级或删除内置项不会影响用户数据。
#[derive(Clone)]
pub struct PresetStore {
    inner: Arc<Mutex<Vec<RetouchPreset>>>,
    path: PathBuf,
    sequence: Arc<AtomicU64>,
}

impl Default for PresetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetStore {
    pub fn new() -> Self {
        Self::with_path(store_path())
    }

    fn with_path(path: PathBuf) -> Self {
        let presets = load_file(&path);
        Self {
            inner: Arc::new(Mutex::new(presets)),
            path,
            sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    fn custom(&self) -> Vec<RetouchPreset> {
        self.inner
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn persist(&self, presets: &[RetouchPreset]) -> AppResult<()> {
        let file = PresetStoreFile {
            schema_version: PRESET_SCHEMA_VERSION,
            presets: presets.to_vec(),
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = self
            .path
            .with_extension(format!("json.tmp-{}", now_ms()));
        let data = serde_json::to_vec_pretty(&file)
            .map_err(|error| AppError::Task(error.to_string()))?;
        std::fs::write(&temp, data)?;
        if let Err(error) = std::fs::rename(&temp, &self.path) {
            std::fs::remove_file(&temp).ok();
            return Err(error.into());
        }
        Ok(())
    }

    fn with_custom(&self, apply: impl FnOnce(&mut Vec<RetouchPreset>) -> AppResult<()>) -> AppResult<Vec<RetouchPreset>> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AppError::Task("预设库锁失败".into()))?;
        apply(&mut guard)?;
        let snapshot = guard.clone();
        drop(guard);
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    pub fn find(&self, preset_id: &str) -> Option<RetouchPreset> {
        builtin_presets()
            .into_iter()
            .chain(self.custom())
            .find(|preset| preset.id == preset_id)
    }

    fn new_id(&self) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("custom-{}-{sequence}", now_ms())
    }

    /// 保存或覆盖自定义预设。内置预设 ID 永远不允许覆盖。
    fn save_custom(
        &self,
        preset_id: Option<&str>,
        name: &str,
        description: &str,
        params: PresetParams,
    ) -> AppResult<RetouchPreset> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::InvalidInput("预设名称不能为空".into()));
        }
        if params.is_empty() {
            return Err(AppError::InvalidInput("预设参数为空，无法保存".into()));
        }
        let mut saved = None;
        self.with_custom(|list| {
            if let Some(id) = preset_id {
                if builtin_presets().iter().any(|preset| preset.id == id) {
                    return Err(AppError::InvalidInput(
                        "内置预设不能覆盖，请另存为新预设".into(),
                    ));
                }
                if let Some(existing) = list.iter_mut().find(|preset| preset.id == id) {
                    existing.name = name.clone();
                    existing.description = description.to_string();
                    existing.params = params.clone();
                    existing.version = "1.0".into();
                    saved = Some(existing.clone());
                    return Ok(());
                }
            }
            let preset = RetouchPreset {
                id: self.new_id(),
                name: name.clone(),
                category: PresetCategory::Custom,
                description: description.to_string(),
                version: "1.0".into(),
                source: PresetSource::Custom,
                params: params.clone(),
                suitable_for: "自定义预设".into(),
                avoid: String::new(),
                preview: None,
            };
            list.push(preset.clone());
            saved = Some(preset);
            Ok(())
        })?;
        Ok(saved.expect("保存后必然有结果"))
    }

    fn delete_custom(&self, preset_id: &str) -> AppResult<()> {
        if builtin_presets().iter().any(|preset| preset.id == preset_id) {
            return Err(AppError::InvalidInput("内置预设不能删除".into()));
        }
        self.with_custom(|list| {
            let before = list.len();
            list.retain(|preset| preset.id != preset_id);
            if list.len() == before {
                return Err(AppError::InvalidInput("预设不存在".into()));
            }
            Ok(())
        })?;
        Ok(())
    }

    fn duplicate(&self, preset_id: &str, name: &str) -> AppResult<RetouchPreset> {
        let source = self
            .find(preset_id)
            .ok_or_else(|| AppError::InvalidInput("预设不存在".into()))?;
        let name = if name.trim().is_empty() {
            format!("{} 副本", source.name)
        } else {
            name.trim().to_string()
        };
        let mut saved = None;
        self.with_custom(|list| {
            let preset = RetouchPreset {
                id: self.new_id(),
                name: name.clone(),
                category: PresetCategory::Custom,
                description: source.description.clone(),
                version: "1.0".into(),
                source: PresetSource::Custom,
                params: source.params.clone(),
                suitable_for: source.suitable_for.clone(),
                avoid: source.avoid.clone(),
                preview: None,
            };
            list.push(preset.clone());
            saved = Some(preset);
            Ok(())
        })?;
        Ok(saved.expect("复制后必然有结果"))
    }

    fn rename(&self, preset_id: &str, name: &str) -> AppResult<RetouchPreset> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::InvalidInput("预设名称不能为空".into()));
        }
        let mut saved = None;
        self.with_custom(|list| {
            let preset = list
                .iter_mut()
                .find(|preset| preset.id == preset_id)
                .ok_or_else(|| AppError::InvalidInput("只有自定义预设可以重命名".into()))?;
            preset.name = name.clone();
            saved = Some(preset.clone());
            Ok(())
        })?;
        Ok(saved.expect("重命名后必然有结果"))
    }

    /// 导入预设文件：一律作为新的自定义预设写入，不覆盖已有项。
    fn import_json(&self, data: &[u8]) -> AppResult<RetouchPreset> {
        let imported = serde_json::from_slice::<RetouchPreset>(data)
            .map_err(|error| AppError::InvalidInput(format!("预设文件无效：{error}")))?;
        if imported.params.is_empty() {
            return Err(AppError::InvalidInput("预设文件没有可应用参数".into()));
        }
        let mut saved = None;
        self.with_custom(|list| {
            let preset = RetouchPreset {
                id: self.new_id(),
                name: if imported.name.trim().is_empty() {
                    "导入的预设".into()
                } else {
                    imported.name.clone()
                },
                category: PresetCategory::Custom,
                description: imported.description.clone(),
                version: "1.0".into(),
                source: PresetSource::Custom,
                params: imported.params.clone(),
                suitable_for: imported.suitable_for.clone(),
                avoid: imported.avoid.clone(),
                preview: None,
            };
            list.push(preset.clone());
            saved = Some(preset);
            Ok(())
        })?;
        Ok(saved.expect("导入后必然有结果"))
    }

    fn export_to(&self, preset_id: &str, destination: &Path) -> AppResult<PresetFileResult> {
        let preset = self
            .find(preset_id)
            .ok_or_else(|| AppError::InvalidInput("预设不存在".into()))?;
        let data = serde_json::to_vec_pretty(&preset)
            .map_err(|error| AppError::Task(error.to_string()))?;
        std::fs::write(destination, data)?;
        Ok(PresetFileResult {
            path: destination.to_string_lossy().into_owned(),
            preset,
        })
    }
}

fn store_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".lumi-retouch").join("presets.json")
}

fn load_file(path: &Path) -> Vec<RetouchPreset> {
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<PresetStoreFile>(&data) else {
        return Vec::new();
    };
    file.presets
        .into_iter()
        .filter(|preset| preset.source == PresetSource::Custom)
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetList {
    pub presets: Vec<RetouchPreset>,
    pub schema_version: u32,
}

#[tauri::command]
pub fn list_presets(store: State<'_, PresetStore>) -> PresetList {
    let mut presets = builtin_presets();
    presets.extend(store.custom());
    PresetList {
        presets,
        schema_version: PRESET_SCHEMA_VERSION,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedPreset {
    pub params: EditParams,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub fn apply_preset_to_photo(
    repo: State<'_, FaceRepository>,
    store: State<'_, PresetStore>,
    path: String,
    params: EditParams,
    preset_id: String,
    strength: f32,
) -> AppResult<AppliedPreset> {
    let preset = store
        .find(&preset_id)
        .ok_or_else(|| AppError::InvalidInput(format!("预设不存在：{preset_id}")))?;
    let faces: Vec<FaceFrame> = repo
        .get(Path::new(&path))
        .map(|entry| entry.faces)
        .unwrap_or_default();
    let (next, warnings) = apply_preset(&params, &preset, strength, &faces);
    Ok(AppliedPreset {
        params: next,
        warnings,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetPreview {
    pub preset_id: String,
    pub url: String,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetPreviews {
    pub previews: Vec<PresetPreview>,
    pub failed: Vec<String>,
}

/// 用当前照片的真实像素为每个预设生成缩略图。尺寸很小，但走的是正式渲染管线，
/// 不使用占位图，也不会把预设名当成图片索引。
#[tauri::command]
pub async fn preset_previews(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    store: State<'_, PresetStore>,
    path: String,
    params: EditParams,
    size: Option<u32>,
) -> AppResult<PresetPreviews> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let mut presets = builtin_presets();
    presets.extend(store.custom());
    let size = size.unwrap_or(240).clamp(96, 480);

    tauri::async_runtime::spawn_blocking(move || {
        let loaded = cache.load(Path::new(&path))?;
        let faces: Vec<FaceFrame> = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let working = fit_within(&loaded.thumb, size);
        let cancel = CancelToken::default();
        let mut previews = Vec::new();
        let mut failed = Vec::new();
        for preset in presets {
            // 从当前参数出发，用户能看到预设叠加在自己调整上的真实效果。
            let (next, preset_warnings) = apply_preset(&params, &preset, 100.0, &faces);
            match pipeline::render(&working, &next, &faces, &cancel)
                .and_then(|rendered| encode_thumb(&rendered, loaded.has_alpha))
            {
                Ok(url) => previews.push(PresetPreview {
                    preset_id: preset.id,
                    url,
                    warnings: preset_warnings,
                }),
                Err(error) => failed.push(format!("{}：{}", preset.name, error)),
            }
        }
        Ok(PresetPreviews { previews, failed })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SinglePresetPreview {
    pub url: String,
    pub warnings: Vec<String>,
}

/// 单张预设预览：强度实时变化时只在小尺寸上重渲染，不会触发全尺寸运算。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn preset_preview(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    store: State<'_, PresetStore>,
    path: String,
    params: EditParams,
    preset_id: String,
    strength: f32,
    size: Option<u32>,
) -> AppResult<SinglePresetPreview> {
    let preset = store
        .find(&preset_id)
        .ok_or_else(|| AppError::InvalidInput("预设不存在".into()))?;
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let size = size.unwrap_or(360).clamp(96, 640);
    tauri::async_runtime::spawn_blocking(move || {
        let loaded = cache.load(Path::new(&path))?;
        let faces: Vec<FaceFrame> = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let working = fit_within(&loaded.thumb, size);
        let (next, warnings) = apply_preset(&params, &preset, strength, &faces);
        let rendered = pipeline::render(&working, &next, &faces, &CancelToken::default())?;
        let url = encode_thumb(&rendered, loaded.has_alpha)?;
        Ok(SinglePresetPreview { url, warnings })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 从当前照片参数提取可保存的预设子集（不含任何人脸、蒙版、背景对象数据）。
#[tauri::command]
pub fn preset_params_from_edit(params: EditParams) -> PresetParams {
    PresetParams::from_edit_params(&params)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSaved {
    pub preset: RetouchPreset,
}

#[tauri::command]
pub fn save_custom_preset(
    store: State<'_, PresetStore>,
    preset_id: Option<String>,
    name: String,
    description: String,
    params: PresetParams,
) -> AppResult<PresetSaved> {
    let preset = store.save_custom(preset_id.as_deref(), &name, &description, params)?;
    Ok(PresetSaved { preset })
}

#[tauri::command]
pub fn delete_custom_preset(store: State<'_, PresetStore>, preset_id: String) -> AppResult<()> {
    store.delete_custom(&preset_id)
}

#[tauri::command]
pub fn duplicate_custom_preset(
    store: State<'_, PresetStore>,
    preset_id: String,
    name: String,
) -> AppResult<PresetSaved> {
    let preset = store.duplicate(&preset_id, &name)?;
    Ok(PresetSaved { preset })
}

#[tauri::command]
pub fn rename_custom_preset(
    store: State<'_, PresetStore>,
    preset_id: String,
    name: String,
) -> AppResult<PresetSaved> {
    let preset = store.rename(&preset_id, &name)?;
    Ok(PresetSaved { preset })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetFileResult {
    pub path: String,
    pub preset: RetouchPreset,
}

#[tauri::command]
pub async fn export_preset(
    store: State<'_, PresetStore>,
    preset_id: String,
    destination: Option<String>,
) -> AppResult<PresetFileResult> {
    let store = store.inner().clone();
    let preset = store
        .find(&preset_id)
        .ok_or_else(|| AppError::InvalidInput("预设不存在".into()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => {
                let suggested = format!("{}.{PRESET_EXTENSION}", sanitize(&preset.name));
                match rfd::FileDialog::new()
                    .set_title("导出预设")
                    .set_file_name(&suggested)
                    .add_filter("LUMI 预设", &["json"])
                    .save_file()
                {
                    Some(mut path) => {
                        if path.extension().is_none() {
                            path.set_extension("json");
                        }
                        path
                    }
                    None => return Err(AppError::Cancelled),
                }
            }
        };
        store.export_to(&preset_id, &target)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub async fn import_preset(
    store: State<'_, PresetStore>,
    source: Option<String>,
) -> AppResult<PresetSaved> {
    let path = match source {
        Some(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => match rfd::FileDialog::new()
            .set_title("导入预设")
            .add_filter("LUMI 预设", &["json"])
            .pick_file()
        {
            Some(path) => path,
            None => return Err(AppError::Cancelled),
        },
    };
    let data = std::fs::read(&path)?;
    let preset = store.import_json(&data)?;
    Ok(PresetSaved { preset })
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|value| {
            if value.is_alphanumeric() || value == '-' || value == '_' {
                value
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.trim_matches('-').is_empty() {
        "preset".into()
    } else {
        cleaned.trim_matches('-').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (PresetStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("lumi-preset-{tag}-{}", now_ms()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("presets.json");
        (PresetStore::with_path(path), dir)
    }

    fn sample_params(exposure: f32) -> PresetParams {
        PresetParams {
            exposure: Some(exposure),
            contrast: Some(12.0),
            ..PresetParams::default()
        }
    }

    #[test]
    fn custom_preset_survives_restart_and_builtins_are_not_persisted() {
        let (store, dir) = temp_store("restart");
        let saved = store
            .save_custom(None, "我的暖调", "测试用", sample_params(18.0))
            .unwrap();
        assert_eq!(saved.source, PresetSource::Custom);

        let reopened = PresetStore::with_path(dir.join("presets.json"));
        let custom = reopened.custom();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].name, "我的暖调");
        assert_eq!(custom[0].params.exposure, Some(18.0));
        // 内置预设由代码提供，不从用户文件加载，也不会被重复保存。
        assert!(reopened.find("builtin-cinematic-teal").is_some());
        assert_eq!(reopened.find("builtin-cinematic-teal").unwrap().source, PresetSource::Builtin);
        let raw = std::fs::read(dir.join("presets.json")).unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(!text.contains("builtin-"), "用户文件不得写入内置预设");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_rejects_empty_name_and_does_not_allow_overwriting_builtin() {
        let (store, dir) = temp_store("guard");
        assert!(store
            .save_custom(None, "   ", "空名称", sample_params(10.0))
            .is_err());
        assert!(store
            .save_custom(None, "空参数", "没有参数", PresetParams::default())
            .is_err());
        assert!(store
            .save_custom(Some("builtin-cinematic-teal"), "改名", "", sample_params(10.0))
            .is_err());
        assert_eq!(store.custom().len(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_rename_duplicate_and_delete_custom_preset() {
        let (store, dir) = temp_store("crud");
        let saved = store
            .save_custom(None, "初版", "说明", sample_params(10.0))
            .unwrap();
        let updated = store
            .save_custom(Some(&saved.id), "改版", "新说明", sample_params(25.0))
            .unwrap();
        assert_eq!(updated.id, saved.id, "覆盖保存必须保持同一预设 ID");
        assert_eq!(updated.params.exposure, Some(25.0));
        assert_eq!(store.custom().len(), 1);

        let renamed = store.rename(&saved.id, "最终名称").unwrap();
        assert_eq!(renamed.name, "最终名称");

        // 复制内置预设允许，且生成新的自定义 ID。
        let copy = store.duplicate("builtin-natural-portrait", "").unwrap();
        assert_ne!(copy.id, "builtin-natural-portrait");
        assert_eq!(copy.source, PresetSource::Custom);
        assert!(copy.name.contains("副本"));

        store.delete_custom(&saved.id).unwrap();
        assert_eq!(store.custom().len(), 1);
        assert!(store.delete_custom(&saved.id).is_err(), "重复删除必须报错");
        assert!(store.delete_custom("builtin-natural-portrait").is_err());
        assert!(store.rename("builtin-natural-portrait", "x").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_then_import_roundtrip_creates_new_custom_preset() {
        let (store, dir) = temp_store("roundtrip");
        let saved = store
            .save_custom(None, "导出测试", "逐字段保留", sample_params(30.0))
            .unwrap();
        let file = dir.join("exported.json");
        let exported = store.export_to(&saved.id, &file).unwrap();
        assert!(file.exists());
        assert_eq!(exported.preset.params.exposure, Some(30.0));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("\"faces\""));
        assert!(!text.contains("\"manualMask\""));

        let imported = store.import_json(&text.into_bytes()).unwrap();
        assert_ne!(imported.id, saved.id, "导入必须生成新 ID");
        assert_eq!(imported.source, PresetSource::Custom);
        assert_eq!(imported.params.exposure, Some(30.0));
        assert_eq!(store.custom().len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_rejects_malformed_or_empty_preset_file() {
        let (store, dir) = temp_store("invalid");
        assert!(store.import_json(b"not json").is_err());
        let empty = serde_json::to_vec(&RetouchPreset {
            id: "x".into(),
            name: "空".into(),
            category: PresetCategory::Custom,
            description: String::new(),
            version: "1.0".into(),
            source: PresetSource::Custom,
            params: PresetParams::default(),
            suitable_for: String::new(),
            avoid: String::new(),
            preview: None,
        })
        .unwrap();
        assert!(store.import_json(&empty).is_err());
        assert_eq!(store.custom().len(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn persisted_file_with_builtin_entry_is_filtered_out() {
        let (_, dir) = temp_store("filter");
        let builtin = builtin_presets().remove(0);
        let file = PresetStoreFile {
            schema_version: PRESET_SCHEMA_VERSION,
            presets: vec![builtin],
        };
        std::fs::write(
            dir.join("presets.json"),
            serde_json::to_vec_pretty(&file).unwrap(),
        )
        .unwrap();
        let reopened = PresetStore::with_path(dir.join("presets.json"));
        assert!(reopened.custom().is_empty(), "内置来源的记录不得被当成用户预设");
        std::fs::remove_dir_all(&dir).ok();
    }
}


