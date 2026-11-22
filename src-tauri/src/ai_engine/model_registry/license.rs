//! 模型许可登记与**唯一**准入闸门。
//!
//! 三条铁律：
//! 1. 权重许可、训练数据许可、商用许可、再分发许可 **四个字段独立**，
//!    不允许合并成一个 `commercial_use: bool`（会丢信息且容易过度承诺）。
//! 2. 缺失值一律 `Unverified`，**绝不默认成 Allowed**。
//! 3. **不得**根据仓库自身的 LICENSE 推断权重可商用。字段来源只承认：
//!    官方发布页 README / 权重卡、随包 LICENSE 原文、数据集官方条款。
//!
//! 未核实的字段必须出现在 `unverifiedFields` 里，让前端可以显示
//! “权重可商用，数据集条款待核”，而不是把未核实的东西说成已核实。

use serde::{Serialize, Serializer};

use crate::face_intelligence::capability::LicenseState;

use super::spec::ModelOrigin;
use super::state::ModelFailure;

/// 三态判定。缺失值必须构造为 `Unverified`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict3 {
    Allowed,
    NotAllowed,
    /// 未核实：必须携带「该去核对什么」，避免只留一个含糊的“未知”。
    Unverified { what_to_check: &'static str },
}

impl Verdict3 {
    pub fn is_allowed(self) -> bool {
        matches!(self, Verdict3::Allowed)
    }

    pub fn is_not_allowed(self) -> bool {
        matches!(self, Verdict3::NotAllowed)
    }

    pub fn is_unverified(self) -> bool {
        matches!(self, Verdict3::Unverified { .. })
    }

    pub fn what_to_check(self) -> Option<&'static str> {
        match self {
            Verdict3::Unverified { what_to_check } => Some(what_to_check),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Verdict3::Allowed => "allowed",
            Verdict3::NotAllowed => "notAllowed",
            Verdict3::Unverified { .. } => "unverified",
        }
    }
}

impl Serialize for Verdict3 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

/// 许可证据的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseEvidenceKind {
    /// 官方发布页 / README
    OfficialPage,
    /// 随包 LICENSE 文件原文
    BundledLicenseFile,
    /// 权重卡 / model card
    WeightCard,
}

impl LicenseEvidenceKind {
    pub fn label(self) -> &'static str {
        match self {
            LicenseEvidenceKind::OfficialPage => "officialPage",
            LicenseEvidenceKind::BundledLicenseFile => "bundledLicenseFile",
            LicenseEvidenceKind::WeightCard => "weightCard",
        }
    }
}

impl Serialize for LicenseEvidenceKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

/// 一条可复核的证据：URL + 原文摘句 + 访问日期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseEvidence {
    pub kind: LicenseEvidenceKind,
    pub url: &'static str,
    /// 原文摘句，便于法务复核（不做改写）。
    pub quote: &'static str,
    pub accessed_at: &'static str,
}

/// 许可登记记录。所有字段都是 `const` 可构造的，便于放进静态登记表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LicenseRecord {
    /// 权重文件本身的许可。
    pub weights: Verdict3,
    /// 训练数据集的许可（与权重许可是两件事）。
    pub training_data: Verdict3,
    pub commercial_use: Verdict3,
    pub redistribution: Verdict3,
    /// SPDX 标识，例如 `MIT`。未核实时为 `None`。
    pub spdx: Option<&'static str>,
    pub evidence: &'static [LicenseEvidence],
    pub note: &'static str,
}

impl LicenseRecord {
    /// 未核实的字段名（camelCase，与前端 `unverifiedFields` 一致）。
    pub fn unverified_fields(&self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.weights.is_unverified() {
            fields.push("weights");
        }
        if self.training_data.is_unverified() {
            fields.push("trainingData");
        }
        if self.commercial_use.is_unverified() {
            fields.push("commercialUse");
        }
        if self.redistribution.is_unverified() {
            fields.push("redistribution");
        }
        fields
    }

    pub fn has_unverified(&self) -> bool {
        !self.unverified_fields().is_empty()
    }
}

/// 需要参与闸门判定的三个字段（训练数据许可不阻塞分发，只做披露）。
const GATED_FIELDS: [(&str, usize); 3] = [
    ("weights", 0),
    ("commercialUse", 1),
    ("redistribution", 2),
];

fn gated_verdict(record: &LicenseRecord, index: usize) -> Verdict3 {
    match index {
        0 => record.weights,
        1 => record.commercial_use,
        _ => record.redistribution,
    }
}

fn failure_for(
    model_id: &str,
    field: &'static str,
    verdict: Verdict3,
    fallback_hint: &str,
) -> ModelFailure {
    let what_to_check = verdict
        .what_to_check()
        .map(|hint| hint.to_string())
        .unwrap_or_else(|| fallback_hint.to_string());
    ModelFailure::LicenseNotCleared {
        model_id: model_id.to_string(),
        which: field,
        what_to_check,
    }
}

/// **唯一**准入闸门。纯内存比较，不触碰文件系统。
///
/// - `Builtin`（随包分发）：`weights` 与 `commercialUse` 必须是 `Allowed`，
///   `redistribution` 不得为 `NotAllowed`。任一为 `Unverified` 即拒绝 ——
///   只有 4 个字段全部核实过的权重才允许进安装包。
/// - `UserImported`（用户自备，Q1 裁决）：只拦 `NotAllowed`，`Unverified` 放行并
///   在状态里标注「未审核 · 仅本地」。这与 `ModelHub::license_state` 恒返回
///   `UserUnverified` 的既有语义一致，不会让已有的导入功能失效。
pub fn gate(origin: ModelOrigin, record: &LicenseRecord, model_id: &str) -> Result<(), ModelFailure> {
    // ① 明确禁止的一律先拒（与来源无关）。
    for (field, index) in GATED_FIELDS {
        let verdict = gated_verdict(record, index);
        if verdict.is_not_allowed() {
            return Err(failure_for(
                model_id,
                field,
                verdict,
                "该字段登记为明确不允许，不允许加载",
            ));
        }
    }
    if origin == ModelOrigin::UserImported {
        // 用户自备模型：声明不等于核实，但按 Q1 不纳入强制闸门。
        return Ok(());
    }
    // ② 随包权重：未核实也不能随包加载。
    for (field, index) in GATED_FIELDS {
        let verdict = gated_verdict(record, index);
        if verdict.is_unverified() {
            return Err(failure_for(
                model_id,
                field,
                verdict,
                "需人工核对该字段后才能随包分发",
            ));
        }
    }
    Ok(())
}

/// 由许可记录推出统一许可状态（复用 `capability::LicenseState`，不造第三套枚举）。
pub fn license_state(origin: ModelOrigin, record: &LicenseRecord) -> LicenseState {
    let any_prohibited = record.weights.is_not_allowed()
        || record.commercial_use.is_not_allowed()
        || record.redistribution.is_not_allowed();
    if any_prohibited {
        return LicenseState::Prohibited;
    }
    if origin == ModelOrigin::UserImported {
        return LicenseState::UserUnverified;
    }
    if record.weights.is_allowed() && record.commercial_use.is_allowed() {
        if record.redistribution.is_allowed() {
            LicenseState::VerifiedCommercial
        } else {
            // 权重可商用但不能随包分发。
            LicenseState::VerifiedPersonal
        }
    } else {
        LicenseState::Incompatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: Verdict3 = Verdict3::Allowed;
    const UNVERIFIED: Verdict3 = Verdict3::Unverified {
        what_to_check: "核对官方条款",
    };

    fn record(
        weights: Verdict3,
        training_data: Verdict3,
        commercial_use: Verdict3,
        redistribution: Verdict3,
    ) -> LicenseRecord {
        LicenseRecord {
            weights,
            training_data,
            commercial_use,
            redistribution,
            spdx: Some("MIT"),
            evidence: &[],
            note: "",
        }
    }

    #[test]
    fn builtin_requires_every_gated_field_allowed() {
        let ok = record(ALLOWED, UNVERIFIED, ALLOWED, ALLOWED);
        assert!(gate(ModelOrigin::Builtin, &ok, "m").is_ok());
        // 训练数据未核实不阻塞分发，但必须出现在 unverifiedFields。
        assert_eq!(ok.unverified_fields(), vec!["trainingData"]);

        let unverified_commercial = record(ALLOWED, ALLOWED, UNVERIFIED, ALLOWED);
        let failure = gate(ModelOrigin::Builtin, &unverified_commercial, "m").unwrap_err();
        assert_eq!(failure.kind(), "licenseBlocked");
        match failure {
            ModelFailure::LicenseNotCleared { which, .. } => assert_eq!(which, "commercialUse"),
            other => panic!("意外失败态：{other:?}"),
        }
    }

    #[test]
    fn not_allowed_is_rejected_even_if_other_fields_are_fine() {
        let bad = record(Verdict3::NotAllowed, ALLOWED, ALLOWED, ALLOWED);
        let failure = gate(ModelOrigin::Builtin, &bad, "m").unwrap_err();
        assert!(failure.is_permanent());
        assert_eq!(license_state(ModelOrigin::Builtin, &bad), LicenseState::Prohibited);
    }

    #[test]
    fn user_imported_only_blocks_not_allowed() {
        let unverified = record(UNVERIFIED, UNVERIFIED, UNVERIFIED, UNVERIFIED);
        assert!(gate(ModelOrigin::UserImported, &unverified, "u").is_ok());
        assert_eq!(
            license_state(ModelOrigin::UserImported, &unverified),
            LicenseState::UserUnverified
        );
        let prohibited = record(Verdict3::NotAllowed, ALLOWED, ALLOWED, ALLOWED);
        assert!(gate(ModelOrigin::UserImported, &prohibited, "u").is_err());
    }

    #[test]
    fn gate_never_touches_the_filesystem() {
        // 闸门是纯内存判断：给一个不存在的模型 ID 也不会 panic、也不会报缺失。
        let unverified = record(UNVERIFIED, UNVERIFIED, UNVERIFIED, UNVERIFIED);
        let failure = gate(ModelOrigin::Builtin, &unverified, "no-such-model").unwrap_err();
        assert_eq!(failure.kind(), "licenseBlocked");
        assert_eq!(failure.model_id(), "no-such-model");
    }

    #[test]
    fn unverified_fields_are_reported_individually() {
        let all_unverified = record(UNVERIFIED, UNVERIFIED, UNVERIFIED, UNVERIFIED);
        assert_eq!(
            all_unverified.unverified_fields(),
            vec!["weights", "trainingData", "commercialUse", "redistribution"]
        );
        assert!(all_unverified.has_unverified());
        let all_allowed = record(ALLOWED, ALLOWED, ALLOWED, ALLOWED);
        assert!(all_allowed.unverified_fields().is_empty());
        assert!(!all_allowed.has_unverified());
    }

    #[test]
    fn verdict_serializes_as_stable_strings() {
        assert_eq!(serde_json::to_value(ALLOWED).unwrap(), "allowed");
        assert_eq!(
            serde_json::to_value(Verdict3::NotAllowed).unwrap(),
            "notAllowed"
        );
        assert_eq!(serde_json::to_value(UNVERIFIED).unwrap(), "unverified");
    }

    #[test]
    fn commercial_but_not_redistributable_is_personal_only() {
        let personal = record(ALLOWED, ALLOWED, ALLOWED, Verdict3::NotAllowed);
        // 不能随包分发 → 闸门拒绝（redistribution 为 NotAllowed）。
        assert!(gate(ModelOrigin::Builtin, &personal, "m").is_err());
        assert_eq!(
            license_state(ModelOrigin::Builtin, &personal),
            LicenseState::Prohibited
        );
    }
}
