# Canonical mapping: paper v0.5 concepts → code

Phase 0 deliverable (plan §4). One row per semantic concept. If a concept has no row,
it is not implemented; if code has no row here, it must not carry semantics.

| Paper concept | Paper § | Code | Notes |
|---|---|---|---|
| Expression syntax `e` | §4.1 | `entoli_syntax::Term` | Pure data. No host closures anywhere in the AST. |
| Values `v` | §4.1 | `entoli_core::Value` | `Closure` captures `Env` + `Rc<Term>` body (data, not host fn). |
| Capability `κ` | §4.1, Rule 4 | `entoli_core::Value::Cap(CapId)` | Opaque runtime handle into `GrantTable`. **No encode arm** — Rule 4 obtained structurally (plan §3.4). |
| Handler `h` | §4.1 | `entoli_syntax::Handler` | `return x -> e` clause + op clauses `E.op(x; k) -> e`. |
| `handle e with h using κ` | §4.2 T-Handle | `Term::Handle` + `Frame::Handle` | Dynamic proxy for the third premise: κ must be a live grant for E, else E002. |
| `do E.op(v)` | §4.2 T-Op | `Term::Do` | Mandatory delivery class at declaration; ALO ops require a witness expression (dynamic proxy for the empty fourth row). |
| Affine continuation `k` | §5 | `entoli_core::Kont` (`consumed: bool`) | Dynamic proxy: second resume = E003 error (plan §3.4). |
| Continuation as uniquely-owned stack | §5.1, §7.3 | `Vec<Frame>` heap structure | The host call stack is never the semantic continuation; frames are representable **by construction** as they are pushed, so no conversion happens at suspension (§7.3 secondary condition of demo A). |
| Tail-position restriction on resume | §4.3 T-Clause | `Frame::ClauseBoundary` + check in `resume` | Dynamic proxy: frames between resume and its clause boundary ⇒ E004. |
| Nested suspension forbidden | §4.3 | suspension signal scan | A suspending effect crossing a live `ClauseBoundary` ⇒ E005. |
| Suspension classes `suspending`/`prompt` | §4.1 | `entoli_syntax::SuspensionClass` | Prompt ops are handled by in-language handlers; suspending ops route through the five-step protocol. |
| Delivery classes `δ` | §4.1, §8.1 | `entoli_syntax::DeliveryClass` | `Idempotent`, `AtLeastOnce` (witness), `Compensable{op'}`. |
| Compensations not compensable | §8.2 | declaration-load check | E011 at program load; bounds pending depth at 2 (I1). |
| Configuration `<e | K | σ>` | §7.1 | `Machine` (e, K) + `DurableStore` (σ) | |
| Five-step protocol | §7.1 | `entoli_runtime::protocol` | Step 1 Intend = one SQLite transaction writing Pending entry **and** encoded continuation. Step 4 = single `Done` update. |
| Store invariant INV(σ) I1–I3 | §7.2 | `entoli_store::invariants` | Checked after every durable transition, at shutdown, after restart, in every crash test (plan §11.4). |
| Rule 2 (total encode / atomic commit) | §7.3 | `entoli_encode` is pure & total over the language domain; store txns may fail as no-transitions | Encode errors are *language-domain* errors (Cap in frame), never I/O. |
| Rule 3 (recovery is ordinary) | §7.4 | `entoli_recovery::driver_for(program)` → ENTOLI `Term` | The driver is an ENTOLI program. Host supplies only machine-local effects (below). |
| Machine-local effect class | plan §8 | `Local.*` (pure reads of σ) + `Protocol.complete` (steps 2–4 on an *existing* intent) | The open question's presumed resolution, made concrete; boundary recorded in EVALUATION.md. |
| Recovery dispatch table | §8.1 | driver term + `frontier()` view | Latest journal entry: `Pending` ⇒ dispatch on δ; `Done` ⇒ resume with stored result; none ⇒ start of program. |
| Journal & pending stack | §7.2, plan §7.2 | `journal` table, `parent_journal_id` | No `Compensating`/`Retrying` states; recovery ops are ordinary nested entries. |
| Hierarchical continuation identity | §12 | dotted string ids `"0"`, `"1.0"` | Concurrency commitment 1. |
| Content addressing | §10.1 | `entoli_encode::hash` | SHA-256 Merkle over canonical AST, closed over referenced defs (SCC groups hashed name-relative). Verified at decode. |
| Handler compatibility tokens | §10.2 | `DurableContinuation.handled_effects` | Recorded now; enforced in Phase 7. |
| Authority at rest, K = Kframes ⊗ Kcaps | §9.1 | `Cap` encodes as `CapSlot{effect}` name only | No policy/budget/credential bytes persisted; decode re-binds from the *current* grant table or refuses with E014. |
| Data classification of stores | §9.2 | `DurableContinuation.data_classification` | Derived join; refined in Phase 6 (all-public until a policy table exists). |
| Diagnostics E001–E014 | plan §12 | `entoli_core::EntoliError` | Used by dynamic proxies now, by the checker in Phase 5. |

## Crash points (plan §9)

`before_intend | after_intend | after_remote_before_receive | after_receive_before_commit
| after_commit | before_resume` — implemented in `entoli_runtime::CrashInjector`, honored
inside fixtures for `after_remote_before_receive`. Crash = `std::process::exit(87)` (no
Rust destructors run; nothing volatile is flushed). Tests restart via a fresh subprocess.
