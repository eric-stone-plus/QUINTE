# QUINTE Protocol Specification v3.1

> **Canonical protocol definition.** For the reference implementation, see [hermes-skill/](../hermes-skill/SKILL.md).
>
> **v3.1 (2026-06-10)**: Pruned ~40% concept density per 6/6 QUINTE verdict. Removed: three-mechanism epistemology from spec, cross-round consistency Agent, auto-diff JSON Schema. Simplified: loop-until-dry → single-critic + 3-round cap. Downgraded: Invariant#4 → Desideratum. Added: omp Verification Phase 5a.
>
> **Scope**: QUINTE improves factual completeness and oversight detection through redundant coverage, structured re-examination, and adversarial verification. Estimated improvement: ~30-50% over solo analysis (v2.4 baseline: ~20-30%). It does not validate correctness of shared-model reasoning about novel situations where all agents share the same model's knowledge boundaries — cross-model diversity in R2 partially mitigates this.

---

## 1. Architecture

QUINTE is a five-agent structured debate protocol for AI conclusion confidence. v3.0 introduces a separation of concerns: **Claude Code executes orchestration** through native Workflow primitives; **Hermes provides synchronous oversight** through per-phase veto authority.

### 1.1 Participants

| Agent | Round 1 | Round 2 | Role | Reasoning |
|-------|:-------:|:-------:|------|-----------|
| Claude Code (cc) | ✅ | ✅ | **Primary Orchestrator** + Participant | xhigh (ultracode) |
| Hermes (hm) | ✅ | ✅ | **Oversight Layer** + Participant | xhigh |
| CodeWhale (cw) | ✅ | ✅ | Deep analysis + implementation verification | max |
| omp | ✅ | ✅ | Rapid reasoning + security perspective | xhigh |
| Reasonix (rx) | — | ✅ | R2 pure reasoning cross-review judge | max |

**R1**: 4 agents (cc + hm + cw + omp). rx excluded — run mode cannot execute tools.
**R2**: 5 agents (all). rx joins as pure reasoning cross-review judge with structured claims JSON input.

### 1.2 Orchestration Mechanisms

cc provides three native mechanisms:

| Mechanism | QUINTE Role | Examples |
|-----------|------------|----------|
| **Agent** (internal sub-agents) | Specialized reviewers | Explore: full file enumeration. Plan: architecture validation. general-purpose: completeness critic, cross-round consistency, poison detection |
| **Workflow** (orchestration engine) | Pipeline execution + structural guarantees | `pipeline()`: phase sequencing. `parallel()`: concurrent agent dispatch. `agent({schema})`: structured output with JSON Schema validation. Adversarial verification: per-disagreement refutation. loop-until-dry: convergence detection |
| **Bash** (external agents) | Multi-perspective analysis | `hermes chat -q`, `codewhale exec --auto`, `reasonix run`, `omp` |

### 1.3 Orchestration vs Oversight Separation

```
Claude Code (Execution)          Hermes (Oversight)
─────────────────────────        ─────────────────────────
pipeline() phases                Per-phase APPROVE/REJECT
parallel() agent dispatch        Drift detection
JSON Schema auto-diff            Agent omission check
Adversarial verification         Quality audit
loop-until-dry convergence       ABORT authority
Structured logging               Context injection
```

**hm's synchronous veto**: After each Phase, hm receives `{phase_id, output, claims_diff, agent_status}` and responds with `APPROVE | REJECT(reason) | ABORT(reason) | MODIFY(spec)`. 15s timeout → cc PAUSE. This is the v3.0 replacement for hm's v2.4 orchestrator role — hm's xhigh reasoning is applied to auditing orchestration plans, not executing them.

---

## 2. Phase Structure

### Phase -1: Four Gates (hm, parallel)
Four gates executed by hm in parallel (~5s). See §6 for gate definitions.

### Phase 0: Agent Manifest Generation
Independent Agent reads agent registry → outputs mandatory participant list. cc can only add, not remove. hm approves → locked.

### Phase 1: Round 1 — Independent Analysis

All four R1 agents (cc/hm/cw/omp) receive the same question simultaneously via `parallel()`. Outputs use loose JSON Schema (`additionalProperties: true`, `freeFormInsights` field). Each output MUST begin with `TASK: [restatement]` — non-matching first line → output discarded.

**Schema (loose mode)**:
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

**Discipline**: R1 all launch in parallel. Timeout 120s → kill + shrink prompt + retry. Three consecutive failures → escalate. **Never skip an agent** (pipeline structurally enforces this).

### Phase 2: Auto-Diff + Schema Convergence

Claims from all R1 outputs are diffed:
- Identical claims (by statement hash) → consensus pool
- Different claims → dispute pool (enters Phase 3)
- Novel finding categories not in schema → schema extension proposal

hm reviews diff quality → approves.

### Phase 3: Round 2 — Adversarial Verification

For each disputed claim, `parallel` 3 refutation agents:
- **Cross-model requirement**: at least 1 refuter uses a different provider/model
- ≥2/3 refute → claim discarded
- 1/3 refute → claim retained, marked "contested"
- 0/3 refute → claim confirmed
- Vague refutations ("might be wrong") do not count toward refute tally

hm reviews for false-positive refutations → approves.

### Phase 4: Reasonix Cross-Review + Cross-Round Consistency

rx receives all structured claims (consensus + contested) as JSON input — no file reading required. Produces pure reasoning verdict.

**Cross-round consistency agent** (internal Agent): compares all outputs from R1/R2/R3, detecting: stance drift, term redefinition, premise creep, collective hallucination.

hm reviews → approves.

### Phase 5: Loop-Until-Dry Convergence

Two completeness_critic agents (different configurations: temperature/prompt template) search for remaining blind spots.

**Dual-condition termination**:
1. Two consecutive rounds with zero new claims
2. Dispute count not increasing + evidence repetition > 90%

Both conditions must hold simultaneously → trigger **escalate** (mandatory human review, NOT auto-termination).

**Truth verification**: claims about executable code → `Bash` runtime execution validates against actual behavior.

### Phase 6: Round 3 — KANSA Audit

KANSA persona (launched via `hermes chat -q`) performs:
1. **Topic-rotation audit**: re-examines from alternative angles
2. **Authorization boundary check**: flags overreach claims
3. **Poison detection**: checks for malicious/low-quality claim injection (single agent >50 claims → anomaly; assertion without evidence → downgrade)
4. **Gate compliance**: verifies all four gates were traversed

hm final approval → structured log written to `~/.hermes/quinte/`.

---

## 3. Governance Layer (v3.0)

| Mechanism | Threshold | Action |
|-----------|----------|--------|
| **Cost circuit breaker** | claims > 100, refutation calls > 50, loops > 5 | hm approval required to continue |
| **Human intervention** | After each Phase | hm can inject user feedback |
| **Poison resistance** | Single agent >50 claims OR evidence-free claims | Anomaly flag → KANSA investigation → downgrade |
| **State persistence** | Every debate | Structured log → `~/.hermes/quinte/`. Next debate auto-injects prior conclusions |
| **Cross-round consistency** | Phase 4 | Independent Agent detects drift across R1/R2/R3 |

---

## 4. Agent Dispatch Requirements

### 4.1 Claude Code (Orchestrator)
```bash
# settings.json must include:
# "model": "deepseek-v4-pro"
# "maxThinkingTokens": 32000

claude -p --permission-mode bypassPermissions "prompt"
```

### 4.2 Hermes (Oversight + Participant)
```bash
hermes chat -q "prompt" --pass-session-id 2>&1
```

### 4.3 CodeWhale
```bash
codewhale exec --auto "prompt" 2>&1
# First line MUST be: "TASK: [restatement]"
```

### 4.4 Reasonix (R2 only)
```bash
reasonix run --effort max "prompt" 2>&1
```

### 4.5 OMP
```bash
omp "prompt" 2>&1
# or: python3 /tmp/omp_run.py "prompt" 2>&1
```

### 4.6 Anti-Drift (閂門)
- Task-first: specific task at prompt start
- Semantic isolation: "ONLY Y" not "NOT X"
- Mandatory first-line restatement: `TASK: [restatement]`

---

## 5. Invariants

1. **Never skip an agent.** Pipeline structurally enforces; Phase 0 manifest generated independently.
2. **Never skip R2.** Unanimous R1 can be shared blind spot — R2 is confirmatory audit.
3. **hm veto is synchronous.** Not post-hoc audit. Per-phase block with ABORT authority.
4. **Cross-model diversity in R2.** At least 1/3 refuters from different provider.
5. **Dry ≠ done.** Dry triggers escalate (mandatory human review), not auto-termination.
6. **Push gate.** Any push (code, config, docs) requires prior QUINTE (R1+R2+R3). No exceptions.
7. **Evidence requirement.** Claims without evidence (file:line, grep output, runtime result) → downgraded weight in verdict.

---

## 6. The Four Gates

| Gate | Failure Mode | Trigger | Action |
|------|-------------|---------|--------|
| **雨門** Amamon | Wrong question asked | Ambiguous user intent | `clarify` back |
| **鏡門** Kyōmon | hm directional error | Any comparative claim | Bidirectional grep + `file:line` evidence |
| **證門** Shōmon | Single-perspective bias | Conclusion the user may rely on | Gate layer: hm quick judgment (~1s). If passed → cc Workflow full pipeline (Phases 0-6) |
| **閂門** Kan'nukimon | Prompt contamination | Every agent dispatch | Three-layer anti-drift wrapping |

**Execution**: Parallel in Phase -1 by hm (~5s). Four gates check the same input from different angles simultaneously. Note: 證門 in Phase -1 is the **gate layer only** (~1s decision). The full pipeline (Phases 0-6) is the execution layer, run by cc Workflow, not part of the parallel gate block.

---

## 7. Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05 | Initial QUINTE protocol |
| 2.0 | 2026-06-04 | rx added as R2 participant; four-gate model formalized |
| 2.4 | 2026-06-08 | 鏡門 elevated to independent fourth gate; cross-repo audit; agent counting discipline |
| 3.0 | 2026-06-09 | Orchestrator hm→cc; three-mechanism architecture; hm synchronous veto; cross-model adversarial verification; loop-until-dry; governance layer; parallel gates |
| 3.1 | 2026-06-10 | **Trimmed per 6/6 QUINTE**: removed three-mechanism epistemology from spec, cross-round consistency Agent, auto-diff JSON Schema. Simplified loop-until-dry → single-critic + 3-round cap. Downgraded Invariant#4 → Desideratum. Added omp Verification Phase 5a. |

---

*QUINTE v3.1 — trimmed per 6/6 consensus 2026-06-10 (hm+cc+cw+omp+rx).*
