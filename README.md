<div align="center">

<img src="assets/quinte-cover.svg" alt="QUINTE" width="100%">

# QUINTE

**A single-model-family, multi-path review runtime with contract gates**

[![Protocol](https://img.shields.io/badge/protocol-current-blue?style=flat)](specs/PROTOCOL.md)
[![CLI](https://img.shields.io/badge/CLI-contract-orange?style=flat)](specs/CLI.md)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

QUINTE runs a fixed engineering review pipeline that detects conflicting
findings, omissions, evidence gaps, and unresolved risk before a host adopts a
conclusion. It is not a generic model delegator or an answer-voting system.

The runtime has three stages:

- **R1:** five fixed paths (`Party A`-`Party E` in the wire contract) produce
  typed first-pass outputs.
- **R2:** the same five paths check an anonymized packet of accepted R1 outputs.
- **R3:** two fixed verdict bindings (`Counterpart Arbiter` and `Primary
  Arbiter`) produce typed inputs for deterministic merge.

The Rust CLI owns the run state machine, fixed roster, typed output gates,
retry boundary, artifacts, and Primary Arbiter handshake. The host invokes the CLI; it
does not recreate QUINTE by launching the parties itself.

## Engineering Model

QUINTE is a **single-model-family, multi-path, three-stage review runtime with
seven execution bindings and contract gates**:

- **Single model family:** all seven bindings share the same family, provider,
  text model, and multimodal model within one run.
- **Multiple paths:** five R1/R2 bindings apply fixed path-specific review
  instructions to the same bounded evidence snapshot.
- **Three stages:** R1 produces first-pass artifacts, R2 checks the accepted R1
  packet, and R3 produces two typed verdict inputs for merge.
- **Seven execution bindings:** five R1/R2 routes and two R3 routes are fixed by
  policy before dispatch.
- **Contract gates:** the scheduler alone owns phase transitions, closed-schema
  validation, retries, receipts, and SHA-256 artifact bindings.

`Party A`-`Party E`, `Counterpart Arbiter`, and `Primary Arbiter` are protocol
wire-role identifiers. They name execution slots and artifact ownership; they
do not denote personas, role-playing, or autonomous control of the scheduler.
Likewise, result fields such as `perspectives` and `dissent` are serialized
compatibility names, not behavioral concepts.

QUINTE preserves the applicable result field names for compatibility with the
RASHOMON Trace 1.1 data contract. That compatibility is limited to the data
shape; QUINTE has no runtime dependency on RASHOMON.

## Runtime Boundary

Policy v2 binds all seven roles to one seat identity. The four binding axes
(`family`, `provider`, `text_model`, `multimodal_model`) must match exactly:

| Seat family | Adapter for all seven execution bindings | Image carrier | Requirement |
| --- | --- | --- | --- |
| MiMo | MiMoCode | repeated `--file` | isolated config/auth from selected Xiaomi key and base URL |
| DeepSeek | Reasonix | none | stateless provider config from selected DeepSeek key and base URL |
| OpenAI | Codex | repeated `--image` | relay must implement the Responses API image input |

The five R1/R2 bindings use policy-defined path instructions; they are not
different model families. Both R3 bindings use the same seat family too.
Cross-family comparison belongs to an outer orchestrator such as MAGI, not one
QUINTE run.
Legacy policy v1 remains readable without rewrite, but its historical native
harness roster is a compatibility surface, not the production v2 default. It
cannot start a new run; back up the file and run `quinte init --force` to
install a production v2 policy.

There is deliberately no command for running one party, skipping R2, replacing
a failed party, or asking a model to advance the state machine. A required lane
must produce closed-schema JSON on its assigned route or the phase does not
pass.

R2 is scheduler-serialized and paced: the default policy leaves at least ten
seconds between R2 transport starts. Trusted retry signals stay on the same
route and use a bounded attempt budget: host-observed timeouts, exact
rate-limit errors, MiMo's structured repetition-detector terminal error, and a
CodeWhale stream that reports both `completed` and `done` but contains no JSON
candidate or only a truncated final candidate. Backoff is bounded and
deterministically jittered, and persisted cooldowns prevent `resume` from
bypassing a wait.

Untrusted output text never controls retry behavior. Outside those exact
terminal signals, invalid UTF-8, JSON, or schema output is non-retryable; a
model merely mentioning `429`, timeout, or repetition is ordinary review
content. Output captured at a host timeout is accepted only when it is a
complete, strict LaneOutput whose `evidence_refs` and `closure_evidence` entries
are empty or exactly match snapshot refs in the run's snapshot manifest.

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

# 2) Host progress helpers (required for interactive agents / Hermes skill)
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

### Host agent skill (Hermes and similar)

The durable skill lives in this repo at [`skills/SKILL.md`](skills/SKILL.md).
Copy or symlink it into the host’s live skill directory after install or pull
(for example Hermes technical profile
`…/skills/multi-agent-debate/quinte/SKILL.md`). Host trees under `~/.hermes`
are not version-controlled; **the repo file is the source of truth**.

External agents must use the stable `quinte host` surface. It owns the global
launch lock, fail-closed one-active check, detached start receipt, one-shot
status, terminal integrity verification, and ambiguous-launch reconciliation.
`quinte-progress`/`quinte-run` remain human display helpers, not machine APIs.

### Credentials and roster

The CLI is self-contained, but a production policy needs the matching adapter
and exactly one selected provider key/base URL pair. Set
`QUINTE_PROVIDER_KEY_ENV` and `QUINTE_PROVIDER_BASE_URL_ENV` to the appropriate
allowlisted names (`XIAOMI_*`, `DEEPSEEK_*`, or `OPENAI_*`). HTTPS is required;
whitespace and `.invalid` placeholders fail closed. Provider traffic inherits
the host proxy environment by default. Set `QUINTE_PROVIDER_PROXY_MODE=direct`
only when the selected provider endpoint is explicitly intended and verified
to bypass that proxy; `direct` adds only that endpoint host to both `NO_PROXY`
casings. Unknown modes fail closed. `quinte doctor --json`
checks all seven bindings before dispatch. Each route row also reports its
static attachment carrier, accepted media types, and `provider_live_probe:
false`; `doctor` verifies local routing/configuration, not a live multimodal
request to the provider.

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
as exact `attachment://` references. MiMo and Codex have native carriers;
Reasonix currently does not, so a DeepSeek/Reasonix run with attachments fails
before a run directory is created. The accepted file list and local carrier do
not constitute a live provider capability probe. Adapters start from a fresh
per-attempt HOME/config tree; selected provider state is constructed from the
allowlisted environment pair and no host profile is copied.

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

- [Protocol specification](specs/PROTOCOL.md) defines the runtime invariants.
- [CLI specification](specs/CLI.md) defines the executable boundary.
- [Windows PowerShell development log](docs/windows-powershell-development-log.md)
  records the native process-launch design and regression boundary.
- [JSON schemas](schemas/) define accepted brief, lane, primary-arbiter response, result,
  and compatibility artifacts.
- [QUINTE skill](skills/SKILL.md) is a thin host entry point to the CLI.

## License

MIT. Host-bound tools and model services retain their own licenses and terms.
