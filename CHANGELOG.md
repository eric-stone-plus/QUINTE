# Changelog

All notable changes to the QUINTE product are documented in this file.

The product version is the Cargo package version. QUINTE-owned protocol,
schema, artifact, packet, receipt, CLI envelope, doctor, and retry/rate
fields follow that version.

## [0.1.4] - Unreleased

### Changed

- Unify the product version cohort on `0.1.4` (producers, schemas, fixtures,
  doctor/CLI envelopes, receipts, retry/rate state, docs). Residual test-era
  `1.0` constants are removed. JSON Schema dialect remains draft/2020-12.

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

## [0.1.3] - Failed release attempt

Tag `v0.1.3` exists, but GitHub tag CI run `29308295624` failed and publish was
skipped. Not a successful release.

## [0.1.2] - Failed release attempt

Tag `v0.1.2` exists, but GitHub tag CI run `29305574326` failed and publish was
skipped. Not a successful release.

## [0.1.1] - Failed release attempt

Tag `v0.1.1` exists, but GitHub tag CI run `29246274532` failed and publish was
skipped. Not a successful release.

## [0.1.0] - Released

First successful GitHub Release. Tag CI run `29218207576` succeeded.

A later deleted `v0.1.4` tag briefly triggered run `29349832363` and was
cancelled; no Release was published for that attempt.
