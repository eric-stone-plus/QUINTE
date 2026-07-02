# QUINTE Protocol Specification · Hermes Agent

> **Canonical protocol definition for the Hermes Agent.**
>
> **Scope**: QUINTE is a Hermes Agent protocol. It depends on Hermes-specific primitives: terminal/background dispatch for real CLI parties, memory+skill for cross-session invariants, cron for autonomous triggering, and terminal evidence for cross-match. `delegate_task` is explicitly forbidden for QUINTE party dispatch because it hides the real CLI route, process boundary, and output artifact required for audit. Provider/model identity is troubleshooting evidence only, not routing authority.

## 1. Runtime Contract

QUINTE is a five-party structured debate protocol for adversarial residual
exposure. Its primary artifact is an adversarial residual set: contradictions,
omissions, evidence gaps, confidence mismatches, prompt drift, mind-changes,
execution mismatches, and suspicious low-evidence convergence exposed by
structured cross-examination.

QUINTE is not a mixture-of-agents answer aggregator. It may improve a final
answer, but that is secondary. The protocol exists to make errors inspectable
before the host adopts a claim or crosses an action boundary.

When the host binds several parties to the same base model, QUINTE is best
understood as controlled inference-time perturbation. Different roles, prompt
positions, evidence budgets, first-pass isolation, and reviewer duties explore
the model's behavioral surface. This can expose instability, omissions, and
unsupported convergence, but it is not independent truth confirmation. The
resulting evidence is a trace that must be calibrated against self-correction,
direct verification, human review, and later outcomes.

When QUINTE is evaluated as a future default route, that evaluation belongs in
HIGHBALL route experiment artifacts: a pre-run manifest and a post-run review
against calibration, baseline, and outcome evidence. QUINTE produces the
adversarial residual trace; it does not certify that its own route earned
policy status.

QUINTE operates through separated roles: the host coordinates execution through
terminal/background dispatch; five host-bound parties participate in R1/R2; the
host holds phase-block authority; and R3 uses a host-bound independent Auditor B
route.

### 1.1 Participants

Party A participates in R1 and R2 through an independent native route with its
own output artifact.
Party B participates in R1 and R2 through an independent native route with its
own output artifact.
Party C participates in R1 and R2 through an independent native route with its
own output artifact.
Party D participates in R1 and R2 through an independent native route with its
own output artifact.
Party E participates in R1 and R2 through an independent native route with its
own output artifact.

**R1**: 5 host-bound parties. Each party produces independent output.
**R2**: the same 5 host-bound parties. Cross-examination of all R1 claims.

**R2 Anonymous Review**: R2 parties are identified as "Participant A/B/C/D/E"
-- no host route names. Pseudonyms prevent route-bias in cross-examination. R3
reveals the mapping for the final verdict.

**R2 Mind-Change Tracking**: R2 prompt embeds agent's own R1 summary (<=300 chars). Agents MUST flag any position change caused by reviewing others' R1, using the format: "CHANGED: [old position] BECAUSE [evidence from agent X]". R3 weighs mind-changed views higher (persuasion = evidence strength).

### 1.2 Dispatch Mechanism

QUINTE dispatch is host-side native CLI orchestration. The current alias-to-CLI
mapping is owned by the host routing configuration; this protocol only defines
which parties must participate and what evidence they must produce.

Layer responsibilities:

- QUINTE defines the R1/R2/R3 phase contract, evidence requirements, and verdict structure.
- Host routing supplies verified native CLI commands for Party A-E and the host-selected Auditor B.
- Host residual routing, typically HIGHBALL, decides when QUINTE is required.
- Host authorization owns authorization for push, delete, reset, and config writes.
- Host guard owns runtime guardrails, protected-file checks, and stale-process cleanup.

Do not use merged-agent APIs, `delegate_task`, wrapper scripts, or hidden
aggregate processes for R1/R2 parties.

### 1.3 Orchestration vs Oversight Separation

`bin/quinte-dispatch-phase.py` is the repository's reference dispatch
reliability layer. It consumes a host-supplied manifest, executes the required
native routes, classifies failures, retries only the same route where recovery
is allowed, and writes a dispatch ledger. A ledger with
`phase_progression_allowed: false` blocks the next phase.

Hermes coordinates CLI processes and performs phase-level oversight. After each
phase, hm reviews outputs, party status, and evidence quality, then either
continues, retries a failed same-layer party, escalates, or aborts. hm does not
participate as an R1/R2 debate party.

## 2. Phase Structure

### Phase -1: Five Gates (hm, parallel)
Five gates executed by hm in parallel (~5s). See §6 for gate definitions.

### Phase 0: Agent Manifest
hm reads host routing and records the mandatory Party A-E manifest. The
manifest is fixed for R1/R2. hm, provider or model names, and the R3 auditor
are not R1/R2 substitutes.

### Phase 1: Round 1 — Independent Analysis

All five R1 parties receive the same question in parallel. Each party produces
independent output. Outputs MUST begin with `TASK: [restatement]` —
outputs with a non-matching first line are discarded.

**JSON Sidecar**: Each R1 party appends a structured JSON block after the markdown body:
```json
{
  "verdict": "string (primary conclusion)",
  "confidence": "number 0.0-1.0",
  "reasoning_chain": ["string (key reasoning steps)"],
  "evidence_citations": ["file:line or command output reference"],
  "residual_candidates": [{
    "id": "R1-A-001",
    "severity": "HIGH",
    "type": "evidence_gap",
    "source": "Party A round 1, file, command, or source",
    "finding": "string",
    "evidence": "file:line, command output, or null",
    "disposition": "unresolved"
  }]
}
```
Markdown remains the primary output (human-readable, R2 cross-review). JSON is the machine-readable sidecar consumed by Phase 2 auto-diff.

**Evidence Validation Gate**: Before Phase 2 consumes JSON confidence scores, hm MUST verify that all `evidence_citations` resolve to real `file:line` locations or reproducible command output. Unresolved citations are tagged `[CITATION_UNVERIFIED]` and the claim confidence is down-weighted 0.5x. Fabricated citations are a free credibility injection channel; this gate closes it.

**R1 Schema (loose mode)**:
```json
{
  "task_restatement": "string (required)",
  "claims": [{
    "id": "string",
    "statement": "string",
    "evidence": "string (file:line or command output)",
    "confidence": "number 0.0-1.0",
    "category": "string"
  }],
  "freeFormInsights": "string (optional)",
  "uncertainties": ["string"],
  "coverage": {
    "files_checked": "number",
    "total_files": "number",
    "coverage_pct": "number"
  }
}
```

**Discipline**: R1 all launch in parallel. On timeout after 120 seconds, kill,
shrink the prompt, and retry. Three consecutive failures require escalation.
**Never skip a party**.

### Phase 2: Auto-Diff + Schema Convergence

Claims from all R1 outputs are diffed:
- Identical claims by statement hash enter the consensus pool.
- Conflicting claims enter the dispute pool.
- Core party claims tagged by host-bound Party A-E identifiers
- Residual candidates enter the residual pool with source, evidence, and
  provisional disposition.

**JSON sidecar auto-diff**: Phase 2 parses JSON sidecar blocks from each R1 output. Parsed JSON fields (`verdict`, `reasoning_chain`, `evidence_citations`, `residual_candidates`) are consumed for automated claim comparison. Parse failure falls back to markdown regex extraction. **Evidence validation gate runs here**: all `evidence_citations` MUST be verified against real `file:line` before confidence scores enter the diff. Unverified citations receive `[CITATION_UNVERIFIED]` and 0.5x weight.

hm reviews diff quality before approval.

### Phase 3: Round 2 — Adversarial Verification

All five R2 parties cross-review disputed R1 claims. Refutations must cite
specific R1 evidence or missing evidence. Vague refutations ("might be wrong")
do not count.

**R2 Anonymous Mode**: Agents are "Participant A/B/C/D/E" -- names hidden to prevent brand-bias. R3 reveals the mapping.

**R2 Mind-Change**: R2 prompt embeds the party's R1 summary. Parties flag position changes with cause + cited evidence. R3 weights persuaded views higher.

**Residual Requirement**: R2 outputs must classify each reviewed dispute as
`verified`, `falsified`, `unresolved`, `escalated`, or `discarded`. A
classification without evidence is treated as `unresolved`.

Each material R2 residual should preserve the R1 source, the R2 reviewer, the
evidence used for classification, and whether the residual affects a possible
action boundary. This information feeds the R3 residual trace.

hm reviews for false-positive refutations before approval.

### Phase 4: Structured Synthesis

hm summarizes consensus and contested claims from R1/R2 outputs. This is a
synthesis step, not a new debate party.

Synthesis must preserve residual provenance. It may not collapse disagreement
into a smooth final narrative unless the residual disposition is `verified` or
`falsified` with evidence.

hm reviews before approval.

### Phase 5: Convergence Check

hm checks for remaining blind spots. Hard cap: 3 rounds. Either condition
triggers **escalate** (mandatory human review, NOT auto-termination):
1. Two consecutive rounds with zero new claims
2. Round count = 3

**Truth verification**: claims about executable code must be validated against
actual runtime behavior.

### Phase 5a: Runtime Verification

When executable claims remain contested, the host-selected implementation or
runtime-verification route receives a bounded subset from Phase 3. It verifies
up to 5 high-impact claims via direct execution, LSP/DAP, tests, or equivalent
runtime evidence. Returns: `verified` / `falsified` / `inconclusive`. Output
feeds into Phase 5 convergence check.

### Phase 6: Round 3 - Dual Verdict

hm (primary arbiter) + independent Auditor B review all R1+R2 evidence and
draft parallel verdicts.

Auditor B is not an R1/R2 debate party. Auditor B enters only at R3, reads all
R1/R2 evidence, verifies citations directly where possible, and writes an
independent verdict draft to the run record.

**R2 Anonymity Reveal**: R3 reveals the mapping from Participant pseudonym to
agent name. Consensus is adopted; disagreement is surfaced as annotated dissent.
hm may not suppress auditor's dissent.

R3 final output must include a RASHOMON-compatible residual trace. HIGHBALL can
consume this trace before protected writes or other action boundaries:

```json
{
  "trace_version": "1.0",
  "question": "string",
  "instrument": "QUINTE",
  "residuals": [{
    "id": "RC-001",
    "severity": "HIGH",
    "type": "evidence_gap",
    "source": "round/participant/file/command",
    "finding": "string",
    "affected_paths": ["path or glob"],
    "error_signature": "literal string, regex, command, or null",
    "evidence": "file:line, command output, source, or null",
    "disposition": "unresolved",
    "required_closure": "human_review",
    "closure_state": "open",
    "closure_evidence": ["file:line, command output, source, waiver, or null"],
    "scope": "what action this closure covers"
  }],
  "trial_manifest": {
    "manifest_version": "1.0",
    "base_model_relation": "same_model",
    "perspective_count": 5,
    "perspectives": [{
      "id": "Party A",
      "role": "R1/R2 debate party",
      "route": "host-bound route or null",
      "artifact": "artifact path or null",
      "prompt_hash": "hash or null",
      "independent_first_pass": true
    }],
    "perturbation_axes": ["role", "reviewer_position", "evidence_budget"],
    "independence_controls": ["independent_first_pass", "anonymous_cross_review", "separate_output_artifacts"],
    "contamination_risks": ["same_model_error_correlation"],
    "cost": {
      "total_tokens": null,
      "wall_time_seconds": null,
      "tool_calls": null,
      "human_minutes": null
    }
  },
  "action_boundary": "protected_write",
  "highball_decision": "review"
}
```

The trace must validate against RASHOMON
`schemas/residual-trace.schema.json`. Hosts may add stricter action-boundary
policy, but QUINTE must not rename or omit schema fields in R3.
R3 should include enough evidence, closure evidence, scope, and residual type
detail for host-side residual quality metrics to be derived. QUINTE must not
self-certify trace quality; HIGHBALL or another consumer computes quality
metrics from the emitted trace.
R3 should also include `trial_manifest` so the host can inspect perspective
count, output artifacts, prompt hashes, perturbation axes, independence
controls, contamination risks, and cost. Same-model QUINTE is evidence of
behavioral stability or instability under the recorded perturbations, not
independent confirmation of truth.
This is why trial manifests record perturbation axes, independence controls,
contamination risks, and cost: without those fields, a same-model run is
indistinguishable from repeated sampling or ordinary self-refinement.
Earlier QUINTE outputs may lack this trace. Treat them as historical evidence
and scan them for adoption; do not rewrite old verdicts solely to make quality
metrics look better.
HIGHBALL residual routing determines when QUINTE is the required route. QUINTE
does not decide that its own protocol is required; it supplies the adversarial
trace after the host has selected it.
When HIGHBALL builds an Action Packet, QUINTE's R3 trace fills the packet's
trace field. QUINTE dispatch ledgers fill execution-evidence fields when the
run used the manifest dispatch layer. Neither the trace nor the ledgers replace
the route decision or authorization boundary.
If later evidence confirms, contradicts, or complicates the trace, that
follow-up belongs in a HIGHBALL outcome ledger. QUINTE does not rewrite R3 or
self-certify route success after the fact.

For `HIGH`, `CRITICAL`, `P0`, or otherwise action-blocking residuals,
`closure_state: open` means the verdict is evidence of a problem, not evidence
of permission. Language-model agreement alone cannot set `closed`; closure
requires external evidence, runtime evidence, or an explicit scoped waiver.

If R3 cannot produce the trace because party output is missing, evidence is
unverifiable, or Auditor B is unavailable, the final verdict must say so
explicitly and mark the run degraded. A degraded run may still inform the user,
but it must not serve as protected-write evidence.

### Phase 7: Recursive QUINTE — Nested Debate

Complex debates may contain sub-questions that exceed single-agent convergence capacity. When hm identifies such a sub-question during any phase, a nested QUINTE may be spawned.

#### Trigger Conditions

A recursive QUINTE may start when **any one** of the following holds:

- Party deadlock: at least two R1 parties produce mutually contradictory analyses.
- Depth required: the sub-question requires its own evidence gathering.
- User directive: the user explicitly requests a sub-debate.

#### Protocol

1. **Isolation**: Sub-QUINTE runs under its own host-provided output directory.
2. **Full protocol**: Complete R1 with five parties, R2 with five-party
   cross-review, and R3 with dual verdict. No shortcuts.
3. **Re-injection**: Verdict injected into parent debate as resolved evidence — challengeable in R2, not re-litigable de novo.
4. **Termination**: Each sub-QUINTE converges independently. Parallel sub-QUINTEs permitted.
5. **Annotation**: Verdicts annotated `[QUINTE↻ N]` where N is nesting depth.

#### Constraints

- Sub-QUINTE may NOT modify parent debate scope, participants, or protocol rules.
- Sub-QUINTE verdicts are evidence, not parent verdicts — parent R3 arbiter retains final authority.
- Recursive QUINTE is host-triggered or explicitly user-requested. It is never
  a shortcut around the full R1/R2/R3 protocol.

## 3. Runtime Controls

Runtime controls:

- Cost circuit breaker: more than 100 claims, more than 50 refutation calls, or more than 3 loops requires hm approval.
- Human intervention: after each phase, hm may inject user feedback.
- Poison resistance: a single party with more than 50 claims or evidence-free claims triggers an anomaly flag, hm review, and downgrade.
- State persistence: every debate records a structured log in the host session store. Prior conclusions are evidence only when explicitly selected; never auto-injected.

## 4. Agent Dispatch Requirements

### 4.1 Host Routing Contract

The host must provide verified native CLI routes for all five R1/R2 parties and
the R3 Auditor B before a full QUINTE run can begin. This protocol does not
publish concrete commands, model names, credentials, local paths, or provider
settings.

Required route properties:

- Native CLI: the route starts a real external process, not a merged prompt or wrapper script.
- Independent output: each party writes to its own audit artifact.
- Traceable process: the host can inspect status, logs, output size, and attempt history.
- Same-route recovery: retry repairs the same route and never swaps in another party.

### 4.2 Anti-Drift
- Task-first: specific task at prompt start
- Semantic isolation: "ONLY Y" not "NOT X"
- Mandatory first-line restatement: `TASK: [restatement]`

### 4.3 Error Classification

Hermes classifies native CLI dispatch results from exit status, stderr/stdout
patterns, output file size, and session logs. Wrapper JSON sidecars are
obsolete.

Error classes:

- `auth`: exit 2, 401, unauthorized, credential, or login failure. Do not retry until the host route credential is repaired.
- `rate_limit`: 429, quota, or provider rate-limit signal. Back off and retry the same route within the manifest attempt budget.
- `timeout`: route exceeds the phase deadline. Shrink the prompt within the manifest limit and retry the same route.
- `interrupted_recoverable`: SIGTERM, interrupted process, or recoverable zero-byte interruption. Resume or re-dispatch the same route when the route supports it.
- `deprecated`: model, command, or route removed or unsupported. Repair the same route; if still unavailable, record degradation and abort or escalate.
- `empty_output`: exit 0 with a zero-byte output artifact. Retry the same route with a shortened prompt.
- `invalid_output`: exit 0 with output that does not begin with `TASK:`. Treat startup banners, TUI-only sessions, and wrong command targets as failed dispatches; retry the same route with a shortened prompt.
- `unknown`: any other failure. Retry within the attempt budget, then escalate with stdout, stderr, transcript, and output artifact evidence.


## 5. Invariants

1. **Never skip a party.** Phase 0 locks the five-party R1/R2 manifest from host routing.
2. **Never skip R2.** Unanimous R1 can be shared blind spot — R2 is confirmatory audit.
3. **hm phase block is synchronous.** Not post-hoc audit. Per-phase block with ABORT authority.
4. **Heterogeneity remains a design goal, not a route fact.** R1/R2 party
   identities are fixed by host routing. Provider/model diversity may be
   recorded as a confidence note, but provider guesses must not change routing.
5. **Dry ≠ done.** Dry triggers escalate (mandatory human review), not auto-termination.
6. **External-write boundary.** QUINTE supplies a verdict trail for protocol or
   architecture decisions. Runtime authorization for `git push`, destructive
   deletes, reset, and config writes belongs to host authorization and still
   requires explicit user authorization.
7. **Evidence requirement.** Claims without evidence (file:line, grep output,
   runtime result) receive downgraded weight. JSON sidecar
   `evidence_citations` MUST be verified as resolvable before confidence scores
   enter Phase 2. Unresolved citations receive `[CITATION_UNVERIFIED]` and 0.5x
   weight.
8. **Party failure requires classified same-route recovery.** 0 bytes is not
   immediate fail; apply §4.3 tier-specific recovery (backoff/shrink/resume)
   inside the same host route. Do not replace one R1/R2 party with Auditor B,
   hm, or another party. If the route remains unavailable after recovery, record
   degradation and abort/escalate instead of forging a five-party verdict.
9. **Auditor B is R3-only.** Auditor B never replaces R1/R2 parties.
10. **Residual trace is the action artifact.** A verdict without a residual
    trace may explain the debate, but it is not sufficient evidence for
    protected writes or other action boundaries.

## 6. The Five Gates

The five gates:

- Intent gate catches the wrong question being asked. Ambiguous user intent returns to clarification.
- Evidence gate catches hm directional error. Any comparative claim needs bidirectional grep and `file:line` evidence.
- Perspective gate catches single-perspective bias. If the conclusion is something the user may rely on, hm performs a quick judgment and runs the full pipeline when the gate passes.
- Prompt gate catches prompt contamination. Every party dispatch receives anti-drift wrapping.
- Protected-write gate catches architecture drift. Public-repo writes, patches, commits, pushes, deletes, resets, and config writes require a host-guarded pre-write check and a QUINTE verdict trail when HIGHBALL selects QUINTE.

**Execution**: Parallel in Phase -1 by hm (~5s). Five gates check the same input from different angles simultaneously.

*QUINTE protocol — engineering protocol only. Cultural context belongs in README/RASHOMON.*
