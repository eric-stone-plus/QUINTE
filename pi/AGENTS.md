# AGENTS.md — PI

PI is a minimal A2A v1.0 review seat agent: it receives one task from a
review orchestrator (QUINTE), calls one OpenAI-compatible model, and
returns one contract artifact (a lane output or an arbiter verdict). It
has no tools, no file access, and no prompt of its own beyond the seat
role it is launched with.

## What PI is not

- Not a coding agent, not a chat product. One task in, one artifact out.
- Not a scheduler. Task execution is one background thread per task;
  tasks are independent and the server never reorders them.
- Not a policy owner. The seat role (which review school, which phase
  contract) is chosen by the orchestrator at launch via `--role`; PI
  does not invent or override it.

## Rules

- Fail closed: a provider error, an unparsable model output, or a
  schema-invalid artifact turns the task terminal-failed — never a
  best-effort partial answer.
- Contracts are versioned. The schemas under `schemas/` are copied from
  the QUINTE release they pair with; changing one without the other is a
  contract break.
- Committed work is English, credential-free, and reproducible on a
  clean checkout (`cargo test` must pass).
- Preserve contributor identity: agent-authored commits use the agent's
  GitHub-linked Git author identity.
