# QUINTE Protocol Specification v2.2

> **Canonical protocol definition.** For the reference implementation, see [hermes-skill/](../hermes-skill/SKILL.md).

---

## 1. Architecture

QUINTE is a five-agent structured debate protocol for AI conclusion confidence. Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate questions through structured rounds of analysis, cross-examination, and final verdict.

### Participants

| Agent | Round 1 | Round 2 | Role |
|-------|:-------:|:-------:|------|
| Hermes (hm) | ✅ | ✅ | Orchestrator + Final Verdict |
| Claude Code (cc) | ✅ | ✅ | Broad coverage, structured reports |
| CodeWhale (cw) | ✅ | ✅ | Deep research, concurrency analysis |
| OMP | ✅ | ✅ | Full participant, all rounds |
| Reasonix (rx) | — | ✅ | R2 pure reasoning cross-review judge |

**R1**: 4 agents (hm + cc + cw + OMP). rx excluded — run mode cannot execute tools. When rx supports tool calls, R1 expands to 5.

**R2**: 5 agents (all). rx joins as pure reasoning cross-review judge.

**All agents use the same base model.** No degradation to lower tiers.

### Shorthands
Hermes=hm, Claude Code=cc, CodeWhale=cw, Reasonix=rx. OMP has no shorthand.

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

**Hard cap**: Exactly 3 rounds. R3 must converge.

---

## 3. Invariants

1. **No delegation.** All agents invoked via direct CLI. Sub-agent frameworks lose context and are interruptible.
2. **Parallel by default.** R1 all agents launch simultaneously. R2 all agents launch simultaneously.
3. **No model degradation.** All agents use the same top-tier model. Flash/lower tiers explicitly forbidden.
4. **Token budget unlimited.** DeepSeek API economics permit exhaustive debate. Never shorten prompts or merge rounds to save tokens.
5. **Adversarial cross-review.** Agents criticize others' work, not defend their own.
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
- **v2.2** (2026-06-03): hm/rx shorthands added, rx R1 prohibition codified, execution discipline
- **v2.1** (2026-06-03): OMP promoted from hot spare to full R1 participant. R1=4, R2=5
- **v2.0** (2026-06-03): Architecture formalized, no-degradation policy
