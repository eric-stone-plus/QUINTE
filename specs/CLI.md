# QUINTE CLI Contract

This document defines the public command boundary for the `quinte` Rust
CLI. The CLI is the execution authority for a QUINTE run. A host may create a
brief, invoke commands, and consume the result; it must not reproduce the
scheduler with ad hoc model calls. The product boundary is a single-model-family,
multi-path, three-stage review runtime with seven execution bindings and
contract gates.

The protocol itself remains defined by [PROTOCOL.md](PROTOCOL.md).

## State Root

The default state root is `~/.quinte`. `QUINTE_HOME` overrides it. A hidden
global `--home DIR` option is also available for tests and controlled host
integration.

Precedence is:

1. global `--home DIR`
2. `QUINTE_HOME`
3. `$HOME/.quinte` (or `%USERPROFILE%\.quinte` on Windows)

Commands that create or inspect policy-bound runs require
`<state-root>/policy.json` to exist. The `primary-arbiter` commands operate on the immutable
policy copy and challenge already stored in an existing run. `quinte init
--force` replaces the global policy; normal runs never rewrite it.

## Installation Boundary

QUINTE is built from source. The release build excludes all test-adapter code.

QUINTE is not a hosted proxy. Policy v2 supports only the proven production
bindings MiMoCode/MiMo, Reasonix/DeepSeek, and Codex/OpenAI. `doctor` checks all
seven same-family execution bindings and the selected provider environment
pair.

## Command Surface

```text
quinte init [--force] [--json]
quinte status [RUN_ID] [--json]
quinte doctor [--json]
quinte run --brief FILE [--wait] [--json]
quinte wait RUN_ID [--json]
quinte resume RUN_ID [--json]
quinte cancel RUN_ID [--json]
quinte inspect RUN_ID [--json]
quinte host preflight [--json]
quinte host start --brief FILE [--json]
quinte host status RUN_ID [--json]
quinte host inspect RUN_ID [--json]
quinte host reconcile [RUN_ID] [--json]
quinte primary-arbiter request RUN_ID [--json]
quinte primary-arbiter submit RUN_ID (--verdict FILE | --response FILE) [--json]
quinte agents list [--json]
quinte agents describe ID [--json]
quinte policy show [--json]
quinte policy validate [--json]
```

No public command runs an individual R1/R2 binding. There is no phase-skip,
substitution, arbitrary model, arbitrary adapter, or model-selected transition
command. `Party` and `Arbiter` names on this command surface are fixed wire-role
identifiers, not personas or scheduler authorities.

The `host` group is the supported external-orchestrator boundary. It adds a
global launch lock, fail-closed one-active guard, durable receipts and
reconciliation without taking ownership of QUINTE phases. See [HOST.md](HOST.md).

### `init`

Creates the state root, `policy.json`, and `runs/`. It refuses to replace an
existing policy unless `--force` is supplied.

The generated policy has five R1/R2 wire roles plus the `Counterpart Arbiter`
and `Primary Arbiter` R3 wire roles. All seven share one seat binding.
`auto_primary_arbiter=true` is the default. Policy v1 is accepted read-only and
normalized in memory without rewriting its historical file.

### `status`

Without `RUN_ID`, lists known run manifests. With `RUN_ID`, returns that run's
manifest. This is a read-only query and exits `0` when the query succeeds,
regardless of whether the reported run itself failed or was cancelled.

### `doctor`

Checks that every executable required by the effective policy is discoverable
and reports platform capabilities. Each route reports `attachment_input`, its
native carrier, the four locally accepted image media types when applicable,
and whether a live provider probe occurred. `provider_live_probe` is currently
always `false`: `doctor` proves the local adapter/config contract, not endpoint
multimodal behavior. A missing required executable exits `2`.

The report intentionally warns that process isolation is not an OS sandbox. The warning does
not by itself fail `doctor`; missing required routes do.

### `run`

Validates the brief, snapshots its evidence roots and attachments, creates a
queued run, and starts a per-run background worker. Without `--wait`, the
command returns the run id and `queued` status immediately; the worker owns
advancement through R1, R2, both R3 execution bindings, and deterministic
merge.

With `--wait`, the initiating process observes the manifest until it reaches a
terminal state or `waiting_primary_arbiter`. The worker remains a separate process, so
Ctrl-C interrupts only observation (exit `130`) and does not cancel the run.
Worker launch metadata and logs are retained under `diagnostics/`. QUINTE does
not require a resident daemon. The worker writes a one-second heartbeat and a
finished marker. `wait` reports a stale/dead worker and directs the caller to
`resume` instead of polling forever.

### `wait`

Polls an existing manifest until the run is terminal or reaches `waiting_primary_arbiter`.
It observes state; it does not advance the scheduler. Ctrl-C interrupts only
the local wait, returns `130`, and does not cancel the run.

### `resume`

Continues the next incomplete phase. Previously accepted lane artifacts are
reused. Before continuing, the runtime verifies the stored per-run brief, policy, and
snapshot manifest hashes and the copied snapshot file hashes. Integrity drift
blocks continuation rather than silently creating a different trial.

`resume` first reconciles scheduler-owned child records using PID plus process
start identity. A verified orphan from a dead worker is terminated before the
lane can be retried; a reused bare PID is never signalled. Every existing
`attempt-<n>` directory counts against the fixed attempt budget, including one
left by a crash before output capture, so restart cannot reset or bypass the
budget.

Use a new run for a changed question, policy, or evidence snapshot.

### `cancel`

Records an explicit cancellation request and asks active child process trees to
terminate, escalating termination if needed. The `cancel` command itself exits
`0` when the request is handled. Commands that subsequently observe a
`cancelled` run use exit `4`.

Cancellation supervision is implemented with Unix process groups and Windows
`taskkill`; it is not an OS sandbox.

### `inspect`

Returns the run manifest, parsed event log, and `result.json` when one exists.
Human output is a status summary; use `--json` when consuming evidence or
integrating the Primary Arbiter.

### `agents`

`agents list` reports the fixed R1/R2 roster. `agents describe ID` accepts a
wire-role id or route id and reports its configured adapter binding. It does
not execute that binding. `Counterpart Arbiter` can be described but is not
included in the R1/R2 list.

### `policy`

`policy show` prints the effective policy. `policy validate` checks its closed
runtime invariants. QUINTE deliberately has no general-purpose CLI policy mutation
command. Policies from before the R3 role rename may use `auditor` with
`party_id` set to `Auditor B`; QUINTE accepts those exact legacy names and
normalizes them to `counterpart_arbiter` / `Counterpart Arbiter` in memory. The
legacy field and party id are accepted only as that pair; partially renamed
combinations are rejected.

Read-only commands and normal runs never rewrite the source `policy.json`;
`init --force` remains the only way to replace it. `policy show` and
`policy validate` can inspect a normalized v1 policy, but `run` refuses it with
an explicit backup-and-migrate instruction. Production v2 also requires
`auto_primary_arbiter=true`.

## Brief Contract

`quinte run` accepts a UTF-8 JSON file conforming to
[`schemas/brief.schema.json`](../schemas/brief.schema.json):

```json
{
  "brief_version": "1.1",
  "question": "Required non-empty question",
  "context": "Optional bounded context",
  "evidence_roots": ["/absolute/or/resolvable/path"],
  "snapshot_ignore": [".firecrawl", "tools/r4se-packages", "**/*.key"],
  "attachments": ["/path/to/evidence.png"],
  "action_scope": "Optional scope for the resulting verdict"
}
```

Unknown fields are rejected. Evidence roots are copied into the run before any
lane starts. The snapshot excludes common generated or sensitive path names,
including `.git`, `node_modules`, `target`, `.quinte`, `.env`, `*.key`, and
`*.pem`; it does not follow symlinks. Optional `snapshot_ignore` entries are
portable `/`-separated glob patterns relative to every evidence root. For a
single-file root, its filename is the relative path. Matching directories are
pruned together with their contents.

Attachments are identified from file bytes, not their extension. QUINTE accepts
PNG, JPEG, WebP, and GIF within the configured size limit. An accepted image
selects the multimodal model. MiMo passes each image with `--file`; Codex passes
each image with `--image`. Reasonix exposes no native image argument while
QUINTE disables file tools, so DeepSeek/Reasonix policies reject a brief with
attachments before creating a run. The source files are not modified.

Each copied image receives an exact `attachment://attachment-N.<type>` entry in
`snapshot-manifest.json`. Claims and residuals may cite that value, or an exact
`snapshot://` value, in `evidence_refs` and `closure_evidence`. Arbitrary
suffixes and paths not present in the manifest fail the output gate.

## State Machine

The persisted `manifest.json` status is one of:

```text
queued
preflight
r1_running
r1_gate
r2_packet
r2_running
r2_gate
r3_cc
waiting_primary_arbiter
merging
completed
degraded
failed
failed_policy
cancelling
cancelled
```

The normal flow is:

```text
queued -> preflight -> r1_running -> r1_gate
       -> r2_packet -> r2_running -> r2_gate
       -> r3_cc -> merging -> completed
```

Existing runs created under policy v1 can instead pass through
`waiting_primary_arbiter` before merge. It is non-terminal and may be returned
with exit `0`; callers must inspect the status value. Policy v1 and policy v2
with automatic PA disabled cannot start a new production run.

`completed`, `degraded`, `failed`, `failed_policy`, and `cancelled` are terminal
states. A completed result still does not authorize any external action.

## R3 Binding Paths

The `Counterpart Arbiter` wire binding runs first in R3. The scheduler then
creates:

- `r3/evidence-packet.json`: the accepted R1/R2 evidence and snapshot binding
- `r3/cc-response.json`: Counterpart Arbiter's typed verdict
- `r3/input-receipt.json`: SHA-256 bindings for all accepted R1/R2 artifacts,
  the evidence packet, and the CC verdict
- `r3/primary-arbiter-request.json`: the challenge the Primary Arbiter must answer

With auto PA enabled, the scheduler sends the evidence and counterpart verdict
to the same-family `Primary Arbiter` wire binding, validates `ArbiterVerdict`,
constructs the bound response, and consumes the challenge without host
submission.

For an existing historical run already waiting for manual PA,
`quinte primary-arbiter request RUN_ID --json` returns the challenge. It contains:

```text
run_id
nonce
policy_sha256
evidence_packet_sha256
input_receipt_sha256
action_scope
issued_at
expires_at
consumed
```

For a historical manual handoff, the external producer must use the evidence
packet and counterpart response to create a typed verdict, then write a
response conforming to
[`schemas/primary-arbiter-response.schema.json`](../schemas/primary-arbiter-response.schema.json):

```json
{
  "primary_arbiter_response_version": "1.0",
  "run_id": "exact value from primary-arbiter-request.json",
  "nonce": "exact value from primary-arbiter-request.json",
  "policy_sha256": "exact value from primary-arbiter-request.json",
  "evidence_packet_sha256": "exact value from primary-arbiter-request.json",
  "input_receipt_sha256": "exact value from primary-arbiter-request.json",
  "action_scope": "exact value from primary-arbiter-request.json, including null",
  "verdict": {
    "arbiter_verdict_version": "1.0",
    "summary": "Evidence-based summary",
    "recommendation": "Recommended actions",
    "residuals": []
  }
}
```

Submit it only through:

```bash
quinte primary-arbiter submit RUN_ID --verdict /path/to/arbiter-verdict.json --json
```

`--verdict` is the preferred host boundary: the external producer supplies only
the `ArbiterVerdict`, and the CLI copies the challenge bindings into the
scheduler-owned response. The verdict file must be outside the run directory.
The lower-level `--response` form remains for non-host API integrations but
must likewise read an external file and match every challenge field exactly.

The CLI rejects unknown response fields, an expired challenge, mismatched run,
nonce, policy, evidence digest, input-receipt digest or action scope, and replay
of a consumed challenge. Submission uses a durable `staging -> accepted`
receipt, so an identical retry can recover either crash window without
accepting a different response. A valid submission is copied into the run,
recorded in the event log, and immediately advances through deterministic
merge.

Model text such as `primary_arbiter_approved` or a lane's self-reported identity
is not a primary-arbiter acceptance signal. Directly placing
`primary-arbiter-response.json` in the run directory is an unsupported internal
operation and cannot bypass challenge validation; host integrations must use
the handshake command.

Runs staged by the earlier HM-named runtime remain resumable: the scheduler
validates `r3/hm-response.json` against its exact historical schema and receipt
binding without rewriting it or synthesizing a current response artifact. New
submissions remain current-only and always use the Primary Arbiter contract.

The challenge is a state-integrity and replay control, not cryptographic user
authentication. QUINTE does not sign the response or prove the operating-system
identity of the process that wrote it. Protect access to the state root and use
an authenticated host control channel when identity authentication is needed.

During merge, conflicting residuals with the same id are retained as
`unresolved` and `open`, and unequal recommendation strings are preserved in
the `dissent` compatibility field. `dissent`, `perspectives`, and the other
trial-manifest names are RASHOMON Trace 1.1 data-contract compatibility fields;
they do not control execution and do not make RASHOMON a runtime dependency.
The CLI writes `result.json` and `report.md` only after merge.

## JSON Output

Commands that reach their normal JSON emission path write one compact envelope
to stdout:

```json
{
  "cli_envelope_version": "1.0",
  "ok": true,
  "data": {}
}
```

The shape of `data` depends on the command. `ok` means the CLI emitted a valid
envelope; callers must still inspect command-specific data and the process exit
code. For example, a completed `doctor --json` check can return an envelope
with `ok: true` while its report has `data.ok: false` and the process exits `2`.

Informational messages and errors use stderr; callers must not merge stderr
into the JSON stream. In particular, `run --json` may announce the newly
created run id on stderr before writing its stdout envelope.

Errors do not currently promise a JSON error envelope. Use the exit code,
stderr, persisted manifest, and `inspect` for failure handling.

## Exit Codes

| Code | Meaning |
| ---: | --- |
| `0` | Command succeeded. For advancing commands this includes `waiting_primary_arbiter`; inspect the returned status. |
| `1` | Runtime, adapter, output-contract, or protocol execution failure. |
| `2` | CLI usage, initialization, brief/snapshot preflight, or missing-route failure. |
| `3` | Policy or integrity violation, including primary-arbiter binding mismatch or replay. |
| `4` | The observed run is cancelled. `quinte cancel` itself returns `0` when handled. |
| `130` | Local `quinte wait` was interrupted; the run was not implicitly cancelled. |

Read-only `status` returns `0` when it can report state, even if the reported
run has a non-success terminal status. `inspect`, `wait`, `resume`, `run`, and
`primary-arbiter submit` map an observed terminal run status to the codes above.

## Artifact Layout

Artifacts are append-only or atomically replaced by the scheduler as
appropriate. Do not edit them to advance a run.

```text
<state-root>/
  policy.json
  runs/<run-id>/
    manifest.json
    events.jsonl
    input/
      brief.json
      policy.json
      snapshot-manifest.json
      task-packet.json
      snapshot/root-*/...
      attachments/attachment-*.*
    packets/
      r2.json
    lanes/
      R1/<route-id>/
        accepted.json
        retry-deadline.json
        attempt-<n>/
          invocation.json
          stdout.bin
          stderr.bin
      R2/<route-id>/
        accepted.json
        retry-deadline.json
        attempt-<n>/...
      R3/cc/
        retry-deadline.json
        attempt-<n>/...
    r3/
      evidence-packet.json
      cc-response.json
      input-receipt.json
      primary-arbiter-request.json
      primary-arbiter-response.json
    diagnostics/
      r2-rate-state.json
    result.json
    report.md
```

`manifest.json` is the current-state projection. `events.jsonl` is the ordered,
fsynced audit trail; an uncommitted torn tail is truncated, while corruption in
a committed record fails closed. A pending transition receipt repairs the
manifest/event crash window. `accepted.json` files are the typed lane outputs
used by later gates. Attempt directories preserve raw adapter evidence even
when an attempt is rejected. `active-pids.json` stores PID plus process start
identity, and `cancel.requested` may appear at the run root as a runtime control
artifact. A completed/degraded manifest contains the SHA-256 of `result.json`;
`inspect` and `wait` reject a missing or modified final result.

`diagnostics/r2-rate-state.json` is the scheduler-owned next-transport deadline
for serial R2 pacing. A lane-local `retry-deadline.json` records retry backoff
for every phase. Both are written atomically before a wait and honored by
`resume`. The event ledger records `r2.pacing_wait`, `lane.retry_scheduled`,
`lane.retry_wait`, and `lane.retry_started` with typed timing metadata; model
output cannot create or override these decisions.

Not every path exists in every run. A failed R1 run has no R2 or R3 products;
a `waiting_primary_arbiter` run has no `primary-arbiter-response.json`, `result.json`, or `report.md`.

The state root may contain copied source evidence and raw model output. Protect,
retain, and delete it according to the sensitivity of the reviewed material.

## Isolation Boundary

The adapters clear inherited environment variables except a small runtime
set, assign per-lane HOME/config/cache/state directories, use separate working
directories, request read-only tool sets where supported, validate strict UTF-8
and closed JSON schemas, cap captured output, and supervise child process trees.

These controls reduce accidental cross-lane state and prompt drift. They do not
install a kernel-enforced filesystem or network sandbox and cannot guarantee
that a native CLI honors every requested permission flag. A lane process still
has the OS credentials of the `quinte` process.

Use an external OS sandbox, container, VM, or restricted account for hostile
code, secrets, or network containment. Do not describe `process` isolation
as a security sandbox.

## Provider Binding

MAGI/container integrations select one key variable and one base URL variable
through `QUINTE_PROVIDER_KEY_ENV` and `QUINTE_PROVIDER_BASE_URL_ENV`. The
selected names must match the seat family: `XIAOMI_*`, `DEEPSEEK_*`, or
`OPENAI_*`. Only that pair is copied into a lane environment. URLs must be
configured HTTPS endpoints without whitespace or `.invalid`; OpenAI relays
must support the Responses API used by Codex.

`QUINTE_PROVIDER_PROXY_MODE` selects provider egress behavior. It defaults to
`inherit`, preserving the allowlisted `HTTP(S)_PROXY`/`NO_PROXY` environment
for mandatory container or host gateways. `direct` preserves the proxy values
but appends the selected provider endpoint host to `NO_PROXY` and `no_proxy`.
Use `direct` only after an endpoint-specific reachability check; it is never
inferred from the provider family or current model. Any other value fails
before provider credentials are imported into a lane environment.

The removed historical credential command and helper are not part of policy-v1
inspection or the production v2 command surface.
