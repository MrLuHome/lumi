//! 前端快照形状（camelCase，逐字对齐 `src/types.ts`）。
//!
//! 快照与「检查」是两件事：
//! - [`RegistrySnapshot`] 只读内存 + `is_file()`，**不计算任何哈希**，可在面板挂载时调用；
//! - 全量校验走 `verify::check`，成本与权重体积成正比。
//!
//! 用户自备模型（Q1 裁决）会出现在快照里，但 `official == false`，
//! `license.state == "userUnverified"`，明确标注「未审核 · 仅本地」，
//! 并且不参与默认美肤 / 默认精修。

use serde::Serialize;

use crate::face_intelligence::capability::LicenseState;

use super::license::{LicenseEvidence, LicenseRecord, Verdict3};
use super::spec::{ModelFormat, ModelOrigin, ModelSpec, ModelTask, OutputHead};

/// 快照 schema 版本。字段增删时递增，前端可据此分支。
pub const SCHEMA_VERSION: u32 = 1;

/// 许可视图：四个字段独立，未核实字段单列。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseView {
    pub state: LicenseState,
    pub spdx: Option<String>,
    pub weights: String,
    pub commercial_use: String,
    pub redistribution: String,
    pub training_data: String,
    pub unverified_fields: Vec<String>,
    pub evidence: Vec<LicenseEvidence>,
    pub note: String,
}

impl LicenseView {
    pub fn from_record(origin: ModelOrigin, record: &LicenseRecord) -> Self {
        Self {
            state: super::license::license_state(origin, record),
            spdx: record.spdx.map(|value| value.to_string()),
            weights: record.weights.label().to_string(),
            commercial_use: record.commercial_use.label().to_string(),
            redistribution: record.redistribution.label().to_string(),
            training_data: record.training_data.label().to_string(),
            unverified_fields: record
                .unverified_fields()
                .into_iter()
                .map(|field| field.to_string())
                .collect(),
            evidence: record.evidence.to_vec(),
            note: record.note.to_string(),
        }
    }

    /// 用户自备模型：一律 `UserUnverified`。
    ///
    /// `declared_*` 是用户在导入时**自己勾选**的声明，不等于核实结果，
    /// 因此这里不把它们映射成 `Allowed` —— 只写进 `note` 供人复核。
    pub fn user_unverified(
        license_name: &str,
        note: &str,
        declared_commercial: bool,
        declared_redistribution: bool,
    ) -> Self {
        const HINT: &str =
            "用户声明不等于核实：需人工核对权重发布页与数据集条款后才能进入正式注册清单";
        let verdict = Verdict3::Unverified {
            what_to_check: HINT,
        };
        let declaration = format!(
            "用户声明：商用={}，再分发={}",
            if declared_commercial { "是" } else { "否" },
            if declared_redistribution { "是" } else { "否" }
        );
        Self {
            state: LicenseState::UserUnverified,
            spdx: if license_name.trim().is_empty() {
                None
            } else {
                Some(license_name.trim().to_string())
            },
            weights: verdict.label().to_string(),
            commercial_use: verdict.label().to_string(),
            redistribution: verdict.label().to_string(),
            training_data: verdict.label().to_string(),
            unverified_fields: vec![
                "weights".into(),
                "commercialUse".into(),
                "redistribution".into(),
                "trainingData".into(),
            ],
            evidence: Vec::new(),
            note: if note.trim().is_empty() {
                format!("用户自备模型；{declaration}；LUMI 未核实许可证，不随安装包分发，不参与默认美肤/默认精修")
            } else {
                format!("{note}；{declaration}；LUMI 未核实，不随安装包分发，不参与默认美肤/默认精修")
            },
        }
    }
}

/// 单条模型的前端视图。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntryView {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub task: ModelTask,
    pub origin: ModelOrigin,
    /// 是否属于**正式注册清单**。用户自备模型恒为 `false`。
    pub official: bool,
    pub format: ModelFormat,
    pub file: String,
    pub size_bytes: u64,
    pub size_on_disk_bytes: u64,
    pub sha256: String,
    pub input: crate::ai_engine::inference::TensorSpec,
    pub outputs: Vec<OutputHead>,
    pub license: LicenseView,
    /// verified | ready | licenseBlocked | missing | corrupt | versionMismatch | loadFailed
    pub state: &'static str,
    pub loaded: bool,
    pub path: Option<String>,
    pub failure: Option<super::state::ModelFailure>,
    /// 全部发现（含被 blocking 掩盖的）。
    pub findings: Vec<super::state::ModelFailure>,
    pub checked_at_unix: u64,
}

impl ModelEntryView {
    /// 从内置登记记录构造视图。
    pub fn from_spec(
        spec: &ModelSpec,
        state: &super::state::ModelRuntimeState,
        findings: Vec<super::state::ModelFailure>,
        size_on_disk_bytes: u64,
        checked_at_unix: u64,
    ) -> Self {
        Self {
            id: spec.id.to_string(),
            display_name: spec.display_name.to_string(),
            version: spec.version.to_string(),
            task: spec.task,
            origin: spec.origin,
            official: spec.origin.is_official(),
            format: spec.format,
            file: spec.file_name.to_string(),
            size_bytes: spec.size_bytes,
            size_on_disk_bytes,
            sha256: spec.sha256.to_string(),
            input: spec.input.clone(),
            outputs: spec.outputs.to_vec(),
            license: LicenseView::from_record(spec.origin, &spec.license),
            state: state.as_str(),
            loaded: state.is_loaded(),
            path: state
                .path()
                .map(|path| path.to_string_lossy().into_owned()),
            failure: state.failure().cloned(),
            findings,
            checked_at_unix,
        }
    }
}

/// 用户自备模型的最小描述（由 registry 从 model_hub manifest 读出）。
#[derive(Debug, Clone)]
pub struct UserModelSnapshot {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub file_name: String,
    pub model_path: Option<String>,
    pub declared_sha256: String,
    pub size_bytes: u64,
    pub present: bool,
    pub validation_ok: bool,
    pub license_name: String,
    pub license_note: String,
    pub declared_commercial: bool,
    pub declared_redistribution: bool,
}

impl UserModelSnapshot {
    /// 转成视图：恒 `official == false`、`origin == userImported`。
    pub fn into_view(self, checked_at_unix: u64) -> ModelEntryView {
        let input = crate::ai_engine::inference::TensorSpec {
            dtype: crate::ai_engine::inference::Dtype::F32,
            layout: crate::ai_engine::inference::Layout::Nchw,
            // 用户模型的 IO 契约由它自己的 manifest 描述，登记表只能给**占位**规格：
            // 形状全 0、值域与通道顺序都是中性默认值，**都不代表真实契约**。
            // `TensorSpec::is_placeholder()` 会判定为占位，`TensorBatch::validate()`
            // 显式拒绝它 —— 占位值不会被当成零元素张量跑推理。
            // （这里出现 `ZeroOne` 只是"未知值域"的中性占位，与 YuNet 的 Raw255 无关。）
            shape: std::borrow::Cow::Borrowed(&[0, 0, 0, 0]),
            value_range: crate::ai_engine::inference::ValueRange::ZeroOne,
            channel_order: crate::ai_engine::inference::ChannelOrder::Rgb,
        };
        let (state, failure, findings) = if self.present {
            if self.validation_ok {
                ("verified", None, Vec::new())
            } else {
                (
                    "loadFailed",
                    Some(super::state::ModelFailure::LoadFailed {
                        model_id: self.id.clone(),
                        detail: "用户模型尚未通过隔离验证".into(),
                    }),
                    Vec::new(),
                )
            }
        } else {
            let missing = super::state::ModelFailure::Missing {
                model_id: self.id.clone(),
                searched: self.model_path.iter().cloned().collect(),
            };
            ("missing", Some(missing.clone()), vec![missing])
        };
        ModelEntryView {
            id: self.id,
            display_name: self.display_name,
            version: self.version,
            task: ModelTask::FaceLandmark,
            origin: ModelOrigin::UserImported,
            official: false,
            format: ModelFormat::Onnx,
            file: self.file_name,
            size_bytes: self.size_bytes,
            size_on_disk_bytes: if self.present { self.size_bytes } else { 0 },
            sha256: self.declared_sha256,
            input,
            outputs: Vec::new(),
            license: LicenseView::user_unverified(
                &self.license_name,
                &self.license_note,
                self.declared_commercial,
                self.declared_redistribution,
            ),
            state,
            loaded: false,
            path: self.model_path,
            failure,
            findings,
            checked_at_unix,
        }
    }
}

/// 汇总计数。`total` / `official` 分开，避免用户模型把正式清单的数字做大。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub total: usize,
    pub official: usize,
    pub ready: usize,
    pub verified: usize,
    pub blocked: usize,
    pub user_imported: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySnapshot {
    pub schema_version: u32,
    pub generated_at_unix: u64,
    pub models: Vec<ModelEntryView>,
    pub summary: SnapshotSummary,
}

impl RegistrySnapshot {
    pub fn new(generated_at_unix: u64, models: Vec<ModelEntryView>) -> Self {
        let mut summary = SnapshotSummary::default();
        for entry in &models {
            summary.total += 1;
            if entry.official {
                summary.official += 1;
            } else {
                summary.user_imported += 1;
            }
            match entry.state {
                "ready" => summary.ready += 1,
                "verified" => summary.verified += 1,
                _ => summary.blocked += 1,
            }
        }
        Self {
            schema_version: SCHEMA_VERSION,
            generated_at_unix,
            models,
            summary,
        }
    }

    pub fn find(&self, id: &str) -> Option<&ModelEntryView> {
        self.models.iter().find(|entry| entry.id == id)
    }

    /// 把某条模型标成「已加载」。供命令层在预热/检测后回填。
    pub fn mark_ready(&mut self, id: &str, loaded: bool) {
        if !loaded {
            return;
        }
        let Some(entry) = self.models.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        entry.state = "ready";
        entry.loaded = true;
        entry.failure = None;
        self.summary = SnapshotSummary::recount(&self.models);
    }

    /// 只有正式注册清单里的模型才允许参与默认美肤 / 默认精修。
    pub fn default_eligible(&self) -> Vec<&ModelEntryView> {
        self.models
            .iter()
            .filter(|entry| entry.official && matches!(entry.state, "verified" | "ready"))
            .collect()
    }
}

impl SnapshotSummary {
    fn recount(models: &[ModelEntryView]) -> Self {
        let mut summary = SnapshotSummary::default();
        for entry in models {
            summary.total += 1;
            if entry.official {
                summary.official += 1;
            } else {
                summary.user_imported += 1;
            }
            match entry.state {
                "ready" => summary.ready += 1,
                "verified" => summary.verified += 1,
                _ => summary.blocked += 1,
            }
        }
        summary
    }
}

/// 当前 UNIX 秒（不引入时间库）。
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::model_registry::builtin;
    use crate::ai_engine::model_registry::state::ModelRuntimeState;

    fn entry(id: &str, state: ModelRuntimeState, official: bool) -> ModelEntryView {
        let mut view = ModelEntryView::from_spec(&builtin::YUNET, &state, Vec::new(), 232_589, 1);
        view.id = id.to_string();
        view.official = official;
        view.origin = if official {
            ModelOrigin::Builtin
        } else {
            ModelOrigin::UserImported
        };
        view
    }

    #[test]
    fn snapshot_serializes_with_camel_case_and_all_fields() {
        let path = std::path::PathBuf::from("/models/yunet/face.onnx");
        let view = ModelEntryView::from_spec(
            &builtin::YUNET,
            &ModelRuntimeState::Verified { path },
            Vec::new(),
            232_589,
            1_795_300_000,
        );
        let snapshot = RegistrySnapshot::new(1_795_300_000, vec![view]);
        let value = serde_json::to_value(&snapshot).unwrap();

        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["generatedAtUnix"], 1_795_300_000u64);
        assert_eq!(value["summary"]["total"], 1);
        assert_eq!(value["summary"]["verified"], 1);

        let model = &value["models"][0];
        assert_eq!(model["id"], builtin::YUNET.id);
        assert_eq!(model["displayName"], "YuNet 2023mar");
        assert_eq!(model["task"], "faceDetect");
        assert_eq!(model["origin"], "builtin");
        assert_eq!(model["official"], true);
        assert_eq!(model["format"], "onnx");
        assert_eq!(model["sizeBytes"], 232_589);
        assert_eq!(model["sizeOnDiskBytes"], 232_589);
        assert_eq!(model["sha256"], builtin::YUNET.sha256);
        assert_eq!(model["state"], "verified");
        assert_eq!(model["loaded"], false);
        assert_eq!(model["path"], "/models/yunet/face.onnx");
        assert!(model["failure"].is_null());
        assert_eq!(model["findings"].as_array().unwrap().len(), 0);

        // 输入契约结构化，不是自由字符串。
        assert_eq!(model["input"]["dtype"], "f32");
        assert_eq!(model["input"]["layout"], "nchw");
        assert_eq!(model["input"]["valueRange"], "raw255");
        assert_eq!(model["input"]["channelOrder"], "bgr");
        assert_eq!(model["input"]["shape"], serde_json::json!([1, 3, 640, 640]));
        assert_eq!(model["outputs"].as_array().unwrap().len(), 12);
        assert_eq!(model["outputs"][0]["name"], "cls_8");
        assert_eq!(
            model["outputs"][0]["shape"],
            serde_json::json!([1, 6400, 1])
        );

        // 许可四字段独立 + 未核实字段单列。
        let license = &model["license"];
        assert_eq!(license["state"], "verifiedCommercial");
        assert_eq!(license["spdx"], "MIT");
        assert_eq!(license["weights"], "allowed");
        assert_eq!(license["commercialUse"], "allowed");
        assert_eq!(license["redistribution"], "allowed");
        assert_eq!(license["trainingData"], "unverified");
        assert_eq!(
            license["unverifiedFields"],
            serde_json::json!(["trainingData"])
        );
        assert_eq!(license["evidence"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn failure_and_findings_both_serialize() {
        let failure = crate::ai_engine::model_registry::state::ModelFailure::LicenseNotCleared {
            model_id: builtin::YUNET.id.into(),
            which: "commercialUse",
            what_to_check: "核对数据集条款".into(),
        };
        let view = ModelEntryView::from_spec(
            &builtin::YUNET,
            &ModelRuntimeState::LicenseBlocked(failure.clone()),
            vec![failure],
            0,
            1,
        );
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["state"], "licenseBlocked");
        assert_eq!(value["failure"]["kind"], "licenseBlocked");
        assert_eq!(value["failure"]["modelId"], builtin::YUNET.id);
        assert_eq!(value["findings"][0]["kind"], "licenseBlocked");
    }

    #[test]
    fn user_models_never_enter_the_official_list() {
        let user = UserModelSnapshot {
            id: "user-kpt-1".into(),
            display_name: "用户关键点".into(),
            version: "1.0".into(),
            file_name: "model.onnx".into(),
            model_path: Some("/data/face-models/user-kpt-1/model.onnx".into()),
            declared_sha256: "aa".repeat(32),
            size_bytes: 1024,
            present: true,
            validation_ok: true,
            license_name: "UNKNOWN".into(),
            license_note: "用户自备".into(),
            declared_commercial: true,
            declared_redistribution: false,
        };
        let view = user.into_view(1);
        assert!(!view.official);
        assert_eq!(view.origin, ModelOrigin::UserImported);
        assert_eq!(view.license.state, LicenseState::UserUnverified);
        assert_eq!(view.license.unverified_fields.len(), 4);
        // 用户模型的形状是**占位**，不允许被当成零元素张量喂进推理。
        assert!(view.input.is_placeholder());

        let snapshot = RegistrySnapshot::new(
            1,
            vec![
                entry(
                    builtin::YUNET.id,
                    ModelRuntimeState::Verified {
                        path: std::path::PathBuf::from("/x"),
                    },
                    true,
                ),
                view,
            ],
        );
        assert_eq!(snapshot.summary.total, 2);
        assert_eq!(snapshot.summary.official, 1);
        assert_eq!(snapshot.summary.user_imported, 1);
        // 默认管线只认正式清单。
        assert!(snapshot
            .default_eligible()
            .iter()
            .all(|entry| entry.official));
        assert!(!snapshot.find("user-kpt-1").unwrap().official);
    }

    #[test]
    fn mark_ready_updates_state_and_summary() {
        let mut snapshot = RegistrySnapshot::new(
            1,
            vec![entry(
                builtin::YUNET.id,
                ModelRuntimeState::Verified {
                    path: std::path::PathBuf::from("/x"),
                },
                true,
            )],
        );
        assert_eq!(snapshot.summary.verified, 1);
        snapshot.mark_ready(builtin::YUNET.id, true);
        assert_eq!(snapshot.models[0].state, "ready");
        assert!(snapshot.models[0].loaded);
        assert_eq!(snapshot.summary.ready, 1);
        assert_eq!(snapshot.summary.verified, 0);
    }
}
