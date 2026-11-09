# ENTOLI production roadmap

Adopted 2026-08-28, at the user's direction: the goal is a production-ready system.
Evidence base: `production-audit-2026-08-28.md` (43 file-anchored findings, A-01..A-43).
Governing framing: Phases 0–9 built a *reference interpreter* to answer "do the
semantics hold?" — it answered yes. Nothing in the audit contradicts the semantics;
everything below is engineering distance. The paper remains the semantic source of
truth; any fix that deviates from it is a finding first (EVALUATION.md), a patch
second.

Progress convention: check items off here; every closed audit finding cites its
A-number in the commit message. Phase-boundary reports continue as before.

---

## Tier 0 — decisions before code

- [x] **T0.1 Phase 10 — metatheory. DONE** (`docs/metatheory-v0.6.md`): full
      transition relation, INV(σ) preservation by mutual induction with the
      window-discipline lemma, machine-local conformance lemma, Theorem 1 split
      into unconditional selection determinacy + conditional progress. Three
      adversarial review rounds (16 agents each); the review also surfaced and
      closed F-011 (recorded dispatch) and recorded P-4/P-5 (dynamic-proxy gaps
      for unchecked programs — see EVALUATION; worklist below). Production
      payoff: the proof licenses retiring the per-step whole-journal audit
      (A-06/A-13) from release builds (T3.1).
- [x] **T0.2 Phase 11 — the decision gate. DONE**
      (`decisions/ADR-0002-phase-11-direction.md`): **Option A — standalone
      core language, deliberately small**; orchestration core only, `foreign`
      bindings as the host boundary, interchange reading as the product thesis.
      B fails its own choose-if clause (no host enforces Repr/Reviv over its
      closures; the Phase 2 kill point passed *because* the value domain is
      closed), C collapses into A with extra packaging. Adopted provisionally
      with two reversal triggers (convergence-study failure; serialisation
      breaking on realistic value domains → revisit C). Tiers 3–5 now read
      under A.
- [x] **T0.3 DONE (2026-11-09): approval-gated brief writer** (pilot/brief.ent, docs/pilot-runbook.md). One real orchestration: an
      approval/settlement flow with a real LLM inference binding and one real
      side-effecting service. All tiers below are prioritized by what the pilot
      needs. *(Needs the user: which workload.)*

## Tier 1 — milestone "runs a real program" (blockers to any use)

- [x] **T1.1 DONE (2026-11-08):** `foreign`
      declarations with signatures, suspending-handler clauses, so world contact
      is reachable from `.ent` source (A-08; today builders-only).
- [x] **T1.2 DONE (2026-11-08):** execute and
      recover a user program from source against a store — prelude derived from
      the program's own installations, not hand-written Rust (A-01, A-02).
- [x] **T1.3 DONE for main-gate + validate in CLI paths (2026-11-08; --ablate still ships, A-30 open):** `compile` runs
      `check_entry` (whole-program row discharge — the falsification criterion's
      first clause), the runtime calls `Program::validate` + the checker on
      load, and the `--ablate` flags leave the production path (A-09, A-11,
      A-30).
- [x] **T1.4 grammar half DONE (2026-11-08: resume-operand bug, keyword guard, A-28 nullary patterns, str_eq, paren types; F-005 row polymorphism and A-31 foreign-sig diagnostics remain):** nullary-constructor
      patterns stop parsing as wildcards (A-28 — silent wrong control flow for
      model-written code); foreign names missing signatures are diagnostics, not
      silent `Data -> Data` (A-31); witness-on-retry-path rule (P-3 residue);
      row polymorphism (F-005) and full εk = ε alignment as calculus follow-ups.

## Tier 2 — robustness (the store must survive hostile bytes)

- [ ] **T2.1 Codec hardening:** depth-limited iterative decode/encode, length
      caps before allocation — corruption becomes `DecodeError`, never SIGSEGV
      or OOM abort (A-07, A-22); iterative Drop for deep Env/Value chains (A-36).
- [ ] **T2.2 Store integrity is loud:** `decode_handled` errors on malformed
      bytes instead of silently truncating the drift gate (A-21/A-29);
      `row_to_entry` reports unknown states instead of normalizing to Done
      (A-34); `commit_final` detects divergent second results (A-38 vicinity).
- [x] **T2.3 F-008 closed: sig hash covers operation types** — paper §10.1 says
      sig(h) = hash of (τ1, τ2, δ) per op; the implementation hashes names and
      classes only (A-10). Paper-deviation repair, with a type-only-drift E012
      regression. *(Closed same day — see EVALUATION.)*
- [x] **T2.4 F-009 closed: strict mode covers the recovery path** — re-invocation
      via clause re-execution must respect strict contracts as `protocol()` does
      (A-27). *(Closed same day — see EVALUATION.)*
- [x] **T2.4b P-4/P-5 DONE (2026-11-09)** (from the metatheory review): mask the
      empty-journal recovery door; give durable windows the prompt path's
      tail-guard. Both statically closed for checked programs; unchecked-program
      hardening.
- [ ] **T2.5 Single-writer discipline:** cross-process store exclusion
      (lease/EXCLUSIVE + busy_timeout) so two runtimes cannot both recover one
      store and double-fire external effects (A-05); a read-only inspection mode
      (A-19).
- [ ] **T2.6 Semantics of arithmetic fixed per build:** checked i64 ops
      surfacing as an EntoliError, identical in debug and release (A-04).
- [ ] **T2.7 Budget spend ordering:** spend inside/after the Intend transaction
      so a declined commit does not charge the budget (A-35).

## Tier 3 — performance (long-lived agents must not be O(N²) in their own history)

- [x] **T3.1 DONE (2026-11-09):** `alloc_journal_id`
      becomes a COUNT with indexes on (state), (parent_journal_id); the
      per-step invariant audit becomes COUNT/EXISTS checks, with the full
      blob-fetching audit behind a debug flag — retired from release builds when
      T0.1 lands (A-06, A-13, A-17).
- [x] **T3.2 env half DONE (2026-11-09; single-pass encode T3.2b remains):** back-references for Rc-shared Env
      chains and closure environments, as Konts already have — the dominant
      blob-size hazard (A-18); single-pass encode sharing its traversal with
      kont_refs/classify (A-24).
- [ ] **T3.3 Hashing once, not per effect:** Codec/def_hashes computed once per
      program load and reused across the run (A-23).
- [ ] **T3.4 Rc data plane:** Ctor args (or Value) behind Rc so variable
      reference and match-binding stop deep-cloning (A-25).
- [ ] **T3.5 Retention:** GC/compaction for Done entries and dead continuation
      snapshots once final_result is committed; retention policy configurable
      (A-14, A-26).
- [x] **T3.6 prepare_cached DONE (2026-11-09; narrow SELECTs folded into T3.1's COUNT paths).**

## Tier 4 — operations

- [ ] **T4.1 Operator CLI:** inspect journal/frontier, list + release
      quarantined continuations (release is currently impossible without raw
      SQL — A-03), view grants/budgets (A-32), resume-one, GC, backup/restore.
- [ ] **T4.2 Observability:** structured tracing for protocol steps, effect
      invocations, quarantine and budget events; metrics hooks (A-15).
- [ ] **T4.3 Bounded execution:** timeouts/cancellation for foreign calls;
      configurable fuel exposed through Runtime (A-16).
- [ ] **T4.4 Config surface + structured errors:** store path, fsync policy,
      compat token, limits via config; E-codes as typed variants, no Debug
      leakage; CLI panics become diagnostics + exit codes (A-12, A-20, A-37+).
- [ ] **T4.5 Data handling per §9.2:** encryption at rest keyed to the derived
      classification level(K); retention policy for confidential frames.
- [ ] **T4.6 Migrate adapter hook (F-006)** so quarantine has a supported exit
      besides discard.

## Tier 5 — productization

- [x] **T5.1 DONE (2026-11-09): llm (curl + witness cache), approve (durable human gate), http (witnessed POST):** LLM inference + one real HTTP service,
      replacing fixtures for the pilot (T0.3).
- [ ] **T5.2 Concurrency:** §12's commitments become semantics — fork/join,
      journal as partial order, per-branch I1, affine budget splitting. Own
      design + adversarial review cycle, like the unification.
- [ ] **T5.3 Model-author loop:** the propose → check → repair API with
      diagnostics as the feedback channel; system-prompt kit grown from
      demo-d-spec; the interchange measurement programme
      (interchange-notes.md §6: token cost, validity at scale, correction
      loops, engine comparison).
- [ ] **T5.4 Release engineering:** CI, packaging, versioning of language +
      store format (a sig-hash change like T2.3 is a store-format event —
      migration tooling), docs, external security review.

---

## Sequencing

1. Record audit + close paper-deviation findings (F-008, F-009) — same-day rule.
2. T0.1 Phase 10 metatheory → T0.2 Phase 11 gate (user decides; memo prepared).
3. Tier 1 milestone end-to-end (runs a real program from source).
4. Tiers 2 + 3 hardening in parallel with T4.1 operator CLI.
5. Pilot (T0.3, T5.1); measurement programme (T5.3); security review.
6. Concurrency (T5.2) as its own reviewed design cycle.
