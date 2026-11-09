**REFERENCE IMPLEMENTATION · ENGINEERING PLAN**

**ENTOLI Core**

From semantic design to executable falsification: what to build, in what
order, and what result would end the project

PillarDelta · Plan v1.0 · August 2026 · Basis: ENTOLI working paper v0.5
(enclosed)

> **SUMMARY**
>
> The first engineering target is not a compiler and not a user-facing
> language. It is ENTOLI Core: a reference interpreter that implements
> the v0.5 calculus closely enough to falsify it. Success is a written
> go/no-go, not a shipped artifact.
>
> The plan below adapts a proposed phase ordering with one structural
> change. The proposal scheduled three to four weeks of static checker
> ahead of the work it itself identifies as the existential test. This
> plan inverts that: the **evaluator and continuation encoding come
> first**, with every static guarantee given a dynamic proxy until the
> checker exists. If encoding fails structurally, that costs two weeks
> rather than six, and no checker will have been written against
> semantics that do not survive.
>
> Two kill points, at Phase 2 and Phase 4. A fourth demonstration is
> added at Phase 5, testing whether a model can write ENTOLI at all ---
> the one major claim in the paper with no scheduled experiment. Roughly
> three months to a decision.

**1**

**Objective**

Build the smallest system that can make the v0.5 claims false. The paper
states three falsification criteria and three demonstrations; the
implementation exists to run them. Where implementation convenience
conflicts with v0.5 semantics, **the implementation changes**. Where the
implementation cannot faithfully realise a rule, that is evidence
against the design rather than licence to add hidden runtime behaviour.

**Definition of complete.** The v0.5 core constructs are implemented; no
semantic rule is substituted by host behaviour; demonstrations A, A2, B
and C pass; every restart uses durable state only; capabilities do not
persist; continuation encoding covers every value the checker accepts;
compatibility mismatch is detected; store invariants are checked
mechanically; and every deviation from v0.5 is documented in
EVALUATION.md.

**2**

**Changes from the proposed plan**

The proposed plan is adopted substantially. Its stop conditions are its
strongest section and are kept in full, because they convert \"the
implementation is hard\" into \"the semantics are wrong,\" which is the
distinction that keeps a project like this honest. Its derivation of a
fifth crash test from Rule 3 is a genuine finding and becomes
demonstration A2. Six changes follow.

  -----------------------------------------------------------------------
  **CHANGE**                 **REASON**
  -------------------------- --------------------------------------------
  **Phase order inverted:**  The proposal calls encoding the existential
  evaluator and encoding     test and Phase 3 a major kill point, then
  before the static checker. puts three to four weeks of checker in front
                             of it. Its own risk assessment argues for
                             the reverse. See §4.

  **Dynamic proxies for      Affinity by a runtime consumed flag,
  static guarantees** during capability exclusion structurally by the
  Phases 1--3.               host type system, representability by
                             hand-written test programs. The same
                             invariants hold; only their enforcement
                             mechanism is deferred.

  **Phase 1 duration         Row polymorphism, affine contexts,
  doubled** (checker: 3--4   tail-position analysis, persistability
  weeks, not 1--2).          predicates and the compensation restriction
                             in one to two weeks is optimistic by roughly
                             a factor of two.

  **New open question:** the Loading the store is an effect; routing it
  recovery driver's access   through the five-step protocol requires an
  to σ.                      Intend, which writes σ, which requires
                             loading σ. A regress. See §8 and stop
                             condition 10.

  **Rust rationale           Ownership discipline is the wrong argument,
  replaced.**                since it does not implement ENTOLI's
                             affinity. The real one is that absence of a
                             serialisation impl is a compile-time
                             property, so capability exclusion from
                             frames becomes structural. See §3.4.

  **New demonstration D** at Every phase tests whether the semantics are
  Phase 5: can a model write sound and implementable. None tests whether
  ENTOLI?                    the language is writable by its intended
                             author, and the paper leaves that author
                             undecided. For an orchestration language for
                             agents this is load-bearing. See §10.5.

  **Approval.request is not  A suspending request that re-asks a human on
  idempotent.**              resume yields a different answer or approval
                             fatigue. The example should declare it
                             at_least_once with a request witness --- an
                             accidental illustration that delivery
                             classes are claims about the world, which is
                             what the conformance harness exists to
                             catch.
  -----------------------------------------------------------------------

**3**

**Governing principles**

**3.1 The paper is the semantic source of truth**

Deviations are recorded, not absorbed. Every point where the interpreter
does something the calculus does not describe is a finding for v0.6.

**3.2 No hidden recovery machinery**

The runtime may supply durable-store primitives and effect invocation.
The recovery driver itself must be representable as an ENTOLI program.
If it cannot be, Rule 3 is wrong and §7.4 of the paper comes apart.

**3.3 No replay between effect sites**

The continuation needed after an external effect is made durable at
Intend, before invocation. The runtime must never reconstruct a
continuation by re-executing pure code from an earlier durable point.
This is the correction that v0.5 made and the thing most likely to be
quietly reintroduced by an implementer under time pressure.

**3.4 Every static guarantee gets a dynamic proxy first**

This is what makes the reordering safe. Each guarantee the checker will
eventually establish has a runtime equivalent available from Phase 1, so
the risky operational work proceeds under the same invariants.

  -------------------------------------------------------------------------
  **GUARANTEE**      **DYNAMIC PROXY (PHASES        **STATIC FORM (PHASE
                     1--4)**                        5)**
  ------------------ ------------------------------ -----------------------
  One-shot resume    consumed flag on the           Affine context,
                     continuation; panic on second  T-Clause
                     resume                         

  No capability in   Capability type has no         Perm(Cap) = false
  frames             serialisation impl; cannot     
                     compile into a frame           

  Representability   Frames built only from a       Repr / Reviv predicates
                     closed set of encodable forms  

  Tail resumption    Evaluator structure permits    Tail-position analysis
                     resume only in tail position   

  Compensation depth Runtime assertion: pending     Declaration side
                     depth ≤ 2                      condition
  -------------------------------------------------------------------------

**3.5 Encoding and storage failure are separate**

encode(K) is pure and total over the language domain. Durable commits
may fail, and a failed commit is a no-transition leaving σ unchanged.
Host-level resource exhaustion is outside the calculus and is handled as
process failure, not as a semantic case.

**3.6 Build the smallest executable semantics first**

No surface syntax, no performance work, no tooling until the core state
machine survives forced process death.

**4**

**Phase plan**

Durations are for one experienced engineer working full time. The two
kill points are marked; nothing after a kill point is worth starting
until it clears.

  ----------------------------------------------------------------------------
  **PHASE**       **DELIVERABLE**           **EXIT CONDITION**      **TIME**
  --------------- ------------------------- ----------------------- ----------
  0 · Freeze      Rust workspace, semantic  Repo builds; one        2 d
                  data types, ADR log, v0.5 canonical mapping from  
                  rules transcribed into    paper concepts to code  
                  implementation notes. No  types exists.           
                  parser.                                           

  1 · Evaluator   Core AST, explicit heap   A handler intercepts an 1 w
                  frames, handler           operation, inspects its 
                  installation, operation   argument, and           
                  capture, one-shot resume  tail-resumes exactly    
                  with consumed flag,       once. Double resume     
                  opaque runtime            panics.                 
                  capabilities.                                     

  **2 · Encoding  Representable frame       A suspended computation 1--2 w
  KILL POINT**    layout, encode/decode,    is encoded, the process 
                  content-addressed         is destroyed, and it    
                  definitions, resource     decodes and resumes in  
                  descriptions, no host     a fresh process ---     
                  closures in frames.       with no external effect 
                                            involved.               

  3 · Durable     SQLite store, journal,    Every Pending entry     1 w
  store           Intend and Commit         references an           
                  transactions, invariant   already-durable         
                  checker, crash injection  continuation. I1--I3    
                  points.                   hold after every        
                                            durable transition.     

  **4 · Recovery  Restart entry point,      Demonstrations A and A2 2 w
  KILL POINT**    pending dispatch,         pass. Pending depth     
                  same-witness retry,       never exceeds two. No   
                  recovery driver expressed privileged second       
                  in core ENTOLI,           recovery machine was    
                  crash-safe compensation.  needed.                 

  5 · Static      Typing, effect rows,      Demonstration B fails   4 w
  checker         T-Op, T-Handle, T-Resume, at compile time for the 
                  affine analysis, latent   exact effect and        
                  rows, tail-position,      capability reasons,     
                  suspension classes,       with both named.        
                  persistability,           Demonstration D returns 
                  compensation restriction. a convergence profile   
                  Throwaway concrete syntax and per-restriction     
                  for demonstration D.      ablation.               

  6 · Authority   Resume-time grant         Demonstration C passes: 1 w
                  resolution, revocation    no capability material  
                  behaviour,                encoded, revoked grant  
                  data-classification       halts resume, re-grant  
                  reporting, re-acquisition binds the new policy.   
                  witnesses.                                        

  7 ·             Typed-AST hashes, effect  A sleeping continuation 1 w
  Compatibility   signature hashes,         cannot silently resume  
                  compatibility tokens,     under a structurally or 
                  quarantine on mismatch,   semantically            
                  delivery conformance      incompatible handler.   
                  harness.                                          

  8 · Evaluation  All compile-fail tests,   Written go/no-go. The   1 w
                  all crash points,         implementation either   
                  repeated crashes,         supports the v0.5       
                  recovery crashes,         claims or supplies a    
                  revocation, mismatch,     concrete                
                  commit failure.           counterexample.         
                  EVALUATION.md.                                    
  ----------------------------------------------------------------------------

**Total to decision: roughly twelve to fourteen weeks.** Phases 0--4 ---
five to six weeks --- clear both kill points and are where the project
most plausibly dies. Phases 9 (semantics v0.6), 10 (metatheory) and 11
(language decision gate) follow only after Phase 8 returns a go.

> **WHY DEMONSTRATION B CAN WAIT**
>
> Deferring the checker delays demonstration B to week nine. That costs
> little epistemically. B is largely the Odersky result, already
> demonstrated in Scala 3 with capture checking, so it functions as a
> regression test rather than a discovery. If B fails, an implementation
> mistake has been made. A and C are the actual experiments: A because
> serialisable continuations are the paper's own stated conjecture, C
> because authority at rest is the claim nobody else has made.

**5**

**Kill points and stop conditions**

**5.1 Phase 2 --- can a continuation be encoded at all?**

The paper lists serialisable continuations as its weakest premise. This
phase answers it with no durability, no store, and no external effects:
build a computation, suspend it, encode, kill the process, decode,
resume, check the result. If arbitrary well-typed continuations cannot
be represented without serialising host closures, **stop and revise the
semantics**. Do not proceed to the durable protocol on the hope that it
works out.

**5.2 Phase 4 --- is recovery ordinary?**

Rule 3 claims recovery is an ENTOLI program rather than runtime
machinery. This phase tests it directly, and §8 below flags the specific
place it may fail. If the recovery driver requires privileged behaviour
with no representation in the calculus, Rule 3 is wrong and the
pending-stack invariant loses its justification.

**5.3 Stop conditions**

Any of the following is a semantic finding, not an implementation bug.
Work stops and returns to the paper.

1.  A well-typed continuation cannot be encoded without serialising
    arbitrary host closures.

2.  Correct recovery requires replaying pure code between effect sites.

3.  A crash during recovery requires a privileged second recovery
    machine.

4.  A capability must be persisted to make resume work.

5.  The nested-suspension restriction cannot be enforced
    compositionally.

6.  Pending depth can exceed two in a well-typed sequential program.

7.  A Pending journal entry can exist without its continuation being
    durable.

8.  An unrecoverable external effect can transitively disappear from a
    caller's effect row.

9.  The interpreter needs behaviour with no representation in the
    calculus to make demonstration A pass.

10. **New:** the recovery driver's store access requires anything beyond
    pure reads of σ and the store's own atomic primitives.

11. **New:** demonstration D shows no convergence --- diagnostics do not
    monotonically reduce the error set across iterations. This does not
    halt the project, but it redirects the gate. See §10.5.

**6**

**Repository**

> entoli/
>
> ├─ crates/
>
> │ ├─ entoli-syntax/ AST, core syntax, source spans
>
> │ ├─ entoli-core/ values, frames, continuations, evaluator \[Phase 1\]
>
> │ ├─ entoli-encode/ frame layout, encode/decode, definition hashes
> \[Phase 2\]
>
> │ ├─ entoli-store/ durable store trait + SQLite backend \[Phase 3\]
>
> │ ├─ entoli-runtime/ five-step protocol, effect invocation \[Phase 3\]
>
> │ ├─ entoli-recovery/ recovery driver as ENTOLI terms \[Phase 4\]
>
> │ ├─ entoli-types/ types, rows, policies, delivery classes \[Phase 5\]
>
> │ ├─ entoli-check/ static checking \[Phase 5\]
>
> │ ├─ entoli-testkit/ fake remotes, crash injection, store inspection
>
> │ └─ entoli-cli/
>
> ├─ docs/ semantics-v0.5.md, invariants.md, decisions/, EVALUATION.md
>
> ├─ examples/ settle.ent, containment.ent, authority_at_rest.ent
>
> └─ tests/ typing/, persistence/, crash/, recovery/, capabilities/,
> compatibility/

**entoli-recovery** carries a standing rule: it must not acquire
privileged behaviour that cannot be expressed in the language. Any
commit adding a host-side special case there is a stop-condition
trigger, and the crate should be reviewed on that basis alone.

**7**

**Core data model**

**7.1 Continuations**

The interpreter must not use the host call stack as the semantic
continuation. Frames are an explicit heap structure, built directly in
encoding-compatible form rather than converted at suspension.

> Frame ::= ApplyArg \| ApplyFn \| Let \| Handle \| ResumeBoundary \|
> Match \| Return
>
> Continuation {
>
> id: ContinuationId, // hierarchical: 0, 0.1, 0.1.0
>
> frames: Vec\<Frame\>,
>
> latent_effects: EffectRow,
>
> consumed: bool // dynamic proxy for affinity
>
> }

Hierarchical identifiers from the start, even though v1 is sequential.
Flat counters are unmigratable once a store contains them, and this is
one of the three concurrency-compatible invariants the paper commits to.
Frames must never hold host closures, raw pointers, sockets, file
handles, arena references, capability values, or dynamically opaque host
values --- only typed symbolic references.

**7.2 Journal**

> JournalEntry {
>
> journal_id, parent_journal_id, // parent gives the pending stack
>
> operation_identity, encoded_argument, idempotency_witness,
>
> continuation_id, state // Pending \| Done(encoded_result)
>
> }

No Compensating or Retrying states. Recovery operations create ordinary
nested entries, and parent_journal_id is what makes the depth-two stack
readable from the store. This is Rule 3 expressed in a schema.

**7.3 Durable continuation**

> DurableContinuation {
>
> continuation_id, encoded_frames,
>
> handled_effects: \[ { signature_hash, compatibility_token } \],
>
> data_classification // derived join, per §9.2 of the paper
>
> }

**8**

**The open engineering question**

Paper §7.4 says recovery is an ordinary ENTOLI program. The recovery
driver must load σ to find the pending entry. But loading σ is an
effect, and if it runs through the five-step protocol it needs an
Intend, which writes σ, which requires loading σ. **That is a regress,
and the paper does not address it.**

The presumed resolution is a class of machine-local effects --- pure
reads of σ and the store's own atomic primitives --- exempt from
journalling and granted only to the recovery driver. That is probably
right and probably obvious, but it qualifies Rule 3: recovery is an
ordinary program *plus one privileged capability*. Phase 4 must
establish the exact boundary of that capability and record it.

> **WATCH CONDITION**
>
> If the machine-local effect class needs anything beyond pure reads of
> σ and the store's atomic commit primitives --- in particular if it
> needs to perform external effects, or to suspend --- then Rule 3 does
> not hold and §7.4 requires rewriting rather than annotation.

**9**

**Crash-injection framework**

Crash testing is a feature of the prototype, not an afterthought. Every
effect site, including recovery effect sites, exposes named test points.

> before_intend \| after_intend \| after_remote_before_receive
>
> after_receive_before_commit \| after_commit \| before_resume

**Hard process death only.** A crash must not be simulated by throwing
an exception while interpreter memory survives --- that is the failure
mode most likely to produce a false pass, because volatile state
silently carries across. Tests persist, terminate the process, start a
fresh one, load durable state only, and continue. Subprocesses, not
in-process exceptions.

**Independent remote fixtures.** Fake ledger, approval service, HTTP
host and resource service must each record their own state independently
of ENTOLI. Without that independence the two cases that matter ---
remote committed with local commit missing, and remote not committed
with local state pending --- cannot be distinguished, and demonstration
A proves nothing.

**10**

**Demonstrations**

**10.1 A · kill and resume inside the window**

> effect Ledger \@suspending { post : Entry -\> Receipt
> \@at_least_once(Idem) }
>
> effect Approval \@suspending { request : Summary -\> Decision
> \@at_least_once(ReqId) }
>
> fn settle(inv) -\> Outcome / {Ledger, Approval} {
>
> let f = audit(inv)
>
> match do Approval.request(f.summary) {
>
> .approve =\> do Ledger.post(f, ι = idem(inv))
>
> .reject =\> .halted(f)
>
> }
>
> }

  ----------------------------------------------------------------------------
  **RUN**   **CRASH POINT**           **EXPECTED**
  --------- ------------------------- ----------------------------------------
  A1        before Intend             No post occurred. Restart runs the
                                      effect once. Ledger count 1.

  A2        after remote commit,      Store holds Pending. Restart reuses the
            before Commit             witness; remote deduplicates. Ledger
                                      count 1.

  A3        before remote commit      Store holds Pending. Restart retries.
                                      Ledger count 1.

  A4        after Commit              Store holds Done. Restart does not
                                      re-invoke. Ledger count 1.
  ----------------------------------------------------------------------------

**Secondary criteria, all mandatory:** restart uses no prior process
memory; the continuation is present in the store at Intend; no pure
segment is replayed; encode performs no external operation; store
invariants hold at every durable point.

**10.2 A2 · crash during compensation**

Required by Rule 3 and absent from the paper's four kill positions.
Original operation succeeds remotely; crash before local completion;
recovery selects the compensation; the compensation succeeds remotely;
crash before the original retry; restart.

Expected: the recovery action is itself recoverable; no third pending
entry is created; the compensation is not unsafely repeated; pending
depth never exceeds two; the final state satisfies the declared delivery
contract. This test is mandatory before recovery semantics may be called
implemented.

**10.3 B · containment as a compile error**

A delegate function handling only Http cannot claim an empty row when
the delegated computation transitively performs Fs.read, and no Cap\[Fs,
\_\] is in scope to handle it. The diagnostic must name both facts. The
test proves containment beyond the delegated envelope and nothing more
--- misuse of an authorised capability from within the envelope is out
of scope and must not be claimed.

**10.4 C · authority at rest**

Create an inference capability with budget B1; run to suspension;
inspect the durable continuation and verify no capability, grant or
budget credential is encoded; revoke the grant; restart; resume must
fail for want of a current grant; create a new grant with B2 \< B1;
resume; the computation must run under B2.

**The test must not claim the store contains no secrets.** It records
the derived data classification of the persisted values instead. This is
the correction v0.5 made in §9.2 and the demonstration must not quietly
re-assert the withdrawn claim.

**10.5 D · can a model write it?**

Phases 0--8 test whether the semantics are sound and implementable. None
tests whether the language is writable by its intended author, and §16
of the paper leaves that author undecided. For an orchestration language
for agents that is the load-bearing question, and it is currently the
only major claim with no scheduled experiment.

It matters more here than for comparable work. Odersky et al. report
that agents generate capability-safe Scala without significant loss in
task performance. ENTOLI adds affine continuations, mandatory delivery
classes, the tail-position restriction, suspension classes and
persistability --- five further ways a generated program fails to
compile, none of them imposed by capture checking. If first-pass compile
rate collapses, the result is a sound language nobody's agents can
write.

**Setup.** A deliberately plain concrete syntax, written for this test
and discarded afterwards: minimal sugar, everything explicit. The
plainness is the point. It measures whether the *semantics* are
generable rather than whether some particular syntax is, which is the
more fundamental question and the one that can kill the design. No
amount of sugar rescues semantics a model cannot hold.

The model receives the effect declarations, a one-page summary of the
typing rules, and two or three worked examples. This is in-context
learning, which is also the realistic deployment condition, since a
house language lives in a system prompt or a skill. A poor result must
not be read as \"models cannot write ENTOLI\" when the model was never
told how. Twenty to thirty tasks, drawn from the demonstrations' own
domain: orchestration with tool calls, approvals, budgets and
compensations.

**Metrics.** Four, in ascending order of importance.

- First-pass compile rate.

- Iterations to green.

- **Monotonicity:** does the diagnostic set shrink at every iteration,
  or does fixing one error introduce another.

- Per-code attribution across E001--E014.

The last two matter more than the first. A low first-pass rate is
survivable if diagnostics drive convergence --- that is the compiler
acting as an error-correcting decoder over the model's output
distribution, which is the original argument for a language of this
shape. It is fatal only if models thrash: fixing E003 and introducing
E005, error counts oscillating rather than falling. Thrashing is a
locality failure, and locality is a property of the design rather than
of the model.

**Ablation.** Absolute numbers are uninterpretable, so each task is also
run with restrictions individually disabled --- affine check off,
delivery classes optional, tail-position off, suspension classes off ---
and the delta measured. This converts \"is ENTOLI writable\" into \"what
does each restriction cost in writability,\" which is actionable in a
way a single percentage is not. If one restriction accounts for most
failures, that is precisely the thing to redesign.

**The strongest fix is rarely a better message.** Where a single
diagnostic dominates, the first move is to make the error
unrepresentable in the surface syntax rather than to explain it better:
a resume form that is syntactically only a tail form cannot produce E004
at all. D at Phase 5 therefore feeds directly into any surface-language
work after the gate (§15), and should be repeated afterwards to measure
whether the syntax actually helped.

> **FALSIFICATION**
>
> If iterations to green fail to converge across the task set, or if a
> single restriction accounts for a majority of failures and cannot be
> made syntactically unrepresentable, that is a finding about the design
> rather than about the model. It does not halt the project. It moves
> the decision gate decisively toward option B or C: semantics worth
> having, hosted in a language models already write.

**11**

**Test strategy**

**11.1 An oracle for the evaluator**

Phase 1 has a verification problem the later phases do not: how do you
know the handler semantics are right, before any type checker exists to
constrain them? OCaml 5 provides one. Its effect handlers are one-shot
for the same representational reasons as ENTOLI's, so a small
transliteration of each handler test program gives an independent oracle
for control flow --- which handler fires, in what order, with what
continuation. Divergence indicates an evaluator bug rather than a
semantic finding. Worth the half-day it costs, and worth abandoning once
the checker exists.

**11.2 Property tests**

- The encoder never encounters an unsupported value form, over generated
  well-typed programs.

- No encoded continuation contains a capability.

- Every pending entry resolves a continuation.

- Pending-stack depth never exceeds two.

- Restarting from any durable point requires no volatile state.

**11.3 Conformance harness for delivery contracts**

The type system does not prove remote semantic properties, so the
implementation makes the obligations executable. Each declared class
generates a test against a sandbox:

> idempotent op ; op ≈ op
>
> at_least_once(ι) op(ι) ; op(ι) ≈ op(ι)
>
> compensable(op') op ; op' ; op ≈ op

Handler metadata records contract_status: untested \| passed \| failed,
and strict mode refuses to run a handler whose declared contract has not
passed. This is testing, not proof, and the distinction belongs in the
documentation as well as the code.

**11.4 Invariant checker**

assert_store_invariant(store) runs after every durable transition,
before shutdown, immediately after restart, and during every
crash-injection test. It is the cheapest instrument in the project and
should be left on in debug builds permanently.

**12**

**Diagnostics**

Many of ENTOLI's claims are compile-time claims, so the diagnostics are
part of the result rather than a convenience. Each must give a source
location, the violated rule, the relevant effect or type, and a minimal
explanation.

> E001 Unhandled effect E008 Policy forbids persistence
>
> E002 Missing capability E009 Capability cannot cross durable boundary
>
> E003 Continuation resumed more than once E010 Invalid re-acquisition
> witness
>
> E004 Non-tail resume in suspending handler E011 Compensation is itself
> compensable
>
> E005 Nested suspension forbidden E012 Effect signature mismatch on
> resume
>
> E006 Non-representable continuation value E013 Compatibility-token
> mismatch on resume
>
> E007 Non-revivable resource E014 Missing resume-time capability grant

**13**

**Acceptance matrix**

  ------------------------------------------------------------------------
  **MILESTONE**     **REQUIRED EVIDENCE**                     **PHASE**
  ----------------- ----------------------------------------- ------------
  Evaluator         Handler intercepts, inspects,             1
                    tail-resumes once; double resume panics   

  Encoding          Valid continuation survives process       2
                    destruction and resumes cold              

  Intend            Continuation durable before any remote    3
                    contact                                   

  Atomic commit     Induced commit failure leaves prior       3
                    durable state intact                      

  Recovery dispatch Selection from the pending stack is       4
                    unique                                    

  Recovery          Pending depth never exceeds two           4
  recursion                                                   

  At-least-once     Same witness reused after crash; remote   4
                    deduplicates                              

  Compensation      Crash during compensation recovers safely 4
                    (A2)                                      

  Demonstration A   Exactly one ledger entry across all four  4
                    crash runs                                

  Affinity (static) Double resume rejected at compile time    5

  Latent effects    Resumed suspension appears in the         5
                    continuation row                          

  Nested suspension Illegal handler suspension rejected       5

  Persistability    Foreign handle capture rejected; valid    5
                    witness accepted                          

  Demonstration B   Compile-time containment failure naming   5
                    both causes                               

  Authority at rest No capability material in encoded frames  6

  Revocation        Sleeping computation loses revoked        6
                    authority                                 

  Re-grant          Resumed computation runs under current    6
                    policy, not captured policy               

  Demonstration C   Full sequence including derived data      6
                    classification                            

  Demonstration D   Convergence profile and per-restriction   5
                    ablation recorded                         

  Compatibility     Incompatible handler quarantines rather   7
                    than resumes                              
  ------------------------------------------------------------------------

**14**

**First sprint**

Two weeks, covering Phases 0 through 2. The goal is to reach the first
kill point as fast as is honest.

12. Rust workspace; source spans and identifiers; ADR log.

13. Runtime Value; explicit Frame enum; Continuation with id, frames and
    consumed flag.

14. Evaluation contexts; handler lookup; delimited capture at do E.op;
    one-shot resume enforced dynamically.

15. Opaque Capability value with no serialisation impl --- this is Rule
    4 obtained structurally, before any checker exists.

16. Small handler programs executing correctly, cross-checked against
    the OCaml oracle.

17. Stable frame encoding; content-addressed top-level definitions;
    encode/decode round trips; property tests over generated frames.

18. Encode, terminate the process, decode in a fresh process, resume,
    verify the result.

> **SPRINT EXIT**
>
> A computation survives a cold restart with no external effect
> involved. If this fails structurally --- if well-typed continuations
> cannot be represented without serialising host closures --- stop. Do
> not proceed to the durable effect protocol until the representation
> model has been revised in the paper.

**15**

**The decision gate**

After Phase 8 returns a go, and after v0.6 and the metatheory, one of
three directions is chosen. The project succeeds if the semantics
survive, even where the outcome is not a standalone language.

  -----------------------------------------------------------------------
  **OPTION**          **CHOOSE IF**
  ------------------- ---------------------------------------------------
  A · Standalone      The semantics require control unavailable in
  language            practical host languages, surface syntax can make
                      the affine and effect discipline usable, and the
                      durability model needs whole-program compilation.

  B · Extension to a  Capability tracking already exists, effect rows can
  typed host          be integrated, and durable boundaries with
                      non-persistent captures can be enforced. Explicitly
                      permitted by the paper's own falsification
                      criterion, and the outcome §16 of the paper regards
                      as most likely for the capability half.

  C · Typed DSL over  Full language ownership is unnecessary and the
  a host runtime      semantic core can be enforced within a restricted
                      subset.
  -----------------------------------------------------------------------

**Immediate next action.** Phase 0, then the evaluator, then encoding.
Not parser design, and not the type checker. The first existential test
is continuation encode and decode across process death; the first
decisive system test is demonstration A2, the crash after remote commit
but before local commit. If A2 cannot pass without adding semantics
absent from the core design, ENTOLI returns to semantic revision before
any further engineering.
