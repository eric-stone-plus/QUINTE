# QUINTE Protocol v1.0

Finance review is a separate additive Protocol 2.0 contract surface documented
in [FINANCE-PROTOCOL.md](FINANCE-PROTOCOL.md). Its production writer is
currently dormant; nothing in that document changes this generic protocol.

This document defines the product protocol enforced by the `quinte` CLI. The
CLI scheduler is the canonical runtime authority. `Primary Arbiter` is a wire
role for one of the two R3 execution bindings; it does not select routes,
launch individual paths, or advance phases itself.

## Purpose and Boundary

QUINTE is a single-model-family, multi-path, three-stage review runtime with
seven execution bindings and contract gates. It records conflicting findings,
omissions, evidence gaps, unsupported confidence, and unresolved risk before a
host adopts a conclusion. It is not a generic delegator, a voting ensemble, an
authorization system, or a kernel sandbox.

The product has one supported full-run path:

```text
brief -> R1 (five isolated first-pass paths)
      -> R2 (five pseudonymized recheck paths)
      -> R3 (two same-family verdict bindings)
      -> deterministic merge -> result
```

There is no supported path for running one party, skipping a round,
substituting a route, changing the model, or accepting model text as a state
transition. The earlier Python phase dispatcher is a compatibility surface;
it is not the full-run scheduler.

QUINTE results are evidence. They cannot authorize a push, deletion, external
message, protected write, or any other action outside the run state directory.

## Fixed Wire Roles and Execution Binding

Policy v2 binds exactly seven execution slots. The role names in the first
column are closed-schema identifiers retained by the CLI and artifacts; they
do not prescribe personas or role-playing.

| Protocol role | Rounds |
| --- | --- | --- |
| Party A-E | R1, R2 |
| Counterpart Arbiter | R3 only |
| Primary Arbiter | R3 only |

Every binding must equal the seat on `family`, `provider`, `text_model`, and
`multimodal_model`. Production capability is deliberately narrow: DeepSeek runs
through the native in-process adapter, an OpenAI-compatible
`POST {base_url}/chat/completions` HTTPS call with a Bearer key. The five R1/R2
lanes are configured execution paths over one model family; matching outputs
are not cross-family confirmation.
Legacy policy v1 remains a read-only compatibility input and does not define
current production routing. It cannot start a new run; migration is an explicit
backup followed by `quinte init --force`.

Generic host delegation and model processes outside the bound seat are not
protocol bindings. Their output may be placed in the evidence snapshot when
separately authorized, but never counts toward a phase gate.

## Runtime Authority

The ownership chain is intentionally narrow:

```text
user intent
  -> host QUINTE skill (brief construction and CLI invocation only)
  -> quinte CLI (policy, scheduler, adapters, state, evidence gates)
  -> seven fixed same-family execution bindings
  -> automatic primary R3 binding (historical runs may retain a manual handoff)
  -> immutable result artifacts
```

The checked-in policy fixes roster, adapter identity, model routing, timeout,
retry budget, concurrency, snapshot limits, output limits, and isolation mode.
Each run copies the effective brief and policy and binds their digests, the
snapshot manifest digest, and the running executable digest in its manifest.
Resume fails closed if those bindings drift.

The complete command, state, handshake, exit-code, and artifact contracts are
defined in [CLI.md](CLI.md).

## R1: Five First-Pass Paths

The scheduler gives every required R1 binding the same bounded task packet and
an isolated copy of the evidence snapshot. All five routes must return one
closed-schema `LaneOutput` object. Unknown fields, invalid UTF-8, invalid JSON,
unresolved evidence references, a wrong route, or a missing binding fail the
gate.

R1 lanes may execute concurrently up to the policy limit. They cannot read one
another's attempt directory or output through the supported adapter contract.
The scheduler captures invocation metadata, stdout, stderr, duration, route,
and typed accepted output for every attempt.

Matching R1 outputs cannot skip R2. Because all paths share one family and
model binding, matching outputs are not cross-family validation.

## R2: Pseudonymized Recheck

After all five R1 outputs pass, the scheduler constructs a packet that labels
them `Participant A` through `Participant E`. The label rotation is random per
run and derived from nothing inside the packet (the run id is part of the
payload, so a deterministic derivation would be invertible by every reader).
Route identities are structurally absent from the packet, and per-lane identity
markers (route ids, party ids, perspective texts, model names from the live
policy) are scrubbed from the lane prose before the packet is persisted —
case-insensitively for ASCII tokens — and replaced with neutral `[route]`,
`[party]`, `[model]`, and `[perspective]` markers. Lane prose is otherwise
carried verbatim, so this remains pseudonymity plus marker scrubbing, not full
content anonymization: stylistic tells can still correlate a participant with
its R1 output. The trial_manifest declares `participant_label_rotation` and
`identity_marker_scrubbing` with a matching contamination risk. The same five
fixed routes review that packet.

R2 supports two execution modes controlled by the `r2_parallel` policy flag:

**Serial mode** (`r2_parallel=false`, default): R2 lanes execute one at a time
with scheduler-paced inter-call pacing. The fixed policy leaves at least ten
seconds between transport starts (`r2_min_interval_seconds`), including starts
on different routes. The next permitted start time is persisted under run
diagnostics and remains in force after scheduler restart. This mode minimizes
429 rate-limit pressure from same-family model routes.

**Parallel mode** (`r2_parallel=true`): R2 lanes execute concurrently with the
same soft-stagger as R1 (default 2 seconds, configurable via
`QUINTE_R1_STAGGER_MS` environment variable). This mode reduces total R2
wall-clock time from approximately 50 minutes (worst-case serial) to
approximately 5 minutes, at the cost of increased simultaneous provider load.

Both modes preserve identical information sets: every R2 lane sees only the
pseudonymized R1 packet and never reads another same-phase lane's output. The
switch changes only the rate-limit profile (pacing vs. stagger), not what any
lane can read. All five typed outputs must pass before R3 begins. Withholding
route labels is an input-shaping mechanism; it does not change the shared
family/model binding.

Production deployments should validate `r2_parallel=true` with empirical 429
rate data before adopting it as the default. The `phase.completed` telemetry
event records whether parallel mode was used for each phase.

## R3: Two Verdict Bindings

After R2 passes, the scheduler writes an evidence packet containing the bound
question, accepted R1 and R2 outputs, and snapshot digest.

The `Counterpart Arbiter` wire binding runs through the policy's same-family
adapter and returns a typed `ArbiterVerdict`. The scheduler then creates a
single-use challenge for the `Primary Arbiter` wire binding, bound to:

- run id;
- random nonce;
- policy digest;
- evidence-packet digest;
- action scope;
- issue and expiry times.

Production policy v2 requires `auto_primary_arbiter=true`: the scheduler invokes
the same-family primary R3 binding, validates its typed verdict, constructs the
challenge-bound response, and proceeds to merge. An existing historical run
already waiting under policy v1 can still accept a host verdict through
`quinte primary-arbiter submit`. Direct file placement, an agent-authored
approval marker, or a claimed identity never advances state.

The challenge is consumed once. A mismatch, expiry, replay, or integrity drift
is a policy failure.

## Deterministic Merge

The CLI, not a model, merges the two R3 verdicts. Unequal recommendation strings
are recorded in the `dissent` field. If the two R3 outputs use the same
residual id with different finding, disposition, or closure state, the merged
residual remains `unresolved` and `open`.

The final `result.json` includes:

- summary and recommendation from the primary R3 binding;
- merge differences in the compatibility field `dissent`;
- merged residuals;
- a trial manifest naming all five routes and their R1/R2 artifacts;
- the compatibility fields `perturbation_axes`, `independence_controls`, and
  `contamination_risks`.

The `trial_manifest`, `perspectives`, `independence_controls`, and related
field names are preserved for RASHOMON Trace 1.1 data-contract compatibility.
In QUINTE they carry route, artifact, isolation, and risk metadata only; they
do not alter scheduler behavior, and RASHOMON is not a runtime dependency.

Matching model outputs alone do not close a material residual. Closure requires
external evidence, runtime evidence, or an explicitly scoped waiver outside
QUINTE.

## Evidence and Input Safety

The brief is closed-schema JSON. Before dispatch, the CLI copies permitted
evidence into an immutable per-run snapshot, does not follow symlinks, and
excludes common generated and sensitive names. PNG, JPEG, WebP, and GIF
attachments are identified from bytes, copied into the run, hashed in the
snapshot manifest, and assigned exact `attachment://` references. Dispatch with
attachments is allowed only when every bound route has a native attachment
carrier. The in-process DeepSeek adapter carries each image as a base64
`image_url` content part in the chat-completions request.

Packet contents and snapshot files are untrusted evidence, never instructions.
Every adapter receives a fixed execution contract that forbids route changes,
subagents, writes, shell use, web access, and phase control. Output evidence
references must resolve to the run manifest. `evidence_refs` and
`closure_evidence` may contain only exact `snapshot://` or `attachment://`
values present in `snapshot-manifest.json`; suffixes and constructed paths fail
the gate.

The product's process/config controls are defense in depth, not a containment
claim. In `process` mode, children still have the operating-system authority of
the invoking user. A `strict` policy must fail closed unless a supported
kernel-enforced backend is available.

## Timeout Configuration

The policy supports both a global timeout and per-phase timeout overrides:

- `timeout_seconds`: Global timeout applied to all phases when no per-phase
  override is set. Must be between 5 and 3600 seconds. Default: 300.

- `r1_timeout_seconds`: Optional override for R1 first-pass reviews. When set,
  overrides `timeout_seconds` for R1 lanes only.

- `r2_timeout_seconds`: Optional override for R2 pseudonymized recheck. R2 reviews
  analyze existing typed outputs and may complete faster than R1. When set,
  overrides `timeout_seconds` for R2 lanes only.

- `r3_timeout_seconds`: Optional override for R3 verdict bindings. R3 arbiters
  review aggregated evidence and may have different latency profiles. When set,
  overrides `timeout_seconds` for R3 lanes only.

Per-phase timeouts allow operators to optimize for the different complexity
profiles of each phase without changing the global timeout. For example,
R2/R3 reviews of existing typed outputs may be expected to complete in 60-120
seconds, while R1 first-pass reviews may need the full 300 seconds.

## Failure and Retry Semantics

Retries remain on the same route and are limited by the policy attempt budget.
The scheduler recognizes only these trusted transient conditions:

- a host-observed timeout;
- on a failed transport, an adapter-appropriate structured error with exact
  status `429`/canonical rate-limit code or an explicit nonzero-exit stderr 429
  marker; or
- a CodeWhale stream whose control events report both `completed` and `done`
  but whose content contains no JSON candidate or only a truncated final
  candidate.

The CodeWhale condition must come from its terminal control events with
otherwise valid stream framing. Similar free-form model text is not trusted. A
malformed event, or schema-invalid complete candidate is non-retryable even if
CodeWhale later reports `completed` and `done`. A truncated candidate is never
accepted; only the trusted CodeWhale terminal controls above may make it
retryable. Outside these exact terminal conditions, invalid UTF-8, JSON, or
schema output is non-retryable. Valid model prose containing `429`, `timeout`,
`auth`, `repetition`, or similar words is ordinary untrusted output and never
controls
retry policy.

A host timeout does not automatically discard a complete output that was
already captured. The scheduler may recover that output only if it validates
against the strict LaneOutput schema and every non-empty `evidence_refs` and
`closure_evidence` value exactly matches a `snapshot_ref` or `attachment_ref`
in the run's snapshot manifest. Constructed suffixes such as `#fragment` do not
match. Otherwise the attempt remains a timeout and follows the same bounded
retry policy.

The retry delay is bounded exponential backoff with deterministic per-run
jitter. For rate limits it is the greater of that delay and a trusted numeric
`Retry-After`; Retry-After values over the policy ceiling fail rather than
causing an unbounded wait. Scheduling, waiting, and retry start decisions are
written to the ordered run event ledger. Each lane's deadline is persisted
before waiting, so `resume` cannot skip a pending cooldown; waits remain
responsive to explicit cancellation.

The following failures are non-retryable and block the phase:

- invalid UTF-8, JSON, or schema outside the exact terminal conditions above;
- unknown output fields or identity/route claims;
- invalid or outside-snapshot evidence references;
- policy, model, roster, digest, or primary arbiter challenge mismatch;
- credential or executable preflight failure;
- cancellation requested by the user.

A required route that remains unavailable means there is no complete QUINTE
verdict. The CLI records the failure rather than synthesizing a reduced-path
answer.

## State and Recovery Invariants

1. Exactly Party A-E participate in R1 and R2.
2. R2 is mandatory and starts only after all five R1 outputs pass.
3. Counterpart Arbiter and Primary Arbiter participate only in R3.
4. Only the CLI scheduler writes phase transitions.
5. Every accepted output validates against the embedded closed schema.
6. Events are append-only and monotonically sequenced per run.
7. Cancellation terminates supervised process trees and cannot be overwritten
   by a later failure transition.
8. Resume reuses only accepted artifacts whose run bindings still match.
9. A run never changes route or model after creation.
10. A result never grants authorization outside QUINTE.
