# QUINTE Host Contract v2 — A2A v1.0 Endpoint

> **Implemented** as `quinte host serve`: an A2A v1.0 JSON-RPC front door
> over the CLI host commands ([HOST-CLI-LEGACY.md](HOST-CLI-LEGACY.md)),
> which run the redesigned core — five-school R1, contested-only R2,
> dual-arbiter R3, one model family
> ([PROTOCOL-REDESIGN.md](PROTOCOL-REDESIGN.md) as amended by
> [SINGLE-VENDOR-DOCTRINE.md](SINGLE-VENDOR-DOCTRINE.md)). This file is the
> wire authority STAMMTISCH already speaks.

## 1. Purpose

QUINTE exposes the full review run — the pipeline defined in
[PROTOCOL.md](PROTOCOL.md): `brief → R1 → R2 → R3 → deterministic merge →
result` — as **one A2A task**. The host boundary becomes a standard,
vendor-neutral JSON-RPC endpoint, so any conforming A2A client (the
STAMMTISCH wire adapter, or any agent or CI orchestrator) can invoke a
review without knowing QUINTE internals.

The round structure is the redesigned adaptive review
([PROTOCOL-REDESIGN.md](PROTOCOL-REDESIGN.md) as amended by
[SINGLE-VENDOR-DOCTRINE.md](SINGLE-VENDOR-DOCTRINE.md)): five-school R1,
contested-only R2, dual-arbiter R3, all seats on one model family
(DeepSeek). This contract therefore fixes only the outer wire — one review
request in, one result artifact out, with an optional interruption for
operator handoff. Whatever the internal structure becomes, this boundary
stays. The scheduler, lanes, phases, timeouts,
retries, arbitration, merge, and evidence ledger stay internal.

## 2. Discovery — Agent Card

Served at `GET /.well-known/agent-card.json` (canonical v1.0 path).

| Card field | Value |
| --- | --- |
| `name` | `quinte` |
| `description` | "Five-school multi-path review runtime: five first-pass lanes, pseudonymized recheck, two-arbiter verdicts, deterministic merge." |
| `version` | follows the QUINTE package version |
| `supportedInterfaces[0]` | `{ "url": <jsonrpc endpoint>, "protocolBinding": "JSONRPC", "protocolVersion": "1.0" }` |
| `capabilities` | `{ "streaming": false, "pushNotifications": false, "extendedAgentCard": false }` |
| `defaultInputModes` / `defaultOutputModes` | `["application/json"]` |
| `skills` | one skill: id `five-school-review`, tags `["review", "evidence", "verdict"]` |
| `securitySchemes` / `security` | `{"bearer": {"type": "http", "scheme": "bearer"}}` — present only when a token is configured |

Binding discipline: hosts **must** treat the card as an identity
document. A host pins the card's canonical digest before a campaign and
compares it on every contact; a drifted card is a hard stop, never a
guess. (STAMMTISCH records `card_sha256` in every invocation receipt —
see its `docs/protocol-layer.md`.) A breaking change to this contract
requires a new card revision plus a deprecation window; the card never
changes shape silently.

Every request carries the `A2A-Version: 1.0` header. Requests without a
supported version fail with a version error, never with a degraded parse.

## 3. JSON-RPC operations

| Operation | Purpose | QUINTE meaning |
| --- | --- | --- |
| `SendMessage` | start or continue a review task | create a run; or answer an arbiter challenge |
| `GetTask` | one-shot snapshot | current run status + terminal artifacts |
| `ListTasks` | paginated task listing | campaign overview and crash recovery (replaces `host reconcile`) |
| `CancelTask` | authorized cancellation | `quinte cancel`; idempotent |

`SendStreamingMessage` is **not offered**: the card advertises
`capabilities.streaming = false` and the method fails with `-32601`. Live
observers poll `GetTask`; phase-level progress is not on this wire.

`SendMessage` accepts `configuration.returnImmediately` (both modes):
`true` returns the created task at once for polling hosts (the STAMMTISCH
adapter uses this); `false` blocks until a terminal or interrupted state,
with the same terminal-state semantics as the A2A specification. Blocking
mode is bounded by a server-side ceiling of 3600 seconds: if the ceiling
expires before the run reaches a terminal or interrupted state, the call
returns the current (possibly non-terminal) task snapshot. A blocking
`SendMessage` never blocks other operations — `GetTask`, `ListTasks`,
`CancelTask`, and card discovery stay responsive while it waits.

### Error codes

| Code | Meaning |
| --- | --- |
| `-32001` | task not found (A2A reserved) |
| `-32002` | task not cancelable (A2A reserved) |
| `-32010` | **busy run** — one-active rule: a non-terminal task already exists |
| `-32011` | brief invalid — the message carries no closed-schema Brief |
| `-32012` | policy failure — binding, roster, or digest mismatch inside the run |
| `-32013` | challenge rejected — expired, replayed, or mismatched arbiter verdict |

All errors are JSON-RPC error objects with machine-readable `code`,
human-readable `message`, and an optional `data` object carrying the
QUINTE `state.code` where one exists.

## 4. Task lifecycle mapping

One QUINTE run = one A2A task. The mapping is fixed and emitted from
durable state, never inferred. The status names below reflect the
current runtime's phases; the protocol redesign may rename or replace
them — the wire contract promises only the A2A state semantics:
`WORKING` until terminal, `INPUT_REQUIRED` for operator handoff:

| QUINTE run status | `TASK_STATE_…` |
| --- | --- |
| `queued`, preflight | `SUBMITTED` |
| `r1_running`, `r1_gate`, `r2_*`, `r3_cc`, `merging` | `WORKING` |
| `waiting_primary_arbiter` (manual-handoff mode) | `INPUT_REQUIRED` |
| `completed`, `degraded` | `COMPLETED` |
| `failed`, `failed_policy` | `FAILED` |
| `cancelled`, `cancelling` | `CANCELED` |

- **One-active rule.** `SendMessage` that would create a second
  non-terminal task is refused with `-32010 busy_run`. This replaces the
  legacy launch-lock discipline at the wire level.
- **Degraded runs.** `degraded` completes the task; the distinction from
  `completed` lives in the result artifact (`result.status`), not in the
  task state. Hosts accept or refuse on the artifact via their own gates.
- **No streaming.** `SendStreamingMessage`/`statusUpdate` is not
  implemented; the card advertises `streaming: false` and hosts observe
  progress by polling `GetTask`. The run's ordered event ledger remains
  the authority for phase transitions (`lane.started`, `lane.finished`,
  gate outcomes, `retry_scheduled`, …) — it is durable state, read via
  the CLI surface, never a wire event stream.

## 5. Messages

### Task start

A `ROLE_USER` message with:

- exactly one `application/json` data part containing the Brief
  (closed-schema `quinte` Brief revision — see `schemas/brief.schema.json`);
- zero or more additional parts carrying permitted evidence attachments;
- `contextId` chosen by the host (campaign grouping; the STAMMTISCH
  adapter uses its run id);
- optional free-form `metadata`.

A message without exactly one valid Brief is refused with `-32011`.

### Arbiter challenge (INPUT_REQUIRED)

When a run reaches `waiting_primary_arbiter` under the manual-handoff
mode, the task pauses at `INPUT_REQUIRED`. `status.message` (a
`ROLE_AGENT` message) carries the challenge as an `application/json` data
part: run id, nonce, policy digest, evidence-packet digest, action scope,
issue and expiry times (the same binding as PROTOCOL.md's R3 challenge).

The host answers with a `ROLE_USER` message on the same `taskId` /
`contextId`, carrying the primary-arbiter verdict as an `application/json`
data part (`schemas/arbiter-verdict.schema.json` — the same payload
`quinte primary-arbiter submit` reads; the response binding with nonce and
digests is rebuilt server-side from the stored challenge, never taken from
the wire). The challenge is
consumed once; a mismatch, expiry, or replay is `-32013` and the task
stays `INPUT_REQUIRED` until a valid answer or a cancel. Direct file
placement or a claimed identity never advances state — the wire replaces
`quinte primary-arbiter submit`, nothing more.

Hosts that poll (rather than stream) must treat `INPUT_REQUIRED` as an
interruption, not a terminal state — the same discipline the A2A
specification defines.

## 6. Artifacts and binding

A `COMPLETED` task carries **three** artifacts. The binding artifact:

| Artifact field | Value |
| --- | --- |
| `artifactId` | UUIDv7 |
| `name` | `review.result` |
| `parts[0]` | `{ "data": <result.json>, "mediaType": "application/json" }` |

plus two deterministic HIGHBALL carriers — `highball.route-request.json`
and `highball.residual-trace.json` — code-derived projections of the
verdict for the downstream delivery stage (see §10). They are generated,
never model output, and each carries a stable `artifactId` (assigned on
first projection and persisted with the task record).

`<result.json>` is the QUINTE result revision in force
(`schemas/result.schema.json`): `run_id`, `status`, `brief_sha256`,
`route_bindings`, `summary`, `recommendation`, `dissent`, `residuals`,
`trial_manifest`. Its integrity fields are part of the artifact and are
re-verified offline by any host — the wire adds the task identity and
the card binding around them:

```text
Agent Card (pinned digest)
  → task id + context id
    → SendMessage/GetTask observations (host-side receipts)
      → review.result artifact (canonical digest)
        → result integrity fields (brief, policy, route bindings)
```

A task in any other state carries no artifacts. A host that observes
artifacts on a non-`COMPLETED` task must fail closed.

## 7. Security and deployment

- **Bindings.** Loopback-only unless a bearer token is configured
  (`securitySchemes` on the card); TLS for any non-loopback deployment.
- **Credentials** never appear in briefs, messages, artifacts, or error
  payloads. Provider credential indirection remains an internal runtime
  concern of the QUINTE process, invisible on this wire.
- **Fail closed.** Unknown fields inside a Brief/verdict are rejected by
  the closed schemas; malformed JSON-RPC is a protocol error, never a
  best-effort parse.
- **Isolation is unchanged.** The A2A endpoint inherits the runtime's
  process/config isolation; it adds no sandbox claim.

## 8. Migration map — legacy CLI surface → A2A

| Legacy CLI host operation | A2A equivalent |
| --- | --- |
| `host preflight --json` | GET Agent Card + `GetTask`/`ListTasks` snapshot |
| `host start --brief FILE --json` | `SendMessage` (returnImmediately) |
| `host status RUN_ID --json` | `GetTask` |
| `host inspect RUN_ID --json` (terminal handoff gate) | `GetTask` + artifact validation, host-side |
| `host reconcile --json` | `ListTasks` |
| `quinte cancel RUN_ID` | `CancelTask` |
| `primary-arbiter submit` | `SendMessage` reply to an `INPUT_REQUIRED` task |

The `contest_supervisor.py` outer helper and the CLI host commands remain
the legacy surface in service; their operational notes stay in
[HOST-CLI-LEGACY.md](HOST-CLI-LEGACY.md).

## 9. Conformance checklist (target)

A conforming implementation must pass, at minimum:

1. the Agent Card is served at the well-known URL and validates;
2. JSON-RPC 2.0 envelope discipline: id echo, `A2A-Version` header,
   no envelope on errors;
3. one `SendMessage` creates exactly one task; a second non-terminal
   request is refused with `-32010`;
4. task states follow §4 exactly, including `INPUT_REQUIRED` for the
   manual arbiter handoff;
5. the arbiter challenge is consumed once: expiry, replay, and mismatch
   are refused with `-32013`;
6. a `COMPLETED` task carries the `review.result` artifact validating
   against the result revision in force, plus the two deterministic
   HIGHBALL carrier artifacts (§6);
7. `CancelTask` is idempotent and refuses terminal tasks;
8. `ListTasks` paginates and exposes interrupted tasks after a crash;
9. an unknown `A2A-Version` fails with a version error;
10. legacy CLI host commands retain their behavior until their removal.

## 10. Status and rollout

This contract was fixed **spec-first**: the wire shape was settled before
implementation so the consumer side could build against it — the STAMMTISCH
A2A adapter (`eric-stone-plus/STAMMTISCH`, `docs/protocol-layer.md`) invokes
exactly this surface: card discovery at preflight, `SendMessage`
(returnImmediately) at invoke, `GetTask` polling, one `review.result`
artifact at collect.

`quinte host serve` runs the redesigned core (PROTOCOL-REDESIGN.md +
SINGLE-VENDOR-DOCTRINE.md): Agent Card GET, `SendMessage`
(returnImmediately) → `host start` (five-school R1, contested-only R2,
dual-arbiter R3), `GetTask` on `COMPLETED` → `review.result` (Result 2.1
with the same-model trial_manifest caveat) plus the two HIGHBALL
carriers (`highball.route-request.json`, `highball.residual-trace.json`)
as typed artifacts, `ListTasks` → `host reconcile` listing, `CancelTask`
→ `quinte cancel`, `-32010` busy_run, `-32011` invalid brief, durable
task records under `$QUINTE_HOME/a2a/`. The legacy CLI host commands
stay in service (HOST-CLI-LEGACY.md).

Open item: token-usage accounting. QUINTE does not yet aggregate
per-run token usage from the seat providers; hosts (STAMMTISCH's cost
ledger) read a documented `upstream.usage` convention that QUINTE does
not yet populate.
