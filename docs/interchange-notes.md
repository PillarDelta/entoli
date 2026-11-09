# ENTOLI as an interchange medium — design notes

Drafted 2026-08-28, from the observation (raised during Phase 9) that the paper
explains *execution* far more fully than *agent-to-agent communication*, while the
demo D result makes the communication reading the higher-impact one:

```
natural language → agent generates ENTOLI
                → checker verifies authority and recovery
                → another agent/runtime executes it
                → execution can pause, move, crash, and resume safely
```

The checker is then the hard boundary of the system: agents propose programs freely;
only valid programs execute. The human costs of the discipline (affinity, delivery
annotations, explicit capabilities) are paid by a generator that does not feel
tedium. If it works, it enables: safe delegation between agents, portable and
inspectable agent plans, limits that survive sub-agent delegation, reliable
long-running agent work, machine-checkable budgets and permissions, and
interoperability between agents and runtimes.

**Calibration rule for all text derived from these notes:** the current evidence
supports *more explicit, machine-checkable, and potentially safer*. It does NOT
support *more efficient / more optimal*, which must be measured before claimed —
token cost of generation, first-pass validity beyond n = 6, correction-loop counts,
execution overhead, and a head-to-head against structured JSON plus a workflow
engine or an existing agent DSL.

Paper status: v0.6 carries a compact, calibrated statement of this reading as §2.3,
with the six obligations below named as open. The fuller dispositions live here.

---

## The six obligations

### 1 · What one agent sends to another

Candidates: surface source, typed AST, module, continuation. The semantics already
privileges two payload forms:

- **A program**: the content-addressed typed AST of §10.1 — closed over everything
  it references (Merkle closure, SCC-grouped), with effect signatures (delivery
  classes included), compat tokens, and foreign declaration *names* (content
  identities; implementations are host-bound at the receiver, never in the payload).
  Self-describing by construction: the row in its type is its complete tool surface.
- **A continuation**: persist(K) plus the store slice it needs — frames,
  (sig, compat) per handled effect, code-identity closure with absence sentinels.
  By Rule 4 it confers no authority; by §10.2 it names exactly the contract it
  depends on.

Open: the envelope. Which form crosses in which situation, the wire encoding, and
whether a "module" (several definitions + declarations, no entry point) is a
distinct unit or just a program without main.

### 2 · Independent verification by the receiver

Answered by construction, modulo checker maturity. Checking is static and the
payload contains nothing opaque, so the receiver re-runs its *own* checker: no
trust in the sender's toolchain is needed. The three falsification-criterion
clauses are exactly what a receiver cares about: no unrecovered external effect, no
unserialisable capture, no tool beyond the declared row. Hash-closure integrity is
mechanically checkable (recompute the Merkle names).

Open: the checker is bidirectional-lite (F-005) — row polymorphism and full
inference are future work, and a lingua-franca use hardens the requirement that
the checker itself be small enough to be independently reimplemented (a spec-level
artifact, like the paper's typing rules, not a codebase).

### 3 · Delegation without transferable credentials

The strongest already-answered piece, and the reason the reading is credible at
all. The payload carries a **row** — a requirement — never a capability — an
authority (Rule 4: no encode arm exists). A received program is inert until the
receiver installs handlers from its own authority (`handle … using κ` around the
received program: delegation IS installation). Limits survive sub-delegation by
demo B's containment, now for the whole language including suspending tools;
budgets are affine and split at fork (§12), so ten delegates under one budget
divide it.

Open: attenuation semantics across agents — can a sender *request* a policy
(π as a proposal), and is the receiver's grant the meet of the request and its own
policy? Today π lives entirely with the receiver's κ, which is safe but
inexpressive.

### 4 · Authentication of code and messages

Not answered, and must not be overclaimed: content addressing gives *integrity* —
the hash is the name, so the artifact cannot be silently altered — and says
nothing about *origin*. Signatures over content hashes are the obvious
composition; key distribution, revocation of trust in a sender, and the
relationship between "who wrote this" and "who grants authority to it" (they are
different parties in the delegation model above, which is a feature) are protocol
work outside the calculus. The calculus's contribution is that authenticity is
*only* needed for provenance/accountability — never for safety, because safety
comes from the receiver's own check and the receiver's own grants.

### 5 · Negotiating incompatible effects or policies

Today: refusal semantics only, and they are complete — E001/E002 at check time
(row escapes / missing capability), E012/E013 at resume (computed / declared
drift), E014 (missing installation), sticky durable quarantine as the failure
mode that loses nothing. Nothing negotiates *past* a refusal. The migrate adapter
hook (§10.2, specified, unimplemented — F-006) is the seed: an adapter is a
receiver-side unilateral concession, and a negotiation would be an exchange of
proposed adapters/policies before execution. Undesigned.

### 6 · Why this beats structured JSON plus a workflow engine

The defensible claim, stated carefully: a JSON tool protocol carries *data* plus
out-of-band conventions about behaviour; an ENTOLI payload carries *checkable
claims*. The row is the complete tool surface, and it composes — a function that
transitively performs an effect cannot hide it (§2.2), which no schema for
individual messages can express. Recovery semantics (delivery classes, witnesses)
are part of the artifact, not the engine's configuration. Drift between suspend
and resume is detectable (sig + compat). The same artifact is executable and
inspectable. A workflow engine enforces at runtime a subset of what the checker
refuses before execution begins.

NOT claimed, pending measurement: efficiency. Required experiments before any
comparative-efficiency sentence enters the paper: (a) token cost, ENTOLI
generation vs JSON tool calls for matched tasks; (b) first-pass validity rate at
scale (demo D is n = 6, one model family, calibrated tasks); (c) correction-loop
counts against checker feedback; (d) interpreter overhead vs a workflow engine on
matched orchestrations; (e) qualitative comparison against existing agent DSLs.

---

## Consequences for the roadmap

- Phase 10 (metatheory) and Phase 11 (language decision) proceed unchanged; the
  interchange reading raises the value of option A/B outcomes but does not alter
  the gate's criteria.
- The demo D convergence study (independent model, adversarial tasks) doubles as
  experiment (b) above.
- Obligations 1 (envelope) and 3 (attenuation) are semantics-adjacent and could
  become a v0.7 design note; 4 (authentication) and 5 (negotiation) are protocol
  layers above the calculus; 6 is an evaluation programme.
