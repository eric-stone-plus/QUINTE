# QUINTE Host Invocation Contract

This document defines the stable boundary for a non-interactive host such as
Codex, Hermes, a CI coordinator, or a local campaign runner. The host owns
launch serialization and observation. The existing `quinte` runtime remains
the sole owner of snapshots, lanes, phase transitions, timeouts, retries,
receipts, arbitration, and deterministic merge.

The machine receipt contract is
[`host-invocation.schema.json`](../schemas/host-invocation.schema.json). Its
revision is independent of the QUINTE package version.

## Commands

```text
quinte host preflight [--json]
quinte host start --brief FILE [--json]
quinte host status RUN_ID [--json]
quinte host inspect RUN_ID [--json]
quinte host reconcile [RUN_ID] [--json]
```

These commands are an additive host surface. Existing `run`, `status`,
`inspect`, `resume`, and `cancel` behavior is unchanged. There is no host
command for an individual lane, a phase skip, a retry, or a model/provider
override.

Hosts should use `--json`, keep stdout and stderr separate, parse the standard
CLI envelope, and then validate its `data` as a Host Invocation Receipt `1.0`.
An `ok: true` envelope only proves that an envelope was emitted; the host must
also inspect `data.state.code`, the projected run status, and the process exit
code.

`host preflight` is an advisory observation, not a launch reservation or an
authorization ticket. It does not reserve the one-active slot, and the state
root, credentials, provider reachability, and active-run set may change after
the receipt is written. Treat `ready` as “ready when observed”; `host start`
acquires its launch lock and reruns doctor and active-run checks immediately
before creating a run.

## Detached Start and One-Active Rule

`host start` is the only recommended launch command for an external
orchestrator. It performs the following operation under the state-root host
launch lock:

1. Load and validate the production policy and run the local doctor checks.
2. Strictly enumerate every entry under `<state-root>/runs`.
3. Refuse launch if any run is non-terminal.
4. Validate and snapshot the brief through the ordinary QUINTE creation path.
5. Start the ordinary detached per-run worker.
6. Atomically write a start receipt before and after worker launch so an
   ambiguous or failed launch remains recoverable.

The active set is every status except `completed`, `degraded`, `failed`,
`failed_policy`, and `cancelled`. In particular, `queued`, gate states,
`waiting_primary_arbiter`, `merging`, and `cancelling` remain active.

One-active is a host resource rule, not a change to the QUINTE protocol. The
legacy `quinte run` command remains compatible and does not acquire the host
lock. Two different launch surfaces therefore cannot be assumed to serialize
with each other. A deployment that requires the invariant must route all new
runs through `quinte host start` (or an outer lock using the same discipline).

Enumeration is fail-closed. A non-directory entry, a non-UTF-8 or non-UUIDv7
run directory, a missing manifest, an invalid manifest, or a directory/manifest
ID mismatch is not equivalent to “no active run”. Start must refuse and direct
the operator to reconcile or inspect the state root. A helper that silently
skips corrupt histories is suitable for a human status list, but not for a
launch safety decision.

## Durable Receipt and Ambiguous Launch Recovery

Receipts are private scheduler-adjacent host records:

```text
<state-root>/host/
  launch.lock
  latest.json
  receipts/<invocation-id>.json
```

`receipts/<invocation-id>.json` is the durable authority. It is written by
temporary sibling, fsync, and atomic replacement. `latest.json` is a
best-effort convenience projection: failure to update it cannot invalidate or
hide an authority receipt that was already persisted. It must not be treated
as the only launch record because preflight, status, or inspect can also
legitimately replace that projection after a start. Reconcile scans the durable
receipt set for the newest start record when no active run remains, so a caller
can still recover a run that reached a terminal state before the start response
was received.

Reconcile accepts only a regular `*.json` receipt whose filename, embedded
`receipt_path`, and embedded `state_root` all bind to the current state root
and invocation identity. QUINTE's hidden atomic-write `.tmp` siblings are
ignored as incomplete writes; other unexpected entries fail closed.

If worker launch fails, the same start receipt is updated with
`state.code=launch_failed`, the latest readable manifest projection, and
`state.blockers`. QUINTE attempts to mark the run terminal. If that state write
also fails, the receipt preserves both errors and keeps the created run in the
active set; the caller must reconcile and must not launch again.

A process can still die after QUINTE creates or starts a run but before the
caller receives the start receipt. The host must not blindly retry `start`:
the one-active check will deliberately block it. Instead call `host reconcile`.
Under the same launch lock, reconcile observes the durable run set and writes a
new receipt:

- no active run: `no_active_run`, with `launch_safe=true`;
- exactly one active run: `reconciled`, binding its run ID and manifest;
- more than one active run: `ambiguous_active_runs`, with
  `launch_safe=false`;
- an explicitly requested unknown/corrupt run: fail closed rather than invent
  a run identity.

Reconcile observes and binds state. It does not advance, resume, cancel, or
retry a run. Recovery of a dead scheduler remains the explicit
`quinte resume RUN_ID --json` operation after inspection and, where required,
user authorization.

## Receipt Fields

Every receipt has these common fields:

| Field | Meaning |
| --- | --- |
| `host_receipt_version` | Fixed wire revision `1.0`. |
| `invocation_id` | Canonical lowercase UUIDv7 identifying this host operation, not the run. |
| `receipt_path` | Exact durable path containing this same receipt. |
| `operation` | `preflight`, `start`, `status`, `inspect`, or `reconcile`. |
| `observed_at` | Time at which this projection was made. |
| `state_root` | Exact QUINTE state root used by the command. |
| `state.code` | Machine branch such as `ready`, `started`, `launch_failed`, `observed`, `terminal`, or a recovery outcome. |
| `state.active_run_ids` | Strictly observed non-terminal run IDs. |

Operation-specific objects are additive and non-null when present:

- `preflight`: the full doctor report;
- `brief`: supplied source path, supplied byte digest, and canonical copied
  Brief digest;
- `manifest`: a stable projection of status and the brief, policy, snapshot,
  runtime, error, worker, and result bindings that exist at observation time;
- `result`: result path, digest, contract revision, integrity verification, and
  whether the result revision is current/actionable;
- `recovery`: recovery outcome, whether a fresh launch is safe, and the durable
  receipt path.

`start` requires `run_id`, `brief`, and `manifest`. `status` and `inspect`
require `run_id` and `manifest`. A completed/degraded projection requires a
verified `result` binding. `reconcile` always requires `recovery`; a
`reconciled` receipt also binds `run_id` and `manifest`.

The source digest and canonical Brief digest may differ because QUINTE parses,
normalizes supported legacy Brief revisions, and serializes the copied input
before binding it. The canonical digest is the run provenance authority; the
source digest proves which caller-supplied bytes were presented.

## Progress, Timeout, and Retry Observability

`host status` is a one-shot read. A host should call it as a separate action at
30–60 second intervals; it must not use `run --wait`, a blocking `wait`, or a
sleeping shell loop inside an interactive agent turn.

The manifest is the authoritative current-state projection. The ordered,
fsynced `events.jsonl` remains the authoritative attempt history. A host status
implementation may include normalized `state.worker` and `state.attempts`
projections, but those projections never control scheduling:

- worker state derives from `diagnostics/worker.json`, the one-second
  heartbeat, the finished marker, and process identity;
- attempt state derives from `lane.started`, `lane.finished`, `lane.accepted`,
  `lane.retry_scheduled`, `lane.retry_wait`, and `lane.retry_started` events;
- timeout is reported only from scheduler-observed `timed_out=true`;
- retryability, failure class, and retry deadline come only from the typed
  event fields and persisted `retry-deadline.json`, never from model prose;
- a stale/dead worker is a recovery signal, not permission to create a second
  run or reset the attempt budget.

`host status` emits these normalized `state.worker` and `state.attempts` fields
for compact progress observation. They are projections only: the underlying
event/diagnostic evidence remains authoritative, and a host must not guess a
timeout or retry solely from elapsed wall time.

## Result and Provenance Acceptance

`host inspect` may expose a `result` only for `completed` or `degraded`. Before
emission it must verify all of the following:

1. `manifest.result_sha256` exists;
2. `result.json` exists and matches that digest;
3. the result validates against a registered result contract revision;
4. `result.run_id` and the manifest/run directory identity agree through the
   ordinary QUINTE integrity path.

`result.verified=true` means these product-integrity checks passed.
`result.actionable=true` only means the result uses the current result contract
revision. It does not authorize an external write, submission, purchase,
deployment, deletion, or other protected action.

The start/inspect receipt preserves the provenance chain:

```text
caller Brief bytes -> brief.source_sha256
normalized copied Brief -> brief.canonical_sha256 == manifest.brief_sha256
policy + evidence snapshot + executable -> manifest digests
accepted R1/R2 + R3 inputs -> QUINTE internal receipts
result.json -> manifest.result_sha256 == result.sha256
```

## Failure Rules and Pitfalls

- Do not merge stderr into JSON stdout. Errors are not guaranteed to be JSON
  envelopes.
- A nonzero command may still have emitted a valid JSON envelope (for example,
  preflight observing an active run). Parse stdout and retain the exit code;
  do not discard either signal.
- Provider egress defaults to inherited proxy behavior. A host may set
  `QUINTE_PROVIDER_PROXY_MODE=direct` only for an endpoint it has explicitly
  verified should bypass that proxy; never infer direct mode from a model name.
- Do not infer success from exit code `0` alone; read the receipt and status.
- Do not treat `degraded` as process success. Its result may be structurally
  valid evidence, while the ordinary QUINTE status mapping remains nonzero.
- Do not treat `waiting_primary_arbiter` as terminal for one-active purposes.
- Do not auto-resume a stale worker. Resume can terminate verified orphans and
  consumes existing attempt directories; it is a distinct recovery action.
- Do not edit receipts, manifests, events, retry deadlines, or result files to
  change scheduler state.
- `process` isolation is not an OS sandbox. The invoked adapter still has the
  operating-system authority of the QUINTE process.
- Protect the state root: it contains copied evidence and raw adapter output.
- The host contract does not make HIGHBALL, Hermes, Codex, or another host part
  of the QUINTE scheduler. It only standardizes invocation and observation.

## Host Conformance Tests

A conforming implementation should test at least:

1. schema-valid receipts for every operation;
2. start refusal while any non-terminal status exists;
3. concurrent starts serialize under the host lock and yield at most one run;
4. corrupt/unknown run directories fail closed;
5. a simulated crash after run creation can be recovered by reconcile without
   launching another run;
6. a start receipt is atomically written and binds both source and canonical
   Brief digests from one captured byte sequence;
7. status projections preserve scheduler timeout/retry event values;
8. completed result tampering makes inspect fail rather than emit
   `verified=true`;
9. stale/dead worker observation recommends explicit resume but does not
   advance the run;
10. existing non-host commands retain their prior behavior.
11. failure of the `latest.json` projection does not fail an operation after
    its authority receipt is durable;
12. worker launch failure updates the same invocation receipt, and failure of
    the terminal-state write is surfaced rather than discarded.

## Maintainer Backlog (Non-Contract)

The following items were observed while exercising the host boundary. They are
deliberately recorded here as follow-up work; none should be papered over by a
host wrapper or by weakening the fail-closed rules above:

- Under heavy concurrent test-process pressure, Unix process-group teardown can
  be observed in `/proc` a moment after the group leader exits. The scheduler
  must continue to avoid positive-PID fallback (which could signal a recycled
  process); the test harness should use a bounded poll before declaring a
  residual descendant.
- Worker PID plus heartbeat is an observation aid, not a cryptographic process
  identity. A future runtime should persist and verify a stronger start-time or
  platform-native identity before automated orphan recovery.
- Atomic receipt/manifest replacement should eventually fsync the parent
  directory as well as the file for stronger durability across sudden power
  loss.
- Repeated status/inspect calls currently observe persisted retry-deadline
  windows but do not advance scheduler recovery. Any future read-triggered
  recovery must remain explicit, serialized, and receipt-backed.
- Provider `NO_PROXY` normalization and relative state-root binding need
  platform-specific review (especially IPv6 authorities and lexical versus
  canonical path identity) before being widened.
- The host scan should eventually reject a symlinked `runs` root itself rather
  than relying only on child-entry checks; a state-root replacement race must
  never make an external tree part of reconciliation.
- Retry-deadline projection currently ignores non-directory entries beneath a
  phase. Future fail-closed validation should distinguish an absent route from
  malformed scheduler state, and event-derived `route_id` values should be
  checked against the manifest binding before being exposed in a receipt.
- Result integrity binds the result bytes to the manifest digest, but the
  manifest and result are still two independently read files. A future amend
  transaction should expose a single generation or lock so a cross-file
  snapshot cannot straddle versions.
- The external HIGHBALL/builder path still needs execution receipts and fuller
  observed OCI fields; those are separate provenance contracts, not QUINTE
  scheduler responsibilities.
