# Corrections for the Historical Residual Reports

Date: 2026-07-15

Applies to:

- `QUINTE残差管理机制调优分析.md`
- `QUINTE残差管理机制调优分析报告.html`

The originals are preserved as historical analysis. They are not a normative
specification and should be read with the corrections below.

## Corrections

| Historical statement or implication | Corrected fact |
| --- | --- |
| Residual evidence and closure evidence are generally unvalidated arbitrary strings. | Accepted R1/R2 `LaneOutput` claim refs, residual refs, and closure refs are checked against exact snapshot refs by `validate_evidence_refs` (`src/run.rs:1968`, `src/run.rs:2026`, `src/run.rs:3086`). The real gap is HM: challenge/schema binding is checked, but HM verdict residual refs are not passed through that validator before merge (`src/run.rs:1228`). |
| Final residuals are a merge or deduplication of R1/R2 findings. | `merge_verdicts` builds final residuals only from HM and CC (`src/run.rs:2979`). R1/R2 artifacts appear in the trial manifest but are not directly unioned into `result.residuals`. Consequently, the real risk is silent collapse when both R3 arbiters omit an earlier high-risk residual. |
| There is an R1 residual merge strategy that merely needs documentation. | No direct R1 residual merge creates the final residual list. Any proposal must first define a conservative R1/R2-to-R3 preservation invariant, not only document deduplication. |
| The report's two same-family-blind-spot residuals and merge residuals are separate confirmed defects. | The HTML duplicates the same-family issue under multiple IDs and partly duplicates merge concerns. Consolidate them when using the report as a backlog; do not count duplicate prose as independent evidence. |
| `QUINTE v0.1.1` in the HTML footer is an established product/run fact. | The report does not preserve enough runtime provenance to prove that label. GitHub shows only `v0.1.0` as a successful Release. Tags `v0.1.1` through `v0.1.3` had failed tag CI, and the deleted `v0.1.4` tag run was cancelled. Current source declares Cargo version `0.1.4`, but its local version migration is incomplete. |
| A mutable `quinte residual list/show/update/escalate` lifecycle is a P0 product requirement. | That was a recommendation, not a verified requirement, and it violates the settled ownership boundary if it mutates `result.json`. QUINTE results are immutable per-run evidence. Cross-run closure, waiver, action packets, and outcome history belong in HIGHBALL's append-only layer. |
| QUINTE should hard-code severity dollar thresholds and closure TTLs. | Those are case-specific policy suggestions. Generic QUINTE must not hard-code shipping amounts, TTLs, or domain taxonomy. If required, bind such policy outside the generic protocol with explicit provenance. |
| `residual_type` must become a fixed global domain enum. | Free text limits aggregation, but a universal shipping-oriented enum is not justified. Any type system must remain generic and extensible; HIGHBALL may maintain cross-run classification metadata without rewriting the QUINTE residual. |
| QUINTE lacks a closure workflow and should therefore mutate residual closure state later. | QUINTE intentionally emits an immutable run outcome. The missing capability is an external, append-only closure/waiver history and action gate in HIGHBALL, plus Hermes collection/presentation. Do not retrofit in-place history into `result.json`. |
| Final merge safely resolves all same-ID conflicts. | Merge compares only finding, disposition, and closure state (`src/run.rs:2980`). Conflicts in severity, residual type, source, evidence refs, required closure, or scope silently retain the first/HM value. Duplicate IDs within one arbiter are also not rejected. |
| Schema-valid IDs are unique. | The schemas constrain ID syntax but arrays have no uniqueness-by-ID semantic check (`schemas/lane-output.schema.json:13`, `schemas/lane-output.schema.json:18`, `schemas/hm-response.schema.json:27`). Add explicit semantic validation. |
| Version `1.0` in example payloads is harmless documentation. | The binding decision is that all QUINTE-owned versions follow the CLI package version `0.1.4`. The current local Rust diff changes some producers, while schemas/tests/docs still require `1.0`; the cohort must be completed atomically before commit. Keep the JSON Schema dialect at draft 2020-12. |

## Still useful from the historical reports

The reports remain useful for explaining why residual visibility matters,
identifying same-model correlation as a contamination risk, and motivating
better classification and closure evidence. Treat their priority labels,
counts, version claims, and component ownership as proposals superseded by this
index and `GROK_HANDOFF.md`.

## Normative reading order for implementation

1. Current repository code and schemas at the recorded commit.
2. `GROK_HANDOFF.md` for accepted decisions and verified defects.
3. `WINDOWS_REAL_MACHINE_ACCEPTANCE.md` for the Windows release gate.
4. This correction index.
5. The two historical reports as non-normative background only.
