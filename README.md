<div align="center">

<img src="quinte-cover.svg?v=5" alt="QUINTE" width="100%">

# QUINTE

**Five-agent structured debate architecture for AI conclusion confidence.**

Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate your questions through structured rounds of analysis, cross-examination, and final verdict.

---

[![DeepSeek](https://img.shields.io/badge/DeepSeek-v4--pro-4B6BFB?style=flat)](https://deepseek.com)
[![Reasoning](https://img.shields.io/badge/reasoning-max-ff6b6b?style=flat)](https://deepseek.com)
[![Flash](https://img.shields.io/badge/flash-forbidden-red?style=flat)](https://deepseek.com)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

---

## Architecture

| Agent | Engine | R1 | R2 | Strengths |
|-------|--------|:--:|:--:|-----------|
| **Hermes** | DeepSeek v4-pro · xhigh | ✅ | ✅ | Orchestration + final verdict |
| **Claude Code** | DeepSeek v4-pro · max | ✅ | ✅ | Broadest coverage, structured reporting |
| **CodeWhale** | DeepSeek v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
| **Reasonix** | DeepSeek v4-pro · max | — | ✅ | Pure reasoning judge (content-embedded) |
| **oh-my-pi** | DeepSeek v4-pro · xhigh | ⚡ | ✅ | Hot spare + cross-reviewer |

⚡ Activates if Claude Code times out (180s no output)

```
Round 1 — Independent     Round 2 — Cross-Review       Round 3
Hermes ──→ analysis       Hermes ──→ reviews all       Hermes ──→ verdict
Claude Code ──→ analysis  Claude Code ──→ reviews all
CodeWhale ──→ analysis    CodeWhale ──→ reviews all
(cc timeout → oh-my-pi fills)  Reasonix ──→ pure judge
                           oh-my-pi ──→ reviews all
```

## Design Principles

- **All DeepSeek v4-pro · reasoning=max · flash forbidden**
- **3 rounds max** — early consensus skips remaining rounds
- **≥2 parties** minimum — no single-agent decisions
- **Cross-review is adversarial** — review others, never yourself
- **Terminal + background CLI** — no delegate_task

## Quick Start

```bash
git clone https://github.com/eric-stone-plus/quinte.git
cd quinte
open quinte.html          # Architecture visualization
bash quinte-demo.sh       # Simulate a debate round
```

## Results

10+ debates conducted across code review, architecture decisions, naming conventions, and tool evaluation.

## Built With

QUINTE orchestrates five independent AI agents. **None are developed by this project.** Each is a standalone tool used as a debate participant.

| Agent | Description | Repository |
|-------|-------------|------------|
| [**Hermes**](https://github.com/nousresearch/hermes-agent) | Orchestrator + debater. Coordinates rounds and produces final verdict. | MIT |
| [**Claude Code**](https://github.com/anthropics/claude-code) | Anthropic's coding agent. Broadest coverage, structured reports. | MIT |
| [**CodeWhale**](https://github.com/Hmbown/CodeWhale) | DeepSeek-native agent. Deepest research, concurrency analysis. | MIT |
| [**Reasonix**](https://github.com/esengine/DeepSeek-Reasonix) | DeepSeek-native reasoning. R2 pure judge, content-embedded. | MIT |
| [**Oh-my-pi**](https://github.com/can1357/oh-my-pi) | Pi-fork agent. Hot spare, LSP/DAP, 40+ providers. | MIT |

All agents run on **DeepSeek v4-pro**. Maximum reasoning per agent's native config: Hermes/oh-my-pi use `xhigh`, others use `max`. Flash is explicitly forbidden.

## License

MIT — the protocol and orchestration layer. Individual agents carry their own licenses (see [Built With](#built-with)).
