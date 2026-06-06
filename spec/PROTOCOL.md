# QUINTE Protocol Specification v2.4

> **Canonical protocol definition.** For the reference implementation, see [hermes-skill/](../hermes-skill/SKILL.md).
>
> **Scope**: QUINTE improves factual completeness and oversight detection through redundant coverage and structured re-examination. Estimated improvement: ~20-30% over solo analysis. It does not validate correctness of shared-model reasoning about novel or ambiguous situations where all agents share the same model's knowledge boundaries.

---

## 1. Architecture

QUINTE is a five-agent structured debate protocol for AI conclusion confidence. Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate questions through structured rounds of analysis, cross-examination, and final verdict.

### Participants

| Agent | Round 1 | Round 2 | Role |
|-------|:-------:|:-------:|------|
| Hermes (hm) | ✅ | ✅ | Orchestrator + Final Verdict |
| Claude Code (cc) | ✅ | ✅ | Broad coverage, structured reports |
| CodeWhale (cw) | ✅ | ✅ | Deep research, concurrency analysis |
| omp | ✅ | ✅ | Full participant, all rounds |
| Reasonix (rx) | — | ✅ | R2 pure reasoning cross-review judge |

**R1**: 4 agents (hm + cc + cw + omp). rx excluded — run mode cannot execute tools. When rx supports tool calls, R1 expands to 5.

**R2**: 5 agents (all). rx joins as pure reasoning cross-review judge.

**All agents use the same base model.** No degradation to lower tiers.

### Shorthands
Hermes=hm, Claude Code=cc, CodeWhale=cw, Reasonix=rx. omp has no shorthand.

---

## 2. Round Structure

### Round 1 — Independent Analysis

All R1 agents receive the same question simultaneously. Each produces an independent analysis with no cross-talk. Outputs go to separate files.

**Discipline**: R1 all launch in parallel. Hermes must collect ALL outputs before proceeding. Any agent timeout/failure → shrink prompt and retry. **Never skip an agent.** Three consecutive retry failures → escalate to user for authorization.

### Round 2 — Cross-Review

Each agent reviews ALL OTHER agents' R1 outputs (never self-review). Purpose:

1. **Confirm consensus** — re-examine agreed points from a different angle
2. **Flag divergences** — where agents disagree
3. **Catch blind spots** — what everyone missed

**R2 is mandatory. Never skip.** Unanimous R1 does not guarantee correctness — same-model agents can share blind spots (POSTMORTEM #19/#21). When R1 is unanimous, R2 serves as confirmatory audit. When R1 diverges, R2 identifies disputes.

### Round 3 — Final Verdict (Hermes only)

Hermes synthesizes all R1+R2 outputs into a final ruling:

1. Recommended approach with reasoning
2. Explicit adoption/rejection of each agent's contributions
3. Risk matrix
4. Execution plan (if applicable)

**Adjudication rules**:

- **Voting**: Each R2 agent's assessment of a disputed finding counts as one vote. ≥3/5 confirms; 2/5 is a split; ≤1/5 rejects.
- **Tiebreaker**: On a 2-2 split (one abstention/timeout), Hermes casts the deciding vote. The decision must evaluate the underlying reasoning chains, not just vote-count. The dissent and tiebreak rationale must be documented.
- **Weighting**: rx's judgments carry full weight (1×) on logical/coherence disputes. On factual disputes where rx lacks source file access, rx's judgments carry 0.5× weight and are noted as "evidence-limited."
- **Recusal**: When Hermes' own R1 finding is disputed in R2, Hermes MUST defer to the majority of other agents on that finding — or document a detailed justification for overruling them.
- **Dissent preservation**: Any agent's R2 finding rejected in R3 MUST be preserved in the verdict with the rejection rationale. Silence is not rejection; explicit documentation is required.

**Hard cap**: Exactly 3 rounds. R3 must converge. If R2 discovers a material error in R1's shared premises, the orchestrator MAY restart R1 with corrected premises (noted in verdict).

---

## 3. Invariants

1. **No delegation.** All agents invoked via direct CLI. Sub-agent frameworks lose context and are interruptible.
2. **Parallel by default.** R1 all agents launch simultaneously. R2 all agents launch simultaneously. R1 wall-clock = max(agent_1...agent_n), not the sum of individual agent times.
3. **No model-tier degradation.** All agents use the same top-tier model. Flash/lower tiers explicitly forbidden. Prompt-size reduction on retry is permitted (see §4) but MUST preserve all factual claims, constraints, and source references from the original prompt.
4. **Token budget unlimited.** DeepSeek API economics permit exhaustive debate. Never shorten prompts or merge rounds to save tokens.
5. **Cross-review.** Agents critically examine others' work. The value is in oversight detection — catching what others missed — and in structured re-examination from different angles, not in genuine epistemic challenge between identically-trained models.
6. **Source verification before dispute.** Before flagging any "inconsistency," verify against source files character-by-character. Check modifiers (max/min/approx/up to/pro-rata).

---

## 4. Degradation Protocol

| Failure | Action |
|---------|--------|
| Any agent 180s zero output | Kill → shrink prompt → retry |
| 3 consecutive retry failures | Escalate to user for authorization |
| Agent unreachable (not installed) | Skip with notation in verdict; continue with ≥3 agents |
| All external agents fail | Hermes solo with explicit caveat |

**Minimum viable debate**: Hermes + 2 other parties.

---

## 5. Trigger Rules

### Mandatory (always trigger QUINTE)
- Code changes, configuration changes, architectural decisions
- Any git push (including punctuation/formatting changes)
- Ledger reconciliation, reporting, cross-validation
- Economic modeling, pricing, contract interpretation
- Any output the user might rely on

### Optional (trigger when uncertain)
- Multi-file coordination, version compatibility, encoding issues
- Debugging complex systems

### Skip (do NOT trigger)
- Simple file lookups (ls, grep, stat)
- Single-tool deterministic queries
- ≤2 tool calls with no reasoning required

**Override**: User says "quinte" → always execute, regardless of automatic rules.

---

## 6. Versioning

This protocol uses calendar-inspired versioning: `v<major>.<minor>`.

- **Major**: Agent count changes, round structure changes, new invariants
- **Minor**: Trigger rule updates, degradation tuning, documentation

### History
- **v2.4** (2026-06-07): Agent dispatch anti-drift requirements (§7). Mandatory 3-layer prompt engineering for all external agent dispatches: task-first structure, semantic isolation ("ONLY Y" not "NOT X"), forced restatement. 5/5 agent consensus from dedicated QUINTE debate. Design lessons from POSTMORTEM added as `references/lessons.md`.
- **v2.3** (2026-06-06): Meta-QUINTE debate passed (5 agents, 3 rounds). Added: scope statement, R3 adjudication rules (voting/tiebreaker/weighting/recusal/dissent), clarified parallel execution model, renamed "adversarial"→"cross-review" for honesty, "no model degradation"→"no model-tier degradation."
- **v2.2** (2026-06-03): hm/rx shorthands added, rx R1 prohibition codified, execution discipline
- **v2.1** (2026-06-03): omp promoted from hot spare to full R1 participant. R1=4, R2=5

---

## 7. Agent Dispatch Requirements

When dispatching prompts to external LLM agents (cc, cw, omp, Reasonix), all implementations MUST apply a three-layer defense against concept namespace collision — the phenomenon where prompt keywords activate wrong training-data associations, causing agents to answer about unrelated domains.

### Why Negation Fails

"NOT X" instructions require the model to first activate X's concept to understand what to negate. By then, the association is already primed and competes with task instructions. Do not use negation-based anti-drift directives.

### Three-Layer Defense

| # | Technique | Mechanism |
|---|-----------|-----------|
| 1 | **Task-first structure** | Place the concrete task before any context, constraints, or system descriptions. The model processes left-to-right; task-first anchors interpretation before ambiguous keywords appear. |
| 2 | **Semantic isolation** | Replace all "NOT X" constructions with "ONLY Y" or contrastive "X means A, not B." Positive framing establishes an identity construct without activating forbidden concepts. |
| 3 | **Forced restatement** | Require the agent's first output line to be: `TASK: [one-sentence restatement of the understood task]`. If the restatement is wrong, the entire output is suspect — discard and retry. |

### Template

```
[Concrete task — file path, specific question]
Constraint: [semantic isolation — what terms mean here, not what they don't mean]
First line of your output MUST be: "TASK: [restatement]"
```

### Output Validation

After agent completion, the orchestrator MUST check the first 5 lines of output for off-topic keywords. If detected, the agent's output for that round is discarded and its vote excluded. Implementations maintain their own keyword blocklists based on observed drift patterns.

### Progressive Deployment

| Phase | Timeline | Content |
|-------|----------|---------|
| Immediate | Today | Three-layer template changes. Zero infrastructure — prompt text only. |
| Short-term | Within 1 week | Keyword alias map (TOML/JSON) + automated first-line validation |
| Medium-term | Within 1 month | Embedding-based collision screening + collision logging feedback loop |

> **Note**: This is a mitigation, not a fix. The root cause — training-data associations overriding explicit instructions — requires better instruction-following in base model post-training. Until then, external guardrails are necessary.
