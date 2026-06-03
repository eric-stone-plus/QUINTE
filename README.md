1|<div align="center">
2|
3|<img src="quinte-cover.svg?v=5" alt="QUINTE" width="100%">
4|
5|# QUINTE
6|
7|**Five-Agent structured debate architecture for AI conclusion confidence.**
8|
9|Single-model AI hits a confidence ceiling. QUINTE breaks through — five independent agents debate your questions through structured rounds of analysis, cross-examination, and final verdict.
10|
11|---
12|
13|[![DeepSeek](https://img.shields.io/badge/DeepSeek-v4--pro-4B6BFB?style=flat)](https://deepseek.com)
14|[![License](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)
15|
16|</div>
17|
18|---
19|
20|## Architecture
21|
22|| Agent | Engine | R1 | R2 | Strengths |
23||-------|--------|:--:|:--:|-----------|
24|| **Hermes** | DeepSeek v4-pro · xhigh | ✅ | ✅ | Orchestration + final verdict |
25|| **Claude Code** | DeepSeek v4-pro · max | ✅ | ✅ | Broadest coverage, structured reporting |
26|| **CodeWhale** | DeepSeek v4-pro · max | ✅ | ✅ | Deepest research, concurrency analysis |
27|| **OMP** | DeepSeek v4-pro · xhigh | ✅ | ✅ | Full participant all rounds, LSP/DAP tools |
28|| **Reasonix** | DeepSeek v4-pro · max | — | ✅ | Pure reasoning judge (R1 tool limitation — temporary) |
29|
30|R1: 4 agents. R2: 5 agents (+Reasonix). When Reasonix run mode supports tool calls, R1 expands to 5.
31|
32|```
33|              Hermes (Orchestrator + Participant)
34|               │
35|    ┌──────────┼──────────┬──────────┐
36|    ▼          ▼          ▼          ▼
37|  Round 1   Round 1    Round 1    Round 1
38|  Hermes    Claude     CodeWhale  OMP
39|  (v4       (v4       (v4       (v4
40|   xhigh)    max)      max)      xhigh)
41|    │          │          │          │
42|    └──────────┼──────────┼──────────┘
43|               ▼
44|         Hermes 标注分歧
45|               │
46|    ┌──────────┼──────────┬──────────┬──────────┐
47|    ▼          ▼          ▼          ▼          ▼
48|  Round 2   Round 2    Round 2    Round 2    Round 2
49|  Hermes    Claude     CodeWhale  Reasonix   OMP
50|  (v4       (v4       (v4       (v4       (v4
51|   xhigh)    max)      max)      max)      xhigh)
52|    │          │          │          │          │
53|    └──────────┼──────────┼──────────┼──────────┘
54|               ▼
55|       Hermes 终裁合成
56|```
57|
58|## Design Principles
59|
60|- **All DeepSeek v4-pro · Hermes/OMP xhigh, rest max · flash forbidden**
61|- **No degradation** — all 5 agents must participate. Timeout → retry with smaller prompt, never skip.
62|- **3 rounds max** — early consensus skips remaining rounds
63|- **Cross-review is adversarial** — review others, never yourself
64|- **Terminal + background CLI** — no delegate_task
65|
66|## Quick Start
67|
68|```bash
69|git clone https://github.com/eric-stone-plus/quinte.git
70|cd quinte
71|open quinte.html          # Architecture visualization
72|bash quinte-demo.sh       # Simulate a debate round
73|```
74|
75|## Built With
76|
77|QUINTE orchestrates five independent AI agents. **None are developed by this project.** Each is a standalone tool used as a debate participant.
78|
79|| Agent | Description | Repository |
80||-------|-------------|------------|
81|| [**Hermes**](https://github.com/nousresearch/hermes-agent) | Orchestrator + debater. Coordinates rounds and produces final verdict. | MIT |
82|| [**Claude Code**](https://github.com/anthropics/claude-code) | Anthropic's coding agent. Broadest coverage, structured reports. | MIT |
83|| [**CodeWhale**](https://github.com/Hmbown/CodeWhale) | DeepSeek-native agent. Deepest research, concurrency analysis. | MIT |
84|| [**OMP**](https://github.com/can1357/oh-my-pi) | oh-my-pi fork. Full debate participant, LSP/DAP, 40+ providers. | MIT |
85|| [**Reasonix**](https://github.com/esengine/DeepSeek-Reasonix) | DeepSeek-native reasoning. R2 pure judge, content-embedded. | MIT |
86|
87|All agents run on **DeepSeek v4-pro**. Hermes/OMP use `xhigh`, others use `max`. Flash is explicitly forbidden.
88|
89|## License
90|
91|MIT — the protocol and orchestration layer. Individual agents carry their own licenses (see [Built With](#built-with)).
92|