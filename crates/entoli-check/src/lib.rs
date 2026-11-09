//! Phase 5: the static checker (paper §4.2–§4.3; plan §12's E-codes at compile time).
//!
//! Bidirectional-lite over declared top-level signatures: synthesis where possible,
//! checking against annotations at definition boundaries. What is static here was a
//! dynamic proxy in Phases 1–4: unhandled effects (E001), missing capabilities (E002),
//! affine continuations (E003), tail-position resume (E004), the nested-suspension
//! side condition ε \ εk ∩ Susp = {} (E005), the at-least-once witness obligation, and
//! row containment at every annotation — the composition property of paper §2.2.

pub mod parse;

use entoli_syntax::{DeliveryClass, Handler, Pattern, PrimOp, Program, SuspensionClass, Term};
use entoli_types::{Row, Signatures, Type, AT_LEAST_ONCE};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

struct Binding {
    name: String,
    ty: Type,
    affine: bool,
    used: bool,
}

#[derive(Default)]
struct Ctx {
    scope: Vec<Binding>,
}

impl Ctx {
    fn bind(&mut self, name: &str, ty: Type) {
        let affine = ty.is_affine();
        self.scope.push(Binding { name: name.into(), ty, affine, used: false });
    }
    fn truncate(&mut self, n: usize) {
        self.scope.truncate(n);
    }
    fn len(&self) -> usize {
        self.scope.len()
    }
    fn used_snapshot(&self) -> Vec<bool> {
        self.scope.iter().map(|b| b.used).collect()
    }
    fn restore_used(&mut self, snap: &[bool]) {
        for (b, u) in self.scope.iter_mut().zip(snap) {
            b.used = *u;
        }
    }
    fn or_used(&mut self, snap: &[bool]) {
        for (b, u) in self.scope.iter_mut().zip(snap) {
            b.used = b.used || *u;
        }
    }
}

/// Per-restriction ablation switches (plan §10.5): demo D measures what each
/// restriction costs in writability by disabling them one at a time. All on by default.
#[derive(Debug, Clone, Copy)]
pub struct CheckFlags {
    pub affine: bool,
    pub delivery: bool,
    pub tail: bool,
    pub nested_suspension: bool,
}

impl Default for CheckFlags {
    fn default() -> Self {
        CheckFlags { affine: true, delivery: true, tail: true, nested_suspension: true }
    }
}

pub struct Checker<'a> {
    program: &'a Program,
    sigs: &'a Signatures,
    susp: BTreeSet<String>,
    pub flags: CheckFlags,
    in_suspending_clause: bool,
    pub diags: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    pub fn new(program: &'a Program, sigs: &'a Signatures) -> Self {
        Self::with_flags(program, sigs, CheckFlags::default())
    }

    pub fn with_flags(program: &'a Program, sigs: &'a Signatures, flags: CheckFlags) -> Self {
        let susp = program
            .effects
            .values()
            .filter(|e| e.class == SuspensionClass::Suspending)
            .map(|e| e.name.clone())
            .collect();
        Checker { program, sigs, susp, flags, in_suspending_clause: false, diags: Vec::new() }
    }

    fn err(&mut self, code: &'static str, message: String) {
        self.diags.push(Diagnostic { code, message });
    }

    // -- entry points -------------------------------------------------------

    /// Check every definition with a declared signature, plus program-load rules.
    pub fn check_defs(&mut self) {
        if let Err(e) = self.program.validate() {
            self.err("E011", e);
        }
        let names: Vec<String> = self.sigs.defs.keys().cloned().collect();
        for name in names {
            let ty = self.sigs.defs[&name].clone();
            match self.program.defs.get(&name) {
                None => self.err("E000", format!("declared def {name} has no body")),
                Some(term) => {
                    let term = term.clone();
                    let mut ctx = Ctx::default();
                    // A top-level definition is a value: its own row must be empty
                    // (its latent effects live in its Fn type).
                    let row = self.check_ty(&term, &ty, &mut ctx, &format!("def {name}"));
                    if !row.is_empty() {
                        self.err(
                            "E001",
                            format!("def {name} performs effects {row} at definition time"),
                        );
                    }
                }
            }
        }
    }

    /// Check an entry term under initial bindings against a required residual row —
    /// for a whole program the required row is {} (every effect handled, every
    /// at-least-once witnessed). Emits the demo-B pair of diagnostics: the effect that
    /// escapes AND the missing capability, both named.
    pub fn check_entry(&mut self, entry: &Term, bindings: &[(String, Type)], required: &Row) {
        let mut ctx = Ctx::default();
        for (n, t) in bindings {
            ctx.bind(n, t.clone());
        }
        let (_, row) = self.synth(entry, &mut ctx);
        let escaped = row.minus(required);
        for e in escaped.0.iter() {
            if e == AT_LEAST_ONCE {
                self.err(
                    "E001",
                    "an at-least-once operation reaches the top level without an idempotency witness".into(),
                );
                continue;
            }
            self.err("E001", format!("unhandled effect {e} in entry (row {row}, required {required})"));
            let cap_in_scope = bindings.iter().any(|(_, t)| matches!(t, Type::Cap(ce) if ce == e));
            if !cap_in_scope {
                self.err("E002", format!("no capability Cap[{e}] is in scope to handle {e}"));
            }
        }
    }

    // -- core judgment ------------------------------------------------------

    fn synth(&mut self, e: &Term, ctx: &mut Ctx) -> (Type, Row) {
        match e {
            Term::Unit => (Type::Unit, Row::empty()),
            Term::Int(_) => (Type::Int, Row::empty()),
            Term::Bool(_) => (Type::Bool, Row::empty()),
            Term::Str(_) => (Type::Str, Row::empty()),
            Term::Var(n) => {
                let mut found = None;
                let affine_on = self.flags.affine;
                for b in ctx.scope.iter_mut().rev() {
                    if b.name == *n {
                        if affine_on && b.affine && b.used {
                            self.diags.push(Diagnostic {
                                code: "E003",
                                message: format!("continuation {n} used more than once (affine)"),
                            });
                        }
                        b.used = true;
                        found = Some(b.ty.clone());
                        break;
                    }
                }
                match found {
                    Some(t) => (t, Row::empty()),
                    None => {
                        self.err("E000", format!("unbound variable {n}"));
                        (Type::Data, Row::empty())
                    }
                }
            }
            Term::Global(n) => match self.sigs.defs.get(n) {
                Some(t) => (t.clone(), Row::empty()),
                None => {
                    self.err("E000", format!("global {n} has no declared type"));
                    (Type::Data, Row::empty())
                }
            },
            Term::Lam { param, body } => {
                let mark = ctx.len();
                ctx.bind(param, Type::Data);
                // A lambda body is deferred execution, not clause execution: a
                // foreign application under it would run outside the crash
                // window (paper v0.6 §6.4; P-2).
                let was_in = self.in_suspending_clause;
                self.in_suspending_clause = false;
                let (bt, br) = self.synth(body, ctx);
                self.in_suspending_clause = was_in;
                ctx.truncate(mark);
                (Type::Fn(Box::new(Type::Data), br, Box::new(bt)), Row::empty())
            }
            Term::App { func, arg } => {
                let (ft, fr) = self.synth(func, ctx);
                match ft {
                    Type::Fn(t1, latent, t2) => {
                        let ar = self.check_ty(arg, &t1, ctx, "function argument");
                        (*t2, fr.union(&ar).union(&latent))
                    }
                    other => {
                        self.err("E000", format!("application of non-function type {other}"));
                        let (_, ar) = self.synth(arg, ctx);
                        (Type::Data, fr.union(&ar))
                    }
                }
            }
            Term::Let { name, bound, body } => {
                let (bt, br) = self.synth(bound, ctx);
                let mark = ctx.len();
                ctx.bind(name, bt);
                let (t, r) = self.synth(body, ctx);
                ctx.truncate(mark);
                (t, br.union(&r))
            }
            Term::If { cond, then, els } => {
                let cr = self.check_ty(cond, &Type::Bool, ctx, "if condition");
                let snap = ctx.used_snapshot();
                let (tt, tr) = self.synth(then, ctx);
                let then_used = ctx.used_snapshot();
                ctx.restore_used(&snap);
                let er = self.check_ty(els, &tt, ctx, "else branch");
                ctx.or_used(&then_used);
                (tt, cr.union(&tr).union(&er))
            }
            Term::Do { effect, op, arg, witness } => self.synth_do(effect, op, arg, witness.as_deref(), ctx),
            Term::Handle { body, handler, cap } => self.synth_handle(body, handler, cap, ctx),
            Term::Resume { kont, arg } => {
                let (kt, kr) = self.synth(kont, ctx);
                match kt {
                    Type::Kont(t2, ek, tout) => {
                        let ar = self.check_ty(arg, &t2, ctx, "resume argument");
                        (*tout, kr.union(&ar).union(&ek))
                    }
                    other => {
                        self.err("E000", format!("resume of non-continuation type {other}"));
                        let (_, ar) = self.synth(arg, ctx);
                        (Type::Data, kr.union(&ar))
                    }
                }
            }
            Term::Ctor { args, .. } => {
                let mut row = Row::empty();
                for a in args {
                    let (_, r) = self.synth(a, ctx);
                    row = row.union(&r);
                }
                (Type::Data, row)
            }
            Term::Match { scrut, arms } => {
                let sr = self.check_ty(scrut, &Type::Data, ctx, "match scrutinee");
                let mut row = sr;
                let mut out: Option<Type> = None;
                let base = ctx.used_snapshot();
                let mut merged = base.clone();
                for arm in arms.iter() {
                    ctx.restore_used(&base);
                    let mark = ctx.len();
                    match &arm.pat {
                        Pattern::Ctor { binders, .. } => {
                            for b in binders {
                                ctx.bind(b, Type::Data);
                            }
                        }
                        Pattern::Any(b) => ctx.bind(b, Type::Data),
                    }
                    let (t, r) = self.synth(&arm.body, ctx);
                    ctx.truncate(mark);
                    for (m, u) in merged.iter_mut().zip(ctx.used_snapshot()) {
                        *m = *m || u;
                    }
                    row = row.union(&r);
                    match &out {
                        None => out = Some(t),
                        Some(prev) if *prev == t => {}
                        Some(prev) => self.err(
                            "E000",
                            format!("match arms disagree: {prev} vs {t}"),
                        ),
                    }
                }
                ctx.restore_used(&merged);
                (out.unwrap_or(Type::Data), row)
            }
            Term::ForeignApp { name, arg } => {
                // Legal only inside a suspending handler clause (unification §3);
                // enabled at migration step M6. Until then every occurrence is
                // rejected, which keeps the pre-unification checker's semantics.
                if !self.in_suspending_clause {
                    self.err(
                        "E000",
                        format!("foreign application {name} outside a suspending handler clause"),
                    );
                }
                if !self.program.foreigns.contains(name) {
                    self.err("E000", format!("undeclared foreign invoker {name}"));
                } else if !self.sigs.foreigns.contains_key(name) {
                    // A-31: a declared foreign with no signature must be a
                    // diagnostic, never a silent Data -> Data default.
                    self.err(
                        "E000",
                        format!("foreign invoker {name} has no declared signature (T1 ~> T2)"),
                    );
                }
                let (a, r) = self
                    .sigs
                    .foreigns
                    .get(name)
                    .cloned()
                    .unwrap_or((Type::Data, Type::Data));
                let row = self.check_ty(arg, &a, ctx, &format!("argument of foreign {name}"));
                let _ = row.clone();
                return (r, row);
            }
            Term::Prim { op, args } => {
                let (ins, out): (Vec<Type>, Type) = match op {
                    PrimOp::Add | PrimOp::Sub | PrimOp::Mul => {
                        (vec![Type::Int, Type::Int], Type::Int)
                    }
                    PrimOp::Lt | PrimOp::IntEq => (vec![Type::Int, Type::Int], Type::Bool),
                    PrimOp::StrEq => (vec![Type::Str, Type::Str], Type::Bool),
                    PrimOp::Concat => (vec![Type::Str, Type::Str], Type::Str),
                    PrimOp::StrOfInt => (vec![Type::Int], Type::Str),
                };
                let mut row = Row::empty();
                if args.len() != ins.len() {
                    self.err("E000", format!("prim {op:?} arity {} vs {}", ins.len(), args.len()));
                }
                for (a, want) in args.iter().zip(ins.iter()) {
                    row = row.union(&self.check_ty(a, want, ctx, "prim argument"));
                }
                (out, row)
            }
        }
    }

    fn synth_do(
        &mut self,
        effect: &str,
        op: &str,
        arg: &Term,
        witness: Option<&Term>,
        ctx: &mut Ctx,
    ) -> (Type, Row) {
        let Some(decl) = self.program.effects.get(effect) else {
            self.err("E000", format!("unknown effect {effect}"));
            return (Type::Data, Row::empty());
        };
        let Some(opdecl) = decl.op(op) else {
            self.err("E000", format!("unknown operation {effect}.{op}"));
            return (Type::Data, Row::empty());
        };
        let delivery = opdecl.delivery.clone();
        let sig = self.sigs.op(effect, op);
        let mut row = self.check_ty(arg, &sig.arg, ctx, &format!("argument of {effect}.{op}"));
        row.insert(effect);
        match (&delivery, witness) {
            (DeliveryClass::AtLeastOnce, Some(w)) => {
                let wr = self.check_ty(w, &Type::Str, ctx, "idempotency witness");
                row = row.union(&wr);
            }
            (DeliveryClass::AtLeastOnce, None) => {
                // deliv(at_least_once) = {AtLeastOnce}: visible until witnessed (§4.2).
                if self.flags.delivery {
                    row.insert(AT_LEAST_ONCE);
                }
            }
            (_, Some(_)) => {
                self.err("E000", format!("witness on non-at-least-once operation {effect}.{op}"));
            }
            _ => {}
        }
        (sig.ret, row)
    }

    fn synth_handle(
        &mut self,
        body: &Term,
        handler: &Handler,
        cap: &Term,
        ctx: &mut Ctx,
    ) -> (Type, Row) {
        let e = &handler.effect;
        // Migration M6: the R-7 stopgap rejection is lifted — suspending effects are
        // handled effects (unification §4), under the tightened T-Clause below.
        let suspending_handler = self
            .program
            .effects
            .get(e)
            .map(|d| d.class == SuspensionClass::Suspending)
            .unwrap_or(false);
        // T-Handle's third premise: writing a handler does not entitle installing it.
        let (ct, cr) = self.synth(cap, ctx);
        match &ct {
            Type::Cap(ce) if ce == e => {}
            other => self.err(
                "E002",
                format!("handler for {e} installed with {other}, not Cap[{e}]"),
            ),
        }
        let (bt, brow) = self.synth(body, ctx);

        // Deep handler: the captured continuation runs the remainder of the body plus
        // the return clause, so εk = ε ∪ {E} (§4.2) and its answer type is the
        // handle's own result type.
        let (tout, rrow) = match &handler.return_clause {
            Some((x, rbody)) => {
                let mark = ctx.len();
                ctx.bind(x, bt.clone());
                let (t, r) = self.synth(rbody, ctx);
                ctx.truncate(mark);
                (t, r)
            }
            None => (bt.clone(), Row::empty()),
        };
        let ek = brow.union(&Row::one(e));

        let mut result_row = brow.without(e).union(&rrow);
        for clause in &handler.clauses {
            let Some(opdecl) = self.program.effects.get(e).and_then(|d| d.op(&clause.op)) else {
                self.err("E000", format!("handler clause for unknown op {e}.{}", clause.op));
                continue;
            };
            let _ = opdecl;
            let sig = self.sigs.op(e, &clause.op);
            let mark = ctx.len();
            ctx.bind(&clause.param, sig.arg.clone());
            // The clause's own kont is bound at row {}: its only legal use is the
            // tail resume, which paper v0.6 §4.3 types as a jump — the latent
            // effects belong to the remainder and are accounted at this handle
            // site already. Every non-tail resume is refused syntactically
            // (E004), so the empty row hides nothing. (P-1: the previous
            // subtraction `own_row \ εk` erased ambient effects from the own
            // row and admitted suspensions inside the crash window.)
            ctx.bind(
                &clause.kont,
                Type::Kont(Box::new(sig.ret.clone()), Row::empty(), Box::new(tout.clone())),
            );
            let was_in = self.in_suspending_clause;
            if suspending_handler {
                self.in_suspending_clause = true;
            }
            let (own_row, resumes) = self.check_clause_body(&clause.body, &clause.kont, &tout, ctx);
            self.in_suspending_clause = was_in;
            ctx.truncate(mark);
            // With the tail resume typed as a jump the body's row IS the own
            // row — no subtraction (paper v0.6 §4.3; P-1).
            let own = own_row.clone();
            if suspending_handler {
                // Unification §4, suspending T-Clause: the clause IS the crash window.
                // (a) empty own row — nothing but pure work, foreign calls, and the
                // tail resume may happen with an intent open;
                if !own.is_empty() {
                    self.err(
                        "E005",
                        format!(
                            "suspending clause {e}.{} performs effects {own} inside the crash window",
                            clause.op
                        ),
                    );
                }
                // (b) all-paths-resume (D-1): the window has exactly two exits, the
                // tail resume (Commit) and a crash — abort is not one of them.
                if !all_paths_resume(&clause.body, &clause.kont) {
                    self.err(
                        "E004",
                        format!(
                            "suspending clause {e}.{} must resume on every control path (all-paths-resume)",
                            clause.op
                        ),
                    );
                }
            } else {
                // §4.3 for prompt clauses: the clause's own work — the tail
                // resume typed as a jump — may not suspend: ε ∩ Susp = {}.
                let suspending: Vec<&String> =
                    own.0.iter().filter(|x| self.susp.contains(*x)).collect();
                if self.flags.nested_suspension && !suspending.is_empty() {
                    self.err(
                        "E005",
                        format!(
                            "handler clause {e}.{} suspends while servicing (effects {:?})",
                            clause.op, suspending
                        ),
                    );
                }
            }
            // The clause's own effects escape THIS delimiter (the clause runs outside
            // it); the resumed remainder's do get discharged here.
            result_row = result_row.union(&own_row.without(e));
            if resumes {
                result_row = result_row.union(&ek.without(e));
            }
        }
        (tout, cr.union(&result_row))
    }

    /// Check a handler clause body: type must be the handle's answer type; `resume k`
    /// only in tail position (E004). Returns (full row, did-it-resume).
    fn check_clause_body(
        &mut self,
        body: &Term,
        kname: &str,
        tout: &Type,
        ctx: &mut Ctx,
    ) -> (Row, bool) {
        // E004 first, syntactically: every Resume of k must be a tail of the body.
        let mut resumes_anywhere = false;
        collect_resumes(body, kname, &mut resumes_anywhere);
        let tail_ok = tail_resume_only(body, kname);
        if self.flags.tail && resumes_anywhere && !tail_ok {
            self.err(
                "E004",
                format!("resume {kname} is not in tail position in its handler clause"),
            );
        }
        let row = self.check_ty(body, tout, ctx, "handler clause body");
        (row, resumes_anywhere)
    }

    fn check_ty(&mut self, e: &Term, want: &Type, ctx: &mut Ctx, what: &str) -> Row {
        // Checking a lambda against a function type is where containment bites:
        // the body's row must fit inside the annotation's row.
        if let (Term::Lam { param, body }, Type::Fn(t1, ann, t2)) = (e, want) {
            let mark = ctx.len();
            ctx.bind(param, (**t1).clone());
            // Deferred execution: no foreign application under a lambda (P-2).
            let was_in = self.in_suspending_clause;
            self.in_suspending_clause = false;
            let brow = self.check_ty(body, t2, ctx, what);
            self.in_suspending_clause = was_in;
            ctx.truncate(mark);
            let escaped = brow.minus(ann);
            for eff in escaped.0.iter() {
                if eff == AT_LEAST_ONCE {
                    self.err(
                        "E001",
                        format!("{what}: an unwitnessed at-least-once operation escapes the annotation -[{ann}]->"),
                    );
                } else {
                    self.err(
                        "E001",
                        format!("{what}: body performs {eff} but the annotation claims -[{ann}]->"),
                    );
                }
            }
            return Row::empty();
        }
        let (t, r) = self.synth(e, ctx);
        if &t != want {
            self.err("E000", format!("{what}: expected {want}, found {t}"));
        }
        r
    }
}

// -- tail-position analysis (syntactic, §4.3) --------------------------------

fn collect_resumes(e: &Term, k: &str, found: &mut bool) {
    match e {
        Term::Resume { kont, arg } => {
            if matches!(&**kont, Term::Var(n) if n == k) {
                *found = true;
            }
            collect_resumes(kont, k, found);
            collect_resumes(arg, k, found);
        }
        Term::Lam { body, .. } => collect_resumes(body, k, found),
        Term::App { func, arg } => {
            collect_resumes(func, k, found);
            collect_resumes(arg, k, found);
        }
        Term::Let { bound, body, .. } => {
            collect_resumes(bound, k, found);
            collect_resumes(body, k, found);
        }
        Term::If { cond, then, els } => {
            collect_resumes(cond, k, found);
            collect_resumes(then, k, found);
            collect_resumes(els, k, found);
        }
        Term::Do { arg, witness, .. } => {
            collect_resumes(arg, k, found);
            if let Some(w) = witness {
                collect_resumes(w, k, found);
            }
        }
        Term::Handle { body, handler, cap } => {
            collect_resumes(body, k, found);
            collect_resumes(cap, k, found);
            if let Some((_, r)) = &handler.return_clause {
                collect_resumes(r, k, found);
            }
            for c in &handler.clauses {
                collect_resumes(&c.body, k, found);
            }
        }
        Term::Ctor { args, .. } | Term::Prim { args, .. } => {
            for a in args {
                collect_resumes(a, k, found);
            }
        }
        Term::ForeignApp { arg, .. } => collect_resumes(arg, k, found),
        Term::Match { scrut, arms } => {
            collect_resumes(scrut, k, found);
            for a in arms.iter() {
                collect_resumes(&a.body, k, found);
            }
        }
        _ => {}
    }
}

/// True iff every control path of `e` ends in `resume k` (unification §4, D-1):
/// the leaves of the tail-position tree are all resumes of k.
fn all_paths_resume(e: &Term, k: &str) -> bool {
    match e {
        Term::Resume { kont, .. } => matches!(&**kont, Term::Var(n) if n == k),
        Term::Let { body, .. } => all_paths_resume(body, k),
        Term::If { then, els, .. } => all_paths_resume(then, k) && all_paths_resume(els, k),
        Term::Match { arms, .. } => arms.iter().all(|a| all_paths_resume(&a.body, k)),
        _ => false,
    }
}

/// True iff every `resume k` in `e` sits in tail position of the clause body.
fn tail_resume_only(e: &Term, k: &str) -> bool {
    fn walk(e: &Term, k: &str, tail: bool) -> bool {
        match e {
            Term::Resume { kont, arg } => {
                let is_k = matches!(&**kont, Term::Var(n) if n == k);
                let mut inner = false;
                collect_resumes(arg, k, &mut inner);
                if inner {
                    return false; // resume of k inside resume's argument: never tail
                }
                if is_k {
                    tail
                } else {
                    walk(kont, k, false) && walk(arg, k, false)
                }
            }
            Term::Let { bound, body, .. } => walk(bound, k, false) && walk(body, k, tail),
            Term::If { cond, then, els } => {
                walk(cond, k, false) && walk(then, k, tail) && walk(els, k, tail)
            }
            Term::Match { scrut, arms } => {
                walk(scrut, k, false) && arms.iter().all(|a| walk(&a.body, k, tail))
            }
            Term::Lam { body, .. } => walk(body, k, false),
            Term::App { func, arg } => walk(func, k, false) && walk(arg, k, false),
            Term::Do { arg, witness, .. } => {
                walk(arg, k, false)
                    && witness.as_ref().map(|w| walk(w, k, false)).unwrap_or(true)
            }
            Term::Handle { body, handler, cap } => {
                walk(body, k, false)
                    && walk(cap, k, false)
                    && handler
                        .return_clause
                        .as_ref()
                        .map(|(_, r)| walk(r, k, false))
                        .unwrap_or(true)
                    && handler.clauses.iter().all(|c| walk(&c.body, k, false))
            }
            Term::Ctor { args, .. } | Term::Prim { args, .. } => {
                args.iter().all(|a| walk(a, k, false))
            }
            Term::ForeignApp { arg, .. } => walk(arg, k, false),
            _ => true,
        }
    }
    walk(e, k, true)
}

/// Convenience: run the whole check for a program + entry.
pub fn check(
    program: &Program,
    sigs: &Signatures,
    entry: &Term,
    bindings: &[(String, Type)],
) -> Vec<Diagnostic> {
    let mut c = Checker::new(program, sigs);
    c.check_defs();
    c.check_entry(entry, bindings, &Row::empty());
    c.diags
}
