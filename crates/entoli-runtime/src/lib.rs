//! Phase 3–4: the five-step effect-site protocol (paper §7.1) and the restart entry
//! point. The runtime supplies exactly what plan §3.2 allows it to supply — durable
//! store primitives and effect invocation. Recovery *logic* lives in `entoli-recovery`
//! as ENTOLI terms; this crate only serves that program's machine-local effects
//! (pure reads of σ plus `Protocol.complete`, the store's atomic completion of an
//! already-intended entry — the plan §8 boundary, recorded in EVALUATION.md F-001).

use entoli_core::{
    CapAuthority, CapId, Env, EntoliError, Frame, Machine, Outcome, Suspension, Value,
};
use entoli_encode::{Codec, DecodeError, EncodeError, ResumeGranter};
use entoli_store::{
    assert_pending_bounds, assert_store_invariants, DurableContinuation, DurableStore,
    EntryState, JournalEntry, SqliteStore,
};
use entoli_syntax::{DeliveryClass, Program, Term};
use std::collections::BTreeMap;
use std::rc::Rc;

pub const LOCAL_EFFECT: &str = "Local";
pub const PROTOCOL_EFFECT: &str = "Protocol";

/// Named crash points (plan §9). `after_remote_before_receive` is honored inside
/// fixtures, between their own commit and their return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashPoint {
    BeforeIntend,
    AfterIntend,
    AfterRemoteBeforeReceive,
    AfterReceiveBeforeCommit,
    AfterCommit,
    BeforeResume,
}

impl CrashPoint {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "before_intend" => CrashPoint::BeforeIntend,
            "after_intend" => CrashPoint::AfterIntend,
            "after_remote_before_receive" => CrashPoint::AfterRemoteBeforeReceive,
            "after_receive_before_commit" => CrashPoint::AfterReceiveBeforeCommit,
            "after_commit" => CrashPoint::AfterCommit,
            "before_resume" => CrashPoint::BeforeResume,
            _ => return None,
        })
    }
}

/// Hard process death at a named point for a named operation. Spec: `E.op:point`.
#[derive(Debug, Default, Clone)]
pub struct CrashInjector {
    pub target: Option<(String, String, CrashPoint)>,
}

impl CrashInjector {
    pub fn parse(spec: &str) -> Option<Self> {
        let (site, point) = spec.split_once(':')?;
        let (effect, op) = site.split_once('.')?;
        Some(CrashInjector {
            target: Some((effect.to_string(), op.to_string(), CrashPoint::parse(point)?)),
        })
    }

    /// Dies with exit code 87 — no destructors, nothing volatile flushed (plan §9).
    pub fn hit(&self, effect: &str, op: &str, point: CrashPoint) {
        if let Some((te, to, tp)) = &self.target {
            if te == effect && to == op && *tp == point {
                eprintln!("CRASH {effect}.{op}:{point:?}");
                std::process::exit(87);
            }
        }
    }
}

/// Phase 6: the grant table — the runtime half of Rule 4. Grants live HERE, never in σ:
/// a persisted continuation names effects, and only this table, in force at resume,
/// turns a name back into authority. Revocation is a table update; no protocol reaches
/// sleeping continuations because nothing needs to (§9.1).
#[derive(Debug, Clone)]
pub struct Grant {
    pub effect: String,
    /// Quantitative policy: remaining invocations (paper §11's budget, simplified).
    pub budget: Option<i64>,
    pub revoked: bool,
}

#[derive(Debug, Default)]
pub struct GrantTable {
    next: u64,
    pub grants: std::collections::BTreeMap<CapId, Grant>,
}

impl GrantTable {
    pub fn grant(&mut self, effect: &str, budget: Option<i64>) -> CapId {
        self.next += 1;
        let id = CapId(self.next);
        self.grants.insert(id, Grant { effect: effect.into(), budget, revoked: false });
        id
    }

    pub fn revoke(&mut self, effect: &str) {
        for g in self.grants.values_mut() {
            if g.effect == effect {
                g.revoked = true;
            }
        }
    }

    pub fn live_for(&self, effect: &str) -> Option<CapId> {
        self.grants
            .iter()
            .find(|(_, g)| g.effect == effect && !g.revoked)
            .map(|(id, _)| *id)
    }

    /// Decrement the live grant's budget for one invocation.
    pub fn spend(&mut self, effect: &str) -> Result<(), String> {
        let Some((_, g)) = self
            .grants
            .iter_mut()
            .find(|(_, g)| g.effect == effect && !g.revoked)
        else {
            return Err(format!("no live grant for {effect}"));
        };
        match &mut g.budget {
            None => Ok(()),
            Some(b) if *b > 0 => {
                *b -= 1;
                Ok(())
            }
            Some(_) => Err(format!("budget exhausted for {effect}")),
        }
    }
}

impl CapAuthority for GrantTable {
    fn effect_of(&self, cap: CapId) -> Option<String> {
        self.grants.get(&cap).filter(|g| !g.revoked).map(|g| g.effect.clone())
    }
}

impl ResumeGranter for GrantTable {
    fn grant_for(&self, effect: &str) -> Option<CapId> {
        self.live_for(effect)
    }
}

/// §9.2: a store's classification is the join of what it actually holds, computed,
/// never asserted. v1 policy: any value inside a `Confidential(...)` constructor is
/// confidential; everything else is public.
pub fn classify_frames(frames: &[Frame]) -> String {
    fn value_confidential(v: &Value) -> bool {
        match v {
            Value::Ctor { name, args } => {
                name == "Confidential" || args.iter().any(value_confidential)
            }
            Value::Closure { env, .. } => env.iter().any(|(_, v)| value_confidential(v)),
            Value::Kont(k) => k.borrow().frames.iter().any(frame_confidential),
            _ => false,
        }
    }
    fn env_confidential(env: &Env) -> bool {
        env.iter().any(|(_, v)| value_confidential(v))
    }
    fn frame_confidential(f: &Frame) -> bool {
        match f {
            Frame::AppFn { env, .. }
            | Frame::Let { env, .. }
            | Frame::If { env, .. }
            | Frame::DoArg { env, .. }
            | Frame::HandleCap { env, .. }
            | Frame::Handle { env, .. }
            | Frame::ResumeKArg { env, .. }
            | Frame::Match { env, .. } => env_confidential(env),
            Frame::AppArg { func } => value_confidential(func),
            Frame::DoWitness { arg, .. } => value_confidential(arg),
            Frame::ResumeApply { kont } => value_confidential(kont),
            Frame::CtorArgs { done, env, .. } | Frame::PrimArgs { done, env, .. } => {
                done.iter().any(value_confidential) || env_confidential(env)
            }
            Frame::ClauseBoundary { .. } | Frame::ResumeBoundary { .. } => false,
            Frame::ForeignArg { .. } => false,
        }
    }
    if frames.iter().any(frame_confidential) {
        "confidential".into()
    } else {
        "public".into()
    }
}

/// A foreign binding (unification §3): the host implementation behind a `foreign`
/// declaration. The witness travels on the machine channel (journal → invocation),
/// never through clause typing; the injector lets the implementation fire
/// `after_remote_before_receive` between its own commit and its return.
pub struct ForeignBinding {
    pub effect: String,
    pub op: String,
    pub ext: Box<dyn ExternalOp>,
}

/// A journalled external operation — the fixture side of step 2. The injector is passed
/// in so the fixture can fire `after_remote_before_receive` between its own durable
/// commit and its return.
pub trait ExternalOp {
    fn invoke(
        &mut self,
        op: &str,
        arg: &Value,
        witness: Option<&str>,
        crash: &CrashInjector,
    ) -> Result<Value, String>;
}

#[derive(Debug)]
pub enum RuntimeError {
    Eval(EntoliError),
    Encode(EncodeError),
    Decode(DecodeError),
    Store(entoli_store::StoreError),
    Invariant(String),
    External { effect: String, op: String, msg: String },
    Refused(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Eval(e) => write!(f, "{e}"),
            RuntimeError::Encode(e) => write!(f, "encode: {e:?}"),
            RuntimeError::Decode(e) => write!(f, "decode: {e:?}"),
            RuntimeError::Store(e) => write!(f, "{e}"),
            RuntimeError::Invariant(m) => write!(f, "INV(σ) violated: {m}"),
            RuntimeError::External { effect, op, msg } => {
                write!(f, "external {effect}.{op} failed: {msg}")
            }
            RuntimeError::Refused(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for RuntimeError {}

impl From<EntoliError> for RuntimeError {
    fn from(e: EntoliError) -> Self {
        RuntimeError::Eval(e)
    }
}
impl From<EncodeError> for RuntimeError {
    fn from(e: EncodeError) -> Self {
        RuntimeError::Encode(e)
    }
}
impl From<DecodeError> for RuntimeError {
    fn from(e: DecodeError) -> Self {
        RuntimeError::Decode(e)
    }
}
impl From<entoli_store::StoreError> for RuntimeError {
    fn from(e: entoli_store::StoreError) -> Self {
        RuntimeError::Store(e)
    }
}

pub struct Runtime {
    /// The program *extended* with the Local/Protocol machine-local declarations.
    pub program: Program,
    sigs: entoli_types::Signatures,
    pub store: SqliteStore,
    /// Foreign bindings by declared name (unification §3): the only path to the
    /// outside world. M7 removed the transitional per-effect registry.
    pub foreigns: BTreeMap<String, ForeignBinding>,
    /// Grants in force for THIS process run — never persisted (Rule 4).
    pub grants: GrantTable,
    pub crash: CrashInjector,
    /// §10.2: declared compatibility tokens per effect — the deployer's statement of
    /// the observable contract. Defaults to "v1" for undeclared effects.
    pub compat: BTreeMap<String, String>,
    /// Phase 7 strict mode (plan §11.3): refuse to invoke an operation whose declared
    /// delivery contract has not passed the conformance harness.
    pub strict_contracts: bool,
    pub contracts_passed: std::collections::BTreeSet<(String, String)>,
    /// Machine-local effects are served only when the recovery driver is running
    /// (plan §8: "granted only to the recovery driver").
    driver_active: bool,
    /// Review finding R-3: within one process run, each persisted continuation may be
    /// materialised at most once — the affinity proxy for the durable object itself.
    loaded_konts: std::collections::BTreeSet<String>,
    /// Run the full whole-journal invariant audit after every durable
    /// transition. Defaults on in debug builds; release builds run the light
    /// I1 check only — the retirement the preservation theorem licenses.
    pub audit_full: bool,
}

/// Canonical journal form of a delivery class (F-011): recorded at Intend so
/// recovery dispatch and I1's compensation clause are functions of σ alone.
fn delivery_str(d: &DeliveryClass) -> String {
    match d {
        DeliveryClass::Idempotent => "idempotent".into(),
        DeliveryClass::AtLeastOnce => "at_least_once".into(),
        DeliveryClass::Compensable { comp_effect, comp_op } => {
            format!("compensable:{comp_effect}.{comp_op}")
        }
    }
}

impl Runtime {
    pub fn new(
        program: &Program,
        sigs: &entoli_types::Signatures,
        store: SqliteStore,
    ) -> Self {
        Runtime {
            program: entoli_recovery_decls(program),
            sigs: sigs.clone(),
            store,
            foreigns: BTreeMap::new(),
            grants: GrantTable::default(),
            crash: CrashInjector::default(),
            compat: BTreeMap::new(),
            strict_contracts: false,
            contracts_passed: std::collections::BTreeSet::new(),
            driver_active: false,
            loaded_konts: std::collections::BTreeSet::new(),
            audit_full: cfg!(debug_assertions),
        }
    }

    pub fn compat_token(&self, effect: &str) -> String {
        self.compat.get(effect).cloned().unwrap_or_else(|| "v1".to_string())
    }

    pub fn bind_foreign(&mut self, name: &str, effect: &str, op: &str, ext: Box<dyn ExternalOp>) {
        self.foreigns.insert(
            name.to_string(),
            ForeignBinding { effect: effect.into(), op: op.into(), ext },
        );
    }

    fn check_invariants(&self) -> Result<(), RuntimeError> {
        let r = if self.audit_full {
            assert_store_invariants(&self.store)
        } else {
            assert_pending_bounds(&self.store)
        };
        r.map_err(|v| RuntimeError::Invariant(v.to_string()))
    }

    /// Fresh run: the journal must be empty (a non-empty journal means restart —
    /// use `recover`).
    pub fn run_program(&mut self, entry: Rc<Term>) -> Result<Value, RuntimeError> {
        if !self.store.entries()?.is_empty() || self.store.final_result()?.is_some() {
            return Err(RuntimeError::Refused(
                "journal is not empty: use recover() to restart".into(),
            ));
        }
        self.drive(entry, Env::empty())
    }

    pub fn run_program_env(&mut self, entry: Rc<Term>, env: Env) -> Result<Value, RuntimeError> {
        if !self.store.entries()?.is_empty() || self.store.final_result()?.is_some() {
            return Err(RuntimeError::Refused(
                "journal is not empty: use recover() to restart".into(),
            ));
        }
        self.drive(entry, env)
    }

    /// Restart entry point (plan Phase 4): consult σ only. If a final result exists the
    /// computation already finished; otherwise run the generated ENTOLI recovery driver.
    pub fn recover(&mut self, driver: Rc<Term>) -> Result<Value, RuntimeError> {
        self.recover_env(driver, Env::empty())
    }

    pub fn recover_env(&mut self, driver: Rc<Term>, env: Env) -> Result<Value, RuntimeError> {
        self.check_invariants()?;
        if let Some(bytes) = self.store.final_result()? {
            let codec = Codec::new(&self.program, &self.sigs, &self.grants);
            return Ok(codec.decode_value(&bytes, &self.grants)?);
        }
        self.driver_active = true;
        let out = self.drive(driver, env);
        self.driver_active = false;
        out
    }

    /// The drive loop: evaluate; on suspension run the protocol (or serve a
    /// machine-local read); on a final value commit it durably.
    fn drive(&mut self, entry: Rc<Term>, env: Env) -> Result<Value, RuntimeError> {
        enum Next {
            Start(Rc<Term>, Env),
            Resume(Vec<entoli_core::Frame>, Value, String),
        }
        let mut next = Next::Start(entry, env);
        loop {
            // The machine borrows the program and authority immutably; it is dropped
            // (its continuation moved out into the Suspension) before the protocol
            // needs `&mut self`.
            let outcome = {
                let mut m = match next {
                    Next::Start(t, e) => Machine::new(&self.program, &self.grants, t).with_env(e),
                    Next::Resume(frames, value, base) => {
                        Machine::resume_frames(&self.program, &self.grants, frames, value, base)
                    }
                };
                m.run().map_err(RuntimeError::Eval)?
            };
            match outcome {
                Outcome::Value(v) => return self.finish(v),
                Outcome::WindowReturn(_) => {
                    return Err(RuntimeError::Refused(
                        "window sentinel resumed outside a clause window".into(),
                    ))
                }
                Outcome::Suspended(susp) => {
                    let (frames, result, base) = self.serve(susp)?;
                    next = Next::Resume(frames, result, base);
                }
                Outcome::ForeignCall { name, .. } => {
                    // Foreign invocation is legal only inside a suspending handler
                    // clause, whose execution the runtime drives itself (unification
                    // §3); reaching the top-level drive loop is a program error.
                    return Err(RuntimeError::Refused(format!(
                        "foreign application {name} outside a suspending handler clause"
                    )));
                }
            }
        }
    }

    fn finish(&mut self, v: Value) -> Result<Value, RuntimeError> {
        let codec = Codec::new(&self.program, &self.sigs, &self.grants);
        let bytes = codec.encode_value(&v)?;
        self.store.commit_final(&bytes)?;
        self.check_invariants()?;
        Ok(v)
    }

    /// Serve one suspension: machine-local effect (driver only, unjournalled) or the
    /// §7.1 five-step protocol. Returns (frames, result, id_base) for step 5.
    fn serve(
        &mut self,
        susp: Suspension,
    ) -> Result<(Vec<entoli_core::Frame>, Value, String), RuntimeError> {
        if susp.effect == LOCAL_EFFECT || susp.effect == PROTOCOL_EFFECT {
            if !self.driver_active {
                return Err(RuntimeError::Refused(format!(
                    "machine-local effect {}.{} outside the recovery driver",
                    susp.effect, susp.op
                )));
            }
            // Review finding R-2: the privilege must not survive the hand-back of
            // control to the program. Durable continuation ids are "k<journal id>";
            // recovery-action continuations are children (their id contains a dot).
            let nearest_durable = susp.frames.iter().rev().find_map(|f| match f {
                Frame::ResumeBoundary { kont_id } if kont_id.starts_with('k') => {
                    Some(kont_id.clone())
                }
                _ => None,
            });
            if let Some(kid) = nearest_durable {
                if !kid[1..].contains('.') {
                    return Err(RuntimeError::Refused(format!(
                        "machine-local effect {}.{} after control returned to the program",
                        susp.effect, susp.op
                    )));
                }
            }
            let result = self.serve_local(&susp)?;
            return Ok((susp.frames, result, "r".into()));
        }
        let result = self.protocol(&susp)?;
        // Step 5 · Resume (V). Affinity guarantees at-most-once per run.
        self.crash.hit(&susp.effect, &susp.op, CrashPoint::BeforeResume);
        Ok((susp.frames, result, "0".into()))
    }

    /// Steps 2–3 under the unified reading (unification §5): execute the suspending
    /// handler's clause as the crash window. The continuation the clause resumes is a
    /// sentinel with no frames, so the clause's tail `resume k r` terminates the
    /// sub-machine with r — and a clause that completes WITHOUT consuming it violated
    /// all-paths-resume (D-1's dynamic proxy). Effects are forbidden inside the window
    /// (empty-own-row, dynamically); foreign calls are served from the bindings table.
    fn execute_clause(
        &mut self,
        handler: &entoli_core::SuspendHandler,
        arg: Value,
        witness: Option<&str>,
        effect: &str,
        op: &str,
    ) -> Result<Value, RuntimeError> {
        let sentinel = entoli_core::new_kont(entoli_core::machine_window_id(), vec![]);
        let clause_env = handler
            .env
            .bind(handler.clause.param.clone(), arg)
            .bind(handler.clause.kont.clone(), Value::Kont(sentinel.clone()));
        enum Next {
            Start(Rc<Term>, entoli_core::Env),
            Resume(Vec<Frame>, Value),
        }
        let mut next = Next::Start(handler.clause.body.clone(), clause_env);
        loop {
            let outcome = {
                let mut m = match next {
                    Next::Start(t, e) => {
                        Machine::new(&self.program, &self.grants, t).with_env(e)
                    }
                    Next::Resume(frames, value) => Machine::resume_frames(
                        &self.program,
                        &self.grants,
                        frames,
                        value,
                        "w".into(),
                    ),
                };
                m.run().map_err(RuntimeError::Eval)?
            };
            match outcome {
                Outcome::WindowReturn(v) => return Ok(v),
                Outcome::Value(_) if sentinel.borrow().consumed => {
                    // P-5: the sentinel was resumed mid-body and the body kept
                    // computing — a non-tail resume in a durable window. Refuse
                    // BEFORE Commit: the entry stays Pending, recovery re-executes.
                    return Err(RuntimeError::Refused(format!(
                        "E004: non-tail resume in the durable window of {effect}.{op}; \
                         nothing may run after the clause's resume"
                    )));
                }
                Outcome::Value(_) => {
                    return Err(RuntimeError::Refused(format!(
                        "suspending clause {effect}.{op} completed without resuming                          (all-paths-resume, unification §4)"
                    )));
                }
                Outcome::ForeignCall { name, arg, frames } => {
                    let binding = self.foreigns.get_mut(&name).ok_or_else(|| {
                        RuntimeError::Refused(format!("unbound foreign invoker {name}"))
                    })?;
                    let r = binding
                        .ext
                        .invoke(&binding.op, &arg, witness, &self.crash)
                        .map_err(|msg| RuntimeError::External {
                            effect: binding.effect.clone(),
                            op: binding.op.clone(),
                            msg,
                        })?;
                    next = Next::Resume(frames, r);
                }
                Outcome::Suspended(s) => {
                    return Err(RuntimeError::Refused(format!(
                        "suspending clause {effect}.{op} performed effect {}.{} inside                          the crash window (empty-own-row, unification §4)",
                        s.effect, s.op
                    )))
                }
            }
        }
    }

    /// Steps 1–4 of §7.1 for a fresh operation.
    fn protocol(&mut self, susp: &Suspension) -> Result<Value, RuntimeError> {
        let codec = Codec::new(&self.program, &self.sigs, &self.grants);

        self.crash.hit(&susp.effect, &susp.op, CrashPoint::BeforeIntend);

        // Step 1 · Intend (D): continuation and intent, one transaction, before any
        // external contact. Encoding is pure (Rule 2); the id is derived from σ.
        let (journal_id, parent) = self.alloc_journal_id()?;
        let continuation_id = format!("k{journal_id}");
        let encoded_frames = codec.encode_kont(&susp.frames)?;
        let (_, handled_sigs) = codec.kont_refs(&susp.frames);
        let mut handled_effects: Vec<(String, [u8; 32], String)> = handled_sigs
            .into_iter()
            .map(|(e, sig)| {
                let compat = self.compat_token(&e);
                (e, sig, compat)
            })
            .collect();
        // The suspended operation's own effect is part of handled(K) even when the
        // remaining frames no longer mention it: recovery will RE-INVOKE it, and the
        // §10.2 merchant case is exactly a semantic change to that handler.
        if !handled_effects.iter().any(|(e, _, _)| e == &susp.effect) {
            if let Some(decl) = self.program.effects.get(&susp.effect) {
                handled_effects.push((
                    susp.effect.clone(),
                    entoli_encode::effect_sig(decl, &self.sigs),
                    self.compat_token(&susp.effect),
                ));
            }
        }
        let encoded_arg = codec.encode_value(&susp.arg)?;
        // F-011: the delivery class in force at Intend is recorded in the entry;
        // an operation with no declared class cannot be journalled (T-Op's
        // fourth row, made dynamic at the boundary).
        let delivery = self
            .program
            .effects
            .get(&susp.effect)
            .and_then(|d| d.op(&susp.op))
            .map(|o| delivery_str(&o.delivery))
            .ok_or_else(|| {
                RuntimeError::Refused(format!(
                    "cannot journal {}.{}: no declared delivery class",
                    susp.effect, susp.op
                ))
            })?;
        let entry = JournalEntry {
            journal_id: journal_id.clone(),
            parent_journal_id: parent,
            effect: susp.effect.clone(),
            op: susp.op.clone(),
            delivery,
            encoded_arg,
            witness: susp.witness.clone(),
            continuation_id: continuation_id.clone(),
            state: EntryState::Pending,
            seq: 0,
        };
        let kont = DurableContinuation {
            continuation_id,
            encoded_frames,
            handled_effects,
            data_classification: classify_frames(&susp.frames),
        };
        // Handler path (unification §5): the installation's κ must be live at Intend,
        // and budgeted policy is spent per invocation here.
        if let Some(h) = &susp.handler {
            if self.strict_contracts
                && !self.contracts_passed.contains(&(susp.effect.clone(), susp.op.clone()))
            {
                return Err(RuntimeError::Refused(format!(
                    "strict mode: delivery contract of {}.{} is untested or failed",
                    susp.effect, susp.op
                )));
            }
            match self.grants.effect_of(h.cap) {
                Some(e) if e == susp.effect => {}
                _ => {
                    return Err(RuntimeError::Refused(format!(
                        "E002: capability for {} is not a live grant at suspension",
                        susp.effect
                    )))
                }
            }
            self.grants
                .spend(&susp.effect)
                .map_err(|m| RuntimeError::Refused(format!("E002: {m}")))?;
        }

        self.store.intend(&entry, &kont)?;
        self.check_invariants()?;

        self.crash.hit(&susp.effect, &susp.op, CrashPoint::AfterIntend);

        if let Some(h) = &susp.handler {
            // Steps 2–3: the clause is the crash window.
            let h = entoli_core::SuspendHandler {
                clause: h.clause.clone(),
                env: h.env.clone(),
                cap: h.cap,
            };
            let result =
                self.execute_clause(&h, susp.arg.clone(), susp.witness.as_deref(), &susp.effect, &susp.op)?;
            // Step 3→4.
            self.crash.hit(&susp.effect, &susp.op, CrashPoint::AfterReceiveBeforeCommit);
            let codec = Codec::new(&self.program, &self.sigs, &self.grants);
            let bytes = codec.encode_value(&result)?;
            drop(codec);
            self.store.commit_done(&journal_id, &bytes)?;
            self.check_invariants()?;
            self.crash.hit(&susp.effect, &susp.op, CrashPoint::AfterCommit);
            return Ok(result);
        }

        // M7: no registry remains — a suspending operation with no installed handler
        // is an unhandled effect, exactly as for prompt effects (unification §5).
        Err(RuntimeError::Eval(EntoliError::UnhandledEffect {
            effect: susp.effect.clone(),
            op: susp.op.clone(),
        }))
    }

    /// Journal ids are derived from σ: children of the top pending entry during
    /// recovery (that is what makes the pending stack a stack), roots otherwise.
    fn alloc_journal_id(&self) -> Result<(String, Option<String>), RuntimeError> {
        // Indexed COUNTs, not a journal scan (T3.1 / A-06, A-17).
        match self.store.pending_top()? {
            Some(top) => {
                let n = self.store.count_children(Some(&top.journal_id))?;
                Ok((format!("{}.{n}", top.journal_id), Some(top.journal_id)))
            }
            None => {
                let n = self.store.count_children(None)?;
                Ok((format!("{n}"), None))
            }
        }
    }

    // -- machine-local effects (plan §8): pure reads of σ + Protocol.complete ----

    fn serve_local(&mut self, susp: &Suspension) -> Result<Value, RuntimeError> {
        let codec = Codec::new(&self.program, &self.sigs, &self.grants);
        let jid = |v: &Value| -> Result<String, RuntimeError> {
            match v {
                Value::Str(s) => Ok(s.clone()),
                other => Err(RuntimeError::Refused(format!("expected journal id, got {other:?}"))),
            }
        };
        match (susp.effect.as_str(), susp.op.as_str()) {
            (LOCAL_EFFECT, "frontier") => {
                let frontier = self.store.frontier()?;
                Ok(match frontier {
                    None => {
                        // P-4: the none-arm runs the program entry point directly,
                        // with no resume boundary for the mask to cut at. Nothing
                        // machine-local is ever legitimately needed after a `none`
                        // dispatch, so the privilege ends here.
                        self.driver_active = false;
                        Value::Ctor { name: "none".into(), args: vec![] }
                    }
                    Some(e) => {
                        let j = Value::Str(e.journal_id.clone());
                        match &e.state {
                            EntryState::Done(_) => {
                                Value::Ctor { name: "done".into(), args: vec![j] }
                            }
                            EntryState::Pending => {
                                // F-011: dispatch on the RECORDED class — a function
                                // of σ alone. The program in force must agree; drift
                                // is a refusal (recoverable), never a different
                                // dispatch, and it is checked here, BEFORE any
                                // compensation could intend a child.
                                let current = self
                                    .program
                                    .effects
                                    .get(&e.effect)
                                    .and_then(|d| d.op(&e.op))
                                    .map(|o| delivery_str(&o.delivery));
                                if current.as_deref() != Some(e.delivery.as_str()) {
                                    return Err(RuntimeError::Refused(format!(
                                        "E012: recovery dispatch drift on {}.{} — recorded delivery at Intend was '{}', the program in force {}; entry {} stays pending",
                                        e.effect,
                                        e.op,
                                        e.delivery,
                                        match &current {
                                            Some(c) => format!("declares '{c}'"),
                                            None => "no longer declares it".into(),
                                        },
                                        e.journal_id
                                    )));
                                }
                                if e.delivery == "idempotent" || e.delivery == "at_least_once" {
                                    Value::Ctor { name: "pending_re".into(), args: vec![j] }
                                } else if let Some(rest) = e.delivery.strip_prefix("compensable:") {
                                    Value::Ctor {
                                        name: format!("pending_comp_{}", rest.replace('.', "_")),
                                        args: vec![j],
                                    }
                                } else {
                                    return Err(RuntimeError::Refused(format!(
                                        "malformed recorded delivery '{}' on entry {}",
                                        e.delivery, e.journal_id
                                    )));
                                }
                            }
                        }
                    }
                })
            }
            (LOCAL_EFFECT, "load_kont") => {
                let j = jid(&susp.arg)?;
                let entry = self
                    .store
                    .entry(&j)?
                    .ok_or_else(|| RuntimeError::Refused(format!("no entry {j}")))?;
                let dk = self
                    .store
                    .continuation(&entry.continuation_id)?
                    .ok_or_else(|| RuntimeError::Refused(format!("no continuation for {j}")))?;
                // Affinity check first, but the per-run slot is CONSUMED only after
                // the guards pass (metatheory round 3): a refused materialisation —
                // quarantine or drift — must stay re-runnable within the run, so a
                // refusal may not spend the affine slot.
                if self.loaded_konts.contains(&dk.continuation_id) {
                    return Err(RuntimeError::Refused(format!(
                        "continuation {} already materialised in this run (affine)",
                        dk.continuation_id
                    )));
                }
                if let Some(reason) = self.store.quarantine_of(&dk.continuation_id)? {
                    return Err(RuntimeError::Refused(format!(
                        "continuation {} is quarantined: {reason}",
                        dk.continuation_id
                    )));
                }
                // §10.2: both halves are required before a sleeping continuation may
                // resume — sig(h.E) = sig(captured.E) catches structural drift (E012),
                // compat(h.E) = compat(captured.E) catches semantic drift (E013).
                // A mismatch quarantines rather than loses the continuation.
                for (eff, stored_sig, stored_compat) in &dk.handled_effects {
                    let current_sig = self.program.effects.get(eff).map(|d| entoli_encode::effect_sig(d, &self.sigs));
                    let current_compat = self.compat_token(eff);
                    let reason = if current_sig != Some(*stored_sig) {
                        Some(format!("E012: effect signature mismatch on resume for {eff}"))
                    } else if current_compat != *stored_compat {
                        Some(format!(
                            "E013: compatibility-token mismatch on resume for {eff} \
                             (captured {stored_compat:?}, current {current_compat:?})"
                        ))
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        drop(codec);
                        self.store.quarantine(&dk.continuation_id, &reason)?;
                        return Err(RuntimeError::Refused(format!(
                            "{reason}; continuation {} quarantined",
                            dk.continuation_id
                        )));
                    }
                }
                let frames = codec.decode_kont(&dk.encoded_frames, &self.grants)?;
                self.loaded_konts.insert(dk.continuation_id.clone());
                Ok(Value::Kont(entoli_core::new_kont(dk.continuation_id, frames)))
            }
            (LOCAL_EFFECT, "load_result") => {
                let j = jid(&susp.arg)?;
                let entry = self
                    .store
                    .entry(&j)?
                    .ok_or_else(|| RuntimeError::Refused(format!("no entry {j}")))?;
                match &entry.state {
                    EntryState::Done(bytes) => Ok(codec.decode_value(bytes, &self.grants)?),
                    EntryState::Pending => {
                        Err(RuntimeError::Refused(format!("entry {j} is pending, not done")))
                    }
                }
            }
            (LOCAL_EFFECT, "comp_arg") => {
                let j = jid(&susp.arg)?;
                let entry = self
                    .store
                    .entry(&j)?
                    .ok_or_else(|| RuntimeError::Refused(format!("no entry {j}")))?;
                Ok(codec.decode_value(&entry.encoded_arg, &self.grants)?)
            }
            (PROTOCOL_EFFECT, "complete") => {
                // Re-invocation of an already-intended entry: steps 2–4 with the stored
                // operation, argument, and witness (same ι — §8.1's at-least-once row).
                let j = jid(&susp.arg)?;
                let entry = self
                    .store
                    .entry(&j)?
                    .ok_or_else(|| RuntimeError::Refused(format!("no entry {j}")))?;
                if let EntryState::Done(bytes) = &entry.state {
                    // Idempotent completion (review finding R-1): a crash between
                    // step 4 and step 5 leaves [.., Done(parent), Done(child)] with the
                    // child's continuation still holding `complete(parent)` — which
                    // must satisfy its own declared equation op ; op ≈ op by returning
                    // the stored result, never by refusing.
                    return Ok(codec.decode_value(bytes, &self.grants)?);
                }
                let arg = codec.decode_value(&entry.encoded_arg, &self.grants)?;
                drop(codec);
                // Unification §5: re-invocation re-executes the clause of the
                // journaled operation under the installation in force at recovery —
                // the innermost matching Handle frame in the DRIVER's own stack (the
                // prelude). Registry fallback until M7.
                let handler = susp.frames.iter().rev().find_map(|f| match f {
                    Frame::Handle { handler, cap, env }
                        if handler.effect == entry.effect
                            && handler.clause(&entry.op).is_some() =>
                    {
                        Some(entoli_core::SuspendHandler {
                            clause: handler.clause(&entry.op).unwrap().clone(),
                            env: env.clone(),
                            cap: *cap,
                        })
                    }
                    _ => None,
                });
                if let Some(h) = handler {
                    // F-009: strict mode gates BOTH invocation sites — a fresh
                    // Intend (protocol) and recovery re-invocation here. Refusing
                    // leaves the entry Pending: recoverable once the contract is
                    // proven, never a wedge. Checked before any budget spend.
                    if self.strict_contracts
                        && !self
                            .contracts_passed
                            .contains(&(entry.effect.clone(), entry.op.clone()))
                    {
                        return Err(RuntimeError::Refused(format!(
                            "strict mode: delivery contract of {}.{} is untested or failed; \
                             recovery re-invocation refused, entry {j} stays pending",
                            entry.effect, entry.op
                        )));
                    }
                    match self.grants.effect_of(h.cap) {
                        Some(e) if e == entry.effect => {}
                        _ => {
                            return Err(RuntimeError::Refused(format!(
                                "E002: capability for {} is not a live grant at recovery",
                                entry.effect
                            )))
                        }
                    }
                    self.grants
                        .spend(&entry.effect)
                        .map_err(|m| RuntimeError::Refused(format!("E002: {m}")))?;
                    let result = self.execute_clause(
                        &h,
                        arg,
                        entry.witness.as_deref(),
                        &entry.effect,
                        &entry.op,
                    )?;
                    self.crash.hit(&entry.effect, &entry.op, CrashPoint::AfterReceiveBeforeCommit);
                    let codec = Codec::new(&self.program, &self.sigs, &self.grants);
                    let bytes = codec.encode_value(&result)?;
                    drop(codec);
                    self.store.commit_done(&j, &bytes)?;
                    self.check_invariants()?;
                    self.crash.hit(&entry.effect, &entry.op, CrashPoint::AfterCommit);
                    return Ok(result);
                }
                Err(RuntimeError::Eval(EntoliError::UnhandledEffect {
                    effect: entry.effect.clone(),
                    op: entry.op.clone(),
                }))
            }
            (e, o) => Err(RuntimeError::Refused(format!("unknown machine-local op {e}.{o}"))),
        }
    }
}

/// The program extended with the machine-local effect declarations the recovery driver
/// performs. Suspending class: they yield to the drive loop; they are simply exempt
/// from journalling (plan §8).
pub fn entoli_recovery_decls(program: &Program) -> Program {
    use entoli_syntax::{EffectDecl, OpDecl, SuspensionClass};
    let mut p = program.clone();
    p.effects.insert(
        LOCAL_EFFECT.into(),
        EffectDecl {
            name: LOCAL_EFFECT.into(),
            class: SuspensionClass::Suspending,
            ops: ["frontier", "load_kont", "load_result", "comp_arg"]
                .into_iter()
                .map(|n| OpDecl { name: n.into(), delivery: DeliveryClass::Idempotent })
                .collect(),
        },
    );
    p.effects.insert(
        PROTOCOL_EFFECT.into(),
        EffectDecl {
            name: PROTOCOL_EFFECT.into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "complete".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p
}
