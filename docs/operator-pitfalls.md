# QUINTE operator & product pitfalls (local retrospective)

**Audience:** implementers and outer hosts (campaign coordinators, systemd timers, Hermes wrappers).  
**Scope:** QUINTE 0.2.x single-family host + adapters. Grounded in specs, shipped code, and real `~/.quinte` runs—not a chat log.  
**Non-goal:** empty MiMo re-burn to “confirm” these items.

---

## Protocol success ≠ research readiness

| Gate | What it means | What it does **not** mean |
|------|----------------|---------------------------|
| `manifest.status=completed` | Protocol run finished under fail-closed phase machine | Evidence was sufficient for a product decision |
| `host inspect` → `result.verified=true` | Result bytes match manifest digest and validate against a registered result contract (`specs/HOST.md` “Inspect Result”) | Decisive evidence was present in the snapshot |
| `result.actionable=true` | Result uses the **current** result contract revision only | External write/submit/deploy is authorized |
| Matching `runtime_sha256` | Binary/policy pin integrity for the host | Model output quality or schema thrash risk is zero |

**Authoritative wording (shipped):** `specs/HOST.md` states that `verified=true` is product-integrity and `actionable=true` only means the current contract revision—not authorization for protected actions.

**Operator rule (no empty burn):** If failures are schema/format thrash (`schema validation failed`, illegal residual/claim `id`, unresolvable `evidence_refs`) on a **thin** evidence pack (tiny single snapshot file, empty `attachments`), do **not** auto-retry until accept. Cap named retries (suggested budget: **≤2–3** explicit attempts). Prefer: leave failed with history, fix the Brief/evidence, or accept an honest “evidence-gap audit” once and stop.

---

## Pitfall catalog

Each entry: **symptom → root cause → detection → prevention → recovery (without blind multi-lane re-burn).**

### P1 — Incomplete run directories poison the entire host

**Symptom.** Every `host status` / `preflight` / `start` fails with:

```text
QUINTE host cannot trust run directory <state-root>/runs/<id>; reconcile manually:
cannot read …/manifest.json: No such file or directory
```

even for unrelated healthy runs.

**Root cause.** Host enumeration is fail-closed: every directory under `runs/` must load a valid manifest (`src/host.rs`, trust path around the `cannot trust run directory` message). Incomplete trees (e.g. bare `quinte run` aborted mid-create, or concurrent launcher left only `input/`/`lanes/` shells) are **not** “no active run.”

**Detection.**

- `ls <state-root>/runs/*` with missing `manifest.json`
- Preflight/status returncode ≠ 0 with empty JSON stdout
- Campaign/coordinator HALTED on “malformed, mismatched, or nonzero receipt”

**Prevention.**

- Route all launches through `quinte host start` (acquires host lock). Do not mix bare `quinte run` with a host-disciplined campaign (`specs/HOST.md` one-active vs legacy `quinte run`).
- Outer hosts must not leave partial UUIDv7 directories under `runs/`.

**Recovery (no burn).**

1. Stop concurrent bare launches.
2. Quarantine incomplete dirs (e.g. move under `~/.quinte/quarantine/<stamp>/`)—do not invent manifests.
3. Re-run `host preflight --json` then `host status` on the real run.
4. If a campaign wrote HALTED with empty `stdout_json` solely due to this poison, archive that marker only after a **fresh** clean status receipt (operator CAS), then continue with explicit resume/retry policy—not a forge of ledger fields.

**Anchors.** `src/host.rs` (`cannot trust run directory`); `specs/HOST.md` “Enumeration is fail-closed”; local quarantine trees under `~/.quinte/quarantine/`.

---

### P2 — Dual launch surfaces break the one-active invariant

**Symptom.** Second launch fails with “active QUINTE run exists”; or two workers race; timer and a manual supervisor both call launch; `host start` refuses while a stray process still holds a non-terminal run.

**Root cause.** One-active is a **host** resource rule under `host start`’s launch lock. Legacy `quinte run` “does not acquire the host lock” (`specs/HOST.md`). Two surfaces are not serialized. Outer systemd timers plus ad-hoc supervisors can also double-launch if both treat “idle” without holding the same discipline.

**Detection.**

- `host status` / reconcile: `active_run_ids` non-empty while another process starts
- Start receipt vs unexpected second run_id under `runs/`
- Campaign log: `launch_fail … active QUINTE run exists: <run_id>`

**Prevention.**

- Single launch path: only `quinte host start` (or one outer mutex wrapping it).
- Pause timers while an operator-driven finish loop is active; reverse when idle.
- Never assume preflight “ready” reserves the slot (`specs/HOST.md`: preflight is advisory, not a reservation).

**Recovery (no burn).**

- `quinte host reconcile --json` (observe/bind only—does not advance or retry).
- Wait for the surviving run to terminal; `inspect` if completed.
- Do not kill by process name as a substitute for `quinte cancel` (`specs/HOST.md` lifecycle).

**Anchors.** `specs/HOST.md` “Detached Start and One-Active Rule”; AGENTS.md “同时只跑 1 个 active run”.

---

### P3 — Complete-but-schema-invalid model JSON is permanent (not “try harder”)

**Symptom.** Lane fails with `schema validation failed` (illegal `id`, wrong types for `uncertainties`/`limitations`, missing residual fields). Outer loops re-run the **same** Brief dozens of times hoping for a green accept.

**Root cause.** Product design: a payload that parses as JSON but fails the typed schema is a **permanent, non-retryable contract failure** (`src/adapters.rs` comments on permanent-vs-transient; AGENTS.md: “完整但 schema 无效的 JSON 候选 = 永久失败不重试”). Transient retries exist for empty/truncated streams; schema thrash is not in that class. Bounded adapter coercion (e.g. quote unquoted keys, null stripping) may help **near-valid** shapes only; required fields and id patterns still reject.

**Detection.**

- Error message contains `schema validation failed` or id pattern `^[A-Za-z0-9._-]{1,64}$`
- `retryable: false` on host status error object
- Attempt history grows without brief/evidence changes

**Prevention.**

- Cap explicit outer retries (≤2–3) for the same brief digest.
- Prefer fixing prompts/policy/adapters with **unit tests** against real failure shapes (see `src/adapters.rs` tests for schema validation and unquoted-key repair) rather than live multi-lane burns.
- Use `quinte validate --kind brief|verdict` before submit paths.

**Recovery (no burn).**

- Inspect the failed lane artifact under `runs/<id>/lanes/…` once; classify permanent vs transient.
- If permanent schema on thin evidence: **leave failed**, document, change Brief or product—do not `--retry-batch` until accept.
- Optional product work: extend local normalization only when tests prove a repeatable shape (historical: unquoted object keys in arbiter streams—`e88eb3d` / quote path in `adapters.rs`).

**Anchors.** `src/adapters.rs` permanent contract failure comments; AGENTS.md design decision; failure class in manifests with `retryable: false`.

---

### P4 — Thin evidence packs still reach protocol accept (honest audit, not readiness)

**Symptom.** Run is `completed` + inspect `verified`/`actionable`, pin matches—but `result.summary` / residuals say decisive evidence is missing, attachments empty, recommendations are “block until evidence injected.” Operators treat gate pass as “decision ready” and re-burn for a “better” answer.

**Root cause.** Protocol and host integrity check structure and digests, not domain sufficiency. A single ~1–2 KB card-metadata snapshot with empty `attachments` is enough to complete R1→R3 while every party converges on **evidence-gap**. That is correct fail-closed research honesty, not a product bug—and not improved by retries.

**Detection (before or after launch).**

```text
# After start or on an existing run:
wc -c runs/<id>/input/snapshot/**/*
# snapshot-manifest: entries count, attachments: []
# result.recommendation / residuals: evidence-gap, CRITICAL, decisive_evidence
```

Real examples (local state root):

| run_id | snapshot | attachments | quality signal |
|--------|----------|-------------|----------------|
| `019fdd81-d9ed-7713-8a85-c8451f46e5f9` | 1 file, **1861** B | 0 | Batch-18 accept after heavy retries; recommendation blocks on decisive_evidence injection |
| `019fde04-17ca-7802-81a3-9e643b2bf520` | 1 file, **1696** B | 0 | Batch-28 CRITICAL evidence gap; no pole into final materials |
| `019fde0f-d307-77a2-bae3-2a92062d55df` | 3 files, **6348** B | 0 | Quant fin-daily review: real sources/outcomes/requirements; actionable P0 removals |

**Prevention.**

- Pre-flight the Brief’s `evidence_roots`: require real artifacts for claimed decisive evidence, or accept that the product is an **evidence audit**.
- Prefer productive Briefs with multi-file roots under real project trees (quant/finance/shipping) when the goal is decision support—not contest card metadata alone.
- Do not use multi-attempt re-burn to “fill” missing evidence the model cannot invent under policy.

**Recovery (no burn).**

- Keep the completed run as the durable audit.
- Close residuals offline (attach documents, amend only via supported primary-arbiter paths if appropriate).
- Start a **new** Brief only after evidence exists—not the same thin pack.

**Anchors.** Local runs above; `specs/HOST.md` verified/actionable definitions; `result.json` residuals with `residual_type` / empty decisive data.

---

### P5 — Outer campaign HALTED on host receipt poison ≠ protocol failure of the named batch

**Symptom.** Campaign marker `HALTED.json` cites batch N / run R with reason “host status returned a malformed… receipt”, while the true stderr names **another** incomplete run directory. Resume tools that require a clean `stdout_json` object refuse to archive the marker.

**Root cause.** Outer coordinators halt on any non-JSON or nonzero host observation. Fail-closed host trust (P1) surfaces as empty stdout + parse error. The durable marker may bind a running/failed batch that is not the poison directory—integrity recovery must not confuse the two.

**Detection.**

- HALTED `receipt.process.stdout_json` is null; `parse_error` set; stderr mentions a different run_id’s missing `manifest.json`
- Ledger still shows batch as `running` while host cannot observe any run

**Prevention.**

- Keep `runs/` clean (P1); never start host-polled campaigns while bare launches can create incomplete dirs.
- Outer resume paths should distinguish: (a) terminal failed batch with clean status receipt vs (b) observation infrastructure failure.

**Recovery (no burn).**

1. Fix host trust (quarantine incomplete dirs).
2. Obtain a **clean** `host status --json` for the campaign run_id.
3. If the batch is terminal failed/completed, let the coordinator materialize a proper receipt-bound HALTED or accept path.
4. Archive observation-only poison markers with operator review + CAS; never hand-edit ledger acceptance fields.

**Anchors.** `src/host.rs` trust enumeration; campaign patterns under private runtime `contest_campaign*.py` (outer product); `~/.quinte/quarantine/*`.

---

### P6 — Pin / binary drift between PATH and campaign binding

**Symptom.** Runs complete under an unexpected `runtime_sha256`; inspect proofs from an older pin no longer match; “works on my PATH” differs from systemd `QUINTE_BIN`.

**Root cause.** Multiple installed binaries (`~/.local/bin/quinte` vs pinned `runtime/bin/quinte-0.2.3-…`). Host receipts embed `runtime_sha256`. Mixing pins invalidates cross-run comparisons and confuses migrate history.

**Detection.**

- Compare `sha256sum $(which quinte)` vs campaign `runtime_binding.runtime_sha256`
- Manifest `runtime_sha256` differs across consecutive “same campaign” runs

**Prevention.**

- Export `QUINTE_BIN` + `QUINTE_RUNTIME_SHA256` for every host command in the campaign environment.
- Prefer pin-aware migrate receipts before switching binaries (`specs/HOST.md` reinstall guidance).

**Recovery (no burn).**

- Reconcile/inspect with the **historical** pin for old runs; do not re-run to “fix” pin.
- Document migration; keep old binaries until inspect proofs are no longer needed.

**Anchors.** `specs/HOST.md` runtime digest fields; manifests under `~/.quinte/runs/*/manifest.json`.

---

### P7 — `evidence_refs` must be exact snapshot URIs (absolute paths are rejected)

**Symptom.** R1/R2 fail with unresolvable evidence reference (e.g. invented paths, `snapshot-manifest.json` bare name, fragment suffixes the manifest does not list).

**Root cause.** Protocol contract: non-empty `evidence_refs` / `closure_evidence` must match `input/snapshot-manifest.json` entries exactly (`AGENTS.md`; lane prompt construction in `src/adapters.rs`). Absolute paths and free-form strings are invalid.

**Detection.** Host/lane error: `unresolvable evidence reference`; schema or evidence validation stage.

**Prevention.** Models must copy `snapshot://…` strings from the manifest; outer Briefs should not encourage path inventiveness. Validate residuals before PA submit where tools allow.

**Recovery (no burn).** One bounded retry only if the failure was transient truncation; if the Brief has no usable snapshot entries, fix evidence packaging offline.

**Anchors.** `AGENTS.md` residual `evidence_refs` rule; `src/adapters.rs` phase prompt text for snapshot-manifest refs; `specs/PROTOCOL.md` evidence reference rules.

---

## Suggested operator checklist (pre-launch)

1. `QUINTE_BIN` pin equals intended `runtime_sha256`.
2. `host preflight --json` ok; `active_run_ids` empty.
3. No incomplete dirs under `runs/` (no missing `manifest.json`).
4. Brief `evidence_roots` contain the artifacts the question claims; or accept audit-only outcome.
5. Single launcher owns the next `host start`.
6. Retry budget written down (≤2–3); schema permanent failures stop the loop.

---

## Related in-repo material

| Path | Role |
|------|------|
| `specs/HOST.md` | Host boundary, one-active, inspect verified/actionable, reconcile |
| `specs/PROTOCOL.md` | Phase machine, evidence_refs, lane contracts |
| `specs/CLI.md` | CLI surface, orphan handling |
| `src/host.rs` | Fail-closed run-dir trust |
| `src/adapters.rs` | Intake, permanent vs transient schema failures, key repair |
| `AGENTS.md` | Operator discipline, residual rules, non-retry of invalid JSON |
| `docs/windows-powershell-development-log.md` | Historical Windows platform notes (separate from this retro) |

---

## Document history

- 2026-08-08: Initial durable retrospective from 0.2.3 host/adapters behavior and local contest/quant runs (no new multi-lane burn for verification).
