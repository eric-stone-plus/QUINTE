# QUINTE — Multi-Agent Debate Architecture

**2026-06-02** | **DeepSeek v4-pro** | **reasoning=max** | **永不降级 flash**

---

## 1. What is QUINTE?

QUINTE is a five-agent structured debate system that produces conclusions with dramatically higher confidence than any single AI model. It's the evolution of Quattro (4-agent) into a 5-agent system with built-in fault tolerance.

### The Five Agents

| Agent | Engine | Role | R1 | R2 | R3 |
|-------|--------|------|:--:|:--:|:--:|
| **Hermes** | DeepSeek v4-pro max | Orchestrator + Participant | ✅ | ✅ | ✅ |
| **Claude Code** | DeepSeek v4-pro max | Broad Coverage, Structured Reports | ✅ | ✅ | — |
| **CodeWhale** | DeepSeek v4-pro max | Deep Research, Concurrency Analysis | ✅ | ✅ | — |
| **Reasonix** | DeepSeek v4-pro max | Pure Reasoning Judge | — | ✅ | — |
| **omp** | DeepSeek v4-pro xhigh | Hot Spare + Cross-Reviewer | ⚡ | ✅ | — |

⚡ = Activates if Claude Code times out (180s no output)

---

## 2. Architecture

```
Round 1 — Independent Analysis
┌─────────┐  ┌──────────┐  ┌──────────┐
│ Hermes  │  │Claude Code│  │CodeWhale │
└────┬────┘  └─────┬─────┘  └─────┬────┘
     │             │              │
     │   cc timeout 180s? → omp fills
     │             │              │
     └──────┬──────┴──────┬───────┘
            ▼
     Hermes annotates disagreements
            │
            ▼
Round 2 — Cross-Review (review others, never self)
┌─────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ Hermes  │ │Claude Code│ │CodeWhale │ │ Reasonix │ │   omp    │
└────┬────┘ └─────┬─────┘ └─────┬────┘ └─────┬────┘ └─────┬────┘
     │            │             │            │            │
     └────────────┴──────┬──────┴────────────┴────────────┘
                         ▼
                Hermes Final Verdict
```

---

## 3. Key Design Decisions

### 3.1 Five Agents, Not Four

The jump from 4 to 5 agents was driven by Claude Code's reliability issue with DeepSeek. cc's HTTP client silently hangs on non-trivial prompts (100% reproducible). Rather than degrade to 3-party mode, omp was introduced as a hot spare.

**The critical insight**: In Round 2, both cc and omp participate simultaneously — even if cc was absent in Round 1. More perspectives in cross-review = higher confidence.

### 3.2 Reasonix: Round 2 Only

Reasonix `run` mode cannot execute tools (can't read files). It's excluded from Round 1. But Round 2 is pure reasoning — all Round 1 outputs are embedded in the prompt. Reasonix excels at identifying consensus, flagging false positives, and catching issues missed by all other agents.

### 3.3 No `delegate_task`

All agents are invoked via `terminal(background=true)` with direct CLI calls. `delegate_task` subagents have no memory, lose context, and get interrupted by user messages. Terminal + PTY + background is the reliable path.

### 3.4 Parallel by Default

Round 1 launches Hermes + cc + cw simultaneously. Round 2 launches all participants in parallel. omp is launched only if cc times out, but both participate in R2.

---

## 4. Model Configuration

All agents use **DeepSeek v4-pro**, reasoning effort **max** (xhigh for omp). Flash is explicitly forbidden — any degradation would undermine the debate's credibility.

| Agent | CLI | reasoning |
|-------|-----|-----------|
| Hermes | self | max |
| Claude Code | `script -q /dev/null claude -p` | max (settings.json) |
| CodeWhale | `codewhale exec --auto` | max (config.toml) |
| Reasonix | `reasonix run --model deepseek-v4-pro --effort max` | max (CLI flag) |
| omp | `python3 /tmp/omp_run.py` | xhigh (CLI flag) |

---

## 5. Degradation Protocol

| Failure | Action |
|---------|--------|
| Claude Code 180s zero output | Kill → omp fills R1. R2: both cc + omp participate |
| CodeWhale 180s zero output | Kill, no retry. ≥2 parties valid |
| Reasonix 180s zero output | Kill, no retry. ≥2 parties valid |
| omp fails | Non-blocking. Noted in verdict |

Minimum viable debate: Hermes + 1 other party.

---

## 6. Results

10+ debates conducted across:
- Codex + DeepSeek compatibility (5-party consensus: impossible without bridge)
- hermes-core-rules-mac sync audit (11 stale files found, all fixed)
- omp role definition (5-party consensus: synthesis engine, not browser tool)
- Quattro → QUINTE global rename (37 replacements, zero errors)

---

## 7. Why This Matters

Single-model AI outputs have an inherent confidence ceiling. No matter how powerful the model, you're getting one perspective. QUINTE breaks through by:

1. **Independent analysis** — Each agent brings its own framework (cc: broad coverage, cw: deep research, Reasonix: pure reasoning)
2. **Cross-examination** — Agents review each other's work, catching blind spots, false positives, and missed issues
3. **Structured verdict** — Hermes synthesizes all findings into a final ruling with explicit agreement/disagreement tracking

This isn't "ask 5 models and average their answers." It's a debate protocol with adversarial review — the same structure that makes human peer review the gold standard for knowledge validation.
