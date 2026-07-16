//! Independent wire-contract revisions and embedded schema registry.
//!
//! These revisions describe serialized artifacts. They intentionally do not
//! derive from the Cargo package version.

pub const PROTOCOL_VERSION: &str = "1.0";
pub const POLICY_VERSION: &str = "1.0";
pub const BRIEF_VERSION: &str = "1.1";
pub const LEGACY_BRIEF_VERSION: &str = "1.0";
pub const RESULT_VERSION: &str = "2.0";
pub const RUN_MANIFEST_VERSION: &str = "1.0";
pub const RUN_EVENT_VERSION: &str = "1.0";
pub const LANE_OUTPUT_VERSION: &str = "1.0";
pub const SNAPSHOT_VERSION: &str = "1.0";
pub const R2_PACKET_VERSION: &str = "1.0";
pub const R3_INPUT_RECEIPT_VERSION: &str = "1.0";
pub const PRIMARY_ARBITER_CHALLENGE_VERSION: &str = "1.0";
pub const PRIMARY_ARBITER_RESPONSE_VERSION: &str = "1.0";
pub const PRIMARY_ARBITER_SUBMISSION_VERSION: &str = "1.0";
pub const ARBITER_VERDICT_VERSION: &str = "1.0";
pub const TRIAL_MANIFEST_VERSION: &str = "1.0";
pub const CLI_ENVELOPE_VERSION: &str = "1.0";
pub const DOCTOR_VERSION: &str = "1.0";
pub const EVIDENCE_PACKET_VERSION: &str = "1.0";
pub const TASK_PACKET_VERSION: &str = "1.0";
pub const RETRY_STATE_VERSION: &str = "1.0";
pub const RATE_STATE_VERSION: &str = "1.0";

pub const BRIEF_SCHEMA: &str = include_str!("../schemas/brief.schema.json");
pub const LEGACY_BRIEF_SCHEMA: &str = include_str!("../schemas/legacy/brief-1.0.schema.json");
pub const LANE_OUTPUT_SCHEMA: &str = include_str!("../schemas/lane-output.schema.json");
pub const PRIMARY_ARBITER_RESPONSE_SCHEMA: &str =
    include_str!("../schemas/primary-arbiter-response.schema.json");
pub const R3_INPUT_RECEIPT_SCHEMA: &str = include_str!("../schemas/r3-input-receipt.schema.json");
pub const RESULT_SCHEMA: &str = include_str!("../schemas/result.schema.json");
pub const LEGACY_RESULT_SCHEMA: &str = include_str!("../schemas/legacy/result-1.0.schema.json");
pub const RUN_MANIFEST_SCHEMA: &str = include_str!("../schemas/run-manifest.schema.json");
pub const RUN_EVENT_SCHEMA: &str = include_str!("../schemas/run-event.schema.json");

#[derive(Clone, Copy, Debug)]
pub struct ContractRevision {
    pub version: &'static str,
    pub schema: &'static str,
}

pub struct ContractSpec {
    pub name: &'static str,
    pub version_field: &'static str,
    pub current_version: &'static str,
    pub accepted_versions: &'static [&'static str],
    pub revisions: &'static [ContractRevision],
}

const BRIEF_REVISIONS: &[ContractRevision] = &[
    ContractRevision {
        version: LEGACY_BRIEF_VERSION,
        schema: LEGACY_BRIEF_SCHEMA,
    },
    ContractRevision {
        version: BRIEF_VERSION,
        schema: BRIEF_SCHEMA,
    },
];
const LANE_OUTPUT_REVISIONS: &[ContractRevision] = &[ContractRevision {
    version: LANE_OUTPUT_VERSION,
    schema: LANE_OUTPUT_SCHEMA,
}];
const PRIMARY_ARBITER_RESPONSE_REVISIONS: &[ContractRevision] = &[ContractRevision {
    version: PRIMARY_ARBITER_RESPONSE_VERSION,
    schema: PRIMARY_ARBITER_RESPONSE_SCHEMA,
}];
const R3_INPUT_RECEIPT_REVISIONS: &[ContractRevision] = &[ContractRevision {
    version: R3_INPUT_RECEIPT_VERSION,
    schema: R3_INPUT_RECEIPT_SCHEMA,
}];
const RESULT_REVISIONS: &[ContractRevision] = &[
    ContractRevision {
        version: "1.0",
        schema: LEGACY_RESULT_SCHEMA,
    },
    ContractRevision {
        version: RESULT_VERSION,
        schema: RESULT_SCHEMA,
    },
];
const RUN_MANIFEST_REVISIONS: &[ContractRevision] = &[ContractRevision {
    version: RUN_MANIFEST_VERSION,
    schema: RUN_MANIFEST_SCHEMA,
}];
const RUN_EVENT_REVISIONS: &[ContractRevision] = &[ContractRevision {
    version: RUN_EVENT_VERSION,
    schema: RUN_EVENT_SCHEMA,
}];

pub const CONTRACT_REGISTRY: &[ContractSpec] = &[
    ContractSpec {
        name: "brief",
        version_field: "brief_version",
        current_version: BRIEF_VERSION,
        accepted_versions: &[LEGACY_BRIEF_VERSION, BRIEF_VERSION],
        revisions: BRIEF_REVISIONS,
    },
    ContractSpec {
        name: "lane_output",
        version_field: "lane_output_version",
        current_version: LANE_OUTPUT_VERSION,
        accepted_versions: &[LANE_OUTPUT_VERSION],
        revisions: LANE_OUTPUT_REVISIONS,
    },
    ContractSpec {
        name: "primary_arbiter_response",
        version_field: "primary_arbiter_response_version",
        current_version: PRIMARY_ARBITER_RESPONSE_VERSION,
        accepted_versions: &[PRIMARY_ARBITER_RESPONSE_VERSION],
        revisions: PRIMARY_ARBITER_RESPONSE_REVISIONS,
    },
    ContractSpec {
        name: "r3_input_receipt",
        version_field: "input_receipt_version",
        current_version: R3_INPUT_RECEIPT_VERSION,
        accepted_versions: &[R3_INPUT_RECEIPT_VERSION],
        revisions: R3_INPUT_RECEIPT_REVISIONS,
    },
    ContractSpec {
        name: "result",
        version_field: "result_version",
        current_version: RESULT_VERSION,
        accepted_versions: &["1.0", RESULT_VERSION],
        revisions: RESULT_REVISIONS,
    },
    ContractSpec {
        name: "run_manifest",
        version_field: "manifest_version",
        current_version: RUN_MANIFEST_VERSION,
        accepted_versions: &[RUN_MANIFEST_VERSION],
        revisions: RUN_MANIFEST_REVISIONS,
    },
    ContractSpec {
        name: "run_event",
        version_field: "event_version",
        current_version: RUN_EVENT_VERSION,
        accepted_versions: &[RUN_EVENT_VERSION],
        revisions: RUN_EVENT_REVISIONS,
    },
    ContractSpec {
        name: "policy",
        version_field: "policy_version",
        current_version: POLICY_VERSION,
        accepted_versions: &[POLICY_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "snapshot",
        version_field: "snapshot_version",
        current_version: SNAPSHOT_VERSION,
        accepted_versions: &[SNAPSHOT_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "r2_packet",
        version_field: "packet_version",
        current_version: R2_PACKET_VERSION,
        accepted_versions: &[R2_PACKET_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "primary_arbiter_challenge",
        version_field: "challenge_version",
        current_version: PRIMARY_ARBITER_CHALLENGE_VERSION,
        accepted_versions: &[PRIMARY_ARBITER_CHALLENGE_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "primary_arbiter_submission",
        version_field: "submission_receipt_version",
        current_version: PRIMARY_ARBITER_SUBMISSION_VERSION,
        accepted_versions: &[PRIMARY_ARBITER_SUBMISSION_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "arbiter_verdict",
        version_field: "arbiter_verdict_version",
        current_version: ARBITER_VERDICT_VERSION,
        accepted_versions: &[ARBITER_VERDICT_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "trial_manifest",
        version_field: "manifest_version",
        current_version: TRIAL_MANIFEST_VERSION,
        accepted_versions: &[TRIAL_MANIFEST_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "cli_envelope",
        version_field: "cli_envelope_version",
        current_version: CLI_ENVELOPE_VERSION,
        accepted_versions: &[CLI_ENVELOPE_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "doctor",
        version_field: "doctor_version",
        current_version: DOCTOR_VERSION,
        accepted_versions: &[DOCTOR_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "evidence_packet",
        version_field: "evidence_packet_version",
        current_version: EVIDENCE_PACKET_VERSION,
        accepted_versions: &[EVIDENCE_PACKET_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "task_packet",
        version_field: "task_packet_version",
        current_version: TASK_PACKET_VERSION,
        accepted_versions: &[TASK_PACKET_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "retry_state",
        version_field: "retry_state_version",
        current_version: RETRY_STATE_VERSION,
        accepted_versions: &[RETRY_STATE_VERSION],
        revisions: &[],
    },
    ContractSpec {
        name: "rate_state",
        version_field: "rate_state_version",
        current_version: RATE_STATE_VERSION,
        accepted_versions: &[RATE_STATE_VERSION],
        revisions: &[],
    },
];

pub fn contract(name: &str) -> Option<&'static ContractSpec> {
    CONTRACT_REGISTRY
        .iter()
        .find(|contract| contract.name == name)
}

pub fn revision(
    contract: &'static ContractSpec,
    version: &str,
) -> Option<&'static ContractRevision> {
    contract
        .revisions
        .iter()
        .find(|revision| revision.version == version)
}

pub fn version_supported(contract: &ContractSpec, version: &str) -> bool {
    contract.accepted_versions.contains(&version)
        && (contract.revisions.is_empty()
            || contract
                .revisions
                .iter()
                .any(|revision| revision.version == version))
}

pub fn schema_id(contract: &ContractSpec, revision: &ContractRevision) -> String {
    let slug = contract.name.replace('_', "-");
    format!(
        "https://github.com/eric-stone-plus/QUINTE/contracts/{slug}/{}/schema.json",
        revision.version
    )
}

pub fn brief_version_supported(version: &str) -> bool {
    contract("brief").is_some_and(|contract| version_supported(contract, version))
}
