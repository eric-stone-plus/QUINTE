# Changelog

All notable changes to the QUINTE product are documented in this file.

The product version is the Cargo package version. QUINTE-owned protocol,
schema, artifact, packet, receipt, CLI envelope, doctor, and retry/rate
fields follow that version.

This tree is a **test build** with no external users. Failed historical tags
are not treated as successful releases.

## [0.1.1] - Unreleased (test)

Current product version for this checkout.

### Changed

- Unify the product version cohort on `0.1.1` (producers, schemas, fixtures,
  doctor/CLI envelopes, receipts, retry/rate state, docs). JSON Schema dialect
  remains draft/2020-12.

### Fixed

- Validate claim and residual ID uniqueness for accepted lane and arbiter
  artifacts.
- Validate HM verdict residual evidence and closure references against the
  exact snapshot before merge.
- Expand residual merge conflict detection beyond finding/disposition/closure
  to severity, type, source, evidence, required_closure, and scope.
- Conservatively preserve high-risk R1/R2 residuals when both R3 arbiters omit
  them, with an explicit dissent trail.
- Contain Windows adapter process trees with kill-on-close Job Objects
  (suspended spawn, assign, resume). Post-leader residual cleanup is no longer
  a no-op. Real-machine Windows acceptance remains an open release gate.

### Notes

- Cross-run residual closure/waiver history and Action Packets remain HIGHBALL
  concerns. QUINTE `result.json` stays immutable per run.
- Earlier local work temporarily used `0.1.4` as a working cohort label; the
  product number was renumbered to `0.1.1` for this test line (no users, no
  successful intermediate releases).

## [0.1.3] - Failed release attempt

Tag `v0.1.3` existed, but tag CI failed and publish was skipped. Not a
successful release. Superseded by the current test line.

## [0.1.2] - Failed release attempt

Tag `v0.1.2` existed, but tag CI failed and publish was skipped. Not a
successful release.

## [0.1.1] - Failed release attempt (historical tag)

A previous `v0.1.1` tag existed with failed tag CI and no successful GitHub
Release. That tag is treated as abandoned; the current unreleased test cohort
reclaims `0.1.1` as the product version number for this line.

## [0.1.0] - Released

First (and only) successful GitHub Release at the time of this note.

A later deleted `v0.1.4` tag was cancelled with no Release published.
