#!/bin/sh
# Prebuilt GitHub Releases are no longer published.
# Build from source instead:
#   git clone https://github.com/eric-stone-plus/QUINTE.git
#   cd QUINTE && cargo build --release
#   install -m 0755 target/release/quinte "${HOME}/.local/bin/quinte"
echo "quinte: prebuilt installer retired; build from source (see README.md)" >&2
exit 1
