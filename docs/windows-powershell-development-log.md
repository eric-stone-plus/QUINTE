# Windows PowerShell Development Log

Date: 2026-07-13

## Scope

This record documents the Windows process-launch fixes made after QUINTE 0.1.0
could discover its fixed agents during `quinte doctor` but could not start four
of them during R1. It intentionally excludes host-specific paths, run IDs,
credentials, and runtime artifacts.

## Incident Signature

- CodeWhale, KiloCode, MiMoCode, and Oh-My-Pi were installed as npm shims.
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
- npm-style commands are validated through their sibling `.ps1` shim, then
  resolved to the exact Node.js or Bun runtime and package entrypoint, or the
  package-local native executable, named by that standard npm shim. The
  validator accepts complete npm `cmd-shim` templates only; extra commands,
  altered control flow, ambiguous calls, and escaping paths fail closed.
- QUINTE launches the runtime directly. This avoids npm's PowerShell pipeline
  branch treating QUINTE's null stdin as an empty pipeline and exiting 0
  without ever starting the package entrypoint.
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
- Worker liveness uses native Windows process handles rather than localized
  `tasklist` text. Waiters reload the durable manifest after a worker exit so
  a concurrently published `waiting_hm` state cannot be mistaken for a crash.

### Silent adapter processes

- Every Windows lane process is created with `CREATE_NO_WINDOW`, including the
  Node.js or Bun runtime and package-local native executable resolved from a
  validated npm PowerShell shim.
- QUINTE still captures stdout and stderr through pipes; hiding the console
  window does not discard model output or diagnostics.
- Silent launch is a QUINTE runtime guarantee, not a per-device PowerShell or
  Windows Terminal preference. Windows hosts receive it by installing the same
  QUINTE release.

### Lane environment

The minimal environment still preserves the explicit process `PATH` and the
Windows OS-root contract (`SystemRoot`, `WinDir`, and `SystemDrive`). Windows
lanes additionally bind `USERPROFILE`, `TEMP`, `TMP`, `APPDATA`, and
`LOCALAPPDATA` to the lane root. This prevents Node-based agents from ignoring
`HOME`/`TMPDIR`, while keeping system-drive expansions from becoming literal
`%SystemDrive%` directories beneath the lane. Shared machine configuration
roots such as `ProgramData` remain outside the lane environment.

### Diagnostics and CI

- Doctor output includes the configured executable, resolved source, resolved
  launcher, launcher type (`native` or `npm-runtime`), and a stable resolution
  code. Missing runtimes, missing or unsafe entrypoints, unsupported shims, and
  absent PATH commands are reported separately.
- The release matrix runs the full feature-enabled tests on every native build
  host before packaging.

## Regression Coverage

Windows tests cover:

- an extensionless POSIX npm sibling plus `.cmd` and `.ps1` Windows siblings;
- fail-closed behavior when only the unsafe `.cmd` sibling remains;
- lossless arguments containing newlines, quotes, `&`, and `<`;
- build, spawn, output capture, and schema parsing through an npm-style shim;
- real Node.js and Bun wrappers spawning the final fake agent with inherited
  streams and no wrapper-specific window-hiding option;
- rejection of comment, here-string, dead-branch, argument-rewrite, ambiguous,
  incomplete, and path-escaping shim variants, plus Bun and package-local
  native template resolution;
- no-window creation flags on Windows adapter commands;
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

## Release CI Follow-up

Date: 2026-07-14

- The Windows integration suite executes the real Node.js and Bun runtimes
  resolved from validated npm shims. The release matrix now installs fixed
  runtime versions instead of depending on the hosted image to provide Bun.
- Branch pushes run the same five native test, build, and packaging jobs as a
  release tag. Publishing remains tag-only, so a tag is created only after the
  exact commit has passed the complete matrix.
- Release tags must match the Cargo package version. Release workflow actions
  are pinned to reviewed commits rather than floating major-version refs.
- The Unix SIGINT test waits for an explicit, feature-gated readiness marker
  after the signal handler is installed. Fake-adapter environment mutation is
  serialized, removing architecture- and load-dependent test races.
- The detached-worker regression uses a fake-agent start/release handshake
  instead of treating a fixed wall-clock limit as proof of process detachment.
- The detached-worker regression keeps a separate 120-second guard for the
  parent CLI return and a 300-second readiness budget for the worker handshake.
  This preserves the detachment assertion without failing on slower native ARM
  runners under concurrent test load; the product wait behavior is unchanged.
- The Windows Node setup action is pinned to a Node 24 runtime release, avoiding
  the hosted-runner deprecation fallback while retaining the fixed Node version
  used by the npm-shim integration tests.
- Fake-agent integration fixtures compile once per test binary and copy the
  cached executable into isolated test directories. This removes repeated
  runtime compiler contention on slower native ARM and Windows runners.
- Cancellation and parallel-lane draining tests use fake-agent readiness and
  durable lane completion evidence instead of short PID or elapsed-time gates.
- Fixture compilation explicitly closes its scoped temporary directory after
  caching the executable bytes. Panic cleanup cancels, releases, and joins a
  controlled worker before the outer test directory is removed.
- Final verification covered the complete feature-enabled suites on Windows
  and Linux, plus the changed asynchronous, cancellation, and draining paths
  under ARM64 user-mode emulation. Native ARM CI remains the release gate.
