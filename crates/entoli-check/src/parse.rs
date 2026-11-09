//! Demonstration D's concrete syntax (plan §10.5): deliberately plain, minimal sugar,
//! everything explicit — it measures whether the *semantics* are generable, not whether
//! a syntax is pleasant. To be discarded after the gate.
//!
//! ```text
//! effect Ledger suspending { post : Data -> Data @at_least_once }
//! def audit : Str -[]-> Data = fn inv -> Audited(inv, 250)
//! def main : Str -[Ledger]-> Data =
//!   fn inv -> do Ledger.post(audit(inv)) witness concat("idem:", inv)
//! ```
//!
//! Conventions: capitalised identifiers are constructors; `f(a, b)` curries; handler
//! clauses are `op(x; k) -> e`; `return x -> e` is the return clause.

use entoli_syntax::{
    Arm, DeliveryClass, EffectDecl, Handler, OpClause, OpDecl, Pattern, PrimOp, Program,
    SuspensionClass, Term,
};
use entoli_types::{Row, Signatures, Type};
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub at: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at offset {}: {}", self.at, self.message)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Str(String),
    Punct(&'static str),
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ParseError> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        if c.is_ascii_alphabetic() || c == '_' {
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push((start, Tok::Ident(src[start..i].to_string())));
            continue;
        }
        if c.is_ascii_digit() || (c == '-' && i + 1 < b.len() && (b[i + 1] as char).is_ascii_digit())
        {
            i += 1;
            while i < b.len() && (b[i] as char).is_ascii_digit() {
                i += 1;
            }
            let n: i64 = src[start..i]
                .parse()
                .map_err(|_| ParseError { at: start, message: "bad integer".into() })?;
            out.push((start, Tok::Int(n)));
            continue;
        }
        if c == '"' {
            i += 1;
            let s0 = i;
            while i < b.len() && b[i] != b'"' {
                i += 1;
            }
            if i == b.len() {
                return Err(ParseError { at: start, message: "unterminated string".into() });
            }
            out.push((start, Tok::Str(src[s0..i].to_string())));
            i += 1;
            continue;
        }
        // Multi-char punctuation first.
        const PUNCTS: &[&str] = &[
            "-[", "]->", "]-o", "~>", "->", "==", "@", "{", "}", "(", ")", "[", "]", ":", ";",
            ",", ".", "=", "+", "-", "*", "<", ">", "_",
        ];
        let mut matched = None;
        for p in PUNCTS {
            if src[i..].starts_with(p) {
                matched = Some(*p);
                break;
            }
        }
        match matched {
            Some(p) => {
                out.push((start, Tok::Punct(p)));
                i += p.len();
            }
            None => {
                return Err(ParseError { at: start, message: format!("unexpected character {c:?}") })
            }
        }
    }
    Ok(out)
}

struct P {
    toks: Vec<(usize, Tok)>,
    pos: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(_, t)| t)
    }
    fn at(&self) -> usize {
        self.toks.get(self.pos).map(|(a, _)| *a).unwrap_or(usize::MAX)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|(_, t)| t.clone());
        self.pos += 1;
        t
    }
    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError { at: self.at(), message: message.into() })
    }
    fn eat_punct(&mut self, p: &'static str) -> Result<(), ParseError> {
        match self.next() {
            Some(Tok::Punct(q)) if q == p => Ok(()),
            other => Err(ParseError { at: self.at(), message: format!("expected {p:?}, got {other:?}") }),
        }
    }
    fn try_punct(&mut self, p: &'static str) -> bool {
        if matches!(self.peek(), Some(Tok::Punct(q)) if *q == p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn ident(&mut self) -> Result<String, ParseError> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            other => Err(ParseError { at: self.at(), message: format!("expected identifier, got {other:?}") }),
        }
    }
    fn try_kw(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

/// After parsing, bare identifiers that name top-level defs (and are not locally
/// bound) become `Global` references, so content addressing sees them (§10.1).
fn resolve(term: &Term, defs: &std::collections::BTreeSet<String>, bound: &mut Vec<String>) -> Term {
    match term {
        Term::Var(n) => {
            if !bound.iter().any(|b| b == n) && defs.contains(n) {
                Term::Global(n.clone())
            } else {
                term.clone()
            }
        }
        Term::Unit | Term::Int(_) | Term::Bool(_) | Term::Str(_) | Term::Global(_) => term.clone(),
        Term::Lam { param, body } => {
            bound.push(param.clone());
            let b = resolve(body, defs, bound);
            bound.pop();
            Term::Lam { param: param.clone(), body: Rc::new(b) }
        }
        Term::App { func, arg } => Term::App {
            func: Rc::new(resolve(func, defs, bound)),
            arg: Rc::new(resolve(arg, defs, bound)),
        },
        Term::Let { name, bound: b, body } => {
            let rb = resolve(b, defs, bound);
            bound.push(name.clone());
            let rbody = resolve(body, defs, bound);
            bound.pop();
            Term::Let { name: name.clone(), bound: Rc::new(rb), body: Rc::new(rbody) }
        }
        Term::If { cond, then, els } => Term::If {
            cond: Rc::new(resolve(cond, defs, bound)),
            then: Rc::new(resolve(then, defs, bound)),
            els: Rc::new(resolve(els, defs, bound)),
        },
        Term::Do { effect, op, arg, witness } => Term::Do {
            effect: effect.clone(),
            op: op.clone(),
            arg: Rc::new(resolve(arg, defs, bound)),
            witness: witness.as_ref().map(|w| Rc::new(resolve(w, defs, bound))),
        },
        Term::Handle { body, handler, cap } => {
            let rc = handler.return_clause.as_ref().map(|(x, e)| {
                bound.push(x.clone());
                let r = resolve(e, defs, bound);
                bound.pop();
                (x.clone(), Rc::new(r))
            });
            let clauses = handler
                .clauses
                .iter()
                .map(|c| {
                    bound.push(c.param.clone());
                    bound.push(c.kont.clone());
                    let b = resolve(&c.body, defs, bound);
                    bound.pop();
                    bound.pop();
                    OpClause {
                        op: c.op.clone(),
                        param: c.param.clone(),
                        kont: c.kont.clone(),
                        body: Rc::new(b),
                    }
                })
                .collect();
            Term::Handle {
                body: Rc::new(resolve(body, defs, bound)),
                handler: Rc::new(Handler {
                    effect: handler.effect.clone(),
                    return_clause: rc,
                    clauses,
                }),
                cap: Rc::new(resolve(cap, defs, bound)),
            }
        }
        Term::Resume { kont, arg } => Term::Resume {
            kont: Rc::new(resolve(kont, defs, bound)),
            arg: Rc::new(resolve(arg, defs, bound)),
        },
        Term::Ctor { name, args } => Term::Ctor {
            name: name.clone(),
            args: args.iter().map(|a| resolve(a, defs, bound)).collect(),
        },
        Term::Match { scrut, arms } => Term::Match {
            scrut: Rc::new(resolve(scrut, defs, bound)),
            arms: Rc::new(
                arms.iter()
                    .map(|a| {
                        let names: Vec<String> = match &a.pat {
                            Pattern::Ctor { binders, .. } => binders.clone(),
                            Pattern::Any(b) => vec![b.clone()],
                        };
                        for n in &names {
                            bound.push(n.clone());
                        }
                        let b = resolve(&a.body, defs, bound);
                        for _ in &names {
                            bound.pop();
                        }
                        Arm { pat: a.pat.clone(), body: b }
                    })
                    .collect(),
            ),
        },
        Term::Prim { op, args } => Term::Prim {
            op: *op,
            args: args.iter().map(|a| resolve(a, defs, bound)).collect(),
        },
        Term::ForeignApp { name, arg } => Term::ForeignApp {
            name: name.clone(),
            arg: Rc::new(resolve(arg, defs, bound)),
        },
    }
}

/// Rewrite `App(Var f, a)` to `ForeignApp` for declared foreign names. Foreign
/// names are second-class (paper §6.4): a bare occurrence outside application
/// position is left as a Var, which the checker then rejects as unknown.
fn foreignize(term: &Term, foreigns: &std::collections::BTreeSet<String>) -> Term {
    let go = |t: &Term| foreignize(t, foreigns);
    match term {
        Term::App { func, arg } => {
            if let Term::Var(n) = &**func {
                if foreigns.contains(n) {
                    return Term::ForeignApp { name: n.clone(), arg: Rc::new(go(arg)) };
                }
            }
            Term::App { func: Rc::new(go(func)), arg: Rc::new(go(arg)) }
        }
        Term::Var(_) | Term::Unit | Term::Int(_) | Term::Bool(_) | Term::Str(_)
        | Term::Global(_) => term.clone(),
        Term::Lam { param, body } => {
            Term::Lam { param: param.clone(), body: Rc::new(go(body)) }
        }
        Term::Let { name, bound, body } => Term::Let {
            name: name.clone(),
            bound: Rc::new(go(bound)),
            body: Rc::new(go(body)),
        },
        Term::If { cond, then, els } => Term::If {
            cond: Rc::new(go(cond)),
            then: Rc::new(go(then)),
            els: Rc::new(go(els)),
        },
        Term::Do { effect, op, arg, witness } => Term::Do {
            effect: effect.clone(),
            op: op.clone(),
            arg: Rc::new(go(arg)),
            witness: witness.as_ref().map(|w| Rc::new(go(w))),
        },
        Term::Handle { body, handler, cap } => Term::Handle {
            body: Rc::new(go(body)),
            handler: Rc::new(Handler {
                effect: handler.effect.clone(),
                return_clause: handler
                    .return_clause
                    .as_ref()
                    .map(|(x, e)| (x.clone(), Rc::new(go(e)))),
                clauses: handler
                    .clauses
                    .iter()
                    .map(|c| OpClause {
                        op: c.op.clone(),
                        param: c.param.clone(),
                        kont: c.kont.clone(),
                        body: Rc::new(go(&c.body)),
                    })
                    .collect(),
            }),
            cap: Rc::new(go(cap)),
        },
        Term::Resume { kont, arg } => {
            Term::Resume { kont: Rc::new(go(kont)), arg: Rc::new(go(arg)) }
        }
        Term::Ctor { name, args } => {
            Term::Ctor { name: name.clone(), args: args.iter().map(go).collect() }
        }
        Term::Match { scrut, arms } => Term::Match {
            scrut: Rc::new(go(scrut)),
            arms: Rc::new(
                arms.iter().map(|a| Arm { pat: a.pat.clone(), body: go(&a.body) }).collect(),
            ),
        },
        Term::Prim { op, args } => {
            Term::Prim { op: *op, args: args.iter().map(go).collect() }
        }
        Term::ForeignApp { name, arg } => {
            Term::ForeignApp { name: name.clone(), arg: Rc::new(go(arg)) }
        }
    }
}

pub fn parse_program(src: &str) -> Result<(Program, Signatures), ParseError> {
    let mut p = P { toks: lex(src)?, pos: 0 };
    let mut program = Program::default();
    let mut sigs = Signatures::default();
    loop {
        if p.peek().is_none() {
            break;
        }
        if p.try_kw("effect") {
            let name = p.ident()?;
            let class = if p.try_kw("suspending") {
                SuspensionClass::Suspending
            } else if p.try_kw("prompt") {
                SuspensionClass::Prompt
            } else {
                return p.err("expected `suspending` or `prompt`");
            };
            p.eat_punct("{")?;
            let mut ops = Vec::new();
            while !p.try_punct("}") {
                let opname = p.ident()?;
                p.eat_punct(":")?;
                let arg = parse_type(&mut p)?;
                p.eat_punct("->")?;
                let ret = parse_type(&mut p)?;
                p.eat_punct("@")?;
                let dname = p.ident()?;
                let delivery = match dname.as_str() {
                    "idempotent" => DeliveryClass::Idempotent,
                    "at_least_once" => DeliveryClass::AtLeastOnce,
                    "compensable" => {
                        p.eat_punct("(")?;
                        let ce = p.ident()?;
                        p.eat_punct(".")?;
                        let co = p.ident()?;
                        p.eat_punct(")")?;
                        DeliveryClass::Compensable { comp_effect: ce, comp_op: co }
                    }
                    other => return p.err(format!("unknown delivery class {other}")),
                };
                sigs.declare_op(&name, &opname, arg, ret);
                ops.push(OpDecl { name: opname, delivery });
            }
            program.effects.insert(name.clone(), EffectDecl { name, class, ops });
        } else if p.try_kw("foreign") {
            // foreign f : T1 ~> T2  — a second-class host-implemented invoker
            // (paper v0.6 §6.4); applications are rewritten below.
            let name = p.ident()?;
            p.eat_punct(":")?;
            let t1 = parse_type(&mut p)?;
            p.eat_punct("~>")?;
            let t2 = parse_type(&mut p)?;
            program.foreigns.insert(name.clone());
            sigs.foreigns.insert(name, (t1, t2));
        } else if p.try_kw("def") {
            let name = p.ident()?;
            p.eat_punct(":")?;
            let ty = parse_type(&mut p)?;
            p.eat_punct("=")?;
            let body = parse_expr(&mut p)?;
            sigs.declare_def(&name, ty);
            program.defs.insert(name, Rc::new(body));
        } else {
            return p.err("expected `effect` or `def`");
        }
    }
    let def_names: std::collections::BTreeSet<String> = program.defs.keys().cloned().collect();
    let resolved: Vec<(String, Term)> = program
        .defs
        .iter()
        .map(|(n, t)| {
            let r = resolve(t, &def_names, &mut Vec::new());
            (n.clone(), foreignize(&r, &program.foreigns))
        })
        .collect();
    for (n, t) in resolved {
        program.defs.insert(n, Rc::new(t));
    }
    Ok((program, sigs))
}

fn parse_type(p: &mut P) -> Result<Type, ParseError> {
    let left = parse_atype(p)?;
    if p.try_punct("-[") {
        let mut row = Row::empty();
        if !matches!(p.peek(), Some(Tok::Punct("]->")) | Some(Tok::Punct("]-o"))) {
            loop {
                row.insert(&p.ident()?);
                if !p.try_punct(",") {
                    break;
                }
            }
        }
        if p.try_punct("]->") {
            let right = parse_type(p)?;
            return Ok(Type::Fn(Box::new(left), row, Box::new(right)));
        }
        p.eat_punct("]-o")?;
        let right = parse_type(p)?;
        return Ok(Type::Kont(Box::new(left), row, Box::new(right)));
    }
    Ok(left)
}

fn parse_atype(p: &mut P) -> Result<Type, ParseError> {
    if p.try_punct("(") {
        let t = parse_type(p)?;
        p.eat_punct(")")?;
        return Ok(t);
    }
    let name = p.ident()?;
    Ok(match name.as_str() {
        "Unit" => Type::Unit,
        "Int" => Type::Int,
        "Bool" => Type::Bool,
        "Str" => Type::Str,
        "Data" => Type::Data,
        "Cap" => {
            p.eat_punct("[")?;
            let e = p.ident()?;
            p.eat_punct("]")?;
            Type::Cap(e)
        }
        other => return p.err(format!("unknown type {other}")),
    })
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "in", "if", "then", "else", "match", "handle", "with", "using",
    "resume", "do", "witness", "return", "effect", "def", "foreign", "suspending",
    "prompt",
];

/// An atom WITHOUT the call suffix: for operand positions where `k (v)` must be
/// two operands, never an application (the resume-argument bug found by the
/// 2026-11-08 authorship study).
fn parse_atom(p: &mut P) -> Result<Term, ParseError> {
    match p.next() {
        Some(Tok::Ident(name)) if !KEYWORDS.contains(&name.as_str()) => Ok(Term::Var(name)),
        Some(Tok::Punct("(")) => {
            let e = parse_expr(p)?;
            p.eat_punct(")")?;
            Ok(e)
        }
        other => Err(ParseError {
            at: p.at(),
            message: format!("expected a variable or parenthesised expression, got {other:?}"),
        }),
    }
}

fn parse_expr(p: &mut P) -> Result<Term, ParseError> {
    if p.try_kw("fn") {
        let param = p.ident()?;
        p.eat_punct("->")?;
        let body = parse_expr(p)?;
        return Ok(Term::Lam { param, body: Rc::new(body) });
    }
    if p.try_kw("let") {
        let name = p.ident()?;
        p.eat_punct("=")?;
        let bound = parse_expr(p)?;
        if !p.try_kw("in") {
            return p.err("expected `in`");
        }
        let body = parse_expr(p)?;
        return Ok(Term::Let { name, bound: Rc::new(bound), body: Rc::new(body) });
    }
    if p.try_kw("if") {
        let cond = parse_expr(p)?;
        if !p.try_kw("then") {
            return p.err("expected `then`");
        }
        let then = parse_expr(p)?;
        if !p.try_kw("else") {
            return p.err("expected `else`");
        }
        let els = parse_expr(p)?;
        return Ok(Term::If { cond: Rc::new(cond), then: Rc::new(then), els: Rc::new(els) });
    }
    if p.try_kw("match") {
        let scrut = parse_expr(p)?;
        p.eat_punct("{")?;
        let mut arms = Vec::new();
        while !p.try_punct("}") {
            let pat = if p.try_punct("_") {
                Pattern::Any("_".into())
            } else {
                let n = p.ident()?;
                if p.try_punct("(") {
                    let mut binders = Vec::new();
                    while !p.try_punct(")") {
                        binders.push(p.ident()?);
                        p.try_punct(",");
                    }
                    Pattern::Ctor { name: n, binders }
                } else if n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                    // A-28: `None` is the nullary constructor, never a catch-all.
                    Pattern::Ctor { name: n, binders: vec![] }
                } else {
                    Pattern::Any(n)
                }
            };
            p.eat_punct("->")?;
            let body = parse_expr(p)?;
            arms.push(Arm { pat, body });
        }
        return Ok(Term::Match { scrut: Rc::new(scrut), arms: Rc::new(arms) });
    }
    if p.try_kw("handle") {
        let body = parse_expr(p)?;
        if !p.try_kw("with") {
            return p.err("expected `with`");
        }
        let effect = p.ident()?;
        p.eat_punct("{")?;
        let mut return_clause = None;
        let mut clauses = Vec::new();
        while !p.try_punct("}") {
            if p.try_kw("return") {
                let x = p.ident()?;
                p.eat_punct("->")?;
                let e = parse_expr(p)?;
                return_clause = Some((x, Rc::new(e)));
            } else {
                let op = p.ident()?;
                p.eat_punct("(")?;
                let param = p.ident()?;
                p.eat_punct(";")?;
                let kont = p.ident()?;
                p.eat_punct(")")?;
                p.eat_punct("->")?;
                let body = parse_expr(p)?;
                clauses.push(OpClause { op, param, kont, body: Rc::new(body) });
            }
        }
        if !p.try_kw("using") {
            return p.err("expected `using`");
        }
        let cap = parse_atom(p)?;
        return Ok(Term::Handle {
            body: Rc::new(body),
            handler: Rc::new(Handler { effect, return_clause, clauses }),
            cap: Rc::new(cap),
        });
    }
    if p.try_kw("resume") {
        let k = parse_atom(p)?;
        let v = parse_prefix(p)?;
        return Ok(Term::Resume { kont: Rc::new(k), arg: Rc::new(v) });
    }
    parse_binop(p)
}

fn parse_binop(p: &mut P) -> Result<Term, ParseError> {
    // ==, < over +,- over *
    let mut lhs = parse_add(p)?;
    loop {
        let (op, flip) = match p.peek() {
            Some(Tok::Punct("==")) => (Some(PrimOp::IntEq), false),
            Some(Tok::Punct("<")) => (Some(PrimOp::Lt), false),
            // a > b is Lt with the operands swapped — no new primitive.
            Some(Tok::Punct(">")) => (Some(PrimOp::Lt), true),
            _ => (None, false),
        };
        match op {
            Some(o) => {
                p.next();
                let rhs = parse_add(p)?;
                let args = if flip { vec![rhs, lhs] } else { vec![lhs, rhs] };
                lhs = Term::Prim { op: o, args };
            }
            None => return Ok(lhs),
        }
    }
}

fn parse_add(p: &mut P) -> Result<Term, ParseError> {
    let mut lhs = parse_mul(p)?;
    loop {
        let op = match p.peek() {
            Some(Tok::Punct("+")) => Some(PrimOp::Add),
            Some(Tok::Punct("-")) => Some(PrimOp::Sub),
            _ => None,
        };
        match op {
            Some(o) => {
                p.next();
                let rhs = parse_mul(p)?;
                lhs = Term::Prim { op: o, args: vec![lhs, rhs] };
            }
            None => return Ok(lhs),
        }
    }
}

fn parse_mul(p: &mut P) -> Result<Term, ParseError> {
    let mut lhs = parse_prefix(p)?;
    while matches!(p.peek(), Some(Tok::Punct("*"))) {
        p.next();
        let rhs = parse_prefix(p)?;
        lhs = Term::Prim { op: PrimOp::Mul, args: vec![lhs, rhs] };
    }
    Ok(lhs)
}

/// Atoms plus call/constructor argument lists. `do` lives here (operand level) so
/// operations compose with operators: `do A.ask(1) + do A.ask(2)`. The witness
/// expression is a prefix form, so `witness concat("i:", x) + 1` cannot swallow the
/// operator.
fn parse_prefix(p: &mut P) -> Result<Term, ParseError> {
    if p.try_kw("do") {
        let effect = p.ident()?;
        p.eat_punct(".")?;
        let op = p.ident()?;
        p.eat_punct("(")?;
        let arg = if p.try_punct(")") {
            Term::Unit
        } else {
            let a = parse_expr(p)?;
            p.eat_punct(")")?;
            a
        };
        let witness = if p.try_kw("witness") { Some(Rc::new(parse_prefix(p)?)) } else { None };
        return Ok(Term::Do { effect, op, arg: Rc::new(arg), witness });
    }
    let mut base = match p.next() {
        Some(Tok::Int(i)) => Term::Int(i),
        Some(Tok::Str(s)) => Term::Str(s),
        Some(Tok::Punct("(")) => {
            if p.try_punct(")") {
                Term::Unit
            } else {
                let e = parse_expr(p)?;
                p.eat_punct(")")?;
                e
            }
        }
        Some(Tok::Ident(name)) => match name.as_str() {
            "true" => Term::Bool(true),
            "false" => Term::Bool(false),
            "concat" | "str_of_int" | "str_eq" => {
                let op = match name.as_str() {
                    "concat" => PrimOp::Concat,
                    "str_of_int" => PrimOp::StrOfInt,
                    _ => PrimOp::StrEq,
                };
                p.eat_punct("(")?;
                let mut args = Vec::new();
                while !p.try_punct(")") {
                    args.push(parse_expr(p)?);
                    p.try_punct(",");
                }
                Term::Prim { op, args }
            }
            _ if KEYWORDS.contains(&name.as_str()) => {
                return Err(ParseError {
                    at: p.at(),
                    message: format!("keyword `{name}` cannot be used as an expression here"),
                })
            }
            _ => {
                let is_ctor = name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false);
                if is_ctor {
                    let mut args = Vec::new();
                    if p.try_punct("(") {
                        while !p.try_punct(")") {
                            args.push(parse_expr(p)?);
                            p.try_punct(",");
                        }
                    }
                    Term::Ctor { name, args }
                } else {
                    Term::Var(name)
                }
            }
        },
        other => return Err(ParseError { at: p.at(), message: format!("unexpected {other:?}") }),
    };
    // Call syntax: f(a, b) curries to App(App(f, a), b).
    while matches!(p.peek(), Some(Tok::Punct("("))) {
        // Constructor already consumed its argument list above.
        if matches!(base, Term::Ctor { .. }) {
            break;
        }
        p.next();
        if p.try_punct(")") {
            base = Term::App { func: Rc::new(base), arg: Rc::new(Term::Unit) };
            continue;
        }
        loop {
            let a = parse_expr(p)?;
            base = Term::App { func: Rc::new(base), arg: Rc::new(a) };
            if p.try_punct(",") {
                continue;
            }
            p.eat_punct(")")?;
            break;
        }
    }
    Ok(base)
}
