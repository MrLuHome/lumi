//! 统一模型注册表（**权威登记层**）。
//!
//! R11 之前进程里有四个互不相识的模型登记处：
//! `ai/model.rs`（YuNet 常量）、`ai_engine/skin_segmentation/model.rs`（皮肤 + 许可闸门）、
//! `ai_engine/segmentation/model.rs`（候选目录）、`face_intelligence/model_hub/manifest.rs`
//! （用户模型 manifest）。字段不重叠、无统一状态、无统一失败语义。
//!
//! 本模块把前三个合并成**一份权威表**，并**只读**地读取第四个：
//!
//! ```text
//! ai → model_registry                      （常量派生，单向）
//! model_registry → face_intelligence::model_hub （只读 manifest，单向）
//! ```
//!
//! **禁止 `face_intelligence` 反向依赖本模块**，否则形成环。
//!
//! 边界：本模块不依赖 Tauri。`resource_dir()` 由 `commands/` 层解析成
//! 一串基础目录后传进来（见 [`locate`]）。

pub mod builtin;
pub mod license;
pub mod locate;
pub mod snapshot;
pub mod spec;
pub mod state;
pub mod verify;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub use license::{LicenseEvidence, LicenseEvidenceKind, LicenseRecord, Verdict3};
pub use snapshot::{
    now_unix, LicenseView, ModelEntryView, RegistrySnapshot, SnapshotSummary, UserModelSnapshot,
    SCHEMA_VERSION,
};
pub use spec::{ModelFormat, ModelOrigin, ModelSpec, ModelTask, OutputHead, Provenance};
pub use state::{ModelFailure, ModelHealth, ModelRuntimeState};

/// 按 ID 查找内置登记记录。
pub fn require(id: &str) -> Option<&'static ModelSpec> {
    builtin::find(id)
}

/// 用户模型目录下的 manifest 文件名（与 `model_hub::storage` 保持一致）。
const MANIFEST_FILE: &str = "manifest.json";
/// 单次快照最多读取的用户模型数量（与 `model_hub` 的安装上限一致）。
const MAX_USER_MODELS: usize = 64;

#[derive(Debug, Clone)]
struct RuntimeSlot {
    state: ModelRuntimeState,
    findings: Vec<ModelFailure>,
    checked_at_unix: u64,
}

/// 注册表运行时。只保存**元数据与状态**，不持有 plan（plan 由 `FaceEngine` 持有，
/// 避免出现进程内第二份 tract 解析结果）。
#[derive(Debug)]
pub struct Registry {
    user_models_root: Option<PathBuf>,
    runtime: RwLock<BTreeMap<String, RuntimeSlot>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Registry {
    /// `user_models_root` 是 `model_hub` 的根目录；`None` 表示不读取用户模型。
    pub fn new(user_models_root: Option<PathBuf>) -> Self {
        Self {
            user_models_root,
            runtime: RwLock::new(BTreeMap::new()),
        }
    }

    fn slots(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, RuntimeSlot>> {
        // 锁中毒不应让状态查询失效：状态只是元数据，取回内部数据继续用。
        self.runtime
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn slots_mut(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, RuntimeSlot>> {
        self.runtime
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// 记录一次检查结果。
    pub fn record(&self, health: &ModelHealth, model_id: &str) {
        let state = health.state();
        let mut slots = self.slots_mut();
        slots.insert(
            model_id.to_string(),
            RuntimeSlot {
                state,
                findings: health.findings.clone(),
                checked_at_unix: now_unix(),
            },
        );
    }

    /// 标记为已加载（plan 已进内存）。
    pub fn mark_ready(&self, model_id: &str, path: PathBuf) {
        let mut slots = self.slots_mut();
        let findings = slots
            .get(model_id)
            .map(|slot| slot.findings.clone())
            .unwrap_or_default();
        slots.insert(
            model_id.to_string(),
            RuntimeSlot {
                state: ModelRuntimeState::Ready { path },
                findings,
                checked_at_unix: now_unix(),
            },
        );
    }

    /// 标记为某个失败态（例如 tract 解析失败）。
    pub fn mark_failure(&self, model_id: &str, failure: ModelFailure) {
        let state = ModelRuntimeState::from_failure(failure.clone());
        let mut slots = self.slots_mut();
        let mut findings = slots
            .get(model_id)
            .map(|slot| slot.findings.clone())
            .unwrap_or_default();
        if !findings.contains(&failure) {
            findings.push(failure);
        }
        slots.insert(
            model_id.to_string(),
            RuntimeSlot {
                state,
                findings,
                checked_at_unix: now_unix(),
            },
        );
    }

    /// 走完整准入链（可选是否做契约校验），并记录结果。
    pub fn check(
        &self,
        spec: &ModelSpec,
        base_dirs: &[PathBuf],
        include_contract: bool,
    ) -> ModelHealth {
        let health = verify::check(spec, base_dirs, include_contract);
        self.record(&health, spec.id);
        health
    }

    /// 校验一个已知路径（热路径用，跳过契约校验，由后端在解析后补上）。
    pub fn check_at(&self, spec: &ModelSpec, path: &Path) -> ModelHealth {
        let health = verify::check_at(spec, path, false);
        self.record(&health, spec.id);
        health
    }

    /// 浅检查（许可 → 存在 → 体积/格式头，不读全文件）：供同步命令使用。
    pub fn check_shallow(&self, spec: &ModelSpec, base_dirs: &[PathBuf]) -> ModelHealth {
        let health = match locate::locate(base_dirs, spec.dir, spec.file_name) {
            Some(path) => verify::check_shallow(spec, &path),
            None => {
                let mut health = ModelHealth::new();
                if let Err(failure) = license::gate(spec.origin, &spec.license, spec.id) {
                    health.push(failure);
                }
                health.push(ModelFailure::Missing {
                    model_id: spec.id.to_string(),
                    searched: locate::describe(&locate::candidates(
                        base_dirs,
                        spec.dir,
                        spec.file_name,
                    )),
                });
                health
            }
        };
        self.record(&health, spec.id);
        health
    }

    /// 只读快照：内存状态 + `is_file` / `metadata`，**不计算任何哈希**。
    pub fn snapshot(&self, base_dirs: &[PathBuf]) -> RegistrySnapshot {
        let generated_at = now_unix();
        let slots = self.slots();
        let mut models = Vec::new();
        for spec in builtin::ALL {
            let located = locate::locate(base_dirs, spec.dir, spec.file_name);
            let size_on_disk = located
                .as_ref()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let (state, findings, checked_at) = match slots.get(spec.id) {
                Some(slot) => (
                    slot.state.clone(),
                    slot.findings.clone(),
                    slot.checked_at_unix,
                ),
                None => {
                    // 从未检查过：只用存在性给一个**保守**状态，不冒充“已验证”。
                    let state = match located {
                        Some(path) => ModelRuntimeState::Verified { path },
                        None => ModelRuntimeState::Missing(ModelFailure::Missing {
                            model_id: spec.id.to_string(),
                            searched: locate::describe(&locate::candidates(
                                base_dirs,
                                spec.dir,
                                spec.file_name,
                            )),
                        }),
                    };
                    (state, Vec::new(), generated_at)
                }
            };
            models.push(ModelEntryView::from_spec(
                spec,
                &state,
                findings,
                size_on_disk,
                checked_at,
            ));
        }
        drop(slots);
        for user in self.user_models() {
            models.push(user.into_view(generated_at));
        }
        RegistrySnapshot::new(generated_at, models)
    }

    /// 读取用户自备模型。
    ///
    /// **只解析 manifest，不计算 SHA256**：`model_hub::storage::load` 会对权重做流式哈希，
    /// 在面板挂载时调用会变成 R1 同类问题，因此这里自己读 manifest.json。
    /// 这是唯一一处刻意“绕过” model_hub 的读取（见 `docs/R11_ROUND_REPORT.md` 已知问题）。
    pub fn user_models(&self) -> Vec<UserModelSnapshot> {
        let Some(root) = &self.user_models_root else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut models = Vec::new();
        for entry in entries.flatten().take(MAX_USER_MODELS * 4) {
            let directory = entry.path();
            if !directory.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if let Some(model) = read_user_model(&directory) {
                models.push(model);
                if models.len() >= MAX_USER_MODELS {
                    break;
                }
            }
        }
        models.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        models
    }
}

fn read_user_model(directory: &Path) -> Option<UserModelSnapshot> {
    let bytes = std::fs::read(directory.join(MANIFEST_FILE)).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let manifest: crate::face_intelligence::model_hub::manifest::ModelManifest =
        serde_json::from_slice(&bytes).ok()?;
    let model_path = directory.join(&manifest.file_name);
    let metadata = std::fs::metadata(&model_path).ok();
    Some(UserModelSnapshot {
        id: manifest.id.clone(),
        display_name: manifest.name.clone(),
        version: manifest.version.clone(),
        file_name: manifest.file_name.clone(),
        model_path: metadata
            .as_ref()
            .map(|_| model_path.to_string_lossy().into_owned()),
        declared_sha256: manifest.sha256.clone(),
        size_bytes: metadata.map(|meta| meta.len()).unwrap_or(0),
        present: model_path.is_file(),
        validation_ok: manifest
            .validation
            .as_ref()
            .map(|summary| summary.ok)
            .unwrap_or(false),
        license_name: manifest.license.name.clone(),
        license_note: manifest.license.note.clone(),
        declared_commercial: manifest.license.commercial_use_claimed,
        declared_redistribution: manifest.license.redistribution_claimed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_resolves_builtin_ids_only() {
        assert!(require(builtin::YUNET.id).is_some());
        assert!(require("user-model").is_none());
    }

    #[test]
    fn snapshot_without_checks_reports_missing_for_absent_models() {
        let registry = Registry::new(None);
        let snapshot = registry.snapshot(&[PathBuf::from("/definitely/not/here")]);
        let entry = snapshot.find(builtin::YUNET.id).unwrap();
        assert_eq!(entry.state, "missing");
        assert_eq!(entry.size_on_disk_bytes, 0);
        assert_eq!(snapshot.summary.blocked, builtin::ALL.len());
    }

    /// 快照**不计算哈希**，用"可判别构造"证明，而不是靠断言"值等于登记值"
    /// （现算与不现算得到的值相同，那条断言永远绿 —— QA 指出，已修）。
    ///
    /// 构造：把真实权重拷到临时目录，**翻 1 字节但保持文件长度不变**，让登记目录
    /// 指向这份被篡改的副本。若快照真的做了 SHA256 并比对，它会判 `corrupt`；
    /// 只看存在性与体积的实现则仍报 `verified` 并给出登记值。
    #[test]
    fn snapshot_does_not_hash_the_weight() {
        let root = std::env::temp_dir().join(format!("lumi-registry-nohash-{}", std::process::id()));
        let dir = root.join("models/yunet");
        std::fs::create_dir_all(&dir).unwrap();
        let tampered = dir.join(builtin::YUNET.file_name);
        let mut bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join(builtin::YUNET.dir)
                .join(builtin::YUNET.file_name),
        )
        .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff; // 长度不变、内容已变 → 只有真算哈希才能发现
        std::fs::write(&tampered, &bytes).unwrap();

        let registry = Registry::new(None);
        let snapshot = registry.snapshot(std::slice::from_ref(&root));
        let entry = snapshot.find(builtin::YUNET.id).unwrap();
        assert_eq!(
            entry.state, "verified",
            "快照做了内容校验（不该）：{:?}",
            entry.findings
        );
        assert!(entry.failure.is_none());
        // 值来自登记表，不是现算的。
        assert_eq!(entry.sha256, builtin::YUNET.sha256);
        // 体积来自 metadata（证明读的是文件系统元数据，不是内容）。
        assert_eq!(entry.size_on_disk_bytes, builtin::YUNET.size_bytes);

        // 对照：真正走完整校验时必须发现篡改，证明这份副本确实"坏"了。
        let health = verify::check_at(&builtin::YUNET, &tampered, false);
        assert_eq!(health.blocking.unwrap().kind(), "corrupt");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn mark_ready_and_failure_round_trip() {
        let registry = Registry::new(None);
        registry.mark_ready(builtin::YUNET.id, PathBuf::from("/x"));
        let mut dirs = locate::dev_base_dirs();
        dirs.push(PathBuf::from("/definitely/not/here"));
        let snapshot = registry.snapshot(&dirs);
        let entry = snapshot.find(builtin::YUNET.id).unwrap();
        assert_eq!(entry.state, "ready");
        assert!(entry.loaded);

        registry.mark_failure(
            builtin::YUNET.id,
            ModelFailure::LoadFailed {
                model_id: builtin::YUNET.id.into(),
                detail: "parse".into(),
            },
        );
        let snapshot = registry.snapshot(&dirs);
        assert_eq!(snapshot.find(builtin::YUNET.id).unwrap().state, "loadFailed");
    }

    #[test]
    fn user_models_are_read_from_manifests_only() {
        let root = std::env::temp_dir().join(format!(
            "lumi-registry-user-{}",
            std::process::id()
        ));
        let directory = root.join("user-kpt-1");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("model.onnx"), b"not-a-real-model").unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "user-kpt-1",
            "name": "用户关键点模型",
            "version": "1.0",
            "author": "user",
            "description": "",
            "task": "faceLandmark",
            "topologyId": "face-68",
            "input": {
                "width": 256,
                "height": 256,
                "layout": "nchw",
                "normalization": "zeroToOne",
                "cropMargin": 1.2
            },
            "output": { "layout": "n2", "coordinate": "normalizedCrop" },
            "license": {
                "name": "UNKNOWN",
                "url": null,
                "commercialUseClaimed": true,
                "redistributionClaimed": false,
                "note": "用户自备"
            },
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "fileName": "model.onnx",
            "sourceUrl": null,
            "sourcePath": "/tmp/model.onnx",
            "importedAt": "unix:1",
            "validation": null
        });
        std::fs::write(
            directory.join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        let registry = Registry::new(Some(root.clone()));
        let models = registry.user_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "user-kpt-1");
        assert!(models[0].present);
        assert!(!models[0].validation_ok);

        let snapshot = registry.snapshot(&[PathBuf::from("/nope")]);
        let entry = snapshot.find("user-kpt-1").unwrap();
        assert!(!entry.official);
        assert_eq!(snapshot.summary.user_imported, 1);
        std::fs::remove_dir_all(&root).ok();
    }
}
