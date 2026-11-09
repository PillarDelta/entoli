# ENTOLI metatheory — v0.6 (Phase 10)

Drafted 2026-08-28 against paper v0.6 and the reference interpreter; revised
twice the same day under adversarial review (rounds of 16 agents; round one
confirmed 8 findings, round two 11). Round two's blocker cluster was not a
proof bug but a machine defect: recovery dispatch resolved delivery classes
against the program in force, so the store invariant was not even a function
of σ. That is now **F-011** in EVALUATION.md, repaired in the implementation
(the delivery class is recorded in the journal entry at Intend; dispatch reads
the record; disagreement refuses), and this document describes the repaired
machine. Remaining round-two findings are incorporated where they land, marked
*(rev)*.

Deliverable per plan Phase 10 and paper §14: a full transition relation, the
store invariant in the role of persistence typing (no separate σ ⊢ judgment is
defined; INV is the property preservation preserves — stated plainly rather
than implied *(rev)*), and a preservation result from which Theorem 1 follows
rather than being argued; plus the §7.4 machine-local conformance lemma.

**Honesty header.** Part A (the durable machine) carries complete case
analyses. Part B consolidates the v0.6 typing rules with standard-territory
sketches; it enters Part A only through syntactic invariants SI-1..SI-3. For
unchecked programs the theorems hold for the **proxied relation** (§3), whose
guards are listed exhaustively — including the load-time validation guard the
current Runtime API leaves to its caller (A-11), recorded as a correspondence
gap in §8. Standing assumptions: Rule 2 totality; codec roundtrip
(Assumption R); **Assumption D** *(rev)* — the continuation captured at
[Comp]'s child [Intend] is the driver's own frame whose next act is
[Complete(parent)], a property of the *generated* driver (its shape is
fixed by `driver_for`: `let compensated = do op′(…) in
resume (load_kont j) (complete j)`), exercised by the A2 crash matrix;
and, for the progress theorem only, finitely many crashes and terminating
clause bodies. Two dynamic-proxy gaps found in round three are recorded as
findings rather than papered over: P-4 (the mask does not cover the
[Rec-Empty] door) and P-5 (no runtime tail guard in durable windows) —
both closed statically for checked programs.

---

## Part A · The durable machine

### 1 · Configurations and the store

A **store** σ = (J, T, F) is:

- J, the **journal**: a finite sequence of entries in insertion order. An
  entry is e = (j, op, δ̂, a, ι, κ, s, p): identifier j; operation name op
  (with its effect); **δ̂, the delivery class recorded at the entry's
  [Intend], as the program then in force declared it** — dispatch and I1
  read δ̂, never a re-resolution (F-011) *(rev)*; argument blob a; witness ι
  (the machine enforces δ̂ = at_least_once ⇒ ι present; the converse is not
  enforced *(rev)*); continuation identifier κ; state s ∈ {P} ∪ {D(r)};
  parent p ∈ {⊥} ∪ ids(J). Insertion order is total.
- T, the **continuation table**: κid ↦ (blob, H, q, ℓ) — the closed frame
  encoding; the verification record H = {(E, sig(E), compat(E))} for
  E ∈ handled(K) plus the code-identity header with absence sentinels; the
  quarantine mark q ∈ {⊥} ∪ Reason, durable and sticky; the derived
  classification ℓ.
- F, the **final result**: ⊥ or a value blob.

`pend(σ)`: the pending entries in insertion order. `frontier(σ)`: the entry
of greatest insertion order, ⊥ if J is empty. `σ[j := D(r)]` updates one
entry's state in place — a Commit never advances the frontier.
`σ[q(κ) := reason]` sets one quarantine mark.

Configurations: ⟨ e | K | σ ⟩ᵥ (running; volatile state v = grant table G,
materialised set L, contracts-passed set, strict flag, foreign bindings, the
program-and-compat in force); ⟨ crash | σ ⟩; ⟨ final(r) | σ ⟩. **[Crash] is
available from every running configuration and from ⟨ final(r) | σ ⟩ before
the result is observed** *(rev: a process can die between [CommitFinal] and
its caller seeing the value; the restart case for that state is [Rec-Final]
below)*. Crash forgets everything but σ. "Recovery resumes from σ alone"
scopes *selection*; execution of the selected action additionally consults
the authority in force (G, contracts, program), which is §9.1's content.

The program in force may differ across [Restart]s within one run. **No
cross-restart program stability is assumed anywhere below** *(rev)*: δ-drift
on a journalled entry is met by refusal (the δ̂ precondition in §4), sig- and
compat-drift by the [Materialise] guard, and an undeclared op by refusal.

### 2 · The invariant

> **INV(σ)** ≙ I1 ∧ I2 ∧ I3 — all three functions of σ alone *(rev: with δ̂
> recorded, I1's compensation clause no longer consults the program)*.
>
> **I1 (bounded pending).** pend(σ) is a parent-chain of depth ≤ 2, and at
> depth 2 the child satisfies p(e₂) = j(e₁) and (effect, op)(e₂) is exactly
> the compensation target named by δ̂(e₁).
>
> **I2 (continuation precedence).** Every entry's κ ∈ dom(T), written in
> the same atomic transaction; κids are never rebound — dom(T) only grows,
> and a cell's blob, H, and ℓ are immutable once written (only q changes).
>
> **I3 (representability) — a stated deviation from paper §7.2's I3**
> *(rev: recorded, per the governance rule, rather than silently
> substituted)*: the paper asserts Repr/Reviv/Perm HOLD of every
> persisted value, established statically; here Repr/Reviv weaken to
> Assumption R (tested, not proved) and Perm to a recorded
> classification plus a deployment obligation. **I3a (no smuggled
> authority, structural):**
> no blob in σ — T blobs, J argument/result blobs, F — contains capability
> or host material; a Cap value *anywhere* (Handle frames and first-class
> occurrences alike) encodes as an effect-name slot, and witnesses are
> plain strings *(rev)*. **I3b (decodability):** every blob decodes to what
> was encoded — **Assumption R (roundtrip)**, exercised by the roundtrip
> suite and both kill tests, not proved. **I3c (policy):** ℓ records the
> derived classification (level(K), §9.2); Perm enforcement is a
> per-deployment obligation recorded here, not a machine theorem *(rev)*.

The preservation burden is I1 and I2; I3a holds by construction at every
enc site, I3b by Assumption R, I3c by construction of ℓ.

### 3 · Syntactic invariants, and the proxied relation

For a checked program, T-Clause (both forms) and §8.2's declaration
condition give:

- **SI-1 (two-exit window, non-terminational form** *(rev)*): a suspending
  clause body performs no effect, its only world contact is foreign
  application not under a λ, no capture occurs inside a window, and a
  clause execution *that completes without being cut* ends in its tail
  resume. (Termination of clause bodies is NOT part of SI-1 — the language
  has general recursion — and is a separate hypothesis where needed,
  Theorem 1b.)
- **SI-2 (one-shot resume).** Each continuation value is resumed at most
  once per run.
- **SI-3 (compensations re-invocable).** δ(op′) ∈ {idempotent,
  at_least_once} for every declared compensation op′; every performed op
  is declared with some δ.

**The window's dynamic extent** *(rev)*: from an entry's [Intend] to that
entry's [Commit] — the frames above the clause boundary belong to the
window while the boundary frame is live.

**The proxied relation** *(rev: round two showed the draft's guard list was
incomplete and mis-justified)*: the sub-relation in which each of the
following refuses — a no-transition *before* any σ-write:

- (i) an operation performed inside a window (machine E005);
- (ii) a clause exiting without its tail resume (all-paths sentinel);
- (iii) a foreign application reaching the top-level drive loop;
- (iv) a second resume, or a second materialisation, of one κid (E003, L);
- (v) a resume of the window's own continuation in non-tail position —
  **prompt clauses only** in the implementation (the clause-boundary
  check lives on the prompt path); the durable window today has NO
  dynamic tail guard: a non-tail resume of the sentinel is not refused,
  execution continues, and the post-resume value is what gets committed.
  Statically closed for checked programs (E004); an open dynamic-proxy
  gap for unchecked ones, recorded as **P-5** *(rev: round three)*;
- (vi) **per-load validation, at every program load — [Restart]s
  included** *(rev: once-at-run-start would let a redeployed compensable
  chain past)*: the E011 declaration-shape condition — compensable(op′)
  requires δ(op′) non-compensable — which is the one leg the
  pending-depth-3 argument needs. (The other two shape facts hold by
  different means: δ-totality is structural in the AST, and the witness
  requirement is the machine's per-step MissingWitness refusal, §1.)
  Without (vi), an unchecked program declaring a compensable *chain*
  reaches pending depth 3 through two honest [Comp] dispatches — no
  per-step guard can see it coming, and the runtime's after-the-fact
  depth assertion is a detector, cited nowhere in the proofs *(rev:
  round two's second blocker)*. Correspondence caveat: `Program::validate`
  implements the E011 leg, but the Runtime API does not invoke it —
  callers must (A-11; roadmap T1.3); recorded in §8.

Runs of checked programs satisfy SI-1..SI-3 by Proposition B4; proxied
runs satisfy the halves of them the §5 proofs cite, because each violating
*σ-writing* transition is refused before its write — with two scope notes
*(rev: round three)*: SI-1's proxied discharge covers the suspending half
only (a prompt capture whose handler is installed *inside* the clause body
runs entirely within the window's sub-machine, cannot cross the window
boundary, and is not refused — harmless to every proof below, which cite
SI-1 only to exclude in-window [Intend]s); and the durable window's tail
discipline is static-only (P-5, guard (v) above). **Admissible runs**:
runs from the empty store in which **each restart segment** either runs a
checked program or runs proxied *(rev: per-segment, so a redeployment
across a crash may switch branches without leaving the theorems'
quantifier)*.

### 4 · The transition relation

**D** transitions atomically commit a new σ; **V** transitions leave σ
unchanged; a failed or refused D-transition is a no-transition, σ intact,
refusals re-runnable. (The model treats a refused [Intend] as leaving
volatile state unchanged too; the implementation debits the budget before
the transaction — divergence A-35, roadmap T2.7, recorded in §8 *(rev)*.)

**Pure and control steps (V).** As before; σ unchanged.

**[Intend] (D).** Performing a suspending op *declared by the program in
force* (an undeclared op refuses — nothing without a δ can be journalled):

    σ' = σ + entry(j, op, δ̂, a, ι, κ, P, p),
    T' = T[κ ↦ (enc(K), H(K), ⊥, ℓ(K))], one transaction;
    δ̂ := the declared class now in force; j, κ fresh.

With a Handle frame present, the capability-liveness, budgeted-spend, and
(strict mode) contract checks are **preconditions of the whole
transaction**: a failure refuses before any σ-write — a no-transition,
exactly as paper §7.1 states and as the implementation orders them
(checks precede `store.intend`). **Without a Handle frame no checks run
and the write still happens** — the machine is deliberately permissive
(paper §7.1: the encoding kill test needs machine-level suspension with
no runtime) — and the run then halts as an unhandled suspension (the
runtime's E001 refusal), awaiting [Restart] *(rev: round three corrected
round two's C7 reading, which had the with-handler checks gating only the
window)*. p = ⊥ except in [Comp].

**[Commit] (D).** At the window's tail resume for j:
σ' = σ[j := D(enc(r))] — the result, like [Intend]'s argument a = enc of
the performed value, passes through enc, so every σ-write is literally an
enc site as I3a and Theorem 2 assume *(rev)*.

**[Resume] (V).** Transfer with r; consumed flag; materialised κids pass
through L.

**[Materialise].** Reading κ from T for resumption: requires κ ∉ L,
q(κ) = ⊥, and the drift guard — for every (E, s, c) ∈ H(κ), the program in
force declares E with sig(E) = s and compat token c; the code-identity
header verifies. On H-drift: **[Quarantine] (D)** — σ' = σ[q(κ) := reason],
sticky, then refusal. On code-identity drift: refusal *without* a mark
*(rev: only the H half quarantines in the machine)*. On success: L-entry,
decode (Assumption R), slots re-bind against G (E014 if absent).

**[CommitFinal] (D).** At top-level value with pend(σ) = ∅: F := enc(v) if
F = ⊥, first write wins (the implementation silently ignores a divergent
second commit — A-42; nothing below relies on detection).

**[Crash].** From any running configuration and from ⟨final(r)|σ⟩.

**[Restart].** ⟨ crash | σ ⟩ → recovery. **[Rec-Final]** *(rev)*: if
F ≠ ⊥, return dec(F) — no frontier consultation, no driver. Otherwise the
generated driver runs under the restart prelude with fresh volatile state.
The driver's pure reads are frontier, load_result, comp_arg;
materialisation and completion are governed by their own rules. The
machine-local dispatch mask confines Local/Protocol to the driver's
frames — with one recorded hole *(rev: round three's blocker, hand-
verified)*: the [Rec-Empty] door runs the program entry point directly,
interposing no resume boundary, so the mask does not cut there. Closed
statically for checked programs (Cap[Local] is absent from program typing
contexts); an open dynamic-proxy gap for unchecked ones, recorded as
**P-4** with a candidate repair (materialise the entry point *as* a
continuation — F-002's degenerate reading made literal — so the door
crosses a root boundary like every other).

**[Complete(j)]** *(rev: named as its own rule — it is invoked from
resumed driver frames on non-frontier entries, where no dispatch fires)*:
available exactly in driver frames under the mask. If s(j) = D(r): return
dec(r) — V, no σ-write (Lemma 3(i)). Else: under the recovery-enabling
preconditions, re-execute the clause of op(j) with the journalled a, ι
under the installation in force (window, V), then [Commit] on j.

**Recovery dispatch** (F = ⊥), on frontier(σ):

- **[Rec-Empty] (V).** ⊥: run the program entry point.
- **[Rec-Done].** (j, …, D(r)): [Materialise] κ(j), resume with r.
- **[Rec-Complete].** (j, …, δ̂ ∈ {idempotent, at_least_once}, P):
  [Materialise] κ(j) *first* — the machine evaluates the continuation
  operand before the completion, so the drift guard precedes
  re-invocation on this path *(rev)* — then [Complete(j)].
- **[Comp].** (j, …, δ̂ = compensable(op′), P): perform op′ with parent j —
  an [Intend] whose captured continuation is, by **Assumption D** (the
  driver structural clause, §8), the driver's own frame whose next act is
  [Complete(j)] — then the child's window and [Commit], then
  [Complete(j)]. Two honesty notes *(rev)*: the compensation precedes the
  parent's [Materialise], so parent H-drift surfaces after the
  compensation has acted; *delivery* drift — the half that chooses the
  dispatch — cannot, because of the precondition below, and once the
  child is committed the parent's remaining plan is [Complete(j)] under
  ANY δ̂, so no re-check at completion is needed: the dispatch decision δ̂
  influenced is already taken and durable. And if the restart prelude
  holds no installation or grant for op′'s effect (revocation while
  sleeping, §9.1), the compensation's own [Intend] still writes — the
  permissive machine — and the run then halts E001/E002 with pend =
  [j, child]: not a σ-intact refusal but a benign, I1-admissible halt,
  recoverable after re-installation (the child's frontier dispatch is
  [Rec-Complete] by SI-3).

**Recovery-enabling preconditions**: **the program in force resolves the
frontier op to exactly δ̂** — checked at dispatch, before [Comp] could
intend a child (F-011) *(rev: round two's first blocker)*; a live grant
with budget for the re-invoked effect (spend per execution, paper §7.1);
strict mode's contract check; materialisation's guards (q, drift, E014).
Each of these failures is a σ-intact, re-runnable refusal — except a
quarantine (one sticky mark) and [Comp]'s missing-compensation-authority
case above (one benign child entry then halt) *(rev: the draft claimed
σ-intact universally)*.

**Closure claim.** No transition removes a journal entry or T cell,
reorders J, rewrites a blob, or changes δ̂/a/ι/κ/p of an existing entry;
entry states move only P → D; dom(T) is extended only by [Intend]; the
only write to an existing T cell is [Quarantine]'s mark; F is written
once.

### 5 · Preservation

> **Lemma 1 (frontier totality).** frontier(σ) is ⊥ or a unique entry with
> state P or D. ∎

> **Lemma 2 (window discipline)** and **Theorem P (preservation)**, proved
> together by strong induction on the length of an admissible run (the
> draft's arbitrary-configuration quantification was false and its Lemma-2
> citation circular; the mutual induction over runs is the repair).
>
> **Lemma 2.** In every admissible run: a parent-⊥ [Intend] fires only
> with pend(σ) = ∅; a parent-j₀ [Intend] (i.e. [Comp]'s child) fires only
> with pend(σ) = [j₀].
>
> **Theorem P.** Every σ reachable in an admissible run satisfies INV(σ).
>
> *Proof (mutual; empty store vacuous).*
>
> *Lemma 2 at step n+1.* A parent-⊥ [Intend] fires from ordinary program
> execution; by SI-1/guard (i) no window contains a capture, so every
> prior program-level entry left its window through [Commit] — or halted
> the run as an unhandled suspension (the permissive write), in which
> case no later [Intend] occurs in that segment and post-restart
> segments enter through the doors, which complete the pending entry
> first *(rev: the third disjunct)* — or this is a recovery run's
> re-entry into program execution. The re-entry doors, exhaustively
> *(rev: rounds two and three split and extended the cases)*:
>
> - [Rec-Empty]: J empty, pend = ∅. [Rec-Final]: terminal, no re-entry.
> - [Rec-Done] on a **Done root**: pend = ∅ — **any** pending entry (root
>   or child *(rev: the draft argued roots only)*) is earlier than the
>   Done root (the root is the frontier), and since states move only
>   P → D it was already Pending at that root's parent-⊥ [Intend],
>   contradicting the IH at that earlier step.
> - [Rec-Done] on a **Done child, parent still P** (the third derived
>   row's state [P, D]): the resumed continuation is the driver's frame
>   ([Comp]'s structural clause) whose next act is completion of the
>   parent; program execution is re-entered only after the parent's
>   [Commit], at which point pend = ∅.
> - [Rec-Done] on a **Done child, parent already D** — the state
>   [D(op), D(op′)], which is R-1's state *(rev: the draft mislabeled
>   [P, D] as R-1's)*: pend is already ∅ (the parent was the only pending
>   entry at the child's [Intend] by the IH; it is now D; nothing is
>   later than the child), and re-entry goes through the driver frame
>   whose completion of the parent takes Lemma 3(i)'s stored-result
>   branch — no D-transition at all on this door.
> - A completed [Rec-Complete] on a **root**: its [Commit] emptied pend.
> - A completed [Rec-Complete] on a **child** (reachable after a crash in
>   the compensation's window): committing the child leaves pend =
>   [parent] — and the tail resume enters the *driver's* frame
>   (Assumption D), not program execution; re-entry waits for the
>   parent's [Complete(j)] as in the Done-child doors. *(rev)*
> - A completed **[Comp] pass** in one crash-free run *(rev: round three
>   — a distinct route, not subsumed by the root-[Rec-Complete] door)*:
>   the child's [Commit] leaves pend = [parent]; the parent's
>   [Complete(j)] empties pend before the parent's program continuation
>   resumes.
>
> A parent-j₀ [Intend] fires only in [Comp], whose preconditions include
> frontier = j₀ pending with δ̂(j₀) compensable. By the doors above no
> other root is pending; no deeper chain exists: a pending child's δ̂ is
> non-compensable — for checked programs by SI-3 *at the child's own
> [Intend]* (δ̂ is recorded, so this is a fact of σ, stable under any
> later redeployment *(rev)*), for proxied runs by guard (vi) — so its
> dispatch is [Rec-Complete], which intends nothing. ∎
>
> *Theorem P at step n+1.* Only D-transitions change σ.
>
> **[Intend]** (with or without a handler — the write is the same
> *(rev)*): I2 — one transaction, fresh κ, dom(T) grows. I3a by enc
> (Cap-as-slot everywhere), I3b by R, I3c by ℓ. I1 — by Lemma 2 at this
> step: parent-⊥ from empty pend (depth 1); parent-j₀ from pend = [j₀]
> (depth 2), the child being δ̂(j₀)'s named compensation by [Comp]'s
> construction — I1's clause reads δ̂, a field of σ. An [Intend] from
> inside a window — the P-1 counterexample — is excluded by SI-1/guard
> (i); a compensable-chain third level by SI-3/guard (vi).
>
> **[Commit].** In-place P → D. **[Quarantine].** One mark; J, blobs,
> pend untouched. **[CommitFinal].** F only. **V/[Crash]/[Restart].**
> σ untouched — and I1's clause now genuinely survives [Restart], because
> it mentions only σ's own fields *(rev: with program-resolved δ it did
> not)*. ∎

> **Lemma 3 (machine-local conformance).** (i) Completion of a Done entry
> returns the stored r, no D-transition (R-1's repair; regression-pinned).
> (ii) Completion of a Pending entry performs, per attempt, one clause
> re-execution and at most one [Commit] on that entry; a post-crash repeat
> lands in (i) if the [Commit] landed, else repeats (ii). At the store,
> completing j n ≥ 1 times ≈ once: one committed write, trajectory P → D.
> At the world, each (ii) attempt is a real re-invocation bounded only by
> the declared equation — §8.3's conditional obligation (the
> crash-mid-retry bare re-delivery recorded open as F-012 is exactly a
> repeated (ii) attempt). The
> driver's reads — frontier, load_result, comp_arg — are pure;
> materialisation is not a read and may write a quarantine mark. ∎

> **Lemma 5 (no committed D-transition repeats)** *(rev: factored out of
> the progress theorem so that Lemma 4 does not import its hypotheses —
> this lemma is unconditional)*: in any admissible run, no D-transition
> re-performs a committed one: [Commit] on a Done entry never fires
> (Lemma 3(i) precedes it), every [Intend] is fresh (j, κ freshness),
> quarantine marks are per-cell one-way, F is write-once — all by the §4
> closure claim. ∎

### 6 · Theorem 1, derived

> **Theorem 1a (selection determinacy).** For every σ reachable in an
> admissible run, exactly one recovery dispatch is enabled at
> ⟨crash | σ⟩, computable from σ alone — concretely from F, frontier(σ),
> and the frontier's recorded δ̂ *(rev: with F-011 this is literally σ;
> and [Rec-Final] reads F, so "σ alone" now covers all four dispatches)*.
> No selection consults state beyond σ; no crash point admits two
> candidate dispatches.
>
> *Proof.* INV by Theorem P. If F ≠ ⊥: [Rec-Final], unique (no other
> dispatch reads F = ⊥ states). Else by Lemma 1 the frontier is ⊥, P, or
> D — [Rec-Empty], ([Rec-Complete] | [Comp] by δ̂, a total disjoint split
> of recorded classes; every entry carries one, since [Intend] refuses
> undeclared ops), or [Rec-Done]. Whether the selected action *executes*
> is a separate, conditional matter (1b); where the program in force
> disagrees with δ̂, the selected action is a refusal, still unique. ∎

> **Theorem 1b (recovery progress).** Hypotheses: admissible run;
> recovery-enabling preconditions hold where cited; **finitely many
> crashes**; and **re-executed clause bodies and their foreign
> applications terminate** *(rev: nothing in SI-1 bounds the V-prefix —
> the language has general recursion and a foreign call may hang; the
> draft's conclusion was unreachable without this hypothesis)*. Then:
> each crash-free recovery pass either refuses (σ unchanged — or advanced
> by exactly one sticky quarantine mark *(rev)* — and re-runnable) or
> reaches program re-entry / [Rec-Final] after a bounded prefix: at most
> one child [Intend], one child [Commit], and one parent [Commit];
> and across crashes σ progresses monotonically — J append-only with **at
> most one child ever created per parent** (once the child exists, it and
> not the parent is the frontier, forever: entries are never removed and
> [Comp] dispatches only on the frontier — a frontier argument, not
> Lemma 2 *(rev)*), entry states move only P → D, q marks are one-way and
> bounded by dom(T) — so under the hypotheses recovery terminates, in
> completed re-entry or in a refusal awaiting intervention.
>
> *Proof.* Bounded prefix: from a P frontier, [Comp] = one child [Intend]
> + child window + child [Commit] + parent completion (≤ one [Commit],
> Lemma 3(ii)); [Rec-Complete] = one materialise + window + [Commit];
> from a D frontier, [Rec-Done] = materialise + resume (V) and, for a
> Done child with pending parent, the driver frame's completion — one
> [Commit] (3(ii)) or none (3(i)). Crash cases, exhaustively: in the
> child's window → [P(parent), P(child)], frontier = child, dispatch
> [Rec-Complete] by the child's recorded non-compensable δ̂, no new
> entries; after the child's [Commit], before the parent's — the state
> [P, D] — frontier stays the Done child, [Rec-Done] + structural clause
> lead to the parent's completion (3(ii), or 3(i) if a prior attempt's
> parent-[Commit] landed); the loop [P, D] → crash → [P, D] creates
> nothing and exits at the parent's [Commit] in any crash-free pass;
> after the parent's [Commit], before its [Resume] — [D, D], R-1's state
> — [Rec-Done] + 3(i), V-only; during any materialise → nothing durable,
> or one sticky q then refusal (monotone) *(rev)*; after a Done root's
> resume, before the next [Intend] → the repeated segment is V-only
> (§7.1's no-replay reading); after [CommitFinal], before observation →
> [Rec-Final], V-only *(rev)*. No committed D-transition repeats —
> Lemma 5. ∎

> **Lemma 4 (per-run affinity, store face).** Within one run: each κid
> materialised at most once (L), each materialised continuation resumed
> at most once (consumed flag); lifts to persisted identities by §6.3's
> identity-preserving encoding. Across runs: every D-transition a
> recovery run performs is a first commit of a previously-Pending entry,
> a fresh [Intend], a first quarantine mark, or the single F write — by
> Lemma 5, which is unconditional *(rev: the draft cited the progress
> theorem and thereby imported hypotheses this lemma does not need)* —
> so re-resumption of a Done frontier after a crash repeats only
> V-transitions. ∎

> **Theorem 2 (authority at rest, formal half).** For every σ reachable
> in an admissible run, no blob or field of σ contains capability
> material: T blobs, J argument and result blobs, and F all pass through
> enc, which encodes *any* Cap value — in a Handle frame or first-class
> in an environment — as an effect-name slot and has no other arm for it;
> witnesses ι are plain strings; [Quarantine] writes no blob. Induction
> on the run. ∎ The step from "no capability material at rest" to the
> paper's "confers no authority" needs one closure fact, stated rather
> than implicit *(rev)*: **no rule of §4 derives authority from σ's
> content** — G is extended only by handler installation and the restart
> prelude's own grants, [Materialise] re-binds slots against G but never
> extends it, and foreign invokers re-bind from the program in force —
> verified by inspection of every rule above. The programming-model half
> is [Materialise]'s definition: slots re-bind against G in force now,
> E014 if absent.

---

## Part B · The typed calculus (rules consolidated, standard parts sketched)

### 7 · Rules and lemmas

Rule set: T-Var, T-Unit/Int/Bool/Str, T-Lam, T-App, T-Let, T-If, T-Match,
T-Op (deliv + witness discharge), T-Handle, T-Resume (εk = ε),
T-Clause-Prompt (ε ∩ Susp = {}, tail, jump-typed tail resume),
T-Clause-Susp (ε = {}, tail, all-paths, foreign application only here and
not under λ), T-Return-Clause, T-Foreign-Decl (second-class names);
declaration conditions: every op carries δ; compensable(op′) requires
δ(op′) ∈ {idempotent, at_least_once}; suspension class per effect.
Γ = Γu ; Γa, affine use-at-most-once.

**Lemma B1 (weakening, unrestricted).** Standard; induction on the
derivation.

**Lemma B2 (substitution).** Standard linear treatment [Girard 87;
Wadler 90]; values carry empty rows. **Obligation for the full
write-out:** stability of the T-Clause side conditions (tail, all-paths,
no-foreign-under-λ) under substitution and partial reduction — this
transports SI-1 from clause text to executing clause bodies; B4 depends
on it.

**Lemma B3 (typing preservation under machine steps).** Known territory
for row-typed handlers [Leijen 17; Brachthäuser 20]; the v0.6-specific
cases are the jump-typed tail resume and the row-less foreign
application.

**Proposition B4 (SI-1..SI-3 for checked programs).** SI-1 from
T-Clause-Susp + no-λ, transported by B2's obligation and B3; SI-2 from
Kont affinity under B2/B3 plus §6.3; SI-3 from the declaration conditions
at check/validation. Proved modulo B2/B3's sketch status; every leg also
has its dynamic guard in §3.

### 8 · Status of this document's claims

| Claim | Status |
|---|---|
| Lemma 1; Lemma 2 + Theorem P (mutual induction); Lemma 3; Lemma 5; Theorem 1a; Theorem 2; Lemma 4 | **Proved here** over admissible runs of the §4 relation, assuming Rule 2 totality (demonstrated for the interpreter's domain), Assumption R (roundtrip; tested), Assumption D (driver shape; exercised) — and, **for the checked-unproxied branch of admissibility, Proposition B4** (itself modulo B2/B3's sketch status) *(rev: the draft's row omitted the B4 dependency)*. |
| Theorem 1b | **Proved here**, conditional on: recovery-enabling preconditions; finitely many crashes; terminating clause bodies and foreign calls. Refusals are no-transitions (at most one quarantine mark) and re-runnable. |
| End-to-end exactly-once at the world | **Not claimed**; conditional on §8.3's equations, strengthened compensable family included. |
| B1, B2, B3 | Sketched; B2's side-condition-stability obligation is the one non-standard piece. Full write-out or mechanization after Phase 11. |
| B4 | Proved modulo B2/B3; dynamic guards cover each leg independently. |
| Correspondence of §4 to the implementation | **Tested, not proved** (crash matrices incl. the R-1 row; quarantine/compat, P-series, and F-011 drift regressions). Known divergences, all recorded: the Runtime API does not itself invoke validation — guard (vi) is caller discipline today (A-11; T1.3); the budget debits before the Intend transaction (A-35; T2.7); [CommitFinal] divergence is silently ignored (A-42; T2.2); the mask does not cover the [Rec-Empty] door (**P-4**) and durable windows have no runtime tail guard (**P-5**) — both statically closed for checked programs, open dynamic proxies for unchecked ones. The theorems cover recovery via the *generated* driver; `recover_env` accepts arbitrary terms, which is embedder rope outside this document's scope. |

The gap most worth closing next is B2/B3 in full (or mechanized), after
Phase 11 fixes the final calculus.
