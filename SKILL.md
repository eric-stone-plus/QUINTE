1|---
2|name: quinte
3|description: "QUINTE — Five-agent structured debate architecture. Now absorbed into multi-agent-debate skill."
4|version: 2.1.0
5|---
6|
7|# QUINTE — Multi-Agent Debate Architecture
8|
9|> **⚠️ This standalone skill has been absorbed into `multi-agent-debate` as of 2026-06-03.**
10|> The full protocol, 5-agent architecture, invocation details, and known pitfalls now live in the `multi-agent-debate` skill.
11|> This repo is retained as a reference for the QUINTE architecture diagram and demo assets.
12|
13|## Architecture
14|
15|```
16|              Hermes (Orchestrator + Participant)
17|               │
18|    ┌──────────┼──────────┬──────────┐
19|    ▼          ▼          ▼          ▼
20|  Round 1   Round 1    Round 1    Round 1
21|  Hermes    Claude     CodeWhale  OMP
22|  (v4       (v4       (v4       (v4
23|   xhigh)    max)      max)      xhigh)
24|    │          │          │          │
25|    └──────────┼──────────┼──────────┘
26|               ▼
27|         Hermes 标注分歧
28|               │
29|    ┌──────────┼──────────┬──────────┬──────────┐
30|    ▼          ▼          ▼          ▼          ▼
31|  Round 2   Round 2    Round 2    Round 2    Round 2
32|  Hermes    Claude     CodeWhale  Reasonix   OMP
33|  (v4       (v4       (v4       (v4       (v4
34|   xhigh)    max)      max)      max)      xhigh)
35|    │          │          │          │          │
36|    └──────────┼──────────┼──────────┼──────────┘
37|               ▼
38|       Hermes 终裁合成
39|```
40|
41|## Participation
42|
43|| Agent | Engine | R1 | R2 | Role |
44||-------|--------|:--:|:--:|------|
45|| Hermes (hm) | v4-pro · xhigh | ✅ | ✅ | Orchestrator + final verdict |
46|| Claude Code (cc) | v4-pro · max | ✅ | ✅ | Broadest coverage, structured reports |
47|| CodeWhale (cw) | v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
48|| OMP | v4-pro · xhigh | ✅ | ✅ | Full participant, all rounds |
49|| Reasonix (rx) | v4-pro · max | — | ✅ | R2 pure reasoning judge |
50|
51|⛔ rx 绝不参与 R1 — run 模式不执行工具。
52|
53|**All DeepSeek v4-pro. Hermes/OMP xhigh, rest max. No flash degradation. Token budget unlimited.**
54|
55|**R1: 4 agents. R2: 5 agents (Reasonix joins).** When Reasonix run mode supports tool calls, R1 expands to 5.
56|
57|**No degradation:** all 5 must participate. Timeout → retry with smaller prompt, never skip.
58|
59|## Key Updates
60|
61|- **2026-06-03 v2.2**: hm/rx shorthands added, rx R1 prohibition, execution discipline
62|- **2026-06-03 v2.1**: OMP promoted from hot spare to full R1 participant. Architecture: R1=4 agents, R2=5.
63|- **2026-06-03 v2.0**: Skill absorbed into `multi-agent-debate`. `oh-my-pi` → `OMP` naming standardized. No-degradation policy.
64|
65|## See Also
66|
67|- `SKILL.md` in this repo — the full QUINTE architecture reference
68|- `multi-agent-debate` skill — full protocol, triggers, invocation, pitfalls (available in Hermes Agent skill registry)
69|