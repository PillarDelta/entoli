# EVALUATION.md — running record of findings and deviations

Every point where the implementation does something the calculus does not describe is a
finding for v0.6, recorded here (plan §3.1).

## PHASE 8 VERDICT: GO — with the v0.6 worklist below

Written 2026-08-27, after Phases 0–7 and an adversarial review (34 independent review/
verification agents against paper v0.5 and plan v1.0; confirmed findings R-1..R-9
below, all repaired or closed in-tree the same day; refuted claims not recorded).

**The implementation supports the v0.5 claims.** Both kill points passed. The three
falsification criteria hold: an unrecovered external effect fails to compile (E001 +
the AtLeastOnce row marker), an unserialisable capture cannot occur (closed encoding;
capabilities have no byte form), a sub-agent reaching an unhanded tool fails to compile
naming both the effect and the missing capability (demo B). Demonstrations A, A2, B,
and C pass, restarted from cold durable state under hard process death; recovery is
generated ENTOLI terms over exactly the machine-local class plan §8 predicted; demo D
returned 6/6 first-pass model writability with the falsification condition untriggered.

**The strongest evidence for the design is R-1**: the one genuine blocker found by
review was a *Theorem 1 counterexample in the implementation* — and its repair was
forced by the design's own vocabulary (the machine's privileged completion effect had
to honor its own declared idempotency equation). The framework localized its own bug.

**What GO does not mean** (paper §14 discipline): end-to-end exactly-once remains
conditional on delivery contracts (harness-tested, not proved); serialisable
continuations are demonstrated for this value domain, not proved feasible in general;
the metatheory (preservation of INV(σ) over a full transition relation) remains to be
written (plan Phase 10); and per paper §16, success is still compatible with the
Phase 11 answer being "extension or library" rather than a standalone language.

## v0.6 worklist (semantic findings that need paper text)

F-002, F-003, F-007, R-1's table row, R-2's privilege scope, R-4's aliasing clause,
R-5's tombstone clause, plus unification of suspending-effect discharge with T-Handle
(F-004/R-7) and row polymorphism for the checker (F-005).

**STATUS 2026-08-28: the paper text is WRITTEN — `docs/paper-v0.6.md`.** Every item
above has its disposition in the text (F-005 as recorded implementation status; row
polymorphism itself remains future checker work). The text was reviewed before commit:
a revision-note coverage pass (verdict: complete), an implementation-consistency pass
(7 findings, all applied), and a 15-agent adversarial workflow (5 lenses — calculus,
fidelity, overclaim, consistency, voice; 38 raw findings, 35 after dedup, top 10
adversarially verified, 7 confirmed; 25 minor pass-throughs triaged by hand). All
confirmed findings repaired in-text the same day; three have implementation
consequences, recorded as P-1..P-3 below.

## Paper-review findings (P-series, 2026-08-28 — from the v0.6 text's own review)

- **P-1 (soundness of the clause condition as implemented).** Expressing §4.3's side
  condition by row subtraction (`ε \ εk`) is unsound: ambient effects that also occur
  in εk are erased from the clause body's own row, so a suspending clause performing a
  suspending S ∈ εk inside its crash window typechecks — breaking the two-exit window,
  §8.1's classification, and I1's depth bound (nestable). Paper v0.6 corrects the rule
  (the tail resume is typed as a jump at row {}; suspending clauses require ε = {},
  prompt clauses ε ∩ Susp = {}) and corrects εk to ε (deep capture discharges E inside
  k). The checker still implements the subtraction (`own_row.minus(&ek)`,
  `crates/entoli-check/src/lib.rs`) and types Kont rows as body ∪ {E}. Dynamic proxy
  already refuses the counterexample at runtime (machine-level E005: an operation
  inside the window would push a second pending entry — `machine.rs`). **CLOSED
  same day:** the clause kont is bound at row {} (the tail resume typed as a jump —
  sound because E004 already refuses every non-tail resume), the subtraction is
  deleted, and both directions are regression-tested in
  `crates/entoli-check/tests/unified.rs` (suspending clause re-performing its own
  effect; prompt clause suspending via an ambient effect). Kont rows elsewhere remain
  the v0.5 over-approximation (ε ∪ {E}) — sound, imprecise; full εk = ε alignment
  stays open.
- **P-2 (foreign invokers must be second-class).** The unification design made foreign
  invokers first-class values; `resume k (λy. f y)` then defers world contact past the
  crash window, invisibly to the row system. Paper v0.6 makes foreign names
  second-class (not values; application form only; forbidden under λ inside the
  clause). The implementation is already name-based (`Term::ForeignApp`, no foreign
  Value variant), and the runtime refuses a `ForeignCall` that reaches the top-level
  drive loop (dynamic proxy); but the checker's lexical `in_suspending_clause` flag
  still admitted λ-deferred foreign applications statically. **CLOSED same day:** a
  lambda body clears the flag (deferred execution is not clause execution);
  regression in `crates/entoli-check/tests/unified.rs`.
- **P-3 (compensable contract understated).** §8.3's v0.5 equation `op ; op' ; op ≈ op`
  does not cover the machine's own reachable schedules: the ambiguous Pending case
  realizes `op' ; op` against a possibly-clean state, and a crash during the retry's
  window re-enters `[Pending(op), Done(op')]` and re-invokes op with no fresh
  compensation (`op ; op' ; op ; op`). Paper v0.6 requires
  `op^a ; op' ; op^b ≈ op` for a ∈ {0,1}, b ≥ 1. **CLOSED same day:** the harness
  (`entoli-testkit/src/conformance.rs`) exercises (a,b) = (1,1), (0,1), (1,2); all
  demonstration fixtures pass the full family, and a negative fixture
  (`OverzealousCompensation` — passes the v0.5 equation, misbehaves against a clean
  state) is caught by the a = 0 instance (`tests/contracts.rs`). Still open: whether
  a compensable op that cannot tolerate bare re-invocation should be *required* to
  carry an idempotency witness for the retry path (a checker rule, not a harness
  rule).

## Deviations

- **D-001** (Phase 0): OCaml oracle skipped, no OCaml toolchain present. See ADR-0001.
- **D-002** (Phase 0): `entoli-types`/`entoli-check` crates deferred to Phase 5.

## Findings

- **F-001** (Phase 4, plan §8 open question): the machine-local effect class needed by
  the recovery driver is exactly: pure reads of σ (`frontier`, `load_kont`,
  `load_result`, `comp_arg`) plus `Protocol.complete(j)` — invocation and atomic
  Done-commit of an *already-intended* entry. Nothing beyond pure reads and the store's
  atomic primitives was required; the driver performs compensations as ordinary
  journalled effects. Rule 3 holds in the qualified form the plan predicted:
  "an ordinary program plus one privileged capability." Watch condition NOT triggered.
- **F-002** (Phase 4): §8.1's "Before 1 / no entry" recovery row needs a degenerate
  reading at program start: with an empty journal the "last committed continuation" is
  the program entry point itself. v0.6 should state this explicitly.
- **F-003** (Phase 4): recovery of a crash after step 4 (`Done` committed, process died
  before the next Intend) re-runs the pure segment following the resume. This is the
  paper's own recovery action, but v0.6 should say explicitly that the no-replay claim
  is about never *re-deriving a continuation*, not about pure code after a resumed
  Done never re-executing — the distinction in ADR-0001 §8.

- **F-004** (Phase 3–6): in v1 the runtime authorizes suspending effects by external
  registration; grant-gating (live grant required + budget spend per invocation) is an
  opt-in mode (`require_grants`), used by demonstration C. Full unification of
  T-Handle-style capability discipline with suspending-effect invocation is v0.6 work.
- **F-005** (Phase 5): the checker is bidirectional-lite — declared signatures at
  definition boundaries, no row polymorphism, unannotated lambda parameters default to
  `Data`, constructor discipline stays dynamic. Sufficient for the falsification
  criterion's three clauses; full inference is deferred.

- **F-006** (Phase 7): the `migrate E from compat A to B { ... }` adapter hook (§10.2,
  optional in the paper) is not implemented; a quarantined continuation currently stays
  quarantined until manual intervention. The quarantine itself, and both mismatch
  detections (E012 computed, E013 declared), are implemented and tested.
- **F-007** (Phase 7): handled(K) as recorded at Intend includes the suspended
  operation's own effect even when the remaining frames no longer reference it —
  recovery re-invokes that operation, so the §10.2 merchant case applies to it first.
  v0.6 should state this explicitly in the persist(K) definition.

## Production track (opened 2026-08-28)

At the user's direction the goal is now a production-ready system. A four-lens
audit (43 findings, A-01..A-43) is recorded in `production-audit-2026-08-28.md`;
the working plan is `production-roadmap.md`. Two audit findings are semantic
deviations from the paper and are recorded here per plan §3.1:

- **F-008 (from A-10): the signature hash omits operation types.** Paper §10.1
  defines `sig(h) = hash({op ↦ (τ1, τ2, δ)})`; `effect_sig` in
  `entoli-encode/src/hash.rs` hashes op names, suspension class, and delivery
  classes only (its own comment says types would "arrive in Phase 5" — Phase 5
  shipped and the hash was never extended). Consequence: a deployment changing
  only an op's argument/result type resumes a stale continuation with no
  E012/quarantine — the §10.2 drift class the mechanism claims to catch. Note:
  extending the hash is a store-format change (all previously persisted sig
  hashes change); no migration tooling exists yet (roadmap T5.4). **CLOSED same
  day:** `effect_sig` takes the program's `Signatures` and canonically encodes
  each op's argument/result types (structural encoding, deliberately not
  Display); `Codec` and `Runtime` carry the signatures. Regression:
  `effect_type_only_drift_is_detected_at_decode` (entoli-encode
  tests/roundtrip.rs) — type-only change → EffectDrift; identical types resume.
  An undeclared signature hashes as the `Data -> Data` default, so types
  appearing later is drift too (consistent with R-6's absence rule).
- **F-009 (from A-27): strict mode is narrower than declared on the recovery
  path.** `strict_contracts` gates fresh invocations in `protocol()` only;
  recovery re-invocation via clause re-execution (`Protocol.complete`) performs
  the external contact with no contract check, so an unproven contract can still
  reach the world during recovery. The refusal must cover both invocation sites;
  a refusal at recovery leaves the entry Pending (recoverable once the contract
  is proven), never a wedge. **CLOSED same day:** the strict gate now runs in the
  recovery re-invocation path before any budget spend. Regression:
  `strict_mode_gates_recovery_reinvocation_too` (entoli-testkit
  tests/contracts.rs) — a failing remote leaves Pending; strict recovery refuses
  naming strict mode with the entry still pending; recovery completes once the
  harness passes are fed in.
- **F-010 (found closing F-009): paper v0.6 §7.1 misstated budget spend on
  recovery.** The §7.1 sentence added during the v0.6 review said re-execution of
  an already-intended entry does not spend again; the implementation spends per
  clause execution — recovery re-invocation included — and demo C's budget
  semantics depend on it (`budgeted_installation_spends_per_invocation`). The
  implementation is right: the budget bounds contacts with the world, not journal
  entries — under per-entry spend a crash-looping operation could reach the world
  unboundedly on one spend. Paper §4.2/§7.1/§11 corrected same day; an exhausted
  budget at recovery refuses with the entry left Pending, recoverable after
  re-grant (§9.1).

- **F-011 (from the Phase 10 metatheory review, 2026-08-28): recovery dispatch was
  not a function of σ.** `frontier` resolved a pending entry's delivery class
  against the *program in force at recovery*: a redeployment changing a pending
  op's δ silently changed what recovery did (re-invoke vs compensate), and a
  compensable-chain redeployment could push pending depth past I1's bound before
  any drift guard ran — the metatheory's INV(σ) was not even well-defined over σ
  alone. The paper's Theorem 1 ("computable from σ and the delivery class of the
  operation named at the frontier") was true only under a program-stability
  assumption nothing stated. **CLOSED same day:** the delivery class in force at
  Intend is recorded in the journal entry (canonical string, store-format change
  like F-008); `frontier` dispatches on the record and refuses (E012-labelled,
  recoverable, entry left Pending) when the program in force disagrees — checked
  before any compensation could intend a child; an op with no declared class can
  no longer be journalled; the store invariant checker now verifies I1's
  compensation clause from σ alone (child = parent's recorded compensation).
  Regression: `recovery_drift.rs` (drift refuses; unchanged program recovers
  through the recorded dispatch). Paper §7.1/§8.1 updated.

- **P-4 (metatheory round 3, hand-verified): the machine-local dispatch mask does
  not cover the empty-journal recovery door.** The mask (driver_active + the R-2
  root-boundary rule) refuses Local/Protocol ops past a root-continuation
  ResumeBoundary — but the generated driver's `.none` arm runs the program entry
  *directly*, interposing no boundary, so in the [Rec-Empty] door an UNCHECKED
  program's entry runs with machine-local reach (read σ, materialise
  continuations). Closed statically for checked programs (Cap[Local] absent from
  program typing contexts). **Worklist:** candidate repair sketched in the
  metatheory — materialise the entry point AS a continuation (F-002's degenerate
  reading made literal: a machine-local read returning the entry as a root
  continuation, so the door crosses a root boundary like every other) + regression.
- **P-5 (metatheory round 3, hand-verified): durable windows have no runtime
  tail-position guard.** The clause-boundary E004 check exists only on the prompt
  path; in `execute_clause` a non-tail resume of the window sentinel is not
  refused — the sentinel is consumed, the rest of the body runs, and the
  POST-RESUME value is what gets committed at step 4 (wrong value, and work after
  the commit point). Closed statically for checked programs (E004). **Worklist:**
  give the durable window the same boundary discipline as the prompt path (or
  refuse any machine step after sentinel consumption other than returning it) +
  regression.
- **Metatheory note:** the metatheory's I3 deviates from paper §7.2's I3 — the
  paper asserts Repr/Reviv/Perm hold of every persisted value, established
  statically; the metatheory weakens Repr/Reviv to the tested codec-roundtrip
  assumption and Perm to the recorded classification plus a deployment
  obligation. Stated in metatheory §2; recorded here per plan §3.1.

## Adversarial authorship study + performance measurements (2026-11-08)

Run for the academic paper (`paper/entoli.tex`); corpus in
`docs/convergence-study-2026-11-08/`, data in `docs/bench-2026-11-08.csv`.

- **Authorship study** (removes demo D's author-calibration bias): a model agent
  authored 12 adversarial tasks from the reference card alone; 36 closed-book
  solvers (12 × {Haiku, Sonnet, Opus}) wrote programs; failures got one round of
  verbatim compiler diagnostics. First pass 14/36 (H 5, S 4, O 5); +6 after one
  round → 20/36. **Zero E003/E004/E005 violations across all 58 generations** —
  every failure was a v1 surface-grammar gap (parenthesised kont types,
  parenthesised resume arguments, nested handle, if-precedence: 17 parse errors)
  or the string-equality gap (5 type errors; the surface language has no Str
  comparison — Tier 1 worklist item). Caveats recorded in the paper: metric is
  compilation not functional correctness (≥3 round-2 programs gamed semantics);
  single vendor, three tiers.
- **Performance** (min of 3, release): per-effect cost 2.1→20.5 ms as the journal
  grows 25→400 entries (the per-step whole-journal audit + id-allocation scans,
  A-06/A-13 — retirement licensed by the Phase 10 preservation theorem); encoded
  continuation size 1.4 KB→333 KB as captured depth grows 10→200 (env encoding
  without sharing, A-18). Both mechanisms known, repairs scoped (T3.1/T3.2).

- **F-012 (external review of the paper, 2026-11-08): the strengthened
  compensable equation family entailed idempotence.** With ≈ compositional,
  `c;o ≈ o` (prefix by o) gives `o;c;o ≈ o;o`, and with `o;c;o ≈ o` this yields
  `o;o ≈ o` — the family excluded exactly the non-idempotent operations
  compensation exists for. **Repair (paper text, same day):** the obligations
  are state-indexed, over world states rather than free equations — c(W) = W
  for every W in which o is undelivered, and c(o(W)) = W for every W — which
  non-idempotent operations satisfy (o = reserve-one, c = release-one-if-held)
  and which, under the protocol's guarantee that c runs only in the ambiguous
  window, make every compensate-then-retry schedule ≈ one delivery. **Residual
  machine gap, OPEN:** crash-mid-retry re-enters [P, D] and re-delivers o bare,
  so the current dispatch additionally requires a compensable op to tolerate
  re-delivery on the retry path (witness or idempotence). Candidate repairs —
  fresh compensation per retry cycle (tension with Rule 3's driver-term purity
  and the one-child-per-parent bound), or per-attempt witnesses (needs a
  durable attempt epoch) — are design work for their own review cycle. No
  theorem depends on resolving it; the harness's (1,2) schedule probes exactly
  this gap. Also from the same review, adopted in the paper: round-separated
  study accounting; the zero-violation claim restated over programs that reach
  semantic checking (post-repair: 56/58 machine-checked + 2 hand-inspected,
  still zero E003/E004/E005); "pure from outside" reworded to requirement-level
  encapsulation; divergences individually characterised; novelty narrowed
  (Temporal's sandbox acknowledged); a complete worked program added.

## Language v1 closed (2026-11-09)

P-4 CLOSED (the `none` dispatch drops machine-local privilege — the empty-journal
door is masked; regression `hardening.rs`). P-5 CLOSED (resuming the window
sentinel with a non-empty sub-machine stack is the non-tail shape: refused as
E004 before Commit, entry stays pending; a tail resume halts the window with a
distinguished `WindowReturn` outcome; regression `hardening.rs`). A-30 CLOSED
(`compile` refuses ablation flags; `compile-ablated` is the measurement-only
path). A-31 CLOSED (declared foreign without a signature is a diagnostic).
`>` comparison added (parses as flipped `<`; no new primitive). Deliberately
remaining open, each recorded: F-005 row polymorphism (deferred by design),
F-006 migrate adapter hook (operator feature, T4.6), F-012 compensable
retry-path dispatch (its own design cycle). Suite 87.

## Tier 1 (2026-11-08): the language runs from source

Built directly from the authorship study's failure list. (a) The study's
dominant failure class was ONE parser bug: `resume k (expr)` greedily parsed
the parenthesis as a call of k and swallowed the following keyword as a
variable — resume/handle operands are now atoms, keywords are refused as
expressions. (b) A-28 closed: a bare Capitalised identifier in pattern
position is a nullary constructor pattern, never a wildcard. (c) `str_eq`
surfaces PrimOp::StrEq (the card documented no string comparison). (d) T1.1:
`foreign f : T1 ~> T2` declarations + application rewriting (second-class,
checker rules unchanged). (e) T1.3: `compile` runs `Program::validate` and,
when `main` exists, the whole-program gate — every row in main's type empty
(E001). (f) T1.2: `run`/`recover <file.ent>` with `--grant` (caps as main
parameters in grant order), `--bind f=KIND:E.op` fixtures, `--crash`;
recovery re-installs the program's own `wrap` prelude around the generated
driver. Milestone test (`tier1.rs`): a surface program with foreign
declarations killed by hard process death inside the crash window (remote
committed) recovers from the same source file with exactly one ledger entry.
Retro-validation: re-compiling the study corpus, +16 of the 22 residual
failures now pass unchanged; the rest used Int `==` on strings and would
need the now-existing `str_eq`. Suite 83.

## Unification migration (M1–M7, 2026-08-27 — COMPLETE)

The suspending-effect/capability unification (docs/unification-design.md) is
implemented: suspending effects are discharged by installed handlers whose clauses the
runtime executes as the crash window (sentinel-continuation mechanism; all-paths-resume
enforced dynamically and statically), world contact is `ForeignApp` over declared
foreign bindings, capability liveness is checked and budgets spent at Intend, recovery
re-executes the clause found in the driver's own prelude, and the transitional
registry (plus `require_grants`) is deleted. F-004 is thereby RESOLVED. Every demo
(A, A2, B, C, compat, R-regressions) runs on the unified path; demo B's containment
now covers suspending tools (E001+E002 both named, test `unified.rs`); demo C's
revocation is "the restart entry point cannot install what it holds no capability
for," with E014 surfacing from the persisted continuation's slot. Suite: 72 tests.

Residual notes for v0.6 text: (a) machine-level perform stays permissive (suspends
handlerless); the E001 rule lives at the runtime boundary; (b) a budget refused at
Intend leaves no journal entry (spend precedes the write); (c) prompt-effect
installations in a recovery prelude are resume-time authority by §9.1 — the
machine-local dispatch mask (Cap[Local]'s dynamic proxy) is the one non-positional
dispatch rule.

## Adversarial review findings (R-series, 2026-08-27 — all closed)

- **R-1 (BLOCKER, fixed + regression `review_regressions.rs`):** a crash after the
  retried operation's Commit during a compensable recovery left σ permanently
  unrecoverable — frontier was the Done child, whose continuation retried
  `Protocol.complete(parent)`, which refused non-pending entries. The machine's own
  privileged effect violated its declared idempotent equation. Fix: complete of a Done
  entry returns the stored result (`op ; op ≈ op`). **v0.6 must add this row to §8.1's
  derived sequence: [Done(op), Done(op')] → complete(op) reads Done, resume.** This was
  a genuine Theorem 1 counterexample in the implementation, found by review, one crash
  deeper than the A2 matrix tested. Phase 4's kill point stands as PASSED *after this
  repair*.
- **R-2 (fixed):** `driver_active` spanned the whole recovery run, leaking the
  machine-local class to resumed user code (double-resume of persisted continuations
  demonstrated pre-fix). Fix: machine-local effects are refused once control passes a
  root-continuation ResumeBoundary; recovery-action (child) continuations retain the
  privilege. **This sharpens Rule 3's "one privileged capability": the capability's
  scope is the driver's own frames, and v0.6 should say so.**
- **R-3 (fixed):** each persisted continuation may be materialised at most once per
  process run (`loaded_konts`) — decode minted fresh `consumed=false` cells, making
  affinity per-copy.
- **R-4 (fixed):** encode flattened Rc-sharing of continuations: one aliased kont
  became two independently resumable copies after a cold resume. Fix: encode-side
  sharing table with back-references; aliases decode to one cell. **v0.6: the affine
  treatment must state that persistence preserves continuation identity, not just
  count.**
- **R-5 (fixed):** a consumed continuation in a captured environment made `encode`
  partial over reachable machine states (Rule 2 totality gap — the suspension could
  never persist). Fix: consumed konts persist as tombstones whose only future is E003.
- **R-6 (fixed):** the encoded header silently dropped names unknown at encode time;
  now recorded with a zero sentinel and verified as "still absent" at decode.
- **R-7 (fixed in checker):** suspending operations never consult in-language Handle
  frames (runtime registry discharges them, F-004); a handler for a suspending effect
  would silently never run. The checker now rejects such handlers, so machine and
  checker agree.
- **R-8/R-9 (test gaps, closed):** the missing A2 crash row (R-1's chain) and §4.3's
  permitted half (suspension by the resumed computation succeeds) are now tested.

## Demonstration D — can a model write ENTOLI? (Phase 5, run 2026-08-27)

Setup per plan §10.5: six fresh-context LLM instances,
each given ONLY the one-page reference card (docs/demo-d-spec.md: syntax, typing rules,
the three restrictions, two worked examples) and one task statement. No tool use, no
access to the implementation. Tasks: refund flow with witnesses, compensable booking,
whole-result transformation (deliberately tempting a non-tail resume), conditional
approval guard, abort-without-resume, continuation-typed helper delegation.

**Results: first-pass compile rate 6/6. Iterations to green: 0 for every task.**

- The E004 temptation (task 3, "multiply the overall result by 10") was correctly
  answered with a `return` clause, not a non-tail resume — the reference card's
  "per-operation in the clause / whole-result in return" framing appears to carry.
- The continuation type annotation `(Int -[Ask]-o Int)` was produced correctly (task 6).
- Compensable + at-least-once declarations and witness derivations were all correct.

**Ablation** (plan §10.5): with zero failures there is no failure mass to attribute, so
the inverse was verified instead — four deliberately-wrong programs each fail with
exactly their restriction's diagnostic (E003/E004/E005/unwitnessed-ALO), and each
compiles under precisely its own `--ablate` flag (demo-d/attempts/ablate_*.ent). The
per-restriction measurement machinery is in place for a harder task set.

**Caveats, stated plainly:** n=6, one model family, tasks authored by the checker's
author, difficulty calibrated to the worked examples. This is a strong first signal —
the restrictions did not impede first-pass generation at this task size — not the
convergence study; a harder, adversarially-chosen task set with an independent model
should repeat this before the Phase 11 gate leans on it. Falsification condition
(no convergence / one restriction dominating) was NOT triggered.

## Phase exit evidence

- Phase 1: `cargo test -p entoli-core` — handler intercepts, inspects, tail-resumes
  exactly once; double resume is E003; non-tail resume is E004; nested suspension E005.
- Phase 2 (KILL POINT): `tests/phase2_cold_resume.rs` — a suspended computation is
  encoded, the process is destroyed (`exit(87)`), and a fresh process decodes and
  resumes it to the correct result, with no external effect involved. PASSED.
- Phase 2 addendum (recorded 2026-08-28, during Phase 9 — the evidence existed but was
  never entered here): drift detection is dependency-precise in both directions —
  `def_drift_is_detected_at_decode` (a referenced definition's change strands the
  continuation) and `unreferenced_def_change_does_not_strand_the_continuation`, both in
  `crates/entoli-encode/tests/roundtrip.rs`; SCC-grouped hashing for mutual recursion
  in `crates/entoli-encode/src/hash.rs`. Cited by paper v0.6 §10.2.
- Phase 3: Intend is one transaction (Pending + continuation); induced commit failure
  leaves σ unchanged; invariants I1–I3 hold after every durable transition.
- Phase 4 (KILL POINT): demonstrations A (A1–A4) and A2 pass — exactly one ledger entry
  across all crash runs, pending depth never exceeds 2, recovery driver is ENTOLI terms,
  no privileged second recovery machine. PASSED.
- Phase 5: `cargo test -p entoli-check` — demonstration B fails at compile time naming
  both the escaping effect (E001, Fs) and the missing capability (E002, Cap[Fs]);
  static E003/E004/E005; witness obligation; surface syntax parses, checks, and
  evaluates; demonstration D above.
- Phase 7: `tests/demo_compat.rs` — a compat-token bump on a slept-through effect
  refuses resume with E013, quarantines durably (sticky, nothing lost, invariants
  hold); matching tokens resume normally. `tests/contracts.rs` — all five declared
  fixture contracts pass their §8.3 equations; a non-deduplicating service claiming
  at-least-once is caught; strict mode refuses unproven contracts and admits proven
  ones.
- Phase 6: `tests/demo_c.rs` — demonstration C passes: no capability/budget material in
  the persisted continuation (byte-level check), derived data classification recorded
  (confidential — the §9.2 correction respected), revoked grant refuses resume with
  E014 naming Infer, re-grant binds the resumed agent to the smaller budget, and
  durable Done entries mean re-granting never re-runs completed inferences.
