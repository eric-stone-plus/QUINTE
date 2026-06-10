# QUINTE Protocol — Design Lessons

> Extracted from POSTMORTEM.md. System-level AI engineering principles learned through QUINTE development and operation.

---

## 1. API-Only = Fake Multi-Perspective

Same model with different prompt parameters does not produce genuine diversity. `reasoning_effort=high` vs `max` is quantitative variance, not qualitative. True multi-agent debate requires different code paths, tool chains, and reasoning engines — the distinction between CLI QUINTE and API simulation.

## 2. Single-Model "Diversity" Is Worse Than None

When you believe you have N independent perspectives but actually have 1 model wearing N masks, you make decisions with false rigor. **Known uncertainty is safer than manufactured consensus.**

## 3. Prompts Are Not Guardrails

Safety, compliance, and process constraints must be programmatic — not written in prompts expecting agents to comply. If an agent shouldn't do something, don't give it that capability.

## 4. Agent Coordination Requires Formal Protocol

R1→R2→R3 must be enforced by the orchestrator's state machine, structured outputs, and validation logic. Agents drift. The orchestrator is a referee, not just a message router.

## 5. No Observability = Blind Operation

Every prompt, response, timestamp, and tool call must be traceable. Observability infrastructure should be built before agents, not retrofitted after bugs appear.

## 6. Agent Behavior Needs Regression Tests

LLM outputs are non-deterministic, but constraint violations are detectable. "Did the agent complete R1→R2→R3?" "Was rx placed in R1?" — these are automatable verification checks.

## 7. Execution by Framework, Not Agent Discipline

"rx never enters R1" written in the skill is useless — the orchestrator must hard-block it. Agents "forget" rules; frameworks must not.

## 8. Impersonating Debate Is Worse Than Skipping It

"I analyzed it myself, conclusion is..." — skipping QUINTE leaves a detectable gap. Fabricated debate is silent, leaving no evidence. The orchestrator must verify that cc/cw/omp background processes actually completed with output.

## 9. Middleware Is an Invisible Failure Point

Any middleware between orchestrator and sub-agent is a potential silent failure point. Example: Hermes API key masking replaced `$KEY` with `***` → cc received 401 → orchestrator couldn't see this failure because there was no observability.

---

## Application

These lessons inform the QUINTE protocol design at every level:
- **Protocol invariants** (lessons 1, 2, 4): why R1+R2+R3 are non-negotiable
- **Implementation requirements** (lessons 3, 5, 6, 7): what every QUINTE implementation MUST provide
- **Operational discipline** (lessons 8, 9): what orchestrators MUST verify

> These are protocol-level principles. Implementation-specific lessons remain in implementation repos.
