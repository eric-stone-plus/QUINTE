# Single-Vendor Doctrine — same-family multi-school review

> Amendment to [PROTOCOL-REDESIGN.md](PROTOCOL-REDESIGN.md), 2026-08-18.
> Supersedes the cross-vendor diversity constraint of D2 and resolves
> the §9 open items.

## 1. Decision

The seat roster runs on **one model family per run**. The family is a
declared binding of the run policy; the bound families are `deepseek`
(official direct API) and `qwen` (token-plan gateway, chat-completions
or Anthropic Messages face per base URL). Mixing families inside one
run is forbidden, and there is no budget for cross-vendor agreement
inside a run. The D2 pillar "cross-vendor agreement is real
confirmation" is therefore unattainable and is **replaced, not
weakened**: the epistemic weight moves from *who* reviews to *how* the
review is framed and *what is verified deterministically*.

## 2. The three-layer redesign

### A. Five-school framing diversity (primary axis)

The roster's diversity is the **school** — a distinct review doctrine —
not the vendor. Same model, five genuinely different lenses; the quant
pack declares (example shape, not a binding roster):

The binding roster is the five schools already implemented in the PI
seat agent (`pi/src/prompt.rs`, party labels matching Result 2.1's
trial_manifest perspectives):

| Seat | Party | School (binding) |
| --- | --- | --- |
| a-r1 | Party A | factor-risk — factor construction, risk premia, regime stability |
| b-r1 | Party B | event-driven — event identification, announcement effects, calendar risk |
| c-r1 | Party C | fundamental-supply-chain — financials, supply chains, business quality |
| d-r1 | Party D | trend-technical-regime — trends, technical signals, regime detection |
| e-r1 | Party E | market-microstructure — liquidity, execution, microstructure evidence |

Policy: all five R1 schools must be distinct and declared; school
prompts are doctrine-pack content (versioned, digest-pinned, carried in
the run manifest). A duplicate school fails preflight — this replaces
the old ≥2-provider-families constraint with **=5-distinct-schools**.
Two valid rosters exist: the shipped default policy's generic review
lenses (formal-specification, failure-mode, evidence-provenance,
operational, synthesis) and the quant pack's PI schools above — a
domain pack declares its own five; both satisfy the constraint.

### B. Adversarial round discipline (unchanged, now the main protocol)

- R2 runs only on genuinely contested outputs, pseudonymized — a same-family
  model re-reading a relabeled output with a recheck doctrine is the
  strongest same-family check available; route identities are structurally
  absent from the packet (verbatim lane prose is a declared contamination
  risk, never a prevented channel).
- R3 dual arbitration with the single-use challenge stays untouched:
  consumed-once, expiry/replay/mismatch refused.
- The divergence predicate stays pure code over R1 outputs.

### C. Deterministic verification sink (strengthened — the new weight carrier)

Numbers never come from the model. Every numeric claim in a lane output
must reference digest-bound evidence; the model critiques methodology
and narrative, not arithmetic. Two new gate kinds, evaluated in code:

- `evidence_grounding` — every claim field in a lane output must map to
  a residual id present in the evidence packet (or the explicit
  "unsupported" marker). Unmapped claims fail the gate.
- `cross_seat_reconciliation` — attestation symmetry with a majority
  rule: a residual id attested by a strict majority of R1 lanes but
  omitted by others marks **both sides** contested (the omitting lanes
  likely misread the packet, and the attesting lanes must be
  re-examined so the residual itself faces cross-examination).
  School-specific findings (attested by a minority) are NOT contested
  — different lenses legitimately raise different residuals. Implemented
  as the `r1_contestation` predicate in `src/run.rs`, which also feeds
  the contested-only R2 selection (k=0 skips R2 durably).

Quant facts (walk-forward sharpe, DSR, PBO, drawdown paths, funding
totals) come from the deterministic computation artifact in the brief;
R1 schools receive that artifact read-only.

## 3. Honest labeling (realized as the existing trial_manifest)

No new manifest vocabulary. The school roster is recorded in the **run
manifest** (school policy resolution, pre-run), and the executed-trial
facts travel in the **Result 2.1 `trial_manifest`** — which already
encodes the same-model reality (`base_model_relation: "same_model"`,
`perspective_count: 5`, `contamination_risks`):

```jsonc
// run manifest (school roster)
"roster": {
  "family": "deepseek",
  "schools": ["factor-risk", "event-driven", "fundamental-supply-chain",
              "trend-technical-regime", "market-microstructure"]
}
// Result 2.1 trial_manifest (executed-trial facts)
"trial_manifest": {
  "base_model_relation": "same_model",          // schema const
  "perspectives": [ /* Party A..E = the five school lanes,
                       each with route_id + r1/r2 artifact refs */ ],
  "perturbation_axes": ["school doctrine", ...],
  "independence_controls": [ ... ],
  "contamination_risks": [
    "same-family: agreement is consistency under multiple lenses,
     not independent confirmation", ...
  ]
}
```

The caveat therefore travels inside the result artifact end to end —
hosts (e.g. STAMMTISCH's `quinte_result` gate) and downstream reviewers
see it as evidence data, and consumers should surface
`trial_manifest.base_model_relation` / `contamination_risks` in their
gate records. A same-family run may never be presented as cross-vendor
confirmation.

## 4. Concrete deltas

- Seat policy: diversity constraint becomes five distinct schools;
  provider families collapse to one (removes catalog tier complexity).
- Gate taxonomy: adds `evidence_grounding` and
  `cross_seat_reconciliation`.
- Run manifest: adds the school `roster` block; the result's
  `trial_manifest` (already required by Result 2.1) carries the
  same-model caveat — no schema change to Result 2.1.
- Conformance checklist additions (appended to §7 of the redesign spec):
  11. All five R1 schools distinct and declared in the manifest.
  12. `evidence_grounding` and `cross_seat_reconciliation` run on every
      R1 round and their outcomes are event-logged.
  13. The manifest carries the same-family caveat on every run.

## 5. Unchanged

- PI seat agent: `--role` already selects the school; the provider
  face follows the run's family binding (OpenAI-compatible chat
  completions, or Anthropic Messages when the bound base URL advertises
  it). No PI changes.
- Round structure, cost profile (7 common / 12 worst), merge rules,
  fail-closed discipline, event ledger, offline verification.
- Host-side contract: STAMMTISCH's A2A adapter and Result 2.1 surface
  need no changes.

## 6. Residual risk (stated, not hidden)

Same-family correlated errors remain possible: one family can share a
blind spot no school framing exposes. The mitigations are (a) the
deterministic gates — the arithmetic and the attestation symmetry are
code-checked, not model-checked; (b) the caveat field forcing every
consumer of the artifact to see the limitation; (c) `INPUT_REQUIRED`
human escalation as the policy-selected fallback when R1 persists in
disagreement through R3. A second vendor remains a drop-in future
option: the seat abstraction never binds the family.
