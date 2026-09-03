# AGENTS.md — QUINTE

> Public repository. Everything committed here must be reproducible by a
> stranger: English only, no personal machine paths, no host-specific
> infrastructure references, no credentials.

## What this is

QUINTE is a generic multi-agent review orchestrator (specs/PROTOCOL-REDESIGN.md):
an A2A v1.0 server to hosts and an A2A v1.0 client to its seats. Rounds, seat
count, domain schemas, gates, and merge rules are policy — a doctrine pack; the
quant review slice is the first pack. The seat roster runs on one model
family per run — a declared policy binding (`deepseek` or `qwen`; the qwen
face is chat-completions or Anthropic Messages per base URL,
specs/SINGLE-VENDOR-DOCTRINE.md). Five distinct schools carry the diversity,
the round discipline stays adversarial, and deterministic gates
(evidence_grounding, cross_seat_reconciliation) carry the verification
weight a same-family roster cannot delegate to the model. The
public contract is Result 2.1 (with the same-model trial_manifest caveat) and
Manifest 2.0.

Product positioning: QUINTE is an internal review mechanism of the STAMMTISCH
quant workstation. End users see STAMMTISCH (data) and GALAHAD (analysis);
they are never exposed to QUINTE or HIGHBALL as concepts.

## Layout

- `src/` — Rust CLI and host (`quinte`, including `quinte host serve`).
- `schemas/` — Result 2.1, Manifest 2.0, brief, and related contracts.
- `specs/` — `PROTOCOL.md`, `FINANCE-PROTOCOL.md`, `HOST.md` (A2A v1.0), CLI and runtime notes.
- `scripts/` — operator helpers (`quinte-progress`, `quinte-run`,
  `quinte-insights`, `quinte-audit`).
- `skills/SKILL.md` — the thin host skill for the CLI.

## Build and test

```bash
cargo test --locked --all-features
cargo install --path . --root ~/.local --locked --force
```

## Contributor rules

1. **Fail closed.** Unparseable output, unknown contract revisions, and
   digest drift halt. Do not guess, do not silently skip.
2. **Single family.** Do not mix providers or families inside one run.
3. **Do not block on `quinte wait`.** Detached start, poll with
   `quinte-progress`. Never `quinte run --wait` or a bare `quinte wait`.
4. **Host serve is a front door**, not a second review CLI. STAMMTISCH
   and other callers must not spawn it.
5. **Canonical skill** is this repository's `skills/SKILL.md`.
6. **English only** in committed files. Host-local overlay and private
   handoff notes stay out of this tree.
7. **Preserve contributor identity.** Agent-authored commits use the agent's
   GitHub-linked Git author identity rather than the human operator or only a
   co-author trailer; human-authored commits retain the human author.

Legacy systemd lifecycle notes live in `specs/HOST-CLI-LEGACY.md`. The
current host contract is `specs/HOST.md`.
