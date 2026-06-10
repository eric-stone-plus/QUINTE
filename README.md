<div align="center">

<img src="assets/quinte-cover.svg?v=5" alt="QUINTE" width="100%">

# QUINTE Protocol

**Five-agent structured debate architecture for AI conclusion confidence.**

Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate your questions through structured rounds of analysis, cross-examination, and final verdict.

---

[![DeepSeek](https://img.shields.io/badge/DeepSeek-v4--pro-4B6BFB?style=flat)](https://deepseek.com)
[![Protocol](https://img.shields.io/badge/protocol-v3.1-blue?style=flat)](spec/PROTOCOL.md)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

---

## What is QUINTE?

QUINTE is a **protocol** for multi-agent structured debate. It defines:

- **5 agents** (Hermes, Claude Code, CodeWhale, omp, Reasonix)
- **3 rounds** (Independent Analysis → Cross-Review → Dual Verdict with 監査 B)
- **Invariants** (no degradation, adversarial review, mandatory cross-examination)

> 📖 **Read the spec**: [spec/PROTOCOL.md](spec/PROTOCOL.md)

### Design Philosophy

QUINTE exists to solve the Rashomon phenomenon: when a single perspective cannot be trusted, structured cross-examination reveals what one agent alone would miss. See [RASHOMON](https://github.com/eric-stone-plus/RASHOMON) for the full design philosophy and [KANSA](https://github.com/eric-stone-plus/KANSA) for the R3 audit consul. Operations are gated by [KENGEN](https://github.com/eric-stone-plus/KENGEN) with BANNIN (番人) session-level authorization.


## Architecture

```
                         Hermes (Orchestrator + Participant)
                                          ▼
          ┌──────────────────────────────────────────────────────────────┐
              Round 1         Round 1         Round 1         Round 1    
               Hermes          Claude        CodeWhale        Oh-My-Pi   
             (v4 xhigh)       (v4 xhigh)       (v4 max)       (v4 xhigh)  
          └──────────────────────────────────────────────────────────────┘
                                          ▼
                        Hermes collects R1, flags divergences.
                                          ▼
     ┌─────────────────────────────────────────────────────────────────────────┐
         Round 2        Round 2        Round 2        Round 2        Round 2   
         Hermes         Claude        CodeWhale      Reasonix       Oh-My-Pi   
       (v4 xhigh)      (v4 xhigh)      (v4 max)       (v4 max)      (v4 xhigh)  
     └─────────────────────────────────────────────────────────────────────────┘
                                          ▼
                              Hermes + Auditor B Verdict (R3)
```

### R3: Dual Verdict

At R3, every verdict is drafted by two arbiters in parallel:

- **Consul A (hm)** — primary arbiter with full session context
- **Auditor B** — topic-matched second arbiter, independently reviews all R1+R2 evidence

| Domain | Auditor B |
|--------|-----------|
| Ledger / economics | omp |
| Contracts / legal | cc |
| Code / architecture | cw |
| Protocol / strategy | rx |

The two drafts are merged: consensus is adopted, disagreement is surfaced as an annotated dissent. A dissent does not block the verdict — it enriches the record. The lead arbiter may not suppress the auditor's dissent.

### Authorization

Operations are gated by [KENGEN](https://github.com/eric-stone-plus/KENGEN) with BANNIN as the session-level guard. No irreversible external write proceeds without explicit user authorization. The agent has no right to ask — it waits silently.

## Quick Start

```bash
git clone https://github.com/eric-stone-plus/QUINTE.git
cd QUINTE

# Read the protocol
cat spec/PROTOCOL.md

# Try the demo
bash demo/quinte-demo.sh

# View architecture diagram (macOS: open, Linux: xdg-open, Windows: start)
open assets/quinte.html 2>/dev/null || xdg-open assets/quinte.html 2>/dev/null || start assets/quinte.html
```

## For Implementors

QUINTE is implementation-agnostic. The protocol can be implemented by any orchestrator.

| Path | What |
|------|------|
| [spec/PROTOCOL.md](spec/PROTOCOL.md) | Canonical protocol specification |
| [spec/extensions.md](spec/extensions.md) | What protocol owns vs what implementations MAY vary |
| [hermes-skill/](hermes-skill/) | Reference implementation on Hermes Agent |
| [CHANGELOG.md](CHANGELOG.md) | Protocol version history |
| [MIGRATION.md](MIGRATION.md) | 2026-06-06 architectural pivot notice |

## Reference Implementation

The [Hermes Agent](https://github.com/nousresearch/hermes-agent) skill at [`hermes-skill/`](hermes-skill/) is the **reference implementation**. It's the most battle-tested implementation with 20+ debate sessions, platform-specific invocation templates, and a registry of known pitfalls.

## Built With

QUINTE orchestrates five independent AI agents. **None are developed by this project.**

| Agent | Role | Repository |
|-------|------|------------|
| [**Hermes**](https://github.com/nousresearch/hermes-agent) | Orchestrator + R3 Verdict | MIT |
| [**Claude Code**](https://github.com/anthropics/claude-code) | Broad coverage, structured reports | MIT |
| [**CodeWhale**](https://github.com/Hmbown/CodeWhale) | Deep research, concurrency | MIT |
| [**omp**](https://github.com/can1357/oh-my-pi) | Full participant, LSP/DAP | MIT |
| [**Reasonix**](https://github.com/esengine/DeepSeek-Reasonix) | R2 pure reasoning judge | MIT |

## License

MIT — the protocol and orchestration layer. Individual agents carry their own licenses.
