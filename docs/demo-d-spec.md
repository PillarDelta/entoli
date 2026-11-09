# ENTOLI — language reference card (demo D hand-out)

ENTOLI is a small language for durable, capability-safe agent orchestration. Programs
are a series of `effect` declarations and `def` definitions. Comments start with `#`.

## Types

`Unit`, `Int`, `Bool`, `Str`, `Data` (the type of all constructor values),
`Cap[E]` (a capability for effect E), function types `T1 -[Row]-> T2`, and
continuation types `T1 -[Row]-o T2`. `Row` is a comma-separated set of effect names a
computation may perform, e.g. `Str -[Ledger, Approval]-> Data`. The empty row `-[]->`
means pure. **A definition's body may not perform any effect missing from its declared
row** — effects propagate to every caller until handled.

## Effects

```
effect Ledger suspending { post : Data -> Data @at_least_once }
effect Log    prompt     { info : Str  -> Unit @idempotent }
```

Every operation declares a delivery class: `@idempotent`, `@at_least_once`, or
`@compensable(Effect.op)`. `suspending` operations may suspend the program durably
(tool calls, approvals); `prompt` operations are handled by in-language handlers.

Rules:
- An `@at_least_once` operation MUST be performed with an idempotency witness:
  `do Ledger.post(entry) witness concat("idem:", id)` — the witness is a `Str`.
  Without it the program does not compile.
- A `@compensable` op's compensation must itself be `@idempotent` or `@at_least_once`.

## Expressions

```
fn x -> e                      # lambda (one parameter)
f(a, b)                        # application (curried sugar)
let x = e1 in e2
if cond then e1 else e2        # cond : Bool
match e { Ctor(x, y) -> e1  None -> e2  other -> e3 }   # bare Capitalised = nullary ctor; lowercase = catch-all binder
Audited(inv, 250)              # constructor: Capitalised identifier, type Data
do E.op(arg)                   # perform an operation
do E.op(arg) witness w         # perform with idempotency witness
handle e with E { op(x; k) -> body  return r -> body2 } using cap
resume k v                     # invoke a continuation (AT MOST ONCE)
1 + 2 * 3, a < b, a == b (Int), str_eq(s1, s2), concat(s1, s2), str_of_int(n)
```

## Handlers and continuations

`handle e with E { ... } using cap` runs `e`; when `e` performs a prompt operation of
E, the matching clause runs with the argument `x` and the continuation `k`. The `cap`
expression must be a `Cap[E]` — writing a handler does not entitle you to install it.
Handlers are deep: after `resume`, later operations of E are handled by the same
handler. The optional `return r -> ...` clause transforms the final result of `e`.

Restrictions the checker enforces:
- **Affinity**: a continuation may be resumed AT MOST once. Binding it and resuming
  twice is a compile error (E003). Not resuming at all is fine (that aborts `e` and
  the clause's value becomes the handle's result).
- **Tail position**: inside a clause, `resume k v` may appear only in tail position —
  as the clause's final expression (possibly under `let ... in <tail>`, `if`, or
  `match` arms). `let a = resume k 1 in a + 1` is a compile error (E004). To
  post-process the result of the handled computation, use the `return` clause.
- **No suspension while servicing**: a handler clause may not perform a `suspending`
  operation before it resumes (E005). Do suspending work outside the handler.

## Worked example 1 — settle an invoice

```
effect Approval suspending { request : Data -> Data @at_least_once }
effect Ledger   suspending { post    : Data -> Data @at_least_once }

def audit : Str -[]-> Data = fn inv -> Audited(inv, 250)

def settle : Str -[Approval, Ledger]-> Data =
  fn inv ->
    let f = audit(inv) in
    match do Approval.request(f) witness concat("req:", inv) {
      approve() -> Posted(do Ledger.post(f) witness concat("idem:", inv))
      reject()  -> Halted(f)
    }
```

The approval service answers with the constructors `approve()` or `reject()`.

## Worked example 2 — a prompt handler with a return clause

```
effect Ask prompt { ask : Int -> Int @idempotent }

def compute : Unit -[Ask]-> Int = fn u -> do Ask.ask(10) + do Ask.ask(20)

# Doubles every answer, then adds 1 to the final result.
def run_it : Cap[Ask] -[]-> Int =
  fn cap ->
    handle compute(()) with Ask {
      ask(x; k) -> resume k (x * 2)
      return r  -> r + 1
    } using cap
```

Note the pattern: per-operation transformation happens in the clause (before
`resume`, in tail position); whole-result transformation happens in `return`.

## Runnable programs

A file meant to RUN (not just compile) additionally declares its host services
and an entry point:

```
foreign ledger_svc : Data ~> Data        # host-implemented; usable only inside
                                         # a suspending handler clause
def main : Cap[Ledger] -[]-> Unit -[]-> Data =
  fn cl -> fn u ->
    handle work(()) with Ledger { post(x; k) -> resume k (ledger_svc(x)) } using cl
```

`main` takes one capability parameter per granted effect (in grant order), then
`Unit`; every row in its type must be empty — `entoli compile` enforces this
(E001) whenever `main` is present. Run and recover:

```
entoli run    prog.ent --dir D --grant Ledger --bind ledger_svc=ledger:Ledger.post
entoli recover prog.ent --dir D --grant Ledger --bind ledger_svc=ledger:Ledger.post
```

For crash recovery to re-execute pending operations, define
`def wrap : Cap[E1] -[]-> ... ((Unit -[E1,..]-> Data) -[]-> Data)` — your
handler prelude as a function over a thunk; `recover` re-installs it around the
generated recovery driver. Have `main` route through `wrap` so the run and
recovery paths install the same handlers.
