---
name: quinte
description: "QUINTE protocol - R1/R2 five host-bound parties; R3 is hm + independent Auditor B."
spec: "https://github.com/eric-stone-plus/QUINTE/blob/main/specs/PROTOCOL.md"
triggers:
  - "quinte"
---

# QUINTE

## Boundary

QUINTE owns adversarial debate. It defines the participant set, evidence
requirements, cross-review, and dual-verdict contract. Implementation support,
host routing, and non-QUINTE audit paths are outside this project.

Concrete CLI routing belongs to the host runtime configuration.

QUINTE is an adversarial residual-exposure protocol, not a mixture-of-agents
answer aggregator. Its output should preserve contradictions, omissions,
evidence gaps, confidence mismatches, drift, execution mismatches, and material
dissent as inspectable artifacts.

## Round Contract

R1 uses Party A-E for independent analysis.
R2 uses the same Party A-E for cross-examination of R1.
R3 uses hm plus the independent Auditor B for dual verdict.

R1 and R2 always use the same five host-bound debate parties. hm does not
participate in R1/R2. Auditor B does not participate in R1/R2.

## Host Binding

The host runtime binds Party A-E to concrete native CLI routes. This repository
does not publish tool names, model names, provider settings, credentials, or
local paths.

Auditor B is host-selected and enters only at R3.

Party failure must be classified and recovered inside the same host route.
Retry with repaired auth/cwd/prompt/network as appropriate, but do not replace
one R1/R2 party with another party, Auditor B, or hm. If a route remains
unavailable after recovery, mark the debate degraded and abort/escalate rather
than forging a five-party verdict.

## Protocol

### R1 - Independent Opinions

All five debate parties dispatch simultaneously. Each output starts with
`TASK: [restatement]`. Never reduce the initial R1 party set.

### R2 - Cross-Examination

All five debate parties review the R1 outputs using anonymous labels:
Participant A/B/C/D/E. Agents flag position changes in this form:
`CHANGED: [old position] BECAUSE [evidence from Participant X]`.

R2 is mandatory even with R1 consensus.

Every material dispute should receive a residual disposition:
`verified`, `falsified`, `unresolved`, `escalated`, or `discarded`.

### R3 - Dual Verdict

R3 = hm + independent Auditor B. Auditor B is host-selected and dispatched
through the host routing layer. Auditor B reviews all R1/R2 evidence and draft
verdict context.

Auditor B checklist:
- Read all R1 and R2 evidence before drafting.
- Verify cited source files or command evidence directly.
- Resolve hard contradictions with file:line or command-output references.
- Annotate material dissent; do not suppress it.
- Preserve unresolved residuals instead of smoothing them into consensus.
- Do not introduce new R1/R2 parties or read unrelated historical audits.
- Emit a residual trace with ids, severity, source, evidence, disposition,
  closure state, closure evidence, and scope for action-blocking findings.
- Include a trial manifest with party artifacts, prompt hashes, perturbation
  axes, independence controls, contamination risks, and cost when action
  boundaries are involved.
- Make the residual trace compatible with RASHOMON
  `schemas/residual-trace.schema.json`.
- Include enough evidence and scope for host-side residual quality metrics;
  do not self-certify the trace as high quality.

### Hard Cap

Three rounds mandatory. R3 must produce a residual trace. It may
preserve unresolved residuals; unresolved high-risk residuals become HIGHBALL
action-gate inputs, not forced consensus. Recursive QUINTE for sub-issues is
allowed, but nested R1/R2 still use QUINTE parties only.

## Dispatch Contract

Every QUINTE process must be an independent background terminal process with a
real CLI executable. No merged parties, no hidden aggregate process, no wrapper
scripts, and no `delegate_task` in the QUINTE domain.

Use `bin/quinte-dispatch-phase.py` when a host-supplied dispatch manifest is
available. It preflights the five Party A-E bindings, dispatches each route,
classifies failures, retries only the same route, and writes a dispatch ledger.
If a required route remains blocked or degraded, the ledger sets
`phase_progression_allowed` to false and the next phase must not start.
For R3, the manifest may bind only Auditor B because Party A-E are not
dispatched in that phase. Relative command paths resolve from the manifest
directory; output artifacts are still written under the run output directory.
Successful process exit is not enough: stdout must begin with `TASK:`. Startup
banner-only or TUI-only output is `invalid_output`, which is retried only within
the same route and blocks progression if unresolved.
Unexpected dispatcher exceptions are recorded as degraded party evidence with
stderr references, preserving the same route identity and blocking phase
progression instead of producing an unledgered crash.

```bash
python3 bin/quinte-dispatch-phase.py "$AUDIT/dispatch-manifest.json" --pretty
```

Use file-based prompts for long tasks. Create the run output directory before
redirects. Capture output as an artifact in the ledger. Concrete commands still
come from the host routing configuration.

## Workflow

For protected analysis that may affect a pushed change, run QUINTE through R1,
R2, and R3 before treating the result as passed. A passed result only permits
proposing a push when the residual trace shows action-blocking findings are
closed, blocked, waived, or not applicable. The actual push still requires
current-session user authorization.

HIGHBALL residual routing decides when QUINTE is required. QUINTE supplies the
adversarial residual trace after it has been selected; it does not self-select
as the route.
If HIGHBALL builds an Action Packet, QUINTE supplies the residual trace and,
when dispatched through the manifest layer, the R1, R2, and R3 dispatch ledgers
as execution evidence. HIGHBALL owns route decision, validation, quality
measurement, execution-evidence enforcement, and boundary decision.
If later evidence confirms or contradicts the QUINTE trace, record that in a
HIGHBALL outcome ledger. Do not rewrite R3 or self-certify downstream success.

Pull and fetch operations do not require QUINTE.

## Key Rules

- Never self-judge simplicity. All five parties every round.
- No R1/R2 substitutions. Same-route recovery only; unresolved route failure
  means degraded/incomplete QUINTE.
- `git add` explicitly, never `git add -A`.
- Push requires explicit current-session user authorization.
- Treat earlier outputs as adoption evidence only; do not
  rewrite old verdicts solely to improve residual-trace metrics.
- Prompt drift: fix the prompt and re-dispatch the same QUINTE party; do not change the R1/R2 party set.
- Clean up residual party processes only after all outputs are read and captured.
- Stale dispatch references are blockers. If a skill or reference maps a
  QUINTE alias differently from the host routing configuration, or uses wrapper
  scripts, update it before dispatch.
