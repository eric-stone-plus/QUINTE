---
name: multi-agent-debate
description: "QUINTE Protocol reference implementation for Hermes Agent — R1四方(hm+cc+cw+omp)+R2五方(+rx)，全DeepSeek v4-pro。"
version: 2.2.1
protocol: "QUINTE v2.3"
spec: "../../spec/PROTOCOL.md"
---

# QUINTE — Hermes Agent Reference Implementation

> **Implements [QUINTE Protocol v2.3](../../spec/PROTOCOL.md).**
> For the canonical protocol specification, see [spec/PROTOCOL.md](../../spec/PROTOCOL.md).
> For extension points, see [spec/extensions.md](../../spec/extensions.md).

This skill is the **reference implementation** of the QUINTE five-agent debate protocol on Hermes Agent. It contains:

- Platform-specific invocation commands (macOS / Windows / Linux)
- Known pitfalls from 20+ debate sessions
- Operational references (setup guides, case studies)

**It does NOT redefine the protocol.** The protocol lives in `spec/PROTOCOL.md`.

---

## Quick Reference

| Agent | CLI (Windows) | Round |
|-------|--------------|-------|
| Claude Code | `claude -p --permission-mode bypassPermissions "..."` | R1+R2 |
| CodeWhale | `codewhale exec --auto "..."` | R1+R2 |
| OMP | `omp -p --model deepseek-v4-pro --thinking xhigh "..."` | R1+R2 |
| Reasonix | `reasonix run --model deepseek-v4-pro --effort max "..."` | R2 only |

All agents: DeepSeek v4-pro. Hermes/OMP: xhigh. Others: max. Flash forbidden.

## See Also

- [spec/PROTOCOL.md](../../spec/PROTOCOL.md) — canonical protocol
- [spec/extensions.md](../../spec/extensions.md) — what protocol owns vs implementations vary
- [references/](../../references/) — operational references, postmortems
