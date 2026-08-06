---
name: quinte
description: Invoke, observe, inspect, or recover the QUINTE single-model-family adversarial review through its stable host CLI contract. Use when the user explicitly asks for QUINTE, a fixed five-path R1/R2 review with two same-family R3 verdict bindings, residual exposure, or continuation of an existing QUINTE run. Do not use it as a generic delegator or to invoke an individual execution binding.
---

# QUINTE Host

Use the installed `quinte` CLI as the sole execution authority. External agents
must use the `quinte host` command group. Do not recreate R1/R2/R3 with model
calls, delegation, shell loops, provider calls, or individual Party/Arbiter
commands. Party and Arbiter names are wire-role identifiers, not separate
business agents.

QUINTE has one declared model family per run: five independent R1 paths, five
anonymized R2 rechecks, a Counterpart Arbiter, and a Primary Arbiter. The
effective provider and model names come from the validated policy and may
change between installations; never encode the current binding as a permanent
plan, seat, or protocol promise.

## Preflight

1. Confirm `quinte --version` and `quinte host --help` resolve from PATH.
2. Run `quinte host preflight --json`. Initialize once with
   `quinte init --json` only when the state root is uninitialized, then rerun
   preflight. Never use `init --force` without an explicit migration reason.
3. Require `data.state.code=ready` only as an advisory snapshot. It is not a
   reservation or authorization ticket: state can change after preflight, and
   `host start` must acquire its launch lock and rerun doctor/active-run checks.
   `active_run_present` means observe or recover that run; it is not permission
   to start another one.
4. Remember that doctor checks local executables, credentials, and adapter
   contracts. A route reporting `provider_live_probe=false` has not proved
   endpoint reachability.
5. Leave provider proxy mode at its default `inherit` unless the endpoint has
   been explicitly verified for direct egress. `QUINTE_PROVIDER_PROXY_MODE=direct`
   is an endpoint routing choice, never a model-family property.

## Brief

Write a current Brief outside the run directory using the installed schema or
canonical example. Include only the question, context, evidence roots,
attachments, action scope, and affected paths the user placed in scope.

Validate before spending a run:

```bash
quinte validate --kind brief BRIEF.json --json
```

Keep evidence bounded and readable. Use `snapshot_ignore` for generated or
irrelevant trees. Outputs may cite only exact `snapshot://` and
`attachment://` references from the persisted snapshot manifest. If the Brief
contains attachments, require every bound route in preflight to report the
needed native attachment capability.

## Start and observe

Start detached through the atomic host boundary:

```bash
quinte host start --brief BRIEF.json --json
```

Record `data.invocation_id`, `data.run_id`, the receipt path, and the manifest
hashes. The QUINTE-owned launch lock and fail-closed run scan enforce one active
run for callers that use this boundary. Never use low-level `quinte run` for
agent automation, and never use `run --wait` or bare `wait` in an interactive
agent turn.

Observe with separate one-shot calls, normally 30–60 seconds apart:

```bash
quinte host status RUN_ID --json
```

Do not wrap observations in a sleeping shell loop. Branch on
`data.manifest.status`, not exit code or `state.code` alone. Human display
helpers may supplement the receipt but are not the machine API.

## Recovery

If the start response is lost, do not launch again:

```bash
quinte host reconcile --json
```

Reconcile identifies durable state; it never advances, retries, resumes, or
cancels a run. Use `quinte resume RUN_ID --json` only after observing a dead or
interrupted scheduler. Use `quinte cancel RUN_ID --json` only with explicit
cancellation authority. Never retry an individual lane or edit scheduler-owned
manifests, events, receipts, attempt directories, or results.

On failure, retain stderr and exit code because CLI errors do not guarantee a
JSON envelope. Inspect the stored manifest error and attempt events before
changing the Brief, policy, network, or provider configuration. Fix the cause,
then create one new run only after the previous run is terminal.

## Accept a result

Queued, running, waiting, failed, cancelled, static HTML, and partial artifacts
are not QUINTE results. At `completed` or `degraded`, run:

```bash
quinte host inspect RUN_ID --json
```

Require `data.result.verified=true`, a matching manifest/result digest, and the
expected result contract. `result.actionable` describes contract currency, not
authorization for an external write, deployment, purchase, deletion, or other
protected action. Preserve dissent, limitations, uncertainties, and open
residuals.

An external cross-family audit is explicit, optional, and post-completion. It
is not a QUINTE phase and must never be presented as part of automatic QUINTE
completion. Read `specs/HOST.md` for host receipts and `specs/PROTOCOL.md` only
when protocol interpretation is required.
