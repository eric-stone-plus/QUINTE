# QUINTE Finance Protocol 2.0

Status: **contract implementation available; production creation disabled**.

Finance Protocol 2.0 is additive. Generic Protocol 1.0, Result 2.1, Manifest
2.0, and A2A transport 1.0 retain their existing readers and writers.

## Fixed schools

| Party | School ID |
| --- | --- |
| Party A | `factor_risk_model` |
| Party B | `event_driven` |
| Party C | `fundamental_supply_chain` |
| Party D | `trend_technical_regime` |
| Party E | `market_microstructure` |

Policy 3.0 binds this exact order, both restricted arbiters, one shared
family/provider/model tuple across all seven routes, a finance profile, and the
`process_information_flow_v1` backend. This backend is a userspace scheduling
and process information-flow contract, not a claim of kernel-enforced
filesystem isolation or containment of an untrusted executable. For each R1
attempt, the scheduler materializes exactly one closed Task Packet 2.0 and an
empty output sink, launches the provider without the run root, sibling attempt
trees, other packets, other outputs, or provider logs in its source set, and
accepts only the output sink as the candidate result. Deployment that cannot
enforce that userspace source-set boundary fails finance preflight.

The R1 packet binds the invocation, policy, profile, claim manifest, primary,
evidence index, recipient route, and recipient authority. Each accepted School
Lane Output binds the exact packet bytes and the packet semantic digest through
`input_packet_exact_sha256` and `input_packet_semantic_sha256`; matching only
the run, route, or school is insufficient.

R2 is built only after all five R1 objects validate. Its projection contains
claim IDs/text, typed dispositions, typed residual codes, and anonymous
`evidence:sha256:<64 lowercase hex>` content IDs. It contains no party, school,
route, adapter, path, contributor-artifact digest, authored prose, or source
label. Contributions are sorted by an unaliased semantic digest and receive
aliases afterward, so input permutation does not affect the packet.
Each R2 packet repeats the immutable-input and recipient-route bindings. Within
each contribution, decisions and anonymous evidence/residual identifiers are
sorted and deduplicated before hashing. The five accepted R1 bindings are first
projected to sorted pairs of exact and semantic digests and hashed in the
`quinte.finance-r1-source-set.v1` domain. R2 Packet 2.0 carries that
`r1_source_set_semantic_sha256`, and the corpus digest covers both this source
set identity and the anonymous corpus. Each R2 School Lane Output then binds
its exact R2 packet bytes and semantic digest in the same way as R1.

## Deterministic merge

Code conservatively folds R1 and R2 for every preregistered claim and school.
A blocker in either phase remains blocking unless a profile-registered closure
rule names nonempty, accepted closure evidence whose evidence class is allowed
for that school. A material residual can be closed only under the same rule and
evidence conditions. Arbiters cannot rewrite this fold, applicability, or
primary status.

`PUBLISH_BOUNDED` requires an accepted, unexpired primary, complete provenance,
all applicable schools clear, every inapplicable school preregistered, and no
open material residual or active invalidation. Otherwise code emits `ABSTAIN`
with typed reason codes. This is never a trade or execution authorization.

## Available commands

All mutating commands require the exact acknowledgement
`I_UNDERSTAND_FINANCE_CREATION_IS_DORMANT` through
`--enable-dormant-finance-writer`.

`quinte finance-init --source DIR --state DIR` validates and atomically copies
the immutable inputs, publishes Manifest 3.0 in `r1_running`, and creates the
five R1 Packet 2.0 artifacts. `quinte finance-advance --state DIR` (also
available as `finance-resume`) advances only after a complete strict R1 set,
creates anonymous R2 packets, and then accepts the complete R2 plus two
restricted arbiter artifacts before terminal publication.

`quinte finance-finalize --input DIR --output DIR` validates a synthetic or
externally scheduled ten-output bundle, verifies exact and semantic bindings,
performs the fold, and writes Finance Review Result 1.0, Run Manifest 3.0,
`highball.route-request.json`, and `highball.residual-trace.json`.

Finalization requires two closed Finance Arbiter Verdict 1.0 artifacts. They
bind the run, policy, invocation, profile, claim manifest, primary, evidence
index, all ten school-output digests, and all seven route digests. Their
vocabulary is limited to identifier, duplicate-residual, scope, and
already-admitted closure-evidence reconciliation; they cannot carry or change
a disposition, applicability result, primary status, or posture.

`quinte finance-verify --bundle DIR` revalidates those terminal artifacts
offline without mutating or repairing state. Terminal bundles include the
pinned inputs and every evidence artifact named by the evidence index. The
verifier rejects non-portable paths, reconstructs packets, fold, result,
manifest, and HIGHBALL carriers from fixed bundle slots, and requires the
reconstructed file tree to match byte for byte.

These commands implement the standalone dormant state/packet/replay lifecycle,
including a per-state writer lock, an exact-byte write-ahead journal, atomic
initial publication, deterministic replay, and offline verification.

Run Event 2.0 is a closed, typed ledger with only `run.created`,
`run.phase_advanced`, and `run.terminalized`. Every event binds the run ID, the
`quinte.finance-run-genesis.v1` digest, a contiguous safe-integer sequence, and
the previous complete event-line digest; only sequence zero has a null previous
digest. Each stored line is strict canonical UTF-8 JSON followed by exactly one
LF, and its SHA-256 covers that LF. Manifest 3.0 stores the authoritative
`{sequence, event_sha256}` checkpoint. The final event binds the terminal status
and, for `completed` or `degraded`, the Finance Review Result binding. Failed
and cancelled terminal events carry no result, and no event may follow a
terminal Manifest. Manifest 3.0 and the terminal event carry the same closed
`termination` facts: the last accepted phase (`r1_running`, `r2_running`, or
`merging`), a non-retryable typed code, and no free-form error text. Failed
runs admit only `output_invalid` or `integrity_failure`; cancelled runs admit
only `operator_cancelled`. The accepted artifact prefix is exact for that
phase, so deterministic replay produces two, three, or four ledger lines
respectively. Active and successful manifests carry `termination: null`.

Pending Transition 2.0 is the exact write-ahead record. It binds the run,
genesis, transition ID, operation (`create`, `advance`, or `terminalize`), the
old manifest digest and checkpoint, canonical-base64 event bytes plus their
length and digest, canonical-base64 target Manifest bytes plus their length and
digest, and every staged artifact binding. Creation alone has null old fields.
Under one transition lock, the writer durably publishes the journal, appends
and fsyncs those exact event bytes, atomically replaces the Manifest and fsyncs
its directory, then durably clears the journal. Reconcile or resume may only
complete that exact tuple; a partial line, hash/checkpoint disagreement,
noncanonical carrier, or any old/target mismatch fails closed and never causes
event regeneration, truncation, or a semantically similar replacement.
Read-only inspection reports pending or torn state without repairing it.

These commands do not invoke a provider and are not production enablement.
Production provider and
A2A finance invocation remain disabled. The Agent Card does not advertise a
finance skill; the generic A2A endpoint rejects native finance invocation.

## Frozen compatibility boundary

No existing schema file is changed. Writer selection is explicit
`GenericWritable` or `FinanceWritable`. Package version, product protocol, and
A2A transport are independent. A2A remains 1.0.
