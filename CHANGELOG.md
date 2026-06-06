# Changelog

## v2.4 (2026-06-07)
- **Anti-drift protocol**: 3-layer prompt engineering defense (task-first + semantic isolation + forced restatement) in `spec/extensions.md`
- 5/5 agent consensus from dedicated QUINTE debate
- Reference implementation: `hermes-core-rules-mac-x86` → `references/anti-drift-layered-defense.md`

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
