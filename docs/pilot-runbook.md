# Pilot runbook — the approval-gated brief writer

The pilot (`pilot/brief.ent`): a REAL LLM drafts a brief, a REAL human approves
it — with the process dead in between, for as long as it takes — and the result
is published to a REAL webhook exactly once. Every step survives `kill -9`.

## One-time setup

```sh
cargo build --release -p entoli-cli
export ANTHROPIC_API_KEY=...            # your key; read at invocation, never logged
export HOOK=https://webhook.site/<your-uuid>   # or any POST endpoint you control
```

## The flow

```sh
B=./target/release/entoli-cli
ARGS=(pilot/brief.ent --dir pilot-store
  --grant Infer --grant Approval --grant Publish
  --bind "llm_svc=llm:Infer.complete:<model-id>"
  --bind "approval_svc=approve:Approval.request"
  --bind "publish_svc=http:Publish.post:$HOOK")

# 1. Run: the LLM drafts (cached under its witness — recovery replays the SAME
#    draft, no double spend), then the human gate suspends and the process EXITS.
$B run $ARGS      # -> "pending human approval (gate:...)" ; exit 1; store persists

# 2. Whenever you decide — minutes or days later, machine rebooted or not:
$B approve --dir pilot-store --witness "gate:durable execution for AI agents" --decision approve

# 3. Recover cold from source: completes and POSTs the draft to $HOOK once.
$B recover $ARGS  # -> RESULT Published(...)
```

Reject instead: `--decision reject` → `recover` ends `Halted(...)`, nothing posted.

## What to try breaking

- `kill -9` the run at any point; `recover` with the same ARGS. The journal's
  recorded witnesses make the LLM call replay from cache and the publish
  deduplicate at the receiver (the witness travels in the POST body).
- Revoke while sleeping: `recover` WITHOUT `--grant Publish` → refusal naming
  Publish; the entry stays pending; re-grant and recover again.
- Inspect the store: `$B check --dir pilot-store`.

## Notes

- The `llm:` binding honours its declared at-least-once contract via a local
  witness-keyed completion cache (`pilot-store/llm-cache/`).
- The `http:` binding is honestly at-least-once: dedup is the receiver's half
  of the contract, keyed on the witness in the payload.
- CI covers the full gate mechanic with the LLM substituted by echo
  (`crates/entoli-cli/tests/pilot.rs`); the live LLM/webhook run is this
  runbook.
