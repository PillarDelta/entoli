//! Content addressing (paper §10.1): definitions are named by a SHA-256 Merkle hash over
//! the canonical AST, closed over the identity of everything referenced. Mutually
//! recursive definitions are hashed as an SCC group with name-relative internal
//! references (ADR-0001 §5), so the hash is well-defined in the presence of cycles and
//! still changes when any dependency changes.

use crate::bytes::Writer;
use crate::codec::write_term_canonical;
use entoli_syntax::{DeliveryClass, EffectDecl, Program, Term};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub type Hash256 = [u8; 32];

pub fn sha(bytes: &[u8]) -> Hash256 {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

/// Direct Global references inside a term (only those that resolve to program defs
/// matter for hashing; unknown names surface as dynamic errors at evaluation).
pub fn globals_of(term: &Term, acc: &mut BTreeSet<String>) {
    match term {
        Term::Global(n) => {
            acc.insert(n.clone());
        }
        Term::Var(_) | Term::Unit | Term::Int(_) | Term::Bool(_) | Term::Str(_) => {}
        Term::ForeignApp { arg, .. } => globals_of(arg, acc),
        Term::Lam { body, .. } => globals_of(body, acc),
        Term::App { func, arg } => {
            globals_of(func, acc);
            globals_of(arg, acc);
        }
        Term::Let { bound, body, .. } => {
            globals_of(bound, acc);
            globals_of(body, acc);
        }
        Term::If { cond, then, els } => {
            globals_of(cond, acc);
            globals_of(then, acc);
            globals_of(els, acc);
        }
        Term::Do { arg, witness, .. } => {
            globals_of(arg, acc);
            if let Some(w) = witness {
                globals_of(w, acc);
            }
        }
        Term::Handle { body, handler, cap } => {
            globals_of(body, acc);
            globals_of(cap, acc);
            if let Some((_, e)) = &handler.return_clause {
                globals_of(e, acc);
            }
            for c in &handler.clauses {
                globals_of(&c.body, acc);
            }
        }
        Term::Resume { kont, arg } => {
            globals_of(kont, acc);
            globals_of(arg, acc);
        }
        Term::Ctor { args, .. } | Term::Prim { args, .. } => {
            for a in args {
                globals_of(a, acc);
            }
        }
        Term::Match { scrut, arms } => {
            globals_of(scrut, acc);
            for a in arms.iter() {
                globals_of(&a.body, acc);
            }
        }
    }
}

/// Tarjan's strongly connected components over the def-reference graph, returned in
/// reverse topological order (dependencies before dependents).
fn sccs(defs: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    struct T<'a> {
        graph: &'a BTreeMap<String, BTreeSet<String>>,
        index: BTreeMap<String, (u32, u32)>, // node -> (index, lowlink)
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        counter: u32,
        out: Vec<Vec<String>>,
    }
    impl<'a> T<'a> {
        fn visit(&mut self, v: &str) {
            let idx = self.counter;
            self.counter += 1;
            self.index.insert(v.to_string(), (idx, idx));
            self.stack.push(v.to_string());
            self.on_stack.insert(v.to_string());
            let deps: Vec<String> = self
                .graph
                .get(v)
                .map(|s| s.iter().filter(|d| self.graph.contains_key(*d)).cloned().collect())
                .unwrap_or_default();
            for w in deps {
                if !self.index.contains_key(&w) {
                    self.visit(&w);
                    let wl = self.index[&w].1;
                    let e = self.index.get_mut(v).unwrap();
                    e.1 = e.1.min(wl);
                } else if self.on_stack.contains(&w) {
                    let wi = self.index[&w].0;
                    let e = self.index.get_mut(v).unwrap();
                    e.1 = e.1.min(wi);
                }
            }
            let (vi, vl) = self.index[v];
            if vi == vl {
                let mut comp = Vec::new();
                while let Some(w) = self.stack.pop() {
                    self.on_stack.remove(&w);
                    let done = w == v;
                    comp.push(w);
                    if done {
                        break;
                    }
                }
                comp.sort();
                self.out.push(comp);
            }
        }
    }
    let mut t = T {
        graph: defs,
        index: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        counter: 0,
        out: Vec::new(),
    };
    let names: Vec<String> = defs.keys().cloned().collect();
    for n in &names {
        if !t.index.contains_key(n) {
            t.visit(n);
        }
    }
    t.out
}

/// Per-definition Merkle-closed hashes.
pub fn def_hashes(program: &Program) -> BTreeMap<String, Hash256> {
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, term) in &program.defs {
        let mut refs = BTreeSet::new();
        globals_of(term, &mut refs);
        refs.retain(|r| program.defs.contains_key(r));
        graph.insert(name.clone(), refs);
    }
    let mut hashes: BTreeMap<String, Hash256> = BTreeMap::new();
    for comp in sccs(&graph) {
        let group: BTreeSet<String> = comp.iter().cloned().collect();
        let mut w = Writer::new();
        w.u32(comp.len() as u32);
        for name in &comp {
            w.str(name);
            // Canonical term bytes; Global refs inside the same SCC are written as
            // names (marker 0), refs outside as their already-computed hashes (marker 1).
            write_term_canonical(&mut w, &program.defs[name], &mut |w, g| {
                if group.contains(g) {
                    w.u8(0);
                    w.str(g);
                } else if let Some(h) = hashes.get(g) {
                    w.u8(1);
                    w.raw(h);
                } else {
                    // Reference to a name with no definition: identity is the name.
                    w.u8(2);
                    w.str(g);
                }
            });
        }
        let group_hash = sha(&w.buf);
        for name in &comp {
            let mut w2 = Writer::new();
            w2.raw(&group_hash);
            w2.str(name);
            hashes.insert(name.clone(), sha(&w2.buf));
        }
    }
    hashes
}

/// Canonical structural encoding of a type for hashing — deliberately NOT Display,
/// so a cosmetic formatting change can never masquerade as (or hide) drift.
fn write_type(w: &mut Writer, t: &entoli_types::Type) {
    use entoli_types::Type::*;
    match t {
        Unit => w.u8(0),
        Int => w.u8(1),
        Bool => w.u8(2),
        Str => w.u8(3),
        Data => w.u8(4),
        Fn(a, r, b) => {
            w.u8(5);
            write_type(w, a);
            write_row(w, r);
            write_type(w, b);
        }
        Cap(e) => {
            w.u8(6);
            w.str(e);
        }
        Kont(a, r, b) => {
            w.u8(7);
            write_type(w, a);
            write_row(w, r);
            write_type(w, b);
        }
    }
}

fn write_row(w: &mut Writer, r: &entoli_types::Row) {
    // BTreeSet iteration is sorted — canonical without further work.
    w.u32(r.0.len() as u32);
    for e in r.0.iter() {
        w.str(e);
    }
}

/// Effect signature hash: `sig(h) = hash({op |-> (τ1, τ2, δ)})` (§10.1) — names,
/// suspension class, delivery classes, AND operation argument/result types (F-008:
/// the type halves were deferred at Phase 5 and never landed, so a type-only change
/// resumed stale continuations undetected). An operation with no declared signature
/// hashes as the `Data -> Data` default, so types *appearing* later is drift too,
/// consistent with §10.1's absence rule.
pub fn effect_sig(decl: &EffectDecl, sigs: &entoli_types::Signatures) -> Hash256 {
    let mut w = Writer::new();
    w.str(&decl.name);
    w.u8(match decl.class {
        entoli_syntax::SuspensionClass::Suspending => 0,
        entoli_syntax::SuspensionClass::Prompt => 1,
    });
    w.u32(decl.ops.len() as u32);
    for op in &decl.ops {
        w.str(&op.name);
        let sig = sigs.op(&decl.name, &op.name);
        write_type(&mut w, &sig.arg);
        write_type(&mut w, &sig.ret);
        match &op.delivery {
            DeliveryClass::Idempotent => w.u8(0),
            DeliveryClass::AtLeastOnce => w.u8(1),
            DeliveryClass::Compensable { comp_effect, comp_op } => {
                w.u8(2);
                w.str(comp_effect);
                w.str(comp_op);
            }
        }
    }
    sha(&w.buf)
}

/// Whole-program identity: defs (by hash) plus effect signatures.
pub fn program_hash(program: &Program, sigs: &entoli_types::Signatures) -> Hash256 {
    let defs = def_hashes(program);
    let mut w = Writer::new();
    w.u32(defs.len() as u32);
    for (name, h) in &defs {
        w.str(name);
        w.raw(h);
    }
    w.u32(program.effects.len() as u32);
    for decl in program.effects.values() {
        w.raw(&effect_sig(decl, sigs));
    }
    w.u32(program.foreigns.len() as u32);
    for f in &program.foreigns {
        w.str(f);
    }
    sha(&w.buf)
}
