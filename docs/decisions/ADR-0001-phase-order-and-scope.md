# ADR-0001 · Phase order, scope, and initial deviations

Date: 2026-08-27 · Status: accepted

## Decisions

1. **Phase order per plan v1.0 §4**: evaluator and continuation encoding before any static
   checker. Every static guarantee has a dynamic proxy from Phase 1 (plan §3.4).
2. **Crates per plan §6**, except `entoli-types`/`entoli-check` which are created in
   Phase 5 (they have no Phase 0–4 content).
3. **Dependencies**: `rusqlite` (bundled SQLite) and `sha2` only. The frame/value encoding
   is hand-rolled and deterministic — serde would blur exactly the property Phase 2 tests
   (a closed, stable, capability-free encoding).
4. **Hard death = `std::process::exit(87)`** at injected crash points: skips all Rust
   destructors (no Drop, no buffered flush, SQLite connection not closed cleanly), which
   is the strictness the plan asks for without SIGABRT's crash-reporter noise on macOS.
   Restart is always a fresh subprocess reading durable state only.
5. **Closure code is encoded as canonical AST inline; global references by name, with the
   name→def-hash table recorded at encode time and verified at decode.** Per-def hashes
   are Merkle-closed over dependencies (mutually recursive defs hash as an SCC group with
   name-relative internal references), so code cannot drift beneath a sleeping
   continuation (§10.1) while a store stays readable by the same program.
6. **Capabilities encode as `CapSlot { effect }` — a name, not authority.** This is the
   K = Kframes ⊗ Kcaps split of §9.1 in mechanical form: no policy, budget, or credential
   bytes cross the boundary; decode re-binds slots from the grant table in force at
   resume, and a missing grant is an E014 refusal naming the effect (demo C's shape).
7. **Recovery frontier rule.** Recovery loads the journal and takes the latest entry:
   `Pending` ⇒ dispatch on its delivery class (§8.1 middle row); `Done` ⇒ resume its
   continuation with the stored result (§8.1 last row); empty ⇒ nothing external ever
   happened, run the program from the start (§8.1 first row, degenerate case). With
   nested recovery entries this reproduces the §7.4 derived state sequence without any
   Compensating/Retrying states.
8. **On "no replay" (plan §3.3).** What is forbidden is *reconstructing* a continuation
   by re-executing pure code from an earlier durable point. Resuming the continuation
   committed at Intend — including re-running the pure segment that follows a committed
   `Done` whose process died before the next Intend — is the paper's own recovery action
   ("resume the named continuation with r") and is not replay: the continuation that
   receives the result is read from σ, never re-derived.
9. **The recovery driver is generated ENTOLI terms** (`driver_for(program)`), one match
   arm per compensable op declared by the program. The generator is host code; its output
   is ordinary ENTOLI syntax, printable and inspectable — no hidden recovery machinery.
   Machine-local effects granted only to the driver: `Local.frontier`, `Local.load_kont`,
   `Local.load_result`, `Local.comp_arg` (pure reads of σ) and `Protocol.complete(j)`
   (steps 2–4 of the §7.1 protocol applied to an existing intent — the store's atomic
   commit primitive plus the op's own invocation, which the calculus already represents).
   This is the plan §8 presumed resolution; Phase 4 records the exact boundary.
10. **ALO witnesses for compensations are derived deterministically** as `"comp:" + j`
    (j = the journal id being compensated), so a crashed compensation retries under the
    same witness without storing anything extra.

## Deviations from plan v1.0 (recorded, per §3.1)

- OCaml 5 oracle (plan §11.1): skipped — OCaml is not installed on this machine. The
  evaluator is instead validated by direct unit tests of handler semantics (which handler
  fires, in what order, with what continuation). Revisit if a semantic dispute arises.
- `examples/*.ent` files are deferred until a concrete syntax exists (Phase 5 demo D);
  demo programs live as AST builders in `entoli-testkit` for now.
