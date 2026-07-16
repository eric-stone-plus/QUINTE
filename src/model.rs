use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::contract::{BRIEF_VERSION, POLICY_VERSION, PROTOCOL_VERSION, RESULT_VERSION};
pub const TEXT_MODEL: &str = "mimo-v2.5-pro";
pub const MULTIMODAL_MODEL: &str = "mimo-v2.5";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Brief {
    pub brief_version: String,
    pub question: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub evidence_roots: Vec<PathBuf>,
    #[serde(default)]
    pub snapshot_ignore: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<PathBuf>,
    #[serde(default)]
    pub action_scope: Option<String>,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    #[serde(default)]
    pub action_binding_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub policy_version: String,
    pub roster: Vec<RoutePolicy>,
    pub counterpart_arbiter: RoutePolicy,
    pub text_model: String,
    pub multimodal_model: String,
    pub max_parallel_r1: usize,
    pub max_parallel_r2: usize,
    pub max_attempts: usize,
    pub timeout_seconds: u64,
    pub retry_backoff_seconds: u64,
    pub retry_backoff_max_seconds: u64,
    pub r2_min_interval_seconds: u64,
    pub max_output_bytes: usize,
    pub max_snapshot_files: usize,
    pub max_snapshot_bytes: u64,
    pub max_attachment_bytes: u64,
    pub sandbox_mode: SandboxMode,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicy {
    pub party_id: String,
    pub route_id: String,
    pub adapter: String,
    pub executable: String,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    Strict,
    Process,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEntry {
    pub snapshot_ref: String,
    pub source_name: String,
    pub sha256: String,
    pub bytes: u64,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentEntry {
    pub attachment_ref: String,
    pub source_name: String,
    pub sha256: String,
    pub bytes: u64,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotManifest {
    pub snapshot_version: String,
    pub created_at: String,
    pub entries: Vec<SnapshotEntry>,
    pub attachments: Vec<AttachmentEntry>,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Preflight,
    R1Running,
    R1Gate,
    R2Packet,
    R2Running,
    R2Gate,
    R3Cc,
    #[serde(alias = "waiting_hm")]
    WaitingPrimaryArbiter,
    Merging,
    Completed,
    Degraded,
    Failed,
    FailedPolicy,
    Cancelling,
    Cancelled,
}

impl RunStatus {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Degraded | Self::Failed | Self::FailedPolicy | Self::Cancelled
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub manifest_version: String,
    pub run_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: RunStatus,
    pub brief_sha256: String,
    pub policy_sha256: String,
    pub snapshot_sha256: String,
    pub runtime_sha256: String,
    pub protocol_version: String,
    pub effective_model: String,
    pub sandbox_mode: SandboxMode,
    pub current_phase: Option<String>,
    pub error: Option<RunError>,
    pub r3_input_receipt: Option<ArtifactBinding>,
    #[serde(alias = "hm_challenge")]
    pub primary_arbiter_challenge: Option<PrimaryArbiterChallenge>,
    #[serde(alias = "hm_submission")]
    pub primary_arbiter_submission: Option<PrimaryArbiterSubmissionReceipt>,
    pub result_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_version: String,
    pub sequence: u64,
    pub timestamp: String,
    pub run_id: String,
    pub event_type: String,
    pub phase: Option<String>,
    pub party_id: Option<String>,
    pub attempt: Option<usize>,
    pub data: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneOutput {
    pub lane_output_version: String,
    pub task_restatement: String,
    pub verdict: String,
    pub confidence: f64,
    pub claims: Vec<Claim>,
    pub residuals: Vec<Residual>,
    pub uncertainties: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    pub statement: String,
    pub evidence_refs: Vec<String>,
    pub confidence: f64,
    pub category: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Residual {
    pub id: String,
    pub severity: Severity,
    pub residual_type: String,
    pub source: String,
    pub finding: String,
    pub evidence_refs: Vec<String>,
    pub disposition: Disposition,
    pub required_closure: String,
    pub closure_state: ClosureState,
    pub closure_evidence: Vec<String>,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
    P0,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Verified,
    Falsified,
    Unresolved,
    Escalated,
    Discarded,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClosureState {
    Open,
    Closed,
    Blocked,
    Waived,
    NotApplicable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct R2Packet {
    pub packet_version: String,
    pub run_id: String,
    pub question: String,
    pub participants: BTreeMap<String, LaneOutput>,
    pub evidence_manifest_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArbiterVerdict {
    pub arbiter_verdict_version: String,
    pub summary: String,
    pub recommendation: String,
    pub residuals: Vec<Residual>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBinding {
    pub artifact_ref: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaneArtifactBinding {
    pub party_id: String,
    pub route_id: String,
    pub artifact_ref: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct R3InputReceipt {
    pub input_receipt_version: String,
    pub run_id: String,
    pub issued_at: String,
    pub r1: Vec<LaneArtifactBinding>,
    pub r2: Vec<LaneArtifactBinding>,
    pub evidence_packet: ArtifactBinding,
    pub cc_response: ArtifactBinding,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryArbiterChallenge {
    pub challenge_version: String,
    pub run_id: String,
    pub nonce: String,
    pub policy_sha256: String,
    pub evidence_packet_sha256: String,
    pub input_receipt_sha256: String,
    pub action_scope: Option<String>,
    pub issued_at: String,
    pub expires_at: String,
    pub consumed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryArbiterResponse {
    #[serde(alias = "hm_response_version")]
    pub primary_arbiter_response_version: String,
    pub run_id: String,
    pub nonce: String,
    pub policy_sha256: String,
    pub evidence_packet_sha256: String,
    pub input_receipt_sha256: String,
    pub action_scope: Option<String>,
    pub verdict: ArbiterVerdict,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryArbiterSubmissionState {
    Staging,
    Accepted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryArbiterSubmissionReceipt {
    pub submission_receipt_version: String,
    pub state: PrimaryArbiterSubmissionState,
    pub response_ref: String,
    pub response_sha256: String,
    pub input_receipt_sha256: String,
    pub staged_at: String,
    pub accepted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResultEnvelope {
    pub result_version: String,
    pub run_id: String,
    pub status: RunStatus,
    pub brief_sha256: String,
    pub question: String,
    pub action_scope: Option<String>,
    pub affected_paths: Vec<String>,
    pub action_binding_sha256: Option<String>,
    pub summary: String,
    pub recommendation: String,
    pub dissent: Vec<String>,
    pub residuals: Vec<Residual>,
    pub trial_manifest: TrialManifest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrialManifest {
    pub manifest_version: String,
    pub base_model_relation: String,
    pub perspective_count: usize,
    pub perspectives: Vec<TrialPerspective>,
    pub perturbation_axes: Vec<String>,
    pub independence_controls: Vec<String>,
    pub contamination_risks: Vec<String>,
    pub wall_time_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrialPerspective {
    pub party_id: String,
    pub route_id: String,
    pub r1_artifact: String,
    pub r2_artifact: String,
    pub independent_first_pass: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CliEnvelope<T: Serialize> {
    pub cli_envelope_version: &'static str,
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> CliEnvelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            cli_envelope_version: crate::contract::CLI_ENVELOPE_VERSION,
            ok: true,
            data,
        }
    }
}
