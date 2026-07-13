# Windows PowerShell Development Log

Date: 2026-07-13

## Scope

This record documents the Windows process-launch fixes made after QUINTE 0.1.0
could discover its fixed agents during `quinte doctor` but could not start four
of them during R1. It intentionally excludes host-specific paths, run IDs,
credentials, and runtime artifacts.

## Incident Signature

- CodeWhale, Kilo, MiMo, and OMP were installed as npm shims.
- OpenCode was installed as a native `.exe` and was the only R1 lane that
  started successfully.
- Adding the npm directory to `PATH` did not fix the four failed lanes.
- `doctor` reported the tools as available because its lookup recognized
  `.cmd`; runtime passed the extensionless name to `std::process::Command`.

The mismatch was Windows-specific. `Command` can infer `.exe`, but an npm
`.cmd` shim needs an explicit extension and a command interpreter. Passing the
full `.cmd` path was not sufficient: Rust correctly rejects batch arguments
containing unsafe shell characters. QUINTE prompts contain newlines and can
contain characters such as `&` and `<`, so a `cmd.exe /c` string would be both
fragile and an avoidable injection boundary.

## Implemented Design

### Command resolution

- `doctor` and runtime now share one resolver.
- Native `.exe` and `.com` programs are launched directly by absolute path.
- npm-style commands are resolved to their sibling `.ps1` shim.
- PowerShell shims run through the absolute Windows PowerShell executable with
  `-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`.
- Every QUINTE argument remains a separate OS argument. No prompt or path is
  concatenated into a shell command string.
- A lone `.cmd` or `.bat` without a sibling `.ps1` fails closed.
- CodeWhale resolution checks the actual `codewhale-tui` entry used at runtime,
  rather than validating only the policy's `codewhale` launcher.

### PowerShell worker behavior

- Background workers do not retain the initiating PowerShell or test runner's
  standard pipe handles.
- Windows worker creation uses a `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` containing
  only private NUL and worker-log handles. It does not change the parent
  process's shared standard-handle inheritance flags.
- Worker creation remains an internal, single-threaded CLI boundary; scheduler
  lane concurrency starts only after the inheritable private duplicates close.
- The worker clears inheritance on those private standard handles before it
  launches adapters, so worker logging handles stop at the scheduler boundary.
- The launcher retains the native process handle until worker metadata is
  durable, preventing a fast exit from turning cleanup into a PID-reuse race.
- A short-lived child that exits before process identity collection is treated
  as completed, not as an unidentified live process. A still-running child
  without a verifiable identity continues to fail closed.

### Lane environment

The minimal environment still preserves the explicit process `PATH`. Windows
lanes additionally bind `USERPROFILE`, `TEMP`, `TMP`, `APPDATA`, and
`LOCALAPPDATA` to the lane root. This prevents Node-based agents from ignoring
`HOME`/`TMPDIR` and falling back to the real user profile or system temp.

### Diagnostics and CI

- Doctor output includes the configured executable, resolved source, resolved
  launcher, and launcher type (`native` or `powershell`).
- The release matrix runs the full feature-enabled tests on every native build
  host before packaging.

## Regression Coverage

Windows tests cover:

- an extensionless POSIX npm sibling plus `.cmd` and `.ps1` Windows siblings;
- fail-closed behavior when only the unsafe `.cmd` sibling remains;
- lossless arguments containing newlines, quotes, `&`, and `<`;
- build, spawn, output capture, and schema parsing through an npm-style shim;
- Windows profile and temp-directory isolation;
- immediate queued return from a PowerShell-hosted background worker;
- fast-exiting lane processes and durable worker completion.

Unix retains a separate extensionless executable resolution test. The Windows
resolver and environment additions are guarded with `cfg(windows)` so macOS
and Linux keep their direct executable behavior.

## macOS Comparison

The macOS technical profile was reviewed as a comparison, not as a source of
truth. Its useful principle is that background processes need an explicit
environment and PATH parity checks; editing `.zshrc` alone does not configure a
worker process. Its shell, Homebrew, Keychain, launchd, PTY, and absolute-path
patterns were not copied to Windows.

The comparison also exposed profile drift and security patterns that should
not be inherited: split runtime roots, legacy direct dispatch instructions,
plaintext credential propagation, and shell files without enforced LF endings.

## Operational Boundary

- Keep runtime data and detailed host diagnostics outside the public source
  repository.
- Never persist credentials in invocation metadata or development logs.
- Verify PowerShell and worker behavior with `doctor`, policy validation, and
  fake-agent tests before spending model calls on a full QUINTE round.
- A full live round remains an integration check, not a substitute for the
  deterministic Windows tests above.
