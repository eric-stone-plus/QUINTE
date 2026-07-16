# QUINTE CLI Contract

This document defines the public command boundary for the `quinte` Rust
CLI. The CLI is the execution authority for a QUINTE run. A host such as the
Primary Arbiter may create a brief, invoke commands, supply the `primary-arbiter` verdict, and consume the
result; it must not reproduce the scheduler with ad hoc agent calls.

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

Official release archives contain one `quinte` executable for each supported
platform and a shared checksum manifest. `install.sh` and `install.ps1`
download and verify that executable; end users do not need Rust, Cargo, or a
repository checkout. The release build excludes all test-adapter code.

QUINTE is not a hosted proxy and does not bundle or impersonate its fixed
native routes. CodeWhale, OpenCode, Kilo, MiMo, OMP, Claude Code, and their
existing token-plan credentials remain runtime prerequisites for a complete
run. `doctor` reports the exact missing prerequisite before execution.

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
quinte primary-arbiter request RUN_ID [--json]
quinte primary-arbiter submit RUN_ID (--verdict FILE | --response FILE) [--json]
quinte agents list [--json]
quinte agents describe ID [--json]
quinte policy show [--json]
quinte policy validate [--json]
```

No public command runs an individual R1/R2 party. There is no phase-skip,
substitution, arbitrary model, arbitrary adapter, or agent-selected transition
command.

### `init`

Creates the state root, `policy.json`, and `runs/`. It refuses to replace an
existing policy unless `--force` is supplied.

The default policy fixes Party A-E to CodeWhale, OpenCode, Kilo, MiMo, and OMP,
and fixes Counterpart Arbiter to Claude Code. Text uses `mimo-v2.5-pro`; a supported image
attachment selects `mimo-v2.5` for the whole run.

### `status`

Without `RUN_ID`, lists known run manifests. With `RUN_ID`, returns that run's
manifest. This is a read-only query and exits `0` when the query succeeds,
regardless of whether the reported run itself failed or was cancelled.

### `doctor`

Checks that every executable required by the effective policy is discoverable
and reports platform capabilities. A missing required executable exits `2`.

The report intentionally warns that process isolation is not an OS sandbox. The warning does
not by itself fail `doctor`; missing required routes do.

### `run`

Validates the brief, snapshots its evidence roots and attachments, creates a
queued run, and starts a per-run background worker. Without `--wait`, the
command returns the run id and `queued` status immediately; the worker owns
advancement through R1, R2, and the Counterpart Arbiter part of R3.

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
party id or route id and reports its configured adapter binding. It does not
run the party. Counterpart Arbiter can be described but is not included in the R1/R2
list.

### `policy`

`policy show` prints the effective policy. `policy validate` checks its closed
runtime invariants. QUINTE deliberately has no general-purpose CLI policy mutation
command. Policies from before the R3 role rename may use `auditor` with
`party_id` set to `Auditor B`; QUINTE accepts those exact legacy names and
normalizes them to `counterpart_arbiter` / `Counterpart Arbiter` in memory. The
legacy field and party id are accepted only as that pair; partially renamed
combinations are rejected.

Read-only commands and normal runs never rewrite the source `policy.json`;
`init --force` remains the only way to replace it.

## Brief Contract

`quinte run` accepts a UTF-8 JSON file conforming to
[`schemas/brief.schema.json`](../schemas/brief.schema.json):

```json
{
  "brief_version": "1.0",
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
selects the multimodal model. The source files are not modified.

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
       -> r3_cc -> waiting_primary_arbiter
       -> merging -> completed
```

`waiting_primary_arbiter` is non-terminal. It is a deliberate host handoff and may be
returned with exit `0`; callers must inspect the status value instead of using
the exit code alone as proof of completion.

`completed`, `degraded`, `failed`, `failed_policy`, and `cancelled` are terminal
states. A completed analysis still does not authorize any external action.

## Primary Arbiter Handshake

Counterpart Arbiter runs first in R3. The scheduler then creates:

- `r3/evidence-packet.json`: the accepted R1/R2 evidence and snapshot binding
- `r3/cc-response.json`: Counterpart Arbiter's typed verdict
- `r3/input-receipt.json`: SHA-256 bindings for all accepted R1/R2 artifacts,
  the evidence packet, and the CC verdict
- `r3/primary-arbiter-request.json`: the challenge the Primary Arbiter must answer

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

The Primary Arbiter must read the evidence packet and Counterpart Arbiter response, independently
draft its verdict, and write a response conforming to
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
    "summary": "Primary Arbiter evidence-based summary",
    "recommendation": "Primary Arbiter recommendation",
    "residuals": []
  }
}
```

Submit it only through:

```bash
quinte primary-arbiter submit RUN_ID --verdict /path/to/arbiter-verdict.json --json
```

`--verdict` is the preferred host boundary: the Primary Arbiter supplies only the
`ArbiterVerdict`, and the CLI copies the challenge bindings into the
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

Model text such as `primary_arbiter_approved` or a lane's self-reported identity is not a
primary-arbiter acceptance signal. Directly placing `primary-arbiter-response.json` in the run directory is
an unsupported internal operation and cannot bypass challenge validation;
host integrations must use the handshake command.

The challenge is a state-integrity and replay control, not cryptographic user
authentication. QUINTE does not sign the response or prove the operating-system
identity of the process that wrote it. Protect access to the state root and use
an authenticated host control channel when identity authentication is needed.

During merge, conflicting residuals with the same id are retained as
`unresolved` and `open`, and recommendation disagreement is preserved as
dissent. The CLI writes `result.json` and `report.md` only after merge.

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

## Credential Commands

### `credential status`

Provision the Claude / MiMo token with OS-native protected-store tooling or UI:

- macOS: a Keychain generic password whose account is the current login user
  and service is `xiaomi-mimo-token-plan-api-key`.
- Windows: a Generic Credential whose target is
  `xiaomi-mimo-token-plan-api-key.quinte`.

QUINTE intentionally exposes no secret-writing command. Verify provisioning
with `quinte credential status --json`.

`ANTHROPIC_API_KEY` remains a legacy non-isolated fallback. Doctor reports
`credential_source` and `credential_isolated` for the Claude route.

### `__credential-helper` (hidden)

Internal Claude Code `apiKeyHelper` entrypoint. It requires a private per-lane
authorization file and binds the request to the canonical lane root and fixed
service. No credential or bearer token is placed in the helper command line or
script. User hosts must not invoke it directly.
