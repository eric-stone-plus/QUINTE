# Grok Engineering Handoff

Date: 2026-07-15

This is the implementation handoff for QUINTE, HIGHBALL, and the dedicated
Hermes host. It separates verified repository facts from design decisions and
recommendations. The current Codex pass did not implement or commit product
changes.

## 1. Repositories and source of truth

| Product | Development checkout | Current baseline |
| --- | --- | --- |
| QUINTE | `/Users/ericstone/Public/QUINTE` | `dce11ab70e088e784c4117009948956e5b492cea` |
| HIGHBALL | `/Users/ericstone/Public/HIGHBALL` | `0a113d42bb10419c403680255d6837064e7ca09d` |
| Hermes host | `/Users/ericstone/Private/hermes-agent` | `513998b4aa84f7735f3b429360b497b9c125134c` |
| macOS profile | `/Users/ericstone/Private/hermes-technical-profile-mac` | `43d77a7aca3822272df372311d9a8cc90203a417` |
| Windows profile | `/Users/ericstone/Private/hermes-technical-profile-win` | `8bd1d575dff1032ec1bd098f0a1db27e34dc3006` |

Do not develop in `/Users/ericstone/.hermes/hermes-agent`. That is a separate
runtime checkout and must remain operationally isolated.

The QUINTE working tree is intentionally dirty. Preserve it before changing
anything:

- modified: `src/doctor.rs`, `src/model.rs`, `src/run.rs`, `src/store.rs`
- binary diff SHA-256: `d8748118b2583707b9a32b64b8a7ee73afd220799717a795e4d272ed619b7d5c`
- the four-file change is only a partial version migration: 32 insertions and
  31 deletions
- two untracked historical reports are in `assets/`; read
  `RESIDUAL_REPORT_CORRECTIONS.md` before relying on them

Create a branch and save the starting diff as evidence before editing. Do not
reset, clean, stash-drop, force-push, or fold these changes into an unrelated
commit.

## 2. Product decisions (binding)

### Version contract

- The CLI package version is the sole product version source.
- For the next release, QUINTE-owned protocol, schemas, artifacts, packets,
  receipts, CLI envelope, doctor output, and retry/rate state all use `0.1.4`.
- Existing `1.0` values are test-era residue. Do not add compatibility migration
  code for them.
- Keep JSON Schema dialect `https://json-schema.org/draft/2020-12/schema`.
- Do not opportunistically change dependency versions or rewrite history.
- The current four-file diff is not committable alone: Rust producers now emit
  the Cargo package version while schemas, fixtures, tests, README, and specs
  still require or demonstrate `1.0`.

### Residual ownership

- QUINTE owns immutable, typed residuals for one run, exact evidence binding,
  R3 arbitration/merge, and immutable `result.json`.
- HIGHBALL owns cross-run append-only closure/waiver evidence, Action Packets,
  outcome history, routing, and the external action boundary.
- Hermes presents results, gathers human closure or waiver evidence, and calls
  HIGHBALL. It does not mutate QUINTE results.
- Do not add an in-place `quinte residual update/close` workflow.
- Do not hard-code money thresholds, closure TTLs, or shipping-domain residual
  enums into generic QUINTE.

### Product boundary

HIGHBALL must treat `quinte` as one atomic product route. QUINTE alone owns its
lanes, providers, retries, pacing, worker lifecycle, cleanup, artifacts, and
finalization. HIGHBALL may accept or block the product outcome; it must not
reconstruct QUINTE's R1/R2/R3 scheduler.

## 3. Verified release history

Verified with authenticated `gh` on 2026-07-15:

- remote `main` at `dce11ab` passed CI run `29346978694` on all configured
  platforms
- `v0.1.0` is the only GitHub Release; run `29218207576` succeeded
- `v0.1.1`, `v0.1.2`, and `v0.1.3` are tags, but their tag runs
  `29246274532`, `29305574326`, and `29308295624` failed; publish was skipped
- a now-deleted `v0.1.4` tag triggered run `29349832363`; it was cancelled and
  no Release exists

Create a real `CHANGELOG.md`. Describe failed tags as unreleased/failed release
attempts, never as successful releases.

## 4. Confirmed QUINTE defects

All line references are against baseline `dce11ab` plus the preserved local
four-file diff.

1. **Windows descendant-process containment is incomplete.** Adapter commands
   are hidden on Windows but are not assigned to a Job Object
   (`src/adapters.rs:819`). Cleanup relies on `taskkill /T` while the leader PID
   still exists (`src/run.rs:3264`), and post-leader residual cleanup is a no-op
   (`src/run.rs:3280`). A child that exits after starting a descendant can leave
   the descendant holding stdout/stderr pipes.
2. **Claude credential contract drifts off macOS.** The non-macOS path requires
   `ANTHROPIC_API_KEY` (`src/adapters.rs:1490`), while the established Hermes
   route uses the token contract. Resolve this explicitly and test Windows;
   do not silently switch credential semantics.
3. **HM residual evidence references are not checked against the snapshot.**
   Lane outputs call `validate_evidence_refs` (`src/run.rs:1968` and
   `src/run.rs:2026`), including residual closure evidence
   (`src/run.rs:3086`). HM submission validates challenge binding but never
   applies that snapshot-reference validator before merge
   (`src/run.rs:1228`).
4. **Claim and residual IDs are not guaranteed unique.** Schemas validate each
   ID's syntax but not uniqueness (`schemas/lane-output.schema.json:13` and
   `schemas/lane-output.schema.json:18`). Add semantic validation for each
   artifact, including HM/CC verdict residuals.
5. **Residual merge conflict detection is incomplete.** Merge keys by ID and
   only compares `disposition`, `closure_state`, and `finding`
   (`src/run.rs:2977`). Conflicts in severity, type, source, evidence,
   required closure, or scope silently retain the first (HM) value. Duplicate
   IDs within one arbiter are folded through the same path.
6. **R1/R2 residuals can collapse at R3.** Final residuals are built only from
   HM and CC (`src/run.rs:2979`). R1/R2 are referenced in the trial manifest but
   are not unioned into final residuals. If both arbiters omit a high-risk
   earlier residual, the final result silently loses it. Add a conservative
   preservation invariant or an explicit disposition trail.

## 5. Confirmed HIGHBALL gap

HIGHBALL documentation correctly declares QUINTE an atomic product boundary
(`README.md:13` and `README.md:54`). However, the current Action Packet schema,
builder, and validator remain legacy compatibility code that requires R1/R2/R3
phase ledgers (`schemas/action-packet.schema.json:397`). Replace the active
integration with a binding to the atomic QUINTE outcome; keep legacy parsing
only where archived packets genuinely require it.

BANNIN must not own stale-process cleanup. QUINTE exclusively owns processes it
starts. Update any technical-profile copy that still states otherwise.

## 6. Dedicated Hermes host

Target experience: invoking `quinte` enters a dedicated host that cannot modify
SOUL, USER, MEMORY, or skills, while the fork remains maintainable against
upstream.

Verified causes of unwanted self-writing in the current Hermes design:

- memory and skill review are independent controls
- `skills.creation_nudge_interval=15` enables post-turn review
- `skills.write_approval=false` permits direct writes
- enabling approval still writes pending JSON and is not a zero-write mode
- the skills toolset and curator remain enabled
- both the ordinary finalizer and Codex runtime can trigger background review

Implement one authoritative `self_improvement.enabled=false` host policy, cover
both completion paths, disable curator and mutable memory/skills toolsets, and
verify before/after directory hashes. Remove Minecraft only from the dedicated
distribution manifest; do not perform broad speculative source deletion.

Hermes development remotes are already arranged as:

```text
origin   -> eric-stone-plus/hermes-agent
upstream -> NousResearch/hermes-agent (push disabled)
main     -> origin/main
```

## 7. Recommended implementation sequence

1. Preserve and document the dirty QUINTE baseline.
2. Complete the `0.1.4` version cohort across all producers, schemas, fixtures,
   tests, README, specs, doctor, receipts, and envelopes in one change.
3. Add ID uniqueness, HM evidence-reference validation, conservative residual
   preservation, and complete merge-conflict handling with adversarial tests.
4. Implement Windows Job Object containment and execute the real-machine plan
   in `WINDOWS_REAL_MACHINE_ACCEPTANCE.md`.
5. Change HIGHBALL's active Action Packet path to bind an atomic QUINTE outcome
   and keep closure history append-only outside `result.json`.
6. Add the dedicated Hermes no-self-improvement policy and hash-based E2E test.
7. Add `CHANGELOG.md`, run the full platform matrix, tag only after green CI,
   and verify all release assets and checksums.

Keep each phase in a separate, reviewable commit. Do not mix an upstream merge,
generated documentation noise, product fixes, and release tagging.

## 8. Minimum verification

```bash
cd /Users/ericstone/Public/QUINTE
git status --short --branch
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features

cd /Users/ericstone/Public/HIGHBALL
git status --short --branch
# Run every repository-provided validator/test command documented by HIGHBALL.

cd /Users/ericstone/Private/hermes-agent
git status --short --branch
git remote -v
git branch -vv
scripts/run_tests.sh tests/test_background_review_session_isolation.py
scripts/run_tests.sh tests/hermes_cli/test_fork_merge_safety.py
scripts/run_tests.sh tests/hermes_cli/test_cmd_update.py
```

Also prove that a dedicated QUINTE host session leaves the protected
SOUL/USER/MEMORY/skills trees byte-for-byte unchanged.
