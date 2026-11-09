use crate::env::Env;
use entoli_syntax::{Arm, Handler, PrimOp, SuspensionClass, Term};
use std::cell::RefCell;
use std::rc::Rc;

/// Opaque capability handle. The value carries no policy, budget, or credential — those
/// live in the runtime's grant table, keyed by this id. `entoli-encode` deliberately has
/// no arm that serialises the id (Rule 4 obtained structurally, plan §3.4): a capability
/// crossing a durable boundary encodes as a `CapSlot` naming only its effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapId(pub u64);

/// Who says what a capability currently grants. The machine consults this at handler
/// installation (T-Handle's third premise, dynamically).
pub trait CapAuthority {
    /// The effect this capability grants right now, or None if unknown/revoked.
    fn effect_of(&self, cap: CapId) -> Option<String>;
}

/// Simple map-backed authority for tests and pure runs.
#[derive(Debug, Default, Clone)]
pub struct MapAuthority {
    pub grants: std::collections::BTreeMap<CapId, String>,
}

impl MapAuthority {
    pub fn grant(&mut self, cap: CapId, effect: &str) {
        self.grants.insert(cap, effect.to_string());
    }
}

impl CapAuthority for MapAuthority {
    fn effect_of(&self, cap: CapId) -> Option<String> {
        self.grants.get(&cap).cloned()
    }
}

/// Affine continuation cell: uniquely-owned frame stack plus the consumed flag that is
/// the dynamic proxy for affinity (§5, plan §3.4).
#[derive(Debug, PartialEq)]
pub struct KontCell {
    pub id: String,
    pub consumed: bool,
    pub frames: Vec<Frame>,
}

pub type Kont = Rc<RefCell<KontCell>>;

pub fn new_kont(id: String, frames: Vec<Frame>) -> Kont {
    Rc::new(RefCell::new(KontCell { id, consumed: false, frames }))
}

/// Runtime values. `Closure` captures data (a term and an environment), never a host
/// function. `Cap` is the only value the encoder refuses to embed.
#[derive(Debug, Clone)]
pub enum Value {
    Unit,
    Int(i64),
    Bool(bool),
    Str(String),
    Ctor { name: String, args: Vec<Value> },
    Closure { param: String, body: Rc<Term>, env: Env },
    Kont(Kont),
    Cap(CapId),
    /// An idempotency witness `ι` — a first-class value so programs can thread it.
    Witness(String),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Unit, Unit) => true,
            (Int(a), Int(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (Witness(a), Witness(b)) => a == b,
            (Ctor { name: n1, args: a1 }, Ctor { name: n2, args: a2 }) => n1 == n2 && a1 == a2,
            (
                Closure { param: p1, body: b1, env: e1 },
                Closure { param: p2, body: b2, env: e2 },
            ) => p1 == p2 && b1 == b2 && e1 == e2,
            (Kont(a), Kont(b)) => Rc::ptr_eq(a, b),
            (Cap(a), Cap(b)) => a == b,
            _ => false,
        }
    }
}

/// Continuation frames (plan §7.1's Frame grammar, plus the evaluation-order frames a
/// small-step machine needs). Everything here is pure data: encodable except for the
/// live `CapId` in `Handle`, which encodes as its effect name only.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// Evaluating the function position of an application; `arg` pending.
    AppFn { arg: Rc<Term>, env: Env },
    /// Function evaluated; evaluating the argument.
    AppArg { func: Value },
    Let { name: String, body: Rc<Term>, env: Env },
    If { then: Rc<Term>, els: Rc<Term>, env: Env },
    /// Evaluating `do E.op`'s argument.
    DoArg { effect: String, op: String, witness: Option<Rc<Term>>, env: Env },
    /// Argument evaluated; evaluating the witness expression.
    DoWitness { effect: String, op: String, arg: Value },
    /// Evaluating the capability expression of `handle ... using κ`.
    HandleCap { body: Rc<Term>, handler: Rc<Handler>, env: Env },
    /// Installed handler — the delimiter. Captured *into* continuations (deep handlers).
    Handle { handler: Rc<Handler>, cap: CapId, env: Env },
    /// Marks a handler clause body in progress: pushed at capture, popped at tail
    /// resume or clause return. Its liveness is the dynamic proxy for §4.3 (E004/E005).
    ClauseBoundary { effect: String, op: String, class: SuspensionClass },
    /// Marks a splice point where a continuation was resumed (plan §7.1). Transparent.
    ResumeBoundary { kont_id: String },
    /// Evaluating `resume`'s continuation expression; the value argument pending.
    ResumeKArg { arg: Rc<Term>, env: Env },
    /// Continuation evaluated; evaluating the value argument.
    ResumeApply { kont: Value },
    /// Evaluating a foreign application's argument (unification design §3). The call
    /// itself is executed by the runtime — the machine stays pure.
    ForeignArg { name: String },
    CtorArgs { name: String, done: Vec<Value>, rest: Vec<Term>, env: Env },
    PrimArgs { op: PrimOp, done: Vec<Value>, rest: Vec<Term>, env: Env },
    Match { arms: Rc<Vec<Arm>>, env: Env },
}
