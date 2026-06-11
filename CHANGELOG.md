# Changelog

## v3.1 (2026-06-10)
- **Spec trim per 5/5 QUINTE verdict** (R1 hm+cc+cw+omp → R2 → R3 hm+rx). Pruned ~40% concept density from v3.0:
  - REMOVED from PROTOCOL.md: three-mechanism epistemology framework, cross-round consistency Agent, auto-diff JSON Schema
  - SIMPLIFIED: loop-until-dry → single-critic + 3-round hard cap
  - DOWNGRADED: Invariant#4 (cross-model) → Desideratum
  - ADDED: omp Verification Phase 5a (LSP/DAP/exec ground-truth)
  - KEPT: orchestration-oversight separation, hm synchronous veto, governance layer, 證門 two-layer, Phase 0 Manifest Agent
- **Version alignment**: PROTOCOL.md title → v3.1, README badge synced, CHANGELOG appended

## v3.0.1 (2026-06-10)
- **Meta-QUINTE: v3.0 Upgrade Value Assessment** — R1(hm+cc+cw+omp) + R2 cross-review + R3(hm+rx) dual verdict on whether v3.0 concepts justify their complexity cost. 5/5 consensus: orchestration-oversight separation KEEP, Invariant#4 downgrade→Desideratum, loop-until-dry simplify, three-mechanism epistemology remove from PROTOCOL.md, omp Verification Phase 5a ADD, omp as cc fallback needs feasibility audit. See `debates/2026-06-10-v3-evaluation/`.
- **Meta-QUINTE: 力大砖飛 Classification** — *При достаточной тяге и кирпич полетит* ("with enough thrust, even a brick flies"). Soviet aviation joke, F-4 Phantom principle, 力任せ. R1+R2+R3 on whether QUINTE is brute-force aesthetics. Verdict: "Governed Brute-Force Ensembling." OMP's "Refined Brute Force" adopted as canonical label. See `debates/2026-06-09-brute-force/`.
- **Meta-Audit: hm R1 Fact-Check** — cc+cw+omp cross-verify hm's previous R1 claims against four repos. 3 HIGH errors found (CHANGELOG v3.0 entry fabricated, file coverage inflated, cc 71% stat external/unverifiable). 6/6 verdicts survive audit. See `debates/2026-06-10-meta-audit/`.
- **Bugfix**: README badge `protocol-v2.4` → `protocol-v3.0` (stale since 2026-06-09 spec upgrade)
- **Bugfix**: CHANGELOG v3.0 entry added (was missing — spec declared v3.0 but CHANGELOG stopped at v2.4)

## v3.0 (2026-06-09)
- **Orchestration-Oversight Separation**: cc executes orchestration, hm synchronous veto oversight
- **Three-Mechanism Epistemology**: Agent (isolated context) + Workflow (pipeline/parallel) + Bash (external agents)
- **Cross-Model Adversarial Verification**: Phase 3, 3 refuters per dispute, ≥1 from different provider
- **Loop-Until-Dry**: Dual critic + dual condition termination + escalate→human
- **Governance Layer**: Cost circuit breaker, poison detection, state persistence, cross-round consistency
- **Parallel Four Gates** (四道門): Amamon, Kyōmon, Shōmon split into gate layer + execution layer, Kan'nukimon
- **JSON Schema Structured Output**: Phase 2 auto-diff claims comparison
- Ratified 5/5 agent consensus. See `debates/2026-06-09-v3-ratification/`.
- Self-audit: 4 HIGH+ issues, 4 fixed. See `debates/2026-06-09-v3-self-audit/`.

## v2.4 (2026-06-07)
- **Anti-drift protocol**: 3-layer prompt engineering defense (task-first + semantic isolation + forced restatement) in `spec/extensions.md`
- 5/5 agent consensus from dedicated QUINTE debate
- Reference implementation active on macOS + Linux (planned) + Windows (verified)

## v2.2 (2026-06-03)
- hm/rx shorthands formally adopted
- rx R1 prohibition codified ("⛔ rx 绝不参与 R1")
- Execution discipline: R1 all four → collect → R2 all five
- Push audit format scan checklist added

## v2.1 (2026-06-03)
- omp promoted from hot spare to full R1 participant
- Architecture: R1=4 agents, R2=5 agents
- No-degradation policy formalized

## v2.0 (2026-06-03)
- QUINTE architecture formalized
- 5-agent system (up from Quattro 4-agent)
- Reasonix added as R2-only cross-review judge
- Skill absorbed into multi-agent-debate (later reversed — see v2.2.1)

## v2.3 (2026-06-06)
- **Meta-QUINTE debate** passed: 5 agents × 3 rounds examined protocol for logical flaws
- **Scope statement** added: honest ~20-30% oversight detection claim
- **R3 adjudication rules** defined: voting, tiebreaker, weighting, recusal, dissent preservation
- **"Adversarial"→"Cross-review"**: renamed for honesty — value is oversight detection, not epistemic challenge
- **"No model degradation"→"No model-tier degradation"**: clarified prompt-size reduction ≠ degradation
- **Parallel execution**: explicitly R1 = max(agent times), not sum
- **Model diversity desideratum** recorded (not hard requirement in v2.3)

## v2.2.1 (2026-06-06)
- **Architectural pivot**: Repo becomes canonical protocol home
- `spec/PROTOCOL.md` extracted as normative protocol definition
- Hermes skill repositioned as reference implementation
- `implementations/` deferred until second implementation exists
- Added: MIGRATION.md, extensions.md, CHANGELOG.md
