//! Finance Protocol 2.0 contracts and deterministic review logic.
//!
//! This module is deliberately additive. Generic Protocol 1.0 artifacts and
//! writers remain unchanged; finance callers must select `FinanceWritable`
//! explicitly and use the dedicated contract families defined here.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Component;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine as _;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const FINANCE_PROTOCOL_VERSION: &str = "2.0";
pub const FINANCE_POLICY_VERSION: &str = "3.0";
pub const FINANCE_MANIFEST_VERSION: &str = "3.0";
pub const FINANCE_PACKET_VERSION: &str = "2.0";
pub const FINANCE_RESULT_SCHEMA_ID: &str =
    "https://github.com/eric-stone-plus/QUINTE/contracts/finance-review-result/1.0/schema.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractMode {
    GenericWritable,
    FinanceWritable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SchoolId {
    FactorRiskModel,
    EventDriven,
    FundamentalSupplyChain,
    TrendTechnicalRegime,
    MarketMicrostructure,
}

pub const SCHOOL_BINDINGS: [(&str, SchoolId); 5] = [
    ("Party A", SchoolId::FactorRiskModel),
    ("Party B", SchoolId::EventDriven),
    ("Party C", SchoolId::FundamentalSupplyChain),
    ("Party D", SchoolId::TrendTechnicalRegime),
    ("Party E", SchoolId::MarketMicrostructure),
];

pub fn school_for_party(party: &str) -> Option<SchoolId> {
    SCHOOL_BINDINGS
        .iter()
        .find_map(|(candidate, school)| (*candidate == party).then_some(*school))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableBinding {
    pub artifact_ref: String,
    pub contract_family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<String>,
    pub revision: String,
    pub exact_sha256: String,
    pub semantic_domain: String,
    pub semantic_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchoolAuthority {
    pub party_id: String,
    pub school_id: SchoolId,
    pub accepted_evidence_classes: Vec<String>,
    pub forbidden_claim_classes: Vec<String>,
    pub question_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceSchoolRoute {
    pub party_id: String,
    pub school_id: SchoolId,
    pub route_id: String,
    pub family: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceArbiterRoute {
    pub arbiter_role: String,
    pub route_id: String,
    pub family: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinancePolicy {
    pub policy_version: String,
    pub protocol_version: String,
    pub profile: PortableBinding,
    pub school_bindings: Vec<FinanceSchoolRoute>,
    pub counterpart_arbiter: FinanceArbiterRoute,
    pub primary_arbiter: FinanceArbiterRoute,
    pub isolation_backend: String,
    pub same_family_required: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceReviewProfile {
    pub finance_review_profile_version: String,
    pub profile_id: String,
    pub schools: Vec<SchoolAuthority>,
    pub allowed_primary_contracts: Vec<String>,
    pub applicability_predicate_codes: Vec<String>,
    pub closure_rule_codes: Vec<String>,
    pub hash_domains: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicabilityMode {
    Mandatory,
    Conditional,
    OutOfScope,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreregisteredApplicability {
    pub mode: ApplicabilityMode,
    pub predicate_code: Option<String>,
    pub predicate_inputs: BTreeMap<String, String>,
    pub predicate_result: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceClaim {
    pub claim_id: String,
    pub text: String,
    pub claim_classes: Vec<String>,
    pub school_applicability: BTreeMap<SchoolId, PreregisteredApplicability>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceClaimManifest {
    pub finance_claim_manifest_version: String,
    pub claims: Vec<FinanceClaim>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Accepted,
    DescriptiveOnly,
    Quarantined,
    Expired,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceEvidenceItem {
    pub evidence_ref: String,
    pub evidence_class: String,
    pub binding: PortableBinding,
    pub status: EvidenceStatus,
    pub provenance_complete: bool,
    pub available_at: String,
    pub expiry_session: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceEvidenceIndex {
    pub finance_evidence_index_version: String,
    pub as_of: String,
    pub evaluation_session: String,
    pub items: Vec<FinanceEvidenceItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PrimaryAuthority {
    pub binding: PortableBinding,
    pub status: EvidenceStatus,
    pub provenance_complete: bool,
    pub evaluation_session: String,
    pub expiry_session: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceReviewInvocation {
    pub finance_review_invocation_version: String,
    pub invocation_id: String,
    pub profile: PortableBinding,
    pub claim_manifest: PortableBinding,
    pub primary: PrimaryAuthority,
    pub evidence_index: PortableBinding,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum FinancePhase {
    R1,
    R2,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SchoolDisposition {
    Clear,
    NotApplicable,
    InsufficientEvidence,
    Contradicted,
    Quarantined,
    Expired,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Materiality {
    NonMaterial,
    Material,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinanceClosureState {
    Open,
    Closed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceResidual {
    pub residual_code: String,
    pub affected_claim_ids: Vec<String>,
    pub materiality: Materiality,
    pub closure_state: FinanceClosureState,
    pub closure_rule_code: Option<String>,
    pub closure_evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchoolClaimDecision {
    pub claim_id: String,
    pub disposition: SchoolDisposition,
    pub evidence_refs: Vec<String>,
    pub alternative_codes: Vec<String>,
    pub confounder_codes: Vec<String>,
    pub falsifier_codes: Vec<String>,
    pub invalidation_codes: Vec<String>,
    pub limitation_codes: Vec<String>,
    pub missing_evidence_codes: Vec<String>,
    pub closure_rule_code: Option<String>,
    pub closure_evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchoolLaneOutput {
    pub school_lane_output_version: String,
    pub run_id: String,
    pub phase: FinancePhase,
    pub school_id: SchoolId,
    pub expected_route_digest: String,
    pub profile_digest: String,
    pub primary_digest: String,
    pub evidence_index_digest: String,
    pub input_packet_exact_sha256: String,
    pub input_packet_semantic_sha256: String,
    pub decisions: Vec<SchoolClaimDecision>,
    pub residuals: Vec<FinanceResidual>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceTaskPacket {
    pub task_packet_version: String,
    pub run_id: String,
    pub phase: FinancePhase,
    pub invocation: PortableBinding,
    pub policy: PortableBinding,
    pub profile: PortableBinding,
    pub claim_manifest: PortableBinding,
    pub primary: PortableBinding,
    pub evidence_index: PortableBinding,
    pub recipient_route_digest: String,
    pub authority: SchoolAuthority,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnonymousDecision {
    pub claim_id: String,
    pub disposition: SchoolDisposition,
    pub evidence_refs: Vec<String>,
    pub residual_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnonymousR1Contribution {
    pub contributor_alias: String,
    pub decisions: Vec<AnonymousDecision>,
}

#[derive(Clone, Debug, Serialize)]
struct UnaliasedR1Contribution {
    decisions: Vec<AnonymousDecision>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct R1SourceIdentity {
    exact_sha256: String,
    semantic_sha256: String,
}

#[derive(Serialize)]
struct R1SourceSet<'a> {
    outputs: &'a [R1SourceIdentity],
}

#[derive(Serialize)]
struct BoundAnonymousCorpus<'a> {
    r1_source_set_semantic_sha256: &'a str,
    corpus: &'a AnonymousR1Corpus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnonymousR1Corpus {
    pub claims: Vec<(String, String)>,
    pub contributions: Vec<AnonymousR1Contribution>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceR2Packet {
    pub packet_version: String,
    pub run_id: String,
    pub recipient_authority: SchoolAuthority,
    pub policy: PortableBinding,
    pub profile: PortableBinding,
    pub claim_manifest: PortableBinding,
    pub primary: PortableBinding,
    pub evidence_index: PortableBinding,
    pub recipient_route_digest: String,
    pub r1_source_set_semantic_sha256: String,
    pub corpus_semantic_sha256: String,
    pub corpus: AnonymousR1Corpus,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FoldedSchoolDecision {
    pub claim_id: String,
    pub school_id: SchoolId,
    pub applicability: ApplicabilityMode,
    pub disposition: SchoolDisposition,
    pub blocking: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PublicationPosture {
    #[serde(rename = "PUBLISH_BOUNDED")]
    PublishBounded,
    #[serde(rename = "ABSTAIN")]
    Abstain,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PostureReason {
    PrimaryNotAccepted,
    PrimaryExpired,
    PrimaryProvenanceIncomplete,
    EvidenceProvenanceIncomplete,
    MandatorySchoolNotClear,
    ConditionalSchoolNotClear,
    InvalidNotApplicable,
    OpenMaterialResidual,
    ActiveInvalidation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicationDecision {
    pub function_revision: String,
    pub posture: PublicationPosture,
    pub reason_codes: Vec<PostureReason>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceReviewResult {
    pub finance_review_result_version: String,
    pub run_id: String,
    pub run_genesis_digest: String,
    pub primary: PrimaryAuthority,
    pub profile: PortableBinding,
    pub claim_manifest: PortableBinding,
    pub evidence_index: PortableBinding,
    pub school_outputs: Vec<PortableBinding>,
    pub arbiter_outputs: Vec<PortableBinding>,
    pub folded_decisions: Vec<FoldedSchoolDecision>,
    pub open_material_residual_codes: Vec<String>,
    pub active_invalidation_codes: Vec<String>,
    pub publication: PublicationDecision,
    pub route_bindings: Vec<String>,
    pub contamination_risks: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HighballRouteRequest {
    pub carrier_version: String,
    pub source_result: PortableBinding,
    pub requested_route: String,
    pub publication_posture: PublicationPosture,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HighballResidualTrace {
    pub carrier_version: String,
    pub source_result: PortableBinding,
    pub residual_codes: Vec<String>,
    pub invalidation_codes: Vec<String>,
    pub folded_decisions: Vec<FoldedSchoolDecision>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinanceRunStatus {
    Preflight,
    R1Running,
    R2Running,
    Merging,
    Completed,
    Degraded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinanceTerminationCode {
    OutputInvalid,
    IntegrityFailure,
    OperatorCancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceTerminationFacts {
    pub phase: FinanceRunStatus,
    pub code: FinanceTerminationCode,
    pub retryable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceRunManifest {
    pub manifest_version: String,
    pub protocol_version: String,
    pub run_id: String,
    pub run_genesis_digest: String,
    pub event_checkpoint: FinanceEventCheckpoint,
    pub status: FinanceRunStatus,
    pub termination: Option<FinanceTerminationFacts>,
    pub policy: PortableBinding,
    pub invocation: PortableBinding,
    pub r1_packets: Vec<PortableBinding>,
    pub r2_packets: Vec<PortableBinding>,
    pub r1_outputs: Vec<PortableBinding>,
    pub r2_outputs: Vec<PortableBinding>,
    pub arbiter_outputs: Vec<PortableBinding>,
    pub result: Option<PortableBinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceEventCheckpoint {
    pub sequence: u64,
    pub event_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceRunCreatedPayload {
    pub status: FinanceRunStatus,
    pub artifact_bindings: Vec<FinanceArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinancePhaseAdvancedPayload {
    pub previous_status: FinanceRunStatus,
    pub status: FinanceRunStatus,
    pub artifact_bindings: Vec<FinanceArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceRunTerminalizedPayload {
    pub previous_status: FinanceRunStatus,
    pub status: FinanceRunStatus,
    pub artifact_bindings: Vec<FinanceArtifactIdentity>,
    pub result: Option<FinanceArtifactIdentity>,
    pub termination: Option<FinanceTerminationFacts>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceArtifactIdentity {
    pub contract_family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<String>,
    pub revision: String,
    pub exact_sha256: String,
    pub semantic_domain: String,
    pub semantic_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event_kind", content = "payload")]
pub enum FinanceRunEventBody {
    #[serde(rename = "run.created")]
    RunCreated(FinanceRunCreatedPayload),
    #[serde(rename = "run.phase_advanced")]
    PhaseAdvanced(FinancePhaseAdvancedPayload),
    #[serde(rename = "run.terminalized")]
    RunTerminalized(FinanceRunTerminalizedPayload),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceRunEvent {
    pub event_version: String,
    pub run_id: String,
    pub run_genesis_digest: String,
    pub sequence: u64,
    pub previous_event_sha256: Option<String>,
    #[serde(flatten)]
    pub body: FinanceRunEventBody,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FinanceArbiterVerdict {
    pub finance_arbiter_verdict_version: String,
    pub run_id: String,
    pub arbiter_role: String,
    pub policy_digest: String,
    pub invocation_digest: String,
    pub profile_digest: String,
    pub claim_manifest_digest: String,
    pub primary_digest: String,
    pub evidence_index_digest: String,
    pub school_output_digests: Vec<String>,
    pub route_binding_digests: Vec<String>,
    pub duplicate_residual_groups: Vec<Vec<String>>,
    pub identifier_reconciliations: BTreeMap<String, String>,
    pub scope_reconciliations: BTreeMap<String, String>,
    pub admitted_closure_evidence_refs: Vec<String>,
}

fn validate_lane_claims_and_evidence(
    profile: &FinanceReviewProfile,
    claims: &FinanceClaimManifest,
    evidence: &FinanceEvidenceIndex,
    outputs: &[SchoolLaneOutput],
) -> anyhow::Result<()> {
    let expected = claims
        .claims
        .iter()
        .map(|claim| claim.claim_id.as_str())
        .collect::<BTreeSet<_>>();
    let admitted = evidence
        .items
        .iter()
        .filter(|item| item.status == EvidenceStatus::Accepted)
        .map(|item| item.evidence_ref.as_str())
        .collect::<BTreeSet<_>>();
    for output in outputs {
        let authority = profile
            .schools
            .iter()
            .find(|authority| authority.school_id == output.school_id)
            .context("school output has no profile authority")?;
        let actual = output
            .decisions
            .iter()
            .map(|decision| decision.claim_id.as_str())
            .collect::<BTreeSet<_>>();
        if actual.len() != output.decisions.len() || actual != expected {
            bail!("school decision set must equal preregistered claims exactly once");
        }
        for decision in &output.decisions {
            for values in [
                &decision.evidence_refs,
                &decision.alternative_codes,
                &decision.confounder_codes,
                &decision.falsifier_codes,
                &decision.invalidation_codes,
                &decision.limitation_codes,
                &decision.missing_evidence_codes,
                &decision.closure_evidence_refs,
            ] {
                if !values.windows(2).all(|pair| pair[0] < pair[1]) {
                    bail!("school decision set-valued fields must be sorted and duplicate-free");
                }
            }
        }
        if !output.residuals.windows(2).all(|pair| {
            (
                pair[0].residual_code.as_str(),
                pair[0].affected_claim_ids.as_slice(),
            ) < (
                pair[1].residual_code.as_str(),
                pair[1].affected_claim_ids.as_slice(),
            )
        }) {
            bail!("school residuals must be sorted and duplicate-free");
        }
        for decision in &output.decisions {
            for reference in decision
                .evidence_refs
                .iter()
                .chain(&decision.closure_evidence_refs)
            {
                if !admitted.contains(reference.as_str()) {
                    bail!("school output cites evidence absent from the pinned evidence index");
                }
                let item = evidence
                    .items
                    .iter()
                    .find(|item| item.evidence_ref == *reference)
                    .unwrap();
                if !authority
                    .accepted_evidence_classes
                    .contains(&item.evidence_class)
                {
                    bail!("school output cites an evidence class outside its profile authority");
                }
            }
            match &decision.closure_rule_code {
                Some(rule) if !profile.closure_rule_codes.contains(rule) => {
                    bail!("school decision uses an unregistered closure rule");
                }
                Some(_) if decision.closure_evidence_refs.is_empty() => {
                    bail!("school decision closure requires admitted evidence");
                }
                None if !decision.closure_evidence_refs.is_empty() => {
                    bail!("school decision closure evidence requires a registered rule");
                }
                _ => {}
            }
        }
        for residual in &output.residuals {
            if residual
                .affected_claim_ids
                .iter()
                .any(|claim| !expected.contains(claim.as_str()))
                || residual
                    .closure_evidence_refs
                    .iter()
                    .any(|reference| !admitted.contains(reference.as_str()))
            {
                bail!("school residual cites an unregistered claim or evidence item");
            }
            if residual
                .closure_rule_code
                .as_ref()
                .is_some_and(|rule| !profile.closure_rule_codes.contains(rule))
            {
                bail!("school residual uses an unregistered closure rule");
            }
            for reference in &residual.closure_evidence_refs {
                let item = evidence
                    .items
                    .iter()
                    .find(|item| item.evidence_ref == *reference)
                    .context("school residual cites unknown closure evidence")?;
                if !authority
                    .accepted_evidence_classes
                    .contains(&item.evidence_class)
                {
                    bail!("school residual cites an evidence class outside its profile authority");
                }
            }
            match residual.closure_state {
                FinanceClosureState::Closed
                    if residual.closure_rule_code.is_none()
                        || residual.closure_evidence_refs.is_empty() =>
                {
                    bail!(
                        "closed school residual requires a registered rule and admitted evidence"
                    );
                }
                FinanceClosureState::Open
                    if residual.closure_rule_code.is_some()
                        || !residual.closure_evidence_refs.is_empty() =>
                {
                    bail!("open school residual cannot self-report closure authority");
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedFinanceBundle {
    pub result_path: PathBuf,
    pub manifest_path: PathBuf,
    pub highball_route_request_path: PathBuf,
    pub highball_residual_trace_path: PathBuf,
    pub r1_packet_paths: Vec<PathBuf>,
    pub r2_packet_paths: Vec<PathBuf>,
    pub publication_posture: PublicationPosture,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FinanceVerification {
    pub verification_version: &'static str,
    pub run_id: String,
    pub manifest_version: String,
    pub result_contract: &'static str,
    pub publication_posture: PublicationPosture,
    pub result_exact_sha256: String,
    pub result_semantic_sha256: String,
    pub highball_carriers_verified: bool,
    pub finance_creation_enabled: bool,
}

pub const DORMANT_WRITER_ACK: &str = "I_UNDERSTAND_FINANCE_CREATION_IS_DORMANT";

fn require_dormant_writer_ack(acknowledgement: &str) -> anyhow::Result<()> {
    if acknowledgement != DORMANT_WRITER_ACK {
        bail!("dormant finance writer requires exact acknowledgement {DORMANT_WRITER_ACK}");
    }
    Ok(())
}

fn copy_exact(source: &Path, target: &Path) -> anyhow::Result<()> {
    let bytes = std::fs::read(source)?;
    if target.exists() {
        if std::fs::read(target)? != bytes {
            bail!(
                "immutable finance input differs on replay: {}",
                target.display()
            );
        }
        return Ok(());
    }
    crate::util::atomic_write(target, &bytes)
}

fn write_idempotent(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if path.exists() {
        if std::fs::read(path)? != bytes {
            bail!(
                "finance replay would rewrite different bytes: {}",
                path.display()
            );
        }
        return Ok(());
    }
    crate::util::atomic_write(path, bytes)
}

fn lifecycle_lock(state: &Path) -> anyhow::Result<std::fs::File> {
    let parent = state.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = state
        .file_name()
        .and_then(|name| name.to_str())
        .context("finance state path has no filename")?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(parent.join(format!(".{name}.finance.lock")))?;
    file.try_lock_exclusive()
        .context("another dormant finance lifecycle operation is active")?;
    Ok(file)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> anyhow::Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FlushFileBuffers, OPEN_EXISTING,
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error()).context("cannot open directory for sync");
    }
    let flushed = unsafe { FlushFileBuffers(handle) };
    unsafe { CloseHandle(handle) };
    if flushed == 0 {
        return Err(std::io::Error::last_os_error()).context("cannot sync directory");
    }
    Ok(())
}

fn atomic_write_durable(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    crate::util::atomic_write(path, bytes)?;
    sync_directory(path.parent().context("durable file has no parent")?)
}

fn remove_file_durable(path: &Path) -> anyhow::Result<()> {
    std::fs::remove_file(path)?;
    sync_directory(path.parent().context("durable file has no parent")?)
}

fn legal_transition(old: Option<FinanceRunStatus>, new: FinanceRunStatus) -> bool {
    matches!(
        (old, new),
        (None, FinanceRunStatus::R1Running)
            | (
                Some(FinanceRunStatus::R1Running),
                FinanceRunStatus::R2Running
            )
            | (
                Some(FinanceRunStatus::R1Running | FinanceRunStatus::R2Running),
                FinanceRunStatus::Failed | FinanceRunStatus::Cancelled
            )
            | (Some(FinanceRunStatus::R2Running), FinanceRunStatus::Merging)
            | (
                Some(FinanceRunStatus::Merging),
                FinanceRunStatus::Completed
                    | FinanceRunStatus::Degraded
                    | FinanceRunStatus::Failed
                    | FinanceRunStatus::Cancelled
            )
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingFinanceTransition {
    pending_transition_version: String,
    run_id: String,
    run_genesis_digest: String,
    transition_id: String,
    operation: String,
    old_manifest_exact_sha256: Option<String>,
    old_event_checkpoint: Option<FinanceEventCheckpoint>,
    event_exact_sha256: String,
    event_byte_length: u64,
    event_bytes_base64: String,
    target_manifest_exact_sha256: String,
    target_manifest_byte_length: u64,
    target_manifest_bytes_base64: String,
    artifact_bindings: Vec<PortableBinding>,
}

#[derive(Serialize)]
struct FinanceRunGenesis<'a> {
    run_id: &'a str,
    writer_capability: &'static str,
    policy: FinanceArtifactIdentity,
    invocation: FinanceArtifactIdentity,
    profile: FinanceArtifactIdentity,
    claim_manifest: FinanceArtifactIdentity,
    primary: FinanceArtifactIdentity,
    evidence_index: FinanceArtifactIdentity,
    school_routes: &'a [FinanceSchoolRoute],
    counterpart_arbiter: &'a FinanceArbiterRoute,
    primary_arbiter: &'a FinanceArbiterRoute,
    r1_packets: Vec<FinanceArtifactIdentity>,
}

const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;

fn artifact_identity(binding: &PortableBinding) -> FinanceArtifactIdentity {
    FinanceArtifactIdentity {
        contract_family: binding.contract_family.clone(),
        schema_id: binding.schema_id.clone(),
        revision: binding.revision.clone(),
        exact_sha256: binding.exact_sha256.clone(),
        semantic_domain: binding.semantic_domain.clone(),
        semantic_sha256: binding.semantic_sha256.clone(),
    }
}

fn manifest_artifact_identities(manifest: &FinanceRunManifest) -> Vec<FinanceArtifactIdentity> {
    manifest_artifact_bindings(manifest)
        .iter()
        .map(|binding| artifact_identity(binding))
        .collect()
}

fn compute_run_genesis(
    run_id: &str,
    policy: &FinancePolicy,
    invocation: &FinanceReviewInvocation,
    policy_binding: &PortableBinding,
    invocation_binding: &PortableBinding,
    r1_packets: &[PortableBinding],
) -> anyhow::Result<String> {
    if r1_packets.len() != SCHOOL_BINDINGS.len() {
        bail!("finance run genesis requires exactly five R1 packets");
    }
    let normalize = |binding: &PortableBinding| {
        let mut binding = binding.clone();
        if let Some(relative) = binding.artifact_ref.strip_prefix("terminal/") {
            binding.artifact_ref = relative.into();
        }
        artifact_identity(&binding)
    };
    semantic_digest_value(
        "quinte.finance-run-genesis.v1",
        &FinanceRunGenesis {
            run_id,
            writer_capability: "FinanceWritable/2.0",
            policy: normalize(policy_binding),
            invocation: normalize(invocation_binding),
            profile: artifact_identity(&invocation.profile),
            claim_manifest: artifact_identity(&invocation.claim_manifest),
            primary: artifact_identity(&invocation.primary.binding),
            evidence_index: artifact_identity(&invocation.evidence_index),
            school_routes: &policy.school_bindings,
            counterpart_arbiter: &policy.counterpart_arbiter,
            primary_arbiter: &policy.primary_arbiter,
            r1_packets: r1_packets.iter().map(normalize).collect(),
        },
    )
}

fn manifest_artifact_bindings(manifest: &FinanceRunManifest) -> Vec<PortableBinding> {
    std::iter::once(&manifest.policy)
        .chain(std::iter::once(&manifest.invocation))
        .chain(&manifest.r1_packets)
        .chain(&manifest.r2_packets)
        .chain(&manifest.r1_outputs)
        .chain(&manifest.r2_outputs)
        .chain(&manifest.arbiter_outputs)
        .chain(manifest.result.iter())
        .cloned()
        .collect()
}

fn expected_school_slot(prefix: &str, school: SchoolId) -> anyhow::Result<String> {
    let name = serde_json::to_value(school)?
        .as_str()
        .context("school identifier is not a JSON string")?
        .to_string();
    Ok(format!("{prefix}/{name}.json"))
}

fn verify_binding_at(
    root: &Path,
    binding: &PortableBinding,
    expected_ref: &str,
    family: &str,
    revision: &str,
    domain: &str,
) -> anyhow::Result<(Vec<u8>, Value)> {
    require_artifact_ref(binding, expected_ref)?;
    let bytes = std::fs::read(root.join(expected_ref))?;
    let value = parse_strict_json(&bytes)?;
    verify_portable_binding(binding, &bytes, &value, family, revision, domain)?;
    Ok((bytes, value))
}

fn validate_manifest_counts(manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    let counts = (
        manifest.r1_packets.len(),
        manifest.r2_packets.len(),
        manifest.r1_outputs.len(),
        manifest.r2_outputs.len(),
        manifest.arbiter_outputs.len(),
        manifest.result.is_some(),
    );
    let phase = match manifest.status {
        FinanceRunStatus::Failed | FinanceRunStatus::Cancelled => manifest
            .termination
            .as_ref()
            .map(|termination| termination.phase)
            .context("terminal finance manifest lacks typed termination facts")?,
        _ => manifest.status,
    };
    let valid = match phase {
        FinanceRunStatus::R1Running => counts == (5, 0, 0, 0, 0, false),
        FinanceRunStatus::R2Running => counts == (5, 5, 5, 0, 0, false),
        FinanceRunStatus::Merging => counts == (5, 5, 5, 5, 2, false),
        FinanceRunStatus::Completed | FinanceRunStatus::Degraded => counts == (5, 5, 5, 5, 2, true),
        _ => false,
    };
    if !valid {
        bail!("finance manifest status has an illegal artifact-count projection");
    }
    Ok(())
}

fn validate_manifest_termination(manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    match (manifest.status, manifest.termination.as_ref()) {
        (FinanceRunStatus::Failed, Some(termination)) => {
            if !matches!(
                termination.phase,
                FinanceRunStatus::R1Running
                    | FinanceRunStatus::R2Running
                    | FinanceRunStatus::Merging
            ) || termination.retryable
                || termination.code == FinanceTerminationCode::OperatorCancelled
                || manifest.result.is_some()
            {
                bail!("failed finance manifest has invalid typed termination facts");
            }
        }
        (FinanceRunStatus::Cancelled, Some(termination)) => {
            if !matches!(
                termination.phase,
                FinanceRunStatus::R1Running
                    | FinanceRunStatus::R2Running
                    | FinanceRunStatus::Merging
            ) || termination.retryable
                || termination.code != FinanceTerminationCode::OperatorCancelled
                || manifest.result.is_some()
            {
                bail!("cancelled finance manifest has invalid typed termination facts");
            }
        }
        (FinanceRunStatus::Failed | FinanceRunStatus::Cancelled, None) => {
            bail!("terminal finance manifest lacks typed termination facts");
        }
        (_, None) => {}
        (_, Some(_)) => bail!("non-failure finance manifest carries termination facts"),
    }
    Ok(())
}

fn validate_dormant_manifest_graph(
    state: &Path,
    manifest: &FinanceRunManifest,
) -> anyhow::Result<()> {
    if manifest.manifest_version != FINANCE_MANIFEST_VERSION
        || manifest.protocol_version != FINANCE_PROTOCOL_VERSION
    {
        bail!("unsupported dormant finance manifest revision");
    }
    crate::schema::validate_value(
        &serde_json::to_value(manifest)?,
        crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
    )?;
    validate_manifest_termination(manifest)?;
    validate_manifest_counts(manifest)?;
    let terminal_prefix = if manifest.policy.artifact_ref.starts_with("terminal/") {
        "terminal/"
    } else {
        ""
    };
    if !matches!(
        manifest.status,
        FinanceRunStatus::Merging | FinanceRunStatus::Completed | FinanceRunStatus::Degraded
    ) && !terminal_prefix.is_empty()
    {
        bail!("only a completed dormant manifest may use the terminal binding graph");
    }
    let expected = |relative: &str| format!("{terminal_prefix}{relative}");
    let immutable_root = state.join(terminal_prefix.strip_suffix('/').unwrap_or(""));

    let (_, policy_value) = verify_binding_at(
        state,
        &manifest.policy,
        &expected("input/policy.json"),
        "policy",
        "3.0",
        "quinte.finance-policy.v3",
    )?;
    let policy: FinancePolicy = serde_json::from_value(policy_value.clone())?;
    validate_finance_policy(&policy)?;
    let (_, invocation_value) = verify_binding_at(
        state,
        &manifest.invocation,
        &expected("input/invocation.json"),
        "finance-review-invocation",
        "1.0",
        "quinte.finance-review-invocation.v1",
    )?;
    let invocation: FinanceReviewInvocation = serde_json::from_value(invocation_value.clone())?;
    if invocation.invocation_id != manifest.run_id {
        bail!("dormant finance invocation and manifest run IDs differ");
    }
    let (profile, profile_bytes, profile_value) =
        load_typed::<FinanceReviewProfile>(&immutable_root.join("input/profile.json"))?;
    validate_profile(&profile)?;
    verify_portable_binding(
        &policy.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;
    verify_portable_binding(
        &invocation.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;
    let (claims, claims_bytes, claims_value) =
        load_typed::<FinanceClaimManifest>(&immutable_root.join("input/claim-manifest.json"))?;
    validate_claim_manifest(&profile, &claims)?;
    verify_portable_binding(
        &invocation.claim_manifest,
        &claims_bytes,
        &claims_value,
        "finance-claim-manifest",
        "1.0",
        "quinte.finance-claim-manifest.v1",
    )?;
    let (evidence, evidence_bytes, evidence_value) =
        load_typed::<FinanceEvidenceIndex>(&immutable_root.join("input/evidence-index.json"))?;
    if evidence.finance_evidence_index_version != "1.0" {
        bail!("finance_evidence_index_version must be 1.0");
    }
    verify_portable_binding(
        &invocation.evidence_index,
        &evidence_bytes,
        &evidence_value,
        "finance-evidence-index",
        "1.0",
        "quinte.finance-evidence-index.v1",
    )?;
    validate_evidence_artifacts(&immutable_root.join("input"), &evidence)?;
    let primary_bytes = std::fs::read(immutable_root.join("input/primary.json"))?;
    let primary_value = parse_strict_json(&primary_bytes)?;
    verify_portable_binding(
        &invocation.primary.binding,
        &primary_bytes,
        &primary_value,
        &invocation.primary.binding.contract_family.clone(),
        &invocation.primary.binding.revision.clone(),
        &invocation.primary.binding.semantic_domain.clone(),
    )?;

    let mut rebuilt_r1 = Vec::new();
    for ((_, school), authority) in SCHOOL_BINDINGS.iter().zip(&profile.schools) {
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == *school)
            .context("fixed school has no policy route")?;
        let packet = build_r1_packet(
            &manifest.run_id,
            PortableBinding {
                artifact_ref: "input/invocation.json".into(),
                ..manifest.invocation.clone()
            },
            PortableBinding {
                artifact_ref: "input/policy.json".into(),
                ..manifest.policy.clone()
            },
            invocation.profile.clone(),
            invocation.claim_manifest.clone(),
            invocation.primary.binding.clone(),
            invocation.evidence_index.clone(),
            semantic_digest_value("quinte.finance-route-binding.v1", route)?,
            authority,
        );
        let expected_ref = expected(&expected_school_slot("packets/r1", *school)?);
        let (bytes, value) = verify_binding_at(
            state,
            &manifest.r1_packets[rebuilt_r1.len()],
            &expected_ref,
            "task-packet",
            "2.0",
            "quinte.finance-task-packet.v2",
        )?;
        if serde_json::to_vec_pretty(&packet)? != bytes || serde_json::to_value(&packet)? != value {
            bail!("dormant R1 packet does not reconstruct from immutable inputs");
        }
        rebuilt_r1.push(packet);
    }
    let genesis = compute_run_genesis(
        &manifest.run_id,
        &policy,
        &invocation,
        &manifest.policy,
        &manifest.invocation,
        &manifest.r1_packets,
    )?;
    if manifest.run_genesis_digest != genesis {
        bail!("dormant finance run genesis does not reconstruct from immutable inputs");
    }

    let mut r1_outputs = Vec::new();
    let mut r1_bindings = Vec::new();
    let graph_phase = manifest
        .termination
        .as_ref()
        .map_or(manifest.status, |termination| termination.phase);
    if graph_phase != FinanceRunStatus::R1Running {
        for (index, (_, school)) in SCHOOL_BINDINGS.iter().enumerate() {
            let expected_ref = expected(&expected_school_slot(
                if terminal_prefix.is_empty() {
                    "outputs/r1"
                } else {
                    "r1"
                },
                *school,
            )?);
            let (bytes, value) = verify_binding_at(
                state,
                &manifest.r1_outputs[index],
                &expected_ref,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
            )?;
            let output: SchoolLaneOutput = serde_json::from_value(value)?;
            if output.school_id != *school {
                bail!("dormant R1 output slot/school mismatch");
            }
            bind_output_to_packet(&output, &rebuilt_r1[index], "quinte.finance-task-packet.v2")?;
            r1_outputs.push(output);
            r1_bindings.push(binding_for(
                expected_ref,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
                &bytes,
                &serde_json::from_slice(&bytes)?,
            )?);
        }
        validate_school_output_set(&manifest.run_id, FinancePhase::R1, &r1_outputs)?;
        validate_lane_claims_and_evidence(&profile, &claims, &evidence, &r1_outputs)?;
        if r1_bindings != manifest.r1_outputs {
            bail!("dormant R1 output binding set does not reconstruct exactly");
        }
        for (index, ((_, school), authority)) in
            SCHOOL_BINDINGS.iter().zip(&profile.schools).enumerate()
        {
            let route = policy
                .school_bindings
                .iter()
                .find(|route| route.school_id == *school)
                .unwrap();
            let packet = build_r2_packet(
                &manifest.run_id,
                authority,
                &claims.claims,
                &r1_outputs,
                &r1_bindings,
                PortableBinding {
                    artifact_ref: "input/policy.json".into(),
                    ..manifest.policy.clone()
                },
                invocation.profile.clone(),
                invocation.claim_manifest.clone(),
                invocation.primary.binding.clone(),
                invocation.evidence_index.clone(),
                semantic_digest_value("quinte.finance-route-binding.v1", route)?,
            )?;
            let expected_ref = expected(&expected_school_slot("packets/r2", *school)?);
            let (bytes, value) = verify_binding_at(
                state,
                &manifest.r2_packets[index],
                &expected_ref,
                "r2-packet",
                "2.0",
                "quinte.finance-r2-packet.v2",
            )?;
            if serde_json::to_vec_pretty(&packet)? != bytes
                || serde_json::to_value(&packet)? != value
            {
                bail!("dormant R2 packet does not reconstruct from immutable inputs and R1 set");
            }
        }
    }
    if graph_phase == FinanceRunStatus::Merging {
        let mut r2_outputs = Vec::new();
        let mut r2_bindings = Vec::new();
        for (index, (_, school)) in SCHOOL_BINDINGS.iter().enumerate() {
            let expected_ref = expected(&expected_school_slot(
                if terminal_prefix.is_empty() {
                    "outputs/r2"
                } else {
                    "r2"
                },
                *school,
            )?);
            let (bytes, value) = verify_binding_at(
                state,
                &manifest.r2_outputs[index],
                &expected_ref,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
            )?;
            let output: SchoolLaneOutput = serde_json::from_value(value)?;
            if output.school_id != *school {
                bail!("dormant R2 output slot/school mismatch");
            }
            let (packet, _, _) = load_typed::<FinanceR2Packet>(
                &state.join(&manifest.r2_packets[index].artifact_ref),
            )?;
            bind_output_to_packet(&output, &packet, "quinte.finance-r2-packet.v2")?;
            r2_outputs.push(output);
            r2_bindings.push(binding_for(
                expected_ref,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
                &bytes,
                &serde_json::from_slice(&bytes)?,
            )?);
        }
        validate_school_output_set(&manifest.run_id, FinancePhase::R2, &r2_outputs)?;
        validate_lane_claims_and_evidence(&profile, &claims, &evidence, &r2_outputs)?;
        if r2_bindings != manifest.r2_outputs {
            bail!("dormant R2 output binding set does not reconstruct exactly");
        }
        let mut school_outputs = r1_bindings.clone();
        school_outputs.extend(r2_bindings);
        let school_output_digests = school_outputs
            .iter()
            .map(|binding| binding.semantic_sha256.clone())
            .collect::<Vec<_>>();
        let route_binding_digests = policy
            .school_bindings
            .iter()
            .map(|route| semantic_digest_value("quinte.finance-route-binding.v1", route))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .chain([
                semantic_digest_value(
                    "quinte.finance-arbiter-route-binding.v1",
                    &policy.counterpart_arbiter,
                )?,
                semantic_digest_value(
                    "quinte.finance-arbiter-route-binding.v1",
                    &policy.primary_arbiter,
                )?,
            ])
            .collect::<Vec<_>>();
        let admitted_evidence_refs = evidence
            .items
            .iter()
            .filter(|item| item.status == EvidenceStatus::Accepted)
            .map(|item| item.evidence_ref.as_str())
            .collect::<BTreeSet<_>>();
        let policy_digest = semantic_digest("quinte.finance-policy.v3", &policy_value)?;
        let invocation_digest =
            semantic_digest("quinte.finance-review-invocation.v1", &invocation_value)?;
        for (index, binding) in manifest.arbiter_outputs.iter().enumerate() {
            let (role, file) = if index == 0 {
                ("counterpart_arbiter", "counterpart-arbiter.json")
            } else {
                ("primary_arbiter", "primary-arbiter.json")
            };
            let expected_ref = expected(&format!(
                "{}{file}",
                if terminal_prefix.is_empty() {
                    "outputs/arbiters/"
                } else {
                    "arbiters/"
                }
            ));
            let (_, value) = verify_binding_at(
                state,
                binding,
                &expected_ref,
                "finance-arbiter-verdict",
                "1.0",
                "quinte.finance-arbiter-verdict.v1",
            )?;
            let verdict: FinanceArbiterVerdict = serde_json::from_value(value)?;
            if verdict.finance_arbiter_verdict_version != "1.0"
                || verdict.run_id != manifest.run_id
                || verdict.arbiter_role != role
                || verdict.policy_digest != policy_digest
                || verdict.invocation_digest != invocation_digest
                || verdict.profile_digest != invocation.profile.semantic_sha256
                || verdict.claim_manifest_digest != invocation.claim_manifest.semantic_sha256
                || verdict.primary_digest != invocation.primary.binding.semantic_sha256
                || verdict.evidence_index_digest != invocation.evidence_index.semantic_sha256
                || verdict.school_output_digests != school_output_digests
                || verdict.route_binding_digests != route_binding_digests
                || verdict
                    .admitted_closure_evidence_refs
                    .iter()
                    .any(|reference| !admitted_evidence_refs.contains(reference.as_str()))
            {
                bail!("dormant arbiter output violates its bound authority");
            }
        }
    }
    if !terminal_prefix.is_empty() {
        let (terminal, _, _) =
            load_typed::<FinanceRunManifest>(&state.join("terminal/manifest.json"))?;
        let prefix = |mut binding: PortableBinding| {
            binding.artifact_ref = format!("terminal/{}", binding.artifact_ref);
            binding
        };
        let mut expected_terminal = terminal;
        expected_terminal.policy = prefix(expected_terminal.policy);
        expected_terminal.invocation = prefix(expected_terminal.invocation);
        expected_terminal.r1_packets = expected_terminal
            .r1_packets
            .into_iter()
            .map(prefix)
            .collect();
        expected_terminal.r2_packets = expected_terminal
            .r2_packets
            .into_iter()
            .map(prefix)
            .collect();
        expected_terminal.r1_outputs = expected_terminal
            .r1_outputs
            .into_iter()
            .map(prefix)
            .collect();
        expected_terminal.r2_outputs = expected_terminal
            .r2_outputs
            .into_iter()
            .map(prefix)
            .collect();
        expected_terminal.arbiter_outputs = expected_terminal
            .arbiter_outputs
            .into_iter()
            .map(prefix)
            .collect();
        expected_terminal.result = expected_terminal.result.map(prefix);
        if manifest.status == FinanceRunStatus::Merging {
            expected_terminal.status = FinanceRunStatus::Merging;
            expected_terminal.result = None;
            expected_terminal.event_checkpoint = manifest.event_checkpoint.clone();
        }
        if expected_terminal != *manifest {
            bail!("dormant top-level manifest is not the terminal-prefixed authority graph");
        }
        if manifest.status != FinanceRunStatus::Merging {
            verify_bundle_shallow(&state.join("terminal"))?;
        }
    }
    validate_state_artifacts(state, manifest)?;
    verify_expected_event_ledger(state, manifest)?;
    Ok(())
}

fn validate_state_artifacts(state: &Path, manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    for binding in manifest_artifact_bindings(manifest) {
        validate_portable_relative_path(&binding.artifact_ref)?;
        let bytes = std::fs::read(state.join(&binding.artifact_ref)).with_context(|| {
            format!(
                "pending transition artifact is absent: {}",
                binding.artifact_ref
            )
        })?;
        let value = parse_strict_json(&bytes)?;
        verify_portable_binding(
            &binding,
            &bytes,
            &value,
            &binding.contract_family.clone(),
            &binding.revision.clone(),
            &binding.semantic_domain.clone(),
        )?;
    }
    Ok(())
}

fn transition_operation(body: &FinanceRunEventBody) -> &'static str {
    match body {
        FinanceRunEventBody::RunCreated(_) => "create",
        FinanceRunEventBody::PhaseAdvanced(_) => "advance",
        FinanceRunEventBody::RunTerminalized(_) => "terminalize",
    }
}

fn build_transition_event(
    old: Option<&FinanceRunManifest>,
    target: &FinanceRunManifest,
) -> anyhow::Result<FinanceRunEvent> {
    let old_status = old.map(|manifest| manifest.status);
    if !legal_transition(old_status, target.status) {
        bail!("illegal dormant finance manifest transition");
    }
    if let Some(old) = old
        && (old.run_id != target.run_id || old.run_genesis_digest != target.run_genesis_digest)
    {
        bail!("finance transition changed its run or genesis identity");
    }
    let artifact_bindings = manifest_artifact_identities(target);
    let body = match old_status {
        None => FinanceRunEventBody::RunCreated(FinanceRunCreatedPayload {
            status: target.status,
            artifact_bindings,
        }),
        Some(previous_status)
            if matches!(
                target.status,
                FinanceRunStatus::Completed
                    | FinanceRunStatus::Degraded
                    | FinanceRunStatus::Failed
                    | FinanceRunStatus::Cancelled
            ) =>
        {
            FinanceRunEventBody::RunTerminalized(FinanceRunTerminalizedPayload {
                previous_status,
                status: target.status,
                artifact_bindings,
                result: target.result.as_ref().map(artifact_identity),
                termination: target.termination.clone(),
            })
        }
        Some(previous_status) => FinanceRunEventBody::PhaseAdvanced(FinancePhaseAdvancedPayload {
            previous_status,
            status: target.status,
            artifact_bindings,
        }),
    };
    let (sequence, previous_event_sha256) = old.map_or((0, None), |manifest| {
        (
            manifest.event_checkpoint.sequence.saturating_add(1),
            Some(manifest.event_checkpoint.event_sha256.clone()),
        )
    });
    if sequence > MAX_SAFE_JSON_INTEGER {
        bail!("finance event sequence exceeds the I-JSON safe integer range");
    }
    Ok(FinanceRunEvent {
        event_version: "2.0".into(),
        run_id: target.run_id.clone(),
        run_genesis_digest: target.run_genesis_digest.clone(),
        sequence,
        previous_event_sha256,
        body,
    })
}

fn canonical_event_line(event: &FinanceRunEvent) -> anyhow::Result<Vec<u8>> {
    let value = serde_json::to_value(event)?;
    crate::schema::validate_value(&value, crate::contract::FINANCE_RUN_EVENT_SCHEMA)?;
    let mut bytes = canonical_json(&value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn placeholder_checkpoint() -> FinanceEventCheckpoint {
    FinanceEventCheckpoint {
        sequence: 0,
        event_sha256: exact_digest(b"uncommitted finance transition"),
    }
}

fn expected_event_ledger(
    target: &FinanceRunManifest,
) -> anyhow::Result<(Vec<u8>, FinanceEventCheckpoint)> {
    if !matches!(
        target.status,
        FinanceRunStatus::R1Running
            | FinanceRunStatus::R2Running
            | FinanceRunStatus::Merging
            | FinanceRunStatus::Completed
            | FinanceRunStatus::Degraded
            | FinanceRunStatus::Failed
            | FinanceRunStatus::Cancelled
    ) {
        bail!("unsupported dormant finance status for deterministic event reconstruction");
    }
    let mut r1 = target.clone();
    r1.status = FinanceRunStatus::R1Running;
    r1.termination = None;
    r1.r2_packets.clear();
    r1.r1_outputs.clear();
    r1.r2_outputs.clear();
    r1.arbiter_outputs.clear();
    r1.result = None;
    r1.event_checkpoint = placeholder_checkpoint();
    let first = canonical_event_line(&build_transition_event(None, &r1)?)?;
    r1.event_checkpoint = FinanceEventCheckpoint {
        sequence: 0,
        event_sha256: exact_digest(&first),
    };
    let mut ledger = first;
    let terminal_phase = target.termination.as_ref().map(|facts| facts.phase);
    if target.status == FinanceRunStatus::R1Running {
        return Ok((ledger, r1.event_checkpoint));
    }
    if terminal_phase == Some(FinanceRunStatus::R1Running) {
        let mut terminal = target.clone();
        terminal.event_checkpoint = placeholder_checkpoint();
        let line = canonical_event_line(&build_transition_event(Some(&r1), &terminal)?)?;
        let checkpoint = FinanceEventCheckpoint {
            sequence: 1,
            event_sha256: exact_digest(&line),
        };
        ledger.extend_from_slice(&line);
        return Ok((ledger, checkpoint));
    }

    let mut r2 = target.clone();
    r2.status = FinanceRunStatus::R2Running;
    r2.termination = None;
    r2.r2_outputs.clear();
    r2.arbiter_outputs.clear();
    r2.result = None;
    r2.event_checkpoint = placeholder_checkpoint();
    let second = canonical_event_line(&build_transition_event(Some(&r1), &r2)?)?;
    r2.event_checkpoint = FinanceEventCheckpoint {
        sequence: 1,
        event_sha256: exact_digest(&second),
    };
    ledger.extend_from_slice(&second);
    if target.status == FinanceRunStatus::R2Running {
        return Ok((ledger, r2.event_checkpoint));
    }
    if terminal_phase == Some(FinanceRunStatus::R2Running) {
        let mut terminal = target.clone();
        terminal.event_checkpoint = placeholder_checkpoint();
        let line = canonical_event_line(&build_transition_event(Some(&r2), &terminal)?)?;
        let checkpoint = FinanceEventCheckpoint {
            sequence: 2,
            event_sha256: exact_digest(&line),
        };
        ledger.extend_from_slice(&line);
        return Ok((ledger, checkpoint));
    }

    let mut merging = target.clone();
    merging.status = FinanceRunStatus::Merging;
    merging.termination = None;
    merging.result = None;
    merging.event_checkpoint = placeholder_checkpoint();
    let third = canonical_event_line(&build_transition_event(Some(&r2), &merging)?)?;
    merging.event_checkpoint = FinanceEventCheckpoint {
        sequence: 2,
        event_sha256: exact_digest(&third),
    };
    ledger.extend_from_slice(&third);

    if target.status == FinanceRunStatus::Merging {
        return Ok((ledger, merging.event_checkpoint));
    }

    let mut terminal = target.clone();
    terminal.event_checkpoint = placeholder_checkpoint();
    let fourth = canonical_event_line(&build_transition_event(Some(&merging), &terminal)?)?;
    let checkpoint = FinanceEventCheckpoint {
        sequence: 3,
        event_sha256: exact_digest(&fourth),
    };
    ledger.extend_from_slice(&fourth);
    Ok((ledger, checkpoint))
}

fn verify_expected_event_ledger(state: &Path, manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    let (expected_bytes, expected_checkpoint) = expected_event_ledger(manifest)?;
    if manifest.event_checkpoint != expected_checkpoint
        || std::fs::read(state.join("events.jsonl"))? != expected_bytes
    {
        bail!("finance event ledger is not the deterministic chain for its manifest graph");
    }
    verify_manifest_checkpoint(state, manifest)
}

#[derive(Debug)]
struct FinanceLedger {
    events: Vec<FinanceRunEvent>,
    lines: Vec<Vec<u8>>,
    digests: Vec<String>,
}

impl FinanceLedger {
    fn checkpoint(&self) -> Option<FinanceEventCheckpoint> {
        self.events
            .last()
            .zip(self.digests.last())
            .map(|(event, digest)| FinanceEventCheckpoint {
                sequence: event.sequence,
                event_sha256: digest.clone(),
            })
    }
}

fn read_finance_ledger(state: &Path) -> anyhow::Result<FinanceLedger> {
    let path = state.join("events.jsonl");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bail!("finance event ledger has a torn final line");
    }
    let mut events: Vec<FinanceRunEvent> = Vec::new();
    let mut lines = Vec::new();
    let mut digests = Vec::new();
    for (index, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        if line == b"\n" {
            bail!("finance event ledger contains an empty line");
        }
        let value = parse_strict_json(line)?;
        crate::schema::validate_value(&value, crate::contract::FINANCE_RUN_EVENT_SCHEMA)?;
        let event: FinanceRunEvent = serde_json::from_value(value)?;
        let canonical = canonical_event_line(&event)?;
        if canonical != line {
            bail!("finance event ledger line is not exact canonical JSON plus LF");
        }
        let expected_sequence = u64::try_from(index)?;
        if event.event_version != "2.0" || event.sequence != expected_sequence {
            bail!("finance event ledger version or sequence is invalid");
        }
        let expected_previous = digests.last().cloned();
        if event.previous_event_sha256 != expected_previous {
            bail!("finance event ledger previous hash is invalid");
        }
        if let Some(first) = events.first()
            && (event.run_id != first.run_id
                || event.run_genesis_digest != first.run_genesis_digest)
        {
            bail!("finance event ledger contains a cross-run splice");
        }
        digests.push(exact_digest(line));
        lines.push(line.to_vec());
        events.push(event);
    }
    Ok(FinanceLedger {
        events,
        lines,
        digests,
    })
}

fn verify_manifest_checkpoint(state: &Path, manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    let ledger = read_finance_ledger(state)?;
    if ledger.checkpoint().as_ref() != Some(&manifest.event_checkpoint) {
        bail!("finance manifest checkpoint does not equal the event ledger tail");
    }
    let tail = ledger
        .events
        .last()
        .context("finance event ledger is empty")?;
    if tail.run_id != manifest.run_id || tail.run_genesis_digest != manifest.run_genesis_digest {
        bail!("finance manifest and event ledger identities differ");
    }
    Ok(())
}

fn append_event_durable(state: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let path = state.join("events.jsonl");
    let created = !path.exists();
    if !created && !std::fs::symlink_metadata(&path)?.file_type().is_file() {
        bail!("finance event ledger is not a regular file");
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if created {
        sync_directory(state)?;
    }
    Ok(())
}

fn sync_manifest_artifacts(state: &Path, manifest: &FinanceRunManifest) -> anyhow::Result<()> {
    let mut directories = BTreeSet::new();
    for binding in manifest_artifact_bindings(manifest) {
        let path = state.join(&binding.artifact_ref);
        std::fs::File::open(&path)?.sync_all()?;
        if let Some(parent) = path.parent() {
            directories.insert(parent.to_path_buf());
        }
    }
    for directory in directories {
        sync_directory(&directory)?;
    }
    sync_directory(state)
}

fn decode_canonical_base64(encoded: &str, expected_length: u64) -> anyhow::Result<Vec<u8>> {
    if expected_length > MAX_SAFE_JSON_INTEGER {
        bail!("pending transition byte length exceeds the I-JSON safe integer range");
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("pending transition contains invalid base64")?;
    if base64::engine::general_purpose::STANDARD.encode(&bytes) != encoded
        || u64::try_from(bytes.len())? != expected_length
    {
        bail!("pending transition base64 or byte length is not canonical");
    }
    Ok(bytes)
}

fn validate_event_target(
    event: &FinanceRunEvent,
    target: &FinanceRunManifest,
    old_checkpoint: Option<&FinanceEventCheckpoint>,
) -> anyhow::Result<Option<FinanceRunStatus>> {
    if event.run_id != target.run_id
        || event.run_genesis_digest != target.run_genesis_digest
        || event.sequence != old_checkpoint.map_or(0, |checkpoint| checkpoint.sequence + 1)
        || event.previous_event_sha256
            != old_checkpoint.map(|checkpoint| checkpoint.event_sha256.clone())
        || manifest_artifact_identities(target)
            != match &event.body {
                FinanceRunEventBody::RunCreated(payload) => payload.artifact_bindings.clone(),
                FinanceRunEventBody::PhaseAdvanced(payload) => payload.artifact_bindings.clone(),
                FinanceRunEventBody::RunTerminalized(payload) => payload.artifact_bindings.clone(),
            }
    {
        bail!("pending finance event does not bind its exact target manifest graph");
    }
    let previous_status = match &event.body {
        FinanceRunEventBody::RunCreated(payload) => {
            if old_checkpoint.is_some() || payload.status != target.status {
                bail!("invalid finance run.created transition");
            }
            None
        }
        FinanceRunEventBody::PhaseAdvanced(payload) => {
            if payload.status != target.status
                || matches!(
                    target.status,
                    FinanceRunStatus::Completed
                        | FinanceRunStatus::Degraded
                        | FinanceRunStatus::Failed
                        | FinanceRunStatus::Cancelled
                )
            {
                bail!("invalid finance run.phase_advanced transition");
            }
            Some(payload.previous_status)
        }
        FinanceRunEventBody::RunTerminalized(payload) => {
            if payload.status != target.status
                || payload.result != target.result.as_ref().map(artifact_identity)
                || payload.termination != target.termination
                || target
                    .termination
                    .as_ref()
                    .is_some_and(|termination| termination.phase != payload.previous_status)
                || !matches!(
                    target.status,
                    FinanceRunStatus::Completed
                        | FinanceRunStatus::Degraded
                        | FinanceRunStatus::Failed
                        | FinanceRunStatus::Cancelled
                )
            {
                bail!("invalid finance run.terminalized transition");
            }
            Some(payload.previous_status)
        }
    };
    if !legal_transition(previous_status, target.status) {
        bail!("pending finance event carries an illegal status transition");
    }
    Ok(previous_status)
}

fn validate_pending(
    state: &Path,
    pending: &PendingFinanceTransition,
) -> anyhow::Result<(FinanceRunManifest, Vec<u8>, FinanceRunEvent, Vec<u8>)> {
    if pending.pending_transition_version != "2.0" {
        bail!("unsupported finance pending transition");
    }
    let event_bytes =
        decode_canonical_base64(&pending.event_bytes_base64, pending.event_byte_length)?;
    let manifest_bytes = decode_canonical_base64(
        &pending.target_manifest_bytes_base64,
        pending.target_manifest_byte_length,
    )?;
    if exact_digest(&event_bytes) != pending.event_exact_sha256
        || exact_digest(&manifest_bytes) != pending.target_manifest_exact_sha256
    {
        bail!("pending transition exact byte digest mismatch");
    }
    let event_value = parse_strict_json(&event_bytes)?;
    crate::schema::validate_value(&event_value, crate::contract::FINANCE_RUN_EVENT_SCHEMA)?;
    let event: FinanceRunEvent = serde_json::from_value(event_value)?;
    if canonical_event_line(&event)? != event_bytes
        || transition_operation(&event.body) != pending.operation
    {
        bail!("pending transition event bytes or operation mismatch");
    }
    let manifest_value = parse_strict_json(&manifest_bytes)?;
    crate::schema::validate_value(
        &manifest_value,
        crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
    )?;
    let manifest: FinanceRunManifest = serde_json::from_value(manifest_value)?;
    if pending.run_id != manifest.run_id
        || pending.run_genesis_digest != manifest.run_genesis_digest
        || manifest.event_checkpoint.sequence != event.sequence
        || manifest.event_checkpoint.event_sha256 != pending.event_exact_sha256
        || pending.artifact_bindings != manifest_artifact_bindings(&manifest)
    {
        bail!("pending transition target identity or artifact list mismatch");
    }
    validate_event_target(&event, &manifest, pending.old_event_checkpoint.as_ref())?;
    validate_state_artifacts(state, &manifest)?;
    Ok((manifest, manifest_bytes, event, event_bytes))
}

fn transition_with_pending(state: &Path, manifest: &mut FinanceRunManifest) -> anyhow::Result<()> {
    let pending_path = state.join("pending-transition.json");
    if pending_path.exists() {
        bail!("finance transition journal must be reconciled before a new transition");
    }
    let old_bytes = std::fs::read(state.join("manifest.json")).ok();
    let old = old_bytes
        .as_deref()
        .map(parse_strict_json)
        .transpose()?
        .map(serde_json::from_value::<FinanceRunManifest>)
        .transpose()?;
    if let Some(old) = old.as_ref() {
        crate::schema::validate_value(
            &serde_json::to_value(old)?,
            crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
        )?;
        validate_state_artifacts(state, old)?;
        verify_manifest_checkpoint(state, old)?;
    } else if !read_finance_ledger(state)?.events.is_empty() {
        bail!("finance event ledger exists without its creation manifest");
    }
    let event = build_transition_event(old.as_ref(), manifest)?;
    let event_bytes = canonical_event_line(&event)?;
    let event_digest = exact_digest(&event_bytes);
    manifest.event_checkpoint = FinanceEventCheckpoint {
        sequence: event.sequence,
        event_sha256: event_digest.clone(),
    };
    let manifest_value = serde_json::to_value(&*manifest)?;
    crate::schema::validate_value(
        &manifest_value,
        crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
    )?;
    validate_state_artifacts(state, manifest)?;
    sync_manifest_artifacts(state, manifest)?;
    let manifest_bytes = serde_json::to_vec_pretty(&*manifest)?;
    let target_digest = exact_digest(&manifest_bytes);
    let transition_id = semantic_digest_value(
        "quinte.finance-transition-id.v1",
        &serde_json::json!({
            "run_id": manifest.run_id,
            "run_genesis_digest": manifest.run_genesis_digest,
            "sequence": event.sequence,
            "operation": transition_operation(&event.body),
            "old_manifest_exact_sha256": old_bytes.as_deref().map(exact_digest),
            "event_exact_sha256": event_digest,
            "target_manifest_exact_sha256": target_digest,
        }),
    )?;
    let pending = PendingFinanceTransition {
        pending_transition_version: "2.0".into(),
        run_id: manifest.run_id.clone(),
        run_genesis_digest: manifest.run_genesis_digest.clone(),
        transition_id,
        operation: transition_operation(&event.body).into(),
        old_manifest_exact_sha256: old_bytes.as_deref().map(exact_digest),
        old_event_checkpoint: old.as_ref().map(|old| old.event_checkpoint.clone()),
        event_exact_sha256: exact_digest(&event_bytes),
        event_byte_length: u64::try_from(event_bytes.len())?,
        event_bytes_base64: base64::engine::general_purpose::STANDARD.encode(&event_bytes),
        target_manifest_exact_sha256: exact_digest(&manifest_bytes),
        target_manifest_byte_length: u64::try_from(manifest_bytes.len())?,
        target_manifest_bytes_base64: base64::engine::general_purpose::STANDARD
            .encode(&manifest_bytes),
        artifact_bindings: manifest_artifact_bindings(manifest),
    };
    let pending_value = serde_json::to_value(&pending)?;
    crate::schema::validate_value(
        &pending_value,
        crate::contract::FINANCE_PENDING_TRANSITION_SCHEMA,
    )?;
    atomic_write_durable(&pending_path, &serde_json::to_vec_pretty(&pending)?)?;
    append_event_durable(state, &event_bytes)?;
    atomic_write_durable(&state.join("manifest.json"), &manifest_bytes)?;
    remove_file_durable(&pending_path)?;
    Ok(())
}

fn reconcile_pending(state: &Path) -> anyhow::Result<()> {
    let pending_path = state.join("pending-transition.json");
    if !pending_path.exists() {
        return Ok(());
    }
    let value = parse_strict_json(&std::fs::read(&pending_path)?)?;
    crate::schema::validate_value(&value, crate::contract::FINANCE_PENDING_TRANSITION_SCHEMA)?;
    let pending: PendingFinanceTransition = serde_json::from_value(value)?;
    let (target, target_bytes, event, event_bytes) = validate_pending(state, &pending)?;
    let current_bytes = std::fs::read(state.join("manifest.json")).ok();
    let current_digest = current_bytes.as_deref().map(exact_digest);
    let manifest_is_old = current_digest == pending.old_manifest_exact_sha256;
    let manifest_is_target =
        current_digest.as_deref() == Some(pending.target_manifest_exact_sha256.as_str());
    if !manifest_is_old && !manifest_is_target {
        bail!("finance pending transition manifest is neither exact old nor exact target bytes");
    }
    let ledger = read_finance_ledger(state)?;
    let ledger_checkpoint = ledger.checkpoint();
    let ledger_is_old = ledger_checkpoint == pending.old_event_checkpoint;
    let ledger_is_target = ledger_checkpoint.as_ref() == Some(&target.event_checkpoint)
        && ledger.lines.last().map(Vec::as_slice) == Some(event_bytes.as_slice());
    if !ledger_is_old && !ledger_is_target {
        bail!("finance pending transition ledger is neither exact old head nor exact target event");
    }
    if manifest_is_target && !ledger_is_target {
        bail!("finance pending transition manifest is ahead of its event ledger");
    }
    if manifest_is_old {
        if let Some(bytes) = current_bytes.as_deref() {
            let value = parse_strict_json(bytes)?;
            let old: FinanceRunManifest = serde_json::from_value(value)?;
            let previous_status = match &event.body {
                FinanceRunEventBody::RunCreated(_) => None,
                FinanceRunEventBody::PhaseAdvanced(payload) => Some(payload.previous_status),
                FinanceRunEventBody::RunTerminalized(payload) => Some(payload.previous_status),
            };
            if Some(old.status) != previous_status
                || old.event_checkpoint != pending.old_event_checkpoint.clone().unwrap()
                || old.run_id != pending.run_id
                || old.run_genesis_digest != pending.run_genesis_digest
            {
                bail!("finance pending transition old manifest identity mismatch");
            }
            validate_state_artifacts(state, &old)?;
        } else if pending.old_manifest_exact_sha256.is_some()
            || pending.old_event_checkpoint.is_some()
        {
            bail!("finance pending transition unexpectedly lost its old manifest");
        }
        if ledger_is_old {
            append_event_durable(state, &event_bytes)?;
        }
        atomic_write_durable(&state.join("manifest.json"), &target_bytes)?;
    }
    remove_file_durable(&pending_path)?;
    Ok(())
}

pub fn dormant_init(
    source: &Path,
    state: &Path,
    acknowledgement: &str,
) -> anyhow::Result<FinanceRunManifest> {
    require_dormant_writer_ack(acknowledgement)?;
    let _lock = lifecycle_lock(state)?;
    if state.exists()
        && (state.join("manifest.json").exists() || state.join("pending-transition.json").exists())
    {
        reconcile_pending(state)?;
        if !state.join("manifest.json").exists() {
            bail!("finance pending transition did not restore a manifest");
        }
        verify_source_replay(source, state)?;
        let manifest = load_typed::<FinanceRunManifest>(&state.join("manifest.json"))?.0;
        validate_dormant_manifest_graph(state, &manifest)?;
        return Ok(manifest);
    }
    if state.exists() {
        bail!("refusing to reuse a partial dormant finance state directory");
    }
    let parent = state.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".finance-state.")
        .tempdir_in(parent)?;
    let work = staging.path();
    std::fs::create_dir_all(work.join("input"))?;
    for file in [
        "policy.json",
        "profile.json",
        "claim-manifest.json",
        "evidence-index.json",
        "invocation.json",
        "primary.json",
    ] {
        copy_exact(&source.join(file), &work.join("input").join(file))?;
    }
    let copied = work.join("input");
    let (policy, policy_bytes, policy_value) =
        load_typed::<FinancePolicy>(&copied.join("policy.json"))?;
    validate_finance_policy(&policy)?;
    crate::schema::validate_value(&policy_value, crate::contract::FINANCE_POLICY_SCHEMA)?;
    let (profile, profile_bytes, profile_value) =
        load_typed::<FinanceReviewProfile>(&copied.join("profile.json"))?;
    validate_profile(&profile)?;
    let (invocation, invocation_bytes, invocation_value) =
        load_typed::<FinanceReviewInvocation>(&copied.join("invocation.json"))?;
    let (claims, claims_bytes, claims_value) =
        load_typed::<FinanceClaimManifest>(&copied.join("claim-manifest.json"))?;
    validate_claim_manifest(&profile, &claims)?;
    let (evidence, evidence_bytes, evidence_value) =
        load_typed::<FinanceEvidenceIndex>(&copied.join("evidence-index.json"))?;
    if evidence.finance_evidence_index_version != "1.0" {
        bail!("finance_evidence_index_version must be 1.0");
    }
    for (relative, bytes) in validate_evidence_artifacts(source, &evidence)? {
        crate::util::atomic_write(&copied.join(relative), &bytes)?;
    }
    verify_portable_binding(
        &policy.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;
    verify_portable_binding(
        &invocation.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;
    verify_portable_binding(
        &invocation.claim_manifest,
        &claims_bytes,
        &claims_value,
        "finance-claim-manifest",
        "1.0",
        "quinte.finance-claim-manifest.v1",
    )?;
    verify_portable_binding(
        &invocation.evidence_index,
        &evidence_bytes,
        &evidence_value,
        "finance-evidence-index",
        "1.0",
        "quinte.finance-evidence-index.v1",
    )?;
    let primary_bytes = std::fs::read(copied.join("primary.json"))?;
    let primary_value = parse_strict_json(&primary_bytes)?;
    if !profile
        .allowed_primary_contracts
        .contains(&invocation.primary.binding.contract_family)
        || !profile
            .hash_domains
            .contains(&invocation.primary.binding.semantic_domain)
    {
        bail!("primary artifact family or semantic domain is not allowed by the pinned profile");
    }
    verify_portable_binding(
        &invocation.primary.binding,
        &primary_bytes,
        &primary_value,
        &invocation.primary.binding.contract_family.clone(),
        &invocation.primary.binding.revision.clone(),
        &invocation.primary.binding.semantic_domain.clone(),
    )?;
    let invocation_binding = binding_for(
        "input/invocation.json",
        "finance-review-invocation",
        "1.0",
        "quinte.finance-review-invocation.v1",
        &invocation_bytes,
        &invocation_value,
    )?;
    let policy_binding = binding_for(
        "input/policy.json",
        "policy",
        "3.0",
        "quinte.finance-policy.v3",
        &policy_bytes,
        &policy_value,
    )?;
    let mut packets = Vec::new();
    for authority in &profile.schools {
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == authority.school_id)
            .context("profile school has no Policy 3.0 route")?;
        let packet = build_r1_packet(
            &invocation.invocation_id,
            invocation_binding.clone(),
            policy_binding.clone(),
            invocation.profile.clone(),
            invocation.claim_manifest.clone(),
            invocation.primary.binding.clone(),
            invocation.evidence_index.clone(),
            semantic_digest_value("quinte.finance-route-binding.v1", route)?,
            authority,
        );
        let value = serde_json::to_value(&packet)?;
        crate::schema::validate_value(&value, crate::contract::FINANCE_TASK_PACKET_SCHEMA)?;
        let bytes = serde_json::to_vec_pretty(&packet)?;
        let name = serde_json::to_value(authority.school_id)?
            .as_str()
            .unwrap()
            .to_string();
        let relative = format!("packets/r1/{name}.json");
        write_idempotent(&work.join(&relative), &bytes)?;
        packets.push(binding_for(
            relative,
            "task-packet",
            "2.0",
            "quinte.finance-task-packet.v2",
            &bytes,
            &value,
        )?);
    }
    let run_genesis_digest = compute_run_genesis(
        &invocation.invocation_id,
        &policy,
        &invocation,
        &policy_binding,
        &invocation_binding,
        &packets,
    )?;
    let mut manifest = FinanceRunManifest {
        manifest_version: "3.0".into(),
        protocol_version: "2.0".into(),
        run_id: invocation.invocation_id,
        run_genesis_digest,
        event_checkpoint: FinanceEventCheckpoint {
            sequence: 0,
            event_sha256: exact_digest(b"uncommitted"),
        },
        status: FinanceRunStatus::R1Running,
        termination: None,
        policy: policy_binding,
        invocation: invocation_binding,
        r1_packets: packets,
        r2_packets: Vec::new(),
        r1_outputs: Vec::new(),
        r2_outputs: Vec::new(),
        arbiter_outputs: Vec::new(),
        result: None,
    };
    transition_with_pending(work, &mut manifest)?;
    let staging_path = staging.keep();
    std::fs::rename(&staging_path, state)?;
    Ok(manifest)
}

pub fn dormant_advance(state: &Path, acknowledgement: &str) -> anyhow::Result<FinanceRunManifest> {
    require_dormant_writer_ack(acknowledgement)?;
    let _lock = lifecycle_lock(state)?;
    reconcile_pending(state)?;
    let (mut manifest, _, _) = load_typed::<FinanceRunManifest>(&state.join("manifest.json"))?;
    validate_dormant_manifest_graph(state, &manifest)?;
    let input = state.join("input");
    match manifest.status {
        FinanceRunStatus::R1Running => {
            let (_, _, claims_value) =
                load_typed::<FinanceClaimManifest>(&input.join("claim-manifest.json"))?;
            let claims: FinanceClaimManifest = serde_json::from_value(claims_value)?;
            let (profile, _, _) = load_typed::<FinanceReviewProfile>(&input.join("profile.json"))?;
            let mut r1 = Vec::new();
            let mut bindings = Vec::new();
            for (_, school) in SCHOOL_BINDINGS {
                let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
                let relative = format!("outputs/r1/{name}.json");
                let (output, bytes, value) =
                    load_typed::<SchoolLaneOutput>(&state.join(&relative))?;
                if output.school_id != school {
                    bail!("dormant R1 school output filename/seat binding mismatch");
                }
                crate::schema::validate_value(&value, crate::contract::SCHOOL_LANE_OUTPUT_SCHEMA)?;
                r1.push(output);
                bindings.push(binding_for(
                    relative,
                    "school-lane-output",
                    "1.0",
                    "quinte.school-lane-output.v1",
                    &bytes,
                    &value,
                )?);
            }
            validate_school_output_set(&manifest.run_id, FinancePhase::R1, &r1)?;
            let (evidence, _, _) =
                load_typed::<FinanceEvidenceIndex>(&input.join("evidence-index.json"))?;
            validate_lane_claims_and_evidence(&profile, &claims, &evidence, &r1)?;
            let (policy, _, _) = load_typed::<FinancePolicy>(&input.join("policy.json"))?;
            let (invocation, _, _) =
                load_typed::<FinanceReviewInvocation>(&input.join("invocation.json"))?;
            for output in &r1 {
                let route = policy
                    .school_bindings
                    .iter()
                    .find(|route| route.school_id == output.school_id)
                    .context("R1 output has no Policy 3.0 route")?;
                if output.expected_route_digest
                    != semantic_digest_value("quinte.finance-route-binding.v1", route)?
                    || output.profile_digest != invocation.profile.semantic_sha256
                    || output.primary_digest != invocation.primary.binding.semantic_sha256
                    || output.evidence_index_digest != invocation.evidence_index.semantic_sha256
                {
                    bail!("R1 output immutable route/profile/primary/evidence binding mismatch");
                }
                let packet_index = SCHOOL_BINDINGS
                    .iter()
                    .position(|(_, school)| *school == output.school_id)
                    .context("R1 output has no fixed packet slot")?;
                let (packet, packet_bytes, packet_value) = load_typed::<FinanceTaskPacket>(
                    &state.join(&manifest.r1_packets[packet_index].artifact_ref),
                )?;
                verify_portable_binding(
                    &manifest.r1_packets[packet_index],
                    &packet_bytes,
                    &packet_value,
                    "task-packet",
                    "2.0",
                    "quinte.finance-task-packet.v2",
                )?;
                bind_output_to_packet(output, &packet, "quinte.finance-task-packet.v2")?;
            }
            let mut packets = Vec::new();
            for authority in &profile.schools {
                let route = policy
                    .school_bindings
                    .iter()
                    .find(|route| route.school_id == authority.school_id)
                    .context("profile school has no Policy 3.0 route")?;
                let packet = build_r2_packet(
                    &manifest.run_id,
                    authority,
                    &claims.claims,
                    &r1,
                    &bindings,
                    manifest.policy.clone(),
                    invocation.profile.clone(),
                    invocation.claim_manifest.clone(),
                    invocation.primary.binding.clone(),
                    invocation.evidence_index.clone(),
                    semantic_digest_value("quinte.finance-route-binding.v1", route)?,
                )?;
                let value = serde_json::to_value(&packet)?;
                crate::schema::validate_value(&value, crate::contract::FINANCE_R2_PACKET_SCHEMA)?;
                let bytes = serde_json::to_vec_pretty(&packet)?;
                let name = serde_json::to_value(authority.school_id)?
                    .as_str()
                    .unwrap()
                    .to_string();
                let relative = format!("packets/r2/{name}.json");
                write_idempotent(&state.join(&relative), &bytes)?;
                packets.push(binding_for(
                    relative,
                    "r2-packet",
                    "2.0",
                    "quinte.finance-r2-packet.v2",
                    &bytes,
                    &value,
                )?);
            }
            manifest.r1_outputs = bindings;
            manifest.r2_packets = packets;
            manifest.status = FinanceRunStatus::R2Running;
            transition_with_pending(state, &mut manifest)?;
            Ok(manifest)
        }
        FinanceRunStatus::R2Running => {
            for (_, school) in SCHOOL_BINDINGS {
                let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
                let (output, _, _) =
                    load_typed::<SchoolLaneOutput>(&state.join(format!("outputs/r2/{name}.json")))?;
                let packet_index = SCHOOL_BINDINGS
                    .iter()
                    .position(|(_, candidate)| *candidate == school)
                    .unwrap();
                let (packet, packet_bytes, packet_value) = load_typed::<FinanceR2Packet>(
                    &state.join(&manifest.r2_packets[packet_index].artifact_ref),
                )?;
                verify_portable_binding(
                    &manifest.r2_packets[packet_index],
                    &packet_bytes,
                    &packet_value,
                    "r2-packet",
                    "2.0",
                    "quinte.finance-r2-packet.v2",
                )?;
                bind_output_to_packet(&output, &packet, "quinte.finance-r2-packet.v2")?;
            }
            let mirror = tempfile::Builder::new()
                .prefix(".finance-finalize-input.")
                .tempdir_in(state)?;
            let finalize_input = mirror.path();
            for file in [
                "policy.json",
                "profile.json",
                "claim-manifest.json",
                "evidence-index.json",
                "invocation.json",
                "primary.json",
            ] {
                copy_exact(&input.join(file), &finalize_input.join(file))?;
            }
            for phase in ["r1", "r2"] {
                for (_, school) in SCHOOL_BINDINGS {
                    let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
                    copy_exact(
                        &state.join(format!("outputs/{phase}/{name}.json")),
                        &finalize_input.join(format!("{phase}/{name}.json")),
                    )?;
                }
            }
            for file in ["counterpart-arbiter.json", "primary-arbiter.json"] {
                copy_exact(
                    &state.join("outputs/arbiters").join(file),
                    &finalize_input.join("arbiters").join(file),
                )?;
            }
            finalize_bundle(finalize_input, &state.join("terminal"))?;
            let verified = verify_bundle(&state.join("terminal"))?;
            let (terminal, _, _) =
                load_typed::<FinanceRunManifest>(&state.join("terminal/manifest.json"))?;
            let prefix = |mut binding: PortableBinding| {
                binding.artifact_ref = format!("terminal/{}", binding.artifact_ref);
                binding
            };
            manifest.policy = prefix(terminal.policy.clone());
            manifest.invocation = prefix(terminal.invocation.clone());
            manifest.r1_packets = terminal
                .r1_packets
                .clone()
                .into_iter()
                .map(prefix)
                .collect();
            manifest.r2_packets = terminal
                .r2_packets
                .clone()
                .into_iter()
                .map(prefix)
                .collect();
            manifest.r1_outputs = terminal
                .r1_outputs
                .clone()
                .into_iter()
                .map(prefix)
                .collect();
            manifest.r2_outputs = terminal.r2_outputs.into_iter().map(prefix).collect();
            manifest.arbiter_outputs = terminal.arbiter_outputs.into_iter().map(prefix).collect();
            let terminal_result = terminal.result.map(prefix);
            manifest.result = None;
            manifest.run_genesis_digest = terminal.run_genesis_digest;
            manifest.status = FinanceRunStatus::Merging;
            transition_with_pending(state, &mut manifest)?;
            manifest.result = terminal_result;
            manifest.status = FinanceRunStatus::Completed;
            transition_with_pending(state, &mut manifest)?;
            debug_assert_eq!(verified.run_id, manifest.run_id);
            Ok(manifest)
        }
        FinanceRunStatus::Completed | FinanceRunStatus::Degraded => {
            verify_bundle(&state.join("terminal"))?;
            Ok(manifest)
        }
        _ => bail!("finance run cannot advance from {:?}", manifest.status),
    }
}

pub fn validate_finance_policy(policy: &FinancePolicy) -> anyhow::Result<()> {
    if policy.policy_version != FINANCE_POLICY_VERSION
        || policy.protocol_version != FINANCE_PROTOCOL_VERSION
        || policy.isolation_backend != "process_information_flow_v1"
    {
        bail!(
            "finance policy must select Policy 3.0, Protocol 2.0, and process information-flow isolation"
        );
    }
    if policy.school_bindings.len() != 5 {
        bail!("finance policy must bind exactly five schools");
    }
    let mut families = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for (((party, school), route), index) in
        SCHOOL_BINDINGS.iter().zip(&policy.school_bindings).zip(0..)
    {
        if route.party_id != *party || route.school_id != *school {
            bail!("finance policy route {index} violates the fixed school map");
        }
        if !routes.insert(&route.route_id) {
            bail!("finance policy route IDs must be unique");
        }
        families.insert((&route.family, &route.provider, &route.model));
    }
    for (expected_role, route) in [
        ("counterpart_arbiter", &policy.counterpart_arbiter),
        ("primary_arbiter", &policy.primary_arbiter),
    ] {
        if route.arbiter_role != expected_role || !routes.insert(&route.route_id) {
            bail!("finance policy arbiter role or route ID is invalid");
        }
        families.insert((&route.family, &route.provider, &route.model));
    }
    if policy.same_family_required && families.len() != 1 {
        bail!(
            "finance policy must keep all five schools and two arbiters on one model family binding"
        );
    }
    if !policy.same_family_required && families.len() > 3 {
        // Mixed-vendor rosters are allowed (same_family_required=false),
        // bounded so a typo'd policy cannot produce a five-way split.
        bail!(
            "mixed-family finance policy binds {} families; keep it to at most 3",
            families.len()
        );
    }
    Ok(())
}

pub fn validate_profile(profile: &FinanceReviewProfile) -> anyhow::Result<()> {
    if profile.finance_review_profile_version != "1.0" {
        bail!("finance_review_profile_version must be 1.0");
    }
    if profile.schools.len() != SCHOOL_BINDINGS.len() {
        bail!("finance profile must define exactly five schools");
    }
    for ((party, school), authority) in SCHOOL_BINDINGS.iter().zip(&profile.schools) {
        if authority.party_id != *party || authority.school_id != *school {
            bail!("finance profile school map does not match the fixed Party A-E authority map");
        }
    }
    Ok(())
}

pub fn validate_claim_manifest(
    profile: &FinanceReviewProfile,
    manifest: &FinanceClaimManifest,
) -> anyhow::Result<()> {
    if manifest.finance_claim_manifest_version != "1.0" {
        bail!("finance_claim_manifest_version must be 1.0");
    }
    let predicates: BTreeSet<_> = profile.applicability_predicate_codes.iter().collect();
    let mut ids = BTreeSet::new();
    for claim in &manifest.claims {
        if !ids.insert(&claim.claim_id) {
            bail!("duplicate finance claim id: {}", claim.claim_id);
        }
        if claim.school_applicability.len() != SCHOOL_BINDINGS.len() {
            bail!("claim {} must preregister all five schools", claim.claim_id);
        }
        if claim.claim_classes.is_empty() {
            bail!("finance claim classes must be explicitly preregistered");
        }
        for authority in &profile.schools {
            if claim
                .claim_classes
                .iter()
                .any(|class| authority.forbidden_claim_classes.contains(class))
                && claim
                    .school_applicability
                    .get(&authority.school_id)
                    .is_some_and(|rule| rule.mode != ApplicabilityMode::OutOfScope)
            {
                bail!("finance claim enters a school whose profile forbids its claim class");
            }
        }
        for (_, school) in SCHOOL_BINDINGS {
            let rule = claim.school_applicability.get(&school).with_context(|| {
                format!(
                    "claim {} is missing {:?} applicability",
                    claim.claim_id, school
                )
            })?;
            match rule.mode {
                ApplicabilityMode::Mandatory => {
                    if rule.predicate_code.is_some() || rule.predicate_result != Some(true) {
                        bail!("mandatory applicability must be fixed true without a predicate");
                    }
                }
                ApplicabilityMode::Conditional => {
                    let code = rule.predicate_code.as_ref().context(
                        "conditional applicability requires a registered predicate code",
                    )?;
                    if !predicates.contains(code) || rule.predicate_result.is_none() {
                        bail!("conditional applicability predicate is not registered or evaluated");
                    }
                }
                ApplicabilityMode::OutOfScope => {
                    if rule.predicate_code.is_some() || rule.predicate_result != Some(false) {
                        bail!("out-of-scope applicability must be fixed false");
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn build_r1_packet(
    run_id: &str,
    invocation: PortableBinding,
    policy: PortableBinding,
    profile: PortableBinding,
    claim_manifest: PortableBinding,
    primary: PortableBinding,
    evidence_index: PortableBinding,
    recipient_route_digest: String,
    authority: &SchoolAuthority,
) -> FinanceTaskPacket {
    FinanceTaskPacket {
        task_packet_version: FINANCE_PACKET_VERSION.into(),
        run_id: run_id.into(),
        phase: FinancePhase::R1,
        invocation,
        policy,
        profile,
        claim_manifest,
        primary,
        evidence_index,
        recipient_route_digest,
        authority: authority.clone(),
    }
}

pub fn build_r2_packet(
    run_id: &str,
    recipient: &SchoolAuthority,
    claims: &[FinanceClaim],
    r1: &[SchoolLaneOutput],
    r1_bindings: &[PortableBinding],
    policy: PortableBinding,
    profile: PortableBinding,
    claim_manifest: PortableBinding,
    primary: PortableBinding,
    evidence_index: PortableBinding,
    recipient_route_digest: String,
) -> anyhow::Result<FinanceR2Packet> {
    validate_school_output_set(run_id, FinancePhase::R1, r1)?;
    if r1_bindings.len() != 5 {
        bail!("R2 source set must bind exactly five R1 artifacts");
    }
    let mut matched = BTreeSet::new();
    for output in r1 {
        let output_value = serde_json::to_value(output)?;
        let exact_candidates = [
            exact_digest(&serde_json::to_vec_pretty(output)?),
            exact_digest(&serde_json::to_vec_pretty(&output_value)?),
        ];
        let index = r1_bindings
            .iter()
            .enumerate()
            .find_map(|(index, binding)| {
                (binding.contract_family == "school-lane-output"
                    && binding.revision == "1.0"
                    && binding.semantic_domain == "quinte.school-lane-output.v1"
                    && exact_candidates.contains(&binding.exact_sha256))
                    .then_some(index)
            })
            .with_context(|| {
                format!(
                    "R2 source set does not bind complete R1 output exact identities {:?}; candidates: {:?}",
                    exact_candidates,
                    r1_bindings.iter().map(|binding| &binding.exact_sha256).collect::<Vec<_>>()
                )
            })?;
        if !matched.insert(index) {
            bail!("R2 source set reuses one R1 artifact for multiple outputs");
        }
    }
    if matched.len() != r1_bindings.len() {
        bail!("R2 source set contains an unrelated R1 artifact identity");
    }
    let mut source_identities = r1_bindings
        .iter()
        .map(|binding| R1SourceIdentity {
            exact_sha256: binding.exact_sha256.clone(),
            semantic_sha256: binding.semantic_sha256.clone(),
        })
        .collect::<Vec<_>>();
    source_identities.sort();
    if source_identities.windows(2).any(|pair| pair[0] == pair[1]) {
        bail!("R2 source set contains a duplicate R1 artifact identity");
    }
    let r1_source_set_semantic_sha256 = semantic_digest_value(
        "quinte.finance-r1-source-set.v1",
        &R1SourceSet {
            outputs: &source_identities,
        },
    )?;
    let mut projected = Vec::with_capacity(5);
    for output in r1 {
        let residual_by_claim = output
            .residuals
            .iter()
            .flat_map(|residual| {
                residual
                    .affected_claim_ids
                    .iter()
                    .map(move |claim| (claim.clone(), residual.residual_code.clone()))
            })
            .fold(
                BTreeMap::<String, Vec<String>>::new(),
                |mut map, (claim, code)| {
                    map.entry(claim).or_default().push(code);
                    map
                },
            );
        let mut decisions = output
            .decisions
            .iter()
            .map(|decision| {
                for reference in &decision.evidence_refs {
                    validate_anonymous_evidence_id(reference)?;
                }
                let mut evidence_refs = decision.evidence_refs.clone();
                evidence_refs.sort();
                evidence_refs.dedup();
                let mut residual_codes = residual_by_claim
                    .get(&decision.claim_id)
                    .cloned()
                    .unwrap_or_default();
                residual_codes.sort();
                residual_codes.dedup();
                Ok(AnonymousDecision {
                    claim_id: decision.claim_id.clone(),
                    disposition: decision.disposition,
                    evidence_refs,
                    residual_codes,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        decisions.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
        let key = semantic_digest_value(
            "quinte.r2-contribution.v2",
            &UnaliasedR1Contribution {
                decisions: decisions.clone(),
            },
        )?;
        projected.push((key, decisions));
    }
    projected.sort_by(|left, right| left.0.cmp(&right.0));
    let contributions = projected
        .into_iter()
        .enumerate()
        .map(|(index, (_, decisions))| AnonymousR1Contribution {
            contributor_alias: format!("contributor-{}", index + 1),
            decisions,
        })
        .collect();
    let corpus = AnonymousR1Corpus {
        claims: claims
            .iter()
            .map(|claim| (claim.claim_id.clone(), claim.text.clone()))
            .collect(),
        contributions,
    };
    let corpus_semantic_sha256 = semantic_digest_value(
        "quinte.finance-r2-corpus.v2",
        &BoundAnonymousCorpus {
            r1_source_set_semantic_sha256: &r1_source_set_semantic_sha256,
            corpus: &corpus,
        },
    )?;
    Ok(FinanceR2Packet {
        packet_version: FINANCE_PACKET_VERSION.into(),
        run_id: run_id.into(),
        recipient_authority: recipient.clone(),
        policy,
        profile,
        claim_manifest,
        primary,
        evidence_index,
        recipient_route_digest,
        r1_source_set_semantic_sha256,
        corpus_semantic_sha256,
        corpus,
    })
}

fn bind_output_to_packet<T: Serialize>(
    output: &SchoolLaneOutput,
    packet: &T,
    domain: &str,
) -> anyhow::Result<()> {
    let packet_value = serde_json::to_value(packet)?;
    let packet_bytes = serde_json::to_vec_pretty(&packet_value)?;
    if output.input_packet_exact_sha256 != exact_digest(&packet_bytes)
        || output.input_packet_semantic_sha256 != semantic_digest(domain, &packet_value)?
    {
        bail!("school output input packet binding mismatch");
    }
    Ok(())
}

fn validate_anonymous_evidence_id(reference: &str) -> anyhow::Result<()> {
    let valid = reference
        .strip_prefix("evidence:sha256:")
        .is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        });
    if !valid {
        bail!("R2 projection evidence references must be anonymous content IDs");
    }
    Ok(())
}

pub fn validate_school_output_set(
    run_id: &str,
    phase: FinancePhase,
    outputs: &[SchoolLaneOutput],
) -> anyhow::Result<()> {
    if outputs.len() != 5 {
        bail!("finance phase must contain exactly five school outputs");
    }
    let mut schools = BTreeSet::new();
    for output in outputs {
        if output.school_lane_output_version != "1.0"
            || output.run_id != run_id
            || output.phase != phase
            || !schools.insert(output.school_id)
        {
            bail!("invalid, duplicate, or cross-run finance school output");
        }
    }
    let expected: BTreeSet<_> = SCHOOL_BINDINGS.iter().map(|(_, school)| *school).collect();
    if schools != expected {
        bail!("finance output set does not contain the fixed five schools");
    }
    Ok(())
}

fn blocking_disposition(disposition: SchoolDisposition) -> bool {
    !matches!(
        disposition,
        SchoolDisposition::Clear | SchoolDisposition::NotApplicable
    )
}

fn rank(disposition: SchoolDisposition) -> u8 {
    match disposition {
        SchoolDisposition::Clear => 0,
        SchoolDisposition::NotApplicable => 1,
        SchoolDisposition::InsufficientEvidence => 2,
        SchoolDisposition::Contradicted => 3,
        SchoolDisposition::Quarantined => 4,
        SchoolDisposition::Expired => 5,
    }
}

pub fn conservative_fold(
    profile: &FinanceReviewProfile,
    manifest: &FinanceClaimManifest,
    r1: &[SchoolLaneOutput],
    r2: &[SchoolLaneOutput],
    admitted_closure_evidence: &BTreeSet<String>,
) -> anyhow::Result<Vec<FoldedSchoolDecision>> {
    let run_id = r1.first().context("missing R1 outputs")?.run_id.as_str();
    validate_school_output_set(run_id, FinancePhase::R1, r1)?;
    validate_school_output_set(run_id, FinancePhase::R2, r2)?;
    fn by_phase(outputs: &[SchoolLaneOutput]) -> BTreeMap<SchoolId, &SchoolLaneOutput> {
        outputs
            .iter()
            .map(|output| (output.school_id, output))
            .collect::<BTreeMap<_, _>>()
    }
    let r1 = by_phase(r1);
    let r2 = by_phase(r2);
    let mut folded = Vec::new();
    for claim in &manifest.claims {
        for (_, school) in SCHOOL_BINDINGS {
            let applicability = claim
                .school_applicability
                .get(&school)
                .context("missing applicability")?;
            let first = r1[&school]
                .decisions
                .iter()
                .find(|item| item.claim_id == claim.claim_id)
                .context("R1 output is missing a preregistered claim")?;
            let second = r2[&school]
                .decisions
                .iter()
                .find(|item| item.claim_id == claim.claim_id)
                .context("R2 output is missing a preregistered claim")?;
            let can_close = |item: &SchoolClaimDecision| {
                item.closure_rule_code
                    .as_ref()
                    .is_some_and(|rule| profile.closure_rule_codes.contains(rule))
                    && !item.closure_evidence_refs.is_empty()
                    && item
                        .closure_evidence_refs
                        .iter()
                        .all(|reference| admitted_closure_evidence.contains(reference))
            };
            let unresolved = [first, second]
                .into_iter()
                .filter(|item| blocking_disposition(item.disposition) && !can_close(item))
                .max_by_key(|item| rank(item.disposition));
            let mut disposition =
                unresolved.map_or(SchoolDisposition::Clear, |item| item.disposition);
            let effective = applicability.mode == ApplicabilityMode::Mandatory
                || (applicability.mode == ApplicabilityMode::Conditional
                    && applicability.predicate_result == Some(true));
            if !effective {
                if first.disposition != SchoolDisposition::NotApplicable
                    || second.disposition != SchoolDisposition::NotApplicable
                {
                    bail!("inapplicable school must return not_applicable in both phases");
                }
                disposition = SchoolDisposition::NotApplicable;
            } else if first.disposition == SchoolDisposition::NotApplicable
                || second.disposition == SchoolDisposition::NotApplicable
            {
                bail!("applicable school cannot return not_applicable");
            }
            folded.push(FoldedSchoolDecision {
                claim_id: claim.claim_id.clone(),
                school_id: school,
                applicability: applicability.mode,
                disposition,
                blocking: effective && disposition != SchoolDisposition::Clear,
            });
        }
    }
    Ok(folded)
}

pub fn publication_posture(
    primary: &PrimaryAuthority,
    evidence: &FinanceEvidenceIndex,
    folded: &[FoldedSchoolDecision],
    open_material_residuals: &[String],
    active_invalidations: &[String],
) -> PublicationDecision {
    let mut reasons = BTreeSet::new();
    if primary.status != EvidenceStatus::Accepted {
        reasons.insert(PostureReason::PrimaryNotAccepted);
    }
    if primary.evaluation_session > primary.expiry_session {
        reasons.insert(PostureReason::PrimaryExpired);
    }
    if !primary.provenance_complete {
        reasons.insert(PostureReason::PrimaryProvenanceIncomplete);
    }
    if evidence.items.iter().any(|item| !item.provenance_complete) {
        reasons.insert(PostureReason::EvidenceProvenanceIncomplete);
    }
    for decision in folded {
        match decision.applicability {
            ApplicabilityMode::Mandatory if decision.disposition != SchoolDisposition::Clear => {
                reasons.insert(PostureReason::MandatorySchoolNotClear);
            }
            ApplicabilityMode::Conditional if decision.blocking => {
                reasons.insert(PostureReason::ConditionalSchoolNotClear);
            }
            ApplicabilityMode::Mandatory
                if decision.disposition == SchoolDisposition::NotApplicable =>
            {
                reasons.insert(PostureReason::InvalidNotApplicable);
            }
            _ => {}
        }
    }
    if !open_material_residuals.is_empty() {
        reasons.insert(PostureReason::OpenMaterialResidual);
    }
    if !active_invalidations.is_empty() {
        reasons.insert(PostureReason::ActiveInvalidation);
    }
    PublicationDecision {
        function_revision: "1.0".into(),
        posture: if reasons.is_empty() {
            PublicationPosture::PublishBounded
        } else {
            PublicationPosture::Abstain
        },
        reason_codes: reasons.into_iter().collect(),
    }
}

pub fn highball_carriers(
    result: &FinanceReviewResult,
    result_binding: PortableBinding,
) -> (HighballRouteRequest, HighballResidualTrace) {
    (
        HighballRouteRequest {
            carrier_version: "1.0".into(),
            source_result: result_binding.clone(),
            requested_route: "HIGHBALL".into(),
            publication_posture: result.publication.posture,
        },
        HighballResidualTrace {
            carrier_version: "1.0".into(),
            source_result: result_binding,
            residual_codes: result.open_material_residual_codes.clone(),
            invalidation_codes: result.active_invalidation_codes.clone(),
            folded_decisions: result.folded_decisions.clone(),
        },
    )
}

fn exact_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn load_typed<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<(T, Vec<u8>, Value)> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let value = parse_strict_json(&bytes).with_context(|| format!("invalid {}", path.display()))?;
    let typed = serde_json::from_value(value.clone()).with_context(|| {
        format!(
            "{} does not match its closed finance contract",
            path.display()
        )
    })?;
    Ok((typed, bytes, value))
}

fn verify_portable_binding(
    binding: &PortableBinding,
    bytes: &[u8],
    value: &Value,
    expected_family: &str,
    expected_revision: &str,
    expected_domain: &str,
) -> anyhow::Result<()> {
    validate_portable_relative_path(&binding.artifact_ref)?;
    if binding.contract_family != expected_family
        || binding.revision != expected_revision
        || binding.semantic_domain != expected_domain
        || (expected_family == "finance-review-result"
            && binding.schema_id.as_deref() != Some(FINANCE_RESULT_SCHEMA_ID))
    {
        bail!(
            "portable binding contract identity mismatch for {}",
            binding.artifact_ref
        );
    }
    if binding.exact_sha256 != exact_digest(bytes)
        || binding.semantic_sha256 != semantic_digest(expected_domain, value)?
    {
        bail!(
            "portable binding digest mismatch for {}",
            binding.artifact_ref
        );
    }
    Ok(())
}

fn validate_portable_relative_path(reference: &str) -> anyhow::Result<()> {
    if reference.is_empty()
        || reference.contains('\\')
        || reference.contains(':')
        || reference.contains('\0')
        || reference.starts_with('/')
        || reference
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        bail!("artifact_ref must be a portable relative path");
    }
    let path = Path::new(reference);
    if path.components().any(|component| {
        !matches!(component, Component::Normal(_))
            || matches!(component, Component::Normal(value) if value.is_empty())
    }) {
        bail!("artifact_ref must contain only portable normal path components");
    }
    Ok(())
}

fn require_artifact_ref(binding: &PortableBinding, expected: &str) -> anyhow::Result<()> {
    validate_portable_relative_path(&binding.artifact_ref)?;
    if binding.artifact_ref != expected {
        bail!("portable binding is assigned to the wrong manifest slot");
    }
    Ok(())
}

fn validate_evidence_artifacts(
    input: &Path,
    evidence: &FinanceEvidenceIndex,
) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut seen_refs = BTreeSet::new();
    let mut seen_paths = BTreeSet::new();
    let mut documents = Vec::new();
    for item in &evidence.items {
        if !seen_refs.insert(&item.evidence_ref) {
            bail!("finance evidence index contains duplicate evidence_ref");
        }
        validate_portable_relative_path(&item.binding.artifact_ref)?;
        if !item.binding.artifact_ref.starts_with("evidence/")
            || !seen_paths.insert(item.binding.artifact_ref.clone())
        {
            bail!("finance evidence artifact must have a unique evidence/ relative path");
        }
        let bytes = std::fs::read(input.join(&item.binding.artifact_ref))?;
        let value = parse_strict_json(&bytes)?;
        verify_portable_binding(
            &item.binding,
            &bytes,
            &value,
            &item.binding.contract_family.clone(),
            &item.binding.revision.clone(),
            &item.binding.semantic_domain.clone(),
        )?;
        documents.push((item.binding.artifact_ref.clone(), bytes));
    }
    let actual = list_regular_files(&input.join("evidence"))?
        .into_iter()
        .map(|relative| format!("evidence/{relative}"))
        .collect::<BTreeSet<_>>();
    if actual != seen_paths {
        bail!("finance evidence tree must equal the closed evidence index exactly");
    }
    Ok(documents)
}

fn list_regular_files(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    fn walk(root: &Path, current: &Path, files: &mut BTreeSet<String>) -> anyhow::Result<()> {
        if !current.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                bail!("finance immutable input tree contains a non-portable entry");
            }
            if kind.is_dir() {
                walk(root, &path, files)?;
            } else {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                validate_portable_relative_path(&relative)?;
                files.insert(relative);
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

fn verify_source_replay(source: &Path, state: &Path) -> anyhow::Result<()> {
    let fixed = [
        "policy.json",
        "profile.json",
        "claim-manifest.json",
        "evidence-index.json",
        "invocation.json",
        "primary.json",
    ];
    for relative in fixed {
        let source_bytes = std::fs::read(source.join(relative))
            .with_context(|| format!("immutable replay source is missing {relative}"))?;
        let stored_bytes = std::fs::read(state.join("input").join(relative))?;
        if source_bytes != stored_bytes {
            bail!("immutable finance source differs on replay: {relative}");
        }
    }
    let (evidence, _, _) =
        load_typed::<FinanceEvidenceIndex>(&state.join("input/evidence-index.json"))?;
    let expected = evidence
        .items
        .iter()
        .map(|item| item.binding.artifact_ref.clone())
        .collect::<BTreeSet<_>>();
    let source_files = list_regular_files(&source.join("evidence"))?
        .into_iter()
        .map(|relative| format!("evidence/{relative}"))
        .collect::<BTreeSet<_>>();
    if source_files != expected {
        bail!("immutable replay source evidence tree differs from its closed evidence index");
    }
    for relative in expected {
        if std::fs::read(source.join(&relative))?
            != std::fs::read(state.join("input").join(&relative))?
        {
            bail!("immutable finance evidence differs on replay: {relative}");
        }
    }
    Ok(())
}

fn list_bundle_files(root: &Path) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        current: &Path,
        output: &mut BTreeMap<String, Vec<u8>>,
    ) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                bail!("finance terminal bundle contains a non-portable filesystem entry");
            }
            if kind.is_dir() {
                walk(root, &path, output)?;
            } else {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                validate_portable_relative_path(&relative)?;
                output.insert(relative, std::fs::read(path)?);
            }
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    walk(root, root, &mut output)?;
    Ok(output)
}

fn compare_bundle_trees(expected: &Path, actual: &Path) -> anyhow::Result<()> {
    if list_bundle_files(expected)? != list_bundle_files(actual)? {
        bail!("existing finance terminal bundle is not the deterministic bundle for this run");
    }
    Ok(())
}

fn binding_for(
    artifact_ref: impl Into<String>,
    family: &str,
    revision: &str,
    domain: &str,
    bytes: &[u8],
    value: &Value,
) -> anyhow::Result<PortableBinding> {
    let artifact_ref = artifact_ref.into();
    validate_portable_relative_path(&artifact_ref)?;
    Ok(PortableBinding {
        artifact_ref,
        contract_family: family.into(),
        schema_id: (family == "finance-review-result").then(|| FINANCE_RESULT_SCHEMA_ID.into()),
        revision: revision.into(),
        exact_sha256: exact_digest(bytes),
        semantic_domain: domain.into(),
        semantic_sha256: semantic_digest(domain, value)?,
    })
}

/// Deterministically finalizes a completed ten-output finance review bundle.
///
/// The scheduler/provider boundary is intentionally outside this function.
/// R1 files are loaded and validated before any R2 file is opened, preserving
/// process-level information-flow sequencing. Each provider attempt must be
/// launched with only its generated packet bytes and output sink; this is not
/// a claim of kernel-enforced filesystem sandboxing.
pub fn finalize_bundle(input: &Path, output: &Path) -> anyhow::Result<FinalizedFinanceBundle> {
    let (policy, policy_bytes, policy_value) =
        load_typed::<FinancePolicy>(&input.join("policy.json"))?;
    validate_finance_policy(&policy)?;
    crate::schema::validate_value(&policy_value, crate::contract::FINANCE_POLICY_SCHEMA)?;

    let (profile, profile_bytes, profile_value) =
        load_typed::<FinanceReviewProfile>(&input.join("profile.json"))?;
    validate_profile(&profile)?;
    verify_portable_binding(
        &policy.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;

    let (claims, claims_bytes, claims_value) =
        load_typed::<FinanceClaimManifest>(&input.join("claim-manifest.json"))?;
    validate_claim_manifest(&profile, &claims)?;
    let (evidence, evidence_bytes, evidence_value) =
        load_typed::<FinanceEvidenceIndex>(&input.join("evidence-index.json"))?;
    if evidence.finance_evidence_index_version != "1.0" {
        bail!("finance_evidence_index_version must be 1.0");
    }
    let evidence_documents = validate_evidence_artifacts(input, &evidence)?;
    let (invocation, invocation_bytes, invocation_value) =
        load_typed::<FinanceReviewInvocation>(&input.join("invocation.json"))?;
    if invocation.finance_review_invocation_version != "1.0" {
        bail!("finance_review_invocation_version must be 1.0");
    }
    verify_portable_binding(
        &invocation.profile,
        &profile_bytes,
        &profile_value,
        "finance-review-profile",
        "1.0",
        "quinte.finance-review-profile.v1",
    )?;
    verify_portable_binding(
        &invocation.claim_manifest,
        &claims_bytes,
        &claims_value,
        "finance-claim-manifest",
        "1.0",
        "quinte.finance-claim-manifest.v1",
    )?;
    verify_portable_binding(
        &invocation.evidence_index,
        &evidence_bytes,
        &evidence_value,
        "finance-evidence-index",
        "1.0",
        "quinte.finance-evidence-index.v1",
    )?;
    let primary_bytes = std::fs::read(input.join("primary.json"))?;
    let primary_value = parse_strict_json(&primary_bytes)?;
    if !profile
        .allowed_primary_contracts
        .iter()
        .any(|family| family == &invocation.primary.binding.contract_family)
        || !profile
            .hash_domains
            .iter()
            .any(|domain| domain == &invocation.primary.binding.semantic_domain)
    {
        bail!("primary artifact family or semantic domain is not allowed by the pinned profile");
    }
    verify_portable_binding(
        &invocation.primary.binding,
        &primary_bytes,
        &primary_value,
        &invocation.primary.binding.contract_family.clone(),
        &invocation.primary.binding.revision.clone(),
        &invocation.primary.binding.semantic_domain.clone(),
    )?;
    let mut input_documents = vec![
        ("policy.json".into(), policy_bytes.clone()),
        ("profile.json".into(), profile_bytes.clone()),
        ("claim-manifest.json".into(), claims_bytes.clone()),
        ("evidence-index.json".into(), evidence_bytes.clone()),
        ("invocation.json".into(), invocation_bytes.clone()),
        ("primary.json".into(), primary_bytes.clone()),
    ];
    input_documents.extend(evidence_documents);

    let school_name = |school: SchoolId| {
        serde_json::to_value(school)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut r1 = Vec::new();
    let mut r1_bindings = Vec::new();
    let mut r1_output_documents = Vec::new();
    for (_, school) in SCHOOL_BINDINGS {
        let name = school_name(school);
        let path = input.join("r1").join(format!("{name}.json"));
        let (lane, bytes, value) = load_typed::<SchoolLaneOutput>(&path)?;
        if lane.school_id != school {
            bail!("R1 school output filename/seat binding mismatch");
        }
        crate::schema::validate_value(&value, crate::contract::SCHOOL_LANE_OUTPUT_SCHEMA)?;
        let binding = binding_for(
            format!("r1/{name}.json"),
            "school-lane-output",
            "1.0",
            "quinte.school-lane-output.v1",
            &bytes,
            &value,
        )?;
        r1.push(lane);
        r1_bindings.push(binding);
        r1_output_documents.push((format!("r1/{name}.json"), bytes));
    }
    validate_school_output_set(&invocation.invocation_id, FinancePhase::R1, &r1)?;

    let invocation_binding = binding_for(
        "input/invocation.json",
        "finance-review-invocation",
        "1.0",
        "quinte.finance-review-invocation.v1",
        &invocation_bytes,
        &invocation_value,
    )?;
    let policy_binding = binding_for(
        "input/policy.json",
        "policy",
        "3.0",
        "quinte.finance-policy.v3",
        &policy_bytes,
        &policy_value,
    )?;
    let mut r1_packets = Vec::new();
    let mut r1_packet_documents = Vec::new();
    for authority in &profile.schools {
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == authority.school_id)
            .context("profile school has no Policy 3.0 route")?;
        let packet = build_r1_packet(
            &invocation.invocation_id,
            invocation_binding.clone(),
            policy_binding.clone(),
            invocation.profile.clone(),
            invocation.claim_manifest.clone(),
            invocation.primary.binding.clone(),
            invocation.evidence_index.clone(),
            semantic_digest_value("quinte.finance-route-binding.v1", route)?,
            authority,
        );
        let value = serde_json::to_value(&packet)?;
        crate::schema::validate_value(&value, crate::contract::FINANCE_TASK_PACKET_SCHEMA)?;
        let bytes = serde_json::to_vec_pretty(&packet)?;
        let name = serde_json::to_value(authority.school_id)?
            .as_str()
            .unwrap()
            .to_string();
        let relative = format!("packets/r1/{name}.json");
        let output = r1
            .iter()
            .find(|output| output.school_id == authority.school_id)
            .context("R1 packet has no corresponding school output")?;
        bind_output_to_packet(output, &packet, "quinte.finance-task-packet.v2")?;
        r1_packets.push(binding_for(
            &relative,
            "task-packet",
            "2.0",
            "quinte.finance-task-packet.v2",
            &bytes,
            &value,
        )?);
        r1_packet_documents.push((relative, bytes));
    }

    let mut r2_packets = Vec::new();
    let mut generated_r2_packets = Vec::new();
    let mut r2_packet_documents = Vec::new();
    for authority in &profile.schools {
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == authority.school_id)
            .context("profile school has no Policy 3.0 route")?;
        let packet = build_r2_packet(
            &invocation.invocation_id,
            authority,
            &claims.claims,
            &r1,
            &r1_bindings,
            policy_binding.clone(),
            invocation.profile.clone(),
            invocation.claim_manifest.clone(),
            invocation.primary.binding.clone(),
            invocation.evidence_index.clone(),
            semantic_digest_value("quinte.finance-route-binding.v1", route)?,
        )?;
        let value = serde_json::to_value(&packet)?;
        crate::schema::validate_value(&value, crate::contract::FINANCE_R2_PACKET_SCHEMA)?;
        let bytes = serde_json::to_vec_pretty(&packet)?;
        let name = serde_json::to_value(authority.school_id)?
            .as_str()
            .unwrap()
            .to_string();
        let relative = format!("packets/r2/{name}.json");
        r2_packets.push(binding_for(
            &relative,
            "r2-packet",
            "2.0",
            "quinte.finance-r2-packet.v2",
            &bytes,
            &value,
        )?);
        r2_packet_documents.push((relative, bytes));
        generated_r2_packets.push(packet);
    }

    let mut r2 = Vec::new();
    let mut r2_bindings = Vec::new();
    let mut r2_output_documents = Vec::new();
    for (_, school) in SCHOOL_BINDINGS {
        let name = school_name(school);
        let path = input.join("r2").join(format!("{name}.json"));
        let (lane, bytes, value) = load_typed::<SchoolLaneOutput>(&path)?;
        if lane.school_id != school {
            bail!("R2 school output filename/seat binding mismatch");
        }
        crate::schema::validate_value(&value, crate::contract::SCHOOL_LANE_OUTPUT_SCHEMA)?;
        let binding = binding_for(
            format!("r2/{name}.json"),
            "school-lane-output",
            "1.0",
            "quinte.school-lane-output.v1",
            &bytes,
            &value,
        )?;
        r2.push(lane);
        r2_bindings.push(binding);
        r2_output_documents.push((format!("r2/{name}.json"), bytes));
    }
    validate_school_output_set(&invocation.invocation_id, FinancePhase::R2, &r2)?;
    for output in &r2 {
        let packet = generated_r2_packets
            .iter()
            .find(|packet| packet.recipient_authority.school_id == output.school_id)
            .context("R2 output has no corresponding packet")?;
        bind_output_to_packet(output, packet, "quinte.finance-r2-packet.v2")?;
    }
    validate_lane_claims_and_evidence(&profile, &claims, &evidence, &r1)?;
    validate_lane_claims_and_evidence(&profile, &claims, &evidence, &r2)?;
    for output in r1.iter().chain(&r2) {
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == output.school_id)
            .context("school output has no Policy 3.0 route")?;
        let expected_route_digest =
            semantic_digest_value("quinte.finance-route-binding.v1", route)?;
        if output.profile_digest != invocation.profile.semantic_sha256
            || output.primary_digest != invocation.primary.binding.semantic_sha256
            || output.evidence_index_digest != invocation.evidence_index.semantic_sha256
            || output.expected_route_digest != expected_route_digest
        {
            bail!("school output immutable input binding mismatch");
        }
    }

    let admitted = evidence
        .items
        .iter()
        .filter(|item| item.status == EvidenceStatus::Accepted)
        .map(|item| item.evidence_ref.clone())
        .collect();
    let folded = conservative_fold(&profile, &claims, &r1, &r2, &admitted)?;
    let mut residuals = BTreeSet::new();
    let mut invalidations = BTreeSet::new();
    for output in r1.iter().chain(&r2) {
        for residual in &output.residuals {
            if residual.materiality == Materiality::Material
                && residual.closure_state == FinanceClosureState::Open
            {
                residuals.insert(residual.residual_code.clone());
            }
        }
        for decision in &output.decisions {
            invalidations.extend(decision.invalidation_codes.iter().cloned());
        }
    }
    let residuals = residuals.into_iter().collect::<Vec<_>>();
    let invalidations = invalidations.into_iter().collect::<Vec<_>>();
    let publication = publication_posture(
        &invocation.primary,
        &evidence,
        &folded,
        &residuals,
        &invalidations,
    );
    let terminal_input_binding = |mut binding: PortableBinding| {
        binding.artifact_ref = format!("input/{}", binding.artifact_ref);
        binding
    };
    let profile_binding = terminal_input_binding(invocation.profile.clone());
    let claim_binding = terminal_input_binding(invocation.claim_manifest.clone());
    let evidence_binding = terminal_input_binding(invocation.evidence_index.clone());
    let mut result_primary = invocation.primary.clone();
    result_primary.binding = terminal_input_binding(result_primary.binding);
    let mut school_outputs = r1_bindings.clone();
    school_outputs.extend(r2_bindings.clone());
    let school_output_digests = school_outputs
        .iter()
        .map(|binding| binding.semantic_sha256.clone())
        .collect::<Vec<_>>();
    let policy_semantic_digest = semantic_digest("quinte.finance-policy.v3", &policy_value)?;
    let invocation_semantic_digest =
        semantic_digest("quinte.finance-review-invocation.v1", &invocation_value)?;
    let route_binding_digests = policy
        .school_bindings
        .iter()
        .map(|route| semantic_digest_value("quinte.finance-route-binding.v1", route))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .chain([
            semantic_digest_value(
                "quinte.finance-arbiter-route-binding.v1",
                &policy.counterpart_arbiter,
            )?,
            semantic_digest_value(
                "quinte.finance-arbiter-route-binding.v1",
                &policy.primary_arbiter,
            )?,
        ])
        .collect::<Vec<_>>();
    let admitted_evidence_refs = evidence
        .items
        .iter()
        .filter(|item| item.status == EvidenceStatus::Accepted)
        .map(|item| item.evidence_ref.as_str())
        .collect::<BTreeSet<_>>();
    let mut arbiter_bindings = Vec::new();
    let mut arbiter_documents = Vec::new();
    for (role, file) in [
        ("counterpart_arbiter", "counterpart-arbiter.json"),
        ("primary_arbiter", "primary-arbiter.json"),
    ] {
        let path = input.join("arbiters").join(file);
        let (verdict, bytes, value) = load_typed::<FinanceArbiterVerdict>(&path)?;
        crate::schema::validate_value(&value, crate::contract::FINANCE_ARBITER_VERDICT_SCHEMA)?;
        if verdict.finance_arbiter_verdict_version != "1.0"
            || verdict.run_id != invocation.invocation_id
            || verdict.arbiter_role != role
            || verdict.policy_digest != policy_semantic_digest
            || verdict.invocation_digest != invocation_semantic_digest
            || verdict.profile_digest != invocation.profile.semantic_sha256
            || verdict.claim_manifest_digest != invocation.claim_manifest.semantic_sha256
            || verdict.primary_digest != invocation.primary.binding.semantic_sha256
            || verdict.evidence_index_digest != invocation.evidence_index.semantic_sha256
            || verdict.school_output_digests != school_output_digests
            || verdict.route_binding_digests != route_binding_digests
            || verdict
                .admitted_closure_evidence_refs
                .iter()
                .any(|reference| !admitted_evidence_refs.contains(reference.as_str()))
        {
            bail!("restricted finance arbiter artifact violates its bound authority");
        }
        arbiter_bindings.push(binding_for(
            format!("arbiters/{file}"),
            "finance-arbiter-verdict",
            "1.0",
            "quinte.finance-arbiter-verdict.v1",
            &bytes,
            &value,
        )?);
        arbiter_documents.push((format!("arbiters/{file}"), bytes));
    }
    let run_genesis_digest = compute_run_genesis(
        &invocation.invocation_id,
        &policy,
        &invocation,
        &policy_binding,
        &invocation_binding,
        &r1_packets,
    )?;
    let result = FinanceReviewResult {
        finance_review_result_version: "1.0".into(),
        run_id: invocation.invocation_id.clone(),
        run_genesis_digest: run_genesis_digest.clone(),
        primary: result_primary,
        profile: profile_binding,
        claim_manifest: claim_binding,
        evidence_index: evidence_binding,
        school_outputs,
        arbiter_outputs: arbiter_bindings.clone(),
        folded_decisions: folded,
        open_material_residual_codes: residuals,
        active_invalidation_codes: invalidations,
        publication,
        route_bindings: policy
            .school_bindings
            .iter()
            .map(|route| route.route_id.clone())
            .chain([
                policy.counterpart_arbiter.route_id.clone(),
                policy.primary_arbiter.route_id.clone(),
            ])
            .collect(),
        contamination_risks: vec!["same_family_error_correlation".into()],
    };
    let result_value = serde_json::to_value(&result)?;
    crate::schema::validate_value(&result_value, crate::contract::FINANCE_REVIEW_RESULT_SCHEMA)?;
    let result_bytes = serde_json::to_vec_pretty(&result)?;
    let result_binding = binding_for(
        "result.json",
        "finance-review-result",
        "1.0",
        "quinte.finance-review-result.v1",
        &result_bytes,
        &result_value,
    )?;
    let mut manifest = FinanceRunManifest {
        manifest_version: FINANCE_MANIFEST_VERSION.into(),
        protocol_version: FINANCE_PROTOCOL_VERSION.into(),
        run_id: result.run_id.clone(),
        run_genesis_digest,
        event_checkpoint: placeholder_checkpoint(),
        status: FinanceRunStatus::Completed,
        termination: None,
        policy: policy_binding,
        invocation: invocation_binding,
        r1_packets,
        r2_packets,
        r1_outputs: r1_bindings,
        r2_outputs: r2_bindings,
        arbiter_outputs: arbiter_bindings,
        result: Some(result_binding.clone()),
    };
    let (event_bytes, checkpoint) = expected_event_ledger(&manifest)?;
    manifest.event_checkpoint = checkpoint;
    let manifest_value = serde_json::to_value(&manifest)?;
    crate::schema::validate_value(
        &manifest_value,
        crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
    )?;
    let (route, trace) = highball_carriers(&result, result_binding);
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".finance-terminal.")
        .tempdir_in(parent)?;
    let staging_root = staging.path();
    for (file, bytes) in input_documents {
        crate::util::atomic_write(&staging_root.join("input").join(file), &bytes)?;
    }
    let mut r1_packet_paths = Vec::new();
    for (relative, bytes) in r1_packet_documents {
        let path = staging_root.join(&relative);
        crate::util::atomic_write(&path, &bytes)?;
        r1_packet_paths.push(output.join(relative));
    }
    let mut r2_packet_paths = Vec::new();
    for (relative, bytes) in r2_packet_documents {
        let path = staging_root.join(&relative);
        crate::util::atomic_write(&path, &bytes)?;
        r2_packet_paths.push(output.join(relative));
    }
    for (relative, bytes) in r1_output_documents
        .into_iter()
        .chain(r2_output_documents)
        .chain(arbiter_documents)
    {
        crate::util::atomic_write(&staging_root.join(relative), &bytes)?;
    }
    let result_path = output.join("result.json");
    let manifest_path = output.join("manifest.json");
    let highball_route_request_path = output.join("highball.route-request.json");
    let highball_residual_trace_path = output.join("highball.residual-trace.json");
    crate::util::atomic_write(&staging_root.join("result.json"), &result_bytes)?;
    crate::util::atomic_write(&staging_root.join("events.jsonl"), &event_bytes)?;
    crate::util::atomic_write(
        &staging_root.join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    crate::util::atomic_write(
        &staging_root.join("highball.route-request.json"),
        &serde_json::to_vec_pretty(&route)?,
    )?;
    crate::util::atomic_write(
        &staging_root.join("highball.residual-trace.json"),
        &serde_json::to_vec_pretty(&trace)?,
    )?;
    verify_bundle_shallow(staging_root)?;
    if output.exists() {
        compare_bundle_trees(staging_root, output)?;
        let verified = verify_bundle(output)?;
        return Ok(FinalizedFinanceBundle {
            result_path,
            manifest_path,
            highball_route_request_path,
            highball_residual_trace_path,
            r1_packet_paths,
            r2_packet_paths,
            publication_posture: verified.publication_posture,
        });
    }
    let staging_path = staging.keep();
    std::fs::rename(&staging_path, output).with_context(|| {
        format!(
            "cannot atomically publish finance terminal bundle {}",
            output.display()
        )
    })?;
    Ok(FinalizedFinanceBundle {
        result_path,
        manifest_path,
        highball_route_request_path,
        highball_residual_trace_path,
        r1_packet_paths,
        r2_packet_paths,
        publication_posture: result.publication.posture,
    })
}

pub fn finalize_bundle_with_ack(
    input: &Path,
    output: &Path,
    acknowledgement: &str,
) -> anyhow::Result<FinalizedFinanceBundle> {
    require_dormant_writer_ack(acknowledgement)?;
    finalize_bundle(input, output)
}

/// Offline verification for a terminal finance bundle. This does not mutate,
/// reconcile, or resume state and is safe for readers and A2A consumers.
pub fn verify_bundle(output: &Path) -> anyhow::Result<FinanceVerification> {
    let verified = verify_bundle_shallow(output)?;
    let temporary = tempfile::tempdir()?;
    let replay_input = temporary.path().join("input");
    let replay_output = temporary.path().join("terminal");
    for (relative, bytes) in list_bundle_files(output)? {
        let target = if let Some(input_relative) = relative.strip_prefix("input/") {
            Some(replay_input.join(input_relative))
        } else if relative.starts_with("r1/")
            || relative.starts_with("r2/")
            || relative.starts_with("arbiters/")
        {
            Some(replay_input.join(&relative))
        } else {
            None
        };
        if let Some(target) = target {
            crate::util::atomic_write(&target, &bytes)?;
        }
    }
    finalize_bundle(&replay_input, &replay_output)
        .context("terminal finance bundle cannot be reconstructed from its offline inputs")?;
    compare_bundle_trees(&replay_output, output)?;
    Ok(verified)
}

fn verify_bundle_shallow(output: &Path) -> anyhow::Result<FinanceVerification> {
    let (manifest, _, manifest_value) =
        load_typed::<FinanceRunManifest>(&output.join("manifest.json"))?;
    crate::schema::validate_value(
        &manifest_value,
        crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
    )?;
    if manifest.manifest_version != FINANCE_MANIFEST_VERSION
        || manifest.protocol_version != FINANCE_PROTOCOL_VERSION
        || !matches!(
            manifest.status,
            FinanceRunStatus::Completed | FinanceRunStatus::Degraded
        )
    {
        bail!("bundle is not a supported terminal finance run");
    }
    let result_binding = manifest
        .result
        .as_ref()
        .context("terminal finance manifest has no result binding")?;
    require_artifact_ref(&manifest.policy, "input/policy.json")?;
    require_artifact_ref(&manifest.invocation, "input/invocation.json")?;
    require_artifact_ref(result_binding, "result.json")?;
    let (result, result_bytes, result_value) =
        load_typed::<FinanceReviewResult>(&output.join("result.json"))?;
    crate::schema::validate_value(&result_value, crate::contract::FINANCE_REVIEW_RESULT_SCHEMA)?;
    verify_portable_binding(
        result_binding,
        &result_bytes,
        &result_value,
        "finance-review-result",
        "1.0",
        "quinte.finance-review-result.v1",
    )?;
    if result.run_id != manifest.run_id || result.run_genesis_digest != manifest.run_genesis_digest
    {
        bail!("finance result run or genesis identity differs from Manifest 3.0");
    }
    verify_expected_event_ledger(output, &manifest)?;
    let mut expected_slots = Vec::new();
    for (_, school) in SCHOOL_BINDINGS {
        let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
        expected_slots.push((
            format!("packets/r1/{name}.json"),
            "task-packet",
            "2.0",
            "quinte.finance-task-packet.v2",
        ));
    }
    for (_, school) in SCHOOL_BINDINGS {
        let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
        expected_slots.push((
            format!("packets/r2/{name}.json"),
            "r2-packet",
            "2.0",
            "quinte.finance-r2-packet.v2",
        ));
    }
    for phase in ["r1", "r2"] {
        for (_, school) in SCHOOL_BINDINGS {
            let name = serde_json::to_value(school)?.as_str().unwrap().to_string();
            expected_slots.push((
                format!("{phase}/{name}.json"),
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
            ));
        }
    }
    for file in ["counterpart-arbiter.json", "primary-arbiter.json"] {
        expected_slots.push((
            format!("arbiters/{file}"),
            "finance-arbiter-verdict",
            "1.0",
            "quinte.finance-arbiter-verdict.v1",
        ));
    }
    let bindings = manifest
        .r1_packets
        .iter()
        .chain(&manifest.r2_packets)
        .chain(&manifest.r1_outputs)
        .chain(&manifest.r2_outputs)
        .chain(&manifest.arbiter_outputs)
        .collect::<Vec<_>>();
    if bindings.len() != expected_slots.len() {
        bail!("terminal finance manifest has an incomplete artifact set");
    }
    for (binding, (expected_ref, family, revision, domain)) in
        bindings.into_iter().zip(expected_slots)
    {
        require_artifact_ref(binding, &expected_ref)?;
        let bytes = std::fs::read(output.join(&binding.artifact_ref))?;
        let value = parse_strict_json(&bytes)?;
        verify_portable_binding(binding, &bytes, &value, family, revision, domain)?;
    }
    let expected = highball_carriers(&result, result_binding.clone());
    let (route, _, _) =
        load_typed::<HighballRouteRequest>(&output.join("highball.route-request.json"))?;
    let (trace, _, _) =
        load_typed::<HighballResidualTrace>(&output.join("highball.residual-trace.json"))?;
    if (route, trace) != expected {
        bail!("HIGHBALL carriers do not derive exactly from the bound finance result");
    }
    Ok(FinanceVerification {
        verification_version: "1.0",
        run_id: result.run_id,
        manifest_version: manifest.manifest_version,
        result_contract: "finance-review-result/1.0",
        publication_posture: result.publication.posture,
        result_exact_sha256: result_binding.exact_sha256.clone(),
        result_semantic_sha256: result_binding.semantic_sha256.clone(),
        highball_carriers_verified: true,
        // Production scheduler/store creation remains dormant until its
        // writer-capability, resume, reconcile, and A2A raw-byte carrier are
        // implemented and enabled together.
        finance_creation_enabled: false,
    })
}

/// Strict JSON parser used at every finance ingress/provider boundary.
/// It rejects invalid UTF-8 and duplicate members at any nesting depth.
pub fn parse_strict_json(bytes: &[u8]) -> anyhow::Result<Value> {
    let text = std::str::from_utf8(bytes).context("finance payload is not strict UTF-8")?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = StrictValue::deserialize(&mut deserializer)
        .context("finance payload is not strict duplicate-free JSON")?;
    deserializer
        .end()
        .context("finance payload has trailing data")?;
    Ok(value.0)
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a duplicate-free JSON value")
            }
            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(value)))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(StrictValue)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value.into())))
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value)))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictValue>()? {
                    values.push(value.0);
                }
                Ok(StrictValue(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate object member: {key}"
                        )));
                    }
                    values.insert(key, map.next_value::<StrictValue>()?.0);
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Canonicalizes the finance JSON profile: UTF-8, sorted object member names,
/// no insignificant whitespace, and finite JSON numbers. Finance contracts
/// prohibit unsafe integer magnitudes and negative zero, making this complete
/// restricted profile deterministic across conforming consumers.
pub fn canonical_json(value: &Value) -> anyhow::Result<Vec<u8>> {
    fn write(value: &Value, target: &mut Vec<u8>) -> anyhow::Result<()> {
        match value {
            Value::Null => target.extend_from_slice(b"null"),
            Value::Bool(true) => target.extend_from_slice(b"true"),
            Value::Bool(false) => target.extend_from_slice(b"false"),
            Value::Number(number) => {
                let text = number.to_string();
                if text == "-0" {
                    bail!("negative zero is outside the finance canonical profile");
                }
                target.extend_from_slice(text.as_bytes());
            }
            Value::String(text) => {
                target.extend_from_slice(serde_json::to_string(text)?.as_bytes())
            }
            Value::Array(values) => {
                target.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        target.push(b',');
                    }
                    write(value, target)?;
                }
                target.push(b']');
            }
            Value::Object(values) => {
                target.push(b'{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        target.push(b',');
                    }
                    target.extend_from_slice(serde_json::to_string(key)?.as_bytes());
                    target.push(b':');
                    write(&values[key], target)?;
                }
                target.push(b'}');
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    write(value, &mut output)?;
    Ok(output)
}

pub fn semantic_digest(domain: &str, value: &Value) -> anyhow::Result<String> {
    if !domain.is_ascii() || domain.is_empty() || domain.contains('\0') {
        bail!("semantic digest domain must be nonempty ASCII without NUL");
    }
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(canonical_json(value)?);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub fn semantic_digest_value<T: Serialize>(domain: &str, value: &T) -> anyhow::Result<String> {
    semantic_digest(domain, &serde_json::to_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn synthetic_binding(reference: &str, character: char) -> PortableBinding {
        PortableBinding {
            artifact_ref: reference.into(),
            contract_family: "synthetic".into(),
            schema_id: None,
            revision: "1.0".into(),
            exact_sha256: digest(character),
            semantic_domain: "quinte.synthetic.v1".into(),
            semantic_sha256: digest(character),
        }
    }

    fn write_bound<T: Serialize>(
        root: &Path,
        relative: &str,
        value: &T,
        family: &str,
        revision: &str,
        domain: &str,
    ) -> PortableBinding {
        let value = serde_json::to_value(value).unwrap();
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        binding_for(relative, family, revision, domain, &bytes, &value).unwrap()
    }

    fn packet_digests<T: Serialize>(packet: &T, domain: &str) -> (String, String) {
        let value = serde_json::to_value(packet).unwrap();
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        (
            exact_digest(&bytes),
            semantic_digest(domain, &value).unwrap(),
        )
    }

    fn rebind_fixture_r1_output(root: &Path, school: SchoolId, lane: &mut Value) {
        let (policy, policy_bytes, policy_value) =
            load_typed::<FinancePolicy>(&root.join("policy.json")).unwrap();
        let (profile, _, _) =
            load_typed::<FinanceReviewProfile>(&root.join("profile.json")).unwrap();
        let (invocation, invocation_bytes, invocation_value) =
            load_typed::<FinanceReviewInvocation>(&root.join("invocation.json")).unwrap();
        let authority = profile
            .schools
            .iter()
            .find(|authority| authority.school_id == school)
            .unwrap();
        let route = policy
            .school_bindings
            .iter()
            .find(|route| route.school_id == school)
            .unwrap();
        let packet = build_r1_packet(
            &invocation.invocation_id,
            binding_for(
                "input/invocation.json",
                "finance-review-invocation",
                "1.0",
                "quinte.finance-review-invocation.v1",
                &invocation_bytes,
                &invocation_value,
            )
            .unwrap(),
            binding_for(
                "input/policy.json",
                "policy",
                "3.0",
                "quinte.finance-policy.v3",
                &policy_bytes,
                &policy_value,
            )
            .unwrap(),
            invocation.profile.clone(),
            invocation.claim_manifest.clone(),
            invocation.primary.binding.clone(),
            invocation.evidence_index.clone(),
            semantic_digest_value("quinte.finance-route-binding.v1", route).unwrap(),
            authority,
        );
        let (exact, semantic) = packet_digests(&packet, "quinte.finance-task-packet.v2");
        lane["input_packet_exact_sha256"] = Value::String(exact);
        lane["input_packet_semantic_sha256"] = Value::String(semantic);
    }

    fn finance_fixture(root: &Path, material_blocker: bool) {
        finance_fixture_with_run_id(root, material_blocker, "finance-run-1");
    }

    fn finance_fixture_with_run_id(root: &Path, material_blocker: bool, run_id: &str) {
        let authorities = SCHOOL_BINDINGS
            .iter()
            .map(|(party, school)| SchoolAuthority {
                party_id: (*party).into(),
                school_id: *school,
                accepted_evidence_classes: vec!["validated_source".into()],
                forbidden_claim_classes: vec!["price_recalculation".into()],
                question_codes: vec!["falsify_primary".into()],
            })
            .collect::<Vec<_>>();
        let profile = FinanceReviewProfile {
            finance_review_profile_version: "1.0".into(),
            profile_id: "synthetic-finance-v1".into(),
            schools: authorities.clone(),
            allowed_primary_contracts: vec!["galahad-calculation-artifact".into()],
            applicability_predicate_codes: vec!["intraday_claim_present_v1".into()],
            closure_rule_codes: vec!["bound_retest_v1".into()],
            hash_domains: vec!["galahad.calculation-artifact.v1".into()],
        };
        let profile_binding = write_bound(
            root,
            "profile.json",
            &profile,
            "finance-review-profile",
            "1.0",
            "quinte.finance-review-profile.v1",
        );
        let routes = SCHOOL_BINDINGS
            .iter()
            .map(|(party, school)| FinanceSchoolRoute {
                party_id: (*party).into(),
                school_id: *school,
                route_id: format!("deepseek-{school:?}").to_lowercase(),
                family: "deepseek".into(),
                provider: "deepseek".into(),
                model: "deepseek-v4-pro".into(),
            })
            .collect();
        let policy = FinancePolicy {
            policy_version: "3.0".into(),
            protocol_version: "2.0".into(),
            profile: profile_binding.clone(),
            school_bindings: routes,
            counterpart_arbiter: FinanceArbiterRoute {
                arbiter_role: "counterpart_arbiter".into(),
                route_id: "deepseek-counterpart-arbiter".into(),
                family: "deepseek".into(),
                provider: "deepseek".into(),
                model: "deepseek-v4-pro".into(),
            },
            primary_arbiter: FinanceArbiterRoute {
                arbiter_role: "primary_arbiter".into(),
                route_id: "deepseek-primary-arbiter".into(),
                family: "deepseek".into(),
                provider: "deepseek".into(),
                model: "deepseek-v4-pro".into(),
            },
            isolation_backend: "process_information_flow_v1".into(),
            same_family_required: true,
        };
        let policy_binding = write_bound(
            root,
            "policy.json",
            &policy,
            "policy",
            "3.0",
            "quinte.finance-policy.v3",
        );

        let applicability = SCHOOL_BINDINGS
            .iter()
            .map(|(_, school)| {
                (
                    *school,
                    PreregisteredApplicability {
                        mode: ApplicabilityMode::Mandatory,
                        predicate_code: None,
                        predicate_inputs: BTreeMap::new(),
                        predicate_result: Some(true),
                    },
                )
            })
            .collect();
        let claims = FinanceClaimManifest {
            finance_claim_manifest_version: "1.0".into(),
            claims: vec![FinanceClaim {
                claim_id: "claim-1".into(),
                text: "Synthetic accepted primary remains bounded after review".into(),
                claim_classes: vec!["daily_relation".into()],
                school_applicability: applicability,
            }],
        };
        let claims_binding = write_bound(
            root,
            "claim-manifest.json",
            &claims,
            "finance-claim-manifest",
            "1.0",
            "quinte.finance-claim-manifest.v1",
        );
        let evidence = FinanceEvidenceIndex {
            finance_evidence_index_version: "1.0".into(),
            as_of: "2026-08-15T20:00:00Z".into(),
            evaluation_session: "2026-08-15".into(),
            items: Vec::new(),
        };
        let evidence_binding = write_bound(
            root,
            "evidence-index.json",
            &evidence,
            "finance-evidence-index",
            "1.0",
            "quinte.finance-evidence-index.v1",
        );
        let primary = serde_json::json!({
            "calculation_artifact_version": "1.0",
            "status": "accepted",
            "synthetic_value": 1
        });
        let primary_binding = write_bound(
            root,
            "primary.json",
            &primary,
            "galahad-calculation-artifact",
            "1.0",
            "galahad.calculation-artifact.v1",
        );
        let invocation = FinanceReviewInvocation {
            finance_review_invocation_version: "1.0".into(),
            invocation_id: run_id.into(),
            profile: profile_binding.clone(),
            claim_manifest: claims_binding,
            primary: PrimaryAuthority {
                binding: primary_binding.clone(),
                status: EvidenceStatus::Accepted,
                provenance_complete: true,
                evaluation_session: "2026-08-15".into(),
                expiry_session: "2026-08-18".into(),
            },
            evidence_index: evidence_binding.clone(),
        };
        let invocation_binding = write_bound(
            root,
            "invocation.json",
            &invocation,
            "finance-review-invocation",
            "1.0",
            "quinte.finance-review-invocation.v1",
        );
        let mut packet_policy_binding = policy_binding.clone();
        packet_policy_binding.artifact_ref = "input/policy.json".into();
        let mut packet_invocation_binding = invocation_binding.clone();
        packet_invocation_binding.artifact_ref = "input/invocation.json".into();
        let lane_decisions = || {
            vec![SchoolClaimDecision {
                claim_id: "claim-1".into(),
                disposition: SchoolDisposition::Clear,
                evidence_refs: Vec::new(),
                alternative_codes: Vec::new(),
                confounder_codes: Vec::new(),
                falsifier_codes: Vec::new(),
                invalidation_codes: Vec::new(),
                limitation_codes: Vec::new(),
                missing_evidence_codes: Vec::new(),
                closure_rule_code: None,
                closure_evidence_refs: Vec::new(),
            }]
        };
        let mut r1_outputs = Vec::new();
        let mut r1_bindings = Vec::new();
        let mut school_output_digests = Vec::new();
        for authority in &authorities {
            let school = authority.school_id;
            let route = policy
                .school_bindings
                .iter()
                .find(|route| route.school_id == school)
                .unwrap();
            let route_digest =
                semantic_digest_value("quinte.finance-route-binding.v1", route).unwrap();
            let packet = build_r1_packet(
                run_id,
                packet_invocation_binding.clone(),
                packet_policy_binding.clone(),
                profile_binding.clone(),
                invocation.claim_manifest.clone(),
                primary_binding.clone(),
                evidence_binding.clone(),
                route_digest.clone(),
                authority,
            );
            let (packet_exact, packet_semantic) =
                packet_digests(&packet, "quinte.finance-task-packet.v2");
            let lane = SchoolLaneOutput {
                school_lane_output_version: "1.0".into(),
                run_id: run_id.into(),
                phase: FinancePhase::R1,
                school_id: school,
                expected_route_digest: route_digest,
                profile_digest: profile_binding.semantic_sha256.clone(),
                primary_digest: primary_binding.semantic_sha256.clone(),
                evidence_index_digest: evidence_binding.semantic_sha256.clone(),
                input_packet_exact_sha256: packet_exact,
                input_packet_semantic_sha256: packet_semantic,
                decisions: lane_decisions(),
                residuals: if material_blocker && school == SchoolId::EventDriven {
                    vec![FinanceResidual {
                        residual_code: "unresolved_event_timing".into(),
                        affected_claim_ids: vec!["claim-1".into()],
                        materiality: Materiality::Material,
                        closure_state: FinanceClosureState::Open,
                        closure_rule_code: None,
                        closure_evidence_refs: Vec::new(),
                    }]
                } else {
                    Vec::new()
                },
            };
            let school_name = serde_json::to_value(school)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let binding = write_bound(
                root,
                &format!("r1/{school_name}.json"),
                &lane,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
            );
            school_output_digests.push(binding.semantic_sha256.clone());
            r1_outputs.push(lane);
            r1_bindings.push(binding);
        }
        for authority in &authorities {
            let school = authority.school_id;
            let route = policy
                .school_bindings
                .iter()
                .find(|route| route.school_id == school)
                .unwrap();
            let route_digest =
                semantic_digest_value("quinte.finance-route-binding.v1", route).unwrap();
            let packet = build_r2_packet(
                run_id,
                authority,
                &claims.claims,
                &r1_outputs,
                &r1_bindings,
                packet_policy_binding.clone(),
                profile_binding.clone(),
                invocation.claim_manifest.clone(),
                primary_binding.clone(),
                evidence_binding.clone(),
                route_digest.clone(),
            )
            .unwrap();
            let (packet_exact, packet_semantic) =
                packet_digests(&packet, "quinte.finance-r2-packet.v2");
            let lane = SchoolLaneOutput {
                school_lane_output_version: "1.0".into(),
                run_id: run_id.into(),
                phase: FinancePhase::R2,
                school_id: school,
                expected_route_digest: route_digest,
                profile_digest: profile_binding.semantic_sha256.clone(),
                primary_digest: primary_binding.semantic_sha256.clone(),
                evidence_index_digest: evidence_binding.semantic_sha256.clone(),
                input_packet_exact_sha256: packet_exact,
                input_packet_semantic_sha256: packet_semantic,
                decisions: lane_decisions(),
                residuals: Vec::new(),
            };
            let school_name = serde_json::to_value(school)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let binding = write_bound(
                root,
                &format!("r2/{school_name}.json"),
                &lane,
                "school-lane-output",
                "1.0",
                "quinte.school-lane-output.v1",
            );
            school_output_digests.push(binding.semantic_sha256);
        }
        let policy_value = serde_json::to_value(&policy).unwrap();
        let invocation_value = serde_json::to_value(&invocation).unwrap();
        let route_binding_digests = policy
            .school_bindings
            .iter()
            .map(|route| semantic_digest_value("quinte.finance-route-binding.v1", route).unwrap())
            .chain([
                semantic_digest_value(
                    "quinte.finance-arbiter-route-binding.v1",
                    &policy.counterpart_arbiter,
                )
                .unwrap(),
                semantic_digest_value(
                    "quinte.finance-arbiter-route-binding.v1",
                    &policy.primary_arbiter,
                )
                .unwrap(),
            ])
            .collect::<Vec<_>>();
        for (role, file) in [
            ("counterpart_arbiter", "counterpart-arbiter.json"),
            ("primary_arbiter", "primary-arbiter.json"),
        ] {
            let verdict = FinanceArbiterVerdict {
                finance_arbiter_verdict_version: "1.0".into(),
                run_id: run_id.into(),
                arbiter_role: role.into(),
                policy_digest: semantic_digest("quinte.finance-policy.v3", &policy_value).unwrap(),
                invocation_digest: semantic_digest(
                    "quinte.finance-review-invocation.v1",
                    &invocation_value,
                )
                .unwrap(),
                profile_digest: profile_binding.semantic_sha256.clone(),
                claim_manifest_digest: invocation.claim_manifest.semantic_sha256.clone(),
                primary_digest: primary_binding.semantic_sha256.clone(),
                evidence_index_digest: evidence_binding.semantic_sha256.clone(),
                school_output_digests: school_output_digests.clone(),
                route_binding_digests: route_binding_digests.clone(),
                duplicate_residual_groups: Vec::new(),
                identifier_reconciliations: BTreeMap::new(),
                scope_reconciliations: BTreeMap::new(),
                admitted_closure_evidence_refs: Vec::new(),
            };
            write_bound(
                root,
                &format!("arbiters/{file}"),
                &verdict,
                "finance-arbiter-verdict",
                "1.0",
                "quinte.finance-arbiter-verdict.v1",
            );
        }
    }

    fn r2_projection_fixture() -> (Vec<FinanceClaim>, Vec<SchoolLaneOutput>, SchoolAuthority) {
        let claims = vec![FinanceClaim {
            claim_id: "claim-1".into(),
            text: "Synthetic claim".into(),
            claim_classes: vec!["daily".into()],
            school_applicability: BTreeMap::new(),
        }];
        let outputs = SCHOOL_BINDINGS
            .iter()
            .enumerate()
            .map(|(index, (_, school))| SchoolLaneOutput {
                school_lane_output_version: "1.0".into(),
                run_id: "run-1".into(),
                phase: FinancePhase::R1,
                school_id: *school,
                expected_route_digest: digest('a'),
                profile_digest: digest('b'),
                primary_digest: digest('c'),
                evidence_index_digest: digest('d'),
                input_packet_exact_sha256: digest('e'),
                input_packet_semantic_sha256: digest('f'),
                decisions: vec![SchoolClaimDecision {
                    claim_id: "claim-1".into(),
                    disposition: if index == 0 {
                        SchoolDisposition::Contradicted
                    } else {
                        SchoolDisposition::Clear
                    },
                    evidence_refs: vec![format!(
                        "evidence:sha256:{}",
                        format!("{index:x}").repeat(64)
                    )],
                    alternative_codes: Vec::new(),
                    confounder_codes: Vec::new(),
                    falsifier_codes: Vec::new(),
                    invalidation_codes: Vec::new(),
                    limitation_codes: Vec::new(),
                    missing_evidence_codes: Vec::new(),
                    closure_rule_code: None,
                    closure_evidence_refs: Vec::new(),
                }],
                residuals: Vec::new(),
            })
            .collect();
        let recipient = SchoolAuthority {
            party_id: "Party A".into(),
            school_id: SchoolId::FactorRiskModel,
            accepted_evidence_classes: Vec::new(),
            forbidden_claim_classes: Vec::new(),
            question_codes: Vec::new(),
        };
        (claims, outputs, recipient)
    }

    fn dummy_packet_bindings() -> (
        PortableBinding,
        PortableBinding,
        PortableBinding,
        PortableBinding,
        PortableBinding,
        String,
    ) {
        (
            synthetic_binding("policy.json", '1'),
            synthetic_binding("profile.json", '2'),
            synthetic_binding("claim-manifest.json", '3'),
            synthetic_binding("primary.json", '4'),
            synthetic_binding("evidence-index.json", '5'),
            digest('6'),
        )
    }

    fn build_fixture_r2_packet(
        recipient: &SchoolAuthority,
        claims: &[FinanceClaim],
        outputs: &[SchoolLaneOutput],
    ) -> anyhow::Result<FinanceR2Packet> {
        let (policy, profile, claim_manifest, primary, evidence_index, route) =
            dummy_packet_bindings();
        let bindings = outputs
            .iter()
            .enumerate()
            .map(|(index, output)| {
                let value = serde_json::to_value(output).unwrap();
                let bytes = serde_json::to_vec_pretty(&value).unwrap();
                binding_for(
                    format!("r1/{index}.json"),
                    "school-lane-output",
                    "1.0",
                    "quinte.school-lane-output.v1",
                    &bytes,
                    &value,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        build_r2_packet(
            "run-1",
            recipient,
            claims,
            outputs,
            &bindings,
            policy,
            profile,
            claim_manifest,
            primary,
            evidence_index,
            route,
        )
    }

    fn stage_dormant_outputs(source: &Path, state: &Path, phase: &str) {
        for (_, school) in SCHOOL_BINDINGS {
            let name = serde_json::to_value(school)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let from = source.join(phase).join(format!("{name}.json"));
            let to = state
                .join("outputs")
                .join(phase)
                .join(format!("{name}.json"));
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(from, to).unwrap();
        }
    }

    fn manifest_at_phase(
        source: &Path,
        state: &Path,
        phase: FinanceRunStatus,
    ) -> FinanceRunManifest {
        let mut manifest = dormant_init(source, state, DORMANT_WRITER_ACK).unwrap();
        if phase == FinanceRunStatus::R1Running {
            return manifest;
        }
        stage_dormant_outputs(source, state, "r1");
        manifest = dormant_advance(state, DORMANT_WRITER_ACK).unwrap();
        if phase == FinanceRunStatus::R2Running {
            return manifest;
        }
        stage_dormant_outputs(source, state, "r2");
        let mut r2_outputs = Vec::new();
        for (_, school) in SCHOOL_BINDINGS {
            let name = serde_json::to_value(school)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let relative = format!("outputs/r2/{name}.json");
            let (_, bytes, value) = load_typed::<SchoolLaneOutput>(&state.join(&relative)).unwrap();
            r2_outputs.push(
                binding_for(
                    relative,
                    "school-lane-output",
                    "1.0",
                    "quinte.school-lane-output.v1",
                    &bytes,
                    &value,
                )
                .unwrap(),
            );
        }
        std::fs::create_dir_all(state.join("outputs/arbiters")).unwrap();
        let mut arbiters = Vec::new();
        for file in ["counterpart-arbiter.json", "primary-arbiter.json"] {
            let relative = format!("outputs/arbiters/{file}");
            std::fs::copy(source.join("arbiters").join(file), state.join(&relative)).unwrap();
            let (_, bytes, value) =
                load_typed::<FinanceArbiterVerdict>(&state.join(&relative)).unwrap();
            arbiters.push(
                binding_for(
                    relative,
                    "finance-arbiter-verdict",
                    "1.0",
                    "quinte.finance-arbiter-verdict.v1",
                    &bytes,
                    &value,
                )
                .unwrap(),
            );
        }
        manifest.r2_outputs = r2_outputs;
        manifest.arbiter_outputs = arbiters;
        manifest.status = FinanceRunStatus::Merging;
        transition_with_pending(state, &mut manifest).unwrap();
        manifest
    }

    fn terminalize_for_test(
        state: &Path,
        manifest: &mut FinanceRunManifest,
        status: FinanceRunStatus,
    ) {
        let code = if status == FinanceRunStatus::Cancelled {
            FinanceTerminationCode::OperatorCancelled
        } else {
            FinanceTerminationCode::OutputInvalid
        };
        let phase = manifest.status;
        manifest.status = status;
        manifest.termination = Some(FinanceTerminationFacts {
            phase,
            code,
            retryable: false,
        });
        transition_with_pending(state, manifest).unwrap();
    }

    #[test]
    fn terminal_termination_facts_and_prefixes_fail_closed() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let base = manifest_at_phase(&source, &state, FinanceRunStatus::R2Running);

        let mut invalid = base.clone();
        invalid.status = FinanceRunStatus::Failed;
        invalid.termination = Some(FinanceTerminationFacts {
            phase: FinanceRunStatus::R2Running,
            code: FinanceTerminationCode::OperatorCancelled,
            retryable: false,
        });
        assert!(validate_manifest_termination(&invalid).is_err());
        assert!(
            crate::schema::validate_value(
                &serde_json::to_value(&invalid).unwrap(),
                crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
            )
            .is_err()
        );

        invalid.status = FinanceRunStatus::Cancelled;
        invalid.termination.as_mut().unwrap().code = FinanceTerminationCode::OutputInvalid;
        assert!(validate_manifest_termination(&invalid).is_err());

        invalid.status = FinanceRunStatus::Failed;
        invalid.termination.as_mut().unwrap().code = FinanceTerminationCode::IntegrityFailure;
        invalid.termination.as_mut().unwrap().retryable = true;
        assert!(validate_manifest_termination(&invalid).is_err());

        invalid.termination.as_mut().unwrap().retryable = false;
        invalid
            .r2_outputs
            .push(synthetic_binding("outputs/r2/extra.json", 'a'));
        assert!(validate_manifest_counts(&invalid).is_err());

        invalid.r2_outputs.clear();
        invalid.result = Some(synthetic_binding("result.json", 'b'));
        assert!(validate_manifest_termination(&invalid).is_err());
    }

    #[test]
    fn run_manifest_schema_requires_exact_merging_arbiter_prefix() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let manifest = manifest_at_phase(&source, &state, FinanceRunStatus::Merging);
        let value = serde_json::to_value(&manifest).unwrap();
        crate::schema::validate_value(&value, crate::contract::FINANCE_RUN_MANIFEST_SCHEMA)
            .unwrap();

        for count in [0, 1] {
            let mut incomplete = value.clone();
            incomplete["arbiter_outputs"]
                .as_array_mut()
                .unwrap()
                .truncate(count);
            assert!(
                crate::schema::validate_value(
                    &incomplete,
                    crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,
                )
                .is_err(),
                "merging manifest accepted {count} arbiter outputs"
            );
        }

        let mut overfull = value;
        overfull["arbiter_outputs"].as_array_mut().unwrap().push(
            serde_json::to_value(synthetic_binding("outputs/arbiters/extra.json", 'f')).unwrap(),
        );
        assert!(
            crate::schema::validate_value(&overfull, crate::contract::FINANCE_RUN_MANIFEST_SCHEMA,)
                .is_err(),
            "merging manifest accepted three arbiter outputs"
        );
    }

    #[test]
    fn fixed_school_map_is_exact() {
        assert_eq!(school_for_party("Party A"), Some(SchoolId::FactorRiskModel));
        assert_eq!(
            school_for_party("Party E"),
            Some(SchoolId::MarketMicrostructure)
        );
        assert_eq!(school_for_party("Party F"), None);
    }

    #[test]
    fn strict_json_rejects_nested_duplicate_members() {
        let error = parse_strict_json(br#"{"outer":{"claim":1,"claim":2}}"#).unwrap_err();
        assert!(error.to_string().contains("duplicate-free"));
    }

    #[test]
    fn run_event_schema_enforces_exact_artifact_prefix_cardinality() {
        fn assert_schema_prefix(event: &FinanceRunEvent, expected: usize) {
            let value = serde_json::to_value(event).unwrap();
            assert_eq!(
                value["payload"]["artifact_bindings"]
                    .as_array()
                    .unwrap()
                    .len(),
                expected
            );
            crate::schema::validate_value(&value, crate::contract::FINANCE_RUN_EVENT_SCHEMA)
                .unwrap();

            let mut empty = value.clone();
            empty["payload"]["artifact_bindings"] = Value::Array(Vec::new());
            assert!(
                crate::schema::validate_value(&empty, crate::contract::FINANCE_RUN_EVENT_SCHEMA,)
                    .is_err()
            );

            let mut short = value;
            short["payload"]["artifact_bindings"]
                .as_array_mut()
                .unwrap()
                .pop();
            assert!(
                crate::schema::validate_value(&short, crate::contract::FINANCE_RUN_EVENT_SCHEMA,)
                    .is_err()
            );

            let mut long = serde_json::to_value(event).unwrap();
            let bindings = long["payload"]["artifact_bindings"].as_array_mut().unwrap();
            let mut extra = bindings[0].clone();
            extra["exact_sha256"] = Value::String(digest('f'));
            extra["semantic_sha256"] = Value::String(digest('e'));
            bindings.push(extra);
            assert!(
                crate::schema::validate_value(&long, crate::contract::FINANCE_RUN_EVENT_SCHEMA,)
                    .is_err()
            );
        }

        let terminal = tempfile::tempdir().unwrap();
        let input = terminal.path().join("input");
        let output = terminal.path().join("output");
        finance_fixture(&input, false);
        finalize_bundle(&input, &output).unwrap();
        let ledger = read_finance_ledger(&output).unwrap();
        assert_eq!(ledger.events.len(), 4);
        for (event, expected) in ledger.events.iter().zip([7, 17, 24, 25]) {
            assert_schema_prefix(event, expected);
        }
        let mut degraded = serde_json::to_value(&ledger.events[3]).unwrap();
        degraded["payload"]["status"] = Value::String("degraded".into());
        crate::schema::validate_value(&degraded, crate::contract::FINANCE_RUN_EVENT_SCHEMA)
            .unwrap();

        let mut preflight_created = serde_json::to_value(&ledger.events[0]).unwrap();
        preflight_created["payload"]["status"] = Value::String("preflight".into());
        assert!(
            crate::schema::validate_value(
                &preflight_created,
                crate::contract::FINANCE_RUN_EVENT_SCHEMA,
            )
            .is_err()
        );
        let mut preflight_advance = serde_json::to_value(&ledger.events[1]).unwrap();
        preflight_advance["payload"]["previous_status"] = Value::String("preflight".into());
        preflight_advance["payload"]["status"] = Value::String("r1_running".into());
        assert!(
            crate::schema::validate_value(
                &preflight_advance,
                crate::contract::FINANCE_RUN_EVENT_SCHEMA,
            )
            .is_err()
        );

        for (phase, expected) in [
            (FinanceRunStatus::R1Running, 7),
            (FinanceRunStatus::R2Running, 17),
            (FinanceRunStatus::Merging, 24),
        ] {
            for status in [FinanceRunStatus::Failed, FinanceRunStatus::Cancelled] {
                let temporary = tempfile::tempdir().unwrap();
                let source = temporary.path().join("source");
                let state = temporary.path().join("state");
                finance_fixture(&source, false);
                let mut manifest = manifest_at_phase(&source, &state, phase);
                terminalize_for_test(&state, &mut manifest, status);
                let terminal = read_finance_ledger(&state).unwrap();
                assert_schema_prefix(terminal.events.last().unwrap(), expected);
            }
        }
    }

    #[test]
    fn failed_and_cancelled_terminal_prefixes_replay_exactly() {
        for phase in [
            FinanceRunStatus::R1Running,
            FinanceRunStatus::R2Running,
            FinanceRunStatus::Merging,
        ] {
            for status in [FinanceRunStatus::Failed, FinanceRunStatus::Cancelled] {
                let temporary = tempfile::tempdir().unwrap();
                let source = temporary.path().join("source");
                let state = temporary.path().join("state");
                finance_fixture(&source, false);
                let mut manifest = manifest_at_phase(&source, &state, phase);
                terminalize_for_test(&state, &mut manifest, status);
                validate_dormant_manifest_graph(&state, &manifest).unwrap();
                let ledger = read_finance_ledger(&state).unwrap();
                let expected_events = match phase {
                    FinanceRunStatus::R1Running => 2,
                    FinanceRunStatus::R2Running => 3,
                    FinanceRunStatus::Merging => 4,
                    _ => unreachable!(),
                };
                assert_eq!(ledger.events.len(), expected_events);
                assert!(manifest.result.is_none());
                let FinanceRunEventBody::RunTerminalized(payload) =
                    &ledger.events.last().unwrap().body
                else {
                    panic!("terminal ledger does not end in run.terminalized");
                };
                assert_eq!(payload.previous_status, phase);
                assert_eq!(payload.termination, manifest.termination);
                assert!(payload.result.is_none());
                assert!(dormant_advance(&state, DORMANT_WRITER_ACK).is_err());
            }
        }
    }

    #[test]
    fn typed_termination_rejects_status_code_phase_and_result_drift() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let active = manifest_at_phase(&source, &state, FinanceRunStatus::R1Running);

        let mut invalid = active.clone();
        invalid.status = FinanceRunStatus::Failed;
        invalid.termination = Some(FinanceTerminationFacts {
            phase: FinanceRunStatus::R1Running,
            code: FinanceTerminationCode::OperatorCancelled,
            retryable: false,
        });
        assert!(validate_manifest_termination(&invalid).is_err());

        invalid.status = FinanceRunStatus::Cancelled;
        invalid.termination.as_mut().unwrap().code = FinanceTerminationCode::IntegrityFailure;
        assert!(validate_manifest_termination(&invalid).is_err());

        invalid.status = FinanceRunStatus::Failed;
        invalid.termination.as_mut().unwrap().code = FinanceTerminationCode::OutputInvalid;
        invalid.termination.as_mut().unwrap().retryable = true;
        assert!(validate_manifest_termination(&invalid).is_err());

        invalid.termination.as_mut().unwrap().retryable = false;
        invalid.termination.as_mut().unwrap().phase = FinanceRunStatus::R2Running;
        assert!(validate_manifest_counts(&invalid).is_err());

        invalid.termination.as_mut().unwrap().phase = FinanceRunStatus::R1Running;
        invalid.result = Some(synthetic_binding("result.json", 'a'));
        assert!(validate_manifest_termination(&invalid).is_err());

        let mut success = active;
        success.termination = Some(FinanceTerminationFacts {
            phase: FinanceRunStatus::R1Running,
            code: FinanceTerminationCode::OutputInvalid,
            retryable: false,
        });
        assert!(validate_manifest_termination(&success).is_err());
    }

    #[test]
    fn terminal_event_rejects_termination_tampering() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let mut manifest = manifest_at_phase(&source, &state, FinanceRunStatus::R1Running);
        terminalize_for_test(&state, &mut manifest, FinanceRunStatus::Failed);
        let mut ledger = read_finance_ledger(&state).unwrap();
        let FinanceRunEventBody::RunTerminalized(payload) =
            &mut ledger.events.last_mut().unwrap().body
        else {
            unreachable!();
        };
        payload.termination.as_mut().unwrap().code = FinanceTerminationCode::IntegrityFailure;
        let tampered = canonical_event_line(ledger.events.last().unwrap()).unwrap();
        let mut bytes = ledger.lines[0].clone();
        bytes.extend_from_slice(&tampered);
        std::fs::write(state.join("events.jsonl"), bytes).unwrap();
        assert!(verify_expected_event_ledger(&state, &manifest).is_err());
    }

    #[test]
    fn semantic_digest_ignores_key_order_and_whitespace() {
        let first = parse_strict_json(br#"{"b":2, "a":1}"#).unwrap();
        let second = parse_strict_json(br#"{ "a" : 1, "b" : 2 }"#).unwrap();
        assert_eq!(
            semantic_digest("quinte.test.v1", &first).unwrap(),
            semantic_digest("quinte.test.v1", &second).unwrap()
        );
        assert_eq!(canonical_json(&first).unwrap(), br#"{"a":1,"b":2}"#);
    }

    #[test]
    fn finance_bundle_happy_path_emits_manifest_result_and_highball_carriers() {
        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, false);
        let finalized = finalize_bundle(&input, &output).unwrap();
        assert_eq!(
            finalized.publication_posture,
            PublicationPosture::PublishBounded
        );
        for path in [
            finalized.result_path,
            finalized.manifest_path,
            finalized.highball_route_request_path,
            finalized.highball_residual_trace_path,
        ] {
            assert!(path.is_file(), "missing {}", path.display());
        }
        let manifest: FinanceRunManifest =
            serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest.manifest_version, "3.0");
        assert_eq!(manifest.protocol_version, "2.0");
        assert_eq!(manifest.r1_outputs.len(), 5);
        assert_eq!(manifest.r2_outputs.len(), 5);
        let verified = verify_bundle(&output).unwrap();
        assert!(verified.highball_carriers_verified);
        assert!(!verified.finance_creation_enabled);
    }

    #[test]
    fn finance_bundle_tampering_fails_closed_before_output() {
        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, false);
        let path = input.join("primary.json");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.push(b'\n');
        std::fs::write(path, bytes).unwrap();
        let error = finalize_bundle(&input, &output).unwrap_err();
        assert!(error.to_string().contains("digest mismatch"), "{error:#}");
        assert!(!output.exists());
    }

    #[test]
    fn offline_verifier_rejects_tampered_highball_carrier() {
        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, false);
        finalize_bundle(&input, &output).unwrap();
        let path = output.join("highball.route-request.json");
        let mut route: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        route["requested_route"] = serde_json::json!("LOWBALL");
        std::fs::write(path, serde_json::to_vec_pretty(&route).unwrap()).unwrap();
        let error = verify_bundle(&output).unwrap_err();
        assert!(error.to_string().contains("HIGHBALL carriers"), "{error:#}");
    }

    #[test]
    fn offline_verifier_never_mutates_its_target_tree() {
        type EntrySnapshot = (
            bool,
            bool,
            bool,
            u64,
            std::time::SystemTime,
            Option<Vec<u8>>,
        );

        fn inventory(root: &Path) -> BTreeMap<String, EntrySnapshot> {
            fn walk(root: &Path, current: &Path, entries: &mut BTreeMap<String, EntrySnapshot>) {
                let metadata = std::fs::symlink_metadata(current).unwrap();
                let kind = metadata.file_type();
                let relative = current
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let key = if relative.is_empty() {
                    ".".to_string()
                } else {
                    relative
                };
                let bytes = kind.is_file().then(|| std::fs::read(current).unwrap());
                entries.insert(
                    key,
                    (
                        kind.is_file(),
                        kind.is_dir(),
                        kind.is_symlink(),
                        metadata.len(),
                        metadata.modified().unwrap(),
                        bytes,
                    ),
                );
                if kind.is_dir() {
                    for child in std::fs::read_dir(current).unwrap() {
                        walk(root, &child.unwrap().path(), entries);
                    }
                }
            }

            let mut entries = BTreeMap::new();
            walk(root, root, &mut entries);
            entries
        }

        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, false);
        finalize_bundle(&input, &output).unwrap();

        let before_valid = inventory(&output);
        verify_bundle(&output).unwrap();
        assert_eq!(inventory(&output), before_valid);

        std::fs::write(output.join("pending-transition.json"), b"{}\n").unwrap();
        let before_pending = inventory(&output);
        assert!(verify_bundle(&output).is_err());
        assert_eq!(inventory(&output), before_pending);

        std::fs::remove_file(output.join("pending-transition.json")).unwrap();
        let events_path = output.join("events.jsonl");
        let mut torn = std::fs::read(&events_path).unwrap();
        assert_eq!(torn.pop(), Some(b'\n'));
        std::fs::write(&events_path, torn).unwrap();
        let before_torn = inventory(&output);
        assert!(verify_bundle(&output).is_err());
        assert_eq!(inventory(&output), before_torn);
    }

    #[test]
    fn one_open_material_residual_forces_abstain() {
        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, true);
        let finalized = finalize_bundle(&input, &output).unwrap();
        assert_eq!(finalized.publication_posture, PublicationPosture::Abstain);
        let result: FinanceReviewResult =
            serde_json::from_slice(&std::fs::read(output.join("result.json")).unwrap()).unwrap();
        assert_eq!(
            result.publication.reason_codes,
            vec![PostureReason::OpenMaterialResidual]
        );
    }

    #[test]
    fn r2_projection_is_invariant_under_input_permutations() {
        let (claims, outputs, recipient) = r2_projection_fixture();
        let expected = build_fixture_r2_packet(&recipient, &claims, &outputs).unwrap();
        for rotation in 1..5 {
            let mut permuted = outputs.clone();
            permuted.rotate_left(rotation);
            let actual = build_fixture_r2_packet(&recipient, &claims, &permuted).unwrap();
            assert_eq!(
                actual.corpus_semantic_sha256,
                expected.corpus_semantic_sha256
            );
            assert_eq!(actual.corpus, expected.corpus);
        }
        let mut reversed = outputs.clone();
        reversed.reverse();
        assert_eq!(
            build_fixture_r2_packet(&recipient, &claims, &reversed)
                .unwrap()
                .corpus,
            expected.corpus
        );
    }

    #[test]
    fn r2_projection_rejects_identity_bearing_evidence_paths() {
        let (claims, mut outputs, recipient) = r2_projection_fixture();
        for malicious in [
            "lanes/R1/deepseek-a/accepted.json",
            "Party A/event.json",
            "route:deepseek-a",
            "adapter/opencode/output.json",
        ] {
            outputs[0].decisions[0].evidence_refs = vec![malicious.into()];
            let error = build_fixture_r2_packet(&recipient, &claims, &outputs).unwrap_err();
            assert!(
                error.to_string().contains("anonymous content IDs"),
                "{error:#}"
            );
        }
    }

    #[test]
    fn dormant_lifecycle_is_explicit_staged_and_idempotent() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        assert!(dormant_init(&source, &state, "wrong").is_err());
        let initialized = dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
        assert_eq!(initialized.status, FinanceRunStatus::R1Running);
        assert_eq!(initialized.r1_packets.len(), 5);
        assert_eq!(
            dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap(),
            initialized
        );

        stage_dormant_outputs(&source, &state, "r1");
        let r2 = dormant_advance(&state, DORMANT_WRITER_ACK).unwrap();
        assert_eq!(r2.status, FinanceRunStatus::R2Running);
        assert_eq!(r2.r2_packets.len(), 5);
        stage_dormant_outputs(&source, &state, "r2");
        std::fs::create_dir_all(state.join("outputs/arbiters")).unwrap();
        for file in ["counterpart-arbiter.json", "primary-arbiter.json"] {
            std::fs::copy(
                source.join("arbiters").join(file),
                state.join("outputs/arbiters").join(file),
            )
            .unwrap();
        }
        let terminal = dormant_advance(&state, DORMANT_WRITER_ACK).unwrap();
        assert_eq!(terminal.status, FinanceRunStatus::Completed);
        assert_eq!(
            dormant_advance(&state, DORMANT_WRITER_ACK).unwrap(),
            terminal
        );
        assert!(verify_bundle(&state.join("terminal")).is_ok());
    }

    #[test]
    fn dormant_resume_replays_a_crash_window_pending_transition() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let expected = dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
        let expected_bytes = serde_json::to_vec_pretty(&expected).unwrap();
        let event_bytes = std::fs::read(state.join("events.jsonl")).unwrap();
        let event: FinanceRunEvent =
            serde_json::from_value(parse_strict_json(&event_bytes).unwrap()).unwrap();
        let pending = PendingFinanceTransition {
            pending_transition_version: "2.0".into(),
            run_id: expected.run_id.clone(),
            run_genesis_digest: expected.run_genesis_digest.clone(),
            transition_id: digest('a'),
            operation: "create".into(),
            old_manifest_exact_sha256: None,
            old_event_checkpoint: None,
            event_exact_sha256: exact_digest(&event_bytes),
            event_byte_length: event_bytes.len() as u64,
            event_bytes_base64: base64::engine::general_purpose::STANDARD.encode(&event_bytes),
            target_manifest_exact_sha256: exact_digest(&expected_bytes),
            target_manifest_byte_length: expected_bytes.len() as u64,
            target_manifest_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&expected_bytes),
            artifact_bindings: manifest_artifact_bindings(&expected),
        };
        assert_eq!(event.sequence, 0);
        std::fs::remove_file(state.join("manifest.json")).unwrap();
        std::fs::write(
            state.join("pending-transition.json"),
            serde_json::to_vec_pretty(&pending).unwrap(),
        )
        .unwrap();
        let resumed = dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
        assert_eq!(resumed.status, FinanceRunStatus::R1Running);
        assert!(!state.join("pending-transition.json").exists());
        assert_eq!(
            std::fs::read(state.join("manifest.json")).unwrap(),
            expected_bytes
        );
    }

    #[test]
    fn dormant_replay_rejects_any_source_or_evidence_tree_drift() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();

        let policy_path = source.join("policy.json");
        let original = std::fs::read(&policy_path).unwrap();
        let mut whitespace_tamper = original.clone();
        whitespace_tamper.push(b'\n');
        std::fs::write(&policy_path, whitespace_tamper).unwrap();
        assert!(dormant_init(&source, &state, DORMANT_WRITER_ACK).is_err());
        std::fs::write(&policy_path, original).unwrap();

        std::fs::create_dir_all(source.join("evidence")).unwrap();
        std::fs::write(source.join("evidence/unreferenced.json"), b"{}\n").unwrap();
        assert!(dormant_init(&source, &state, DORMANT_WRITER_ACK).is_err());
    }

    #[test]
    fn packet_binding_rejects_cross_school_swap() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        finance_fixture(&source, false);
        let left = source.join("r1/factor_risk_model.json");
        let right = source.join("r1/event_driven.json");
        let mut left_output: SchoolLaneOutput =
            serde_json::from_slice(&std::fs::read(&left).unwrap()).unwrap();
        let right_output: SchoolLaneOutput =
            serde_json::from_slice(&std::fs::read(&right).unwrap()).unwrap();
        left_output.input_packet_exact_sha256 = right_output.input_packet_exact_sha256;
        left_output.input_packet_semantic_sha256 = right_output.input_packet_semantic_sha256;
        std::fs::write(&left, serde_json::to_vec_pretty(&left_output).unwrap()).unwrap();
        let error = finalize_bundle(&source, &temporary.path().join("terminal")).unwrap_err();
        assert!(
            error.to_string().contains("input packet binding mismatch"),
            "{error:#}"
        );
    }

    #[test]
    fn r2_source_set_rejects_duplicate_or_unrelated_artifact_identity() {
        let (claims, outputs, recipient) = r2_projection_fixture();
        let (policy, profile, claim_manifest, primary, evidence_index, route) =
            dummy_packet_bindings();
        let mut bindings = outputs
            .iter()
            .enumerate()
            .map(|(index, output)| {
                let value = serde_json::to_value(output).unwrap();
                let bytes = serde_json::to_vec_pretty(&value).unwrap();
                binding_for(
                    format!("r1/{index}.json"),
                    "school-lane-output",
                    "1.0",
                    "quinte.school-lane-output.v1",
                    &bytes,
                    &value,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        bindings[4] = bindings[0].clone();
        assert!(
            build_r2_packet(
                "run-1",
                &recipient,
                &claims,
                &outputs,
                &bindings,
                policy,
                profile,
                claim_manifest,
                primary,
                evidence_index,
                route,
            )
            .is_err()
        );
    }

    #[test]
    fn ledger_tamper_insert_reorder_suffix_and_splice_fail_closed() {
        fn completed_run(root: &Path, run_id: &str) -> (PathBuf, PathBuf) {
            let source = root.join("source");
            let state = root.join("state");
            finance_fixture_with_run_id(&source, false, run_id);
            dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
            stage_dormant_outputs(&source, &state, "r1");
            dormant_advance(&state, DORMANT_WRITER_ACK).unwrap();
            stage_dormant_outputs(&source, &state, "r2");
            std::fs::create_dir_all(state.join("outputs/arbiters")).unwrap();
            for file in ["counterpart-arbiter.json", "primary-arbiter.json"] {
                std::fs::copy(
                    source.join("arbiters").join(file),
                    state.join("outputs/arbiters").join(file),
                )
                .unwrap();
            }
            dormant_advance(&state, DORMANT_WRITER_ACK).unwrap();
            (source, state)
        }

        let temporary = tempfile::tempdir().unwrap();
        let cases = ["edit", "insert", "reorder", "suffix", "splice"];
        for case in cases {
            let root = temporary.path().join(case);
            std::fs::create_dir_all(&root).unwrap();
            let (_source, state) = completed_run(&root, "finance-run-1");
            let original = std::fs::read(state.join("events.jsonl")).unwrap();
            let mut lines = original
                .split_inclusive(|byte| *byte == b'\n')
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>();
            let tampered = match case {
                "edit" => {
                    let position = lines[0].iter().position(|byte| *byte == b'2').unwrap();
                    lines[0][position] = b'3';
                    lines.concat()
                }
                "insert" => {
                    lines.insert(1, lines[0].clone());
                    lines.concat()
                }
                "reorder" => {
                    lines.swap(1, 2);
                    lines.concat()
                }
                "suffix" => lines[..lines.len() - 1].concat(),
                "splice" => {
                    let other = temporary.path().join("splice-other");
                    std::fs::create_dir_all(&other).unwrap();
                    let (_, other_state) = completed_run(&other, "finance-run-2");
                    let other_lines = std::fs::read(other_state.join("events.jsonl"))
                        .unwrap()
                        .split_inclusive(|byte| *byte == b'\n')
                        .map(<[u8]>::to_vec)
                        .collect::<Vec<_>>();
                    assert_ne!(lines[2], other_lines[2]);
                    lines[2] = other_lines[2].clone();
                    lines.concat()
                }
                _ => unreachable!(),
            };
            std::fs::write(state.join("events.jsonl"), tampered).unwrap();
            let manifest = load_typed::<FinanceRunManifest>(&state.join("manifest.json"))
                .unwrap()
                .0;
            assert!(
                verify_expected_event_ledger(&state, &manifest).is_err(),
                "accepted {case}"
            );
        }
    }

    #[test]
    fn lane_decision_set_and_evidence_membership_are_exact() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        finance_fixture(&source, false);
        let lane_path = source.join("r1/factor_risk_model.json");
        let original = std::fs::read(&lane_path).unwrap();
        let mut lane: Value = serde_json::from_slice(&original).unwrap();
        let duplicate = lane["decisions"][0].clone();
        lane["decisions"].as_array_mut().unwrap().push(duplicate);
        rebind_fixture_r1_output(&source, SchoolId::FactorRiskModel, &mut lane);
        std::fs::write(&lane_path, serde_json::to_vec_pretty(&lane).unwrap()).unwrap();
        let error = finalize_bundle(&source, &temporary.path().join("duplicate")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("preregistered claims exactly once")
                || error.to_string().contains("non-unique elements"),
            "{error:#}"
        );

        std::fs::write(&lane_path, &original).unwrap();
        let mut lane: Value = serde_json::from_slice(&original).unwrap();
        lane["decisions"][0]["evidence_refs"] = serde_json::json!([
            "evidence:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ]);
        rebind_fixture_r1_output(&source, SchoolId::FactorRiskModel, &mut lane);
        std::fs::write(&lane_path, serde_json::to_vec_pretty(&lane).unwrap()).unwrap();
        let mutated: SchoolLaneOutput = serde_json::from_value(lane).unwrap();
        let (profile, _, _) =
            load_typed::<FinanceReviewProfile>(&source.join("profile.json")).unwrap();
        let (claims, _, _) =
            load_typed::<FinanceClaimManifest>(&source.join("claim-manifest.json")).unwrap();
        let (evidence, _, _) =
            load_typed::<FinanceEvidenceIndex>(&source.join("evidence-index.json")).unwrap();
        let error = validate_lane_claims_and_evidence(&profile, &claims, &evidence, &[mutated])
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("absent from the pinned evidence index"),
            "{error:#}"
        );
    }

    #[test]
    fn arbiter_binding_drift_is_rejected_and_cannot_change_decisions() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        finance_fixture(&source, false);
        let path = source.join("arbiters/primary-arbiter.json");
        let mut verdict: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        verdict["profile_digest"] = serde_json::json!(digest('f'));
        std::fs::write(path, serde_json::to_vec_pretty(&verdict).unwrap()).unwrap();
        let error = finalize_bundle(&source, &temporary.path().join("output")).unwrap_err();
        assert!(error.to_string().contains("bound authority"), "{error:#}");
        assert!(!temporary.path().join("output").exists());

        // Closed schema has no disposition/applicability field for an arbiter.
        verdict["disposition"] = serde_json::json!("clear");
        assert!(
            crate::schema::validate_value(
                &verdict,
                crate::contract::FINANCE_ARBITER_VERDICT_SCHEMA
            )
            .is_err()
        );
    }

    #[test]
    fn swapped_lane_files_are_rejected_in_finalizer_and_dormant_r1() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        finance_fixture(&source, false);
        let first = source.join("r1/factor_risk_model.json");
        let second = source.join("r1/event_driven.json");
        let first_bytes = std::fs::read(&first).unwrap();
        let second_bytes = std::fs::read(&second).unwrap();
        std::fs::write(&first, &second_bytes).unwrap();
        std::fs::write(&second, &first_bytes).unwrap();
        assert!(
            finalize_bundle(&source, &temporary.path().join("terminal"))
                .unwrap_err()
                .to_string()
                .contains("filename/seat")
        );

        let state = temporary.path().join("state");
        std::fs::write(&first, &first_bytes).unwrap();
        std::fs::write(&second, &second_bytes).unwrap();
        dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
        stage_dormant_outputs(&source, &state, "r1");
        std::fs::write(
            state.join("outputs/r1/factor_risk_model.json"),
            second_bytes,
        )
        .unwrap();
        assert!(
            dormant_advance(&state, DORMANT_WRITER_ACK)
                .unwrap_err()
                .to_string()
                .contains("filename/seat")
        );

        let r2_first = source.join("r2/factor_risk_model.json");
        let r2_second = source.join("r2/event_driven.json");
        let r2_first_bytes = std::fs::read(&r2_first).unwrap();
        let r2_second_bytes = std::fs::read(&r2_second).unwrap();
        std::fs::write(&r2_first, &r2_second_bytes).unwrap();
        std::fs::write(&r2_second, &r2_first_bytes).unwrap();
        assert!(
            finalize_bundle(&source, &temporary.path().join("r2-terminal"))
                .unwrap_err()
                .to_string()
                .contains("filename/seat")
        );
    }

    #[test]
    fn invalid_dormant_init_is_atomic_and_retryable() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let policy_path = source.join("policy.json");
        let original = std::fs::read(&policy_path).unwrap();
        std::fs::write(&policy_path, b"{}\n").unwrap();
        assert!(dormant_init(&source, &state, DORMANT_WRITER_ACK).is_err());
        assert!(!state.exists());
        std::fs::write(policy_path, original).unwrap();
        assert_eq!(
            dormant_init(&source, &state, DORMANT_WRITER_ACK)
                .unwrap()
                .status,
            FinanceRunStatus::R1Running
        );
    }

    #[test]
    fn portable_binding_rejects_traversal_and_windows_paths() {
        for malicious in [
            "../result.json",
            "/tmp/result.json",
            "C:/result.json",
            r"..\result.json",
            ".",
            "a/../b",
        ] {
            let mut binding = synthetic_binding(malicious, 'a');
            let value = serde_json::json!({"ok": true});
            let bytes = serde_json::to_vec(&value).unwrap();
            binding.exact_sha256 = exact_digest(&bytes);
            binding.semantic_sha256 = semantic_digest("quinte.synthetic.v1", &value).unwrap();
            assert!(
                verify_portable_binding(
                    &binding,
                    &bytes,
                    &value,
                    "synthetic",
                    "1.0",
                    "quinte.synthetic.v1"
                )
                .is_err(),
                "accepted {malicious}"
            );
        }
    }

    #[test]
    fn same_run_different_existing_bundle_is_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let input = temporary.path().join("input");
        let output = temporary.path().join("output");
        finance_fixture(&input, false);
        finalize_bundle(&input, &output).unwrap();
        std::fs::write(output.join("unbound-extra.json"), b"{}\n").unwrap();
        assert!(
            finalize_bundle(&input, &output)
                .unwrap_err()
                .to_string()
                .contains("deterministic bundle")
        );
    }

    #[test]
    fn policy_rejects_cross_family_arbiter() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        finance_fixture(&source, false);
        let path = source.join("policy.json");
        let mut policy: FinancePolicy =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        policy.primary_arbiter.model = "different-model".into();
        assert!(validate_finance_policy(&policy).is_err());
    }

    #[test]
    fn r2_projection_canonicalizes_internal_order() {
        let (mut claims, mut outputs, recipient) = r2_projection_fixture();
        claims.push(FinanceClaim {
            claim_id: "claim-0".into(),
            text: "Earlier claim".into(),
            claim_classes: vec!["daily".into()],
            school_applicability: BTreeMap::new(),
        });
        for output in &mut outputs {
            let mut decision = output.decisions[0].clone();
            decision.claim_id = "claim-0".into();
            output.decisions.push(decision);
            output.decisions[0].evidence_refs.reverse();
        }
        let first = build_fixture_r2_packet(&recipient, &claims, &outputs).unwrap();
        for output in &mut outputs {
            output.decisions.reverse();
            for decision in &mut output.decisions {
                decision.evidence_refs.reverse();
            }
        }
        let second = build_fixture_r2_packet(&recipient, &claims, &outputs).unwrap();
        assert_eq!(first.corpus, second.corpus);
        assert_ne!(
            first.r1_source_set_semantic_sha256,
            second.r1_source_set_semantic_sha256
        );
    }

    #[test]
    fn closure_requires_registered_rule_and_accepted_authorized_evidence() {
        let authorities = SCHOOL_BINDINGS
            .iter()
            .map(|(party, school)| SchoolAuthority {
                party_id: (*party).into(),
                school_id: *school,
                accepted_evidence_classes: vec!["allowed".into()],
                forbidden_claim_classes: Vec::new(),
                question_codes: Vec::new(),
            })
            .collect();
        let profile = FinanceReviewProfile {
            finance_review_profile_version: "1.0".into(),
            profile_id: "p".into(),
            schools: authorities,
            allowed_primary_contracts: Vec::new(),
            applicability_predicate_codes: Vec::new(),
            closure_rule_codes: vec!["registered".into()],
            hash_domains: Vec::new(),
        };
        let claims = FinanceClaimManifest {
            finance_claim_manifest_version: "1.0".into(),
            claims: vec![FinanceClaim {
                claim_id: "claim-1".into(),
                text: "claim".into(),
                claim_classes: vec!["class".into()],
                school_applicability: BTreeMap::new(),
            }],
        };
        let reference = format!("evidence:sha256:{}", "a".repeat(64));
        let evidence = FinanceEvidenceIndex {
            finance_evidence_index_version: "1.0".into(),
            as_of: "now".into(),
            evaluation_session: "session".into(),
            items: vec![FinanceEvidenceItem {
                evidence_ref: reference.clone(),
                evidence_class: "allowed".into(),
                binding: synthetic_binding("evidence/item.json", 'a'),
                status: EvidenceStatus::DescriptiveOnly,
                provenance_complete: true,
                available_at: "now".into(),
                expiry_session: None,
            }],
        };
        let mut outputs = SCHOOL_BINDINGS
            .iter()
            .map(|(_, school)| SchoolLaneOutput {
                school_lane_output_version: "1.0".into(),
                run_id: "run".into(),
                phase: FinancePhase::R1,
                school_id: *school,
                expected_route_digest: digest('1'),
                profile_digest: digest('2'),
                primary_digest: digest('3'),
                evidence_index_digest: digest('4'),
                input_packet_exact_sha256: digest('5'),
                input_packet_semantic_sha256: digest('6'),
                decisions: vec![SchoolClaimDecision {
                    claim_id: "claim-1".into(),
                    disposition: SchoolDisposition::Contradicted,
                    evidence_refs: Vec::new(),
                    alternative_codes: Vec::new(),
                    confounder_codes: Vec::new(),
                    falsifier_codes: Vec::new(),
                    invalidation_codes: Vec::new(),
                    limitation_codes: Vec::new(),
                    missing_evidence_codes: Vec::new(),
                    closure_rule_code: Some("registered".into()),
                    closure_evidence_refs: vec![reference.clone()],
                }],
                residuals: Vec::new(),
            })
            .collect::<Vec<_>>();
        assert!(validate_lane_claims_and_evidence(&profile, &claims, &evidence, &outputs).is_err());
        let mut accepted = evidence.clone();
        accepted.items[0].status = EvidenceStatus::Accepted;
        assert!(validate_lane_claims_and_evidence(&profile, &claims, &accepted, &outputs).is_ok());
        outputs[0].decisions[0].closure_rule_code = Some("unregistered".into());
        assert!(validate_lane_claims_and_evidence(&profile, &claims, &accepted, &outputs).is_err());
    }

    #[test]
    fn lifecycle_lock_excludes_a_concurrent_writer() {
        let temporary = tempfile::tempdir().unwrap();
        let state = temporary.path().join("state");
        let first = lifecycle_lock(&state).unwrap();
        assert!(lifecycle_lock(&state).is_err());
        drop(first);
        assert!(lifecycle_lock(&state).is_ok());
    }

    #[test]
    fn pending_transition_rejects_digest_and_artifact_drift() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let state = temporary.path().join("state");
        finance_fixture(&source, false);
        let manifest = dormant_init(&source, &state, DORMANT_WRITER_ACK).unwrap();
        let old_manifest_bytes = std::fs::read(state.join("manifest.json")).unwrap();
        stage_dormant_outputs(&source, &state, "r1");
        let mut target = manifest.clone();
        let (profile, _, _) =
            load_typed::<FinanceReviewProfile>(&state.join("input/profile.json")).unwrap();
        let (claims, _, _) =
            load_typed::<FinanceClaimManifest>(&state.join("input/claim-manifest.json")).unwrap();
        let (policy, _, _) = load_typed::<FinancePolicy>(&state.join("input/policy.json")).unwrap();
        let (invocation, _, _) =
            load_typed::<FinanceReviewInvocation>(&state.join("input/invocation.json")).unwrap();
        let mut outputs = Vec::new();
        for (_, school) in SCHOOL_BINDINGS {
            let name = serde_json::to_value(school)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let relative = format!("outputs/r1/{name}.json");
            let (output, bytes, value) =
                load_typed::<SchoolLaneOutput>(&state.join(&relative)).unwrap();
            outputs.push(output);
            target.r1_outputs.push(
                binding_for(
                    relative,
                    "school-lane-output",
                    "1.0",
                    "quinte.school-lane-output.v1",
                    &bytes,
                    &value,
                )
                .unwrap(),
            );
        }
        for authority in &profile.schools {
            let route = policy
                .school_bindings
                .iter()
                .find(|route| route.school_id == authority.school_id)
                .unwrap();
            let packet = build_r2_packet(
                &target.run_id,
                authority,
                &claims.claims,
                &outputs,
                &target.r1_outputs,
                target.policy.clone(),
                invocation.profile.clone(),
                invocation.claim_manifest.clone(),
                invocation.primary.binding.clone(),
                invocation.evidence_index.clone(),
                semantic_digest_value("quinte.finance-route-binding.v1", route).unwrap(),
            )
            .unwrap();
            let name = serde_json::to_value(authority.school_id)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let relative = format!("packets/r2/{name}.json");
            let value = serde_json::to_value(&packet).unwrap();
            let bytes = serde_json::to_vec_pretty(&packet).unwrap();
            write_idempotent(&state.join(&relative), &bytes).unwrap();
            target.r2_packets.push(
                binding_for(
                    relative,
                    "r2-packet",
                    "2.0",
                    "quinte.finance-r2-packet.v2",
                    &bytes,
                    &value,
                )
                .unwrap(),
            );
        }
        target.status = FinanceRunStatus::R2Running;
        let event = build_transition_event(Some(&manifest), &target).unwrap();
        let event_bytes = canonical_event_line(&event).unwrap();
        target.event_checkpoint = FinanceEventCheckpoint {
            sequence: event.sequence,
            event_sha256: exact_digest(&event_bytes),
        };
        let target_bytes = serde_json::to_vec_pretty(&target).unwrap();
        let mut pending = PendingFinanceTransition {
            pending_transition_version: "2.0".into(),
            run_id: target.run_id.clone(),
            run_genesis_digest: target.run_genesis_digest.clone(),
            transition_id: digest('a'),
            operation: "advance".into(),
            old_manifest_exact_sha256: Some(exact_digest(&old_manifest_bytes)),
            old_event_checkpoint: Some(manifest.event_checkpoint.clone()),
            event_exact_sha256: exact_digest(&event_bytes),
            event_byte_length: event_bytes.len() as u64,
            event_bytes_base64: base64::engine::general_purpose::STANDARD.encode(&event_bytes),
            target_manifest_exact_sha256: exact_digest(&target_bytes),
            target_manifest_byte_length: target_bytes.len() as u64,
            target_manifest_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&target_bytes),
            artifact_bindings: manifest_artifact_bindings(&target),
        };
        pending.target_manifest_exact_sha256 = digest('e');
        std::fs::write(
            state.join("pending-transition.json"),
            serde_json::to_vec_pretty(&pending).unwrap(),
        )
        .unwrap();
        assert!(reconcile_pending(&state).is_err());
        std::fs::remove_file(state.join("pending-transition.json")).unwrap();

        pending.target_manifest_exact_sha256 = exact_digest(&target_bytes);
        pending.artifact_bindings[0].exact_sha256 = digest('b');
        std::fs::write(
            state.join("pending-transition.json"),
            serde_json::to_vec_pretty(&pending).unwrap(),
        )
        .unwrap();
        assert!(reconcile_pending(&state).is_err());
    }
}
