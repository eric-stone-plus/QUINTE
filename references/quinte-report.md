> **⚠️ HISTORICAL — 2026-06-02. Superseded by [spec/PROTOCOL.md](../spec/PROTOCOL.md) v2.3.**
> This report describes an earlier architecture: OMP as hot spare, 3-round with early consensus, "adversarial" framing. See [CHANGELOG.md](../CHANGELOG.md) for evolution.

1|# QUINTE — Multi-Agent Debate Architecture
2|
3|**2026-06-02** | **DeepSeek v4-pro** | **reasoning=max** | **永不降级 flash**
4|
5|---
6|
7|## 1. What is QUINTE?
8|
9|QUINTE is a five-agent structured debate system that produces conclusions with dramatically higher confidence than any single AI model. It's the evolution of Quattro (4-agent) into a 5-agent system with built-in fault tolerance.
10|
11|### The Five Agents
12|
13|| Agent | Engine | Role | R1 | R2 | R3 |
14||-------|--------|------|:--:|:--:|:--:|
15|| **Hermes** | DeepSeek v4-pro max | Orchestrator + Participant | ✅ | ✅ | ✅ |
16|| **Claude Code** | DeepSeek v4-pro max | Broad Coverage, Structured Reports | ✅ | ✅ | — |
17|| **CodeWhale** | DeepSeek v4-pro max | Deep Research, Concurrency Analysis | ✅ | ✅ | — |
18|| **Reasonix** | DeepSeek v4-pro max | Pure Reasoning Judge | — | ✅ | — |
19|| **OMP** | DeepSeek v4-pro xhigh | Hot Spare + Cross-Reviewer | ⚡ | ✅ | — |
20|
21|⚡ = Activates if Claude Code times out (180s no output)
22|
23|---
24|
25|## 2. Architecture
26|
27|```
28|Round 1 — Independent Analysis
29|┌─────────┐  ┌──────────┐  ┌──────────┐
30|│ Hermes  │  │Claude Code│  │CodeWhale │
31|└────┬────┘  └─────┬─────┘  └─────┬────┘
32|     │             │              │
33|     │   cc timeout 180s? → omp fills
34|     │             │              │
35|     └──────┬──────┴──────┬───────┘
36|            ▼
37|     Hermes annotates disagreements
38|            │
39|            ▼
40|Round 2 — Cross-Review (review others, never self)
41|┌─────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
42|│ Hermes  │ │Claude Code│ │CodeWhale │ │ Reasonix │ │   OMP    │
43|└────┬────┘ └─────┬─────┘ └─────┬────┘ └─────┬────┘ └─────┬────┘
44|     │            │             │            │            │
45|     └────────────┴──────┬──────┴────────────┴────────────┘
46|                         ▼
47|                Hermes Final Verdict
48|```
49|
50|---
51|
52|## 3. Key Design Decisions
53|
54|### 3.1 Five Agents, Not Four
55|
56|The jump from 4 to 5 agents was driven by Claude Code's reliability issue with DeepSeek. cc's HTTP client silently hangs on non-trivial prompts (100% reproducible). Rather than degrade to 3-party mode, omp was introduced as a hot spare.
57|
58|**The critical insight**: In Round 2, both cc and omp participate simultaneously — even if cc was absent in Round 1. More perspectives in cross-review = higher confidence.
59|
60|### 3.2 Reasonix: Round 2 Only
61|
62|Reasonix `run` mode cannot execute tools (can't read files). It's excluded from Round 1. But Round 2 is pure reasoning — all Round 1 outputs are embedded in the prompt. Reasonix excels at identifying consensus, flagging false positives, and catching issues missed by all other agents.
63|
64|### 3.3 No `delegate_task`
65|
66|All agents are invoked via `terminal(background=true)` with direct CLI calls. `delegate_task` subagents have no memory, lose context, and get interrupted by user messages. Terminal + PTY + background is the reliable path.
67|
68|### 3.4 Parallel by Default
69|
70|Round 1 launches Hermes + cc + cw simultaneously. Round 2 launches all participants in parallel. omp is launched only if cc times out, but both participate in R2.
71|
72|---
73|
74|## 4. Model Configuration
75|
76|All agents use **DeepSeek v4-pro**, reasoning effort **max** (xhigh for omp). Flash is explicitly forbidden — any degradation would undermine the debate's credibility.
77|
78|| Agent | CLI | reasoning |
79||-------|-----|-----------|
80|| Hermes | self | max |
81|| Claude Code | `script -q /dev/null claude -p` | max (settings.json) |
82|| CodeWhale | `codewhale exec --auto` | max (config.toml) |
83|| Reasonix | `reasonix run --model deepseek-v4-pro --effort max` | max (CLI flag) |
84|| OMP | `python3 /tmp/omp_run.py` | xhigh (CLI flag) |
85|
86|---
87|
88|## 5. Degradation Protocol
89|
90|| Failure | Action |
91||---------|--------|
92|| Claude Code 180s zero output | Kill → omp fills R1. R2: both cc + omp participate |
93|| CodeWhale 180s zero output | Kill, no retry. ≥2 parties valid |
94|| Reasonix 180s zero output | Kill, no retry. ≥2 parties valid |
95|| omp fails | Non-blocking. Noted in verdict |
96|
97|Minimum viable debate: Hermes + 1 other party.
98|
99|---
100|
101|## 6. Results
102|
103|10+ debates conducted across:
104|- Codex + DeepSeek compatibility (5-party consensus: impossible without bridge)
105|- QUINTE skill audit (5-agent consensus: multiple issues found and resolved)
106|- omp role definition (5-party consensus: synthesis engine, not browser tool)
107|- Quattro → QUINTE global rename (37 replacements, zero errors)
108|
109|---
110|
111|## 7. Why This Matters
112|
113|Single-model AI outputs have an inherent confidence ceiling. No matter how powerful the model, you're getting one perspective. QUINTE breaks through by:
114|
115|1. **Independent analysis** — Each agent brings its own framework (cc: broad coverage, cw: deep research, Reasonix: pure reasoning)
116|2. **Cross-examination** — Agents review each other's work, catching blind spots, false positives, and missed issues
117|3. **Structured verdict** — Hermes synthesizes all findings into a final ruling with explicit agreement/disagreement tracking
118|
119|This isn't "ask 5 models and average their answers." It's a debate protocol with adversarial review — the same structure that makes human peer review the gold standard for knowledge validation.
120|