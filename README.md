<div align="center">

<img src="assets/quinte-cover.svg" alt="QUINTE" width="100%">

# QUINTE

**A protocol-enforcing CLI for five-party adversarial review**

[![Protocol](https://img.shields.io/badge/protocol-current-blue?style=flat)](specs/PROTOCOL.md)
[![CLI](https://img.shields.io/badge/CLI-contract-orange?style=flat)](specs/CLI.md)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

QUINTE exposes disagreements, omissions, evidence gaps, and unresolved risk
before a host adopts a conclusion. It is not a generic agent delegator or an
answer-voting system.

The protocol has three rounds:

- **R1:** Party A-E produce independent analyses.
- **R2:** the same five parties cross-review anonymized R1 outputs.
- **R3:** the Primary Arbiter and the Counterpart Arbiter produce the dual verdict.

The Rust CLI owns the run state machine, fixed roster, typed output gates,
retry boundary, artifacts, and Primary Arbiter handshake. The host invokes the CLI; it
does not recreate QUINTE by launching the parties itself.

## Design Vocabulary

QUINTE names its two layers deliberately:

- **Scenario-oriented prompt layer (面向情景).** The seven protocol roles are
  scripted scenario positions, not objects with encapsulated behavior. A role
  is a constructed situation — brief, evidence packet, output schema, and
  constraints — inside which a model improvises an analysis. The protocol does
  not program behavior; it stages situations and gates what returns.
- **Contract-oriented orchestration layer.** Everything outside the scenario —
  roster, rounds, state machine, typed output gates, retry boundary, and
  SHA-256 artifact bindings — is deterministic and closed-schema. Scenario
  text never advances the state machine; only typed artifacts do.

Object-oriented programming encapsulated behavior in objects. QUINTE induces
behavior in scenarios and arbitrates it with contracts.

## Runtime Boundary

The default policy binds the protocol roles to these native routes:

| Role | Route | Rounds |
| --- | --- | --- |
| Party A | CodeWhale | R1, R2 |
| Party B | OpenCode | R1, R2 |
| Party C | KiloCode| R1, R2 |
| Party D | MiMoCode| R1, R2 |
| Party E | Oh-My-Pi | R1, R2 |
| Primary Arbiter | Hermes | R3 only |
| Counterpart Arbiter | ClaudeCode | R3 only |

R1 and R2 use `mimo-v2.5-pro` for text-only briefs. A supported image
attachment selects `mimo-v2.5` for the run. These are same-family behavioral
perspectives, not independent model confirmation.

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

Build from source (prebuilt GitHub Releases are not published). Source on
`main` is the only install channel, so a rebuild is how you stay current with
upstream.

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

Interactive hosts must not use `quinte run --wait` or bare `quinte wait`. Use
detached `quinte run --brief … --json` and poll `quinte-progress`, or stream
with `quinte-run --brief …`. Keep **one active run** at a time.

### Credentials and roster

The CLI is self-contained, but a complete run depends on the fixed native agent
roster and existing token-plan credentials. `quinte doctor` checks CodeWhale,
OpenCode, Kilo, MiMo, OMP, Claude Code, and their credential sources.

Provision the Claude/MiMo token with Keychain Access on macOS or Windows
Credential Manager on Windows, then verify with `quinte credential status`.
QUINTE exposes no secret-writing command; see the CLI contract for the exact
account/service or target identity.

Create a brief such as `brief.json`:

```json
{
  "brief_version": "1.0",
  "question": "Which material risks remain in this change?",
  "context": "Review the implementation, tests, and operational boundary.",
  "evidence_roots": ["/absolute/path/to/project"],
  "snapshot_ignore": [".git", "build/**", "**/*.key"],
  "attachments": [],
  "action_scope": "decision support for this change only"
}
```

Start the run with machine-readable output:

```bash
quinte run --brief brief.json --json
```

The default command returns immediately with a queued run while a supervised
background worker advances R1, R2, and the Counterpart Arbiter:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"run_id":"...","status":"queued","run_dir":"..."}}
```

Use `--wait` to keep the initiating terminal attached to state observation
(not to the worker itself). It normally returns when the Primary Arbiter input is required:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"run_id":"...","status":"waiting_primary_arbiter","run_dir":"..."}}
```

`waiting_primary_arbiter` with exit code `0` is a successful handoff, not a completed
verdict. The Primary Arbiter must read the bound request and evidence, submit its response,
and then inspect the result:

```bash
quinte primary-arbiter request RUN_ID --json
quinte primary-arbiter submit RUN_ID --verdict primary-arbiter-verdict.json --json
quinte inspect RUN_ID --json
```

`quinte wait RUN_ID` observes the same boundary. Ctrl-C interrupts only the
wait and leaves the background run active.

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

OpenCode, Kilo, and MiMo receive validated images with native `--file`
arguments, OMP receives staged `@file` inputs, and CodeWhale/Claude receive
their native read-only image path forms. OMP runs from a WAL-consistent,
per-attempt SQLite credential snapshot; copied credentials are removed after
each attempt.

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

- [Protocol specification](specs/PROTOCOL.md) defines the debate invariants.
- [CLI specification](specs/CLI.md) defines the executable boundary.
- [Windows PowerShell development log](docs/windows-powershell-development-log.md)
  records the native process-launch design and regression boundary.
- [JSON schemas](schemas/) define accepted brief, lane, primary-arbiter response, result,
  and compatibility artifacts.
- [QUINTE skill](skills/SKILL.md) is a thin host entry point to the CLI.

## License

MIT. Host-bound tools and model services retain their own licenses and terms.
