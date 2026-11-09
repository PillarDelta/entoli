//! Phase 5: types, effect rows, and signatures (paper §4.1–§4.2).
//!
//! v1 scope, recorded honestly: no row polymorphism and no inference across
//! unannotated lambdas (an unannotated parameter is `Data`). Top-level definitions
//! carry declared types; the checker is bidirectional-lite. `Data` is the single
//! algebraic type of constructor values — constructor discipline stays dynamic in v1,
//! effect and capability discipline is static (which is what the falsification
//! criterion needs: unrecovered effects, unserialisable captures, unreached tools).

use std::collections::BTreeSet;
use std::fmt;

/// The `{AtLeastOnce}` marker of T-Op's `deliv(δ)`: visible in the row until an
/// idempotency witness discharges it, so weak delivery propagates to every caller
/// (paper §4.2 — "the composition property of §2.2 in its concrete form").
pub const AT_LEAST_ONCE: &str = "#AtLeastOnce";

/// An effect row ε: a set of effect names, possibly carrying the AtLeastOnce marker.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row(pub BTreeSet<String>);

impl Row {
    pub fn empty() -> Row {
        Row(BTreeSet::new())
    }
    pub fn one(e: &str) -> Row {
        Row(BTreeSet::from([e.to_string()]))
    }
    pub fn union(&self, other: &Row) -> Row {
        Row(self.0.union(&other.0).cloned().collect())
    }
    pub fn insert(&mut self, e: &str) {
        self.0.insert(e.to_string());
    }
    pub fn without(&self, e: &str) -> Row {
        let mut s = self.0.clone();
        s.remove(e);
        Row(s)
    }
    pub fn minus(&self, other: &Row) -> Row {
        Row(self.0.difference(&other.0).cloned().collect())
    }
    pub fn contains(&self, e: &str) -> bool {
        self.0.contains(e)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// ε ⊆ ε' — the check `handle`/annotations use to discharge or propagate.
    pub fn subset_of(&self, other: &Row) -> bool {
        self.0.is_subset(&other.0)
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}", self.0.iter().cloned().collect::<Vec<_>>().join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Int,
    Bool,
    Str,
    /// The algebraic data universe (constructor values); dynamic in v1.
    Data,
    /// τ1 -[ε]-> τ2
    Fn(Box<Type>, Row, Box<Type>),
    /// Cap[E] — policy π arrives in Phase 6.
    Cap(String),
    /// τ2 -[εk]-o τ' — the affine continuation type with its latent row (§4.2,
    /// blocker 3's repair).
    Kont(Box<Type>, Row, Box<Type>),
}

impl Type {
    pub fn is_affine(&self) -> bool {
        matches!(self, Type::Kont(..))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Unit => write!(f, "Unit"),
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::Str => write!(f, "Str"),
            Type::Data => write!(f, "Data"),
            Type::Fn(a, r, b) => write!(f, "({a} -[{r}]-> {b})"),
            Type::Cap(e) => write!(f, "Cap[{e}]"),
            Type::Kont(a, r, b) => write!(f, "({a} -[{r}]-o {b})"),
        }
    }
}

/// Operation signature: `op : τ1 -> τ2 @δ` (δ already lives in `entoli_syntax::OpDecl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpSig {
    pub arg: Type,
    pub ret: Type,
}

/// Declared types for a program: per-operation signatures and per-definition types.
/// Kept beside the untyped syntax so Phases 1–4 stay untouched (plan §3.4's staging).
#[derive(Debug, Clone, Default)]
pub struct Signatures {
    pub ops: std::collections::BTreeMap<(String, String), OpSig>,
    pub defs: std::collections::BTreeMap<String, Type>,
    /// Foreign invoker signatures: name → (argument, result) (unification §3).
    pub foreigns: std::collections::BTreeMap<String, (Type, Type)>,
}

impl Signatures {
    pub fn op(&self, effect: &str, op: &str) -> OpSig {
        self.ops
            .get(&(effect.to_string(), op.to_string()))
            .cloned()
            .unwrap_or(OpSig { arg: Type::Data, ret: Type::Data })
    }
    pub fn declare_op(&mut self, effect: &str, op: &str, arg: Type, ret: Type) {
        self.ops.insert((effect.into(), op.into()), OpSig { arg, ret });
    }
    pub fn declare_def(&mut self, name: &str, ty: Type) {
        self.defs.insert(name.into(), ty);
    }
}
