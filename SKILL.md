---
name: quinte
description: "QUINTE — Five-agent structured debate architecture. Now absorbed into multi-agent-debate skill."
version: 2.1.0
---

# QUINTE — Multi-Agent Debate Architecture

> **⚠️ This standalone skill has been absorbed into `multi-agent-debate` as of 2026-06-03.**
> The full protocol, 5-agent architecture, invocation details, and known pitfalls now live in the `multi-agent-debate` skill.
> This repo is retained as a reference for the QUINTE architecture diagram and demo assets.

## Architecture

```
              Hermes (Orchestrator + Participant)
               │
    ┌──────────┼──────────┬──────────┐
    ▼          ▼          ▼          ▼
  Round 1   Round 1    Round 1    Round 1
  Hermes    Claude     CodeWhale  omp
  (v4       (v4       (v4       (v4
   xhigh)    max)      max)      xhigh)
    │          │          │          │
    └──────────┼──────────┼──────────┘
               ▼
         Hermes 标注分歧
               │
    ┌──────────┼──────────┬──────────┬──────────┐
    ▼          ▼          ▼          ▼          ▼
  Round 2   Round 2    Round 2    Round 2    Round 2
  Hermes    Claude     CodeWhale  Reasonix   omp
  (v4       (v4       (v4       (v4       (v4
   xhigh)    max)      max)      max)      xhigh)
    │          │          │          │          │
    └──────────┼──────────┼──────────┼──────────┘
               ▼
       Hermes 终裁合成
```

## Participation

| Agent | Engine | R1 | R2 | Role |
|-------|--------|:--:|:--:|------|
| Hermes | v4-pro · xhigh | ✅ | ✅ | Orchestrator + final verdict |
| Claude Code | v4-pro · max | ✅ | ✅ | Broadest coverage, structured reports |
| CodeWhale | v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
| omp | v4-pro · xhigh | ✅ | ✅ | Full participant, all rounds. LSP/DAP tools |
| Reasonix | v4-pro · max | — | ✅ | R2 pure reasoning judge (R1 tool limitation — temporary) |

**All DeepSeek v4-pro. Hermes/omp xhigh, rest max. No flash degradation. Token budget unlimited.**

**R1: 4 agents. R2: 5 agents (Reasonix joins).** When Reasonix run mode supports tool calls, R1 expands to 5.

**No degradation:** all 5 must participate. Timeout → retry with smaller prompt, never skip.

## Key Updates

- **2026-06-03 v2.1**: omp promoted from hot spare to full R1 participant. Architecture: R1=4 agents, R2=5.
- **2026-06-03 v2.0**: Skill absorbed into `multi-agent-debate`. `oh-my-pi` → `omp` naming standardized. No-degradation policy.

## See Also

- `multi-agent-debate` skill — full protocol, triggers, invocation, pitfalls
- https://github.com/eric-stone-plus/hermes-core-rules-mac
