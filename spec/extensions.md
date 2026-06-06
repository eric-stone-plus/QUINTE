# QUINTE Protocol — Extension Points

> What the protocol owns vs what implementations MAY vary.

---

## Protocol-Owned (Non-Negotiable)

These are defined in [PROTOCOL.md](PROTOCOL.md) and MUST be identical across all implementations:

1. **Agent count and roles**: 5 agents (hm, cc, cw, OMP, rx). R1=4, R2=5.
2. **Round structure**: R1→R2→R3, exactly 3 rounds, R3 must converge.
3. **Cross-review rule**: Review others, never self. R2 never skipped.
4. **Degradation thresholds**: 180s zero output → kill → retry. 3 failures → escalate.
5. **Model requirement**: All agents use same base model, no tier degradation.
6. **Trigger rules**: Mandatory/optional/skip categories.
7. **Versioning scheme**: `v<major>.<minor>` with defined increment rules.

---

## Implementation-Delegated (MAY Vary)

These are defined by each implementation's operational context:

### Agent Invocation
How each agent is called — CLI paths, flags, environment variables, PTY wrappers — is platform-specific.
- **macOS**: `script -q /dev/null claude -p`, `HOME=/Users/...`
- **Windows**: `claude -p --permission-mode bypassPermissions` (no script wrapper)
- **Linux**: (not yet documented)

### Output File Paths
Where R1/R2 outputs are written. Protocol only requires that outputs are separate, identifiable, and accessible to all agents in subsequent rounds.

### Timeout Tuning
The 180s threshold is the protocol default. Implementations MAY adjust for known platform quirks (e.g., Windows cc consistently needs 120s+).

### Platform-Specific Pitfalls
Each implementation maintains its own known-issues registry:
- Windows: cc timeout frequency, omp CLI vs Python script, cw Unix path handling
- macOS: HOME sandbox, keychain interference
- Linux: (TBD)

### Session Persistence
How debate outputs are archived for later retrieval. Protocol only requires that all R1+R2 outputs are preserved until R3 completes.

### Conformance Verification
Implementations SHOULD validate that their SKILL.md references the correct protocol version. Formal CI-based conformance testing is deferred until a second implementation exists.

---

## Extension Boundaries

| Concern | Protocol | Implementation |
|---------|:--------:|:--------------:|
| How many agents? | ✅ | — |
| What model tier? | ✅ | — |
| How to invoke each agent? | — | ✅ |
| Where to write outputs? | — | ✅ |
| How to archive sessions? | — | ✅ |
| How long to wait before retry? | ✅ (180s default) | ✅ (platform tuning) |
| What to do on persistent failure? | ✅ (escalate) | — |
| Prompt engineering for agent dispatch? | ✅ (§7) | ✅ (keyword blocklist) |

---

## Prompt Engineering (Anti-Drift)

> **Now in [PROTOCOL.md §7](PROTOCOL.md#7-agent-dispatch-requirements)**. This section retained for the implementation-delegated keyword blocklist only.

Each implementation maintains its own keyword blocklist for output validation (§7 Output Validation). Known drift patterns:

| Implementation | Known drift keywords |
|---------------|---------------------|
| macOS | hermes-desktop, AppIcon, dark mode, git hook, README formatting, commit range, update mechanism, build script, theme, icon |
| Windows | (TBD) |
| Linux | (TBD) |
