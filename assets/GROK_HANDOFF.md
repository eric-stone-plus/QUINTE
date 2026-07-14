# QUINTE / HIGHBALL / Hermes handoff

Date: 2026-07-15

Single normative handoff for this worktree. Other files under `assets/` are
either the cover image, the Windows real-machine checklist, or non-normative
historical residual reports.

## Product version

| | |
| --- | --- |
| Package / CLI | **0.1.1** (unreleased test line; no external users) |
| Successful historical Release | `v0.1.0` only |
| Failed tags (not releases) | `v0.1.1`–`v0.1.3` (CI failed); deleted `v0.1.4` tag (cancelled) |
| Superseded local label | interim `0.1.4` cohort renumbered to **0.1.1** |
| JSON Schema dialect | draft/2020-12 |

CLI package version is the sole product version source. All QUINTE-owned
protocol, schema, artifact, packet, receipt, envelope, doctor, and retry/rate
fields use `0.1.1`. Do not hard-code money thresholds, TTLs, or shipping-domain
enums into generic QUINTE.

## Repositories

| Product | Checkout | Notes |
| --- | --- | --- |
| QUINTE | `/Users/ericstone/Public/QUINTE` | Branch name may still say `impl/0.1.4-handoff`; product version is **0.1.1** |
| HIGHBALL | `/Users/ericstone/Public/HIGHBALL` | Atomic Action Packet path |
| Hermes | `/Users/ericstone/Private/hermes-agent` | `self_improvement.enabled` host policy |
| Profiles | `…/hermes-technical-profile-{mac,win}` | `config.quinte-host.yaml` |

Do not develop in `/Users/ericstone/.hermes/hermes-agent` (runtime isolation).

### Implementation baseline (start of Grok pass)

- HEAD: `dce11ab70e088e784c4117009948956e5b492cea`
- Dirty four-file partial version migration (doctor/model/run/store); binary
  diff SHA-256:
  `d8748118b2583707b9a32b64b8a7ee73afd220799717a795e4d272ed619b7d5c`
- That baseline was preserved and completed into the `0.1.1` cohort (not
  discarded).

## Ownership

- **QUINTE:** one-run residuals, evidence binding, R3 merge, immutable
  `result.json`, process lifecycle for processes it starts.
- **HIGHBALL:** cross-run closure/waiver, Action Packets, routing; bind atomic
  `quinte` product outcome only — no R1/R2/R3 scheduler reconstruction.
- **Hermes:** present results, collect human closure/waiver, call HIGHBALL; no
  mutation of QUINTE results. Dedicated host: `self_improvement.enabled=false`
  (no SOUL/USER/MEMORY/skills self-writes).

## Implementation status

| Area | Status |
| --- | --- |
| Version cohort `0.1.1` | Done |
| Claim/residual ID uniqueness | Done |
| HM residual evidence refs vs snapshot | Done |
| Full residual field merge conflicts | Done |
| High-risk R1/R2 residual preservation | Done |
| Windows Job Object containment (code) | Done on macOS build path |
| Windows real-machine lifecycle matrix | **OPEN gate** — see `WINDOWS_REAL_MACHINE_ACCEPTANCE.md` |
| Claude non-macOS credential (`ANTHROPIC_API_KEY`) | Left explicit; Windows proof in open gate |
| HIGHBALL atomic `--quinte-result` binding | Done |
| Hermes zero self-improvement + hash tests | Done |
| `CHANGELOG.md` | Done |

## Historical residual reports (non-normative)

`QUINTE残差管理机制调优分析.md` and the matching `.html` are background only.

| Historical implication | Fact |
| --- | --- |
| Evidence refs unvalidated | R1/R2 and HM residual refs validated against snapshot |
| Final residuals = R1/R2 dedup | Final from HM/CC + high-risk R1/R2 preservation |
| Mutable `quinte residual update` P0 | No; `result.json` immutable; HIGHBALL owns cross-run closure |
| Hard-coded $ / TTL / shipping enums | Not in generic QUINTE |
| Schema IDs unique | Syntax only was insufficient; semantic uniqueness enforced |
| Version `1.0` examples harmless | Product versions are **0.1.1** |

## Windows release gate (OPEN)

Code ships Job Object kill-on-close (suspend → assign → resume). The full
matrix in `WINDOWS_REAL_MACHINE_ACCEPTANCE.md` has **not** been run on a real
Windows 11 host. macOS tests must not claim that gate passed. On Windows
acceptance, every QUINTE-owned version field must equal **0.1.1**.

## Verify

```bash
cd /Users/ericstone/Public/QUINTE
cargo test --all-targets --all-features
cargo run -- --version   # quinte 0.1.1
```
