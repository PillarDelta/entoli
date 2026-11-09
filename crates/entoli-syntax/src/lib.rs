//! ENTOLI core syntax (paper v0.5 §4.1), as pure data.
//!
//! Nothing in this crate may hold a host closure, pointer, or handle: the AST must be
//! canonically encodable byte-for-byte (paper §10.1) and safe to reference from frames.

use std::collections::BTreeMap;
use std::rc::Rc;

pub type EffectName = String;
pub type OpName = String;

/// Suspension class `s` (§4.1): suspending ops may yield to the durable machine,
/// prompt ops are discharged by in-language handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspensionClass {
    Suspending,
    Prompt,
}

/// Delivery class `δ` (§4.1). Mandatory on every operation: T-Op admits no operation
/// without one, which is what empties the fourth recovery row (§8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryClass {
    Idempotent,
    /// Requires an idempotency witness at every `do` site.
    AtLeastOnce,
    /// Recovery: perform the named compensation op, then re-invoke (§8.1).
    Compensable {
        comp_effect: EffectName,
        comp_op: OpName,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpDecl {
    pub name: OpName,
    pub delivery: DeliveryClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDecl {
    pub name: EffectName,
    pub class: SuspensionClass,
    pub ops: Vec<OpDecl>,
}

impl EffectDecl {
    pub fn op(&self, name: &str) -> Option<&OpDecl> {
        self.ops.iter().find(|o| o.name == name)
    }
}

/// Primitive operations, enough for the demonstration programs. Pure, total on the
/// value forms the evaluator feeds them (dynamic error otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimOp {
    Add,
    Sub,
    Mul,
    Lt,
    IntEq,
    StrEq,
    Concat,
    StrOfInt,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `.name(x1, ..., xn)` — matches a constructor, binding its fields.
    Ctor { name: String, binders: Vec<String> },
    /// Binds anything.
    Any(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pat: Pattern,
    pub body: Term,
}

/// One handler: `{ return x -> e ; E.op(x; k) -> e , ... }` for a single effect E.
#[derive(Debug, Clone, PartialEq)]
pub struct Handler {
    pub effect: EffectName,
    /// `return x -> e`; identity when absent.
    pub return_clause: Option<(String, Rc<Term>)>,
    pub clauses: Vec<OpClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpClause {
    pub op: OpName,
    pub param: String,
    pub kont: String,
    pub body: Rc<Term>,
}

impl Handler {
    pub fn clause(&self, op: &str) -> Option<&OpClause> {
        self.clauses.iter().find(|c| c.op == op)
    }
}

/// Expressions `e` (§4.1), extended with the data plumbing the demos need
/// (constructors, match, primitives, literals).
#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Var(String),
    Unit,
    Int(i64),
    Bool(bool),
    Str(String),
    Lam {
        param: String,
        body: Rc<Term>,
    },
    App {
        func: Rc<Term>,
        arg: Rc<Term>,
    },
    Let {
        name: String,
        bound: Rc<Term>,
        body: Rc<Term>,
    },
    If {
        cond: Rc<Term>,
        then: Rc<Term>,
        els: Rc<Term>,
    },
    /// `do E.op(arg)`; `witness` is the idempotency witness `ι`, mandatory for
    /// at-least-once operations (T-Op's deliv side condition, dynamically enforced).
    Do {
        effect: EffectName,
        op: OpName,
        arg: Rc<Term>,
        witness: Option<Rc<Term>>,
    },
    /// `handle body with { handler } using cap` (§4.2 T-Handle).
    Handle {
        body: Rc<Term>,
        handler: Rc<Handler>,
        cap: Rc<Term>,
    },
    /// `resume k v` — affine (§5).
    Resume {
        kont: Rc<Term>,
        arg: Rc<Term>,
    },
    Ctor {
        name: String,
        args: Vec<Term>,
    },
    Match {
        scrut: Rc<Term>,
        arms: Rc<Vec<Arm>>,
    },
    Prim {
        op: PrimOp,
        args: Vec<Term>,
    },
    /// Reference to a top-level definition, resolved through the program's
    /// content-addressed definition table (§10.1).
    Global(String),
    /// Application of a foreign invoker (v0.6 unification design §3): the only
    /// world-contact form, legal only inside a suspending handler clause. Second-class
    /// by design — the name refers to a top-level `foreign` declaration.
    ForeignApp {
        name: String,
        arg: Rc<Term>,
    },
}

/// A whole program: content-addressed top-level definitions plus effect declarations.
/// BTreeMaps for deterministic iteration (hashing depends on it).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub defs: BTreeMap<String, Rc<Term>>,
    pub effects: BTreeMap<EffectName, EffectDecl>,
    /// Foreign invoker declarations (unification design §3): closed, host-bound,
    /// referenced by name; part of the program's content identity.
    pub foreigns: std::collections::BTreeSet<String>,
}

impl Program {
    /// Declaration-load validation (plan §12 E011 among others):
    /// a compensation may not itself be compensable (§8.2), the compensation target
    /// must exist, and every referenced effect declaration must be well-formed.
    pub fn validate(&self) -> Result<(), String> {
        for eff in self.effects.values() {
            for op in &eff.ops {
                if let DeliveryClass::Compensable { comp_effect, comp_op } = &op.delivery {
                    let target = self
                        .effects
                        .get(comp_effect)
                        .and_then(|e| e.op(comp_op))
                        .ok_or_else(|| {
                            format!(
                                "compensation target {comp_effect}.{comp_op} of {}.{} is not declared",
                                eff.name, op.name
                            )
                        })?;
                    if matches!(target.delivery, DeliveryClass::Compensable { .. }) {
                        return Err(format!(
                            "E011: compensation {comp_effect}.{comp_op} (for {}.{}) is itself compensable (§8.2)",
                            eff.name, op.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Builder helpers so demo programs read close to the paper's surface examples.
pub mod build {
    use super::*;

    pub fn var(n: &str) -> Term {
        Term::Var(n.into())
    }
    pub fn unit() -> Term {
        Term::Unit
    }
    pub fn int(i: i64) -> Term {
        Term::Int(i)
    }
    pub fn str_(s: &str) -> Term {
        Term::Str(s.into())
    }
    pub fn boolean(b: bool) -> Term {
        Term::Bool(b)
    }
    pub fn lam(p: &str, b: Term) -> Term {
        Term::Lam { param: p.into(), body: Rc::new(b) }
    }
    pub fn app(f: Term, a: Term) -> Term {
        Term::App { func: Rc::new(f), arg: Rc::new(a) }
    }
    pub fn let_(n: &str, bound: Term, body: Term) -> Term {
        Term::Let { name: n.into(), bound: Rc::new(bound), body: Rc::new(body) }
    }
    pub fn if_(c: Term, t: Term, e: Term) -> Term {
        Term::If { cond: Rc::new(c), then: Rc::new(t), els: Rc::new(e) }
    }
    pub fn do_(effect: &str, op: &str, arg: Term) -> Term {
        Term::Do { effect: effect.into(), op: op.into(), arg: Rc::new(arg), witness: None }
    }
    pub fn do_w(effect: &str, op: &str, arg: Term, witness: Term) -> Term {
        Term::Do {
            effect: effect.into(),
            op: op.into(),
            arg: Rc::new(arg),
            witness: Some(Rc::new(witness)),
        }
    }
    pub fn handle(body: Term, handler: Handler, cap: Term) -> Term {
        Term::Handle { body: Rc::new(body), handler: Rc::new(handler), cap: Rc::new(cap) }
    }
    pub fn resume(k: Term, v: Term) -> Term {
        Term::Resume { kont: Rc::new(k), arg: Rc::new(v) }
    }
    pub fn ctor(name: &str, args: Vec<Term>) -> Term {
        Term::Ctor { name: name.into(), args }
    }
    pub fn match_(scrut: Term, arms: Vec<Arm>) -> Term {
        Term::Match { scrut: Rc::new(scrut), arms: Rc::new(arms) }
    }
    pub fn arm_ctor(name: &str, binders: &[&str], body: Term) -> Arm {
        Arm {
            pat: Pattern::Ctor {
                name: name.into(),
                binders: binders.iter().map(|s| s.to_string()).collect(),
            },
            body,
        }
    }
    pub fn arm_any(binder: &str, body: Term) -> Arm {
        Arm { pat: Pattern::Any(binder.into()), body }
    }
    pub fn prim(op: PrimOp, args: Vec<Term>) -> Term {
        Term::Prim { op, args }
    }
    pub fn global(n: &str) -> Term {
        Term::Global(n.into())
    }
    pub fn foreign_app(name: &str, arg: Term) -> Term {
        Term::ForeignApp { name: name.into(), arg: Rc::new(arg) }
    }
    pub fn clause(op: &str, param: &str, kont: &str, body: Term) -> OpClause {
        OpClause { op: op.into(), param: param.into(), kont: kont.into(), body: Rc::new(body) }
    }
}
