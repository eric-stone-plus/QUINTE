# QUINTE Protocol Redesign — From-Scratch Re-architecture

> **Design record.** QUINTE is being rebuilt from scratch. This document
> fixes the new architecture before any implementation: what is
> re-decided, why, and what stays. Decisions below were made 2026-08-13;
> implementation is deliberately deferred until the vendor/model
> re-selection, but the design is settled and every consumer side
> (notably STAMMTISCH's A2A adapter) can build against it now.
>
> The in-service 0.2.x runtime and [PROTOCOL.md](PROTOCOL.md) remain the
> implemented reality until cutover.

## 1. Decisions

| # | Decision | Direction |
| --- | --- | --- |
| D1 | Product identity | QUINTE becomes a **generic multi-agent review orchestrator**. Rounds, seat counts, domain schemas, gates, and merge rules are policy — a doctrine pack. The quant review slice is the first pack, not the product. |
| D2 | Seat model | Every seat is an **external A2A v1.0 endpoint**. One protocol everywhere: QUINTE is an A2A *server* to hosts ([HOST.md](HOST.md)) and an A2A *client* to its seats. No vendor, framework, or model is bound. v1.0 is the canonical seat wire (version header, `agent-card.json`, `TASK_STATE_*` spellings); the seat client keeps a read-only 0.2.x fallback (legacy `agent.json` card path, lowercase states) so PI-generation seats stay reachable during migration. |
| D3 | Round structure | **Adaptive**: R1 always runs; R2 rechecks only the outputs R1 actually contested; R3 dual arbitration always runs. Deterministic escalation rule (code, never model judgment), recorded in the event ledger. |

The driving constraints: no model or agent vendor is bound and no token
plan exists, so seat cost is real; the structure must buy the most
epistemic value per invocation without weakening the evidence discipline.

## 2. What stays (non-negotiable, carried over from PROTOCOL.md)

1. The run is a deterministic state machine; `events.jsonl` is the
   authority and the manifest is a projection.
2. Closed schemas on every input and output; unknown fields fail the
   gate; normalization only where PROTOCOL.md already prescribes it, and
   never silently.
3. Fail closed: a missing seat, a binding drift, an unparsable output,
   or an unresolved evidence reference means no verdict — never a
   reduced-path answer.
4. Results are evidence. QUINTE never authorizes a protected action.
5. The merge is deterministic code, not a model. Dissent is recorded,
   not voted away.
6. Offline verification: a bundle of events, receipts, and artifacts
   re-verifies without any product installed.
7. One active run per orchestrator (the A2A `busy_run` refusal,
   [HOST.md](HOST.md) §3).

## 3. The new architecture

```text
              ┌─────────────────────────────────────────┐
  hosts ─────▶│  QUINTE orchestrator                    │
 (A2A client) │  (A2A server: HOST.md)                  │
              │    run state machine · event ledger     │
              │    adaptive round policy · merge        │
              │    seat resolution · evidence gates     │
              └──────────────┬──────────────────────────┘
                             │ A2A v1.0 client (per seat)
       ┌──────────┬──────────┼──────────┬──────────┐
       ▼          ▼          ▼          ▼          ▼
   seat A     seat B     seat C     seat D     seat E
 (any A2A   (any A2A   (any A2A   (any A2A   (any A2A
  agent)     agent)     agent)     agent)     agent)
```

One wire protocol end to end. The seats are opaque agents — QUINTE sees
Agent Cards, tasks, and artifacts, never vendor internals. Seat
heterogeneity is the point: with no bound vendor, R1 lanes can be
genuinely cross-vendor, which makes agreement real confirmation (the
inverse of the old same-family caveat — agreement across distinct
providers is recorded as evidence).

## 4. Round structure (D3) — adaptive, deterministic

### R1 — first-pass review (always)

`n` isolated first-pass seats (policy default 5). Each seat receives the
same bounded task packet: the closed-schema Brief plus the evidence
snapshot, and returns one closed-schema lane output. Seats execute
concurrently up to the policy limit and never read one another's output.
A policy diversity constraint (e.g. at least two distinct provider
families across the roster when `n >= 3`) prevents a degenerate
all-same-vendor run; the resolved roster is recorded in the run
manifest.

### Divergence gate (deterministic escalation rule)

After all R1 outputs pass their schema gates, a coded predicate — never
a model — decides which outputs are contested:

- any two lanes disagree on the normalized recommendation, **or**
- any two lanes disagree on the disposition (open/closed) of the same
  residual id, **or**
- any lane flags unsupported confidence or explicitly requests recheck.

The predicate, its inputs, and its outcome are event-ledger facts.

### R2 — pseudonymized recheck (only when contested)

Contested R1 outputs are pseudonymized (the current PROTOCOL.md labeling
discipline) into a recheck packet with the original Brief. Recheck seats
(either the same roster or a policy-declared recheck roster) verify each
contested output independently. R2 seats never see uncontested outputs'
identities; the packet is input-shaped exactly as today.

R1 agreement across the roster skips R2 entirely — cross-vendor
agreement is the strongest signal the run can produce, and recheck value
concentrates where R1 actually diverged. Cost per run: `n + k + 2`
invocations, where `0 <= k <= n` and `k = 0` is the common path.

### R3 — dual arbitration (always)

Unchanged from PROTOCOL.md in spirit: a Counterpart Arbiter and a
Primary Arbiter each return a closed-schema verdict; the Primary verdict
is bound by the single-use challenge (run id, nonce, policy digest,
evidence-packet digest, action scope, issue/expiry). The manual-handoff
mode surfaces as the A2A `INPUT_REQUIRED` interruption ([HOST.md](HOST.md)
§5). Two arbitration seats are the cheapest insurance the design has
against a single bad verdict; they are not on the re-evaluation table.

### Merge

Deterministic merge over R1 (+R2 rechecks) + the two R3 verdicts.
Unequal recommendations are recorded in `dissent`; residuals with
conflicting dispositions stay `unresolved` and `open`. R2 recheck
outcomes resolve their contested outputs; persistent disagreement after
recheck is recorded, not averaged.

### Cost profile

| Shape | Invocations per run |
| --- | --- |
| R1 agreement (common path) | `n + 2` — default 7 |
| Contested (worst case) | `n + k + 2` — default 12 |
| Old fixed 5+5+2 | 12 every run |

## 5. Seats: policy, resolution, binding (D2)

- **Policy declares requirements, never endpoints.** Each seat slot
  declares capability tags, budget tier, and diversity constraints.
- **Per-run resolution.** At run creation the orchestrator resolves each
  slot to a concrete A2A endpoint from the seat catalog (a registry the
  operator maintains), fetches and validates its Agent Card, and records
  the endpoint plus the card's canonical digest in the run manifest.
  Resolution failures are preflight failures — fail closed, no
  substitution.
- **Binding.** Every seat invocation is receipted: endpoint, card
  digest, task id, verbatim upstream payload digest. Card drift
  mid-run halts the run. This mirrors the host-side binding discipline
  in [HOST.md](HOST.md) §2 and STAMMTISCH's `a2a.invocation.v1` receipts
  (`eric-stone-plus/STAMMTISCH`, `docs/protocol-layer.md`).
- **Retries.** Per-seat attempt budgets stay policy-driven; the trusted
  transient conditions are re-declared per seat adapter because they are
  vendor-specific (the PROTOCOL.md taxonomy is the reference for the
  legacy adapters, not a promise for future seats).

## 6. Domain as doctrine pack (D1)

Everything domain-specific leaves the core:

```text
doctrine pack =
  round policy        (n, recheck roster, concurrency, pacing, timeouts)
  seat policy         (slot requirements, diversity constraints, budgets)
  schemas             (brief, lane output, verdict, result, challenge)
  gates               (schema checks, thresholds, receipt flags)
  merge rules         (residual disposition logic)
```

The quant review slice becomes the first pack — the successor to the
current galahad/quant schemas — and a new domain is a new pack with its
own conformance suite. This is the same shape as STAMMTISCH's doctrine
packs; the orchestrator core is domain-free.

## 7. Conformance checklist (target)

1. Agent Card served per [HOST.md](HOST.md) §2; seat cards validated per
   §5 binding.
2. One run = one A2A task; `busy_run` refusal; state mapping exact.
3. R1 runs the policy roster; diversity constraint violations fail
   preflight.
4. The divergence predicate is pure code over R1 outputs and its
   outcome is event-logged; identical R1 inputs replay identically.
5. R2 runs only on contested outputs; the recheck packet carries no
   route identities.
6. R3 challenge: consumed once; expiry, replay, mismatch refused.
7. Merge is deterministic; replay of the same run directory yields the
   byte-identical result.
8. Seat drift (card digest change) mid-run halts with receipts intact.
9. Bundle verify re-checks every receipt, artifact digest, and gate
   offline.
10. Legacy 0.2.x CLI behavior is unchanged until cutover.

## 8. Migration and rollout

- The 0.2.x runtime, the CLI host surface
  ([HOST-CLI-LEGACY.md](HOST-CLI-LEGACY.md)), and the installed
  deployments stay untouched until the new core passes the conformance
  checklist.
- The new core is built as a fresh crate alongside the legacy one;
  cutover is a binary swap plus a state migration path for archived
  runs (read-only legacy inspection stays available).
- The host side already exists: STAMMTISCH's wire adapter consumes the
  [HOST.md](HOST.md) surface today and needs no changes.

## 9. Open items — resolved 2026-08-18 by the single-vendor decision

The vendor re-selection question is closed: **one family (DeepSeek,
official direct API), no second-vendor budget.** See
[SINGLE-VENDOR-DOCTRINE.md](SINGLE-VENDOR-DOCTRINE.md), which replaces
the D2 cross-vendor diversity constraint with the same-family
multi-school doctrine and resolves these items:

- Seat catalog: single-family roster collapses the catalog to the
  school registry (five distinct schools, digest-pinned prompts in the
  doctrine pack); no discovery service needed in the first cut.
- Cost accounting: one provider simplifies per-run budget caps to a
  per-invocation attempt budget (still policy-driven).
- Timeout/retry calibration: homogeneous seats — one calibration set
  instead of per-vendor sets.
- `INPUT_REQUIRED`: unchanged scope (arbiter handoff) for this cut.
