#!/bin/sh
# Prebuilt GitHub Releases are not published.
# Build from source and put CLI + scripts on PATH — see README.md Quick Start:
#   git clone … && cargo build --release
#   install target/release/quinte scripts/quinte-{progress,run} → PATH
#   quinte init && quinte doctor
# After pull: rebuild, reinstall binary, re-sync skills/SKILL.md into the host.
echo "quinte: prebuilt installer retired; build from source (README.md Quick Start)" >&2
exit 1
