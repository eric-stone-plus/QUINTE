<div align="center">

<img src="assets/quinte-cover.svg" alt="QUINTE" width="100%">

# QUINTE

**A multi-agent review orchestrator with contract gates**

[![Redesign](https://img.shields.io/badge/redesign-2026--08-purple?style=flat)](specs/PROTOCOL-REDESIGN.md)
[![A2A v1.0](https://img.shields.io/badge/A2A-v1.0-blue?style=flat)](specs/HOST.md)
[![CLI](https://img.shields.io/badge/CLI-contract-orange?style=flat)](specs/CLI.md)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat)](LICENSE)

</div>

QUINTE is a review orchestrator: it dispatches one bounded review task to
independent agent seats, records conflicting findings, omissions, evidence
gaps, and unresolved risk, and merges the outcomes deterministically — before
a host adopts a conclusion. It is not an answer-voting system, and its results
are evidence, never authorization.

> **Redesign shipped (2026-08-18).** QUINTE was rebuilt from scratch as a
> generic multi-agent review orchestrator — see
> [specs/PROTOCOL-REDESIGN.md](specs/PROTOCOL-REDESIGN.md) for the design record
> (amended by [specs/SINGLE-VENDOR-DOCTRINE.md](specs/SINGLE-VENDOR-DOCTRINE.md))
> and [specs/HOST.md](specs/HOST.md) for the A2A v1.0 endpoint contract. The
> sections below document the shipped core; the legacy 0.2.x CLI host surface
> remains documented in [specs/HOST-CLI-LEGACY.md](specs/HOST-CLI-LEGACY.md).

## Architecture

- **One protocol everywhere.** QUINTE is an [A2A v1.0](https://a2a-protocol.org)
  agent to its hosts (Agent Card, `SendMessage`, `GetTask`, one
  `review.result` artifact per completed task — [HOST.md](specs/HOST.md)) and
  an A2A client to its seats. The seat roster runs on **one model family**
  (DeepSeek, official direct API) per the
  [single-vendor doctrine](specs/SINGLE-VENDOR-DOCTRINE.md); the five distinct
  review schools carry the diversity, resolved per run and recorded in the run
  manifest.
- **Adaptive rounds.** R1 first-pass review always runs (policy default five
  distinct schools); R2 anonymized recheck runs **only on the outputs R1
  actually contested**, via a deterministic, event-logged escalation rule
  (unanimity durably skips R2, k=0); R3 dual arbitration always runs.
  Common-path cost is `n + 2` invocations per run instead of a fixed twelve.
- **Policy-driven product.** Rounds, seat requirements, domain schemas, gates,
  and merge rules live in a doctrine pack. The quant review slice is the first
  pack; a new domain is a new pack, not a new product.
- **Non-negotiables carried over.** Deterministic state machine, event-ledger
  authority, closed schemas, fail-closed behavior, deterministic merge,
  offline verification, one active run.

## Current implementation

The new core implements the redesign (specs/PROTOCOL-REDESIGN.md, amended by
specs/SINGLE-VENDOR-DOCTRINE.md): five-school adaptive review — R1 always,
contested-only anonymized R2 (durably skipped on unanimity), dual-arbiter R3 —
over A2A v1.0 seats (the PI seat agent in pi/), one model family per the
single-vendor decision, deterministic merge, and the honest-labeling
trial_manifest caveat. The legacy 0.2.x CLI host surface remains documented in
[specs/HOST-CLI-LEGACY.md](specs/HOST-CLI-LEGACY.md); the A2A v1.0 front door
`quinte host serve` ([specs/HOST.md](specs/HOST.md)) exposes that host surface
as Agent Card / `SendMessage` / `GetTask` for STAMMTISCH.

An additive finance contract/replay surface is documented in
[FINANCE-PROTOCOL.md](specs/FINANCE-PROTOCOL.md). It provides strict Policy
3.0, Packet 2.0, Manifest 3.0, School Lane Output 1.0, Finance Review Result
1.0, deterministic fold/posture, HIGHBALL carriers, and offline verification.
The standalone dormant finance lifecycle is implemented behind an exact
operator acknowledgement. Production provider invocation and A2A finance
creation remain disabled.

## Quick Start

For a stable host install, use the immutable GitHub Release for your platform
and verify its archive against the published `SHA256SUMS`. Build from source
when developing or when testing an unreleased `main` commit.

### macOS / Linux

```bash
git clone https://github.com/eric-stone-plus/QUINTE.git
cd QUINTE
cargo build --release

# 1) CLI on PATH (required)
install -m 0755 target/release/quinte ~/.local/bin/quinte
# ensure ~/.local/bin is on PATH

# 2) Host progress helpers (required for interactive agents / host skill)
install -m 0755 scripts/quinte-progress scripts/quinte-run ~/.local/bin/
# or symlink so updates follow the checkout:
#   ln -sfn "$PWD/scripts/quinte-progress" ~/.local/bin/quinte-progress
#   ln -sfn "$PWD/scripts/quinte-run" ~/.local/bin/quinte-run

# 3) First-time state + environment check
quinte init    # first time only; creates ~/.quinte
quinte doctor  # after every install or rebuild
quinte --version
command -v quinte-progress quinte-run
```

### Windows (PowerShell)

```powershell
git clone https://github.com/eric-stone-plus/QUINTE.git
cd QUINTE
cargo build --release
$dir = Join-Path $env:LOCALAPPDATA "Programs\quinte\bin"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Copy-Item target\release\quinte.exe (Join-Path $dir "quinte.exe") -Force
Copy-Item scripts\quinte-progress, scripts\quinte-run $dir -Force
# add $dir to the user PATH, then open a new shell
quinte init
quinte doctor
```

### After every update

```bash
git pull
cargo build --release
install -m 0755 target/release/quinte ~/.local/bin/quinte   # or re-copy on Windows
# if you installed scripts by copy (not symlink), re-install them too
quinte doctor
```

### Host agent skill

The durable skill lives in this repo at [`skills/SKILL.md`](skills/SKILL.md).
Copy or symlink it into the host’s live skill directory after install or pull
(for example `<host-profile>/skills/multi-agent-debate/quinte/SKILL.md`). Host
skill trees are not version-controlled; **the repo file is the source of
truth**.

External agents must use the stable `quinte host` surface. It owns the global
launch lock, fail-closed one-active check, detached start receipt, one-shot
status, terminal integrity verification, and ambiguous-launch reconciliation.
`quinte-progress`/`quinte-run` remain human display helpers, not machine APIs.

For an ordered queue of Briefs, `scripts/contest_supervisor.py` is the
canonical optional outer-host pattern. It is one-shot and fail-closed: it
requires absolute `QUINTE_HOME`/`QUINTE_BIN` paths plus a SHA-256 pin, uses a
separate nonblocking lock, defaults to dry-run, and only accepts the next
contiguous sequence after a separate `host inspect` proves
`result.verified=true` and `result.actionable=true`. A failed, degraded,
ambiguous, malformed, or digest-drifted observation atomically creates a
`HALTED` sentinel. It never launches individual lanes or resumes old runs.
See [HOST-CLI-LEGACY.md](specs/HOST-CLI-LEGACY.md#ordered-outer-supervision) and
the offline tests for the complete boundary.

### Credentials and roster

The CLI is self-contained. The production seat binds the DeepSeek family,
executed by the native in-process adapter: an OpenAI-compatible
`POST {base_url}/chat/completions` HTTPS call with a Bearer key, no host CLI
required. Credentials come from exactly one selected provider key/base URL
pair: set `QUINTE_PROVIDER_KEY_ENV=DEEPSEEK_API_KEY` and
`QUINTE_PROVIDER_BASE_URL_ENV=DEEPSEEK_BASE_URL`, then export both variables.
HTTPS is required; whitespace and `.invalid` placeholders fail closed.
Provider traffic inherits the host proxy environment by default. Set
`QUINTE_PROVIDER_PROXY_MODE=direct` only when the selected provider endpoint
is explicitly intended and verified to bypass that proxy; `direct` adds only
that endpoint host to both `NO_PROXY` casings. Unknown modes fail closed.
`quinte doctor --json` checks all seven bindings before dispatch. Each route
row also reports its static attachment carrier, accepted media types, and
`provider_live_probe: false`; `doctor` verifies local routing/configuration,
not a live multimodal request to the provider.

Create a brief such as `brief.json`:

```json
{
  "brief_version": "1.1",
  "question": "Which material risks remain in this change?",
  "context": "Review the implementation, tests, and operational boundary.",
  "evidence_roots": ["/absolute/path/to/project"],
  "snapshot_ignore": [".git", "build/**", "**/*.key"],
  "attachments": [],
  "action_scope": "decision support for this change only"
}
```

Start through the machine-readable host contract:

```bash
quinte host preflight --json
quinte host start --brief brief.json --json
quinte host status RUN_ID --json
quinte host inspect RUN_ID --json
# if the start response was lost:
quinte host reconcile --json
```

`host start` returns immediately with a durable receipt while a supervised
background worker advances R1, R2, both R3 bindings, and deterministic merge:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"host_receipt_version":"1.0","operation":"start","invocation_id":"...","run_id":"...","state":{"code":"started","active_run_ids":["..."]},"manifest":{"status":"queued"}}}
```

`host preflight` is advisory and does not reserve a launch slot. `host start`
rechecks doctor results and active runs under its launch lock, so callers must
not treat a prior `ready` receipt as authorization to launch.

The low-level `--wait` flag is only for a human using `quinte run`; machine
hosts do not use it. Production policy v2 requires
`auto_primary_arbiter=true`, so a new run normally reaches `completed`.
Historical runs created under policy v1 may still expose:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"run_id":"...","status":"waiting_primary_arbiter","run_dir":"..."}}
```

`waiting_primary_arbiter` with exit code `0` is a historical/manual handoff,
not a completed verdict. Only for such an existing run, submit an external
verdict and inspect:

```bash
quinte primary-arbiter request RUN_ID --json
quinte primary-arbiter submit RUN_ID --verdict primary-arbiter-verdict.json --json
quinte inspect RUN_ID --json
```

The low-level `quinte wait RUN_ID` observes the same boundary for humans.
Ctrl-C interrupts only that wait and leaves the background run active.

See [CLI.md](specs/CLI.md) for the complete command contract, Primary Arbiter response
schema, state transitions, exit codes, and artifact layout.

## State and Evidence

The default state root is `~/.quinte`:

```text
~/.quinte/
  policy.json
  runs/<run-id>/
    manifest.json
    events.jsonl
    input/
    lanes/
      <phase>/<route-id>/retry-deadline.json
    packets/
    r3/
    diagnostics/
      r2-rate-state.json
    result.json
    report.md
```

Inputs are copied into a per-run snapshot. Lane attempts retain their
invocation metadata, raw stdout/stderr, and accepted typed result. `result.json`
is the machine artifact; `report.md` is its human-readable rendering. Accepted
R1/R2 artifacts, the evidence packet, and the Counterpart Arbiter verdict are bound by an R3
input receipt before the Primary Arbiter sees the challenge. The final manifest also binds
`result.json` by SHA-256. Files are created as the run reaches each phase, so
failed or waiting runs are expected to have only a prefix of this layout.

Briefs may use `snapshot_ignore` to omit root-relative evidence paths with
portable `/`-separated glob patterns. For a single-file evidence root, its
filename is the relative path. A matched directory is pruned with all of its
contents; for example, `[".firecrawl", "tools/r4se-packages"]` omits both
trees. Built-in exclusions for credentials and common generated trees remain
in force.

Production v2 adapters receive validated attachments through their native
read-only input mechanisms. PNG, JPEG, WebP, and GIF are recognized from bytes,
copied into `input/attachments`, hashed in `snapshot-manifest.json`, and exposed
as exact `attachment://` references. The in-process DeepSeek adapter carries
them as base64 image parts in the chat-completions request. The accepted file
list and local carrier do not constitute a live provider capability probe.
Adapters start from a fresh per-attempt HOME/config tree; selected provider
state is constructed from the allowlisted environment pair and no host profile
is copied.

## Isolation and Authorization

The runtime uses per-lane working directories, isolated HOME/config directories,
cleared environments, adapter tool restrictions, output schemas, and process
tree supervision. These are process and configuration controls. **They are not
a kernel-enforced filesystem or network sandbox.** A child executable still
runs with the operating-system authority of the user who started QUINTE.

Do not treat process isolation as a containment boundary for an untrusted executable. Run
the CLI under an external sandbox, VM, container, or restricted OS account when
that threat model applies.

A QUINTE result is evidence, not authorization. It cannot authorize a push,
deletion, external write, or other protected action. The host and user retain
that authority.

## Repository Contracts

- [Protocol redesign record](specs/PROTOCOL-REDESIGN.md) — the from-scratch
  re-architecture: adaptive rounds, A2A seats, doctrine-pack domains.
- [A2A endpoint contract](specs/HOST.md) — the A2A v1.0 host surface
  (`quinte host serve`).
- [Legacy CLI host contract](specs/HOST-CLI-LEGACY.md) — the in-service host
  surface of the 0.2.x runtime.
- [Protocol specification](specs/PROTOCOL.md) defines the runtime invariants
  of the legacy 0.2.x implementation.
- [CLI specification](specs/CLI.md) defines the executable boundary.
- [Windows PowerShell development log](docs/windows-powershell-development-log.md)
  records the native process-launch design and regression boundary.
- [JSON schemas](schemas/) define accepted brief, lane, primary-arbiter response, result,
  and compatibility artifacts.
- [QUINTE skill](skills/SKILL.md) is a thin host entry point to the CLI.

## License

Apache-2.0. Host-bound tools and model services retain their own licenses and terms.
