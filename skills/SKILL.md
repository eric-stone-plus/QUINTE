---
name: quinte
description: Run or continue a QUINTE adversarial review through the quinte CLI, including five-party R1/R2 analysis, two same-family R3 arbiters, run inspection, and recovery. Use when the user explicitly asks for QUINTE, a five-party structured review, cross-examination, residual exposure, or continuation or inspection of an existing QUINTE run. Do not use it as a generic delegator or to run one QUINTE role.
---

# QUINTE CLI

Use `quinte` as the sole execution authority. Do not recreate its phases with
manual agent calls, `delegate_task`, shell loops, or any host phase dispatcher.
Do not run, replace, or skip an individual party.

Do not read `shimei-host-overlay.json` or any archive overlay as a dispatch
source. Commands come only from the installed `quinte` CLI.

## Install prerequisites (host must verify)

QUINTE is **source-built only** (no GitHub prebuilt Releases). Before the first
run in a session, confirm the host environment:

1. `quinte --version` resolves on PATH to the binary built from this checkout
   (`cargo build --release` → install/copy onto PATH).
2. `quinte-progress` and `quinte-run` are on PATH (from repo `scripts/`;
   symlink into `~/.local/bin` preferred so pulls stay in sync).
3. `quinte doctor --json` is green after install, rebuild, or `git pull`.
4. This skill file matches the repo copy at `skills/SKILL.md` (Hermes live
   paths under `~/.hermes` are not version-controlled — re-sync after pull).

If progress helpers are missing, do not fall back to `quinte run --wait` or a
sleeping shell loop; install the scripts first. See the repository README
Quick Start for full install and update steps.

R2 anti-429 handling is CLI-owned. The scheduler keeps R2 serial with 10-second
pacing, soft-staggers R1 starts, makes at most three same-route attempts, and
applies typed 15-to-120-second bounded backoff. Do not add sleeps, retries, or
lane logic in the host skill.

Policy switch `r2_parallel` (default `false`) opts into parallel R2: lanes fan
out with the same soft-stagger as R1 instead of serial pacing. The switch
changes only the rate-limit profile, never a lane's information set — in both
modes every R2 lane sees only the anonymized R1 packet, never other R2 outputs.

When the user explicitly asks for a QUINTE review, actually invoke the
`quinte` CLI. Manual analysis must not be presented as a QUINTE result.

## Efficiency (no 429, no frozen session)

1. **One active run at a time.** Before `quinte run`, scan with
   `quinte-progress` on any recent id or check that no other run is
   `r1_running` / `r2_running`. If a prior run is still active, poll it or
   `quinte cancel <id> --json` after the user agrees — never stack a second
   five-party fan-out (shared model backends → 429 and thrash).
2. **Never block the interactive session.** Do not use `quinte run --wait` or
   bare `quinte wait`. Start detached; poll `quinte-progress <run-id>` every
   30–60 s as **separate** tool calls and narrate each line. Do not wrap polls
   in one shell `for`/`sleep` loop. Prefer `quinte-run --brief <file>` when the
   host streams stdout; interpret its final status instead of treating an exit
   code as proof of completion.
3. **Compact evidence.** Extract essential text into a small evidence directory;
   avoid dumping large PDF trees. Prefer `.doc`/`.docx` text over scanned PDF
   OCR. Normalize trailing/nbsp filenames. Use `snapshot_ignore` for noise.
   Parties may only cite exact `snapshot://` refs from the snapshot manifest —
   invented paths fail R1 non-retryably and waste a full multi-party round.
4. **Fail once, fix brief, then re-run.** On `failed`, read
   `quinte status <id> --json` error text first. Do not immediately re-fire the
   same brief. Fix evidence/refs, cancel leftovers, then start one new run.
5. **Narrate liveness.** `quinte-progress` shows `act Ns ago` from worker
   heartbeat/events (not only manifest transitions). If `act` stays high and
   lanes show no change for several polls, report that and consider
   `quinte resume` / `cancel` rather than silent waiting.

## Run

1. Run `quinte doctor --json`. If it reports that QUINTE is not initialized,
   run `quinte init --json` once and rerun `doctor`; stop on any other nonzero
   exit. Never use `--force` without an explicit reason.
2. Write a current Brief JSON file containing the question and only the evidence
   roots, attachments, context, and action scope the user placed in scope.
   Schema-valid minimal examples ship at `examples/brief.json` and, after
   `quinte init`, at `~/.quinte/canary/brief.json`; copy their field set
   (`brief_version`, `question`, `context`, `evidence_roots`, `attachments`,
   `action_scope`) instead of probing the schema by trial and error. Do not
   infer a contract revision from the package version.
   Then validate before spending a multi-party round:
   `quinte validate --kind brief <file> --json` — fix every reported field
   error first; a schema-invalid brief fails the run after lanes have already
   started. Also verify `evidence_roots` resolve to at least one readable
   file — an empty snapshot wastes a full round (run 019f710b produced 12
   P0/CRITICAL residuals on zero evidence).
3. Start the run detached: `quinte run --brief <file> --json`, and record the
   returned run id. Create the brief and start `run` in the same execution
   action when possible; do not claim dispatch until a run id returns.
4. Track progress with `quinte-progress <run-id>` (scripts on PATH). Poll every
   30–60 s and narrate. For a human terminal,
   `quinte-progress <run-id> --watch` streams every 15 s.
5. Branch on the returned `status`, not the exit code alone. A default detached
   run returns `queued`; current policies normally continue through the
   automatic Primary Arbiter to `completed`.
   Expected duration: R1 about 1–4 min (parallel, soft-staggered), R2 about
   3–8 min (serial + 10 s pacing), then automatic R3 arbitration.

Legacy compatibility: `waiting_primary_arbiter` and the manual PA commands are
historical surfaces only; production v2 uses automatic same-family arbitration
and requires no host handoff.

Use `quinte resume <run-id> --json` after an interrupted scheduler process,
`quinte-progress <run-id>` to observe, and `quinte cancel <run-id> --json`
only for an explicit cancellation. Ctrl-C on `wait` returns `130` without
cancelling the run.

## External Audit (post-completion, explicit and optional)

QUINTE never invokes an auditor as part of its state machine. After any run
reaches `completed`, a host may explicitly run
`quinte-audit --home <state-root> <run-id>`. The auditor must be outside the
seat family; MAGI owns that cross-family routing.

1. The script reads `result.json` (+ `r3/cc-response.json` when present), asks
   the auditor to check composition fidelity, evidence support, blind spots,
   and actionability.
2. It writes an immutable timestamped artifact and updates the mode-specific
   `r3/external-audit-*.json` pointer; `result.json`, `manifest.json`, and the
   hash chain are never modified.
3. Verdicts: `PASS` (adopt), `PASS_WITH_NOTES` (adopt after reading findings),
   `FAIL` (do NOT adopt — route the findings back as a new brief or amend).
   `UNPARSED` means the auditor's output was not clean JSON; read
   `audit_raw` in the artifact and re-run once before trusting it.
4. Exit codes are `0` PASS, `10` PASS_WITH_NOTES, `20` FAIL, and `2` operational
   or contract failure. Invocation is always explicit.

Audit independence is the point — never substitute a roster member as the
auditor.

## Contract Ownership

Policy migration, normalization, schemas, identity validation, model binding,
retry, and process cleanup are owned by the installed CLI. Do not edit or
reinterpret `policy.json`; stop if `quinte policy validate --json` fails.

The installed CLI's contract registry owns the Brief revision independently of
the package version. Copy contract discriminators from the canonical
schema/example rather than hand-writing numeric revisions. Unknown fields are
rejected. Write the brief outside the run directory, and treat only a
schema-valid result emitted by that CLI as the product outcome.

## Production seat binding

All seven roles must match the seat's `family`, `provider`, `text_model`, and
`multimodal_model`. Current production bindings are MiMo via isolated
MiMoCode config/auth, DeepSeek via Reasonix, and OpenAI via Codex. The OpenAI
relay must implement the Responses API. Provider key/base-URL selectors are
allowlisted; HTTP, whitespace, and `.invalid` endpoints fail closed. Historical
v1 policies/results remain inspectable, but their retired adapters cannot be
dispatched by a new run. Back up a v1 `policy.json`, then run
`quinte init --force` to install the production v2 default.

## Pitfall: R2 timeout 与证据精简

**症状：** R2 反复 timeout（3 次后 run failed），`"code": "r2_failed", "message": "Party A failed in R2: timeout"`。

**根因：** policy `timeout_seconds` 默认 300，对大证据集（>50KB 文本，特别是合同全文）不够。

**修复：**
1. 精简证据——裁剪合同到滞期费相关条款（ODA/dispatch/demurrage），移除空 PDF 提取（扫描件 pdftotext 输出 <10 bytes）
2. `timeout_seconds` 提到 600（上限，policy validation 硬限制）

## Windows Snapshot Paths

Windows builds use internal verbatim paths for deep snapshot and lane-input
trees. Briefs and artifact references stay portable; never add `\\?\` or
`\\?\UNC\` yourself. Use `snapshot_ignore` for unrelated or generated subtrees.

## Evidence Preparation

- Prefer native `.doc`/`.docx` text extraction before scanned PDF OCR.
- Normalize filenames with trailing or non-breaking spaces in a temporary
  evidence directory instead of hand-writing ambiguous paths.
- Include relevant domain rules in `context`, `action_scope`, or the evidence
  snapshot. Parties do not inherit local business knowledge across sessions.
- A single-agent calculation may identify a residual, but it does not override
  a completed QUINTE product result without new evidence and closure.

QUINTE output is evidence, not authorization for a protected action. The runtime
uses process/config isolation but does not provide an OS filesystem or network
sandbox.

Read [the CLI contract](../specs/CLI.md) for commands, exit codes, state, and
artifact details. Read [the protocol](../specs/PROTOCOL.md) only when protocol
interpretation is required.
