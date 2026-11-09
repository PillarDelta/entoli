# ADR-0002 · Phase 11: ENTOLI is a standalone core language

Date: 2026-09-01 · Status: adopted provisionally (reversal triggers below)

## The question

Plan v1.0 §15: after the Phase 8 GO, paper v0.6, and the Phase 10 metatheory,
choose one of three directions — **A** standalone language, **B** extension to a
typed host, **C** typed DSL over a host runtime. The choice redirects Tiers 3–5
of the production roadmap; Tier 1 and the paper-deviation repairs proceed under
any outcome.

## Decision

**Option A — a standalone core language, deliberately small.** ENTOLI is the
orchestration core only: durable control, typed effect rows, affine
continuations, the effect-site protocol. Everything that touches the world is a
`foreign` binding, and the host boundary is exactly the foreign-application
boundary the unified machine already enforces. The product thesis is the
interchange reading (paper §2.3, `docs/interchange-notes.md`): the durable,
inspectable, capability-free continuation encoding is the artifact; the surface
language exists to author programs whose checkpoints have that property.

## Why not B or C

Each option is judged against its own choose-if clause from plan §15.

**B fails its clause.** "Durable boundaries with non-persistent captures can be
enforced" is the load-bearing conjunct, and no practical host enforces
Repr/Reviv over its own closures: a host-language lambda closes over anything in
scope — sockets, credentials, arbitrary heap — and neither Scala capture
checking nor any mainstream effect-system embedding makes *serialisability with
no captured authority* a checked property of ordinary closures. The Phase 2
kill point passed **because** the value domain is closed (ADR-0001 §3: the
hand-rolled, capability-free encoding is the property under test, and serde-over
-host-closures would blur exactly that). Choosing B re-opens the premise the
paper's own §14 names weakest — serialisable continuations — in a domain we do
not control. The paper's §16 expectation that the *capability half* lands in
host type systems stands; that is an influence claim, not an implementation
direction.

**C collapses into A.** A "restricted subset" of a host that excludes captured
authority, arbitrary recursion into host values, and host-side effects is not a
subset the host's own tooling can delimit — the delimiter would be our checker,
the runtime would be our machine, and the host syntax would be a skin over the
same closed core. That is option A with extra packaging and a worse error
surface (two compilers disagreeing about one program). C's clause — "the
semantic core can be enforced within a restricted subset" — is true only where
the restriction is so severe the host contributes nothing but familiarity.

**A meets its clause.** (i) The semantics require control practical hosts do not
give: a closed value domain, machine-owned continuations, and a compile gate
that can refuse a program whose rows do not discharge. (ii) Surface usability
is no longer conjecture: demo D had a model author 6/6 programs against the
checker (docs/demo-d-spec.md, EVALUATION.md), which is the authoring loop the
product actually needs. (iii) The durability model needs whole-program
knowledge — per-def Merkle hashes over the SCC graph, the grant table, the
driver generated from the program's own declarations — which is
whole-program compilation in exactly the clause's sense.

## Consequences

- Tiers 3–5 are read under A: performance work targets our machine (T3), the
  operator surface is our CLI/store (T4), and "real foreign bindings" (T5.1)
  are the host boundary, not an embedding.
- The interchange reading is the thesis to *measure*, not assert: the six
  obligations and the measurement programme in `docs/interchange-notes.md`
  gate any efficiency or portability claim. No such claim is made unmeasured.
- Tier 1 proceeds immediately (it was direction-independent): T1.1 surface
  `foreign` + foreign application, T1.2 run/recover CLI, T1.3 compile gate,
  T1.4 checker completions.

## Reversal triggers

Adopted provisionally; revisit if either fires:

1. **Convergence-study failure** — model authors cannot converge on checked
   programs at realistic program sizes (the demo-D loop stops scaling), which
   would undercut A's usability conjunct and favour C's familiarity argument.
2. **Serialisation breaks on realistic value domains** — the pilot workload
   needs values the closed codec cannot carry without growing toward
   host-closure generality, which would undercut the closed-domain premise and
   force the B/C question again on new evidence.

Neither trigger has fired as of this memo; both are observable from the pilot
(T0.3) without dedicated studies.
