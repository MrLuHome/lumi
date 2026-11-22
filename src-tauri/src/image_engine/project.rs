use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::face_reshape::identity::ProjectFace;
use crate::image_engine::export::ExportSettings;
use crate::image_engine::params::{BodyShape, EditParams};

/// 版本 3：新增人物对象、蒙版元数据与身材/背景参数。
/// 版本 1/2 项目仍然可以打开，缺失字段使用默认值。
pub const PROJECT_VERSION: u32 = 3;
pub const PROJECT_EXTENSION: &str = "lumi";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MaskKind {
    #[default]
    Brush,
    Linear,
    Radial,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaskPoint {
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_radius")]
    pub radius: f32,
}

fn default_radius() -> f32 {
    0.08
}

/// 蒙版层只保存可重放的矢量数据，不保存像素结果，
/// 因此重新打开项目后仍可无损调整。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskLayer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: MaskKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "default_feather")]
    pub feather: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub points: Vec<MaskPoint>,
}

fn default_true() -> bool {
    true
}

fn default_feather() -> f32 {
    40.0
}

fn default_opacity() -> f32 {
    100.0
}

/// 第五轮人物记录。`subject_id` 与 `params.body[].subjectId` 一致，
/// 不复用人脸 UUID，避免身材与人脸参数在重开后错配。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectSubject {
    pub subject_id: String,
    pub mask_source: String,
    pub mask_model_version: String,
    pub mask_strokes: usize,
    pub body: BodyShape,
    pub anchors: usize,
}

/// 蒙版来源与版本。不保存 Alpha 像素，只保存可重放的笔触与哈希。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MaskMeta {
    /// 当前固定为 `manual`；未来 AI 模型可用时写入模型 ID。
    pub source: String,
    pub model_id: String,
    pub model_version: String,
    /// 最近一次渲染的覆盖率。由前端或渲染报告回填，无法计算时为 0。
    pub coverage: f32,
    pub stroke_hash: String,
    pub revision: u32,
}

/// 第七轮：来源信息（RAW/Raster）、RAW 显影参数、色彩输出设置与计算后端。
/// 字段全部可选，旧项目（v1/v2/v3）缺失时按默认值处理，不要求迁移。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectSource {
    /// `raw` 或 `raster`。
    pub source_type: String,
    pub path: String,
    pub hash: String,
    /// 相机/文件元数据快照（JSON，仅用于展示，不参与渲染）。
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectRaw {
    pub params: crate::image_engine::raw::RawDevelopParams,
    /// 解码器版本；变化时提示可能需要重新渲染。
    pub decoder_version: String,
    /// 旧/新版本写入的未知字段：加载后必须原样带回。
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectColor {
    pub output: crate::color_management::ColorOutputSettings,
    /// 旧/新版本写入的未知字段：加载后必须原样带回。
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectCompute {
    pub preference: crate::gpu::BackendPreference,
}

/// 当前 RAW 解码/显影实现版本，写入项目用于提示重渲染。
pub const RAW_DECODER_VERSION: &str = "libraw-0.21.1+lumi-mhc-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectPhoto {
    pub path: String,
    pub params: EditParams,
    pub masks: Vec<MaskLayer>,
    pub export: Option<ExportSettings>,
    /// 原图文件 SHA-256。重开时若变化，不自动套用任何人脸参数。
    pub source_hash: String,
    /// 持久化人脸记录：UUID、检测框、初始关键点、置信度、模型版本。
    pub faces: Vec<ProjectFace>,
    /// 第五轮人物记录（身材对象、独立蒙版笔触数量、锚点数量）。
    pub subjects: Vec<ProjectSubject>,
    /// 第五轮蒙版元数据。
    pub mask_meta: Option<MaskMeta>,
    /// 第七轮：来源文件信息（RAW 缺失时保留参数并提示重定位）。
    pub source: Option<ProjectSource>,
    /// 第七轮：RAW 显影参数。
    pub raw: Option<ProjectRaw>,
    /// 第七轮：色彩输出设置。
    pub color: Option<ProjectColor>,
    /// 未识别的字段原样保留：旧客户端保存新版本项目时不得静默丢弃未知数据。
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl ProjectPhoto {
    /// 保存前把第五轮记录与 `params` 同步，避免两份数据不一致。
    pub fn sync_subject_records(&mut self) {
        self.subjects = self
            .params
            .body
            .iter()
            .map(|entry| ProjectSubject {
                subject_id: entry.subject_id.clone(),
                mask_source: if entry.has_own_mask() {
                    "manual".into()
                } else {
                    self.params.subject_mask.source.clone()
                },
                mask_model_version: self.params.subject_mask.model_version.clone(),
                mask_strokes: entry.mask_strokes.len(),
                body: entry.shape,
                anchors: entry.anchors.len(),
            })
            .collect();
        let mask = &self.params.subject_mask;
        let stroke_hash = mask.stroke_hash();
        // 没有任何蒙版内容、也没有历史元数据的照片不凭空生成 maskMeta。
        if self.mask_meta.is_none() && mask.strokes.is_empty() && self.subjects.is_empty() {
            return;
        }
        // 覆盖率与 revision 是渲染侧回填的元数据，保存不能把它们重置。
        // 只有笔触/形状真正变化时才递增 revision 并作废旧覆盖率。
        let previous = self.mask_meta.as_ref();
        let unchanged = previous
            .map(|meta| {
                meta.stroke_hash == stroke_hash
                    && meta.source == mask.source
                    && meta.model_version == mask.model_version
            })
            .unwrap_or(false);
        self.mask_meta = Some(MaskMeta {
            source: mask.source.clone(),
            model_id: previous
                .map(|meta| meta.model_id.clone())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "manual-alpha".into()),
            model_version: mask.model_version.clone(),
            coverage: if unchanged {
                previous.map(|meta| meta.coverage).unwrap_or(0.0)
            } else {
                0.0
            },
            stroke_hash,
            revision: if unchanged {
                previous.map(|meta| meta.revision).unwrap_or(1)
            } else {
                previous.map(|meta| meta.revision + 1).unwrap_or(1)
            },
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDoc {
    pub version: u32,
    pub created_at: String,
    #[serde(default)]
    pub active_index: usize,
    #[serde(default)]
    pub export: ExportSettings,
    /// 第七轮：计算后端偏好（Auto/Gpu/Cpu）。
    #[serde(default)]
    pub compute: Option<ProjectCompute>,
    pub photos: Vec<ProjectPhoto>,
    /// 项目身份：用于防止 A 项目的保存结果覆盖 B 项目。
    #[serde(default)]
    pub project_uuid: String,
    /// 保存代数：每次成功保存 +1，旧快照不得覆盖更新的磁盘内容。
    #[serde(default)]
    pub revision: u64,
    /// 最近一次成功保存时间（ISO 8601）。`createdAt` 必须保持不变。
    #[serde(default)]
    pub updated_at: String,
    /// 未识别的字段原样保留，不因一次保存而丢失。
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl ProjectDoc {
    pub fn new(photos: Vec<ProjectPhoto>, active_index: usize, export: ExportSettings) -> Self {
        Self {
            version: PROJECT_VERSION,
            created_at: now_label(),
            active_index,
            export,
            compute: None,
            photos,
            project_uuid: String::new(),
            revision: 0,
            updated_at: String::new(),
            extra: BTreeMap::new(),
        }
    }
}

fn now_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}")
}

pub(crate) fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    // 仅用于“上次保存时间”展示，精确到秒的 Unix 秒与 ISO 前缀均可；
    // 这里保持与前端 createdAt 一致的 ISO 形式。
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Howard Hinnant 的 days -> civil 转换，避免为时间戳引入新依赖。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// 保存结果：前端用它更新项目身份与保存代数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    pub project_uuid: String,
    pub revision: u64,
}

/// 读取磁盘上已有项目的身份信息；损坏或不存在时返回 None。
/// 只解析需要的字段，不因未知/新增字段失败。
fn read_existing_identity(path: &Path) -> Option<(String, u64)> {
    let bytes = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let uuid = value
        .get("projectUuid")
        .and_then(|entry| entry.as_str())
        .unwrap_or("")
        .to_string();
    let revision = value.get("revision").and_then(|entry| entry.as_u64()).unwrap_or(0);
    Some((uuid, revision))
}

/// 项目文件是纯 JSON（参数 + 蒙版 + 导出设置），不包含预览图。
///
/// 写入流程是“单文件事务”：临时文件 → 写入 → fsync → 回读校验 → 备份 →
/// 原子 rename → 目录 fsync。任一步失败都不会留下被误认为有效的半成品，
/// 原项目文件保持可读。
pub fn save(path: &Path, project: &ProjectDoc) -> AppResult<()> {
    save_with_meta(path, project).map(|_| ())
}

pub fn save_with_meta(path: &Path, project: &ProjectDoc) -> AppResult<SaveOutcome> {
    if project.version > PROJECT_VERSION {
        return Err(AppError::Unsupported(format!(
            "项目版本 {} 高于当前支持的 {}",
            project.version, PROJECT_VERSION
        )));
    }
    let mut project = project.clone();

    // 并发保护：同一路径同时只允许一个保存事务，避免两个写入交错。
    let _guard = lock_saves()?;

    let existing = read_existing_identity(path);
    if project.project_uuid.is_empty() {
        project.project_uuid = match &existing {
            Some((uuid, _)) if !uuid.is_empty() => uuid.clone(),
            _ => crate::ai_engine::face_frame::new_face_uuid(),
        };
    } else if let Some((uuid, _)) = &existing {
        if !uuid.is_empty() && *uuid != project.project_uuid {
            return Err(AppError::Task(
                "目标文件属于另一个项目，已拒绝覆盖以免数据错配".into(),
            ));
        }
    }
    // 旧快照不得覆盖磁盘上更新的保存结果。`revision == 0` 表示调用方
    // 没有携带保存代数（旧客户端/新项目），此时不做代数比较但仍递增写盘。
    if project.revision > 0 {
        if let Some((uuid, disk_revision)) = &existing {
            let same_project = uuid.is_empty() || *uuid == project.project_uuid;
            if same_project && *disk_revision > project.revision {
                return Err(AppError::Task(format!(
                    "项目已在别处更新（磁盘 revision {}，本次 revision {}），请重新打开后再保存",
                    disk_revision, project.revision
                )));
            }
        }
    }
    project.revision = existing
        .as_ref()
        .map(|(_, revision)| *revision)
        .unwrap_or(0)
        .max(project.revision)
        + 1;
    project.updated_at = iso_now();

    for photo in project.photos.iter_mut() {
        photo.sync_subject_records();
    }
    let json = serde_json::to_vec_pretty(&project)
        .map_err(|error| AppError::Task(format!("项目序列化失败：{error}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("lumi.tmp");
    let outcome = SaveOutcome {
        project_uuid: project.project_uuid.clone(),
        revision: project.revision,
    };

    let write_result = write_atomically(path, &json);
    if write_result.is_err() {
        // 失败时清理临时文件，避免留下看起来像项目文件的半成品。
        let _ = std::fs::remove_file(&temporary);
    }
    write_result.map(|_| outcome)
}

/// 保存事务锁。整包保存与 ChangeSet 保存共用，防止两条路径交错写入。
pub(crate) fn lock_saves() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SAVE_LOCK
        .lock()
        .map_err(|_| AppError::Task("项目保存锁已损坏".into()))
}

/// 原子写：临时文件 → fsync → 回读校验 → 备份 → rename → 目录 fsync → 回读校验。
/// ChangeSet 提交复用同一实现，保证两条保存路径具有相同的持久化语义。
pub(crate) fn write_atomically(path: &Path, json: &[u8]) -> AppResult<()> {
    let temporary = path.with_extension("lumi.tmp");
    let result = write_atomically_inner(path, &temporary, json);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn write_atomically_inner(path: &Path, temporary: &Path, json: &[u8]) -> AppResult<()> {
    {
        let mut file = std::fs::File::create(temporary)?;
        file.write_all(json)?;
        // fsync：确认数据真正落盘后再替换，崩溃时不会得到空/截断文件。
        file.sync_all()?;
    }
    let written = std::fs::read(temporary)?;
    if written != json {
        return Err(AppError::Task("项目写入校验失败".into()));
    }
    // 覆盖已有项目前保留一份备份；备份失败时中止，不冒险覆盖唯一副本。
    if path.exists() {
        let backup = path.with_extension("lumi.bak");
        std::fs::copy(path, &backup)?;
    }
    std::fs::rename(temporary, path)?;
    // 目录 fsync：确保 rename 本身持久化（部分平台不支持则忽略）。
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    // 回读校验：确认最终文件与序列化结果一致。
    let final_bytes = std::fs::read(path)?;
    if final_bytes != json {
        return Err(AppError::Task("项目写入校验失败".into()));
    }
    Ok(())
}

pub fn load(path: &Path) -> AppResult<ProjectDoc> {
    load_with_report(path).map(|(project, _)| project)
}

/// 加载项目并返回迁移日志。
///
/// 迁移只“补默认值”，从不删除字段：未知字段、稠密关键点、RAW 与 ICC 信息
/// 全部原样保留。版本高于当前支持时直接拒绝。
pub fn load_with_report(path: &Path) -> AppResult<(ProjectDoc, Vec<String>)> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(AppError::Unsupported("项目文件超过 64MB".into()));
    }
    let bytes = std::fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Decode(format!("项目文件格式错误：{error}")))?;
    let project: ProjectDoc = serde_json::from_value(value.clone())
        .map_err(|error| AppError::Decode(format!("项目文件格式错误：{error}")))?;
    if project.version > PROJECT_VERSION {
        return Err(AppError::Unsupported(format!(
            "项目版本 {} 高于当前支持的 {}",
            project.version, PROJECT_VERSION
        )));
    }
    if project.photos.is_empty() {
        return Err(AppError::Decode("项目中没有照片".into()));
    }
    let mut report = Vec::new();
    if project.version < PROJECT_VERSION {
        let unknown = count_unknown_fields(&value);
        report.push(format!(
            "项目版本 v{} 已按 v{} 加载：缺失字段补默认值，保留未知字段 {} 个",
            project.version, PROJECT_VERSION, unknown
        ));
        report.push(
            "迁移未修改磁盘文件；人脸稠密关键点、RAW 显影参数与 ICC 设置均原样保留".to_string(),
        );
        append_migration_log(path, &report);
    }
    Ok((project, report))
}

/// 统计对象中出现、但当前类型不认识的顶层字段数量（只用于迁移日志）。
fn count_unknown_fields(value: &serde_json::Value) -> usize {
    let Some(object) = value.as_object() else {
        return 0;
    };
    let known = [
        "version",
        "createdAt",
        "activeIndex",
        "export",
        "compute",
        "photos",
        "projectUuid",
        "revision",
        "updatedAt",
        "lumiAppliedRequests",
    ];
    object
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .count()
}

/// 追加迁移日志。日志失败绝不影响项目打开。
fn append_migration_log(path: &Path, lines: &[String]) {
    use std::io::Write;
    let Some(home) = std::env::var_os("HOME") else {
        for line in lines {
            eprintln!("{line}");
        }
        return;
    };
    let dir = Path::new(&home).join("Library/Logs/LumiRetouch");
    if std::fs::create_dir_all(&dir).is_err() {
        for line in lines {
            eprintln!("{line}");
        }
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("migration.log"))
    {
        let _ = writeln!(file, "[{}] {}", iso_now(), path.display());
        for line in lines {
            let _ = writeln!(file, "  {line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{HslChannel, HslParams};

    #[test]
    fn project_roundtrip_keeps_params_and_masks() {
        let dir = std::env::temp_dir().join("lumi-project-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portrait.lumi");

        let params = EditParams {
            exposure: 24.0,
            hsl: HslParams {
                orange: HslChannel {
                    saturation: 18.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/portrait.jpg".into(),
                params,
                masks: vec![MaskLayer {
                    id: "m1".into(),
                    name: "面部提亮".into(),
                    kind: MaskKind::Radial,
                    enabled: true,
                    invert: false,
                    feather: 30.0,
                    opacity: 80.0,
                    points: vec![MaskPoint {
                        x: 0.5,
                        y: 0.4,
                        radius: 0.2,
                    }],
                }],
                export: Some(ExportSettings::default()),
                source_hash: "abc123".into(),
                faces: Vec::new(),
                subjects: Vec::new(),
                mask_meta: None,
                source: None,
                raw: None,
                color: None,
                extra: BTreeMap::new(),
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.photos.len(), 1);
        assert_eq!(loaded.photos[0].params.exposure, 24.0);
        assert_eq!(loaded.photos[0].masks.len(), 1);
        assert_eq!(loaded.photos[0].masks[0].kind, MaskKind::Radial);
        assert_eq!(loaded.photos[0].source_hash, "abc123");
        // 二次保存保留上一版本备份，且回读校验通过。
        save(&path, &project).unwrap();
        assert!(path.with_extension("lumi.bak").exists());
        assert_eq!(load(&path).unwrap().photos[0].params.exposure, 24.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_v3_keeps_raw_color_and_compute() {
        let dir = std::env::temp_dir().join("lumi-project-v3-raw");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("raw.lumi");

        let mut photo = ProjectPhoto {
            path: "/photos/sample.CR3".into(),
            source_hash: "hash".into(),
            source: Some(ProjectSource {
                source_type: "raw".into(),
                path: "/photos/sample.CR3".into(),
                hash: "hash".into(),
                metadata: Some(serde_json::json!({"make": "Canon"})),
            }),
            ..Default::default()
        };
        photo.raw = Some(ProjectRaw {
            params: crate::image_engine::raw::RawDevelopParams {
                exposure_ev: 0.75,
                wb_mode: crate::image_engine::raw::WbMode::Custom,
                max_edge: None,
                ..Default::default()
            },
            decoder_version: RAW_DECODER_VERSION.into(),
            extra: BTreeMap::new(),
        });
        photo.color = Some(ProjectColor {
            output: crate::color_management::ColorOutputSettings {
                profile: crate::color_management::ProfileKind::AdobeRgb,
                ..Default::default()
            },
            extra: BTreeMap::new(),
        });
        let mut project = ProjectDoc::new(vec![photo], 0, ExportSettings::default());
        project.compute = Some(ProjectCompute {
            preference: crate::gpu::BackendPreference::Gpu,
        });

        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        let photo = &loaded.photos[0];
        assert_eq!(photo.source.as_ref().unwrap().source_type, "raw");
        let raw = photo.raw.as_ref().unwrap();
        assert!((raw.params.exposure_ev - 0.75).abs() < 1e-6);
        assert_eq!(raw.decoder_version, RAW_DECODER_VERSION);
        assert_eq!(
            photo.color.as_ref().unwrap().output.profile,
            crate::color_management::ProfileKind::AdobeRgb
        );
        assert_eq!(
            loaded.compute.as_ref().unwrap().preference,
            crate::gpu::BackendPreference::Gpu
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_without_round7_fields_still_opens() {
        let dir = std::env::temp_dir().join("lumi-project-legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.lumi");
        // 模拟第六轮项目：没有 source/raw/color/compute。
        let json = serde_json::json!({
            "version": 2,
            "createdAt": "1",
            "activeIndex": 0,
            "export": {},
            "photos": [{
                "path": "/photos/a.jpg",
                "params": {},
                "masks": [],
                "export": null,
                "sourceHash": "abc",
                "faces": [],
                "subjects": [],
                "maskMeta": null
            }]
        });
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.photos.len(), 1);
        assert!(loaded.photos[0].source.is_none());
        assert!(loaded.photos[0].raw.is_none());
        assert!(loaded.photos[0].color.is_none());
        assert!(loaded.compute.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_roundtrip_keeps_face_identity_records() {
        use crate::ai_engine::face_frame::NormPoint;
        use crate::face_reshape::identity::FaceBox;

        let dir = std::env::temp_dir().join("lumi-project-faces");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("group.lumi");
        let face = ProjectFace {
            uuid: "11111111-2222-4333-8444-555555555555".into(),
            bbox: FaceBox {
                x: 0.1,
                y: 0.2,
                width: 0.2,
                height: 0.24,
            },
            landmarks: vec![NormPoint { x: 0.15, y: 0.3 }; 5],
            score: 0.94,
            model: "yunet-5".into(),
            confidence: 0.94,
            mask_ref: None,
            dense: None,
            level3: None,
            extra: BTreeMap::new(),
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/group.jpg".into(),
                params: EditParams::default(),
                masks: Vec::new(),
                export: None,
                source_hash: "deadbeef".into(),
                faces: vec![face.clone()],
                subjects: Vec::new(),
                mask_meta: None,
                source: None,
                raw: None,
                color: None,
                extra: BTreeMap::new(),
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, PROJECT_VERSION);
        assert_eq!(loaded.photos[0].faces, vec![face]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v3_roundtrip_keeps_subject_body_and_mask_meta() {
        use crate::image_engine::params::{
            BackgroundMode, BackgroundParams, BodyAdjust, BodyShape, MaskPoint, SubjectMaskParams,
            SubjectMaskStroke, SubjectMaskStrokeKind,
        };
        let dir = std::env::temp_dir().join("lumi-project-v3");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v3.lumi");
        let params = EditParams {
            subject_mask: SubjectMaskParams {
                enabled: true,
                strokes: vec![SubjectMaskStroke {
                    kind: SubjectMaskStrokeKind::Paint,
                    radius: 0.2,
                    hardness: 70.0,
                    strength: 100.0,
                    points: vec![MaskPoint { x: 0.5, y: 0.6 }],
                }],
                ..Default::default()
            },
            body: vec![BodyAdjust {
                subject_id: "subject-uuid-1".into(),
                enabled: true,
                shape: BodyShape {
                    waist: -30.0,
                    ..Default::default()
                },
                anchors: Vec::new(),
                strokes: Vec::new(),
                mask_strokes: Vec::new(),
            }],
            background: BackgroundParams {
                enabled: true,
                mode: BackgroundMode::SolidColor,
                color: "#102030".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/body.jpg".into(),
                params,
                ..Default::default()
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, PROJECT_VERSION);
        let photo = &loaded.photos[0];
        assert_eq!(photo.params.body[0].subject_id, "subject-uuid-1");
        assert_eq!(photo.params.body[0].shape.waist, -30.0);
        assert_eq!(photo.params.background.mode, BackgroundMode::SolidColor);
        assert_eq!(photo.subjects.len(), 1);
        assert_eq!(photo.subjects[0].subject_id, "subject-uuid-1");
        let meta = photo.mask_meta.as_ref().unwrap();
        assert_eq!(meta.source, "manual");
        assert!(!meta.stroke_hash.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v2_project_without_new_fields_loads_with_defaults() {
        let dir = std::env::temp_dir().join("lumi-project-v2");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v2.lumi");
        std::fs::write(
            &path,
            br#"{"version":2,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[{"path":"/legacy.jpg","params":{"exposure":12}}]}"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap();
        let photo = &loaded.photos[0];
        assert!(photo.params.body.is_empty());
        assert!(!photo.params.background.is_active());
        assert!(photo.subjects.is_empty());
        assert!(photo.mask_meta.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn future_version_is_rejected() {
        let dir = std::env::temp_dir().join("lumi-project-version");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.lumi");
        std::fs::write(
            &path,
            br#"{"version":99,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[{"path":"/a.jpg"}]}"#,
        )
        .unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("版本"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_project_is_rejected() {
        let dir = std::env::temp_dir().join("lumi-project-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.lumi");
        std::fs::write(
            &path,
            br#"{"version":1,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[]}"#,
        )
        .unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
