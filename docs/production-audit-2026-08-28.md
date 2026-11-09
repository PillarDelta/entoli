# Production-readiness audit — 2026-08-28

Evidence base for `production-roadmap.md`. Produced by a four-lens parallel audit
(robustness, declared-but-missing functionality, operability, performance) over the
shipped crates, during the session that closed Phase 9. Findings are file-anchored;
line numbers are as of commit d3a9fe5. None contradicts the semantics — every item
is engineering distance between a reference interpreter and a production runtime.

**43 findings: 7 blocker, 19 major, 17 minor.**

## Blockers

### A-01 · [gaps] crates/entoli-cli/src/main.rs:63-88,145-171 (pick_program, wrap_driver, demo); crates/entoli-cli/Cargo.toml:9

The shipped binary has no way to execute or recover any user program. The only run/recover mode is `demo`, whose pick_program panics on anything but the four hardcoded fixtures (a, a2, c, sneaky) pulled from entoli-testkit — the test-fixture crate is a production dependency of the CLI. `compile` can check a .ent file but nothing can run one; there is no surface convention for an entry point, bindings, grants, or foreign bindings for a parsed program. Recovery likewise depends on per-demo hand-written prelude wrappers (settle_wrap etc.) that live in entoli-testkit; entoli-recovery::driver_for generates only the frontier match, and no shipped code constructs the handler prelude the driver needs (Protocol.complete searches the driver's own Handle frames — runtime lib.rs:788-800 — and returns UnhandledEffect without one, leaving pending entries permanently stuck).

**Impact:** A production deployment cannot run any program that was not compiled into the binary via Rust builders, and cannot recover one without hand-writing a Rust prelude per program; a wrong or missing prelude silently makes durable pending work unrecoverable. None of this is recorded as a deferral in EVALUATION.md.

### A-02 · [operations] crates/entoli-cli/src/main.rs:17-32 (full command surface: phase2-encode, phase2-resume, demo, check, compile, contracts)

The CLI is a kill-test harness, not an operator tool. `demo` only runs four hardcoded testkit programs (pick_program, main.rs:63-71); `compile` (main.rs:233) checks a .ent file then discards it — there is no command to RUN a user program against a store. No command exists to: list/release quarantined continuations, view or set grants/budgets, resume one specific continuation, garbage-collect Done entries, or back up a store. `check` (main.rs:174) is the only inspection and it dumps every row unpaginated.

**Impact:** In production an operator cannot deploy any real program, and every operational task beyond raw journal listing requires writing Rust against the crates or hand-editing SQLite.

### A-03 · [operations] crates/entoli-store/src/sqlite.rs:230-239 (quarantine) and the DurableStore trait (crates/entoli-store/src/lib.rs)

Quarantine is write-only: `quarantine()` sets the reason, but no store API, trait method, or CLI command clears it (grep confirms no unquarantine/release anywhere). Runtime refuses to load a quarantined continuation (entoli-runtime/src/lib.rs:710-715), and E012/E013 mismatches auto-quarantine (lib.rs:733-740).

**Impact:** Any continuation quarantined by a signature/compat drift — including a deliberate, later-reverted deploy — is stuck forever; the only recovery is manual `UPDATE continuations SET quarantined = NULL` in sqlite3.

### A-04 · [operations] crates/entoli-store/src/sqlite.rs:12-36 (schema) — no DELETE/VACUUM anywhere in the workspace

The journal and continuations tables are append-only with no compaction, GC, or retention path; every effect invocation persists a full continuation snapshot (encoded_frames BLOB, one per journal entry). Done entries and their now-dead continuations are never removable even after final_result is committed.

**Impact:** Store grows without bound per agent; long-lived agents accumulate one full continuation blob per effect for their entire lifetime with no supported cleanup.

### A-05 · [performance] crates/entoli-encode/src/codec.rs:449-457 (write_env), 485-489 (Closure arm); decode side codec.rs:720-734

Environment encoding has no structure sharing: every frame's persistent Env chain (Rc-shared in memory, env.rs:7) is written out in full, and every Closure value re-encodes its entire captured env recursively. Only Kont values get back-references (codec.rs:492-497); Env/Value Rc nodes get none. A continuation of D frames whose envs accumulate B bindings encodes as Θ(D·B) bytes (Θ(D²) for a let/recursion chain); mutually-captured closures are exponential in the worst case. read_env then rebuilds each env as a fresh chain, so decode also loses the sharing in memory.

**Impact:** encoded_frames blobs grow quadratically (worst-case exponentially) with continuation depth, and this blob is written at EVERY effect site (lib.rs:527) — SQLite write volume, journal file size, and resume decode time all blow up superlinearly with program nesting depth. This is the dominant scale hazard in the encode path.

### A-06 · [performance] crates/entoli-runtime/src/lib.rs:316-318 (check_invariants), called at 592, 611, 826, 401; crates/entoli-store/src/invariants.rs:36-53; sqlite.rs:189-193, 211-228

assert_store_invariants runs after every Intend and every commit_done. It calls store.entries() — 'SELECT * FROM journal', materializing every encoded_arg/encoded_result blob — then fetches the FULL DurableContinuation (entire encoded_frames blob, sqlite.rs:217) for every journal entry just to check it exists. So the Nth effect site re-reads all N prior continuation snapshots.

**Impact:** Per-effect cost is O(journal length × blob size); over a run of N effects the runtime reads O(N²) rows and O(N² × continuation size) bytes. Combined with the blob-size blowup above, long-running agents degrade rapidly. I1/I2 are checkable with two COUNT/EXISTS queries touching no blobs.

### A-07 · [robustness] crates/entoli-encode/src/codec.rs:990 (read_term), :736 (read_value), :618 (read_frame); also write_value :459, collect_value :883

Decode and encode are unboundedly recursive over term/value structure with no depth limit. A corrupted or adversarial continuation/result blob (e.g. ~100KB of repeated Lam tag 0x05) or a legitimately deep runtime value (long Cons-list built as nested Ctors) overflows the host stack.

**Impact:** Stack overflow is a SIGSEGV/abort, not a DecodeError. Since recovery re-reads the same blob from sigma on every restart, a poisoned journal produces a permanent crash loop with no clean corruption diagnostic; on the encode side the crash happens at Intend, before journaling, so restart re-runs the same program and dies again deterministically.

## Majors

### A-08 · [gaps] crates/entoli-check/src/parse.rs:284-345 (top-level loop, error "expected `effect` or `def`" at :333) and :394-629 (no ForeignApp production)

The surface parser has no `foreign` declaration and no syntax that produces Term::ForeignApp — grep confirms zero "foreign" occurrences in parse.rs, and Program.foreigns / Signatures.foreigns are populated only in entoli-testkit and test files. Post-unification (M1-M7 marked COMPLETE), ForeignApp is the ONLY world-contact form, so no surface-syntax program can touch the outside world; a suspending handler clause written in .ent can only do pure work and resume. A bare call `svc(x)` parses as App(Var) and fails the checker as unbound.

**Impact:** The unified world-contact design is unreachable from the language's own syntax; every real (effectful) program must be hand-built with Rust builders. The M1-M7 "COMPLETE" record and the demo-d surface-syntax claims do not note this gap — it is an unrecorded deferral.

### A-09 · [gaps] crates/entoli-cli/src/main.rs:233-265 (compile) — never calls check_entry; crates/entoli-check/src/lib.rs:143-164

The production compile path runs only Checker::check_defs, never check_entry. check_defs only verifies each def's body row against its declared annotation; nothing requires any row to be discharged. A program whose main honestly declares `Str -[Ledger]-> Data` and performs Ledger with no handler anywhere prints OK. The headline falsification property — an unrecovered external effect fails to compile with E001+E002 naming both the effect and the missing capability (demo B) — is only enforced by the library `check()` entry with caller-supplied bindings, which the CLI never invokes.

**Impact:** The one static gate a deployer can actually run does not enforce the language's core compile-time guarantee; unhandled-effect programs pass `compile` and fail only at runtime as UnhandledEffect.

### A-10 · [gaps] crates/entoli-encode/src/hash.rs:189-213 (effect_sig)

effect_sig's own comment says "Types arrive in Phase 5; until then the signature covers names, suspension class, and delivery classes" — Phase 5 shipped, but the hash was never extended: op argument/result types (kept in entoli-types::Signatures, which effect_sig cannot even see, since it takes only an EffectDecl) are excluded from the E012 structural-drift check. The paper's declared sig(h) = hash({op ↦ (τ1, τ2, δ)}) is implemented as hash({op ↦ δ}).

**Impact:** A deployment that changes only an operation's argument or result type resumes a stale continuation without E012/quarantine; wrongly-typed values flow into the resumed program and fail dynamically (or misbehave silently) mid-recovery — exactly the drift class §10.2 claims is caught. Unrecorded.

### A-11 · [gaps] crates/entoli-runtime/src/lib.rs:290-338 (Runtime::new, run_program) — no validate/check; crates/entoli-recovery/src/lib.rs:74-76; crates/entoli-check/src/lib.rs:113-137

The runtime never calls Program::validate() (E011) nor any part of the checker; entoli-runtime does not even depend on entoli-check. Validation is caller discipline (the CLI demo path calls it; embedders may not), while driver_for explicitly assumes "validate() has already run" and silently defaults a missing compensation target's delivery to Idempotent. Additionally Checker::check_defs iterates sigs.defs, so a builder-constructed def with no declared signature is skipped entirely yet still runs.

**Impact:** A production embedder can run programs that violate load-time rules (e.g. compensable compensation, E011) with no dynamic proxy other than the I1 depth invariant tripping mid-recovery — at which point σ holds an unrecoverable stack. Static-guarantee enforcement is wholly optional and disconnected from execution.

### A-12 · [operations] crates/entoli-runtime/src/lib.rs:626-642 (alloc_journal_id) and lib.rs:316-318 with call sites at 591-592, 610-611, 825-826 (check_invariants); invariants.rs:36 loads all entries

Every effect invocation loads the ENTIRE journal at least twice: alloc_journal_id calls store.entries() + pending_stack() and counts rows in Rust, and check_invariants runs after every intend and every commit_done, each doing a full-table scan and decode. Cost per effect is O(N) in journal size, O(N^2) over a run.

**Impact:** Combined with the append-only schema, throughput degrades continuously as an agent ages; a journal of tens of thousands of entries makes each new effect visibly slow, and there is no way to switch the per-step invariant audit off.

### A-13 · [operations] entire workspace (grep for tracing/log/metrics returns nothing)

Zero logging, tracing, or metrics in any shipped crate. The runtime's only output channels are the final Ok/Err return and the crash injector's eprintln (entoli-runtime/src/lib.rs:68). No record of which effect is being invoked, protocol step timings, quarantine events, budget spends, or recovery progress.

**Impact:** A production incident (hung external call, quarantine storm, slow recovery) is invisible; the operator's only forensic tool is reading the SQLite file after the fact.

### A-14 · [operations] crates/entoli-runtime/src/lib.rs:205-213 (ExternalOp::invoke) and lib.rs:497-504 (execute_clause foreign call); machine.rs:100-101 (run = run_fuel(u64::MAX))

External operations are synchronous with no timeout, retry, or cancellation — `invoke` blocks the whole runtime; and the drive loop runs the machine with unlimited fuel, so a divergent program spins forever. No deadline, watchdog, or fuel limit is configurable at the Runtime level.

**Impact:** One hung remote endpoint or one looping program permanently wedges the agent process with no observable state and no supervised recovery short of kill -9 (which, to be fair, the design survives — but nothing detects the need).

### A-15 · [operations] crates/entoli-store/src/sqlite.rs:39-47 (SqliteStore::open)

No busy_timeout is set and the connection is opened read-write with pragma updates. A second process opening the same store — e.g. an operator running `entoli-cli check --dir D` against a live agent — races the runtime and surfaces immediate SQLITE_BUSY as StoreError mid-protocol; there is no read-only inspection path.

**Impact:** Inspecting a live store can fail spuriously or make the running agent's next commit fail; operators have no safe concurrent-inspection story.

### A-16 · [operations] crates/entoli-cli/src/main.rs throughout (40 expect/unwrap/panic sites, e.g. lines 48, 51, 58, 69, 121, 147-149, 176, 184, 234-235, 244)

All operator-facing failures — missing flag, unreadable file, corrupt store, bad crash spec, unknown demo name — surface as Rust panics with backtraces rather than usage messages or diagnostics. `check` will panic (not report) on a store it cannot decode (main.rs:184, 201, 207).

**Impact:** The one inspection tool an operator has is itself brittle: a corrupted store aborts the inspector instead of describing the corruption, and scripting exit codes is unreliable (panic = 101, mixed with intended exit 1/2).

### A-17 · [performance] crates/entoli-runtime/src/lib.rs:626-642 (alloc_journal_id); entoli-store/src/sqlite.rs:189-193, 253-259

alloc_journal_id, executed at Step 1 of every effect-site protocol (lib.rs:525), loads the ENTIRE journal via entries() (SELECT * including arg/result blobs) plus a full pending_stack() scan, only to count siblings of the top pending entry. There is no index on journal(state) or journal(parent_journal_id) (schema sqlite.rs:12-36).

**Impact:** Journal-id allocation is O(entire journal) in rows and blob bytes per effect, O(N²) over a run — pure overhead for what is a single COUNT(*) WHERE parent_journal_id = ? query.

### A-18 · [performance] crates/entoli-encode/src/codec.rs:268-271 (Codec::new → def_hashes); hash.rs:147-187; call sites entoli-runtime/src/lib.rs:398, 519, 607, 646, 822

Codec::new recomputes def_hashes(program) — Tarjan SCC over the def graph plus canonical serialization and SHA-256 of every definition — and a fresh Codec is constructed per protocol invocation (lib.rs:519), again after each clause (lib.rs:607, 822), per machine-local recovery op (lib.rs:646), and at finish (lib.rs:398). effect_sig is likewise recomputed per header write (codec.rs:312) and per resume check (lib.rs:721). The hashes are pure functions of the immutable Program.

**Impact:** O(program size) SHA-256 work per effect site and per recovery-driver step: total hashing cost is O(program size × number of effects) instead of O(program size) once. Large programs pay whole-program hashing on every persist.

### A-19 · [performance] crates/entoli-runtime/src/lib.rs:527-528, 564 (protocol Step 1); codec.rs:275-286 (encode_kont), 791-810 (kont_refs)

Each Intend makes four independent full traversals of the entire captured continuation: encode_kont's collect pass (codec.rs:277-279), its write pass, kont_refs (lib.rs:528 — walks all frames again to recompute the same Refs the collect pass just built and discarded), and classify_frames (lib.rs:564). Serialization is fully non-incremental: nothing is memoized across effect sites even though the outer frames of the continuation are typically unchanged between consecutive effects.

**Impact:** Per-effect cost is 4×O(|K|) traversals plus O(|K|) bytes encoded and stored; a loop performing effects at depth D costs O(N·D) minimum even before the env-duplication blowup. At minimum the redundant kont_refs/classify traversals should share the encode pass.

### A-20 · [performance] crates/entoli-core/src/value.rs:55-67 (Value::Ctor { args: Vec<Value> }); machine.rs:129 (Var lookup .cloned()), 367 (match binder v.clone()); entoli-runtime/src/lib.rs:604 (susp.arg.clone())

Value is deep-cloned: Ctor holds args by value (Vec<Value>, not Rc), so every variable reference (machine.rs:129), every match-arm binder bind (machine.rs:367), and the runtime's susp.arg.clone() copies the entire data structure. Destructuring cons(h, t) clones the whole tail; traversing a list of length n is Θ(n²) copies. Terms and envs are Rc-shared, but the data plane is not.

**Impact:** Interpreter time and allocation grow quadratically with the size of program data (lists, trees) — an agent accumulating state pays O(state size) per variable mention. Wrapping Ctor args (or Value itself) in Rc removes it.

### A-21 · [performance] crates/entoli-store/src/sqlite.rs:12-36 (schema), and all query methods (no DELETE anywhere in entoli-store/entoli-runtime)

The journal and continuations tables only ever grow: done entries and their full continuation snapshots are never pruned, and there is no compaction/checkpoint path. Store size is O(effects × continuation size) for the life of the database.

**Impact:** Long-running agents accumulate an unbounded SQLite file of stale continuation blobs, and every whole-journal scan (findings above) gets slower in proportion. Production needs a retention/compaction story for entries no longer on the recovery path.

### A-22 · [robustness] crates/entoli-store/src/sqlite.rs:90-118 (decode_handled), consumed at crates/entoli-runtime/src/lib.rs:720-741

decode_handled returns a silently truncated (possibly empty) list on any malformed handled_effects blob — every early-exit path is `return out`, never an error. The E012/E013 signature and compat-token verification in load_kont iterates over whatever list comes back.

**Impact:** A single corrupted handled_effects blob silently disables the drift-detection safety gate: a sleeping continuation resumes against changed handler code with no quarantine and no error — exactly the failure class (merchant case, section 10.2) the mechanism exists to stop.

### A-23 · [robustness] crates/entoli-encode/src/codec.rs:611, 680, 685, 694, 699, 707, 745, 1029, 1038, 1049, 1073, 1090

Vec::with_capacity(n as usize) where n is a u32 read from the untrusted encoded blob, allocated before any element is validated. n = 0xFFFFFFFF frames (~100B each) requests ~400GB.

**Impact:** Corrupted store bytes trigger allocation failure, which aborts the Rust process (handle_alloc_error), or OOM-kills the host, instead of returning DecodeError::Malformed. Combined with recovery re-reading the blob, another crash-loop path on store corruption.

### A-24 · [robustness] crates/entoli-core/src/machine.rs:547, 551, 555 (apply_prim Add/Sub/Mul); Cargo.toml [profile.release] sets only debug=true

Language-level integer arithmetic uses unchecked native i64 +/-/* . Release builds (overflow-checks off) wrap silently; debug builds panic, so semantics diverge between build profiles.

**Impact:** In production a budget/amount computation that overflows wraps silently and the wrong value is durably committed to the journal as a fact; in a debug deployment the same program panics mid-protocol instead of producing an EntoliError.

### A-25 · [robustness] crates/entoli-store/src/sqlite.rs:39-47 (open), crates/entoli-runtime/src/lib.rs:286+704 (loaded_konts affine check)

No cross-process exclusion: SqliteStore sets no locking_mode=EXCLUSIVE, no busy_timeout, no lease/pid guard, and the one-shot-resume (affinity) proxy loaded_konts is an in-memory per-process set. Two runtimes can open the same store.sqlite and both run recover() concurrently.

**Impact:** Both processes pass the affine check, both load the same pending continuation and re-execute the external clause — duplicate external side effects; commit_done's state='pending' guard only makes the loser error after the double invocation. Any concurrent access also surfaces as raw SQLITE_BUSY StoreErrors with no retry.

### A-26 · [robustness] crates/entoli-runtime/src/lib.rs:626-642 (alloc_journal_id), :591-592/:610-611/:825-826 (check_invariants after every transition) with crates/entoli-store/src/invariants.rs:35-74

Every effect operation loads the ENTIRE journal (entries() materializes all rows including result blobs) for id allocation, and the invariant check additionally fetches every continuation blob for every entry after every Intend/Commit. The journal is never pruned or compacted.

**Impact:** Per-operation cost is O(total history) in time and memory — quadratic over an agent's lifetime. A long-lived durable agent (the product's stated purpose) degrades without bound and eventually exhausts memory loading its own history on each step.

## Minors

### A-27 · [gaps] crates/entoli-runtime/src/lib.rs:766-834 (Protocol.complete) vs :568-575 (protocol strict gate)

strict_contracts ("refuse to invoke an operation whose declared delivery contract has not passed the conformance harness") is checked only in protocol() before a fresh Intend; the recovery re-invocation path Protocol.complete executes the clause with grant/budget checks but no strict-contracts check.

**Impact:** In strict mode, an operation whose contract is untested/failed can still be (re-)invoked against the external world during recovery — the mode's guarantee is silently narrower than declared.

### A-28 · [gaps] crates/entoli-check/src/parse.rs:427-442 (pattern) vs :589-603 (expression); crates/entoli-core/src/machine.rs:354-377; crates/entoli-check/src/lib.rs:283-290

A bare capitalized identifier is a nullary constructor in expression position but a wildcard binder (Pattern::Any) in pattern position: `match r { None -> a, Just(v) -> b }` parses None as a catch-all, and the evaluator then always takes the first arm. Neither the checker nor the parser warns (no unreachable-arm or exhaustiveness diagnostics; the arm binds a Data variable named None). The correct spelling `None()` is undocumented.

**Impact:** Silent wrong control flow in surface programs — model-written code (the demo-D audience) will plausibly write bare nullary patterns and get always-first-arm behavior with no diagnostic.

### A-29 · [gaps] crates/entoli-store/src/sqlite.rs:90-118 (decode_handled)

decode_handled returns a silently truncated list on any malformed/short handled_effects blob (every bounds failure is `return out`). The declared rule is that BOTH drift halves (E012 sig, E013 compat) "are required to resume" (store lib.rs:44-47, runtime lib.rs:717-741), but a corrupted blob simply drops entries from the checked list.

**Impact:** Store corruption degrades drift detection silently: a continuation can resume against changed code with no E012/E013 and no quarantine, instead of failing loudly.

### A-30 · [gaps] crates/entoli-cli/src/main.rs:236-247 (--ablate); crates/entoli-check/src/lib.rs:68-80 (CheckFlags)

The demo-D ablation switches (disable affine E003, witness/delivery, tail E004, nested-suspension E005 checking) ship in the production `compile` command; output of an ablated compile is an indistinguishable "OK".

**Impact:** A deployer can unknowingly gate on a compile that had core restrictions disabled; the flags exist for the plan §10.5 measurement and have no place in a production gate. (Dynamic proxies catch most violations at runtime, so consequence is failure-late rather than unsoundness.)

### A-31 · [gaps] crates/entoli-check/src/lib.rs:319-331 (ForeignApp typing)

A foreign name present in Program.foreigns but absent from Signatures.foreigns silently types as Data -> Data (unwrap_or) — no diagnostic distinguishes "declared with no signature" from "fully declared", and nothing cross-checks that Program.foreigns and Signatures.foreigns agree.

**Impact:** Foreign-boundary type checking silently degrades to fully dynamic for any binding whose signature registration was forgotten (easy, since both tables are populated by hand in Rust — there is no syntax).

### A-32 · [operations] crates/entoli-runtime/src/lib.rs:226-240 (RuntimeError Display) and lib.rs:571-588, 723-731 (ad-hoc E-code strings)

Encode/Decode errors are Displayed via Debug ({e:?}, lib.rs:230-231); diagnostic codes E002/E012/E013 are format!-ed into untyped Refused strings rather than structured variants, unlike EntoliError which has proper variants; Refused prints a bare message with no journal/continuation context beyond what happened to be interpolated.

**Impact:** Operators and tooling cannot match on error codes reliably (string-grep only), and Debug-formatted internals leak into user-facing output; automated alerting on specific failure classes is impractical.

### A-33 · [operations] crates/entoli-runtime/src/lib.rs:290-307 (Runtime::new) and sqlite.rs:41-44; cli/main.rs:48 (hardcoded store.sqlite filename), lib.rs:306 (compat default "v1")

No configuration surface exists: WAL+synchronous=FULL, store filename, invariant-check frequency, compat default token, and (absent) fuel/timeout limits are all hardcoded. The only tunables are programmatic struct fields (strict_contracts, compat map) with no config file, env var, or CLI flag except --compat in the demo harness.

**Impact:** Any production tuning (relaxing fsync policy for throughput, changing store location, setting budgets outside demo C's --grant flag) requires recompiling.

### A-34 · [operations] crates/entoli-runtime/src/lib.rs:87-134 (GrantTable, in-memory only, per Rule 4)

Grants/budgets are process-local by design, but there is no introspection at all: no CLI command or runtime API dumps the grant table, remaining budgets, or revocations for a running agent, and `check` shows nothing about authority.

**Impact:** An operator diagnosing an E002/E014 budget-exhausted or revoked-grant failure cannot see current authority state; the design decision (grants never in σ) makes an explicit view command the only possible window, and it doesn't exist.

### A-35 · [performance] crates/entoli-store/src/sqlite.rs:159, 190, 198, 206, 217, 231, 244, 256

Every store method re-prepares its SQL from scratch (conn.prepare / conn.execute with literal SQL); rusqlite's prepare_cached is never used. entries()/frontier()/entry() also SELECT * — fetching blob columns even when callers only need counts or metadata.

**Impact:** SQL parse/plan overhead on every one of the O(J) statements issued per effect site (multiplied by the invariant/alloc scans). One-line prepare_cached change plus narrower column lists; add indexes on journal(state) and journal(parent_journal_id) when the scans are fixed.

### A-36 · [performance] crates/entoli-encode/src/codec.rs:252-255, 492-497 (EncCtx.seen: Vec<usize> with .iter().position per Kont)

Continuation back-reference detection does a linear scan of all previously written Kont pointers for each Kont value encountered — O(k²) for k reachable continuation values in one encode.

**Impact:** Programs holding many first-class continuations (e.g. accumulated in a data structure) make encode quadratic in k on top of its size cost; a HashMap<usize, u32> fixes it.

### A-37 · [robustness] crates/entoli-store/src/sqlite.rs:68-71 (row_to_entry)

Any state string other than 'pending' is mapped to Done, and a 'done' row with NULL encoded_result becomes Done(vec![]) via unwrap_or_default. Corruption that bypasses the CHECK constraint (partial write, external edit) is normalized instead of reported.

**Impact:** A pending-in-fact entry misread as Done makes recovery skip re-invocation and resume the continuation with a malformed empty result — the failure surfaces later as a confusing decode error (or wrong control flow) rather than an immediate store-corruption diagnostic.

### A-38 · [robustness] crates/entoli-runtime/src/lib.rs:577-591 (protocol: grants.spend before store.intend)

The capability budget is decremented in memory before the Intend transaction. If intend fails (disk full, induced failure), sigma correctly sees a no-transition but the budget stays spent.

**Impact:** A transient store failure plus retry within one process double-charges the budget, producing spurious 'budget exhausted' (E002) refusals — availability loss on exactly the runs already experiencing storage trouble.

### A-39 · [robustness] crates/entoli-core/src/env.rs:7-14 (Env linked list), crates/entoli-core/src/value.rs (nested Ctor/Kont ownership)

Env is an Rc linked list and values nest recursively; neither has an iterative Drop. Dropping a very long environment chain or deeply nested value recurses in drop glue.

**Impact:** Deep program state can crash the process with a stack overflow at scope exit even before the codec is involved; same abort-not-error failure mode as the codec recursion.

### A-40 · [robustness] crates/entoli-check/src/parse.rs:394-605 (parse_expr -> parse_prefix recursion), crates/entoli-check/src/lib.rs (synth recursion)

The recursive-descent parser and the checker's synth recurse without a depth limit over source nesting.

**Impact:** `entoli compile` on deeply nested (or hostile) source aborts with a stack overflow instead of emitting a diagnostic — relevant if compile is exposed as a service.

### A-41 · [robustness] crates/entoli-cli/src/main.rs:234-236 (compile: expect on missing arg / unreadable file), :48 (store open expect), :58 (--compat expect), :121 (budget parse expect)

The CLI's production-facing subcommands (notably `compile`) use .expect()/panic! for user errors: missing argument, unreadable file, malformed flag.

**Impact:** Users get a Rust panic backtrace and exit code 101 instead of a clean diagnostic and documented exit code; panic output can be mistaken for an interpreter bug in ops tooling.

### A-42 · [robustness] crates/entoli-store/src/sqlite.rs:172-180 (commit_final ON CONFLICT DO NOTHING)

A second commit_final with a DIFFERENT encoded result is silently ignored — no comparison against the stored value, no error.

**Impact:** A nondeterminism or recovery bug that produces a divergent final result is invisible: the store keeps the first value and the caller believes it committed the second. A mismatch check would turn silent divergence into a detectable Refused.

### A-43 · [robustness] crates/entoli-runtime/src/lib.rs:377 (drive calls m.run()) with crates/entoli-core/src/machine.rs:100-102 (run = run_fuel(u64::MAX))

The production drive loop runs the machine with effectively infinite fuel; the existing fuel mechanism is not exposed through Runtime.

**Impact:** A non-terminating or accidentally divergent program hangs the runtime forever with no bound on CPU or on continuation-stack memory growth; no operational kill-switch short of process kill.
