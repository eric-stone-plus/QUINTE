---
name: quinte
description: "QUINTE — Five-agent structured debate architecture. Hermes+cc+cw+Reasonix+oh-my-pi, all DeepSeek v4-pro max reasoning. R1 independent analysis → R2 cross-review → R3 final verdict."
version: 1.0.0
---

# QUINTE — Multi-Agent Debate Architecture

Five independent AI agents debate questions through structured rounds: independent analysis, adversarial cross-review, and final verdict. Dramatically higher confidence than any single model.

## Architecture

| Agent | Engine | R1 | R2 | Role |
|-------|--------|:--:|:--:|------|
| Hermes | v4-pro · xhigh | ✅ | ✅ | Orchestrator + final verdict |
| Claude Code | v4-pro · max | ✅ | ✅ | Broadest coverage, structured reports |
| CodeWhale | v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
| Reasonix | v4-pro · max | — | ✅ | R2 pure reasoning judge |
| oh-my-pi | v4-pro · xhigh | ⚡ | ✅ | Hot spare (cc timeout 180s) |

## Invocation

All agents via `terminal(background=true)`, direct CLI. No `delegate_task`.

## Built With

- [Hermes](https://github.com/nousresearch/hermes-agent)
- [Claude Code](https://github.com/anthropics/claude-code)
- [CodeWhale](https://github.com/Hmbown/CodeWhale)
- [Reasonix](https://github.com/esengine/DeepSeek-Reasonix)
- [oh-my-pi](https://github.com/can1357/oh-my-pi)

## License

MIT — protocol and orchestration. Individual agents carry their own licenses.

## Repo

https://github.com/eric-stone-plus/quinte
