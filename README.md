<div align="center">

<img src="assets/quinte-cover.svg" alt="QUINTE" width="100%">

# QUINTE

**A protocol-enforcing CLI for five-party adversarial review**

[![Protocol](https://img.shields.io/badge/protocol-current-blue?style=flat)](specs/PROTOCOL.md)
[![CLI](https://img.shields.io/badge/CLI-v0.1-orange?style=flat)](specs/CLI.md)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

QUINTE exposes disagreements, omissions, evidence gaps, and unresolved risk
before a host adopts a conclusion. It is not a generic agent delegator or an
answer-voting system.

The protocol has three rounds:

- **R1:** Party A-E produce independent analyses.
- **R2:** the same five parties cross-review anonymized R1 outputs.
- **R3:** Hermes `hm` and an independent Auditor B produce the dual verdict.

The v0.1 Rust CLI owns the run state machine, fixed roster, typed output gates,
retry boundary, artifacts, and Hermes handshake. Hermes invokes the CLI; it
does not recreate QUINTE by launching the parties itself.

## v0.1 Boundary

The default policy binds the protocol roles to these native routes:

| Role | Route | Rounds |
| --- | --- | --- |
| Party A | CodeWhale | R1, R2 |
| Party B | OpenCode | R1, R2 |
| Party C | Kilo | R1, R2 |
| Party D | MiMo | R1, R2 |
| Party E | OMP | R1, R2 |
| Auditor B | Claude Code | R3 only |

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
candidate. Backoff is bounded and deterministically jittered, and persisted
cooldowns prevent `resume` from bypassing a wait.

Untrusted output text never controls retry behavior. Outside those exact
terminal signals, invalid UTF-8, JSON, or schema output is non-retryable; a
model merely mentioning `429`, timeout, or repetition is ordinary review
content. Output captured at a host timeout is accepted only when it is a
complete, strict LaneOutput whose `evidence_refs` and `closure_evidence` entries
are empty or exactly match snapshot refs in the run's snapshot manifest.

## Quick Start

Install a checksum-verified release binary on macOS or Linux without Rust:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/eric-stone-plus/QUINTE/main/install.sh | sh
```

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/eric-stone-plus/QUINTE/main/install.ps1 | iex
```

The installer verifies the selected release archive against `SHA256SUMS`, puts
the single binary on the user `PATH`, and initializes `~/.quinte` on first
install. It does not require Cargo, Rust, a source checkout, a daemon, or a
container.

The CLI executable is self-contained, but a complete run intentionally depends
on its fixed native agent roster and existing token-plan credentials. Run
`quinte doctor` to check CodeWhale, OpenCode, Kilo, MiMo, OMP, Claude Code, and
their credential sources. The installer never downloads those tools or creates
credentials silently.

Source builds remain available to contributors with `cargo build --release`.

Create a brief such as `brief.json`:

```json
{
  "brief_version": "1.0",
  "question": "Which material risks remain in this change?",
  "context": "Review the implementation, tests, and operational boundary.",
  "evidence_roots": ["/absolute/path/to/project"],
  "attachments": [],
  "action_scope": "decision support for this change only"
}
```

Start the run with machine-readable output:

```bash
quinte run --brief brief.json --json
```

The default command returns immediately with a queued run while a supervised
background worker advances R1, R2, and Auditor B:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"run_id":"...","status":"queued","run_dir":"..."}}
```

Use `--wait` to keep the initiating terminal attached to state observation
(not to the worker itself). It normally returns when Hermes input is required:

```json
{"cli_envelope_version":"1.0","ok":true,"data":{"run_id":"...","status":"waiting_hm","run_dir":"..."}}
```

`waiting_hm` with exit code `0` is a successful handoff, not a completed
verdict. Hermes must read the bound request and evidence, submit its response,
and then inspect the result:

```bash
quinte hm request RUN_ID --json
quinte hm submit RUN_ID --verdict hm-verdict.json --json
quinte inspect RUN_ID --json
```

`quinte wait RUN_ID` observes the same boundary. Ctrl-C interrupts only the
wait and leaves the background run active.

See [CLI.md](specs/CLI.md) for the complete command contract, Hermes response
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
R1/R2 artifacts, the evidence packet, and the CC verdict are bound by an R3
input receipt before Hermes sees the challenge. The final manifest also binds
`result.json` by SHA-256. Files are created as the run reaches each phase, so
failed or waiting runs are expected to have only a prefix of this layout.

OpenCode, Kilo, and MiMo receive validated images with native `--file`
arguments, OMP receives staged `@file` inputs, and CodeWhale/Claude receive
their native read-only image path forms. OMP runs from a WAL-consistent,
per-attempt SQLite credential snapshot; copied credentials are removed after
each attempt.

## Isolation and Authorization

v0.1 uses per-lane working directories, isolated HOME/config directories,
cleared environments, adapter tool restrictions, output schemas, and process
tree supervision. These are process and configuration controls. **They are not
a kernel-enforced filesystem or network sandbox.** A child executable still
runs with the operating-system authority of the user who started QUINTE.

Do not treat v0.1 as a containment boundary for an untrusted executable. Run
the CLI under an external sandbox, VM, container, or restricted OS account when
that threat model applies.

A QUINTE result is evidence, not authorization. It cannot authorize a push,
deletion, external write, or other protected action. The host and user retain
that authority.

## Repository Contracts

- [Protocol specification](specs/PROTOCOL.md) defines the debate invariants.
- [CLI specification](specs/CLI.md) defines the v0.1 executable boundary.
- [Dispatch specification](specs/DISPATCH.md) documents the earlier phase-only
  manifest and ledger compatibility layer.
- [Windows PowerShell development log](docs/windows-powershell-development-log.md)
  records the native process-launch design and regression boundary.
- [JSON schemas](schemas/) define accepted brief, lane, hm response, result,
  and compatibility artifacts.
- [QUINTE skill](skills/SKILL.md) is a thin Hermes entry point to the CLI.

The scripts under `bin/` remain phase-dispatch compatibility tools. They are
not the normal full-run Hermes interface and do not replace the Rust state
machine.

## License

MIT. Host-bound tools and model services retain their own licenses and terms.
