# ENTOLI

**A language where crash recovery, exactly-once effects, and capability
safety are type-checked — not engineered.**

Kill an ENTOLI program at any instruction. It resumes cold — new process,
new machine, a week later — exactly where it stopped. No external effect is
repeated. None is lost. And the file it slept in could not have acted on
anyone's behalf, because there is no authority inside it to steal.

None of this is a careful runtime doing its best. It is what the checker
establishes about a program before it is allowed to run.

ENTOLI exists for programs that act on the real world and cannot get it
wrong: agents that call a model, wait days for a human decision, then
publish, pay, or deploy. Building one today means buying three guarantees
from three different places — a workflow engine for durability, idempotency
keys and retry wrappers for exactly-once delivery, credential hygiene so a
snapshot of the sleeping agent can't be replayed with its secrets inside —
and then holding them together by care. ENTOLI derives all three from a
single mechanism: **typed algebraic effect handlers with one-shot
continuations**.

- Every external operation is a declared **effect**, journaled with its
  saved continuation *before* the world is touched.
- Every effect states **how it recovers, in its type**. A program that
  performs an effect it cannot recover does not compile.
- **Capabilities have no serialized form.** What sleeps on disk is data,
  with zero power.

Here is the whole idea in one program — the pilot this repository ships and
tests ([`pilot/brief.ent`](pilot/brief.ent)):

```text
effect Infer    suspending { complete : Str -> Str  @at_least_once }
effect Approval suspending { request  : Str -> Data @at_least_once }
effect Publish  suspending { post     : Str -> Str  @at_least_once }

def brief : Str -[Infer, Approval, Publish]-> Data =
  fn topic ->
    let draft = do Infer.complete(concat("Write a crisp 150-word brief on: ", topic))
                witness concat("draft:", topic) in
    match do Approval.request(draft) witness concat("gate:", topic) {
      approve() -> Published(do Publish.post(draft) witness concat("pub:", topic))
      reject()  -> Halted(topic)
    }
```

No queue, no state machine, no retry wrapper — plain sequential code. Yet
kill the process mid-draft, and recovery replays under the same witness: the
model is never paid twice. The approval can take a week, with nothing
running and no credential on disk. Crash during publish and recover as many
times as you like — the post happens exactly once. All of it follows from
the three `@at_least_once` annotations above.

ENTOLI is designed to be **written by models and verified by a checker**:
agents propose programs, the checker is the hard boundary, and only valid
programs execute.

![The checked-generator loop](docs/figures/pipeline.svg)

## The three guarantees

**Nothing is lost.** Every external operation runs through a five-step
protocol that journals intent — and the saved continuation — *before* any
contact with the world. A crash at any point leaves a store from which exactly
one recovery action follows.

![The effect protocol](docs/figures/protocol.svg)

**Nothing happens twice.** Every operation declares how it recovers
(`@idempotent`, `@at_least_once` with a witness, `@compensable`). A program
that performs an unrecoverable effect does not compile. Recovery re-invokes
under the recorded witness; services deduplicate; results already committed
are delivered, never re-executed.

**Authority cannot sleep.** Capabilities never cross the durable boundary. A
suspended agent on disk — or in transit — contains data and *zero* power;
resuming re-grants under whatever policy holds *now*. Revocation is not a
protocol; it is an absence.

![Authority at rest](docs/figures/authority.svg)

## Getting started

All you need is a [Rust toolchain](https://rustup.rs) (stable). SQLite is
bundled — there are no system dependencies.

```sh
git clone https://github.com/PillarDelta/entoli.git
cd entoli
cargo build --release -p entoli-cli

# the full crash/recovery suite ships in-tree
cargo test --workspace
```

The binary lands at `./target/release/entoli-cli`; the walkthrough below uses
it as `$B`.

## Seen end to end

The pilot in [`pilot/brief.ent`](pilot/brief.ent): a model drafts a brief, a
human approves it — with the process dead in between, for as long as it takes
— and the result is published exactly once.

![The pilot](docs/figures/pilot.svg)

```sh
B=./target/release/entoli-cli

# run: the draft happens, the human gate suspends, the process exits
$B run pilot/brief.ent --dir pilot-store \
  --grant Infer --grant Approval --grant Publish \
  --bind "llm_svc=echo:Infer.complete" \
  --bind "approval_svc=approve:Approval.request" \
  --bind "publish_svc=ledger:Publish.post"

# whenever the human decides — minutes or days later
$B approve --dir pilot-store \
  --witness "gate:durable execution for AI agents" --decision approve

# recover cold: completes, publishes exactly once
$B recover pilot/brief.ent --dir pilot-store \
  --grant Infer --grant Approval --grant Publish \
  --bind "llm_svc=echo:Infer.complete" \
  --bind "approval_svc=approve:Approval.request" \
  --bind "publish_svc=ledger:Publish.post"
```

Swap `echo` for the real model binding
(`--bind "llm_svc=llm:Infer.complete:<model>"`, key via environment) and
`ledger` for a real webhook (`--bind "publish_svc=http:Publish.post:<url>"`)
— see [`docs/pilot-runbook.md`](docs/pilot-runbook.md). Kill the process at
any step; `recover` completes with exactly-once effects. The crash matrix is
pinned by the test suite.

## What is established, measured, and open

- **Proved** (formal machine, [`docs/metatheory-v0.6.md`](docs/metatheory-v0.6.md)):
  preservation of the store invariant; recovery-action selection computable
  from the store alone; authority at rest.
- **Tested under hard process death**: kill-and-recover at every protocol
  step; revocation while sleeping; compatibility drift quarantine; the full
  suite ships in-tree.
- **Measured**: in a two-round adversarial study, closed-book model-written
  programs never violated the semantic restrictions in any checked generation
  (corpus in [`docs/convergence-study-2026-11-08/`](docs/convergence-study-2026-11-08/));
  per-effect runtime cost is flat as the journal grows
  ([`docs/bench-2026-11-09-post-T3.csv`](docs/bench-2026-11-09-post-T3.csv)).
- **Open, recorded**: every known gap and deliberate deferral is in
  [`docs/EVALUATION.md`](docs/EVALUATION.md) and
  [`docs/production-roadmap.md`](docs/production-roadmap.md). This is a
  research-grade reference implementation on its way to production, and its
  claims are calibrated accordingly.

## The paper

The working paper is [`paper/ENTOLI_V3.pdf`](paper/ENTOLI_V3.pdf) —
semantics, theorems, the authorship study, and the evaluation. A companion
position paper on a network of mobile ENTOLI agents over attested hosts is
[`paper/capsule-network.tex`](paper/capsule-network.tex).

## Repository map

| Path | Contents |
|---|---|
| `crates/entoli-syntax` | AST, effect/delivery/suspension declarations |
| `crates/entoli-core` | CEK-style evaluator, one-shot continuations |
| `crates/entoli-encode` | closed codec, content addressing, sharing-preserving encoding |
| `crates/entoli-store` | durable store (SQLite), journal, invariants |
| `crates/entoli-runtime` | the five-step protocol, grants, recovery execution |
| `crates/entoli-recovery` | the recovery driver, generated as ENTOLI terms |
| `crates/entoli-check` | parser and static checker (the boundary) |
| `crates/entoli-cli` | `compile` / `run` / `approve` / `recover` |
| `pilot/`, `docs/` | the pilot program, papers' evidence, roadmap, findings register |

---

Copyright © 2026 Pillar Delta PC. All rights reserved. See [LICENSE](LICENSE).
