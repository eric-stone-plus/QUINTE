# QUINTE — Reference Implementations

> **Protocol spec**: [spec/PROTOCOL.md](../spec/PROTOCOL.md). QUINTE is platform-agnostic — any orchestrator running any OS can implement it. This file points to working implementations.

## Current Implementations

| Platform | Repo | Status |
|----------|------|--------|
| macOS | [hermes-core-rules-mac-x86](https://github.com/eric-stone-plus/hermes-core-rules-mac-x86) → `skills/multi-agent-debate/SKILL.md` | ✅ active (20+ sessions) |
| Linux | [hermes-core-rules-linux](https://github.com/eric-stone-plus/hermes-core-rules-linux) | 🆕 planned (Arch) |
| Windows | native Hermes + agent CLI toolchain | ✅ verified (all 5 agents operational; see extensions.md) |

**Why redirects**: Maintaining duplicates in the spec repo caused version drift. The protocol spec defines *what*; implementations define *how*.
