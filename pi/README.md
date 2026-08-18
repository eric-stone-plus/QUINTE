# PI

Minimal A2A v1.0 review seat agent for QUINTE: **one task in, one contract
artifact out.** No tools, no file access, no scheduler — one background
thread per task, one OpenAI-compatible completion, one schema-validated
artifact.

## Run

```sh
cargo build --release
DEEPSEEK_API_KEY=... ./target/release/pi serve \
  --role a-r1 --addr 127.0.0.1:8901
```

Seat roles: `a-r1` … `e-r1` (five schools, first pass), `a-r2` … `e-r2`
(adversarial cross-examination), `r3-arbiter` (counterpart arbitration).

## Protocol

- `GET /.well-known/agent.json` — agent card (A2A v1.0).
- `POST /` — JSON-RPC `SendMessage` (starts one task) and `GetTask`
  (state + artifact). Task records persist to `~/.pi/tasks.jsonl`.

## Contract

The artifact must validate against the QUINTE schema the seat pairs with
(`schemas/lane-output.schema.json` or `schemas/arbiter-verdict.schema.json`).
Fail closed: provider error, unparsable output, or schema violation all
turn the task terminal-failed — never a partial answer.
