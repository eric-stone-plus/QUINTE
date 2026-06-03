---
name: quinte
description: "QUINTE — Five-agent structured debate architecture. Now absorbed into multi-agent-debate skill."
version: 2.0.0
---

# QUINTE — Multi-Agent Debate Architecture

> **⚠️ This standalone skill has been absorbed into `multi-agent-debate` as of 2026-06-03.**
> The full protocol, 5-agent architecture, invocation details, and known pitfalls now live in the `multi-agent-debate` skill.
> This repo is retained as a reference for the QUINTE architecture diagram and demo assets.

## Architecture

| Agent | Engine | R1 | R2 | Role |
|-------|--------|:--:|:--:|------|
| Hermes | v4-pro · xhigh | ✅ | ✅ | Orchestrator + final verdict |
| Claude Code | v4-pro · max | ✅ | ✅ | Broadest coverage, structured reports |
| CodeWhale | v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
| Reasonix | v4-pro · max | — | ✅ | R2 pure reasoning judge |
| omp | v4-pro · xhigh | ⚡ | ✅ | Hot spare (cc timeout 180s) |

## Key Update (2026-06-03)

- **oh-my-pi → omp**: Standardized naming across all QUINTE skills
- **No degradation**: All 5 agents must participate. Timeout → retry with smaller prompt, never skip.
- **Reasonix R2-only is temporary**: run mode may support tool calls in future versions.

## See Also

- `multi-agent-debate` skill for the full protocol, triggers, and invocation details
- https://github.com/eric-stone-plus/hermes-core-rules-mac
