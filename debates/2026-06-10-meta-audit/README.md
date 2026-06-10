# Meta-Audit: hm R1 Fact-Check — 2026-06-10

**Topic**: Re-QUINTE to verify whether hm's R1 claims from the v3.0 evaluation were factually correct. Three external agents independently verify every claim.

**R1 (3 agents)**: cc, cw, omp cross-check hm's R1 against the four repos

**Key Findings**:

| Claim | Verdict | Severity |
|-------|---------|----------|
| File coverage: "26/26 (100%)" | MISLEADING — real coverage ~70% | HIGH |
| cc timeout rate: "71%" | UNVERIFIABLE — external source, not in audit scope | HIGH |
| CHANGELOG L25-26 records v3.0 | FACTUALLY WRONG — describes v2.3, CHANGELOG has no v3.0 entry | HIGH |
| Badge version: v2.4 vs spec v3.0 | VERIFIED TRUE | — |
| Concept density: "~2.5× increase" | MISLEADING — own numbers give ~1.86×, items double-counted | MEDIUM |
| omp "excluded from architecture decisions" | MISLEADING — omp voted 5/5 in ratification | MEDIUM |

**Outcome**: Three factual errors found (CHANGELOG fabrication, file coverage inflation, external stat). hm's six substantive verdicts survive the audit. Methodology flaws documented; conclusions directionally correct.

**Files**: 3 (cc + cw + omp R1)
