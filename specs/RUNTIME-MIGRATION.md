# Runtime Binding Migration and Parser Incident Notes

This note records the boundary between the QUINTE product and a host that
keeps an ordered campaign. It is an operational contract for external,
headless callers; it does not add a `quinte migrate` command and it does not
change the QUINTE run state machine.

## One run, one coherent model family

Every production run has one declared seat binding. The `family`, `provider`,
`text_model`, and `multimodal_model` axes must agree across the five R1/R2
routes and the two R3 routes. Path instructions and wire-role names create
different review paths, not different business agents or model families.

The provider binding is a runtime configuration, not an account seat, quota
reservation, token-plan promise, or permanent product identity. A run may happen
to use DeepSeek today and another supported family after an explicit policy
change.
The host must record the effective binding in the normal policy and manifest
receipts; it must never infer a provider or model from a display name or from
model prose.

Provider credentials remain selected through the documented environment
selectors and are copied only into the assigned lane environment. A migration
receipt contains binding metadata and digests, never credential material.

## Headless host boundary

An external caller chooses one absolute state root and one absolute executable
for an invocation, computes the executable SHA-256, and passes the same values
to every operation:

```bash
export QUINTE_HOME=/absolute/path/to/quinte-state
export QUINTE_BIN=/absolute/path/to/quinte
export QUINTE_RUNTIME_SHA256="sha256:<64-hex-digest>"
"$QUINTE_BIN" host preflight --json
"$QUINTE_BIN" host start --brief /absolute/path/to/brief.json --json
"$QUINTE_BIN" host status RUN_ID --json
"$QUINTE_BIN" host inspect RUN_ID --json
```

If a launch response is lost, use `host reconcile`; do not start a second run.
The host consumes durable receipts and validates the state root, run identity,
manifest digest, result digest, and runtime digest. A preflight receipt is an
observation, not a reservation. QUINTE remains the sole owner of lanes, phases,
retry budgets, pacing, workers, and result merging.

For a Linux `Type=oneshot` coordinator that directly starts a detached QUINTE
worker, `KillMode=process` keeps the worker outside the coordinator's cleanup
boundary. `KillMode=mixed` is safe only after the worker has been durably
delegated to an independent unit or scope. Stopping the timer or coordinator
does not cancel a QUINTE run; cancellation is an explicit QUINTE operation.

## Explicit binding migration

Changing the executable used by an ordered outer campaign is a binding
migration, not a run migration. The safe transaction has these properties:

1. Acquire the campaign/tick and launch locks in the same order as the live
   coordinator.
2. Require the outer timer to be inactive, and require zero active or
   unresolved batches. A reviewed halt marker must be handled by its prescribed
   review/archive workflow before migration; never delete or edit it by hand.
3. Verify the old immutable executable still matches the current binding and
   verify the new executable at a different immutable path. Run the new
   executable's host preflight against the existing state root and require all
   seven routes to pass.
4. Read the ledger bytes and retain their SHA-256 as a compare-and-swap (CAS)
   value. Prepare an append-only migration receipt containing the `from` and
   `to` bindings, preflight evidence, old-ledger digest, and operator context.
5. Atomically append the receipt and update only the campaign's current runtime
   binding. Recheck the ledger CAS immediately before the binding write.
6. Leave every batch, attempt directory, run manifest, event log, host receipt,
   result, and historical runtime digest byte-for-byte unchanged.

The receipt pointer and digest form a chain. A later host may accept a
completed historical run made with an older executable only when its digest is
explicitly present in that chain and the run's manifest, inspect proof, result
bytes, and result digest all agree. A new launch must use the chain tail (the
current binding). A binary that merely happens to be available on `PATH` is
never an authorized historical runtime.

At minimum, an implementation should reject:

| Condition | Required outcome |
| --- | --- |
| active/unresolved batch or active timer | refuse without changing campaign state |
| old binary missing or digest drifted | refuse; preserve the old binding |
| new path equals the old path, or new digest equals old digest | refuse |
| preflight has fewer than seven passing routes | refuse |
| ledger CAS mismatch | refuse; leave any orphan receipt for review, never overwrite the ledger |
| missing/changed migration receipt or non-contiguous chain | refuse all subsequent launches |
| historical manifest/result mismatch or unauthorized digest | reject that historical proof |

There is no implicit rollback. If a rollback is required, perform another
explicit, locked binding migration to the preserved immutable executable and
retain both receipt links. Never replace a binary in place and never rewrite
old receipts to make a migration appear not to have happened.

## MiMo parser incident and contract

One observed MiMo JSON-events response contained a valid `LaneOutput` inside a
Markdown fence and then emitted the same object again as raw JSON before the
terminal control event. Treating the complete text payload as one JSON string
produced an `Extra data` parse failure even though a valid candidate was
present. The incident was a transport-shape problem, not evidence that a lane
had produced two independent findings.

The parser contract is now deliberately ordered and fail-closed:

- inspect text payloads and strong-shaped whole-value objects in source order;
- collect both fenced and raw LaneOutput-shaped candidates, de-duplicating only
  identical spans;
- select the last candidate as authoritative, because it is the model's final
  output position;
- validate that candidate against the closed schema; if it is malformed or
  schema-invalid, reject the stream rather than falling back to an older valid
  draft;
- treat control-event metadata and ordinary prose braces as non-candidates;
- preserve the typed provider error envelope separately from model text.

Regression coverage includes fenced-plus-raw duplication, a valid earlier
candidate followed by malformed or schema-invalid JSON, reordered malformed
objects, raw nested MiMo events, prose braces after a valid result, and typed
repetition errors. Raw output bytes and the event stream remain the diagnostic
authority; a host must not repair or concatenate them after the fact.

The remaining parser backlog is intentionally separate from runtime migration:
multi-event text assembly, explicit R3 authoritative-final semantics, nested
`OmpJson` wrappers, and tighter whole-value control-event handling each need a
dedicated fixture and contract decision before implementation.

## Operator evidence checklist

Before accepting a migrated campaign or a parser-related retry, retain:

- the exact executable paths and SHA-256 values;
- preflight, start, status, inspect, or reconcile receipt paths and digests;
- the run manifest, ordered events, and raw lane stdout/stderr bytes;
- the migration receipt and the ledger CAS digests;
- the reason for any named retry and the fact that old attempt history was not
  overwritten.

Do not put provider keys, private evidence paths, or raw sensitive prompts in a
public issue or repository document.
