# Finance School Review Compatibility and Contract Change Map

Status: **design gate; no implementation in this change**

Baseline: QUINTE `0.2.4`, inspected 2026-08-15

Architecture input: [Finance School Review Redesign Memo](finance-school-review-redesign-memo.md)

## 1. Decision summary

The finance review is an additive protocol mode, not a reinterpretation of
QUINTE's current generic contracts. The implementation must introduce a
finance-specific ingress, school output, arbiter verdict, and result family,
plus new revisions of the shared run envelopes that carry their bindings.
Existing schema files and their meanings remain immutable.

The selected result contract is `finance-review-result/1.0`, with version key
`finance_review_result_version`. It is deliberately **not** a generic Result
3.0. This avoids colliding with historical `result/1.0` and preserves the
existing Result 2.1 `actionable` gate. Shared run state does advance to Run
Manifest 3.0 because its lifecycle and binding set change.

Finance runs use product Protocol 2.0. Fixed school authority, finance evidence
admission, restricted arbiter powers, and conjunctive publication semantics are
incompatible with the generic LaneOutput/arbiter/merge semantics normatively
defined by Protocol 1.0. This product-protocol revision is independent of A2A:
the A2A transport header, JSON-RPC methods, task states, and terminal artifact
cardinality remain at version 1.0.

The non-negotiable invariants are:

- one declared model family across the five lanes and both arbiter bindings;
- exactly five preregistered school authorities, with a fixed Party A-E map;
- immutable claim IDs, claim classes, and applicability inputs fixed before R1;
- one R1 and one R2 output for each school;
- GALAHAD remains authoritative for numerical calculation and primary status;
- QUINTE never repairs, invents, or promotes numerical evidence;
- code, not a model, computes `PUBLISH_BOUNDED` or `ABSTAIN`;
- no vote, score, confidence average, or arbiter override can promote a claim;
- completed finance runs are immutable; correction requires a new run; and
- Cross-family verification is out of scope under the single-vendor doctrine.

## 2. Baseline and frozen contracts

The following schema bytes are compatibility fixtures. The implementation must
pin these digests in tests and must not edit, rename, or repoint their `$id`s.
New revisions go in new files with unique contract identities.

| Contract identity | Repository file | SHA-256 of exact schema bytes |
| --- | --- | --- |
| `brief/1.0` | `schemas/legacy/brief-1.0.schema.json` | `dc62e8e79e962ea8057f3020ad71d451df7847e555c7af70e201fd7bc7fc1aa0` |
| `brief/1.1` | `schemas/brief.schema.json` | `c780ca57318586eaaa62ec7f20c2b28f83c46655836d9c3fc36caa300ba9e680` |
| `lane-output/1.0` | `schemas/lane-output.schema.json` | `23fa7ce691ba4b4ed2623b61e1e851dcfdcc2df6cfa4f455c0d7500bbae54ec2` |
| `arbiter-verdict/1.0` | `schemas/arbiter-verdict.schema.json` | `6d961e436e039fa1d0be4fc8f423c1fda42585cab1649117ff52274eda85bffb` |
| `primary-arbiter-response/1.0` | `schemas/primary-arbiter-response.schema.json` | `4034e8cb4e5d505e03b1b3ac0c4055852bcaf7b8fc25c7e1f16fa19ac9bc4750` |
| `legacy-hm-response/1.0` | `schemas/legacy/hm-response-1.0.schema.json` | `aafaa370136d21226374588936e0af8c48b7b3101e2aeed58c99fa2ab39baf96` |
| `r3-input-receipt/1.0` | `schemas/r3-input-receipt.schema.json` | `dd1468d1a109a6c2a06fdc79fbfb6e10b7d0147c029ca739b0d6f6fcb7a364b5` |
| `result/1.0` | `schemas/legacy/result-1.0.schema.json` | `a0d2febf3ae66508694f08cdd2ef23c126a2c2ee00e37e6c0d4d592332fc6590` |
| `result/2.0` | `schemas/legacy/result-2.0.schema.json` | `d895e09ce577e183e033071cbc1a84f49bedc8849b0f45bc3ff0516857a7a7f3` |
| `result/2.1` | `schemas/result.schema.json` | `3ece950427d6a77482b44aabe77b26cc2a7d16bbc5b75b4402aea01b3985eb59` |
| `run-manifest/1.0` | `schemas/legacy/run-manifest-1.0.schema.json` | `a016e5a8d7b6ff63174085310513d09f1f0eb6c122eb40e9a8d7f5f9e1eb96d0` |
| `run-manifest/2.0` | `schemas/run-manifest.schema.json` | `c9d5f0268d614732f18b3d7ce694b87a0a5f1d75df4fc51f7a9c144953365f0d` |
| `run-event/1.0` | `schemas/run-event.schema.json` | `5e74d64cd4d8116b685cf61e8b1c7c07505c159f396cefcf5a7cf93b9c2b24a7` |
| `host-receipt/1.0` | `schemas/host-invocation.schema.json` | `306406d9264de08b3624a182e84b6345976df85c141c5ccd06a41e334f0fb39b` |

The physical `host-invocation.schema.json` filename is historical; its `$id`
and contract identity are `host-receipt/1.0`.

Lane Output 1.0 is especially sensitive: Result 1.0, 2.0, and 2.1, Arbiter
Verdict 1.0, Primary Arbiter Response 1.0, and legacy HM Response 1.0 all
depend on its residual definition. It cannot become the school output by
adding optional fields or relaxing constraints.

## 3. Authoritative revision matrix

| Surface | Preserved reader semantics | Finance writer contract | Reason |
| --- | --- | --- | --- |
| Product protocol | `1.0` | `2.0` | The review and merge rules change normatively. |
| A2A transport | `1.0` | `1.0` | Transport operations and lifecycle do not change. |
| Brief | `1.0`, `1.1` | none | Finance ingress must not pass through lossy Brief normalization. |
| Finance review invocation | none | `finance-review-invocation/1.0` | Closed, native binding of profile, claim manifest, primary artifact, and evidence index. |
| Policy | `1.0` compatibility input; `2.0` generic | `3.0` | Binds school IDs, authorities, profile, and finance mode. |
| Finance review profile | none | `finance-review-profile/1.0` | Immutable doctrine and applicability authority. |
| Finance claim manifest | none | `finance-claim-manifest/1.0` | Preregisters stable claims, claim classes, and applicability inputs before findings exist. |
| Finance evidence index | none | `finance-evidence-index/1.0` | Typed evidence admission, provenance, cutoff, and lineage. |
| Snapshot manifest | `1.0` | `2.0` | Adds the immutable finance input bindings. |
| Task packet | `1.0` | `2.0` | Binds common inputs plus one school authority for R1. |
| R2 packet | `1.0` | `2.0` | Carries a purpose-built anonymized R1 projection. |
| Lane output | `1.0` | none | Historical generic family remains frozen. |
| School lane output | none | `school-lane-output/1.0` | Closed school decision and evidence contract for R1/R2. |
| Evidence/R3 packet | `1.0` | `2.0` | Binds all finance inputs and accepted school outputs. |
| Arbiter verdict | `1.0` | none | Generic recommendation-bearing verdict remains frozen. |
| Finance arbiter verdict | none | `finance-arbiter-verdict/1.0` | Reconciliation only; it has no publication authority. |
| R3 input receipt | `1.0` | `2.0` | The immutable input-binding set changes. |
| Primary-arbiter packet, challenge, response, and submission receipt | emitted or embedded legacy shapes at `1.0` (packet currently unregistered) | `2.0` | Binding and verdict semantics change; all four need explicit schemas and registry entries. |
| Run event | `1.0` | `2.0` | Typed event-kind payloads are required for offline replay. |
| Pending transition | current internal versionless shape | `2.0` | Journals exact event/manifest bytes and checkpoint digests for deterministic recovery. |
| Run manifest | `1.0`, `2.0` | `3.0` | New persisted run mode and complete binding graph. |
| Result | `1.0`, `2.0`, `2.1` | none | Generic family and Result 2.1 actionability remain frozen. |
| Finance review result | none | `finance-review-result/1.0` | Unambiguous finance result and deterministic posture. |
| Host receipt | `1.0` | `2.0` | Finance inspection needs a distinct gate and binding projection. |
| Finance freshness observation | none | `finance-freshness-observation/1.0` | Pins the calendar/session fact used by a consumer-time expiry gate. |
| Retry state, rate state | `1.0` | `2.0` | Serialized finance state must bind run, school/route, policy, and manifest identity against replay or swapping. |
| A2A task record | current internal versionless shape | `2.0` | Persists a sanitized finance task identity without replaying raw input bytes. |
| Trial manifest | `1.0` | none | Frozen generic Result 2.1 subobject; Finance Result uses native school/trace bindings instead. |
| CLI envelope, doctor | `1.0` | `1.0` | Enclosure by a finance run alone does not justify a bump. |

Retry State 2.0 and Rate State 2.0 are selected, not conditional. Their v1
objects contain no run ID, school ID, policy/manifest digest, or equivalent
anti-replay binding. Path context is not a serialized binding. Finance writers
therefore emit v2, while runtime and host readers continue to dispatch and
validate v1 for generic runs.

All new finance families use family-specific version keys:
`finance_review_invocation_version`, `finance_review_profile_version`,
`finance_claim_manifest_version`, `finance_evidence_index_version`,
`school_lane_output_version`, `finance_arbiter_verdict_version`,
`finance_review_result_version`, and
`finance_freshness_observation_version`.
Dispatch must first require exactly one mutually exclusive family discriminator,
then select the matching strict parser. Shared envelopes retain their existing
keys, such as `manifest_version`, `snapshot_version`, and `event_version`.
New persisted internal contracts use `pending_transition_version` and
`task_record_version`.

Shared families now have simultaneous writers. A single global
`current_version` or constants such as `POLICY_VERSION`,
`RUN_MANIFEST_VERSION`, `RUN_EVENT_VERSION`, `SNAPSHOT_VERSION`, or
`TASK_PACKET_VERSION` cannot select output safely: advancing one would make the
generic path emit finance revisions, while leaving it unchanged would make the
finance path impossible. Contract registration must separate accepted reader
revisions from explicit per-mode writer revisions. Policy normalization,
packet/event factories, store saves, result verification, and receipt emission
select `GenericWritable` or `FinanceWritable` from the already-bound run or
endpoint capability, never from a process-global "current" revision.

The legacy schema-less registry entries need special care. Today,
`ContractSpec::version_supported` permits an accepted version when a contract's
`revisions` list is empty, but requires a matching concrete revision as soon as
the list is non-empty. Adding only a new schema would therefore make accepted
old artifacts unusable. The exact schema-less registry inventory is Policy,
Snapshot, R2 Packet, Primary Arbiter Challenge, Primary Arbiter Submission,
Trial Manifest, CLI Envelope, Doctor, Evidence Packet, Task Packet, Retry
State, and Rate State. In addition, Primary Arbiter Packet 1.0 is emitted with
a version field but has no constant, registry entry, or schema. R3 Input
Receipt and Primary Arbiter Response are already schema-backed and are not in
this category.

Before registering a new revision for an advancing family, either:

1. freeze every historically accepted shape as an explicit legacy schema,
   then register all old revisions and the new revision; or
2. refactor the registry to distinguish schema-backed and typed-legacy
   validators explicitly.

The first option is preferred because it gives offline consumers portable
contracts. Policy 1.0 and 2.0 must both remain accepted when Policy 3.0 is
registered. Primary Arbiter Packet 1.0 must first be frozen and registered,
then v2 added. Trial Manifest, CLI Envelope, and Doctor remain explicit
typed-legacy v1 validators or gain frozen v1 schemas. Trial Manifest 1.0 is not
embedded in Finance Result: its generic party/route/path structure cannot carry
the required school identities and digests. Finance Result uses its native
school/output/trace bindings and the run genesis identity that Manifest 3.0
also binds. It does not bind the exact terminal Manifest bytes; the terminal
Manifest binds the result, so the opposite edge would create a digest cycle.
Never solve this by removing old acceptance or using one permissive schema for
multiple revisions.

New schema filenames and identities are fixed before implementation. Every
file has the exact `$id`
`https://github.com/eric-stone-plus/QUINTE/contracts/<family>/<revision>/schema.json`;
the version property named below is a required constant. Existing current-file
names are not repointed to a new revision.

| New file | Contract identity | Required version property |
| --- | --- | --- |
| `schemas/finance-review-invocation.schema.json` | `finance-review-invocation/1.0` | `finance_review_invocation_version: "1.0"` |
| `schemas/finance-review-profile.schema.json` | `finance-review-profile/1.0` | `finance_review_profile_version: "1.0"` |
| `schemas/finance-claim-manifest.schema.json` | `finance-claim-manifest/1.0` | `finance_claim_manifest_version: "1.0"` |
| `schemas/finance-evidence-index.schema.json` | `finance-evidence-index/1.0` | `finance_evidence_index_version: "1.0"` |
| `schemas/school-lane-output.schema.json` | `school-lane-output/1.0` | `school_lane_output_version: "1.0"` |
| `schemas/finance-arbiter-verdict.schema.json` | `finance-arbiter-verdict/1.0` | `finance_arbiter_verdict_version: "1.0"` |
| `schemas/finance-review-result.schema.json` | `finance-review-result/1.0` | `finance_review_result_version: "1.0"` |
| `schemas/finance-freshness-observation.schema.json` | `finance-freshness-observation/1.0` | `finance_freshness_observation_version: "1.0"` |
| `schemas/policy-3.0.schema.json` | `policy/3.0` | `policy_version: "3.0"` |
| `schemas/snapshot-2.0.schema.json` | `snapshot/2.0` | `snapshot_version: "2.0"` |
| `schemas/task-packet-2.0.schema.json` | `task-packet/2.0` | `task_packet_version: "2.0"` |
| `schemas/r2-packet-2.0.schema.json` | `r2-packet/2.0` | `packet_version: "2.0"` |
| `schemas/evidence-packet-2.0.schema.json` | `evidence-packet/2.0` | `evidence_packet_version: "2.0"` |
| `schemas/r3-input-receipt-2.0.schema.json` | `r3-input-receipt/2.0` | `input_receipt_version: "2.0"` |
| `schemas/primary-arbiter-packet-2.0.schema.json` | `primary-arbiter-packet/2.0` | `primary_arbiter_packet_version: "2.0"` |
| `schemas/primary-arbiter-challenge-2.0.schema.json` | `primary-arbiter-challenge/2.0` | `challenge_version: "2.0"` |
| `schemas/primary-arbiter-response-2.0.schema.json` | `primary-arbiter-response/2.0` | `primary_arbiter_response_version: "2.0"` |
| `schemas/primary-arbiter-submission-receipt-2.0.schema.json` | `primary-arbiter-submission/2.0` | `submission_receipt_version: "2.0"` |
| `schemas/run-event-2.0.schema.json` | `run-event/2.0` | `event_version: "2.0"` |
| `schemas/pending-transition-2.0.schema.json` | `pending-transition/2.0` | `pending_transition_version: "2.0"` |
| `schemas/run-manifest-3.0.schema.json` | `run-manifest/3.0` | `manifest_version: "3.0"` and `protocol_version: "2.0"` |
| `schemas/host-receipt-2.0.schema.json` | `host-receipt/2.0` | `host_receipt_version: "2.0"` |
| `schemas/retry-state-2.0.schema.json` | `retry-state/2.0` | `retry_state_version: "2.0"` |
| `schemas/rate-state-2.0.schema.json` | `rate-state/2.0` | `rate_state_version: "2.0"` |
| `schemas/a2a-task-record-2.0.schema.json` | `a2a-task-record/2.0` | `task_record_version: "2.0"` |

The preferred legacy-freeze files use the corresponding exact historical
identity under `schemas/legacy/`, for example `policy-1.0.schema.json`,
`policy-2.0.schema.json`, `snapshot-1.0.schema.json`,
`primary-arbiter-packet-1.0.schema.json`, and the other schema-less v1 family
names enumerated above. Those schemas describe observed accepted bytes; they
do not normalize or retrofit historical run files. The repository-path freeze
in section 2 is intentional because those paths are themselves test fixtures,
in addition to the portable invariants of exact bytes and `$id`.

## 4. Fixed authority and data flow

The five policy bindings are exact and cannot be selected or renamed by a
model:

| Wire party | `school_id` |
| --- | --- |
| Party A | `factor_risk_model` |
| Party B | `event_driven` |
| Party C | `fundamental_supply_chain` |
| Party D | `trend_technical_regime` |
| Party E | `market_microstructure` |

The profile and Policy 3.0 jointly bind this map before run creation. A school
output repeats its expected ID for validation but has no authority to declare
or change it. Missing, duplicate, swapped, unknown, self-declared, or
route-mismatched IDs fail the run.

```text
GALAHAD CalculationArtifact + profile + finance evidence index
  + finance claim manifest
  -> finance-review-invocation/1.0
  -> Snapshot 2.0 + Policy 3.0 + Manifest 3.0 (Protocol 2.0)
  -> R1: five isolated Task Packet 2.0 inputs
       -> five school-lane-output/1.0 artifacts
  -> R2: one identity-scrubbed corpus in five R2 Packet 2.0 wrappers
       -> five school-lane-output/1.0 artifacts
  -> Evidence Packet 2.0
  -> counterpart finance-arbiter-verdict/1.0
  -> R3 Input Receipt 2.0
  -> Primary Arbiter Packet/Challenge/Response 2.0
       (the response wraps finance-arbiter-verdict/1.0)
  -> deterministic merge and publication-posture function
  -> finance-review-result/1.0 + Manifest 3.0 terminal bindings
  -> durable human-confirmed finance-product registration
  -> STAMMTISCH finance gate and offline-verifiable bundle
```

R1 packets contain the common immutable inputs and only the receiving lane's
authority packet; no other lane output is reachable. R2 must not embed raw
SchoolLaneOutput objects. It carries a separately defined projection of R1
claims and residuals built only from closed typed codes, preregistered claim
IDs/text, dispositions, residual codes, and bound evidence references. It
excludes all lane-authored free text, raw findings, source labels, and metadata
that could identify an author. Route, party, school, adapter, ordering,
artifact paths, digests, or aliases also cannot reveal the contributor. The
complete serialized packet, not merely the `participants` keys, is the
anonymity test surface. This is contributor anonymity, not anonymity of the
recipient's own role: each R2 school receives a recipient-specific Packet 2.0
wrapper that hash-binds its own authority/profile and the common anonymized
corpus. A collision-free deterministic alias map is stable across resume but
is never exposed to the model.

Packet and prompt separation alone does not enforce the memo's information-set
boundary against a process running with the invoking user's filesystem
authority. Finance Policy 3.0 therefore requires an implemented, tested
isolation backend that denies each lane access to the run root, sibling working
directories, provider logs, and unlisted host paths while exposing only a
read-only per-attempt input mount and its output sink. A backend that merely
sets a working directory or requests tool restrictions is insufficient.
Finance preflight fails if this capability is unavailable; generic process-mode
semantics remain unchanged.

The two arbiters may reconcile identifiers, duplicates, contradictions, and
scope. They cannot change a school disposition or applicability, change the
primary status, or close a material residual without closure evidence already
admitted and bound. The deterministic merger independently validates those
constraints before evaluating posture.

Each School Lane Output 1.0 is a closed object bound to the run, phase, school,
route expectation, profile, primary artifact, evidence index, affected claim
IDs, and the exact indexed evidence it actually used. Typed fields, not prose,
carry tested alternatives, confounders, falsifiers, invalidations, limitations,
missing evidence, material residuals, and one disposition from `clear`,
`contradicted`, `insufficient_evidence`, `quarantined`, `expired`, or the
profile-authorized `not_applicable`. There is exactly one accepted R1 and one
accepted R2 object per school; retry attempts do not count as additional
opinions.

The claim manifest is a separate closed, immutable artifact bound by the
invocation before a run directory is created. It assigns stable claim IDs and
preregisters each claim's text, claim class (including timing, liquidity,
execution, and intraday classifications), predicate inputs, and mandatory,
conditional, or out-of-scope school set. The profile owns the predicate code
and vocabulary; the claim manifest supplies its frozen inputs. Invocation,
Snapshot 2.0, Policy 3.0, every R1/R2/R3 packet, Manifest 3.0, and Finance
Review Result 1.0 bind the claim manifest's exact and semantic identities.

Final per-claim, per-school disposition is a code-derived conservative fold of
that school's accepted R1 and R2 objects, not an arbiter decision. A blocker or
material residual in either phase remains blocking unless already-admitted,
bound closure evidence satisfies the profile's preregistered closure rule.
R2 cannot erase an R1 contradiction merely by returning `clear`, and a newly
introduced R2 blocker cannot be ignored because R1 was clear. The arbiters may
reconcile identifiers and duplicates only after this fold's inputs validate.

Finance Review Result 1.0 binds the exact and semantic primary, profile, claim-
manifest, and evidence-index identities; all ten accepted school outputs; the
complete R3 input and both arbiter input/output bindings; one final disposition
and applicability result for each school on each claim; material residual and
closure-evidence state; active invalidations; evaluation and expiry sessions;
the code-derived posture and reason codes; and the seven route/model-family
bindings with an explicit same-family error-correlation contamination risk.
Free-form summary text is explanatory only and cannot be the sole copy of any
decision or binding.

Runtime completion is separate from publication posture. A finance run may
finish as `completed` or `degraded` and still be `ABSTAIN`; A2A
`TASK_STATE_COMPLETED` means only that the product emitted its one terminal
artifact.

## 5. Binding and digest contract

Every portable artifact binding has a reusable core containing:

- a portable artifact reference, never a local ingestion path;
- contract family or schema ID and exact revision;
- ordinary SHA-256 of the exact stored source bytes;
- semantic digest and its registered domain when the artifact has semantic
  identity.

Admission metadata is domain-specific rather than copied into every binding.
Primary and evidence bindings additionally carry or reference the status,
cutoff, provenance, availability, and expiry facts needed for admission;
packet, result, event, and receipt bindings do not acquire incoherent evidence
fields merely because they use the same binding core.

Exact and semantic hashes are intentionally independent. Semantic digests use:

```text
SHA-256(domain ASCII || NUL || canonical UTF-8 JSON)
```

The canonical JSON algorithm must be specified as RFC 8785 JCS, or as an
equally complete restricted canonical form before schemas are accepted.
`serde_json::to_vec` is not a cross-language canonicalization contract.
Parsers must accept only strict UTF-8, reject duplicate object member names
before semantic parsing, and emit lowercase `sha256:<64 hex>` strings.
Known-answer vectors must be implemented in QUINTE and at least one independent
consumer. Reordering keys or changing insignificant whitespace must preserve
the semantic digest and change the exact-byte digest; drift in either bound
value fails closed.

No digest includes itself. An artifact's enclosing exact-byte digest always
lives in an external binding and covers every stored byte; a field inside the
artifact may bind an input or named projection, but cannot claim to be the
ordinary SHA-256 of the enclosing bytes. For every semantic domain, the
contract defines a named unsigned projection that removes its own semantic-
digest field before JCS. External bindings are preferred for semantic digests
too, but a contract may carry one when that exclusion is explicit.
Profile, claim manifest, evidence index, invocation, school output, finance
arbiter verdict, finance result, posture, run genesis, and freshness-observation
vectors must each publish the exact projection and known answer; a generic JCS
vector alone is insufficient. The scheduler also preserves the exact extracted
authoritative candidate bytes for school and arbiter outputs and separately
binds provider stdout/control-envelope bytes. It must not define "exact output"
as an unacknowledged typed reserialization.

The minimum semantic domains are:

- `quinte.finance-review-invocation.v1`;
- `quinte.finance-review-profile.v1`;
- `quinte.finance-claim-manifest.v1`;
- `quinte.finance-evidence-index.v1`;
- `quinte.school-lane-output.v1`;
- `quinte.finance-arbiter-verdict.v1`;
- `quinte.finance-review-result.v1`;
- `quinte.finance-publication-posture.v1`;
- `quinte.finance-run-genesis.v1`;
- `quinte.finance-freshness-observation.v1`.

The profile's hash-domain registry may allow additional primary-artifact and
evidence domains, but only from an explicit allowlist.

### Finance ingress and primary authority

`finance-review-invocation/1.0` is the sole native ingress for finance mode. It
binds the exact profile, claim manifest, one primary CalculationArtifact, one
evidence index, the question/context, and their schema identities, revisions,
exact-byte digests, and semantic digests. It is the portable persisted contract
and contains no host paths. CLI/A2A ingestion is a separate locator mapping
that maps each declared portable reference to exactly one command argument or
raw part. Missing, extra, duplicate, or mismatched mappings fail before the
copy transaction. CLI filenames and transport part IDs are locators only and
never enter persisted portable references.

The profile allowlists exact GALAHAD CalculationArtifact contract revisions
and digest domains; QUINTE references those upstream contracts rather than
copying their numerical schema. The accepted primary input must expose a typed
primary status (`accepted`, `descriptive_only`, `quarantined`, or `expired`),
evaluation and expiry sessions, active invalidations, and the upstream
validated-bar, calendar, session-map, configuration, code, environment, and
lineage digests needed by the memo. If the current generic ResearchKit
CalculationArtifact does not carry this finance envelope, GALAHAD must first
publish an immutable domain-specific wrapper or revision. QUINTE must not
infer the fields from prose or duplicate GALAHAD's calculations.

### Evidence index

Every evidence entry declares its class, schema/revision, exact and semantic
digests, `as_of`, availability, retrieval, source provenance, evaluation
session, and expiry where applicable. Numerical lineage accepts only profile-
allowed validated quantitative artifacts. News, generated dailies, summaries,
and article counts are discovery/narrative evidence and cannot provide prices,
returns, statistics, sample sizes, or numerical lineage. Evidence available
after the cutoff cannot affect the run; admitting it requires a new invocation
with a later cutoff.

Snapshot 2.0 closes the transitive offline input set. Every byte object
referenced by the primary's required provenance or the evidence index—validated
bars, calendar, session map, configuration, code/environment artifacts, and
other profile-required sources—is either copied into the immutable snapshot or
is represented by a profile-authorized immutable content-addressed object that
the exported bundle must include. Creation rejects dangling references,
duplicate portable references, inconsistent duplicate digests, unreferenced
substitutions, and declared-but-unsupplied entries. A digest without the bound
bytes is not sufficient for the promised offline verification.

### Manifest and replay

Manifest 3.0 is the root of the finance binding graph. It binds the invocation,
exact profile and claim-manifest bytes and semantic digests, primary bytes and
semantic digest, evidence index, Snapshot 2.0, Policy 3.0, fixed school map,
every R1/R2 output, Evidence Packet 2.0, the standalone counterpart verdict,
R3 Input Receipt 2.0, Primary Arbiter Packet/Challenge/Response/Submission
Receipt 2.0, the Primary Response's wrapped verdict, runtime and model-family
bindings, and the final finance result's contract family, schema ID, revision,
exact digest, and semantic digest. R3 Input Receipt 2.0 binds only the
pre-primary inputs and standalone counterpart output; the later packet,
response, submission receipt, manifest, and result bind the primary path. An
accepted artifact binding must be durably recorded before any phase-complete
decision relies on the artifact's existence.

The digest graph is acyclic: immutable inputs and writer capability feed the
run-genesis projection; later artifacts and the Finance Result bind that
genesis plus their prior inputs; the terminal event binds the result; and the
terminal Manifest binds the result and terminal event checkpoint. Downstream
receipts and bundles bind the exact terminal Manifest and Result bytes. Neither
the result nor an event binds the mutable/terminal Manifest's exact digest.

Run Event 2.0 uses event-kind-specific closed payloads. The current v1
`data: {}` surface cannot prove offline that profile, claim, primary, evidence,
and school bindings survived replay. Event 2.0 is a new exact-byte hash chain,
not a property inherited from Event 1.0:

- Manifest 3.0 carries a `run_genesis_digest`, computed in its own registered
  domain over a named immutable creation projection that excludes lifecycle
  status, timestamps, event checkpoint, and later artifact/result bindings.
- Every event binds that genesis digest, a contiguous sequence, and
  `previous_event_sha256` (`null` only for sequence zero). Its stored line is
  strict JCS UTF-8 plus one LF; the event SHA-256 covers that complete line and
  lives in the next event, pending journal, or manifest rather than recursively
  inside the event itself.
- Manifest 3.0 carries the authoritative checkpoint `{sequence,
  event_sha256}`. This detects suffix deletion; chain validation detects edit,
  insertion, reordering, and cross-run splicing.
- One run-transition lock governs journal, event, and manifest updates. A
  Pending Transition 2.0 first binds the old manifest/checkpoint digest, exact
  staged event bytes/digest, and exact target manifest bytes/digest. After it
  is fsynced, the event is appended and fsynced, then the manifest is atomically
  replaced and its directory fsynced, then the journal is cleared. Recovery
  deterministically completes or rejects those exact bytes; it never creates a
  semantically similar replacement event.
- The final `run.terminalized` event binds terminal status and the result
  binding when one exists. It is appended before the terminal manifest, which
  checkpoints it. No event is appended after a terminal manifest, avoiding a
  manifest/event digest cycle and forbidden terminal rewrite.

Ledger reading dispatches each line by version, requires one event revision
matching the manifest, and verifies directory/run identity, genesis, chain,
checkpoint, and contiguous sequence. Event 1.0 and 2.0 cannot mix in one run.
Creation, transition, reconciliation, resume, and terminal verification
recompute the graph from persisted bytes rather than trust a deserialized
manifest summary.

Manifest 3.0 also has monotonic phase/status invariants beyond field presence.
Each phase requires every prior accepted binding; terminal `completed` or
`degraded` requires the complete graph and one Finance Result family/revision
with both result digests. Failed and cancelled states bind their last valid
checkpoint and typed failure facts but expose no result. An orphan result is
never authoritative. Schema conditionals enforce what JSON Schema can express,
and a closed semantic validator rejects every illegal status/binding
combination.

## 6. Deterministic publication posture

For every claim, code computes applicability from the pinned profile and the
pre-run claim classification. A conditional school may be `not_applicable`
only when its registered predicate evaluates false. Neither a lane nor an
arbiter may waive it after seeing findings.

The posture function is conjunctive:

```text
primary status == accepted
AND primary is unexpired at the bound evaluation session
AND required primary and evidence provenance is complete
AND every mandatory school disposition == clear
AND (every conditional school is clear
     OR is not_applicable under its preregistered false predicate)
AND no material residual is open
AND no active invalidation exists
    => PUBLISH_BOUNDED
otherwise
    => ABSTAIN
```

The implementation should return typed reason codes for every failed conjunct
and include the exact inputs and function revision in the
`quinte.finance-publication-posture.v1` digest. The function never reads arbiter
prose, lane support counts, confidence values, or a recommendation string.
`PUBLISH_BOUNDED` is a publication eligibility statement, not a trade,
position, order, or authorization.

Run status is a second deterministic function over runtime facts, never a
model field: successful complete execution maps to `completed`; only a closed,
enumerated non-integrity degradation condition may map to `degraded`. A valid
school finding such as `contradicted` or `insufficient_evidence`, or a valid
non-accepted primary status, is a domain outcome: the run completes with an
immutable Finance Result whose posture is `ABSTAIN`. Missing, invalid, or
under-bound mandatory output, or any contract/binding/integrity failure, maps
to a failed state and exposes no result. Both successful terminal statuses may
carry either posture as allowed by that closed mapping, and A2A maps both to
`TASK_STATE_COMPLETED`. The implementation must publish the full
status/posture truth table before enabling the writer.

The immutable result posture is evaluated at the result's bound evaluation
session; it does not mutate when time advances. Every later consumer applies a
separate freshness gate. The consumer derives a Finance Freshness Observation
1.0 from its trusted clock, the result-bound calendar/session map, and a pinned
algorithm revision; the observation binds the result digests, schedule digest,
observed instant/session, expiry session, and gate outcome. Host Receipt 2.0
embeds or binds QUINTE's observation, while STAMMTISCH Gate Record v1 binds its
own. Historical offline replay verifies the recorded observation, never the
verifier's wall clock; a new act of consumption creates a new observation and
therefore cannot reuse a pre-expiry gate after expiry.

## 7. Compatibility behavior

### Read and write matrix

| Binary capability | Historical generic run | New finance run |
| --- | --- | --- |
| Legacy binary | Existing behavior in the generic namespace | Must never be pointed at the finance namespace. A legacy-capability conformance harness must reject Manifest 3.0 rather than coerce, skip, or write it. |
| Dual-reader before finance writer enablement | Read and verify without rewriting bytes | Read/verify fixtures; creation disabled. |
| Finance-enabled binary | Generic creation continues to load/copy Policy 2.0 and emit Manifest 2.0 and Result 2.1 | Loads/copies Policy 3.0 and emits Manifest 3.0 and Finance Review Result 1.0. |

The run mode is selected and validated before a run directory is created. It
cannot change on resume. Historical completed runs remain byte-for-byte
inspectable; no automatic migration, normalization, or write-back is allowed.
This does not revoke the existing explicit generic `primary-arbiter amend`
operation for eligible Manifest 2.0/Result 2.1 runs. That compatibility path
remains generic-only and continues to use its existing validated rewrite
semantics; finance terminal state never enters it. Unknown, mixed-family, or
cross-revision combinations fail closed.

Before the first Manifest 3.0 is published, creation must strictly validate,
copy, and hash the exact invocation, profile, claim manifest, primary artifact,
evidence index, and closed transitive snapshot bytes, then verify their semantic
bindings. Any failure removes the unpublished UUID run tree. No caller may
observe a run whose root manifest exists before those immutable inputs are
durable.

Policy selection is a deployment authority, not a field inferred from ingress.
The generic state root/endpoint loads and copies its pinned Policy 2.0; the
dedicated finance state root/endpoint loads and copies its pinned Policy 3.0.
The current single `<state-root>/policy.json` and `host::policy_or_error` cannot
serve both modes, so each endpoint has its own state root and policy file.
Invocation parsing may confirm the endpoint family but cannot choose a policy
or widen a generic endpoint. Historical status, observations, timeout, retry,
and recovery logic reads the run-copied bound policy (or immutable manifest
projection), never the endpoint's current global policy.

`src/store.rs` currently deserializes every manifest directly into one
`RunManifest` and validates every save as Manifest 2.0. It must first inspect
strict JSON and `manifest_version`, then dispatch to separate typed legacy and
finance models. Mutating APIs must carry an explicit capability such as
`LegacyReadOnly`, `GenericWritable`, or `FinanceWritable`. Manifest 1.0 reads
and completed Manifest 2.0 reads never become finance writes; an active,
supported Manifest 2.0 remains writable only through the generic path; no
revision is up-converted; and an old or incapable binary never resumes or
reconciles Manifest 3.0 by projecting it onto Manifest 2.0. Active-run scans
must fail the whole operation on an unknown manifest and must never skip it and
conclude that starting another run is safe.

Read-only operations stay read-only. `list`, `status`, `inspect`, host preflight,
A2A `GetTask`/`ListTasks`, event viewing, and offline verification may report a
pending transition or corrupt/torn ledger but never complete a journal, append,
truncate, replace a manifest, delete an orphan, or otherwise repair bytes.
Only an explicit, writer-capable `reconcile` or `resume` path may execute the
Pending Transition 2.0 recovery protocol. Reader dispatch therefore cannot call
the current mutation-capable `load_manifest`/`read_events` recovery behavior.

`verify_result_integrity` currently makes a result actionable when
`revision.version == RESULT_VERSION`. Globally changing `RESULT_VERSION` would
silently make Result 2.1 non-actionable. Replace this with explicit contract-
family dispatch: preserve the Result 2.1 integrity/actionability decision
exactly, and expose a separate finance verification result containing
contract name, schema ID, revision, gate kind, `publication_posture`, and the
finance gate outcome. Manifest 3.0 declares the expected result family and
revision; the verifier requires exactly one matching discriminator and checks
both result digests. It must not select a parser solely from attacker-controlled
result bytes or accept a swapped generic Result 2.1.

Finance adapter output is strict. The generic adapter may retain its historical
candidate extraction, shape normalization, and uniquely-near evidence-reference
remapping for generic runs, but finance lanes and arbiters must accept only the
authoritative final strict-JSON object from each provider's documented wrapper,
validate its closed contract, and require exact evidence references and
digests. Provider control envelopes may frame the object but never repair it or
supply decision fields. No unquoted-key repair, scalar-to-array normalization,
synthesized ID, fuzzy provenance remap, or fallback to an older candidate is
permitted; a later malformed or invalid candidate invalidates an earlier valid
one.

`primary-arbiter amend` must reject every finance run at the CLI, run API, and
store terminal-write capability boundary. Manual primary submission is not
selected by this map. Finance mode is automatic-primary-only and rejects use of
`primary-arbiter request`, `submit`, and `amend`; the automatic path emits
Primary Arbiter Response 2.0 with its wrapped Finance Arbiter Verdict 1.0.
Once a finance result is terminal, any correction, newly admitted fact, later
cutoff, or changed profile starts a new run linked by an explicit supersession
reference.

### A2A and host boundary

A2A remains transport 1.0:

- `A2A-Version: 1.0` remains required;
- the implemented `SendMessage`, `GetTask`, `ListTasks`, and `CancelTask`
  methods retain their meanings;
- task-state mapping remains unchanged;
- a terminal task still carries exactly one artifact named `review.result`;
- STAMMTISCH's `a2a.invocation.v1` receipt remains unchanged; and
- finance versus generic is discriminated, after decoding the part, by the
  payload's mutually exclusive contract-family version key, not by changing
  the transport or adding ambiguous artifact metadata.

The finance part also carries a closed contract binding naming the family,
schema ID, and revision. Those values must agree with the decoded payload's
exclusive version key and Manifest 3.0's expected-result binding; they are not
an alternate discriminator. A completed task with any artifact count other
than one, any artifact name other than `review.result`, or disagreeing part and
payload identities fails closed.

The Agent Card must advertise the finance input/output contract capability and
its schema identities. Because STAMMTISCH pins the entire canonical card
digest, this is a coordinated identity change even though the A2A version does
not change. The card's product `version` must not be sourced from the product
`PROTOCOL_VERSION` constant; HOST.md says it follows the package version, while
the current implementation uses `PROTOCOL_VERSION`.

A Card must not advertise an invocable finance capability while creation is
disabled. Reader-only deployments retain the generic Card (or a separately
specified non-invocable availability state); the finance-capable Card is
published atomically with the pinned finance endpoint writer authorization and
downstream gates. Dormant code may be deployed earlier without widening the
advertised skill.

Current `extract_brief` selects one JSON part, optionally projects a foreign
object into Brief 1.1, and discards other structured parts. Finance start must
instead explicitly select exactly one native finance invocation and bind every
referenced artifact. It must never project a generic/GALAHAD object into that
contract. More importantly, A2A `data` parts are parsed and reserialized JSON;
that cannot preserve an upstream exact-byte digest. This applies to finance
inputs and to the terminal finance result. The implementation must choose and
specify one interoperable carrier in both directions before coding:

1. an A2A raw/base64 part containing the exact bytes, plus media type,
   filename, and declared binding; or
2. a content-addressed URL part with separately fetched and verified immutable
   bytes.

The raw/base64 carrier is preferred for a self-contained exchange. Generic
Result 2.1 may retain its existing JSON `data` part, while the finance input
parts and finance `review.result` part use the specified exact-byte carrier.
Sending only `data` is insufficient when either side claims to bind source
bytes. A QUINTE/STAMMTISCH cross-repository golden test must prove byte identity
for invocation inputs and terminal output over the chosen carrier.

This split is a mandatory safety boundary: a legacy generic endpoint can map a
foreign object containing `question` or `title` but no `brief_version` into a
Brief 1.1. A finance invocation sent as an ordinary sole JSON part could
therefore start the wrong generic product. The finance endpoint accepts the
native invocation only through its raw carrier and rejects generic Briefs; an
updated generic endpoint explicitly rejects
`finance_review_invocation_version` before any foreign-object projection. The
separately pinned endpoint/Card prevents an unupdated legacy endpoint from ever
receiving finance input.

`src/a2a/http.rs` must reject a non-UTF-8 JSON-RPC envelope rather than use
replacement characters. Raw/base64 parts require canonical-base64 validation,
declared decoded byte length and SHA-256, encoded-body and decoded-per-part/
aggregate size limits, ordinary decoded-byte digest verification before JSON
parsing, and snapshot quota enforcement after decode.

A2A Task Record is an internal persisted contract that must advance from its
current versionless shape. Finance Task Record 2.0 binds run and input contract
family, sanitized decoded-part references/digests, endpoint/Card capability,
and stable terminal artifact ID/family metadata. It never stores or echoes the
raw/base64 finance input message in task history. Save failure fails the start;
deletion/corruption recovery reconstructs the same identity only from verified
Manifest 3.0 bindings, or fails closed—never an empty ambiguous message or a
fresh artifact ID.

CLI Host Receipt 1.0 remains byte- and meaning-stable. It cannot express the
finance gate: its result projection contains only `verified`, `actionable`,
`contract_version`, SHA-256, and path, and its manifest projection is generic.
Host Receipt 2.0 must name the result contract family/schema, Manifest 3.0,
exact and semantic result digests, profile/claim-manifest/primary/evidence
bindings, immutable `publication_posture`, a separately named finance gate
outcome, and the Finance Freshness Observation used for that current gate. Do
not reuse `actionable` with new semantics. Host Receipt 2.0 is a QUINTE CLI-host
contract; it is not STAMMTISCH's A2A invocation receipt. Receipt recovery
dispatches v1/v2 before validation: a recovered v1 start binds a Brief, while a
recovered v2 start binds a Finance Invocation.

Receipt selection follows the observed run family: generic run operations emit
Host Receipt 1.0 and finance run operations emit Host Receipt 2.0. With the
required endpoint/namespace split, preflight and start use the endpoint's
declared family—generic endpoint 1.0, finance endpoint 2.0—before a run exists.

The CLI mirrors this non-lossy split. Generic commands retain `quinte run
--brief FILE` and `quinte host start --brief FILE`. Finance uses dedicated
`--finance-invocation FILE` plus an explicit repeatable portable-reference to
local-file mapping; it never overloads `--brief` or infers mode from a lossy
projection. Finance is automatic-primary-only as specified above.

If finance emits `report.md`, it is a non-authoritative view regenerated only
from a fully verified Finance Result. It is visibly labeled as such, is never
a merge/publication/gate input, and may be discarded and regenerated. Any
consumer that elects to preserve it as an audited artifact must bind its exact
digest separately; tampering it can never change the authoritative result.

## 8. File impact map

This table is the expected implementation surface, not authorization to edit
it in this design-only change.

| Area | Files | Required change |
| --- | --- | --- |
| New schemas | `schemas/` and `schemas/legacy/` | Add every new family/revision in the matrix; freeze schema-less v1 packet shapes where required; add synthetic redistributable finance fixtures under `tests/fixtures/finance/`. |
| Contract registry and validation | `src/contract.rs`, `src/schema.rs` | Add family-specific specs and reader-revision dispatch, mode-indexed writer revisions, duplicate-key-safe parsing, canonical digest projections/domains, and cross-revision rejection. Decouple package, product-protocol, and A2A versions. |
| Data model and policy | `src/model.rs`, `src/policy.rs`, `src/brief.rs` or a new finance-invocation module, `src/lib.rs`, `src/error.rs` | Separate generic/finance types; enforce the exact five-school map, claim manifest/applicability predicates, primary allowlist, stable finance errors, and Policy 3.0 selection without globally normalizing Policy 2.0. |
| Store and lifecycle | `src/store.rs`, `src/run.rs` | Version-dispatched reads/writes, immutable binding graph, Protocol 2.0 packets/events/resume, strict finance gates, deterministic posture, and terminal immutability. |
| Adapter boundary | `src/adapters.rs` | Add school/finance-arbiter output modes with strict parsing and no repair or provenance remap. Preserve generic behavior. |
| CLI and local host | `src/cli.rs`, `src/ui.rs`, `src/host.rs`, `src/doctor.rs`, `scripts/contest_supervisor.py` | Add the dedicated finance invocation/locator surface, select Policy 3.0 capability, display distinct posture/freshness reasons, emit Host Receipt 2.0, reject manual-primary commands for finance, and keep the Result 2.1 `verified && actionable` path unchanged. |
| A2A host | `src/a2a/mod.rs`, `src/a2a/wire.rs`, `src/a2a/host_map.rs`, `src/a2a/card.rs`, `src/a2a/http.rs` | Negotiate finance capability, version/sanitize Task Record, select and bind all input parts, reject lossy envelopes, preserve exact bytes, dispatch the one terminal result family, enforce encoded/decoded limits, and keep A2A v1.0 lifecycle semantics. |
| Dependencies | `Cargo.toml`, `Cargo.lock` | Add only audited dependencies actually required for JCS, duplicate-key detection, or canonical base64; pin them reproducibly. |
| Product specifications | `README.md`, `skills/SKILL.md`, `specs/PROTOCOL.md`, new Protocol 2.0 spec or versioned protocol file, `specs/HOST.md`, `specs/HOST-CLI-LEGACY.md`, `specs/CLI.md`, `specs/RUNTIME-MIGRATION.md` | Specify dual protocol modes, exact schema files, CLI locator mapping, isolation, carrier, errors, inspection/freshness gates, operator behavior, and compatibility. Reconcile the documented Agent Card package version with code. |
| QUINTE tests | `tests/schema_contract.rs`, `policy_contract.rs`, `policy_compat.rs`, `store_contract.rs`, `run_e2e.rs`, `adapters_direct_json.rs`, `cli_validate.rs`, `cli_async.rs`, `cli_basics.rs`, `host_contract.rs`, `a2a_host.rs`, `tests/common/mod.rs`, `tests/fixtures/fake_agent.rs`, `tests/test_contest_supervisor.py` | Cover the migration and adversarial matrix below without weakening generic fixtures. |
| Audit/operator tools | `scripts/quinte-audit`, `scripts/quinte-insights`, `scripts/quinte-progress`, `scripts/quinte-run`, `scripts/test-quinte-audit.py` as applicable | Recognize Manifest 3.0 and finance posture without rewriting old artifacts; project explicit school IDs rather than infer them from legacy routes. |

Known existing documentation drift is not a finance design input: the
migration-era README/HOST text that described adaptive rounds and
vendor-neutral routing as future work predates the 2026-08-18 cutover, when
the adaptive round structure shipped and the single-vendor doctrine closed the
vendor question. This change preserves the implemented single-family
invariant. The HOST.md statement that Agent Card `version` follows the package
version also differs from `src/a2a/card.rs`, as noted above. These
discrepancies should be corrected explicitly, not silently attributed to the
finance contracts.

There is another pre-existing HOST/code discrepancy: HOST.md and the Agent
Card advertise `SendStreamingMessage`/streaming, but `src/a2a/mod.rs` currently
dispatches only `SendMessage`, `GetTask`, `ListTasks`, and `CancelTask`. Finance
work neither claims that streaming is implemented nor uses it as a prerequisite.
The capability must be implemented and tested or removed from the Card in a
separately identified conformance fix.

## 9. Cross-repository adaptation map

No external repository is modified in this design phase. These are required
follow-on gates before end-to-end finance enablement.

| Owner | Existing boundary | Required additive adaptation | Compatibility requirement |
| --- | --- | --- | --- |
| GALAHAD | Owns CalculationArtifact and deterministic finance calculations. Current ResearchKit v2 validation/replay does not admit the semiconductor lead/lag artifact as-is. | Publish/allowlist a domain-specific primary schema, or an additive CalculationArtifact revision plus strict lineage/replay support, carrying the memo's status, expiry, invalidation, schedule, and lineage semantics; publish digest vectors and synthetic fixtures. | QUINTE references GALAHAD's contract and never copies the kernel or recalculates values. Existing generic CalculationArtifact readers remain strict. |
| STAMMTISCH | A2A transports parsed JSON artifacts; `quinte_result_21` encodes Result 2.1; shipped GALAHAD integration is a paper session, not this primary-artifact path; generic examples/pipelines can automatically pass QUINTE's stored `review.result` into later stages; bundles replay gate records. | Add a finance doctrine/pipeline/fixture and immutable GALAHAD finance-artifact ingress (or new adapter), exact-byte storage, distinct `quinte_finance_review_result_10` gate, `stammtisch.gate-record.v1`, freshness observation, and dual-version offline replay/export. Require exactly one upstream artifact named `review.result`. | Preserve `quinte_result_21`, A2A v1.0, one `review.result`, `a2a.invocation.v1`, and existing generic pipeline behavior. Finance output fails the old gate and Result 2.1 fails the finance gate. The new finance path must stop for durable human confirmation before any downstream consumption. |

QUINTE-only implementation is out of scope for the full product flow.
Production finance invocation stays disabled until GALAHAD, QUINTE, and
STAMMTISCH have shipped their explicit readers and the cross-repository golden
suite passes. After QUINTE finishes, a durable explicit human confirmation is
required before the finance product flows onward; STAMMTISCH cannot silently
turn the diagram's next box into an automatic stage. Existing generic
STAMMTISCH example/pipeline auto-chaining remains untouched; it is not
evidence that the new finance boundary may auto-chain.

The known downstream file surfaces are:

- GALAHAD: `quantkit/quantkit/semiconductor.py`,
  `quantkit/tests/test_semiconductor.py`,
  `researchkit/researchkit/artifacts.py`, its schema and validation tests, and
  `docs/roadmap.md`. The inventoried semiconductor kernel and tests are active
  concurrent work rather than a released wire contract; follow-on changes must
  preserve and coordinate with them.
  changes their meaning or shape.
- STAMMTISCH: `src/adapters/a2a/`, `src/adapters/mod.rs`, `src/runner.rs`,
  `src/gates.rs`, `src/bundle.rs`, `src/schemas.rs`, a new gate-record schema,
  doctrine gate definitions, pipeline fixtures, conformance/product-pipeline
  tests, and the architecture/protocol documentation. Because the current
  adapter returns parsed `Value` artifacts and the runner canonicalizes them,
  preserving a raw terminal finance result requires an additive byte-preserving
  adapter/runner/store interface, not only a wire-part change.

The finance gate must emit `stammtisch.gate-record.v1`; v0 has a closed gate
kind enum. Bundle verification dispatches v0 and v1 by the record's `schema`
field and validates each against its own frozen schema. It must not broaden or
repoint the v0 schema. Gate Record v1 binds the Finance Freshness Observation
used at consumption, the result/Manifest exact digests, the finance gate kind
and outcome, reason codes, and the upstream artifact name/card/receipt
identity required for offline replay.

## 10. Migration and adversarial test matrix

### Contract and digest compatibility

- Assert every frozen schema byte digest in section 2.
- Require unique `$id`s and family-specific version keys; old/new schemas do
  not cross-accept, and unknown or mixed families/revisions fail closed.
- Read historical Results 1.0/2.0/2.1 and Manifests 1.0/2.0 without byte
  rewriting; finance mode never emits them.
- Prove generic creation still loads/copies Policy 2.0 and emits Manifest 2.0
  and Result 2.1, and its `actionable` behavior is unchanged.
- Exercise known-answer canonical JSON and domain-separated digest vectors in
  QUINTE and an independent verifier; reject duplicate JSON member names,
  non-UTF-8, uppercase/malformed digests, and unregistered domains.
- Prove key order/whitespace changes preserve semantic digest but alter exact
  digest, and that tampering either binding independently fails.
- Cross the CLI and A2A raw carrier with byte variants that parse to the same
  JSON and prove the exact source bytes arrive unchanged.
- Fault-inject every invocation/profile/claim/primary/evidence/transitive-
  snapshot validation, copy, hash, and fsync boundary before Manifest 3.0
  publication; no failed creation leaves a discoverable run tree or reusable
  partial binding.
- Assert the new shared envelopes use their specified version keys and exact
  filenames/`$id`s, including Manifest 3.0 `protocol_version: "2.0"`; prove
  per-mode writer selection never advances a generic artifact accidentally.

### Authority, information sets, and evidence discipline

- Require exactly five unique bindings with the fixed Party/school mapping;
  reject missing, duplicate, swapped, unknown, route-mismatched, and
  model-self-declared IDs.
- Prove Policy 3.0, profile, and claim manifest are fixed before run creation
  and cannot change through R1, R2, arbitration, resume, or reconcile. Reject
  claim IDs, classes, predicate inputs, or school applicability invented after
  R1 begins.
- Verify R1 can see only its authority and common inputs; attempt direct path,
  packet, and prompt-level access to every other lane output.
- Scan the entire serialized R2 Packet 2.0 for route, party, school, adapter,
  source, ordering, lane-authored prose, metadata, artifact-path, and digest
  side channels; prove the typed projection and deterministic alias map are
  stable for resume but unavailable to the model. Swap recipient wrappers and
  resume them under another school; authority/digest validation must fail.
- Reject any school output that cites an unindexed artifact, an inexact ref,
  a wrong schema/revision, a drifted exact or semantic digest, or a forbidden
  evidence class. Confirm finance parsing performs no repair/remap.
- Reject daily bars as intraday/microstructure evidence and discovery/news/
  daily summaries as numerical lineage. Reject inferred missing bars.
- Prove post-cutoff evidence cannot change a run, including after crash/resume;
  a later fact requires a new invocation and run ID.
- Test exchange holidays, daylight-saving transitions, half days, session-map
  collisions, missing sessions, and expiry boundaries against pinned schedule
  artifacts rather than weekday arithmetic.

### Merge, posture, and arbiter limits

- For each primary status (`accepted`, `descriptive_only`, `quarantined`, and
  `expired`), run cases with one, four, and five supportive lanes; every
  non-accepted or expired case remains `ABSTAIN`.
- Test every mandatory disposition. One `insufficient_evidence`,
  `contradicted`, `quarantined`, or `expired` school blocks the affected claim.
- Test each conditional predicate on both sides. `not_applicable` passes only
  when the preregistered predicate is false; an intraday timing, liquidity, or
  execution claim makes microstructure mandatory.
- Tamper an arbiter output to change a school disposition, applicability,
  primary status, materiality, or closure state; schema or semantic validation
  rejects it. Arbiter prose asserting the same changes has no effect.
- Prove one open material residual or active invalidation yields `ABSTAIN`,
  independent of arbiter prose, recommendation, confidence, or support count.
- Attempt duplicate/residual-ID collisions and contradictory closure evidence;
  reconciliation remains conservative and cannot discard a blocker.
- Exercise the R1/R2 conservative fold: R1 clear/R2 block remains blocked; R1
  block/R2 clear without admitted closure remains blocked; only valid,
  preregistered, bound closure evidence may close an earlier residual.
- Unit-test the posture truth table and reason ordering as a pure deterministic
  function, including identical output/digest across process restarts. Test the
  independent run-status/posture truth table and prove neither lane nor arbiter
  output selects runtime status.

### Persistence, recovery, and boundary behavior

- Tamper each binding independently at every stage: bytes, schema ID,
  revision, exact digest, semantic digest, profile, claim manifest, primary
  status, provenance, cutoff, expiry, school, route, model family, or
  invalidation. Include whitespace-only tamper of invocation, policy, and
  snapshot to prove Manifest 3.0 exact digests are persisted-byte digests,
  without changing historical compact typed-digest semantics.
- Crash immediately before and after every durable transition. Reconcile and
  resume must preserve the profile, claim manifest, primary, evidence index,
  fixed map, all ten school outputs, both arbiter bindings, posture inputs, and
  event-chain checkpoint.
- Exercise artifact-publication windows individually: accepted output before
  acceptance binding/event, event before manifest binding, evidence packet
  before counterpart output, counterpart output before R3 receipt, receipt
  before manifest binding, each primary-response staging state, result write,
  terminal event append, and terminal manifest replacement.
  A file's presence alone never completes a phase.
- Reject partial writes, event/manifest disagreement, stale attempt output,
  mixed Protocol 1.0/2.0 packets, mixed Event 1.0/2.0 ledgers, noncontiguous or
  cross-run events, unknown Event 2.0 kinds, edited/inserted/reordered events,
  previous-hash mismatch, suffix deletion, cross-run splice, bad genesis/head,
  a recovery-created nonidentical event, illegal status/binding combinations,
  and a runtime/writer digest change. Crash at journal write, event append,
  manifest replace, and journal clear; replay is exact-byte idempotent and any
  mismatch rejects rather than synthesizes a replacement. Only the exact
  journaled artifact/event/manifest tuple may commit.
- Prove finance runs cannot use `primary-arbiter amend`, cannot be converted to
  generic results, and cannot be rewritten after terminal state, including
  direct run/store APIs rather than only CLI commands.
- Require Retry State 2.0 and Rate State 2.0 for finance and validate their
  Manifest 3.0 route, school, run, policy, and binding digests. Update the
  host-side reader to dispatch v1 generic/v2 finance; reject v1 in finance and
  swapped diagnostic files that could steer a different school.
- Keep the terminal path `result.json`, with its family declared by Manifest
  3.0. Test cancellation and crash recovery with an orphan finance result
  written before the terminal manifest: it is never exposed through inspect or
  A2A and is removed or ignored by the authoritative finalization path.
- For every adapter `OutputKind`, allow only the documented provider envelope
  around a strict finance object; reject duplicate members/discriminators,
  legacy fallback, and an earlier valid candidate followed by a malformed or
  contract-invalid final candidate.
- Preserve existing A2A Result 2.1 conformance. Add finance start/collect tests
  with multiple bound input parts, raw exact input and output bytes, one
  terminal artifact named exactly `review.result`, card capability pinning,
  incorrect/missing parts, noncanonical base64, non-UTF-8 envelopes, encoded
  and decoded size limits, and family cross-gate rejection. Reject duplicate
  JSON-RPC envelope, nested part, discriminator, and binding member names before
  parsing into a generic JSON value.
- Prove generic endpoint projection rejects the native finance discriminator
  before `map_host_brief`, while the finance endpoint rejects Brief and JSON
  `data` ingress. Delete, corrupt, swap, and fail saves of Task Record 2.0;
  finance identity/artifact ID remains stable or recovery fails closed, and
  task history never echoes raw input bytes.
- Emit and validate Host Receipt 1.0 for generic runs and 2.0 for finance runs;
  neither parser accepts the other's revision or gate semantics. Test
  endpoint-family and pinned Policy 2.0/3.0 selection for preflight/start before
  a run ID exists; later endpoint-policy changes cannot alter historical
  timeout/status observations.
- In coexistence tests, list/status/inspect/reconcile/cancel handle supported
  Manifest 2.0 and 3.0 runs without skipping unknown entries, enforce the
  deliberate one-active policy or namespace isolation, and never normalize
  Policy 1.0/2.0 into finance or Policy 3.0 into generic mode. A legacy scan of
  a finance namespace must fail closed rather than report no active run.
- Put an unknown manifest revision beside supported runs and require Store/CLI
  list, A2A `ListTasks`, host preflight, and host start to fail the whole
  operation without a partial or falsely empty response. Byte, mtime, and tree
  inventories prove `list`, `status`, `inspect`, `GetTask`, `ListTasks`, event
  view, and offline verify leave pending/torn state untouched; only explicit
  reconcile/resume may mutate it.
- Resume and reconcile an active pre-upgrade Manifest 2.0 run through the dual
  binary's `GenericWritable` capability to Result 2.1, preserving Policy 2.0,
  Event 1.0, every v1 packet/state writer, and Host Receipt 1.0. If its bound
  runtime digest disallows the new binary, retain the exact pinned old writer
  rather than bypass drift validation.
- Prove the pre-write rollback inventory detects a complete Manifest 3.0,
  pending transition, partial run tree, or ledger evidence; any ambiguity
  selects post-write containment instead of restoring a legacy endpoint.
- Verify Event 1.0 UI/progress projections remain generic and Event 2.0
  projections display the bound school ID without inferring it from a route.
- Export a STAMMTISCH finance bundle and verify schemas, exact/semantic digest
  chains, complete transitive referenced bytes, receipts, gate reasoning, and
  posture from bundle bytes with no QUINTE or GALAHAD runtime installed. Reject
  dangling, duplicate, substituted, or declared-but-unsupplied objects. Tamper
  every artifact/receipt/gate-record link and require deterministic failure.
  Repeated verification of identical bundle bytes yields byte-identical
  reports; separately exported bundles may legitimately differ in creation
  metadata.
- Test result-time posture and consumer-time freshness independently at the
  session before, at, and after expiry. Tamper result/calendar/observation
  bindings; replay uses the recorded observation, while a new consumption uses
  a new trusted observation and cannot reuse a stale passing gate.
- Tamper or delete `report.md`: authoritative verification and posture do not
  change, and regeneration from the verified result is deterministic.
- Run the existing QUINTE, STAMMTISCH, and GALAHAD compatibility suites
  as release gates in addition to the new finance fixtures.

Only synthetic, redistributable fixtures may enter the public repositories.
No symbols, private data, credentials, endpoints, or machine-specific paths
belong in schemas or tests.

## 11. Release sequence and rollback boundary

The rollout is reader-first and writer-last:

1. Freeze historical schemas, fixtures, and digest vectors. Finalize all new
   schemas and canonicalization vectors before runtime changes.
2. Ship QUINTE dual readers/verifiers and finance fixtures with finance
   creation disabled. Historical writes remain unchanged.
3. Ship GALAHAD's allowed primary-artifact reader, offline/fixture-only writer,
   and independent digest vectors. Production emission remains disabled.
4. Ship STAMMTISCH's new finance gate, gate-record reader, raw-byte carrier,
   and offline bundle verifier while retaining the old gate.
6. Pass cross-repository schema, carrier, tamper, resume/reconcile, and offline
   golden suites.
7. Drain active generic Manifest 2.0 runs before dual-binary cutover, or retain
   each run's exact digest-pinned writer to finish/fail it; reader compatibility
   alone does not authorize resume across a `runtime_sha256` change.
8. Deploy dormant finance reader/writer code with creation disabled and without
   advertising an invocable finance skill. Pin the binary digest in operators.
9. Predeploy and pin the dedicated finance state root/namespace, Policy 3.0,
   endpoint, readers, resumer, and downstream gate configuration. Atomically
   publish the finance-capable Card and its pinned digest when all gates are
   ready. The single irreversible enablement seam is the feature-gated
   authorization to create the first Manifest 3.0 run.

A dedicated finance state root or namespace and a separately pinned
finance-capable endpoint/Card are mandatory deployment boundaries. They
prevent an old binary from accidentally discovering and mutating Manifest 3.0
runs. A release test points each legacy and finance binary at both namespaces
and proves that the routing guard prevents cross-access and false safe-launch
decisions.

Before the first Manifest 3.0 run is durably created, rollback may restore the
old binary, Card, endpoint, and generic writer together only after a
fail-closed namespace and ledger inventory proves that no Manifest 3.0 or
other durable or partial finance run state exists. Inability to prove absence
crosses the system into the post-write containment rules. After that point,
rollback cannot mean downgrade:

- disable new finance creation;
- keep a finance-capable read-only verifier and, for in-flight runs, the exact
  pinned binary needed to finish or fail them safely;
- never point an old binary at the finance namespace or let it resume,
  reconcile, inspect, or write a Manifest 3.0 run; generic inspection remains
  available in the generic namespace;
- never convert Manifest 3.0 to 2.0 or Finance Review Result 1.0 to Result 2.1;
- never amend or mutate a completed finance result; and
- route new generic starts to the legacy namespace only after confirming that
  its endpoint, Card digest, binary digest, Policy 2.0, Manifest 2.0, and Result
  2.1 writer set move together.

Readers and offline verifiers for every emitted finance revision must remain
available indefinitely. The release is complete only when historical generic
runs remain verifiable, both generic and finance STAMMTISCH bundles replay from
bytes, and the writer-selection boundary has a tested, fail-closed rollback
drill.

## 12. Implementation entry criteria

Implementation may begin only after maintainers accept this map and settle the
following artifacts in writing:

- the exact GALAHAD primary CalculationArtifact family/revision and its status,
  expiry, invalidation, and semantic-digest contract;
- the canonical JSON specification, unsigned projections, and published
  known-answer vectors for every semantic domain;
- the exact CLI portable-reference locator mapping and A2A raw-byte carrier,
  including canonical base64, decoded-byte bindings, and size limits;
- every exact schema file, `$id`, and discriminator in section 3: invocation,
  profile, claim manifest, evidence index, school output, finance arbiter,
  finance result, freshness observation, Policy 3.0, Snapshot 2.0, Manifest
  3.0, Pending Transition 2.0, Retry/Rate State 2.0, A2A Task Record 2.0, Host
  Receipt 2.0, and every revised packet/event/primary-arbiter envelope;
- the deterministic claim/applicability, R1/R2 folding, closure, materiality,
  posture, run-status, and consumer-freshness vocabularies/truth tables;
- the Event 2.0 genesis/projection, hash-chain, checkpoint, exact transition
  journal, lock order, terminal event ordering, and read-only/recovery split;
- the enforced finance isolation backend and its preflight conformance proof;
- the dedicated generic/finance state-root, pinned Policy, endpoint/Card,
  writer-capability, and CLI command surfaces; and
- the cross-repository release owners and atomic enablement procedure.

Until maintainers approve this map, no runtime changes begin. After approval,
Result 2.1, Manifest 2.0, Lane Output 1.0, Arbiter Verdict 1.0, and generic
host/A2A behavior remain unchanged, while finance creation stays disabled until
all release gates pass.
