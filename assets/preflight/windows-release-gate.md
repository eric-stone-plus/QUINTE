# Windows Real-Machine Release Gate (OPEN)

Date: 2026-07-15  
Host used for implementation: macOS (this machine)  
QUINTE branch: `impl/0.1.4-handoff`  
Plan reference: `assets/WINDOWS_REAL_MACHINE_ACCEPTANCE.md`

## Status

**OPEN — not passed.**  
Job Object containment is implemented in source and covered by portable structural tests plus the existing Unix process-group suite. The full Windows lifecycle matrix was **not** executed on a supported Windows 11 host and must not be treated as green for release.

## Implemented (code-level, verified on macOS)

- Adapter spawn creates a Windows Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
- Process is created with `CREATE_SUSPENDED`, assigned via `AssignProcessToJobObject`, then resumed
- Residual cleanup after leader exit terminates the owned job (no longer a no-op)
- Timeout/cancel/output-limit paths pass the job into tree termination
- Portable structural test: `windows_job_object_containment_is_wired_in_source`
- Full `cargo test --all-targets --all-features` on macOS (see `quinte-cargo-phase3.txt`)

## Not executed here (required before release)

From `assets/WINDOWS_REAL_MACHINE_ACCEPTANCE.md` §3–§8:

| Case | Status |
| --- | --- |
| Normal completion, no survivors | NOT RUN |
| Timeout with forever-sleeping descendant | NOT RUN |
| Cancel with grandchild writers | NOT RUN |
| Ctrl-C foreground interrupt | NOT RUN |
| Leader exits first (grandchild retained pipes) | NOT RUN |
| Grace ignored / forced job kill bound | NOT RUN |
| Output cap flood | NOT RUN |
| Adapter crash after spawn | NOT RUN |
| Worker crash recovery without leaked child jobs | NOT RUN |
| 100-run repetition (handle/process leak) | NOT RUN |
| Pipe/handle inheritance checks | NOT RUN |
| PowerShell path/Unicode/long-path CLI matrix | NOT RUN |
| Windows Claude credential contract | NOT RUN |
| Evidence bundle with process-tree exports | NOT RUN |

## Release rule

Do not tag or publish a Windows-supporting release until the real-machine acceptance bundle is produced on a physical or ordinary Windows 11 x64 VM per `WINDOWS_REAL_MACHINE_ACCEPTANCE.md` §8–§9. macOS unit tests must not be claimed as substitute proof for that matrix.
