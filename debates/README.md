# QUINTE Debate Logs

Each subdirectory is one complete QUINTE debate session. Naming: `YYYY-MM-DD-<topic-slug>/`.

## Debate Index

| Date | Topic | Verdict | Files |
|------|-------|---------|-------|
| 2026-06-10 | [v3.0 Upgrade Value Assessment](2026-06-10-v3-evaluation/) | 6/6: trim ~40%, keep orchestration-oversight | R1×4 + R2 + R3-A + R3-B + final |
| 2026-06-10 | [Meta-Audit: hm R1 Fact-Check](2026-06-10-meta-audit/) | 3 HIGH errors found; 6/6 verdicts survive | R1×3 + final |
| 2026-06-09 | [力大砖飛 — *При достаточной тяге и кирпич полетит*](2026-06-09-brute-force/) | Governed Brute-Force Ensembling | R1×4 + R2×5 + R3-A + R3-B + final |
| 2026-06-09 | [v3.0 Ratification](2026-06-09-v3-ratification/) | 5/5 pass with amendments | R1×5 + R2×5 + final |
| 2026-06-09 | [v3.0 Self-Audit](2026-06-09-v3-self-audit/) | 4 HIGH+ issues, 4 fixed | R1×3 + R2 + final |

## Structure

```
debates/
├── 2026-06-10-v3-evaluation/       # v3.0 upgrade value QUINTE (hm+rx R3)
├── 2026-06-10-meta-audit/          # hm R1 fact-check by cc+cw+omp
├── 2026-06-09-brute-force/         # 力大砖飛 — brute force classification
├── 2026-06-09-v3-ratification/     # v3.0 protocol ratification (5 agents)
└── 2026-06-09-v3-self-audit/       # v3.0 self-audit under new architecture
```

## Naming Convention

- `{agent}_round{N}.md` — agent round output (e.g., `claude_round1.md`)
- `{agent}_q{N}.md` — per-question audit (e.g., `cc_q1.md`)
- `final_verdict.md` — R3 merged ruling
- `README.md` — per-debate summary and context

## Notes

- omp outputs may contain `=== REASONING ===` blocks — internal reasoning trace
- cc/cw thinking/reasoning blocks are internal and not preserved in logs
- rx is R2-only (pure reasoning cross-review); never participates in R1
