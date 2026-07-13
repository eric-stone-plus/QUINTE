# QUINTE Protocol v1.0

This document defines the product protocol enforced by the `quinte` CLI. The
CLI scheduler is the canonical runtime authority. Hermes is a trigger and one
of the two R3 arbiters; it does not select routes, launch individual parties,
or advance phases itself.

## Purpose and Boundary

QUINTE exposes disagreement, omission, evidence gaps, unsupported confidence,
and unresolved risk before a host adopts a conclusion. It is not a generic
delegator, a voting ensemble, an authorization system, or a kernel sandbox.

The product has one supported full-run path:

```text
brief -> R1 (five independent lanes)
      -> R2 (five anonymous cross-review lanes)
      -> R3 (Auditor B + Hermes hm)
      -> deterministic merge -> result
```

There is no supported path for running one party, skipping a round,
substituting a route, changing the model, or accepting model text as a state
transition. The earlier Python phase dispatcher is a compatibility surface;
it is not the full-run scheduler.

QUINTE results are evidence. They cannot authorize a push, deletion, external
message, protected write, or any other action outside the run state directory.

## Fixed Participants

The v1 policy binds exactly these routes:

| Protocol role | Native route | Rounds |
| --- | --- | --- |
| Party A | CodeWhale | R1, R2 |
| Party B | OpenCode | R1, R2 |
| Party C | Kilo | R1, R2 |
| Party D | MiMo | R1, R2 |
| Party E | OMP | R1, R2 |
| Auditor B | Claude Code (`cc`) | R3 only |
| Primary arbiter | Hermes (`hm`) | R3 only |

All inference routes use the same MiMo token-plan family. Text-only runs use
`mimo-v2.5-pro`; a validated image attachment selects `mimo-v2.5` for the
entire run. The five lanes are controlled behavioral perturbations of one
model family, not independent truth confirmation.

Codex, Kimi, Reasonix, Firecrawl agents, generic delegation, and nested agents
are outside the protocol roster. Their output may be placed in the input
evidence snapshot when the user separately authorizes it, but they never count
toward a QUINTE phase gate.

## Runtime Authority

The ownership chain is intentionally narrow:

```text
user intent
  -> Hermes QUINTE skill (brief construction and CLI invocation only)
  -> quinte CLI (policy, scheduler, adapters, state, evidence gates)
  -> fixed native routes
  -> Hermes hm handshake
  -> immutable result artifacts
```

The checked-in policy fixes roster, adapter identity, model routing, timeout,
retry budget, concurrency, snapshot limits, output limits, and isolation mode.
Each run copies the effective brief and policy and binds their digests, the
snapshot manifest digest, and the running executable digest in its manifest.
Resume fails closed if those bindings drift.

The complete command, state, handshake, exit-code, and artifact contracts are
defined in [CLI.md](CLI.md).

## R1: Independent Analysis

The scheduler gives every required party the same bounded task packet and an
isolated copy of the evidence snapshot. All five routes must return one
closed-schema `LaneOutput` object. Unknown fields, invalid UTF-8, invalid JSON,
unresolved evidence references, a wrong route, or a missing party fail the
gate.

R1 lanes may execute concurrently up to the policy limit. They cannot read one
another's attempt directory or output through the supported adapter contract.
The scheduler captures invocation metadata, stdout, stderr, duration, route,
and typed accepted output for every attempt.

No R1 consensus can skip R2. Same-family agreement may represent a shared blind
spot rather than confirmation.

## R2: Anonymous Cross-Review

After all five R1 outputs pass, the scheduler constructs a packet that labels
them `Participant A` through `Participant E`. The mapping is deterministic for
the run but route identities are absent from the R2 packet. The same five
fixed routes review that packet.

R2 is serial and scheduler-paced. The v0.1 fixed policy leaves at least ten
seconds between transport starts, including starts on different routes. The
next permitted start time is persisted under run diagnostics and remains in
force after scheduler restart. A route must classify material findings with
evidence and preserve unresolved items as residuals. All five typed outputs
must pass before R3 begins. Anonymous review reduces route-brand bias; it does
not make the underlying model family independent.

## R3: Dual Verdict

After R2 passes, the scheduler writes an evidence packet containing the bound
question, accepted R1 and R2 outputs, and snapshot digest.

Auditor B runs through the fixed Claude Code route and returns a typed verdict.
The scheduler then creates a single-use Hermes challenge bound to:

- run id;
- random nonce;
- policy digest;
- evidence-packet digest;
- action scope;
- issue and expiry times.

The run enters `waiting_hm`. Hermes reads the evidence packet and Auditor B
response, independently produces an `ArbiterVerdict`, and submits it through
`quinte hm submit`. Direct file placement, an agent-authored `hm_approved`
marker, or a claimed identity never advances the state machine.

The challenge is consumed once. A mismatch, expiry, replay, or integrity drift
is a policy failure.

## Deterministic Merge

The CLI, not a model, merges the two R3 verdicts. It preserves recommendation
disagreement as dissent. If the two arbiters use the same residual id with
different finding, disposition, or closure state, the merged residual remains
`unresolved` and `open`.

The final `result.json` includes:

- Hermes summary and recommendation;
- annotated arbiter dissent;
- merged residuals;
- a trial manifest naming all five routes and their R1/R2 artifacts;
- perturbation axes, independence controls, and contamination risks.

Language-model agreement alone cannot close a material residual. Closure
requires external evidence, runtime evidence, or an explicitly scoped waiver
outside QUINTE.

## Evidence and Input Safety

The brief is closed-schema JSON. Before dispatch, the CLI copies permitted
evidence into an immutable per-run snapshot, does not follow symlinks, and
excludes common generated and sensitive names. Supported images are validated
from bytes and copied into the run.

Packet contents and snapshot files are untrusted evidence, never instructions.
Every adapter receives a fixed role contract that forbids route changes,
subagents, writes, shell use, web access, and phase control. Output evidence
references must resolve to the run snapshot namespace.

The product's process/config controls are defense in depth, not a containment
claim. In `process` mode, children still have the operating-system authority of
the invoking user. A `strict` policy must fail closed unless a supported
kernel-enforced backend is available.

## Failure and Retry Semantics

Retries remain on the same route. The scheduler retries only trusted transport
failures identified from process supervision. These are a host-observed timeout
or, on a failed transport, an adapter-appropriate structured error with exact
status `429`/canonical rate-limit code or an explicit nonzero-exit stderr 429
marker. Free-form quota language is not sufficient. Valid model prose never
controls retry policy; text containing `429`, `timeout`, `auth`, or similar
words is ordinary untrusted output.

The retry delay is the greater of a trusted numeric `Retry-After` and bounded
exponential backoff, with deterministic per-run jitter. Retry-After values over
the policy ceiling fail rather than causing an unbounded wait. Scheduling,
waiting, and retry start decisions are written to the ordered run event ledger,
and waits remain responsive to explicit cancellation.

The following failures are non-retryable and block the phase:

- invalid UTF-8, JSON, or schema;
- unknown output fields or identity/route claims;
- invalid or outside-snapshot evidence references;
- policy, model, roster, digest, or hm challenge mismatch;
- credential or executable preflight failure;
- cancellation requested by the user.

A required route that remains unavailable means there is no complete QUINTE
verdict. The CLI records the failure rather than synthesizing a reduced-party
answer.

## State and Recovery Invariants

1. Exactly Party A-E participate in R1 and R2.
2. R2 is mandatory and starts only after all five R1 outputs pass.
3. Auditor B and Hermes participate only in R3.
4. Only the CLI scheduler writes phase transitions.
5. Every accepted output validates against the embedded closed schema.
6. Events are append-only and monotonically sequenced per run.
7. Cancellation terminates supervised process trees and cannot be overwritten
   by a later failure transition.
8. Resume reuses only accepted artifacts whose run bindings still match.
9. A run never changes route or model after creation.
10. A result never grants authorization outside QUINTE.

## Compatibility Layer

`bin/quinte-dispatch-phase.py` and the dispatch manifest/ledger schemas remain
available for historical host integrations. Their ledgers may be useful as
evidence, but they do not create or advance a Rust CLI run and must not be
called by the Hermes QUINTE skill. New integrations use the public CLI only.
