# QUINTE Phase Dispatcher Compatibility

> Compatibility status: retained for older host integrations. The Rust
> `quinte` CLI is the canonical full-run scheduler. Hermes must not combine
> this phase-only tool with a CLI run or treat its ledger as a CLI state
> transition.

This specification defines the earlier executable reliability layer for
host-bound, phase-only QUINTE dispatch. It exists because a failed party route is itself
material evidence: a five-party debate cannot silently become a four-party
debate, and a recovered route must be recovered as the same route.

## Contract

A dispatch manifest is a host-supplied binding. QUINTE does not choose
providers, credentials, models, aliases, or fallback parties.

A dispatch ledger is a factual attempt record. It records which required routes
ran, which route each attempt used, where stdout and stderr were captured, how
large each output artifact was, which failure class applied, and whether the
phase may progress.

R1 and R2 require exactly Party A, Party B, Party C, Party D, and Party E.
R3 requires Auditor B. An R3 manifest may keep the recorded Party A-E bindings
for traceability or leave `parties` empty, because only Auditor B is dispatched
in R3. hm is not an R1/R2 route and Auditor B never substitutes for a failed
R1/R2 party.

## Executable Entry Points

Use `bin/quinte-dispatch-phase.py` to run a phase from a JSON manifest. The
script validates the manifest, checks local prompt and executable references,
dispatches every required route, retries within the same route when allowed,
and prints a JSON ledger.

Relative executable paths in route commands are resolved from the dispatch
manifest directory before the process starts. The child process still runs with
the run output directory as its working directory so prompt, stdout, and stderr
artifacts remain grouped together. This prevents preflight from checking one
path while subprocess execution uses another.

Use `bin/validate-dispatch-manifest.py` before a run when the host wants to
check bindings without dispatching.

Use `bin/validate-dispatch-ledger.py` after a run or in CI to verify party
order, summary counts, route stability across attempts, output files, and the
phase progression flag.

## Failure Classes

`auth` means the route returned credential, login, permission, or authorization
failure evidence. It blocks immediately until the same route is repaired.

`deprecated` means the command, route, or model is unavailable or unsupported.
It blocks until the same route is repaired.

`rate_limit` means the provider or command signaled quota or rate limiting.
The dispatcher backs off and retries the same route within the attempt budget.

`timeout` means the route exceeded the manifest deadline. The dispatcher retries
with a shortened prompt inside the same route.

`empty_output` means the route exited successfully but produced a zero-byte
stdout artifact. The dispatcher retries with a shortened prompt inside the same
route.

`invalid_output` means the route exited successfully and wrote bytes, but the
first non-empty stdout line did not begin with `TASK:`. This usually indicates
a startup banner, shell wrapper, TUI, or wrong command target rather than a
completed party answer. The dispatcher retries the same route with a shortened
prompt and records degradation if the attempt budget is exhausted.

`dispatcher_exception` means the host-side dispatcher raised an unexpected
runtime exception while attempting a route. The failed party is recorded as
degraded with stderr evidence, the same route identity is preserved, and phase
progression is blocked rather than losing the run to an unledgered crash.

`interrupted_recoverable` means the process was interrupted in a recoverable
way, such as SIGTERM. The dispatcher retries or resumes only within the same
route.

`unknown` means no more specific class matched. The dispatcher retries within
the attempt budget and records degradation if attempts are exhausted.

## Phase Blocking

A ledger with `status: complete` sets `phase_progression_allowed` to true.

A ledger with `status: blocked` or `status: degraded` sets
`phase_progression_allowed` to false. The next QUINTE phase must not start from
that ledger. A degraded run may inform discussion, but it is not a complete
QUINTE evidence artifact for protected action boundaries.

## Same-Route Recovery

Every attempt for a party must preserve the same `route_id`. A retry may repair
credentials, working directory, prompt length, environment, or network
conditions, but it must not swap Party A-E, hm, Auditor B, or any other route
into the failed party slot.

The dispatch ledger stores a command hash rather than asserting that command
text is stable across future runs. Stability is enforced inside one ledger by
attempt route identity and by the host's manifest discipline.
