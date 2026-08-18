# Finance School Review Redesign Memo

Status: **Accepted for a separate implementation session**

Decision date: 2026-08-15

Implementation status: **Not started**

## Decision

QUINTE will support a doctrine-bound five-school review profile for financial
research. The five R1/R2 lanes will no longer be anonymous, interchangeable
opinions under this profile. Each lane will have one fixed epistemic role:

1. factor and risk-model review;
2. event-driven review;
3. fundamental and supply-chain review;
4. trend and technical-regime review; and
5. market-microstructure review.

The econometric/statistical-arbitrage calculation remains outside these five
lanes. It is the primary numerical artifact produced by GALAHAD from validated,
hash-bound market data. QUINTE reviews that artifact for confounders,
contradictions, mechanism failures, regime failures, and invalidations. It does
not recalculate prices, invent missing evidence, or promote a failed numerical
result.

This is a conjunctive adversarial review, not a committee vote. Agreement among
lanes cannot turn `ABSTAIN`, `quarantined`, `descriptive_only`, or `expired`
primary evidence into an accepted finding. One material unresolved blocker is
enough to prevent publication of the affected claim.

## Why this design

The existing five-lane architecture is useful for independent first passes and
anonymized rechecks, but generic perspectives can overlap, omit an important
discipline, or converge through shared model bias. Fixed school authority makes
coverage auditable and makes every lane falsifiable. It also prevents a model
from treating five generated opinions as five independent market observations.

The design preserves QUINTE's single-model-family invariant. School separation
is an epistemic and contract-level perturbation, not evidence that the model
outputs are statistically independent. The result manifest must continue to
declare same-family error correlation as a contamination risk.

## Product boundaries

```text
validated source artifacts
  -> GALAHAD deterministic calculations
  -> hash-bound primary CalculationArtifact
  -> QUINTE five-school adversarial review
  -> deterministic blocking/publication posture
  -> STAMMTISCH offline-verifiable bundle
```

The boundaries are strict:

- GALAHAD owns market-data admission, return derivation, statistics,
  multiplicity, out-of-sample tests, expiry, and the primary numerical status.
- A GALAHAD interpreter may explain an accepted artifact, but cannot change its
  deterministic status.
- QUINTE owns school-specific adversarial review and typed residuals. It cannot
  create or repair numerical evidence.
- STAMMTISCH owns orchestration, deterministic gates, receipt chains, and
  offline bundle verification.
- A generated financial daily, edited summary, or article count is discovery
  material only. It is never OHLCV, a CalculationArtifact input, or a vote.

## School authorities

Every profile binds an exact school identifier, accepted evidence classes,
questions, forbidden claims, applicability rule, and blocking policy before a
run starts. A lane cannot rewrite its own authority.

| School | Required review | Permitted evidence | No authority to |
| --- | --- | --- | --- |
| Factor and risk model | Test whether market, style, liquidity, volatility, FX/rates, index overlap, or other pre-registered exposures explain the claimed residual relation | Validated bars and frozen point-in-time factor/control artifacts | Originate direction, use downstream outcomes as controls, or rescue a failed primary estimate |
| Event driven | Find point-in-time events, confounders, and immediate invalidations | Independently reacquired original-source artifacts with publication, availability, retrieval, and cutoff bindings | Supply prices, returns, statistics, sample size, effect direction, or numerical lineage |
| Fundamental and supply chain | Test whether taxonomy and comparable operating evidence support or contradict the proposed economic mechanism | Point-in-time classifications and primary disclosures with unit, period, availability, and restatement lineage | Establish a price edge, backfill current classifications, or claim causality by narrative alone |
| Trend and technical regime | Diagnose persistence, decay, breaks, volatility state, crowding, and reversal risk | Deterministic indicators derived from the same validated bars and calendar as the primary calculation | Confirm an otherwise rejected edge or act as an independent vote |
| Market microstructure | Test intraday price discovery, liquidity, timing, and order-flow explanations when the profile's intraday evidence gate is met | Validated completed intraday bars and venue-aware microstructure artifacts | Infer missing bars, use daily OHLCV as intraday evidence, or bypass a daily `ABSTAIN` state |

The financial doctrine, not QUINTE core, defines the domain-specific evidence
classes and applicability policies. QUINTE core enforces the pinned profile and
closed output contracts.

## Applicability is preregistered

Every claim declares which schools are mandatory, conditional, or explicitly
out of scope before lane execution. This prevents convenient after-the-fact
waivers.

- A mandatory school returning `insufficient_evidence`, `contradicted`,
  `quarantined`, or `expired` blocks the claim.
- A conditional school may return `not_applicable` only when a deterministic
  profile predicate is false. For example, microstructure may be
  `not_applicable` to a daily-only claim when no intraday claim is made.
- Once an intraday timing, liquidity, or execution claim is present, the
  microstructure lane becomes mandatory and its data gate must pass.
- No lane or arbiter may change applicability after reading the findings.

## Proposed versioned contracts

Do not mutate Result 2.1 or Manifest 2.0 in place. The implementation session
must introduce new revisions and retain explicit read-only compatibility for
old completed runs.

### Review profile

A new canonical profile artifact should include at least:

- profile schema/version and profile identifier;
- exact five school definitions and fixed lane mapping;
- accepted and forbidden evidence classes per school;
- claim-to-school applicability predicates;
- materiality and blocking rules;
- cutoff, freshness, and expiry policy;
- allowed primary-artifact schemas and revisions;
- hash-domain registry; and
- a domain-separated profile digest.

Suggested digest domain:
`quinte.finance-review-profile.v1`.

The profile bytes and semantic digest must both be bound into the run manifest.

### School lane output

Each R1/R2 output should be closed and typed, including:

- lane-output revision, run id, school id, phase, and profile digest;
- bound claim ids and input artifact references;
- evidence items actually used, each with exact-byte and semantic digests;
- tested alternatives, confounders, falsifiers, and invalidations;
- explicit limitations and missing evidence;
- disposition: `clear`, `contradicted`, `insufficient_evidence`,
  `quarantined`, `expired`, or profile-authorized `not_applicable`;
- typed residuals with severity, affected claim ids, closure state, and source;
  and
- a domain-separated output digest.

Suggested digest domain:
`quinte.school-lane-output.v1`.

Free-form prose may explain a typed finding but cannot carry the only copy of a
decision, evidence reference, or blocker.

### Finance review result

The next result revision should bind:

- the exact primary CalculationArtifact bytes and semantic digest;
- validated-bar, calendar, session-map, configuration, code, and environment
  digests already carried by the primary artifact;
- the review-profile bytes/digest;
- exactly one final school disposition for every required school;
- all R1/R2 lane output digests and both arbiter input/output digests;
- per-claim applicability and blocking outcomes;
- active invalidations and expiry session;
- a deterministic publication posture; and
- the usual route/model-family/contamination manifest.

The publication posture is not model-authored. QUINTE code derives it from the
typed inputs:

```text
primary status is accepted and unexpired
AND required provenance is complete
AND every mandatory school is clear
AND every conditional school is clear or deterministically not_applicable
AND no material residual is open
AND no active invalidation exists
    -> PUBLISH_BOUNDED
otherwise
    -> ABSTAIN
```

There is no `3-of-5`, weighted score, confidence average, or arbiter override.
The result may summarize supporting evidence, but support never promotes the
primary status.

Suggested domains:

- `quinte.finance-evidence-index.v1`
- `quinte.finance-review-result.v1`
- `quinte.finance-publication-posture.v1`

All domain-separated digests use:

```text
SHA-256(domain ASCII || NUL || canonical UTF-8 JSON)
```

Exact artifact bytes also receive an ordinary SHA-256. The two hashes serve
different purposes and must not be conflated.

## R1, R2, and arbiter behavior

### R1: school-isolated first pass

Each school receives the same immutable evidence index plus its own pinned
authority packet. It cannot see other lane outputs. It must attempt to falsify
the affected claims from within its discipline, report missing evidence, and
emit the closed school-lane contract.

### R2: anonymized cross-examination

Each school receives an anonymized digest-bound packet of all R1 claims and
residuals. It may challenge contradictions and unsupported inferences, but its
final disposition remains within its original authority. R2 is not a consensus
round and cannot change the profile or evidence cutoff.

### Arbiters

Arbiters reconcile identifiers, contradictions, duplicate residuals, and scope.
They may not close a material residual without bound closure evidence, alter a
school disposition, waive applicability, or promote the primary numerical
status. The deterministic merger computes the final publication posture after
the arbiter artifacts have passed schema and binding checks.

## Time and market-data discipline

Financial review inputs must carry an `as_of` cutoff, an evaluation session,
and an expiry session. Session labels come from pinned exchange schedules, not
weekday arithmetic. A completed US session maps to the next actual target
market session using a separately hashed schedule-derived map. Holidays,
daylight-saving changes, half days, collisions, and missing sessions fail
closed or become explicit exclusions.

QUINTE cannot browse for a replacement number or accept an agent's remembered
market value. Any newly discovered fact must first become a valid immutable
source artifact through the owning product's acquisition contract. Evidence
available after the cutoff is narrative-only unless a new review run is
started with a later cutoff.

## Migration plan for the implementation session

1. Freeze this memo as the architecture input and inventory every Result 2.1,
   Manifest 2.0, lane-output, arbiter, policy, and host/A2A dependency.
2. Add the versioned finance review-profile schema and synthetic fixture before
   runtime code.
3. Add the versioned school-lane and finance-result schemas; keep legacy
   readers explicit and immutable.
4. Extend policy/model types so the five lane bindings carry fixed school ids
   and the exact profile digest.
5. Bind the profile and evidence index into snapshot, events, manifest, R1/R2
   packets, arbiter packets, and final result.
6. Replace recommendation-based merge semantics for this profile with the
   deterministic publication-posture function.
7. Update the A2A artifact contract without changing A2A transport semantics.
8. Add a new STAMMTISCH deterministic gate for the versioned school result.
   Do not weaken or silently reinterpret the existing Result 2.1 gate.
9. Add the GALAHAD financial doctrine profile and synthetic, redistributable
   fixtures. Do not embed private data or machine-specific configuration.
10. Run schema, Rust, host/A2A, tamper, resume/reconcile, and STAMMTISCH offline
    bundle verification suites before any live invocation.

## Required adversarial tests

The new session is not complete without tests proving that:

- a profile has exactly five unique required school bindings;
- duplicate, missing, swapped, or self-declared school ids are rejected;
- profile bytes, semantic digest, and run bindings cannot drift;
- exact input bytes cannot change behind an unchanged semantic claim;
- one, four, or five supportive lanes cannot promote failed primary evidence;
- one open material blocker forces `ABSTAIN` regardless of arbiter prose;
- an arbiter cannot rewrite a school disposition or applicability;
- `not_applicable` is accepted only under the preregistered predicate;
- daily bars cannot satisfy the intraday/microstructure gate;
- financial-daily/news summaries cannot enter numerical evidence lineage;
- post-cutoff events cannot retroactively affect the review;
- stale, expired, quarantined, or tampered primary artifacts fail closed;
- R1 isolation and R2 anonymized information sets remain intact;
- same-family contamination remains explicit in the manifest;
- crash/reconcile/resume preserves every school binding and digest; and
- a STAMMTISCH-exported bundle can be verified offline with no QUINTE runtime.

## Non-goals

- No trade recommendation, position, order, or automatic execution path.
- No replacement of GALAHAD's numerical calculation kernel.
- No claim that five lanes are five independent data sources or model families.
- No hard-coded semiconductor symbols, private endpoints, credentials, or host
  paths in public code.
- No live rollout, scheduler, or mutation of historical completed runs in the
  redesign session.

## New-session handoff

Start a fresh session in the QUINTE repository and read, in order:

1. `AGENTS.md`;
2. this memo;
3. `specs/PROTOCOL.md`;
4. `specs/HOST.md`;
5. `schemas/result.schema.json`, `schemas/run-manifest.schema.json`, and
   `schemas/lane-output.schema.json`;
6. the corresponding model, run, merge, host/A2A, contract, and test code; and
7. STAMMTISCH's `docs/architecture.md` and `docs/protocol-layer.md` before any
   cross-repository contract change.

The first implementation response should be a compatibility and contract
change map, not code. It must identify the exact revision strategy, affected
files, migration tests, and rollback boundary. No existing schema revision is
edited in place.
