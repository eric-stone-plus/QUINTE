# Conformance checklist status (PROTOCOL-REDESIGN §7 + doctrine additions)

Assessed 2026-08-18 against the redesigned core.

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Agent Card per HOST.md §2; seat cards validated per §5 binding | done | `src/a2a/card.rs` (live card verified), seat card digest pinning in policy/run |
| 2 | One run = one A2A task; `busy_run` refusal; state mapping exact | done | `host_map::send_message` (`-32010` on active runs) |
| 3 | R1 runs the policy roster; diversity violations fail preflight | done | roster enforced; `policy::validate` requires five declared, pairwise-distinct school perspectives (legacy v1 policies exempt until cutover) |
| 4 | Divergence predicate pure code, event-logged, replays identically | done | `r1_contestation` + durable `r2/skipped.json`; identical inputs replay identically |
| 5 | R2 only on contested outputs; no route identities | done | contested-only packet, rotated anonymized labels |
| 6 | R3 challenge single-use, expiry/replay/mismatch refused | done | `create_primary_arbiter_challenge` + binding validation |
| 7 | Deterministic merge; byte-identical replay | done | `merge_verdicts` + replay tests |
| 8 | Seat card drift mid-run halts | done | binding discipline, receipts intact |
| 9 | Bundle verify offline | done | host export/verify over event ledger |
| 10 | Legacy CLI behavior unchanged until cutover | done | HOST-CLI-LEGACY.md surface in service |
| 11 | Five distinct schools declared in the manifest | done | default policy carries five distinct perspectives; PI carries the quant five |
| 12 | `evidence_grounding` + `cross_seat_reconciliation` run every R1 round, event-logged | done | `validate_evidence_refs` + `r1_contestation` |
| 13 | Manifest carries the same-family caveat every run | done | Result 2.1 `trial_manifest.base_model_relation: "same_model"` (schema const) + `contamination_risks` |

Open items (carry-over, not blockers):

- token-usage aggregation for host cost ledgers (HOST.md open item).
