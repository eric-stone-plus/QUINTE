<div align="center">

<img src="assets/quinte-cover.svg" alt="QUINTE" width="100%">

# QUINTE (クインテ)

**Five-party structured residual-exposure protocol**

R1/R2 parties are host-bound debate roles. Concrete tools are selected by the
host runtime, not by this repository.

[![Protocol](https://img.shields.io/badge/protocol-current-blue?style=flat)](specs/PROTOCOL.md)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

## What is QUINTE?

QUINTE is a protocol for structured debate under controlled roles. It defines:

- **5 host-bound parties in R1/R2**
- **3 rounds**: Independent Analysis, Cross-Review, and Dual Verdict with Auditor B
- **Invariants** (no degradation, adversarial review, mandatory cross-examination)

> 📖 **Read the spec**: [specs/PROTOCOL.md](specs/PROTOCOL.md)

QUINTE is not a mixture-of-agents answer aggregator. Its primary job is
adversarial residual exposure: force independent testimony, make disagreements
and evidence gaps visible, verify what can be verified, and preserve material
dissent before any action boundary is crossed.

Its action artifact is a residual trace, not a polished answer. R3 must preserve
the residual id, source, severity, evidence, disposition, closure state, and
scope for every action-blocking finding so HIGHBALL can decide whether a
protected action is allowed.
The trace must follow RASHOMON `schemas/residual-trace.schema.json`.
R3 should include a `trial_manifest` describing Party A-E artifacts, prompt
hashes, perturbation axes, independence controls, contamination risks, and cost
so same-model QUINTE is evaluated as controlled behavioral perturbation rather
than independent confirmation.

### Cultural Note

QUINTE is named from the Roman Republic metaphor: independent parties,
cross-examination, and a dual verdict. This is naming context only. It does not
define routing, authorization, or fallback behavior. See [RASHOMON](../RASHOMON)
for broader philosophy.


## Round Contract

**R1 — 5 Parties**: five host-bound debate roles. Independent analysis,
parallel dispatch.

**R2 — 5 Parties**: the same five host-bound debate roles. Cross-examination
with anonymous review.

**R3 — Dual Verdict**: hm + Auditor B. Auditor B is an independent arbiter that does not participate in R1/R2. Consensus is adopted and material dissent is annotated.

### R3: Dual Verdict

At R3, every verdict is drafted by two arbiters in parallel:

- **hm** — primary arbiter with full session context
- **Auditor B** — independent second arbiter, reviews all R1+R2 evidence

The two drafts are merged: consensus is adopted, disagreement is surfaced as an annotated dissent. The lead arbiter may not suppress the auditor's dissent.

R3 also emits the residual trace consumed by HIGHBALL. A verdict without that
trace may be useful context, but it is not protected-write evidence.
Dispatch ledgers are separate execution-completeness evidence. For protected
or irreversible boundaries, HIGHBALL may require complete R1, R2, and R3
dispatch ledgers in the Action Packet before a QUINTE trace can be used as
boundary evidence.
Earlier outputs may predate this contract. They should be evaluated for
adoption, not rewritten merely to improve metrics.
Follow-up outcomes are host artifacts. If later command, runtime, source,
human-review, or external evidence confirms or contradicts a QUINTE trace, the
host records that in a HIGHBALL outcome ledger. QUINTE must not self-certify its
own downstream success.

### Authorization

Operations are gated by the host runtime. No irreversible external write proceeds without explicit user authorization.



## Implementation

QUINTE runs on a protocol-enforcing agent runtime. It is not implementation-agnostic; it depends on runtime-specific primitives for debate-party dispatch, evidence capture, session persistence, and protected writes. R1/R2 dispatch must use real CLI routes supplied by the host runtime.

Project references:

- [specs/PROTOCOL.md](specs/PROTOCOL.md) is the canonical protocol specification.
- [specs/DISPATCH.md](specs/DISPATCH.md) defines the host-bound dispatch ledger.
- [skills/](skills/) contains the QUINTE protocol skill.

Current invocation routes are owned by the host runtime. Historical invocation
experiments are not dispatch authority.

The repository includes a lightweight dispatch reliability layer:

- `bin/quinte-dispatch-phase.py` runs one phase from a host-supplied manifest.
- `bin/validate-dispatch-manifest.py` checks the five-party R1/R2 binding and the R3 Auditor B route.
- `bin/validate-dispatch-ledger.py` checks the generated attempt ledger and blocks phase progression when a required route remains unresolved.

This layer does not select providers or substitute failed parties. It only
executes the host's manifest, classifies failures, retries the same route where
allowed, and records an auditable ledger.
R3 dispatch may bind only Auditor B, because R1/R2 parties are not dispatched
in that phase. Relative executable paths are resolved from the manifest
directory before the child process runs in the run output directory.
Output must begin with `TASK:`. A route that exits successfully but only writes
a startup banner, TUI text, or another non-answer is classified as
`invalid_output`, retried on the same route, and blocks phase progression if it
remains unresolved.
Unexpected dispatcher-side exceptions are recorded as degraded party ledgers
with stderr evidence, so a QUINTE run should fail closed with an auditable
ledger instead of disappearing as an unclassified orchestration crash.

## Host Binding

QUINTE does not publish concrete tool, model, provider, credential, or local
path bindings. A host runtime must bind the five R1/R2 parties and the R3
Auditor B route before a full run can begin.

Host binding requirements:

- R1/R2 parties: five independent native CLI routes with separate output artifacts.
- Auditor B: one host-selected independent R3 route.
- Evidence capture: file, command, runtime, or source references preserved in run artifacts.
- Protected action: authorized by the host operation layer, not by QUINTE itself.

## License

MIT — the protocol and orchestration layer. Host-bound tools carry their own
licenses.
