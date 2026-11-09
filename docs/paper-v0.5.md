**A LANGUAGE FOR DURABLE, CAPABILITY-SAFE AGENTS**

**ENTOLI**

Durable execution as a typing problem: affine effect handlers,
non-persistent authority, and recovery as an ordinary program

Kyriakos \[surname\] · PillarDelta · Working paper, v0.5 · August 2026

> **ABSTRACT**
>
> Systems that let language-model agents act on the world need three
> guarantees at once: that a computation survives process death, that a
> tool call is not silently repeated or silently lost, and that an agent
> cannot exercise authority it was never granted. Existing work supplies
> the first two through record-replay frameworks and the third through
> capability-tracking type systems. We are not aware of prior work
> addressing their interaction, and the interaction is where both
> guarantees quietly fail: a checkpoint captures authority, and a
> revocation cannot reach a sleeping process.
>
> We present ENTOLI, a core design in which durability, tool invocation,
> and capability control are consequences of one mechanism --- typed
> algebraic effect handlers with affine continuations. Three results
> follow. Restricting continuations to one shot makes durable suspension
> representable as a uniquely-owned stack and removes the reconciliation
> problem that multi-shot resumption poses for a persisted journal. An
> effect-site protocol with exactly two atomic transitions partitions
> every crash into three cases, of which one is ambiguous; because no
> operation may be declared without a delivery class, that case
> dispatches to exactly one recovery action and the undecidable fourth
> case is statically unreachable. And forbidding capabilities from
> crossing a durable boundary yields authority at rest: a persisted
> continuation confers no authority, so revocation needs no protocol and
> authority cannot outlive its justification.
>
> We state both results narrowly. Recovery determinacy establishes
> unique *selection* of a recovery action, not end-to-end exactly-once
> correctness, which rests on delivery contracts discharged outside the
> calculus (§8.3). Authority at rest establishes the absence of
> authority in a persisted store, not the absence of secrets (§9.2).
> This revision corrects the durable machine in three places found by
> review: the continuation is committed before external contact rather
> than after, which also removes a hidden replay of inter-effect
> segments (§7.1); recovery actions are ordinary effects run through the
> ordinary protocol at a depth bounded by typing, so a crash during
> recovery falls under the same theorem (§7.4, §8.2); and captured
> continuations carry a latent effect row, without which the
> nested-suspension restriction was vacuous (§4.2). §14 states plainly
> which claims are theorems and which are design arguments awaiting a
> calculus.

**0**

**Changes in this revision**

Review of v0.4 accepted the overall design and raised three technical
blockers in the state machine and type system, plus a correction to Rule
2 and two overreaching literature claims. All six are adopted. Two of
the three blockers turned out to have consequences beyond the repair,
noted below.

  -----------------------------------------------------------------------
  **RAISED**                **DISPOSITION**
  ------------------------- ---------------------------------------------
  **Blocker 1.** The        **Adopted, and it was worse than stated.** K
  protocol loses the        is now committed at Intend, before external
  continuation: K is not    contact (§7.1). The old machine was not
  durable until step 4, so  unrecoverable --- it could re-derive K by
  a crash in the window     re-executing the pure segment since the last
  leaves recovery with an   commit --- but that is replay, which this
  operation and no          design claims to have removed. The repair
  continuation to receive   makes the no-replay claim true for the first
  its result.               time. Step 4 shrinks to a single journal
                            write.

  **Blocker 2.**            **Adopted, resolved without a second
  compensable(op') is not   machine.** Recovery actions run through the
  itself crash-safe;        ordinary five-step protocol, so they are
  recovery actions are      covered by the same invariant and the same
  effects too, and a crash  theorem (§7.4). Unbounded recursion is
  mid-compensation repeats  prevented by a typing side condition: a
  the compensation.         compensation may not itself be compensable
                            (§8.2). The state sequence the review asked
                            for is then derived rather than stipulated.

  **Blocker 3.** k carries  **Adopted, with a narrower repair.**
  no latent effect row, so  Continuations carry a latent row and T-Resume
  the nested-suspension     contributes it (§4.2). Adding the row alone
  side condition is vacuous over-restricts --- it would forbid any
  when the clause body      handler of a suspending effect from resuming
  resumes.                  at all --- so the side condition is scoped by
                            a tail-position requirement on resume (§4.3).

  Rule 2 conflates total    **Adopted; the previous rule demanded
  encoding with infallible  something physically impossible.** Split into
  storage                   a total, pure encode plus a failure-atomic
                            commit that may decline to transition (§7.3).

  Universal literature      **Softened** in the abstract and §2.
  claims are indefensible   

  Interpreter before        **Endorsed by review; unchanged** (§15), with
  metatheory                the three blockers now fixed first as review
                            advised.
  -----------------------------------------------------------------------

**1**

**Introduction**

An agent program is an unusual object. It runs for days rather than
milliseconds. It calls tools that cost money, mutate external systems,
and fail. It processes untrusted text that may contain instructions
written by an adversary. And it must survive the machine it started on.

Each of these has an established answer in isolation. Long-running
computation that survives process death is handled by durable execution
frameworks, whose semantics were given a rigorous account by Burckhardt
et al. for Azure Durable Functions \[5\]. Capability-based confinement
of agent tool use has been addressed dynamically by CaMeL, which
interposes a custom interpreter and attaches capability metadata to
values \[6\], and statically by Odersky et al., who place agents inside
a Scala 3 safety harness using capture checking \[20\]. Both lines are
strong, and this work takes from both.

**What is missing is the interaction.** The durability literature does
not ask what happens to authority when a computation sleeps;
capabilities are ordinary values in an ordinary heap, and a checkpoint
captures them along with everything else. The capability literature does
not ask what happens to a tracked capability when the process holding it
dies and is reconstructed four days later from storage. Neither question
is idle. A checkpoint that captures a capability is a credential written
to disk with an unbounded lifetime, and a revocation that cannot reach a
sleeping continuation is not a revocation.

We argue that both problems dissolve if durability, tool access, and
authority are consequences of one mechanism rather than three
subsystems. That mechanism is typed algebraic effects with handlers
\[21, 2, 16\], read in the capability-passing style of Brachthäuser et
al. \[3\], where an effect type expresses what a computation requires
from its context rather than what it does to the world.

**1.1 Contributions**

- **A core design** in which an effect row is part of every function
  type, a handler is the only means of discharging an effect, and
  installing a handler requires holding a capability for it (§4).

- **An affinity restriction on continuations** with two independent
  justifications: it makes the persisted stack uniquely owned and
  serialisable in place \[4, 24\], and it removes the reconciliation
  problem that multi-shot resumption poses for a durable journal (§5).

- **A durable machine, a store invariant, and a commit protocol** with
  exactly two atomic transitions per effect site, from which
  recovery-action selection is total and deterministic (§7, §8).

- **Authority at rest:** capabilities are non-persistable, so revocation
  needs no protocol and sleeping agents cannot carry stale authority
  across a policy change (§9).

- **An explicit accounting** of which claims are established, which are
  conditional on contracts discharged elsewhere, and which remain design
  arguments awaiting a calculus (§14).

**2**

**Thesis**

**Durable execution is a determinism problem, and determinism is a
typing problem.** Widely used replay-based durable-execution systems
require that code inside a durable region be replayable, and none of the
ones we know of can enforce it, because the host language will not
discuss the property. Nothing in Python or TypeScript prevents a
workflow body from reading the clock, and nothing prevents an imported
library from opening a socket. Frameworks respond with documentation,
linters, and runtime interposition on the standard library, a contest
they can win partially and never completely.

Burckhardt et al. make this precise: their replay-based model exists
specifically so that orchestrations can be written in languages whose
runtimes do not support checkpointing \[5\]. Replay is a workaround for
a missing language feature. Supply the feature and the workaround goes
away with its failure modes.

**2.1 The stronger claim**

The property a durable system needs is not only that pure code be
replayable, but that every external effect declare how it recovers. That
declaration is a type-level obligation, and once it exists,
recovery-action selection becomes total.

**2.2 Why this might justify a language**

An earlier draft asserted that no library could obtain this result. That
is too categorical, and we withdraw it. A sufficiently restrictive typed
DSL, a compiler plugin, a generated API, or an effect-aware host
language can all reject programs. The defensible claim is narrower and,
we think, more interesting:

> ENTOLI makes recoverability of external effects part of the *static
> semantics*, so that recovery obligations compose through ordinary
> program typing rather than living as framework conventions.

Composition is the operative word. A framework can demand an idempotency
key at one call site. What it cannot do is propagate that obligation
through an arbitrary call graph, so that a function which transitively
performs an unrecovered effect cannot claim to be recoverable. That is
what an effect row does for free, and it is the reason the mechanism has
to sit in the type system rather than beside it.

> **THE FALSIFICATION CRITERION**
>
> ENTOLI earns its existence at three points, and we regard the design
> as refuted if any fails. A program performing an unrecoverable
> external effect must fail to compile. A continuation capturing an
> unserialisable resource must fail to compile. A sub-agent reaching a
> tool it was never handed must fail to compile. If all three are
> obtainable inside an existing capability-tracking language, ENTOLI
> should be a durability extension rather than a language.

**3**

**Related work**

**3.1 Effects, handlers, and capabilities**

Algebraic effects and handlers \[21\] give a modular account of
computational effects and provide, at each operation site, the delimited
continuation of the surrounding computation. Row-typed treatments \[16\]
make the effect set part of the type. Brachthäuser et al. reinterpret
the row as the set of capabilities a computation requires from its
context rather than the side effects it performs \[3\]; ENTOLI adopts
this reading, since it is what allows one mechanism to serve both the
durability and the authority requirements.

Object-capability security \[8, 19\] and the principle of least
privilege \[23\] supply the discipline: authority travels only by
reference and no reference can be fabricated. Odersky et al. apply this
to agents using Scala 3 capture checking, obtaining local purity and
demonstrating that agents generate capability-safe code without
significant loss of task performance \[20\]. CaMeL achieves a comparable
outcome dynamically, separating control flow from data flow and
enforcing policy at tool-call time \[6\], evaluated on AgentDojo \[7\].

**Relation to this work.** ENTOLI's capability model is not novel
against \[20\], which we regard as having settled the static-capability
question for agents. Neither \[20\] nor \[6\] considers durable
suspension; both assume the agent process is alive throughout. §9 is
what the capability model becomes when the process is allowed to die.

**3.2 Durable execution and one-shot control**

Burckhardt et al. give the semantics of Durable Functions across three
progressively refined models and prove them equivalent, including a
replay-based model that persists and restores intermediate workflow
state \[5\]. Compensating actions for long-lived transactions descend
from Sagas \[9\], and the discipline of idempotency keys for
at-least-once delivery is argued in \[14\]. The impossibility bounding
all of it is the coordinated attack problem \[11, 13\]: no protocol
makes a remote commit and a local record simultaneously observable.

Suspending a computation and resuming it later by capturing its
continuation is established in the web-server literature \[22\], and
orthogonal persistence explored values outliving their creating process
\[1\]. On representation, Bruggeman et al. show a continuation known to
be invoked once can be implemented on a stack without the copying
inherent to multi-shot capture \[4\], and OCaml restricts effect-handler
resumption to one shot for the same reason \[24\]. §5 shows this
restriction has a second, semantic justification once continuations are
persisted.

**3.3 Resources, cost, and injection**

Effect systems for resource accounting descend from \[17\] and static
cost bounds from amortized resource analysis \[15\]; §11 uses these only
to state what is and is not provable. Indirect prompt injection was
characterised in \[12\]; our claim against it is narrow and marked as
such in §13.2.

**4**

**The core calculus**

> **NOTATION**
>
> Rendered in ASCII. \|- is the typing turnstile, \|-\> maps-to, -o the
> affine arrow, \* tensor, =\> and \~\> separate handler input from
> output, U row union, {} the empty row, \< \> machine configurations, @
> a deadline obligation.

**4.1 Syntax**

> Values v ::= x \| () \| λx:τ. e \| { h } \| κ // κ ranges over
> capabilities
>
> Expressions e ::= v \| e e \| let x = e in e
>
> \| do E.op(v) // perform an operation
>
> \| handle e with { h } // install a handler
>
> \| resume k v // invoke a continuation (affine)
>
> Handler h ::= return x -\> e \| E.op(x; k) -\> e , h
>
> Types τ ::= Unit \| τ -\[ε\]-\> τ \| Cap\[E, π\] \| Res\[τ, φ\]
>
> Effect rows ε ::= {} \| {E} \| ε U ε \| α // α is a row variable
>
> Effect decl E ::= effect E \@s { op : τ -\> τ \@δ , \... }
>
> Suspension s ::= suspending \| prompt // NEW in v0.4, see §4.3
>
> Delivery δ ::= idempotent \| compensable(op') \| at_least_once(Idem)
>
> Policy π ::= constraint set on a capability (hosts, lane, budget)

**4.2 Typing**

The judgment is Γ \|- e : τ / ε. The context splits as Γ = Γu ; Γa,
unrestricted and affine, following the standard linear treatment \[10,
25\]; affine bindings are used at most once.

> **T-OP**
>
> E.op : τ1 -\> τ2 \@δ in E Γ \|- v : τ1 / {}
>
> Γ \|- do E.op(v) : τ2 / {E} U deliv(δ)

Here deliv(at_least_once) = {AtLeastOnce} unless an idempotency witness
is in scope, and {} otherwise. Weak delivery is therefore visible in the
type and propagates to every caller until deliberately handled. This is
the composition property of §2.2 in its concrete form.

> **T-HANDLE**
>
> Γ \|- e : τ / ε U {E} Γ \|- h : E =\> τ \~\> τ' / ε Γ \|- κ : Cap\[E,
> π\]
>
> Γ \|- handle e with { h } using κ : τ' / ε

E is discharged from the row, so a function that handles everything it
delegates has an empty row and is pure from outside --- the local-purity
property of \[20\], obtained here through handlers rather than capture
sets. The third premise is what makes authority unforgeable: writing a
handler does not entitle one to install it.

**Continuations carry a latent effect row.** v0.4 typed the captured
continuation as k : τ2 -o τ', with no record of what resuming it would
perform. Review observed that this makes the restriction of §4.3
vacuous: a clause body containing no syntactically suspending operation
can still cause a suspension by resuming. The continuation type
therefore carries its row, and T-Resume contributes it.

> **T-RESUME**
>
> Γ \|- k : τ2 -\[εk\]-o τ' Γ \|- v : τ2 / {}
>
> Γ \|- resume k v : τ' / εk

At an operation site inside handle e with h, the captured continuation
runs the remainder of e followed by the return clause, so εk = ε U {E}
in general. The row is therefore not free information: it is exactly
what §4.3 needs and what v0.4 discarded.

**4.3 Nested suspension is forbidden**

The store invariant of §7.2 bounds how many operations may be pending. A
handler that suspends while servicing an operation breaks that bound,
and v0.3 left the case open. v1 forbids it, statically --- but the
obvious formulation is too strong, and the correct one needs the latent
row above.

Effect declarations carry a suspension class s. An effect is suspending
if performing it may yield to the durable machine and prompt otherwise;
write Susp for the suspending effects.

**Why the naive rule fails.** With T-Resume in place, requiring ε ∩ Susp
= {} over the whole clause body would forbid any handler of a suspending
effect from ever resuming its continuation, since εk contains E itself.
That is not a restriction, it is a prohibition on the feature.

**The distinction that matters** is between the handler doing its own
work and the handler transferring control back. A suspension raised by
the handler while it is servicing the operation adds a second pending
entry; a suspension raised by the resumed computation does not, because
by then the handler has finished. So the restriction is scoped by
position: resume must appear in tail position, and the side condition
applies to the clause body's own row with the tail contribution
excluded.

> **T-CLAUSE**
>
> Γu ; Γa , x : τ1 , k : τ2 -\[εk\]-o τ' \|- e : τ' / ε ε \\ εk ∩ Susp =
> {} tail(resume k, e)
>
> Γu ; Γa \|- ( E.op(x; k) -\> e ) : E =\> τ \~\> τ' / ε

Tail-resumptive handlers are in any case the efficient and common shape
in effect-typed languages \[16, 3\], so the condition costs less than it
appears. The genuine cost remains: a handler cannot await a human
decision while servicing an inference call. The workaround keeps
suspension at one level --- the handler returns a value that causes its
caller to suspend.

**5**

**Affine continuations**

k : τ2 -\[εk\]-o τ' is affine: resumable at most once, droppable (that
is abort), never resumable twice. Multi-shot resumption is removed
permanently. Two independent arguments support this and we consider the
second the stronger.

**5.1 Representation**

A one-shot continuation is a uniquely-owned linked stack that can be
walked and written in place, avoiding the copying inherent to multi-shot
capture \[4\]; OCaml adopts the same restriction for the same reason
\[24\]. Since ENTOLI must not merely capture continuations but serialise
them, the argument is stronger here than in either. General continuation
serialisation is the least credible premise in this design, and
uniqueness of ownership is the largest available move against it.

**5.2 Reconciliation**

A persisted multi-shot continuation resumed twice raises a question with
no defensible answer. Each resumption performs effects already delivered
to the world, so the journal would have to fork into a tree of realised,
non-hypothetical futures. There is no reconciliation rule both sound and
implementable. Affinity does not solve this; it removes the states in
which it can be posed.

The cost is that backtracking, generators, and nondeterministic search
must be written as an explicit trampoline over reified state, in a
library, above the effect system. Acceptable for an orchestration
language, unacceptable for a general-purpose one.

**6**

**Persistability**

**6.1 Three predicates, not one**

Whether a value may cross a durable boundary is three questions.
Conflating them is what makes captured foreign resources appear
intractable.

  -------------------------------------------------------------------------
  **PREDICATE**   **QUESTION**                          **KIND**
  --------------- ------------------------------------- -------------------
  Repr(τ)         Does τ have a byte encoding stable    Type property,
                  under its own code identity?          decidable

  Reviv(τ)        Can a value of τ be reconstituted on  Type property,
                  the far side of a suspension?         decidable

  Perm(τ, π)      Does policy admit τ crossing a        Policy judgment,
                  durable boundary at all?              per-handler
  -------------------------------------------------------------------------

Persistable(τ) = Repr(τ) & Reviv(τ) is a type property; Perm is separate
because it varies by deployment. A password is representable and
revivable and may still be barred. That separation is load-bearing in
§9.2.

**6.2 Re-acquisition witnesses**

A socket fails Reviv; its address does not. The resource is therefore
expressed as a description plus a re-acquisition function:

> Res\[Socket, φ\] where φ : Addr -\> Socket / {Net}

The previous draft claimed \"only the address is written\" and left φ
unexamined. But φ is executable behaviour, and if it may capture freely
then the hard part has simply moved inside it. Four conditions remove
the sleight of hand:

- **Closed.** φ must be a top-level definition with no free variables.
  Nothing is captured, so nothing is smuggled.

- **Referenced, not serialised.** φ is written into the store as its
  content hash (§10.1), never as bytes of code.

- **Handled at resume.** The row of φ is discharged by the resume-time
  handler, not the suspend-time one. Re-acquisition therefore runs under
  current authority, which is what makes §6 compose with §9.

- **Prompt.** The row of φ must satisfy row(φ) ∩ Susp = {}. A
  re-acquisition that could itself suspend would create a pending entry
  during resume, breaking the invariant of §7.2.

**7**

**The durable machine**

**7.1 The effect-site protocol**

Configurations are \< e \| K \| σ \>: expression, continuation stack,
durable store. A crash may occur between any two transitions; recovery
resumes from σ alone. Transitions are marked **D** where they commit
atomically and **V** where they do not.

**Correction from v0.4.** The previous protocol wrote only Pending(#op,
v, ι) at step 1 and deferred persist(K) to step 4. Review pointed out
that recovery from a crash in the window then holds an operation and no
continuation to receive its result. The objection is right, and tracing
it exposes something worse: the old machine was not stuck, because it
could re-derive the continuation by re-executing the pure segment since
the last commit. That is replay \\u2014 the mechanism this design claims
to have eliminated \\u2014 quietly reintroduced between effect sites.
Committing the continuation at Intend removes both problems, and the
no-replay claim becomes true for the first time.

  --------------------------------------------------------------------------
  **STEP**     **TRANSITION**                                     **KIND**
  ------------ -------------------------------------------------- ----------
  1 · Intend   σ' = σ\[ j \|-\> Pending(#op, v, ι, κid), κid      **D**
               \|-\> encode(K) \] --- intent and continuation,    
               one transaction, before any external contact.      

  2 · Invoke   **Handler contacts the external world. This is the V
               crash window.**                                    

  3 · Receive  Result r arrives in volatile memory.               V

  4 · Commit   σ'' = σ'\[ j \|-\> Done(r) \] --- a single journal **D**
               write. The continuation is already durable.        

  5 · Resume   \< resume k r \| K \| σ'' \>. Affinity guarantees  V
               this occurs at most once per run.                  
  --------------------------------------------------------------------------

The continuation at Intend and the continuation at Commit are the same
object \\u2014 the one awaiting r \\u2014 so nothing is lost by moving
the write earlier, and the transaction that must be fast at the critical
moment is now the small one. Step 1 also retains its original purpose:
without a durable record written before step 2, a crash inside the
window leaves no evidence anything was attempted, and recovery cannot
distinguish \"never started\" from \"may have completed\" \[11, 13\]. We
do not claim to solve the coordinated attack problem. We claim to make
its one ambiguous case dispatchable.

**7.2 The store invariant**

> **INVARIANT · INV(σ)**
>
> I1 **Bounded pending.** The pending entries of σ form a stack of depth
> at most two: an operation, and at most one recovery action for it
> (§8.2).
>
> I2 **Continuation precedence.** Every pending entry names a
> continuation identifier already resolvable in σ. No entry reaches
> Pending without its continuation being durable first.
>
> I3 **Representability.** Every continuation referenced from σ
> satisfies Repr and Reviv, and every value within it satisfies Perm,
> all established statically before the store is written.

I1 replaces v0.4's unique-pending clause, which review's compensation
trace showed to be false: recovery performs effects, so a compensation
in flight is a second pending entry. Depth two rather than unbounded is
a consequence of the typing condition in §8.2, not an assumption. I1
becomes a per-branch property when concurrency arrives (§12). I2 is what
the corrected step 1 buys and is strictly stronger than v0.4's
commit-coupling clause. I3 is what §6 buys, as a precondition rather
than a runtime check.

**7.3 Total encoding, failure-atomic commit**

**Correction from v0.4.** Rule 2 previously required that persistence be
infallible. Review is right that this conflates two requirements and
demands one that is physically unavailable: a disk fills, an fsync
fails, a device dies, and no type system prevents it. The requirement
splits.

> **RULE 2 · TOTAL ENCODING, FAILURE-ATOMIC COMMIT**
>
> encode(K) is pure and total for well-typed K. The durable commit is
> atomic and may fail, in which case it is a no-transition: σ is left
> exactly as it was.

Totality of encode is a type-system obligation and achievable, since I3
makes every frame layout known at compile time. Failure of the commit is
then harmless rather than catastrophic, because a declined transition at
step 1 means the effect was never attempted and a declined transition at
step 4 leaves a recoverable Pending. This is the ordinary atomic
state-machine reading, and it is stronger than the rule it replaces
precisely because it demands less.

The implementation constraint survives the correction in weakened form:
because encode must be total and cheap at the moment of suspension, the
continuation is best maintained in representable layout as frames are
pushed rather than converted at the suspension point. That is affordable
only because I3 is static and continuations are affine and hence
uniquely owned (§5.1). We expect this to be the first thing
demonstration A tests.

**7.4 Recovery is an ENTOLI program**

Review's second blocker is the deepest point raised against this design.
Recovery actions perform external effects, so they can crash; if
recovery runs on separate machinery, it needs its own crash analysis,
its own invariant, and Theorem 1 does not reach it. The trace is
concrete: op succeeds, the machine crashes, recovery invokes the
compensation, the compensation succeeds, the machine crashes again
before retrying, and the store still reads Pending --- so the
compensation runs a second time.

One repair is to add durable recovery states. We reject it, because it
answers the symptom and leaves the recursion: whatever machine executes
those states can itself crash. The uniform answer is that there is no
second machine.

> **RULE 3 · RECOVERY IS ORDINARY**
>
> A recovery action is an effect, performed through the five-step
> protocol of §7.1, with its own journal entry, its own delivery class,
> and its own durable continuation. The recovery driver is an ENTOLI
> program, not runtime machinery.

A crash during compensation is therefore an ordinary crash at an
ordinary effect site, handled by the ordinary three-way split, and
covered by the same theorem. The state sequence review asked for \\u2014
Pending \\u2192 Compensating \\u2192 Retrying \\u2192 Done \\u2014 is
not stipulated but read off the pending stack:

> Pending(op) dispatch on δ(op)
>
> Pending(op), Pending(op') recover op' by δ(op'), then Done(op'), then
> retry op
>
> Pending(op), Done(op') compensation complete; retry op, then Done(op)

This also satisfies, in the strongest available form, the binding
condition on demonstration A in §13: no recovery machinery appears in
the interpreter that is not represented in the calculus, because the
recovery driver is written in the language.

**8**

**Recovery**

**8.1 Determinacy**

Recovery reads σ, takes the top of the pending stack (I1), and acts.
Every case names a continuation that is already durable (I2).

  ------------------------------------------------------------------------
  **CRASH POINT** **STORE      **RECOVERY**
                  STATE**      
  --------------- ------------ -------------------------------------------
  Before 1        no entry     Resume from the last committed
                               continuation. The effect never reached the
                               world.

  Between 1 and 4 Pending      **Dispatch on δ,** with K recovered from
                               the identifier in the entry. The only
                               ambiguous case.

  After 4         Done(r)      Resume the named continuation with r. The
                               effect is never repeated.
  ------------------------------------------------------------------------

  -----------------------------------------------------------------------
  **DELIVERY CLASS    **ACTION ON PENDING**
  Δ**                 
  ------------------- ---------------------------------------------------
  idempotent          Re-invoke.

  at_least_once(ι)    Re-invoke with the same witness ι, written at step
                      1; the remote system deduplicates \[14\].

  compensable(op')    Perform op' as an effect (Rule 3), then re-invoke.
                      The Saga discipline \[9\], made crash-safe by
                      running the compensation through the same protocol.

  --- none ---        **Unreachable.** T-Op rejects the program.
  -----------------------------------------------------------------------

> **THEOREM 1 · RECOVERY-ACTION DETERMINACY**
>
> For every reachable crash point, including crashes during recovery,
> there exists exactly one recovery *action*, computable from σ and the
> delivery class of the operation named at the top of the pending stack.
> No recovery path consults runtime state beyond σ, and no crash point
> admits two candidate actions.
>
> *Sketch. The three-way crash split is exhaustive by construction,
> since steps 1 and 4 are the only D-transitions. The top of the pending
> stack is unique and its continuation resolvable by I1 and I2. In the
> middle case δ is total over declared operations, and the fourth row is
> empty because T-Op admits no operation without a delivery class.
> Crashes during recovery reduce to the same three cases by Rule 3, and
> the induction terminates because the stack depth is bounded by §8.2.*

**8.2 Compensations may not be compensable**

Rule 3 makes recovery ordinary, which raises the question of whether it
terminates. If op' were itself declared compensable(op''), a crash
during compensation would push a third entry, and so on without bound.
The restriction that prevents this is a side condition at effect
declaration:

> compensable(op') requires δ(op') in { idempotent, at_least_once }

A compensation must therefore be recoverable by re-invocation alone,
which bounds the pending stack at depth two and gives I1 its constant.
This is a real constraint on how compensations are written, and we think
a healthy one: a compensating action that cannot safely be retried is a
poor compensating action.

**8.3 What Theorem 1 does not establish**

The theorem selects an action. It does not establish that the action is
correct, and the distinction matters enough to state at length.

Declaring an operation idempotent is an assertion about a foreign
system, not a fact the type system derives. Declaring compensable(op')
fixes what ENTOLI will do; nothing in the calculus establishes that op ;
op' ; op is observationally equivalent to one successful op. End-to-end
exactly-once behaviour is therefore *conditional* on the declared
contracts being true, and those contracts are discharged outside the
calculus.

This is the ordinary shape of a foreign-function boundary and not a
defect peculiar to ENTOLI. What the design buys is that the obligation
is named, located at a specific declaration, and non-optional, where
today it is an undocumented assumption diffused across a codebase. The
obligations are small enough to state and to test:

> idempotent requires op ; op ≈ op
>
> at_least_once(ι) requires op(ι) ; op(ι) ≈ op(ι)
>
> compensable(op') requires op ; op' ; op ≈ op

We propose these be discharged by a conformance harness rather than by
assertion: each declared class generates a test exercising the stated
equation against a sandbox, and a handler failing its equation is
rejected at deployment. That converts a claim into an obligation with a
check attached, which is roughly where unsafe blocks actually live in
practice --- unverified, but visible, localised, and reviewable.

**9**

**Authority at rest**

**9.1 The rule and the theorem**

What happens to a capability held in a continuation that sleeps for four
days? Neither literature poses the question, and it has an uncomfortable
default answer: the capability is checkpointed with everything else,
writing a credential to disk with unbounded lifetime and no revocation
path.

> **RULE 4 · NO PERSISTED AUTHORITY**
>
> Perm(Cap\[E, π\], \_) is false for all E and π. Capabilities never
> cross a durable boundary.

At suspension the continuation splits as K = Kframes \* Kcaps and only
Kframes is written. Capabilities are re-granted by the handler at
resume, which is where they came from originally, so the programming
model is unchanged.

> **THEOREM 2 · AUTHORITY AT REST**
>
> A persisted continuation confers no authority. A resumed computation
> holds precisely the authority granted by the handler in force at
> resume time, never the authority in force at suspend time.

Revocation therefore requires no protocol: a revoked grant cannot be
found at resume and every dependent sleeping agent halts. And authority
cannot outlive its justification, which is the correct behaviour for an
agent asleep across an incident, a policy change, or a departure.
Neither property is obtainable by adding durability to \[20\], since
capture checking has no notion of a boundary across which a capture set
must be discharged, nor by adding capabilities to \[5\], since replay
reconstructs the capability with the rest of the state.

**9.2 Correction: absence of authority is not absence of secrets**

A previous draft concluded from Theorem 2 that \"continuation stores are
not secret material and may be handled under ordinary data handling.\"
That is wrong, and it contradicted our own §6: Perm exists precisely
because a value can be representable and revivable and still be
policy-barred, which presupposes that some persisted values are
sensitive.

A persisted frame may legitimately hold a customer record, a prompt, a
retrieved document, a model output, or any other confidential value that
policy permits to cross. Theorem 2 says nothing about these. It says
only that no capability is among them.

The correct treatment computes rather than assumes. If each type carries
a classification under policy, the classification of a store is the join
of what it holds:

> level(K) = ⊔ { level(v) \| v in frames(K) }

So a store's handling requirement is derived from its contents rather
than asserted about the store, and a continuation containing only public
data genuinely is ordinary while one containing a customer record is
not. This is strictly more useful than the claim it replaces, and it is
available only because Perm was already separated out in §6.

**10**

**Code identity and resume compatibility**

**10.1 Content addressing**

Definitions are named by a Merkle hash \[18\] over the typed AST, closed
over the identity of everything referenced, so code cannot change
beneath a running computation. This removes implicit code-version drift
and nothing else: schemas, ABIs, service APIs, and store formats version
as normal. Extending the hash to effect signatures, delivery classes
included, makes promotion from idempotent to at-least-once a breaking
change by construction:

> sig(h) = hash( { op \|-\> (τ1, τ2, δ) \| op in E } )

**10.2 The decision on handler identity**

Signature equality is not semantic equality, and the previous draft left
this open while calling it possibly-acceptable information hiding. It is
not acceptable. A four-day-old continuation may resume against a handler
with an identical signature whose implementation now routes to a
different merchant, reads an amount differently, or has changed what
constitutes the idempotency domain. Signature equality approves the
resume; semantic compatibility has failed silently. Three options were
available.

  -----------------------------------------------------------------------
  **OPTION**              **VERDICT**
  ----------------------- -----------------------------------------------
  Pin the implementation  **Rejected.** Any bugfix to a handler strands
  hash                    every sleeping continuation depending on it.
                          Handlers are the component most in need of
                          redeployment, being the interface to a changing
                          outside world. This makes the system
                          unmaintainable.

  Treat handler semantics **Rejected.** This is the current behaviour and
  as                      the merchant case shows it unsafe.
  deployment-controlled   
  abstraction             

  Declared compatibility  **Adopted.** See below.
  identity                
  -----------------------------------------------------------------------

What must be stable across a suspension is neither the implementation
nor the signature but the *observable contract the continuation depends
on* --- a third thing, which the compiler cannot compute because the
knowledge lives with the deployer. So it is declared alongside the
computed hash, and both are required:

> persist(K) = \< frames, { (sig(E), compat(E)) \| E in handled(K) } \>
>
> // resume K under h requires, for every E in handled(K):
>
> // sig(h.E) = sig(captured.E) // computed, catches structural drift
>
> // compat(h.E) = compat(captured.E) // declared, catches semantic
> drift
>
> migrate E from compat \"2026-06\" to \"2026-08\" { \... } // optional
> adapter

Bumping the token is a deliberate act that strands continuations
captured under the old contract, and stranding becomes a migration event
with a hook rather than a silent misinterpretation. Where no adapter is
supplied, resume fails cleanly and the continuation is quarantined
rather than lost. This is schema versioning applied to effect semantics,
and it puts the obligation where the knowledge is.

**11**

**Cost, jurisdiction, and deadlines**

Inference is not an ordinary function call: it is nondeterministic,
priced, latency-bearing, fallible, and in a European deployment
jurisdictionally constrained. Putting all five in one syntactic slot is
good ergonomics and bad semantics, since they differ in provability.

  ------------------------------------------------------------------------
  **TIER**        **EXAMPLE**       **STATUS**
  --------------- ----------------- --------------------------------------
  Intrinsic       Infer             Type-level; checked by T-Op.
  effect                            

  Static policy   sovereign,        Provable given bounded loops and
                  \<=0.002eur       retries, by amortized resource
                                    analysis \[15\]. Jurisdiction is
                                    provable only relative to a runtime
                                    that guarantees the lane; the type
                                    records the requirement, not its
                                    discharge.

  Runtime         \@200ms           Not provable. Reified as an
  contract                          obligation.
  ------------------------------------------------------------------------

> complete : Prompt -\[ Infer\[sovereign, \<=0.002eur, \@200ms\] \]-\>
> Result\[Completion, Timeout\]

The deadline introduces a failure mode into the result type, so a
program ignoring expiry does not typecheck. The claim moves from
provable performance, which is false, to provable totality with respect
to timeout, which is true and is what the caller needs.

**12**

**Concurrency-compatible invariants**

The previous title promised full semantics and delivered commitments.
There is no fork transition, no join rule, no race treatment, no
concurrent recovery result, and no scheduling model here. What follows
are three constraints taken now because each changes the shape of the
core rather than adding to it.

1.  **Continuation identity is a path, not a counter.** A fork creates a
    tree; hierarchical identifiers are free today and unmigratable once
    a store contains linear ones.

2.  **The journal is a partial order.** Per-branch sequences with
    explicit join entries. A totally ordered journal cannot represent
    concurrent effects, and I1 becomes a per-branch property under this
    reading rather than a global one.

3.  **Quantitative capabilities are affine and split at fork.** Ten
    branches under one budget must divide it, not each inherit it. If
    capabilities are unrestricted values in v1, every bound in §11
    becomes false the day concurrency arrives.

Cancellation then needs no separate mechanism: it is revocation at a
subtree root, and Rule 4 already makes revocation halt dependents at
resume.

**13**

**Evaluation plan**

Three demonstrations on a tree-walking interpreter, before any compiler
work, each corresponding to one clause of the falsification criterion in
§2. The binding condition throughout: no recovery machinery may appear
in the interpreter that is not represented in the calculus. If
demonstration A passes only by adding ad-hoc reconciliation, it has
failed.

**13.1 A · Kill and resume inside the window**

> fn settle(inv) -\> Outcome / {Ledger, Approval} {
>
> let f = audit(inv)
>
> match do Approval.request(f.summary) { // suspension, days
>
> .approve =\> do Ledger.post(f, ι = idem(inv)) // \@at_least_once
>
> .reject =\> .halted(f)
>
> }
>
> }

Four kills, four expected outcomes: before step 1, nothing posted and
resume posts once; inside the window with the remote committed, resume
re-invokes under the same ι and the remote deduplicates; inside the
window with the remote not committed, resume posts once; after step 4,
no re-invocation. Pass condition: exactly one ledger entry in all four
runs, each restarted from cold storage with no volatile state carried
across. Secondary condition, from §7.3: no serialisation work occurs at
the suspension point.

**13.2 B · Containment as a compile error**

> fn delegate(task, net: Cap\[Http, {docs.internal}\]) -\> Report / {} {
>
> handle research(task) with { Http -\> net }
>
> }
>
> // research/1 performs do Fs.read(\...), possibly at an injected
> instruction.

Fs remains in the row of research, so delegate cannot claim / {}, and no
Cap\[Fs, \_\] is in scope to handle it. The demonstrated claim is
deliberately narrow: escalation beyond the delegated envelope is
impossible. Misuse of docs.internal from within the envelope is neither
addressed nor claimed --- that is the province of information-flow
tracking as in \[6\].

**13.3 C · Authority at rest**

Suspend an agent holding a budgeted inference capability. Dump the
persisted continuation and confirm the absence of capability material,
budget, and credentials --- and, per §9.2, record what non-authority
data it does contain, since that is the store's actual classification.
Revoke the grant, attempt resume, observe a refusal naming the missing
capability. Restore with a smaller budget, resume, observe the agent
bound by the new figure.

**14**

**Status of claims**

Review observed that several items labelled theorem or rule are
convincing design arguments rather than consequences of a fully
specified calculus. That is correct. The table below states where each
item stands, so that nothing in this paper is read as more settled than
it is.

  -----------------------------------------------------------------------
  **CLAIM**                  **STATUS**
  -------------------------- --------------------------------------------
  Effect rows discharge on   Established, and inherited from known
  handling (T-Handle)        results on typed handlers \[21, 16, 3\].

  Affinity of continuations  Established as a typing restriction. Its
                             implementation payoff is supported by \[4,
                             24\] but unmeasured here.

  Theorem 1 ·                **Conditional.** Follows from I1--I3, the
  recovery-action            totality of δ, and the depth bound of §8.2.
  determinacy                Now covers crashes occurring during
                             recovery. I1--I3 are asserted, not proved,
                             pending a full transition relation.

  End-to-end exactly-once    **Not claimed.** Conditional on delivery
                             contracts discharged outside the calculus
                             (§8.3).

  Theorem 2 · authority at   **Conditional.** Follows immediately from
  rest                       Rule 4 given that only Kframes are encoded.
                             Requires the frame/capability split to be
                             formalised.

  Secrecy of persisted       **Withdrawn** (§9.2).
  stores                     

  Rule 2 · total encoding,   **Modelling choice.** Totality of encode
  atomic commit              follows from I3; failure-atomicity is a
                             requirement placed on the store, not a claim
                             made about it.

  Rule 3 · recovery is       **Design decision with a derived
  ordinary                   consequence.** The depth bound of §8.2 is a
                             typing condition. That the derived state
                             sequence is complete is argued, not proved.

  Resume compatibility       **Design decision, not a result.** Soundness
  (§10.2)                    depends on deployers bumping the token
                             honestly, which no mechanism enforces.

  Serialisable continuations **Conjecture.** The weakest premise in the
  are feasible               design.
  -----------------------------------------------------------------------

To deserve the word calculus in the usual sense, what remains is
unglamorous and specific: complete typing rules, evaluation contexts and
a full transition relation, substitution and weakening under the affine
context, handler reduction, persistence typing, and a preservation
result establishing that every reachable configuration satisfies INV(σ),
from which Theorem 1 would genuinely follow rather than being argued.

**15**

**On ordering**

An earlier review asked for the metatheory before implementation, and
also asked for demonstration A. We proposed the reverse emphasis; review
subsequently accepted it, on condition that the state-machine and typing
defects be repaired before any code is written. §4 and §7 are that
repair. The reasoning is restated so it can still be rejected.

The risky content of this design is operational rather than
type-theoretic. The effect calculus itself is close to known territory
\[21, 16, 3\] and its metatheory holds few surprises. What is genuinely
uncertain is whether a continuation can be maintained in
continuously-representable form, whether encode can be kept total and
cheap enough to run at every effect site rather than only at suspension,
and whether re-acquisition composes with resume-time authority. None of
these is discovered by proving preservation; all are discovered on the
first kill test.

§7.1 is the argument in miniature, and it cuts both ways. Committing the
continuation at step 4 rather than step 1 was a defect visible to a
careful reader tracing the state machine, and it was found by reading
rather than by running --- a point for review, not for implementation.
What it exposed underneath, that the old machine silently replayed pure
segments between effect sites, is exactly the class of thing a running
interpreter surfaces on the first kill test and a preservation proof
would have faithfully preserved.

So: the invariant is stated now, which is cheap and was owed. The
interpreter comes next, because it is a cheaper counterexample-finder
than a proof attempt for this class of problem. The full metatheory
comes third, with INV(σ) as the property preservation must preserve ---
written against a machine already known to exist.

**16**

**Limitations**

- **Continuation serialisation remains the weakest premise.** §5, §6 and
  §7.3 improve its odds and constrain its implementation; none
  establishes it.

- **Delivery contracts are unverified.** §8.3 proposes a conformance
  harness; a harness is testing, not proof, and a handler can pass its
  equation on a sandbox and fail in production.

- **Compatibility tokens depend on discipline.** Nothing forces a
  deployer to bump the token when semantics change. This relocates the
  problem to a place where it is visible, which is progress, not a
  solution.

- **Nested suspension is forbidden rather than solved.** The restriction
  is real and §4.3 states its cost.

- **Concurrency is absent, not deferred cleanly.** I1 is stated globally
  and will need restating per-branch; §12 lists what has been protected
  and implies what has not.

- **The capability half needs no new language.** It is achievable in
  Scala 3 today \[20\]. A reasonable reading of this paper is that
  ENTOLI should be a durability extension to a capability-tracking host
  rather than a language of its own. Whether Rule 4 can be expressed
  that way is unknown, and answering it is the shortest available path
  to refuting the whole project.

- **The intended author is undecided.** Whether ENTOLI is written by
  people or by models is upstream of every surface decision. Affinity
  discipline and mandatory delivery classes are tedious for a person and
  free for a checked generator, which is a quiet argument for the
  latter.

**17**

**Conclusion**

Durable execution and capability safety have been pursued independently
and successfully. Their interaction has not been examined, and it is
where both guarantees fail: a checkpoint captures authority, and a
revocation cannot reach a sleeping process. ENTOLI derives both from one
mechanism and obtains two properties as consequences --- unique
recovery-action selection, because the type system makes the undecidable
crash case unreachable rather than handling it, and authority at rest,
because a capability is forbidden from crossing the boundary durability
creates.

Both are stated conditionally, and §14 says on what. Both are refutable
by a small interpreter, and one of them, we suspect, will be. That
interpreter is the next artifact.

**References**

**\[1\]** Atkinson, M. P. and Morrison, R. Orthogonally Persistent
Object Systems. VLDB Journal 4(3), 1995, 319--401.

**\[2\]** Bauer, A. and Pretnar, M. Programming with Algebraic Effects
and Handlers. Journal of Logical and Algebraic Methods in Programming
84(1), 2015, 108--123.

**\[3\]** Brachthäuser, J. I., Schuster, P. and Ostermann, K. Effects as
Capabilities: Effect Handlers and Lightweight Effect Polymorphism.
PACMPL 4(OOPSLA), Article 126, 2020. DOI 10.1145/3428194.

**\[4\]** Bruggeman, C., Waddell, O. and Dybvig, R. K. Representing
Control in the Presence of One-Shot Continuations. PLDI '96, 99--107.
DOI 10.1145/231379.231395.

**\[5\]** Burckhardt, S., Gillum, C., Justo, D., Kallas, K., McMahon, C.
and Meiklejohn, C. S. Durable Functions: Semantics for Stateful
Serverless. PACMPL 5(OOPSLA), Article 133, 2021. DOI 10.1145/3485510.

**\[6\]** Debenedetti, E., Shumailov, I., Fan, T., Hayes, J., Carlini,
N., Fabian, D., Kern, C., Shi, C., Terzis, A. and Tramèr, F. Defeating
Prompt Injections by Design (CaMeL). arXiv:2503.18813, 2025.

**\[7\]** Debenedetti, E., Zhang, J., Balunović, M., Beurer-Kellner, L.,
Fischer, M. and Tramèr, F. AgentDojo: A Dynamic Environment to Evaluate
Prompt Injection Attacks and Defenses for LLM Agents. NeurIPS 2024.

**\[8\]** Dennis, J. B. and Van Horn, E. C. Programming Semantics for
Multiprogrammed Computations. CACM 9(3), 1966, 143--155.

**\[9\]** Garcia-Molina, H. and Salem, K. Sagas. SIGMOD '87, 249--259.

**\[10\]** Girard, J.-Y. Linear Logic. Theoretical Computer Science
50(1), 1987, 1--101.

**\[11\]** Gray, J. Notes on Data Base Operating Systems. In Operating
Systems: An Advanced Course, LNCS 60, Springer, 1978, 393--481.

**\[12\]** Greshake, K., Abdelnabi, S., Mishra, S., Endres, C., Holz, T.
and Fritz, M. Not What You've Signed Up For: Compromising Real-World
LLM-Integrated Applications with Indirect Prompt Injection. AISec '23,
79--90.

**\[13\]** Halpern, J. Y. and Moses, Y. Knowledge and Common Knowledge
in a Distributed Environment. JACM 37(3), 1990, 549--587.

**\[14\]** Helland, P. Life beyond Distributed Transactions: an
Apostate's Opinion. CIDR 2007, 132--141.

**\[15\]** Hoffmann, J., Aehlig, K. and Hofmann, M. Multivariate
Amortized Resource Analysis. ACM TOPLAS 34(3), Article 14, 2012.

**\[16\]** Leijen, D. Type Directed Compilation of Row-Typed Algebraic
Effects. POPL 2017, 486--499.

**\[17\]** Lucassen, J. M. and Gifford, D. K. Polymorphic Effect
Systems. POPL 1988, 47--57.

**\[18\]** Merkle, R. C. A Digital Signature Based on a Conventional
Encryption Function. CRYPTO '87, LNCS 293, Springer, 369--378.

**\[19\]** Miller, M. S. Robust Composition: Towards a Unified Approach
to Access Control and Concurrency Control. PhD thesis, Johns Hopkins
University, 2006.

**\[20\]** Odersky, M., Zhao, Y., Xu, Y., Bračevac, O. and Pham, C. N.
Securing Agents With Tracked Capabilities. CAIS '26, 812--838. DOI
10.1145/3786335.3813127.

**\[21\]** Plotkin, G. and Pretnar, M. Handlers of Algebraic Effects.
ESOP 2009, LNCS 5502, Springer, 80--94.

**\[22\]** Queinnec, C. The Influence of Browsers on Evaluators, or
Continuations to Program Web Servers. ICFP 2000, 23--33.

**\[23\]** Saltzer, J. H. and Schroeder, M. D. The Protection of
Information in Computer Systems. Proceedings of the IEEE 63(9), 1975,
1278--1308.

**\[24\]** Sivaramakrishnan, K. C., Dolan, S., White, L., Kelly, T.,
Jaffer, S. and Madhavapeddy, A. Retrofitting Effect Handlers onto OCaml.
PLDI 2021, 206--221. DOI 10.1145/3453483.3454039.

**\[25\]** Wadler, P. Linear Types Can Change the World! In Programming
Concepts and Methods, North Holland, 1990.
