<p align="center">
  <img src="quinte-arch.svg" alt="QUINTE Architecture" width="800">
</p>

---

# QUINTE

**Five-agent structured debate architecture for AI conclusion confidence.**

Single-model AI outputs hit a confidence ceiling — one perspective, no matter how powerful, can only go so far. QUINTE breaks through by running independent analyses across five agents, cross-examining their findings, and synthesizing a final verdict with explicit agreement tracking.

---

## Architecture

```
Round 1 — Independent Analysis     Round 2 — Cross-Review          Round 3
Hermes ──→ analysis                Hermes ──→ reviews all           Hermes ──→ verdict
Claude Code ──→ analysis           Claude Code ──→ reviews all
CodeWhale ──→ analysis             CodeWhale ──→ reviews all
(cc timeout → omp fills)           Reasonix ──→ pure reasoning judge
                                   omp ──→ reviews all (if active)
```

| Agent | Engine | R1 | R2 | Strengths |
|-------|--------|:--:|:--:|-----------|
| **Hermes** | DeepSeek v4-pro max | ✅ | ✅ | Orchestration + final verdict |
| **Claude Code** | DeepSeek v4-pro max | ✅ | ✅ | Broadest coverage, structured reporting |
| **CodeWhale** | DeepSeek v4-pro max | ✅ | ✅ | Deepest research, concurrency analysis |
| **Reasonix** | DeepSeek v4-pro max | — | ✅ | Pure reasoning judge (content-embedded) |
| **omp** | DeepSeek v4-pro xhigh | ⚡ | ✅ | Hot spare + cross-reviewer |

⚡ Activates if Claude Code times out (180s no output)

---

## Key Design Principles

- **All agents use DeepSeek v4-pro · reasoning=max · flash forbidden**
- **Terminal + background CLI** — no delegate_task, no subprocess isolation issues
- **3 rounds max** — if consensus is reached earlier, skip remaining rounds
- **≥2 parties** minimum for valid debate; no single-agent decisions
- **Cross-review is adversarial** — review others, never yourself

---

## Results

10+ debates across code review, architecture decisions, naming conventions, and tool evaluation. The 5-agent protocol has caught issues that any single agent or pair would have missed.

---

## Quick Start

```bash
# Clone the demo
git clone https://github.com/eric-stone-plus/quinte.git
cd quinte

# Open the architecture visualization
open quinte.html

# Run the demo script
bash quinte-demo.sh "Your debate topic here"

# Read the full report
cat quinte-report.md
```

---

## Repositories

| Repo | Purpose |
|------|---------|
| [hermes-core-rules-mac](https://github.com/eric-stone-plus/hermes-core-rules-mac) | Operational rules + QUINTE debate skill |
| [quinte](https://github.com/eric-stone-plus/quinte) | This repo — architecture demo + report |

---

## License

MIT
