**A LANGUAGE FOR DURABLE, CAPABILITY-SAFE AGENTS**

**ENTOLI**

Durable execution as a typing problem: affine effect handlers,
non-persistent authority, and recovery as an ordinary program

Kyriakos \[surname\] · PillarDelta · Working paper, v0.6 · August 2026

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
>
> This revision is the first written against execution rather than
> review of text: a reference interpreter has passed the two decisive
> tests set for it in advance --- cross-process cold resume of an
> encoded continuation, and recovery expressed as generated ENTOLI
> terms --- and survived an adversarial review, conducted with
> independently prompted model review agents commissioned by the
> authors (§0), whose one genuine blocker was a Theorem 1
> counterexample one crash deeper than the previous revision's recovery
> table.
> The repair generalises recovery's input from the pending stack to the
> journal frontier (§8.1) and binds the machine's own operations to the
> delivery equations of §8.3. The largest change makes the one-mechanism
> claim true rather than aspirational: the interpreter's first version
> discharged suspending effects through a host registry beside the
> handlers, and v0.6 removes the second mechanism --- a suspending effect
> is discharged by an ordinary installed handler whose clause the machine
> journals around, with foreign invocation as the only world contact
> (§4.3, §6.4, §7.1). Encoding is corrected to preserve continuation
> identity rather than count (§6.3), and Rule 2's totality is restated
> over reachable configurations, where it is now true (§7.3). §14 records
> that serialisable continuations have moved from conjecture to
> demonstrated for the interpreter's value domain.

**0**

**Changes in this revision**

v0.5 was revised from review of the text. v0.6 is revised from
execution: a reference interpreter, built to a pre-registered
development plan whose order put two kill tests first --- continuation
encoding across process death, then recovery as an ordinary program ---
passed both, ran the §13 demonstrations under hard process death, and
was then adversarially reviewed: thirty-four independently prompted
model review and verification agents, commissioned by the authors and
run against the v0.5 text and the plan, their confirmed findings
repaired in-tree the same day. The review confirmed nine findings
(R-1..R-9), including one blocker --- and the second kill test's
verdict stands as passed only *after* that blocker's repair, since the
pre-review crash matrix was missing the row it found. The
implementation itself recorded seven findings (F-001..F-007). Every
finding that needs paper text has a disposition below, and dispositions
that turned out to have consequences beyond the repair say so. Claims
challenged by review and successfully defended close the table; the
review's refuted findings were not otherwise recorded, so that side of
the register is the curated subset worth carrying, not a complete
accounting.

The single largest change is not a repair but a unification (F-004 +
R-7): the first implementation discharged suspending effects through a
second mechanism, and v0.6 deletes it.

  -----------------------------------------------------------------------
  **RAISED**                **DISPOSITION**
  ------------------------- ---------------------------------------------
  **R-1 · BLOCKER.** A      **Adopted, three connected changes.**
  crash after a retried     Recovery's selection input is the journal
  operation's Commit,       *frontier* --- the entry of greatest
  during a compensable      insertion order --- not the pending stack,
  recovery, reaches         which is a sub-structure of the journal
  \[Done(op), Done(op')\]   (§8.1). §7.4's derived sequence gains its
  with an empty pending     missing fourth row. And the machine's own
  stack. §8.1 dispatches on completion primitive is an operation with a
  "the top of the pending   delivery class, bound by §8.3's equations
  stack" and does not cover exactly as foreign operations are --- the
  the state; the first      first implementation violated its own
  implementation wedged in  declared idempotency and that violation was
  it permanently.           the counterexample. Stated as a proof
                            obligation for Theorem 1.

  **R-4.** The frame graph  **Adopted.** Repr is defined over the frame
  was encoded as a tree: a  *graph*; encoding preserves sharing of
  continuation reachable    continuation values, and decode yields one
  from two frames became    cell per persisted identity (§6.3). The
  two independently         affinity guarantee quantifies over persisted
  consumable copies after a identities within a run (§5.3); an encoding
  cold resume.              that duplicates a continuation is unsound
                            even if each copy is one-shot.

  **R-5.** A consumed       **Adopted.** Rule 2's totality ranges over
  continuation lexically    *reachable machine configurations*, and the
  captured at a later       consumed case persists as a tombstone ---
  suspension is reachable   identity and consumed flag, no frames ---
  but was refused by        whose only future is the affinity error
  encode: Rule 2's totality (§6.3, §7.3). Environment pruning is noted as
  claim was false as        the optimisation that would make the case
  stated.                   unreachable; the tombstone is what makes
                            Rule 2 true without it.

  **F-003.** Read casually, **Adopted.** §7.1 now states the property
  "no replay" seems to      precisely: a continuation is never re-derived
  forbid re-running the     by re-executing pure code --- the
  pure segment after a      continuation that receives r is always read
  resumed Done.             from σ. Pure execution forward of a resumed
                            Done is ordinary execution, not replay.

  **F-002.** §8.1's "no     **Adopted.** With an empty journal, the last
  entry" row is undefined   committed continuation is the program entry
  at program start.         point (§8.1). The recovery driver's
                            empty-journal arm is this sentence as code.

  **F-001 + R-2 + R-3.**    **Adopted.** The class was measured at four
  Rule 3's "one privileged  pure reads of σ plus atomic completion of an
  capability" needed its    already-intended entry; under the unification
  boundary measured and its completion dissolves into ordinary clause
  scope stated; at first    re-execution, leaving the four reads plus one
  the privilege leaked to   dynamic dispatch mask (§7.4). Its scope is
  resumed user code, and    the driver's own frames: Cap\[Local\] is
  decode could mint fresh   granted by the restart entry point to the
  copies of a persisted     driver and is not among the capabilities
  continuation.             re-granted to resumed program continuations.
                            Materialisation is affine: loading a
                            persisted continuation is itself one-shot per
                            run (§5.3). Under the unification the scope
                            rule is ordinary capability discipline plus
                            the stated mask (§7.4).

  **F-007.** persist(K)     **Adopted.** handled(K) includes the effect
  under-specified           of the operation pending at K even when the
  handled(K): recovery      remaining frames no longer reference it;
  re-invokes the pending    recovery re-invokes that operation, and the
  operation, whose effect   merchant case of §10.2 applies to it first.
  the remaining frames may  
  not mention.              

  **R-6.** The persisted    **Adopted.** Code identity covers names with
  header silently dropped   *no* definition at persist time: an absence
  names that had no         sentinel is recorded, and a definition
  definition at encode      appearing under a name the sleeping code
  time.                     references is drift, symmetric with one
                            changing (§10.1).

  **Phase 2 finding.**      **Adopted as text.** A sleeping continuation
  §10.1--10.2 left the      is stranded by a change to what it still
  precision of drift        references and by nothing else; per-
  detection unstated.       dependency Merkle closure achieves this, with
                            SCC-grouped hashing for mutual recursion
                            (§10.2).

  **F-004 + R-7.** The      **Adopted, as the revision's largest
  paper claims one          change.** A suspending effect is discharged
  mechanism; the first      by an ordinary installed handler whose
  implementation had two.   clauses are
  Suspending effects were   strictly tail-resumptive on every control
  discharged by a host      path and whose only non-pure step is a
  registry consulting no    foreign invocation; the five-step protocol is
  capability at the site,   what the machine does to execute such a
  and in-language handlers  clause (§4.3, §6.4, §7.1). Capability,
  for them were statically  policy, budget, revocation, and recovery
  rejected as a stopgap.    re-granting then hold at suspending sites by
  Demonstration B's         the same T-Handle premise that governs prompt
  containment was proved    sites, and demonstration B's containment
  for half the language.    covers the whole language (§13.2).

  §13.1's example re-asks a **Adopted.** The settle example now declares
  human on resume:          request : Summary -\> Decision
  Approval.request without  \@at_least_once(ReqId); the fixture
  a witness is a different  deduplicates by request id and replays the
  question the second time. decision (§13.1).

  **F-005 + F-006.** The    **Recorded as implementation status.** The
  checker that ran the      checker is bidirectional-lite --- declared
  demonstrations is         signatures at definition boundaries, no row
  bidirectional-lite, and   polymorphism, constructor discipline dynamic
  §10.2's migrate adapter   --- sufficient for the falsification
  hook is unimplemented.    criterion's three clauses, short of the
                            calculus's row variables (§13). The migrate
                            hook is specified but unimplemented: a
                            quarantined continuation stays quarantined,
                            durably and losing nothing, until manual
                            intervention (§10.2).

  **R-8 + R-9.** Two test   **Closed in the suite.** A2's crash matrix
  gaps: the A2 matrix       gained the R-1 row --- stated as mandatory
  lacked R-1's crash row,   in §13.1 --- and §4.3's permitted half,
  and §4.3's permitted half suspension by the resumed computation, is
  was untested.             exercised.

  **Challenged and          The machine-local class does not violate the
  unchanged.**              no-hidden-machinery rule: step-2 invocation
                            and the store's atomic commit are exactly
                            what the runtime is permitted to supply. The
                            tail/boundary discipline is per-clause, not
                            per-continuation-identity; the proposed
                            counterexamples fall outside the type
                            grammar. Dependency-precise drift detection
                            accepting changes to unreferenced definitions
                            is intended behaviour, now tested in both
                            directions.
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
  installing a handler requires holding a capability for it (§4) ---
  for every effect, including the suspending effects that reach the
  outside world (new in v0.6).

- **An affinity restriction on continuations** with two independent
  justifications: it makes the persisted stack uniquely owned and
  serialisable in place \[4, 24\], and it removes the reconciliation
  problem that multi-shot resumption poses for a durable journal (§5).

- **A durable machine, a store invariant, and a commit protocol** with
  exactly two atomic transitions per effect site, from which
  recovery-action selection is total and deterministic (§7, §8) --- and
  which is not a second mechanism beside handlers but what the machine
  does to execute a suspending handler clause (§7.1).

- **Authority at rest:** capabilities are non-persistable, so revocation
  needs no protocol and sleeping agents cannot carry stale authority
  across a policy change (§9).

- **An explicit accounting** of which claims are established, which are
  conditional on contracts discharged elsewhere, and which remain design
  arguments awaiting a calculus (§14) --- now updated against a
  reference interpreter that passed both kill tests under hard process
  death and an adversarial review (§0, §13, §14).

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

**2.3 The interchange reading** *(new in v0.6)*

If the intended author is a model (§16), the language's role extends
past its own runtime: one agent generates an ENTOLI program from a
natural-language request; a checker --- not the generating agent ---
verifies authority and recovery; another agent or runtime executes it;
and execution pauses, moves, crashes, and resumes under §7's machine.
The checker is then the hard boundary of the system: agents may propose
programs freely, and only valid programs execute. Under this reading
the human costs of the discipline --- affinity, mandatory delivery
classes, explicit capabilities --- are paid by a generator that does
not feel tedium, and the falsification criterion above becomes an
interoperability contract between agents rather than a compiler
feature.

Three properties of the present design carry the reading further than
intent. The unit of exchange is already defined: a program is a
content-addressed typed AST (§10.1), closed over everything it
references, with its effect signatures --- delivery classes included
--- and compatibility tokens part of the artifact; a persisted
continuation (§6, §10.2) is likewise a well-defined transferable
object. Verification is independent by construction: checking is
static and the payload contains nothing opaque, so a receiver
re-checks with its own checker and trusts no one's toolchain. And
delegated authority is not a credential: by Rule 4 the payload carries
a row --- a requirement --- and never a capability, so a received
program is inert until the receiving side installs handlers from its
own authority, limits survive sub-delegation by the containment of
§13.2, and budgets split rather than replicate (§12).

What the design does not yet supply is stated here in §12's manner ---
as named obligations rather than absorbed silently. The envelope: which
of source, typed AST, module, or continuation-plus-store crosses, and
in what protocol. Authentication: content addressing gives integrity
--- the hash is the name --- and says nothing about origin; the
calculus's contribution is only that authenticity is never needed for
*safety*, which rests on the receiver's own check and the receiver's
own grants. A delegation protocol: attenuation of policy across
agents, beyond what the row and π express today. Negotiation: at
present incompatibility is a clean refusal --- at check time, or
quarantine at resume --- and nothing negotiates past a refusal. And
the comparative claim, on which we are deliberate: against structured
JSON plus a workflow engine, the present evidence supports *more
explicit and machine-checkable* --- the row is the whole tool surface
and composes (§2.2), recovery semantics are part of the artifact,
drift is detectable --- and does not support *more efficient*, which
would require measurement (generation token cost, first-pass validity
beyond n = 6, correction loops, execution overhead) before it is
claimed.

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
> \| handle e with { h } using κ // install a handler under a
> capability
>
> \| resume k v // invoke a continuation (affine)
>
> \| f v // foreign application; f is a declared foreign name, not a
> value (NEW in v0.6; only inside a suspending clause, §4.3, §6.4)
>
> Handler h ::= return x -\> e \| E.op(x; k) -\> e , h
>
> Types τ ::= Unit \| τ -\[ε\]-\> τ \| τ -\[ε\]-o τ \| Cap\[E, π\] \|
> Res\[τ, φ\] // -o the affine continuation arrow; Foreign\[τ1, τ2\]
> classifies foreign declarations only and is not a value type (§6.4)
>
> Effect rows ε ::= {} \| {E} \| ε U ε \| α // α is a row variable
>
> Effect decl E ::= effect E \@s { op : τ -\> τ \@δ , \... }
>
> Foreign decl ::= foreign f : τ1 \~\> τ2 // top-level, host-implemented
> (§6.4)
>
> Suspension s ::= suspending \| prompt
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

**T-Handle's statement is unchanged from v0.5; its force is extended.**
It now governs *every* effect, suspending effects included: there is no
other means of discharge, and no registry beside the handlers (§7.1).
The policy π travels with the installation and is enforced at each
clause execution, with budgeted policies spent per execution (§7.1). A
whole program must genuinely reach row {}, which is the falsification
criterion's first clause made real for the entire language.

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
runs the remainder of e followed by the return clause, under the same
reinstalled handler --- capture is deep (§7.1) --- so E performed by
the remainder is discharged inside k, and εk = ε: the row of the
remainder as seen from outside the handler. The row is therefore not
free information: it is exactly what §4.3 needs and what v0.4
discarded.

**4.3 Handler clauses at the suspension boundary**

The store invariant of §7.2 bounds how many operations may be pending. A
handler that suspends while servicing an operation breaks that bound,
and v0.3 left the case open. The restriction is static --- but the
obvious formulation is too strong, and the correct one needs the latent
row above. In v0.6 the rule splits by the suspension class of the
handled effect: prompt clauses keep the v0.5 condition, and suspending
clauses --- which the machine journals around (§7.1) --- carry two
further conditions that make the crash window well-formed.

Effect declarations carry a suspension class s. An effect is suspending
if performing it may yield to the durable machine and prompt otherwise;
write Susp for the suspending effects.

**Why the naive rule fails.** With T-Resume in place, requiring ε ∩ Susp
= {} over the whole clause body --- tail resume included --- would
forbid resumption whenever the ambient row still contains suspending
work, since the tail resume contributes εk = ε and the remainder of an
orchestration program almost always suspends again. That is not a
restriction, it is a prohibition on handling suspending effects
anywhere but at the very end of a program.

**The distinction that matters** is between the handler doing its own
work and the handler transferring control back. A suspension raised by
the handler while it is servicing the operation adds a second pending
entry; a suspension raised by the resumed computation does not, because
by then the handler has finished. So the restriction is scoped by
position: resume must appear in tail position, and within the clause
judgment the tail resume is typed as a *jump*, at row {} --- its latent
effects belong to the remainder and are already accounted at the handle
site by T-Handle's first premise --- so the side condition reads
directly off the clause body's own row. (Subtracting εk from the body's
row does not express this: since the ambient effects occur in εk, the
subtraction would also erase them from the body's own row, and with
them the very suspensions the rule exists to forbid.)

> **T-CLAUSE (prompt E)**
>
> Γu ; Γa , x : τ1 , k : τ2 -\[εk\]-o τ' \|- e : τ' / ε // tail resume
> typed at row {} ε ∩ Susp = {} tail(resume k, e)
>
> Γu ; Γa \|- ( E.op(x; k) -\> e ) : E =\> τ \~\> τ' / ε

Tail-resumptive handlers are in any case the efficient and common shape
in effect-typed languages \[16, 3\], so the condition costs less than it
appears. The genuine cost remains: a handler cannot await a human
decision while servicing an inference call. The workaround keeps
suspension at one level --- the handler returns a value that causes its
caller to suspend.

**Suspending clauses are the crash window, and the window has exactly
two exits.** A suspending clause executes between the machine's two
atomic transitions (§7.1): its body runs with an intent already
journaled, and its tail resume is the commit. Two further conditions
follow, each forced by a crash-state analysis rather than by taste.

> **T-CLAUSE (suspending E)**
>
> Γu ; Γa , x : τ1 , k : τ2 -\[εk\]-o τ' \|- e : τ' / ε // tail resume
> typed at row {} ε = {} tail(resume k, e) all_paths(resume k, e)
>
> Γu ; Γa \|- ( E.op(x; k) -\> e ) : E =\> τ \~\> τ' / ε
>
> and foreign applications (§6.4) may appear only in such an e.

**The clause's own row is empty.** The body is pure computation, plus
foreign application, plus the tail resume. v0.5's condition excluded
only *suspending* work while servicing; for a suspending clause even a
prompt excursion is forbidden, because it would transfer control out of
the crash window while the intent is open, and a crash during the
excursion creates states the three-way split of §8.1 does not classify.
A relaxation admitting prompt effects whose handlers sit inside the
clause --- so the excursion cannot leave the window --- is plausible and
remains open pending its own crash-state analysis (§16).

**Every control path resumes.** The calculus makes k droppable ---
abort is a legal clause exit --- and tail(resume k, e) constrains only
the resumes that appear. A suspending clause that aborts would exit the
crash window through a third door that is neither atomic transition nor
crash: the intent stays Pending forever while the process runs on, and
a later recovery would re-execute the clause --- possibly resuming this
time, silently reversing the abort. So for suspending clauses only,
every control path must end in resume k: the window has exactly two
exits, Commit and crash, which is what Theorem 1's exhaustiveness
needs. Abort is not lost --- it moves to where §11 already puts
decisions about failure: the foreign call returns a Result, the clause
resumes with it, and the *program*, whose continuation is durable,
decides. Cancellation of a whole computation remains §12's mechanism,
revocation at the subtree root. Prompt clauses keep abort unrestricted.

**5**

**Affine continuations**

k : τ2 -\[εk\]-o τ' is affine: resumable at most once, droppable (that
is abort --- for the suspending-clause exception see §4.3), never
resumable twice. Multi-shot resumption is removed permanently. Two
independent arguments support this and we consider the second the
stronger.

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

**5.3 Affinity at the store**

Affinity has a store-level face, and it belongs here beside the typing
restriction (found as R-3 in the interpreter, where its absence let
decode mint fresh consumable copies of a persisted continuation).
**Materialisation is affine:** loading a persisted continuation from σ
is itself one-shot per process run --- a second load of the same
identity is refused. Together with the identity-preserving encoding of
§6.3, this closes the gap between "each continuation *value* is one-
shot" and the property the theorems actually need, that each *persisted
identity* is resumed at most once *per process run*. Across runs the
guarantee is deliberately weaker: recovery's Done case re-resumes a
persisted identity after every crash that precedes the next Intend
(§8.1), and that is sound because no committed transition is repeated.
What must never happen is two consumptions of one identity within a run
--- which is exactly what duplication (§6.3) or a second load would
permit.

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

**6.3 Persistence preserves identity**

Two corrections from the interpreter's adversarial review (R-4, R-5)
belong to the definition of persistence itself, not to its
implementation.

**Repr is defined over the frame graph, not a frame tree.** A
continuation is a value; a first-class k stored in the heap and
referenced from two frames is *one* continuation reachable twice.
Encoding preserves sharing of continuation values --- a shared identity
is written once and referenced thereafter --- and decode yields exactly
one cell per persisted identity. The consequence is stated plainly: the
affinity guarantee quantifies over persisted identities within a run
(§5.3), and any encoding
that duplicates a continuation is unsound even if each copy is
individually one-shot, because two consumable copies of one identity
*is* multi-shot resumption with a cold resume in the middle.

**A consumed continuation persists as a tombstone.** Affine typing
bounds *use*; it does not erase the binding. A consumed continuation
lexically captured in an environment at a later suspension is therefore
reachable, and Rule 2's totality (§7.3) must cover it. It persists as a
tombstone --- identity and consumed flag, no frames --- whose only
future is the affinity error. The alternative for the calculus is noted:
environments pruned to the free variables of the remaining computation
make the case unreachable and shrink stores, and v0.6 may adopt pruning
as an optimisation, but the tombstone clause is what makes Rule 2 true
without it.

**6.4 Foreign invokers**

Under the unification of §7.1 every contact with the outside world
happens inside a suspending handler clause, and the value that makes the
contact is a *foreign invoker* --- §6.2's re-acquisition discipline
applied to the world itself:

> foreign f : τ1 \~\> τ2 // top-level; f is host-implemented and
> classified Foreign\[τ1, τ2\]

Four conditions, exactly parallel to §6.2's four:

- **Closed.** A foreign binding is top-level and captures nothing.

- **Referenced, not serialised.** It persists as its content identity
  (name plus the declaring program's hash, §10.1); the host re-binds it
  at load.

- **Bound at resume.** The invoker that runs on re-invocation is the one
  bound in the program in force at recovery. Foreign bindings drift
  under compatibility tokens exactly as handlers do, because they are
  part of the handler (§10.2).

- **Opaque.** A foreign application is not an effect and carries no row;
  it is the V-transition of step 2 (§7.1). It may fail --- its result
  type is a Result --- and the delivery class of the operation whose
  clause calls it is a *claim about it*, discharged by the conformance
  harness of §8.3, never by the calculus.

Foreign names are **second-class**: a foreign name is not a value, may
not be bound, passed, returned, stored, or abstracted over, and appears
only in the application form f v. That form is legal **only inside a
suspending handler clause** (§4.3), and not under a λ within it --- a
closure carrying a deferred foreign application out through resume
would contact the world outside any crash window, which is precisely
what the restriction exists to prevent. World contact is therefore
syntactically identifiable, and the D/V annotations of §7.1 become
checkable properties of the term.

**7**

**The durable machine**

**7.1 The effect-site protocol is clause execution**

Configurations are \< e \| K \| σ \>: expression, continuation stack,
durable store. A crash may occur between any two transitions; recovery
resumes from σ alone. Transitions are marked **D** where they commit
atomically and **V** where they do not.

**Correction from v0.5.** v0.5 presented the five-step protocol as
the machine's treatment of an effect site, and left open what actually
performed step 2: the first implementation answered with a host registry
--- a second discharge mechanism beside the handlers, consulting no
capability at the site, against the paper's central claim. v0.6 removes
it. **The five-step protocol is not a second mechanism beside handlers;
it is what the machine does to execute a suspending effect's handler
clause.** A suspending clause is already forced by §4.3 into the shape

> E.op(x; k) -\> \<work\> ; resume k r // resume in tail position, on
> every path

and the protocol reads onto that shape exactly: Intend at capture,
before the clause body runs; the clause body as the crash window, its
only world contact a foreign application (§6.4); Commit at the clause's
tail resume, before control transfers. Everything this section says
about the effect site --- the two atomic transitions, the one ambiguous
crash case, dispatch on δ --- is a statement about clause execution, and
everything T-Handle says --- installation requires Cap\[E, π\],
innermost handler wins, discharge from the row --- governs suspending
effects because they are simply handled effects whose clauses the
machine journals around. The regress ("what handles the handler's own
contact with the world?") bottoms out where §8.3 already put it: the
foreign-function boundary.

do E.op(v) with E suspending proceeds as follows. The machine finds the
nearest Handle frame for E. For a checked program the frame is
statically guaranteed to exist; for an unchecked one the refusal naming
the effect is the *runtime's* entry rule, not the machine's --- the
machine itself stays permissive, and a handlerless suspending operation
simply suspends, which is what lets a continuation be exercised with no
runtime present at all (the encoding kill test needs exactly that). The
machine then captures
K as the entire remaining computation *including that Handle frame*
(deep handlers: a re-suspension of E in the resumed computation is
handled by the same installation). The installation's capability must be
live at Intend, and a budgeted policy is spent there, before the entry
is written --- a refused spend leaves no journal entry, a no-transition
exactly like a declined commit (§7.3). The spend is per clause
execution, recovery re-invocation included: the budget bounds contacts
with the world, not journal entries, so a crash-driven re-invocation
spends again, and an exhausted budget refuses recovery with the entry
left Pending --- recoverable once authority is re-granted (§9.1), never
a wedge.

  --------------------------------------------------------------------------
  **STEP**     **TRANSITION = CLAUSE EXECUTION**                  **KIND**
  ------------ -------------------------------------------------- ----------
  1 · Intend   σ' = σ\[ j \|-\> Pending(#op, v, ι, κid), κid      **D**
               \|-\> encode(K) \] --- intent and continuation,    
               one transaction, at capture, before the clause     
               body runs. Policy checked and budget spent first.  

  2 · Invoke   **The clause body executes with x = v. Its foreign V
               application contacts the external world. This is   
               the crash window.**                                

  3 · Receive  The clause body's result r, in volatile memory.    V

  4 · Commit   σ'' = σ'\[ j \|-\> Done(r) \] --- a single journal **D**
               write, at the clause's tail resume k r, before     
               control transfers. The continuation is already     
               durable.                                           

  5 · Resume   \< resume k r \| K \| σ'' \>. Affinity guarantees  V
               this occurs at most once per run.                  
  --------------------------------------------------------------------------

The entry written at Intend records the operation's delivery class as
declared *at that moment*: recovery dispatches on the record, never on a
re-reading (§8.1), so selection is a function of σ alone even across a
redeployment. The continuation at Intend and the continuation at Commit
are the same object --- the one awaiting r --- so nothing is lost by
writing it early, and the transaction that must be fast at the critical
moment is the small one. Step 1 also retains its original purpose: without a
durable record written before step 2, a crash inside the window leaves
no evidence anything was attempted, and recovery cannot distinguish
\"never started\" from \"may have completed\" \[11, 13\]. We do not
claim to solve the coordinated attack problem. We claim to make its one
ambiguous case dispatchable.

**The no-replay property, said precisely** (a v0.5 reader could
misread it, and the distinction matters): the property is that a
continuation is never re-derived by re-executing pure code --- the
continuation that receives r is always read from σ. Pure execution
*forward* of a resumed Done is ordinary execution, not replay. After a
committed step 4, a crash before the next Intend re-runs the pure
segment that follows the resume; that is §8.1's Done case doing exactly
what it says.

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

One boundary is worth marking after R-1: the pending stack that I1
bounds is a *sub-structure* of the journal, not recovery's whole input.
Recovery's selection input is the journal frontier (§8.1), which may
rest on a Done entry while the pending stack is empty. I1 is untouched
by that generalisation; it bounds what can be pending, not what recovery
reads.

**7.3 Total encoding, failure-atomic commit**

Rule 2 in v0.4 required that persistence be infallible, conflating a
type-system obligation with a physical impossibility; v0.5 split it. The
interpreter's review then falsified the totality half as stated (R-5):
encode was partial over *reachable* states, refusing a consumed
continuation captured in a later suspension's environment. v0.6 states
the rule over the domain it must actually cover.

> **RULE 2 · TOTAL ENCODING, FAILURE-ATOMIC COMMIT**
>
> encode(K) is pure and total over reachable machine configurations,
> with the consumed case persisting as a tombstone (§6.3). The durable
> commit is atomic and may fail, in which case it is a no-transition: σ
> is left exactly as it was.

Totality of encode is a type-system obligation and achievable, since I3
makes every frame layout known at compile time and §6.3 closes the two
reachable cases v0.5 missed. Failure of the commit is then harmless
rather than catastrophic, because a declined transition at step 1 means
the effect was never attempted and a declined transition at step 4
leaves a recoverable Pending. This is the ordinary atomic state-machine
reading, and it is stronger than the rule it replaces precisely because
it demands less.

The implementation constraint survives in weakened form: because encode
must be total and cheap at the moment of suspension, the continuation is
best maintained in representable layout as frames are pushed rather than
converted at the suspension point. That is affordable only because I3 is
static and continuations are affine and hence uniquely owned (§5.1).
v0.5 expected this to be the first thing demonstration A tests. The
interpreter satisfies it by construction --- frames are maintained in
representable layout as they are pushed, and Intend writes the
already-encoded form --- but the no-work-at-suspension condition was
not separately instrumented, and §14 records it as unmeasured.

**7.4 Recovery is an ENTOLI program**

Recovery actions perform external effects, so they can crash; if
recovery ran on separate machinery, it would need its own crash
analysis, its own invariant, and Theorem 1 would not reach it. The trace
is concrete: op succeeds, the machine crashes, recovery invokes the
compensation, the compensation succeeds, the machine crashes again
before retrying, and the store still reads Pending --- so the
compensation runs a second time. One repair is to add durable recovery
states. We reject it, because it answers the symptom and leaves the
recursion: whatever machine executes those states can itself crash. The
uniform answer is that there is no second machine.

> **RULE 3 · RECOVERY IS ORDINARY**
>
> A recovery action is an effect, performed through the five-step
> protocol of §7.1, with its own journal entry, its own delivery class,
> and its own durable continuation. The recovery driver is an ENTOLI
> program, not runtime machinery.

A crash during compensation is therefore an ordinary crash at an
ordinary effect site, handled by the ordinary three-way split, and
covered by the same theorem. The state sequence review asked for ---
Pending → Compensating → Retrying → Done --- is not stipulated but read
off the journal, and the interpreter forced the table to grow a row the
v0.5 text did not have (R-1):

> Pending(op) dispatch on δ(op)
>
> Pending(op), Pending(op') recover op' by δ(op'), then Done(op'), then
> retry op
>
> Pending(op), Done(op') compensation complete; retry op, then Done(op)
>
> Done(op), Done(op') resume op''s continuation, whose next act is to
> complete op; completion of an entry already Done returns the stored
> result

The fourth row is the state R-1 found: a crash after the retried
operation's Commit but before its Resume, during a compensable recovery
--- one crash deeper than v0.5's three-row sequence, reachable, and
where the first implementation wedged. Recovery is correct there precisely
because completion is idempotent, which brings the rule to what makes it
true:

**The machine-local operations are bound by §8.3's equations.** The
recovery driver's privileged class, measured in the pre-unification
interpreter, was exactly: four pure reads of σ (frontier, load_kont,
load_result, comp_arg) plus complete(j) --- invocation and atomic
Done-commit of an *already-intended* entry. Under the unification,
complete is not a separate primitive: completing an entry *is* the
machine re-executing the journaled operation's clause, with the
ordinary Commit at its tail resume (§8.1), and the privileged class
shrinks to the four pure reads plus the dispatch mask below. What
survives verbatim is the obligation completion carried --- declared
idempotent, so **re-invocation against an entry already Done returns
the stored result** --- now the equation op ; op ≈ op applied to the
machine's own recovery step. v0.5 never stated that the machine is
bound by §8.3's equations exactly as foreign systems are. It is: R-1
*was* the machine violating this equation (a completion finding Done
refused instead of returning the stored result). v0.6 states it as a
lemma obligation --- every machine-local operation satisfies its
declared equation, the pure reads trivially and re-invocation by the
Done rule above --- and Theorem 1's crashes-during-recovery case is
proved by induction on the frontier with that lemma (§8.1).

**The privilege has a scope, and it is the driver's own frames** (R-2).
Cap\[Local\] --- the capability for the machine-local class --- is
granted by the restart entry point to the driver and is *not* among the
capabilities re-granted to resumed program continuations: once the
driver resumes a program continuation, the program must not inherit the
ability to read σ and re-materialise continuations. In the first
implementation this was a leak. Under the unification this scope rule is
ordinary capability discipline --- Cap\[Local\] is simply never granted
to program code --- plus one stated dynamic rule for the unchecked
machine: machine-local effect dispatch never crosses a root-continuation
resume boundary. The static guarantee is Cap\[Local\]'s absence from
program typing contexts; the dispatch mask is its dynamic proxy and
stays until a total checker exists. For ordinary effects, a recovery
prelude's installations being reachable from resumed code is not a leak
but §9.1's content: a resumed computation holds precisely the authority
in force at resume time, and the prelude is that authority.

Cap\[Local\] does not persist --- the same as every capability (Rule
4). If the driver's own continuation (a compensation entry) is resumed
after a crash, the recovery prelude re-grants it.

This section also satisfies, in the strongest available form, the
binding condition on demonstration A in §13: no recovery machinery
appears in the interpreter that is not represented in the calculus,
because the recovery driver is written in the language, and the class of
things it is privileged to do has now been measured (§14).

**8**

**Recovery**

**8.1 Determinacy**

Recovery reads σ and acts on its **frontier**. The definition is exact,
because R-1 showed an informal one ("the top of the pending stack") does
not cover the reachable states:

> **The frontier of σ is the journal entry of greatest insertion
> order.** Step 4's Commit updates an entry in place and does not
> advance the frontier.

So after R-1's chain --- retry committed, crash before resume --- the
frontier is the Done *child*, even though the parent's Done was written
later. Dispatch is total over the three forms the frontier can take,
and every case names a continuation that is already durable (I2):

  ------------------------------------------------------------------------
  **FRONTIER**    **RECOVERY**
  --------------- --------------------------------------------------------
  no entry        Resume the program entry point: with an empty journal,
                  the last committed continuation *is* the entry point.
                  (The degenerate reading v0.5 left implicit; the
                  driver's empty-journal arm is this sentence as code.)

  Pending         **Dispatch on δ,** with K recovered from the identifier
                  in the entry. The only ambiguous case.

  Done(r)         Resume the named continuation with r. The effect is
                  never repeated; if that continuation's next act is a
                  completion of an outer entry, completion of a Done
                  entry returns the stored result (§7.4).
  ------------------------------------------------------------------------

Per effect site this is v0.5's three-way crash split, unchanged: a
crash before step 1 leaves no entry for that site, a crash in the
window leaves Pending, a crash after step 4 leaves Done. What the
frontier formulation adds is generality across *nested* sites --- the
recovery-during-recovery states, where the informal reading ran out one
crash early.

Dispatch on δ, for a Pending frontier, reads the delivery class
*recorded in the entry at Intend* --- not the program in force, whose
declaration may have drifted while the entry slept. If the program in
force resolves the entry's operation to a different class (or no longer
declares it), recovery refuses --- delivery is part of the computed
signature (§10.1), and this is that drift surfacing at the dispatch
site, before any compensation could act on a changed reading; the entry
stays Pending, recoverable after a compatible redeployment. With the
record agreeing, dispatch re-executes the clause of the journaled
operation --- with the journaled argument and witness, under the
handler installation in force at recovery (§7.1, §9.1):

  -----------------------------------------------------------------------
  **DELIVERY CLASS    **ACTION ON PENDING**
  Δ**                 
  ------------------- ---------------------------------------------------
  idempotent          Re-execute the clause.

  at_least_once(ι)    Re-execute the clause with the same witness ι,
                      written at step 1; the remote system deduplicates
                      \[14\].

  compensable(op')    Perform op' as an effect (Rule 3), then re-execute
                      the clause. The Saga discipline \[9\], made
                      crash-safe by running the compensation through the
                      same protocol.

  --- none ---        **Unreachable.** T-Op rejects the program.
  -----------------------------------------------------------------------

Re-execution is strictly more faithful than calling a registered
external: pure pre- and post-processing inside the clause re-runs from
the journaled argument --- deterministic, and not replay in §7.1's sense,
since the continuation is never re-derived --- and the §10.2 merchant
problem attaches to exactly the right object, the handler whose
sig-and-compat identity the store already verifies.

> **THEOREM 1 · RECOVERY-ACTION DETERMINACY**
>
> For every reachable crash point, including crashes during recovery,
> there exists exactly one recovery *action*, computable from σ and the
> delivery class of the operation named at the frontier. No recovery
> path consults runtime state beyond σ, and no crash point admits two
> candidate actions.
>
> *Sketch. The three-way crash split is exhaustive by construction,
> since steps 1 and 4 are the only D-transitions. The frontier is unique
> by definition and its continuation resolvable by I2. In the Pending
> case δ is total over declared operations, and the fourth row is empty
> because T-Op admits no operation without a delivery class. Crashes
> during recovery reduce to the same cases by Rule 3; the induction is
> on the frontier, terminates because the pending depth is bounded by
> §8.2, and uses the machine-local conformance lemma of §7.4 in the
> Done case --- the case where the first implementation was a
> counterexample (R-1) until the lemma's obligation was honoured.*

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
> compensable(op') requires, over world states W: op'(W) = W whenever
> op is undelivered in W, and op'(op(W)) = W for every W

The compensable obligation is *state-indexed* (corrected per F-012: an
earlier draft stated it as the free equation family op^a ; op' ; op^b ≈
op, which under compositional ≈ entails op ; op ≈ op --- prefix
op' ; op ≈ op by op --- excluding the non-idempotent operations the
class exists for). The two conditions say: compensating a non-delivery
is a no-op (the compensation fires in the ambiguous case, where op may
never have been delivered), and the compensation undoes one delivery.
Non-idempotent operations satisfy both --- reserve-one / release-one-
if-held --- and under the protocol's guarantee that op' runs only in
the ambiguous window they make every compensate-then-retry schedule
equivalent to one delivery. One residual machine obligation is recorded
open (F-012): a crash during the retry's own window re-enters
\[Pending(op), Done(op')\] and re-delivers op bare, so under the
current dispatch a compensable operation must additionally tolerate
re-delivery on the retry path, by witness or by idempotence.

These are discharged by a conformance harness rather than by assertion:
each declared class generates a test exercising the stated equation
against a sandbox, and a handler failing its equation is rejected at
deployment --- implemented in the reference interpreter, whose strict
mode refuses unproven contracts and whose compensable check exercises
the family at (a, b) = (1,1), (0,1), (1,2), catching a compensation
that passes the v0.5 equation but misbehaves against a clean state.
That converts a claim into an obligation with
a check attached, which is roughly where unsafe blocks actually live in
practice --- unverified, but visible, localised, and reviewable.

One party is *not* exempt from these equations, and v0.5 neglected to
say so: the machine itself. The machine's own recovery step carries a
declared delivery class, and the equations bind it not as tested
contracts but as proof obligations --- completion declares idempotent
and must satisfy op ; op ≈ op, that is, re-invocation against an entry
already Done returns the stored result (§7.4), which is precisely what
the first implementation failed to do (R-1). For foreign operations the
equation is a claim about the world, discharged by the harness; for
machine-local ones it is a lemma about the machine, discharged by proof
(§7.4, Theorem 1).

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
Kframes is written. A capability inside a persisted Handle frame
persists as an effect-name slot --- which effect was handled, never the
authority to handle it. Capabilities are re-granted by the handler at
resume, which is where they came from originally, so the programming
model is unchanged.

Under the unification there is now exactly one story, and it covers
recovery. The suspended K contains the Handle frames of every
installation in force at suspension, each with its capability stripped
to its slot. At restart, the entry point runs an ordinary prelude ---
the same handle … using κ nesting that main used --- around the
recovery driver. Decode re-binds slots against the installations in
force *now*; a slot with no matching installation is a refusal naming
the effect, and revocation is therefore not an operation at all but an
absence: the restart entry point cannot install what it holds no
capability for. Budgets bind the same way: the new κ's policy governs
every subsequent clause execution (§7.1), so a re-granted agent is
bound by the figure in force at resume, not the figure at suspension.

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

**Absence is an identity** (R-6: the first implementation silently
dropped the case). A name the sleeping code references that had *no*
definition at persist time is recorded in the header with an absence
sentinel and verified as still absent at resume. A definition
*appearing* under such a name is drift, symmetric with one changing:
the sleeping code's behaviour depended on the absence.

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
> // handled(K) includes the effect of the operation pending at K, even
> // when the remaining frames no longer reference it: recovery
> // re-invokes that operation, so the merchant case applies to it
> // first.
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

The compatibility identity attaches to the *installed handler* ---
foreign bindings included, since they are part of the handler (§6.4).
Under the unification that is the object that actually changes between
suspension and resume, so the check lands where the merchant problem
lives.

Bumping the token is a deliberate act that strands continuations
captured under the old contract, and stranding becomes a migration event
with a hook rather than a silent misinterpretation. Where no adapter is
supplied, resume fails cleanly and the continuation is quarantined
rather than lost. This is schema versioning applied to effect semantics,
and it puts the obligation where the knowledge is. (In the reference
interpreter the adapter hook is specified but unimplemented: quarantine
is durable and sticky, nothing is lost, and release today requires
manual intervention.)

**Verification is dependency-precise, in both directions.** A sleeping
continuation is stranded by a change to what it still references and by
nothing else. v0.5 rejected pinning the implementation for stranding too
much; the interpreter confirms that per-dependency Merkle closure, with
SCC-grouped hashing for mutually recursive definitions, achieves the
intended precision --- a change to a referenced definition strands, a
change to an unreferenced one does not, and both directions are what
§10.2's own argument requires (the second was challenged in review and
is intended behaviour).

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

Where policy is enforced is fixed by the unification: π travels with
the handler installation, and budgeted policy is spent at each clause
execution --- at Intend for a fresh operation, at re-invocation during
recovery (§7.1).

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

**Evaluation**

v0.5 specified three demonstrations on a tree-walking interpreter, each
corresponding to one clause of the falsification criterion in §2, with
one binding condition throughout: no recovery machinery may appear in
the interpreter that is not represented in the calculus. **They have now
been run.** Every kill below is hard process death --- a subprocess
aborted at a named crash point, restarted from cold durable state with
no volatile memory carried across --- never an in-process exception, and
remote fixtures record their own state independently. The binding
condition held: the recovery driver is expressed as generated ENTOLI
terms, and its privileged class is measured in §7.4. The checker in
force for these runs is bidirectional-lite --- declared signatures at
definition boundaries, no row polymorphism, constructor discipline
dynamic --- sufficient for the falsification criterion's three clauses,
short of the calculus's row variables.

**13.1 A · Kill and resume inside the window**

> fn settle(inv) -\> Outcome / {Ledger, Approval} {
>
> let a = audit(inv)
>
> match do Approval.request(a.summary, ι = req_id(inv)) { //
> \@at_least_once(ReqId): suspension, days
>
> .approve =\> do Ledger.post(a, ι = idem(inv)) // \@at_least_once
>
> .reject =\> .halted(a)
>
> }
>
> }

The example is corrected from v0.5, whose request carried no witness:
re-asked on resume it is a *different question* to a human --- a false
delivery contract of exactly the kind §8.3 says the calculus cannot
check. The request now carries a request-id witness; the approval
fixture deduplicates by it and replays the recorded decision.

Four kills, four expected outcomes: before step 1, nothing posted and
resume posts once; inside the window with the remote committed, resume
re-invokes under the same ι and the remote deduplicates; inside the
window with the remote not committed, resume posts once; after step 4,
no re-invocation. Pass condition: exactly one ledger entry in all four
runs, each restarted from cold storage. Secondary condition, from §7.3:
no serialisation work occurs at the suspension point.

**Result: passed** --- exactly one ledger entry in all four runs. The
secondary condition holds by construction --- frames are maintained in
representable layout as they are pushed (§7.3) --- but was not
separately instrumented. The compensable variant (A2) passed its
full crash matrix, which now mandatorily includes the R-1 row: a crash
after the retried operation's Commit but before its Resume, during a
compensable recovery. That row is one crash deeper than v0.5's derived
sequence reached, and this machine's recovery could not have been
called implemented without it. Pending depth never exceeded two (I1).

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

**Result: passed, and --- after the unification --- for the whole
language.** The program fails to compile naming both causes: the
escaping effect and the missing capability. In the first implementation
this held only for prompt effects, because suspending effects were
discharged by a global registry: a delegated computation could reach any
suspending tool the *process* carried, regardless of what the delegator
handed over, and the paper's central security claim was proved only
where the stakes were lowest. Under §7.1 the same compile error covers
a suspending Ledger.post inside research --- whether or not a ledger
binding exists in the process --- because reachability is now decided
by the installed handlers and nothing else.

**13.3 C · Authority at rest**

Suspend an agent holding a budgeted inference capability. Dump the
persisted continuation and confirm the absence of capability material,
budget, and credentials --- and, per §9.2, record what non-authority
data it does contain, since that is the store's actual classification.
Revoke the grant, attempt resume, observe a refusal naming the missing
capability. Restore with a smaller budget, resume, observe the agent
bound by the new figure.

**Result: passed.** The byte-level check found no capability or budget
material in the persisted continuation; the derived classification was
recorded (confidential --- the store held a customer-adjacent value, and
§9.2 forbids pretending otherwise). Revocation is exercised as §9.1 now
states it: the restart entry point simply does not install the Infer
handler, and resume refuses, naming the effect whose slot found no
installation. Re-granting under a smaller budget bound the resumed agent
to the smaller figure, and durable Done entries meant re-granting never
re-ran completed inferences.

**14**

**Status of claims**

Review of v0.5 observed that several items labelled theorem or rule are
convincing design arguments rather than consequences of a fully
specified calculus. That remains correct, and the table now also
records what the interpreter changed: one conjecture is demonstrated
for a real value domain, one theorem met a counterexample and gained a
stated proof obligation, and one claim --- one mechanism --- became
true by construction rather than assertion.

  -----------------------------------------------------------------------
  **CLAIM**                  **STATUS**
  -------------------------- --------------------------------------------
  Effect rows discharge on   Established, and inherited from known
  handling (T-Handle)        results on typed handlers \[21, 16, 3\].
                             Now governs suspending effects too (§7.1);
                             the first implementation's second discharge
                             mechanism is deleted.

  Affinity of continuations  Established as a typing restriction. Its
                             implementation payoff is supported by \[4,
                             24\] and by the interpreter's construction
                             (frames maintained in representable
                             layout, §7.3), but unmeasured. Persistence
                             preserves identity, not only count (§6.3),
                             and materialisation is affine (§5.3) ---
                             both corrections from review.

  Theorem 1 ·                **Conditional.** Follows from I1--I3, the
  recovery-action            totality of δ, the depth bound of §8.2, and
  determinacy                --- new --- the machine-local conformance
                             lemma of §7.4. Covers crashes during
                             recovery. A counterexample was found in the
                             implementation (R-1) and repaired; the
                             lemma it exposed is a stated proof
                             obligation. I1--I3 are asserted, not
                             proved, pending a full transition relation
                             (the metatheory phase).

  End-to-end exactly-once    **Not claimed.** Conditional on delivery
                             contracts discharged outside the calculus
                             (§8.3). The conformance harness now exists
                             and exercises the equations against
                             fixtures; a harness is testing, not proof.

  Theorem 2 · authority at   **Conditional.** Follows immediately from
  rest                       Rule 4 given that only Kframes are encoded.
                             The frame/capability split is implemented
                             and byte-level checked (§13.3) but not
                             formalised.

  Secrecy of persisted       **Withdrawn** (§9.2).
  stores                     

  Rule 2 · total encoding,   **Modelling choice.** Totality now ranges
  atomic commit              over reachable configurations and is made
                             true by the tombstone clause (§6.3, §7.3)
                             --- as stated in v0.5 it was false (R-5).
                             Failure-atomicity is a requirement placed
                             on the store, not a claim made about it.

  Rule 3 · recovery is       **Design decision with a measured
  ordinary                   consequence.** The privileged class is four
                             pure reads of σ plus one dynamic dispatch
                             mask --- completion having dissolved into
                             ordinary clause re-execution --- scoped to
                             the driver's own frames (§7.4). The derived
                             state sequence gained its missing row from
                             a real counterexample; its completeness is
                             argued, not proved.

  Resume compatibility       **Design decision, not a result.** Soundness
  (§10.2)                    depends on deployers bumping the token
                             honestly, which no mechanism enforces.
                             Detection is dependency-precise in both
                             directions, and absence is an identity
                             (§10.1) --- both now exercised.

  Serialisable continuations **Demonstrated for the interpreter's value
  are feasible               domain:** cross-process cold resume under
                             hard process death, with sharing preserved.
                             The general claim --- arbitrary host value
                             domains, compiled representations ---
                             remains open, and remains the design's
                             weakest premise.
  -----------------------------------------------------------------------

To deserve the word calculus in the usual sense, what remains is
unglamorous and specific: complete typing rules, evaluation contexts and
a full transition relation, substitution and weakening under the affine
context, handler reduction, persistence typing, and a preservation
result establishing that every reachable configuration satisfies INV(σ),
from which Theorem 1 would genuinely follow rather than being argued ---
with the machine-local conformance lemma of §7.4 among its stated
obligations.

**15**

**On ordering**

An earlier review asked for the metatheory before implementation, and
also asked for demonstration A. We proposed the reverse emphasis; review
subsequently accepted it, on condition that the state-machine and typing
defects be repaired before any code is written. §4 and §7 were that
repair. The reasoning was: the risky content of this design is
operational rather than type-theoretic, and none of it is discovered by
proving preservation --- all of it is discovered on the first kill
test.

v0.6 can report how the wager paid. The interpreter exists, both kill
tests passed, and the ordering produced exactly the kind of finding it
was chosen to produce: R-1 is a Theorem 1 counterexample one crash
deeper than the v0.5 text's derived sequence, surfaced against the
running machine, in a state no reader of that text had reached ---
including its authors. A preservation proof written against the v0.5
machine would have faithfully preserved the defect, because the defect
was in the machine's specification, not its implementation. The repair,
in turn, was forced entirely by the design's own vocabulary --- the
machine had declared its completion idempotent and was not honouring
the equation --- which is the outcome that argues the vocabulary is the
right one.

So the order stands: the invariant was stated first, which was cheap
and owed; the interpreter came second and was the cheaper
counterexample-finder; the full metatheory comes third, written against
a machine already known to exist.

**16**

**Limitations**

- **Continuation serialisation remains the weakest premise as a general
  claim.** It is demonstrated for the interpreter's value domain (§13,
  §14), which retires it for this interpreter's setting --- one
  tree-walking interpreter over a closed value domain --- and says
  nothing yet about compiled representations or host-language interop.

- **Delivery contracts are unverified.** §8.3's conformance harness now
  exists and a strict mode refuses unproven contracts; a harness is
  testing, not proof, and a handler can pass its equation on a sandbox
  and fail in production.

- **Compatibility tokens depend on discipline.** Nothing forces a
  deployer to bump the token when semantics change. This relocates the
  problem to a place where it is visible, which is progress, not a
  solution.

- **Nested suspension is forbidden rather than solved, and suspending
  clauses are stricter still.** The empty-row and all-paths-resume
  conditions (§4.3) buy a two-exit crash window at the cost of any
  excursion from it --- even a log line must today live outside the
  clause. The candidate relaxation (prompt handlers installed inside
  the clause) awaits a crash-state analysis.

- **Concurrency is absent, not deferred cleanly.** I1 is stated globally
  and will need restating per-branch; §12 lists what has been protected
  and implies what has not.

- **The capability half needs no new language.** It is achievable in
  Scala 3 today \[20\]. A reasonable reading of this paper is that
  ENTOLI should be a durability extension to a capability-tracking host
  rather than a language of its own. Whether Rule 4 can be expressed
  that way is unknown, and answering it is the shortest available path
  to refuting the whole project.

- **The intended author is undecided, with first data.** Whether ENTOLI
  is written by people or by models is upstream of every surface
  decision --- and the interchange reading of §2.3 stands or falls with
  it. Affinity discipline and mandatory delivery classes are
  tedious for a person and free for a checked generator --- and the
  first measurement points the same way: six fresh-context model
  instances, given only a one-page reference card, produced six
  first-pass-compiling programs across orchestration tasks, including
  the deliberate non-tail-resume temptation (answered correctly with a
  return clause) and a continuation-typed helper. With zero failures
  there was no failure mass to attribute across the restrictions, so
  the measurement machinery was verified inversely: four deliberately
  wrong programs each fail with exactly their restriction's diagnostic,
  and each compiles when precisely that restriction is disabled. The
  caveats are stated plainly --- n = 6, a single model family, tasks
  authored by the checker's author and calibrated to the worked
  examples --- and the convergence study, on adversarially chosen tasks
  with an independent model, remains to be run. The falsification
  condition (thrash, or one restriction dominating) was not triggered;
  the quiet argument that the intended author is a checked generator
  now has its first supporting data.

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

v0.5 closed by predicting that both claims were refutable by a small
interpreter and suspecting one would be. The interpreter exists, and
the suspicion was nearly right: review found a genuine Theorem 1
counterexample one crash deeper than v0.5's recovery table, and the
one-mechanism claim was false of the first implementation until the
second mechanism was deleted. Both repairs were forced by the design's
own vocabulary, which is the evidence we weigh most. Both theorems
remain conditional, and §14 says on what.
The metatheory --- preservation of INV(σ) over a full transition
relation, with the §7.4 lemma among its obligations --- is the next
artifact.

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
