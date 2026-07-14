# Windows Real-Machine Acceptance Memo

Date: 2026-07-15  

This is a release gate, not a unit-test wish list. Run it on a real supported
Windows machine after implementing descendant-process containment. Record the
QUINTE commit, **product version `0.1.1`**, Windows build, shell version,
adapter versions, and hashes of all captured artifacts.

Implementation on macOS is **not** a substitute pass. Gate status and product
version are summarized in `GROK_HANDOFF.md`.

## 1. Defect this plan must close

On Windows, adapter commands are hidden but are not placed in a Job Object
(`src/adapters.rs:819`). Cleanup uses `taskkill /PID <pid> /T`
(`src/run.rs:3264`), while cleanup after an exited leader is a no-op
(`src/run.rs:3280`). A leader can therefore exit after spawning a descendant,
leaving the descendant alive and holding inherited stdout/stderr pipes.

The implementation should use an owned Windows Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, assign every adapter/worker process before
untrusted child code can escape, and close the job handle on normal completion,
timeout, cancellation, output-limit failure, scheduler error, and process exit.
If assignment cannot be made race-free with ordinary spawn, use a suspended
creation/resume design rather than accepting a containment window.

## 2. Required test environment

- physical or ordinary VM Windows 11 x64; do not rely only on Wine
- PowerShell 5.1 and PowerShell 7 where both are supported
- checkout path containing spaces and non-ASCII characters
- normal path plus a long-path case near the supported limit
- release build from the exact candidate commit
- at least one fake adapter capable of spawning a child and grandchild,
  retaining inherited pipes, exiting the leader early, ignoring graceful
  termination, emitting excessive output, and sleeping indefinitely

Do not use production credentials in fault-injection fixtures.

## 3. Process lifecycle matrix

For every row, capture the initial process tree, terminal result, QUINTE event
log, final process tree, open-handle observation, and run artifacts.

| Case | Injection | Required result |
| --- | --- | --- |
| Normal | leader and descendants exit normally | completed result; no surviving process or open pipe |
| Timeout | descendant sleeps forever | bounded timeout; entire job gone; scheduler returns promptly |
| Cancel | cancel while grandchild writes output | cancelled result; entire job gone; no late artifact mutation |
| Ctrl-C | interrupt foreground wait | bounded cancellation; worker and adapter descendants gone |
| Leader exits first | leader spawns grandchild then exits | grandchild still contained and terminated; no pipe hang |
| Grace ignored | descendants ignore graceful signal | forced job termination within documented bound |
| Output cap | descendant floods stdout/stderr | output-limited result; job gone; captured output remains bounded |
| Adapter crash | leader crashes after spawn | no orphan descendant; retry classification remains correct |
| Worker crash | scheduler worker is killed | recovery detects it; owned child jobs are not leaked |
| Repetition | run fault cases 100 times | zero accumulated adapter/worker descendants and zero stuck runs |

Use Process Explorer or equivalent handle/process-tree evidence in addition to
`Get-Process`. A passing CLI exit alone does not prove descendant cleanup.

## 4. Pipes and handle inheritance

Verify all of the following:

- only intended stdio handles are inheritable
- parent-side duplicate handles are closed after spawn
- a grandchild cannot keep scheduler reads blocked after its leader exits
- stdout and stderr reader threads finish on every terminal path
- cancellation and timeout remain bounded even when a pipe writer misbehaves
- repeated runs do not monotonically increase QUINTE handle count

Capture before/after handle counts for the 100-run repetition case.

## 5. CLI, paths, and PowerShell

Run `init`, `doctor`, `run`, `wait`, `hm request`, `hm submit`, `inspect`, and
`cancel` using:

- PowerShell quoting with paths containing spaces
- a Unicode checkout/evidence path
- forward and backslash input where the CLI documents support
- an attachment and evidence root on a different drive
- a read-only evidence source
- an interrupted command followed by resume/inspect

All generated artifact references must remain portable slash-form references
inside the run, while native filesystem operations remain correct.

## 6. Credential and environment isolation

- Verify the agreed Claude token contract on Windows. The current non-macOS
  code requires `ANTHROPIC_API_KEY`; do not accept accidental dependence on an
  unrelated ambient variable.
- Confirm `env_clear` plus the allowlist exposes no unrelated secrets.
- Confirm secrets do not appear in stdout, stderr, event logs, snapshots,
  receipts, `result.json`, crash output, or diagnostic files.
- Run `doctor` with each required credential missing and confirm the failure is
  specific, actionable, and version-correct.

## 7. Artifact and protocol integrity

For a successful and each failed/cancelled run, verify:

- event sequence is monotonic and terminal status is stable after restart
- snapshot, runtime, policy, packet, receipt, and result digests validate
- exact snapshot-reference validation applies to lanes and HM verdict residuals
- claim and residual IDs are unique in every accepted artifact
- an R1/R2 high-risk residual cannot disappear merely because both R3 arbiters
  omit it
- conflicting residual fields produce an explicit conservative result
- no artifact changes after finalization or cancellation
- every QUINTE-owned version field equals the release package version `0.1.1`

## 8. Evidence bundle

Store a single acceptance bundle containing:

- `ACCEPTANCE.md` with machine and tool versions
- candidate commit and clean/dirty status
- commands and exit codes
- per-case process-tree screenshots or exports
- per-case event logs and run artifacts
- before/after handle counts
- a machine-readable case summary
- SHA-256 manifest for the bundle

Redact credentials without altering structural evidence.

## 9. Release gate

Windows acceptance passes only if all lifecycle cases terminate within their
documented bounds, no descendant or handle leak remains, artifact invariants
hold, and the 100-run repetition case is clean. A flaky pass, manual process
kill, reboot requirement, or unexplained surviving process is a release block.
