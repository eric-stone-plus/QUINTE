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

## Participation

<table>
<colgroup>
<col width="140">
<col width="200">
<col width="50">
<col width="50">
<col width="440">
</colgroup>
<tr><th>Agent</th><th>Engine</th><th>R1</th><th>R2</th><th>Role</th></tr>
<tr><td><b>Hermes</b></td><td>DeepSeek v4-pro · xhigh</td><td>✅</td><td>✅</td><td>Orchestrator + final verdict</td></tr>
<tr><td><b>Claude Code</b></td><td>DeepSeek v4-pro · max</td><td>✅</td><td>✅</td><td>Broadest coverage, structured reporting</td></tr>
<tr><td><b>CodeWhale</b></td><td>DeepSeek v4-pro · max</td><td>✅</td><td>✅</td><td>Deepest research, concurrency analysis</td></tr>
<tr><td><b>OMP</b></td><td>DeepSeek v4-pro · xhigh</td><td>✅</td><td>✅</td><td>Full participant, all rounds</td></tr>
<tr><td><b>Reasonix</b></td><td>DeepSeek v4-pro · max</td><td>—</td><td>✅</td><td>R2 pure reasoning judge</td></tr>
</table>

⛔ rx 绝不参与 R1 — run 模式不执行工具。

**All DeepSeek v4-pro. Hermes/OMP xhigh, rest max. No flash degradation. Token budget unlimited.**

**R1: 4 agents. R2: 5 agents (Reasonix joins).** When Reasonix run mode supports tool calls, R1 expands to 5.

**No degradation:** all 5 must participate. Timeout → retry with smaller prOMPt, never skip.

## Key Updates

- **2026-06-03 v2.2**: hm/rx shorthands added, rx R1 prohibition, execution discipline
- **2026-06-03 v2.1**: OMP promoted from hot spare to full R1 participant. Architecture: R1=4 agents, R2=5.
- **2026-06-03 v2.0**: Skill absorbed into `multi-agent-debate`. `oh-my-pi` → `OMP` naming standardized. No-degradation policy.

## See Also

- `SKILL.md` in this repo — the full QUINTE architecture reference
- `multi-agent-debate` skill — full protocol, triggers, invocation, pitfalls (available in Hermes Agent skill registry)
