<div align="center">

<img src="quinte-cover.svg?v=5" alt="QUINTE" width="100%">

# QUINTE

**Five-Agent structured debate architecture for AI conclusion confidence.**

Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate your questions through structured rounds of analysis, cross-examination, and final verdict.

---

[![DeepSeek](https://img.shields.io/badge/DeepSeek-v4--pro-4B6BFB?style=flat)](https://deepseek.com)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

</div>

---

## Architecture

| Agent | Model | R1 | R2 | Strengths |
|-------|-------|----|----|-----------|
| **Hermes** | deepseek-v4-pro | ✅ | ✅ | Orchestration + Final Verdict |
| **Claude Code** | deepseek-v4-pro | ✅ | ✅ | Broadest coverage, structured reports |
| **CodeWhale** | deepseek-v4-pro | ✅ | ✅ | Deepest research, concurrency |
| **OMP** | deepseek-v4-pro | ✅ | ✅ | Full participant, LSP/DAP |
| **Reasonix** | deepseek-v4-pro | — | ✅ | Pure reasoning judge (R1 — limited) |

R1: 4 agents. R2: 5 agents (+Reasonix). When Reasonix run mode supports tool calls, R1 expands to 5.

```
                             Hermes (Orchestrator + Participant)
                                              ▼
                  ┌─────────────────────────────────────────────────────────┐
                    Round 1         Round 1         Round 1        Round 1
                     Hermes         Claude         CodeWhale         OMP
                    (v4 xhigh)     (v4 max)        (v4 max)       (v4 xhigh)
                  └─────────────────────────────────────────────────────────┘
                                              ▼
                                  Hermes flags divergences
                                              ▼
              ┌───────────────────────────────────────────────────────────────────┐
                Round 2       Round 2       Round 2       Round 2       Round 2
                Hermes         Claude       CodeWhale     Reasonix        OMP
               (v4 xhigh)     (v4 max)      (v4 max)      (v4 max)     (v4 xhigh)
              └───────────────────────────────────────────────────────────────────┘
                                              ▼
                                     Hermes final verdict
```

## Design Principles

- **All DeepSeek v4-pro · Hermes/OMP xhigh, rest max · flash forbidden**
- **No degradation** — all 5 agents must participate. Timeout → retry with smaller prompt, never skip.
- **3 rounds max** — early consensus skips remaining rounds
- **Cross-review is adversarial** — review others, never yourself
- **Terminal + Background CLI** — no delegate_task

## Quick Start

```bash
git clone https://github.com/eric-stone-plus/quinte.git
cd quinte
open quinte.html          # Architecture visualization
bash quinte-demo.sh       # Simulate a debate round
```

## Built With

QUINTE orchestrates five independent AI agents. **None are developed by this project.** Each is a standalone tool used as a debate participant.

| Agent | Description | Repository |
|-------|-------------|------------|
| [**Hermes**](https://github.com/nousresearch/hermes-agent) | Orchestrator + debater. Coordinates rounds and produces final verdict. | MIT |
| [**Claude Code**](https://github.com/anthropics/claude-code) | Anthropic's coding agent. Broadest coverage, structured reports. | MIT |
| [**CodeWhale**](https://github.com/Hmbown/CodeWhale) | DeepSeek-native agent. Deepest research, concurrency analysis. | MIT |
| [**OMP**](https://github.com/can1357/oh-my-pi) | oh-my-pi fork. Full debate participant, LSP/DAP, 40+ providers. | MIT |
| [**Reasonix**](https://github.com/esengine/DeepSeek-Reasonix) | DeepSeek-native reasoning. R2 pure judge, content-embedded. | MIT |

All agents run on **DeepSeek v4-pro**. Hermes/OMP use `xhigh`, others use `max`. Flash is explicitly forbidden.

## License

MIT — the protocol and orchestration layer. Individual agents carry their own licenses (see [Built With](#built-with)).
