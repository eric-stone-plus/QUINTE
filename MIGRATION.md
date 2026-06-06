# Migration Notice — 26-06-06

## What Changed

The QUINTE repository has been restructured from a Hermes-skill showcase to the **canonical protocol specification**.

### Before
- `SKILL.md` declared QUINTE "absorbed into multi-agent-debate skill"
- Protocol rules lived inside the Hermes skill file
- Dependency arrow: repo → skill (wrong direction)

### After
- `spec/PROTOCOL.md` is the normative protocol definition
- `hermes-skill/SKILL.md` is the reference implementation pointing to the spec
- Dependency arrow: skill → repo (correct)

## What This Means For You

### If you use QUINTE through Hermes Agent
**No change.** The multi-agent-debate skill still works identically. Its SKILL.md now references `spec/PROTOCOL.md` instead of self-defining the protocol.

### If you implement QUINTE in another system
The protocol is now defined in `spec/PROTOCOL.md` — a standalone, implementation-agnostic specification. Start there.

### If you maintain a QUINTE integration
Update any hard references from the old repo SKILL.md to `spec/PROTOCOL.md`.

## Timeline
- **26-06-06**: Restructure committed. Old SKILL.md archived to `references/archive/`.
- **26-07-06**: Grace period ends. Consumers should have updated references.

## Questions
Open an issue at [github.com/eric-stone-plus/QUINTE](https://github.com/eric-stone-plus/QUINTE).
