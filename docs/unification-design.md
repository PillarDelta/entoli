# Design: suspending effects as handled effects

Status: IMPLEMENTED (migration M1–M7 complete, 2026-08-27) · resolves EVALUATION.md F-004 / R-7
Companion: `v0.6-revision-notes.md` item 3.1. Implementation migration: §8 below.

Implementation notes: the machine remains *permissive* at the perform site — a
handlerless suspending operation still suspends at machine level (the Phase 2 kill
harness depends on machine-level suspension with no runtime); the RUNTIME refuses it
as E001, which is where the design's "find the nearest Handle frame" rule lives.
Budgeted policy is spent at Intend (before the entry is written), so a refused spend
leaves no journal entry. Draft 2 incorporated the confirmed findings of the design's
own adversarial review
(D-1..D-4, recorded inline at the sections they changed): the all-paths-resume
condition on suspending clauses (D-1), the machine-local dispatch mask retained as
Cap[Local]'s dynamic proxy (D-2), the precise frontier definition (D-3), and the
dual-dispatch migration ordering (D-4).

---

## 1 · The problem

The paper's thesis is that durability, tool invocation, and authority are consequences
of **one** mechanism: typed handlers in capability-passing style. The v1
implementation has two mechanisms:

| | prompt effects | suspending effects (v1) |
|---|---|---|
| discharged by | in-language `handle … using κ` | host registry (`Runtime::register`) |
| capability at site | T-Handle's third premise, enforced | none (opt-in grant table bolted on for demo C) |
| chosen by | innermost installed handler | global, per-effect, fixed for the process |
| row discharge | at the handle | by fiat (the "required row" of the entry check) |

Consequences, in order of severity:

1. **Demonstration B's containment holds for half the language.** A delegated
   computation cannot reach an unhanded *prompt* tool; a suspending tool is reachable
   whenever the registry — a global — carries it. The paper's central security claim
   is proved only where the stakes are lowest.
2. **Authority at rest is a parallel construction.** Demo C works through the
   `require_grants` mode and a grant table that mirrors what capabilities should do,
   rather than through the capability the handler was installed with.
3. **Rule 3 needed a hand-made privilege boundary** (R-2's fix): "machine-local
   effects are refused past a root-continuation resume boundary" is a special rule
   compensating for the absence of capability discipline at suspending sites.
4. The checker must *reject* handlers for suspending effects (R-7) — a stopgap that
   makes the two mechanisms consistent by amputation.

## 2 · The core idea

**The five-step protocol is not a second mechanism beside handlers. It is what the
machine does to execute a suspending effect's handler clause.**

A suspending clause is already forced by T-Clause into the shape

```
E.op(x; k) -> <work> ; resume k r        -- resume in tail position (§4.3)
```

Read the protocol onto that shape:

| §7.1 step | clause execution |
|---|---|
| 1 · Intend (D) | at capture, before the clause body runs |
| 2 · Invoke (V) | the clause body — the crash window |
| 3 · Receive (V) | the clause body's result `r` |
| 4 · Commit (D) | at the clause's tail `resume k r`, before control transfers |
| 5 · Resume (V) | the transfer itself |

Everything the paper says about the effect site — the two atomic transitions, the one
ambiguous crash case, dispatch on δ — becomes a statement about *clause execution*,
and everything T-Handle says — installation requires `Cap[E, π]`, innermost handler
wins, discharge from the row — now governs suspending effects because they are simply
handled effects whose clauses the machine journals around.

The regress ("what handles the handler's own contact with the world?") bottoms out
where §8.3 already put it: the foreign-function boundary.

## 3 · Foreign invokers

New value form, applying §6.2's re-acquisition discipline to the outside world:

```
Foreign[τ1, τ2]            -- the type of a foreign invoker
foreign f : τ1 ~> τ2       -- top-level binding; f is host-implemented
```

Conditions, exactly parallel to §6.2's four:

- **Closed.** A foreign binding is top-level, captures nothing.
- **Referenced, not serialised.** It persists as its content identity (name + the
  declaring program's hash); the host re-binds it at load.
- **Bound at resume.** The invoker that runs on re-invocation is the one bound in the
  program *in force at recovery* — foreign bindings drift under compat tokens like
  handlers do (§10.2 applies to them because they are part of the handler).
- **Opaque.** A foreign application is not an effect and carries no row; it is the
  V-transition of step 2. It may fail (`Result`), and the delivery class of the
  operation whose clause calls it is a *claim about it* — discharged by the
  conformance harness, never by the calculus (§8.3 unchanged).

Foreign application is legal **only inside a suspending handler clause**. (Statically:
`Foreign` values are constructible only by `foreign` bindings, and T-Clause for a
suspending effect is the only rule that admits an application of one. This keeps
world-contact syntactically identifiable — the D/V annotations of §7.1 become
checkable properties of the term.)

## 4 · Typing changes

**T-Handle** — unchanged in statement, extended in force: it now covers suspending E.
Installing any handler requires `Γ ⊢ κ : Cap[E, π]`. The policy π (budget, lane —
§11) travels with the installation and is *checked and spent at each clause
execution*, which is where v1's grant-table `spend` moves.

**T-Clause for suspending E** — the existing side conditions, tightened by two:

```
Γu ; Γa, x : τ1, k : τ2 -[εk]-o τ' ⊢ e : τ' / ε
ε \ εk = {}                                        -- NEW: nothing but the tail resume
tail(resume k, e)
all_paths(resume k, e)                             -- NEW (D-1): every control path resumes
foreign applications appear only in e               -- and only here, globally
```

v0.5 required `ε \ εk ∩ Susp = {}` (no *suspending* work while servicing). For a
suspending clause v0.6 requires the clause's own row to be **empty**: the body is pure
computation plus foreign application plus the tail resume. Rationale: the body runs
inside the crash window with an intent journaled; a prompt excursion from inside the
window would transfer control out of it with the intent open, and a crash during the
excursion creates states the three-way split does not classify. Forbidden now;
§9 lists relaxation as an open question. Prompt clauses keep the v0.5 condition.

**All-paths-resume (D-1, review-confirmed blocker in draft 1).** The paper makes k
droppable — abort is a legal clause exit, and `tail(resume k, e)` constrains only the
resumes that *appear*. A suspending clause that aborts would exit the crash window
through a third door that is neither D-transition nor crash: the intent stays Pending
forever while the process runs on, and a later recovery would re-execute the clause —
possibly resuming this time, silently reversing the abort. So for suspending clauses
(only), every control path must end in `resume k`: the window has exactly two exits,
Commit and crash, which is what Theorem 1's exhaustiveness needs. Abort is not lost —
it moves to where §11 already puts decisions about failure: the foreign call returns a
`Result`, the clause resumes with it, and the *program* (whose continuation is
durable) decides. Cancellation of the whole computation stays §12's mechanism
(revocation at the root), unaffected. Prompt clauses keep abort unrestricted.

**T-Op** — unchanged. The row, the delivery classes, the witness obligation, and the
`{AtLeastOnce}` marker all stay as they are.

**Row discharge** — `handle` discharges suspending E from the row like any effect.
The v1 checker's "required row" escape hatch (suspending effects assumed discharged
by the registry) is deleted: a whole program must genuinely reach row `{}`, which is
the falsification criterion's first clause made real for the entire language.

## 5 · Machine semantics

`do E.op(v)` with E suspending, under the unified machine:

1. Find the nearest `Handle` frame for E (E001 if none — now *possible* for
   suspending effects, and demo B's containment covers them).
2. Capture K = the entire remaining computation **including that Handle frame**
   (deep handlers; re-suspensions of E in the resumed computation are handled by the
   same installation). This is unchanged from v1 — the whole stack was already
   captured; what is new is that the stack must *contain* the authority.
3. **Intend**: journal `Pending(#op, v, ι, κid)` + `encode(K)`, one transaction.
   Policy check: the installation's κ must be live; budgeted π is spent here.
   The Handle frame's κ is Kcaps — stripped to its effect name, as today.
4. **Invoke**: execute the clause body with `x = v` in the clause's environment.
   The foreign application inside it is the crash window.
5. **Commit**: when the clause reaches `resume k r` — mechanically, when the machine
   pops the clause boundary in tail position — write `Done(r)`. Single write.
6. **Resume**: transfer, as today.

**Recovery re-invocation** (`complete(j)` in v1) becomes: *re-execute the clause of
the journaled operation, with the journaled argument and witness, under the handler
installation in force at recovery.* This is strictly more faithful than v1's "call
the registered external": pure pre/post-processing inside the clause re-runs from the
journaled argument (deterministic, and not replay in the §7.1 sense — the
continuation is never re-derived), and the §10.2 merchant problem attaches to exactly
the right object, the handler, whose sig+compat identity is already what the store
verifies (F-007 falls out instead of being a special case).

The idempotent-completion rule from R-1 carries over verbatim: re-invocation against
an entry already `Done` returns the stored result.

## 6 · Authority, revocation, recovery

- **Authority at rest** now has one story. The suspended K contains the Handle frame;
  its κ persists as an effect-name slot (Rule 4, unchanged). At recovery, the restart
  entry point runs an ordinary ENTOLI **prelude** — the same `handle … using κ`
  nesting `main` used — around the driver. Decode re-binds slots against the
  installations in force; a missing installation is E014 naming the effect. Demo C's
  revocation flow is then literally "run recovery without installing the Infer
  handler," and budgets bind because the new κ's π governs clause execution.
  The v1 grant table survives only as the host-side mint for κ values.
- **Rule 3's privilege: static story plus a dynamic mask (amended per D-2).**
  Draft 1 claimed T-Handle alone closes R-2's leak. Review refuted that in the
  dynamic machine: T-Handle gates *installation*, dispatch is positional (innermost
  Handle frame, and the search must cross resume boundaries for ordinary nesting to
  work), and the prelude's `Local` installation necessarily sits *beneath* the frames
  of any continuation the driver resumes — reachable from unchecked resumed code by
  ordinary lookup. So the design keeps R-2's rule, promoted from hack to machine
  semantics: **machine-local effect dispatch never crosses a root-continuation resume
  boundary.** This is exactly the plan §3.4 pattern — the static guarantee is
  `Cap[Local]`'s absence from program typing contexts; the dispatch mask is its
  dynamic proxy, and it stays until a total checker exists. `driver_active` can still
  be deleted (the mask subsumes it). For *ordinary* effects the prelude's
  installations being reachable from resumed code is not a leak but §9.1's content:
  a resumed computation holds precisely the authority in force at resume time, and
  the prelude is that authority; checked programs remain bounded by their rows.
  `Protocol.complete` dissolves into the machine's ordinary clause re-execution; the
  machine-local class shrinks to the four pure reads of σ plus the mask above.
- **Compensations** are unchanged: the driver performs `do E.op'(…)` and the
  prelude's installations discharge it. §8.2's depth bound and E011 hold as-is.

## 7 · What this buys, concretely

1. Demonstration B extends to suspending tools: `delegate` handling only Http cannot
   claim `{}` while `research` posts to a Ledger — *whether or not* a ledger binding
   exists in the process. Escalation beyond the delegated envelope is impossible for
   the whole language.
2. Demo C stops being a mode (`require_grants` deleted); §9/§11 run through κ.
3. Rule 3 is clean again: one privileged capability, scoped by ordinary means.
4. The paper's "one mechanism" sentence becomes true, which was the thesis.
5. Handler compatibility (§10.2) attaches to the object that actually changes — the
   installed handler with its foreign bindings — rather than to a registry entry.

## 8 · Implementation migration (Phase 9.5, est. 3–5 sessions)

Re-sequenced per D-4 (review-confirmed: draft 1's ordering turned the demo suite red
from M2 until the final step, exactly while the crash matrices were needed as the
regression net). The invariant is now explicit: **every migration step ends with the
full suite green**, achieved by transitional dual dispatch.

- **M1** `entoli-syntax`: `Term::ForeignApp`, top-level `foreign` declarations in
  `Program` (name → content identity; host binds implementations at load). No
  behavior change; suite green.
- **M2** `entoli-core` + `entoli-runtime`, **dual dispatch**: a suspending `perform`
  uses the innermost Handle frame *if one exists*, else falls back to the registry
  path unchanged. Existing handlerless demos keep passing; new unit tests cover the
  handler path. The checker's R-7 rejection stays for now (parsed programs), while
  builder-level tests may install suspending handlers directly.
- **M3** clause execution around steps 1–5 for the handler path; policy spend at
  step 1; fixtures additionally exposed as foreign bindings. Registry path untouched.
- **M4** recovery: `recover(prelude, driver)`; re-invocation = clause re-execution
  when the entry's op resolves to a handler, stored-external otherwise; keep the
  machine-local dispatch mask (per §6 as amended — it is Cap[Local]'s dynamic proxy,
  not a casualty); delete only `driver_active`. Both R-2 regression tests must pass
  via the mask at every step.
- **M5** demos re-expressed **one at a time** with handler preludes (demo A, then A2,
  then C — C's revocation becomes omit-the-installation and `require_grants` folds
  into κ policy for that demo alone), each landing green before the next.
- **M6** `entoli-check`: lift the R-7 rejection; implement suspending-T-Clause
  (empty own row + all-paths-resume + foreign-only-here); extend demo B with a
  suspending-effect containment case.
- **M7** delete the registry fallback, `ExternalOp`, and `require_grants`; the R-7
  rejection is gone, dual dispatch collapses to handlers-only. Final green run is the
  unified machine.

## 9 · Open questions (recorded, not blocking)

0. ~~Abort from a suspending clause~~ — was listed as deferrable ("the clause
   decides", old 9.3); review showed it blocking (D-1) and it is now resolved in §4:
   all-paths-resume, with failure decisions moved to the program via `Result`.

1. **Prompt excursions from suspending clauses** are forbidden (empty-row condition).
   Legitimate wants exist (logging inside the window). Candidate relaxation: allow
   prompt effects whose handlers sit *inside* the clause, so the excursion cannot
   leave the window. Needs a crash-state analysis before admission.
2. **Nested suspending handlers for the same effect**: innermost wins (standard);
   compat identity is per-installation. Does quarantine key on (effect, installation)
   or effect alone? Proposal: installation, since §10.2's contract is the handler's.
3. **Foreign failure semantics**: `Result` at the clause level vs. a `Timeout`-style
   obligation row (§11). Proposal: foreign applications return `Result` and the
   clause decides; deadlines stay §11 obligations.
4. **Does `Cap[Local]` itself persist?** No — same as every κ. If the driver's own
   continuation (a compensation entry) is resumed, the recovery prelude re-grants it.
   This must be stated in the v0.6 text of Rule 3.
