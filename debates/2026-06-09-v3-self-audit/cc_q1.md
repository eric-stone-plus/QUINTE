TASK: Audit v3.0 self-consistency of Shōmon gate-vs-pipeline two-layer design across PROTOCOL.md and GATES.md, plus any other contradictions.

---

## Audit Result: PROTOCOL.md ↔ GATES.md Self-Consistency

### Primary Finding: Shōmon Two-Layer Design Is Contradicted

The two documents **disagree materially** on the structure of 證門 (Shōmon). This is not a minor phrasing difference — it would produce different implementations depending on which document a developer follows.

#### What PROTOCOL.md says (single-layer, §6, line 212)

| Gate | Failure Mode | Trigger | Action |
|------|-------------|---------|--------|
| **證門** Shōmon | Single-perspective bias | Conclusion the user may rely on | **Full R1+R2+R3** |

PROTOCOL.md §2 Phase -1 (line 57-58) then runs all four gates **in parallel via hm**, including this Shōmon gate whose "action" is the entire pipeline. A reader following PROTOCOL alone would conclude that Shōmon-the-gate IS the full pipeline, executed as part of the ~5s Phase -1 block — which is both contradictory (the pipeline takes 30–180s) and structurally wrong (the pipeline spans Phases 0–6, not Phase -1).

#### What GATES.md says (two-layer, §门 3, lines 51-86)

```
證門 = 闸门层(hm, ~1s) + 执行层(cc Workflow, 30-180s)
```

- **闸门层 (gate layer)**: Phase -1 only. hm quickly judges "conclusional output? → enter cc pipeline." ~1s. This is what runs in the parallel four-gate block.
- **执行层 (execution layer)**: Phases 0-6. cc Workflow runs R1→R2→R3→loop-until-dry→KANSA. hm provides per-phase synchronous veto. This is NOT the gate itself.

GATES.md makes this explicit in its key clarification (lines 147-151):

> **證門 ≠ cc Workflow pipeline。** 證門闸门（~1s 判断）和證門执行（pipeline）是两层。闸门决定"要不要进 pipeline"，执行层是 pipeline 本身。

#### The Contradiction

PROTOCOL.md is **unaware of the two-layer split**. It collapses gate and execution into one row of the gate table. GATES.md's refined design — where the gate *decides whether to enter* the pipeline, and the pipeline is a *separate execution engine* — is not reflected in the canonical PROTOCOL spec. This means:

1. **PROTOCOL §6 table says Shōmon's action = "Full R1+R2+R3"** — but the Phase -1 parallel block runs all four gates. That would imply the full pipeline runs in Phase -1, which contradicts the explicit Phase 0–6 structure in PROTOCOL §2 itself.
2. **GATES.md clarifies the architecture** but PROTOCOL.md hasn't been updated to incorporate the distinction.
3. The other three gates (雨門, 鏡門, 閂門) are **consistent** across both documents — they are pure gate-level checks with no execution layer. The asymmetry (only Shōmon fans out to a pipeline) is the correct design but PROTOCOL.md never states it.

### Secondary Inconsistency: Version Metadata Drift

| Document | Title says | Footer says |
|----------|-----------|-------------|
| PROTOCOL.md | v3.0 | — |
| GATES.md | v3.0 (line 1) | **GATES-v3.1.md** (line 155) |

GATES.md self-identifies as v3.0 in the header but v3.1 in the footer stamp. If the two-layer clarification was a post-v3.0 refinement, the version numbers should reconcile.

### Secondary Inconsistency: GATES.md's "v3.0 变更" describes a change not in PROTOCOL.md

GATES.md line 5:

> v3.0 变更: 四门从串行改为并行。證門（原由 hm 手动调度 R1+R2+R3）现在拆分为两层…

This claims the two-layer split is a **v3.0 change**. But PROTOCOL.md, the canonical v3.0 protocol definition, does not include it. Either:
- The split was added after PROTOCOL.md v3.0 was ratified (making it a v3.1 change mislabeled as v3.0), or
- PROTOCOL.md is incomplete and needs to absorb GATES.md's refinement.

### What Is Consistent

- All four gate **names and kanji** (雨門, 鏡門, 證門, 閂門) match across documents.
- The **failure modes and triggers** for 雨門, 鏡門, and 閂門 align.
- The **Phase -1 parallel execution** model for the four gate-level checks is in both documents.
- The **cc Workflow pipeline structure** (Phase 0→6, per-phase hm veto) is identical in both documents.
- The **participant table** (cc/hm/cw/omp/rx) and orchestration/oversight separation (§1.3 in PROTOCOL) are unique to PROTOCOL but not contradicted by GATES.
- GATES.md's execution flow diagram (lines 111-145) correctly maps to PROTOCOL's Phase 0–6 structure.

### Recommended Fix

PROTOCOL.md §6 gate table should be updated to reflect the two-layer Shōmon design:

```
| **證門** Shōmon | Single-perspective bias | Conclusion the user may rely on | Gate layer: hm quick judgment → if passed, cc Workflow R1+R2+R3+Rx+KANSA |
```

And §2 Phase -1 should clarify that Shōmon in Phase -1 is the **gate layer only** (~1s decision), not the full pipeline. Alternatively, GATES.md should be cited as the canonical gate-layer definition and PROTOCOL §6 should point to it.

---

**Verdict**: The documents are **internally contradictory on Shōmon's architecture**. PROTOCOL.md presents a single-layer gate-is-pipeline model; GATES.md presents a refined two-layer gate-decides/pipeline-executes model. The latter is clearly the intended v3.0 design (GATES.md is more precise and self-consistent with the Phase 0–6 structure), but the canonical protocol spec hasn't been updated to match. All other gate and pipeline elements are consistent between the two documents.
