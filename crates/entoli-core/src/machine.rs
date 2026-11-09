use crate::env::Env;
use crate::error::EntoliError;
use crate::value::{new_kont, CapAuthority, Frame, Value};
use entoli_syntax::{DeliveryClass, Pattern, PrimOp, Program, SuspensionClass, Term};
use std::rc::Rc;

/// A suspension delivered to the durable machine: a suspending operation together with
/// the *entire* remaining continuation, moved out of the evaluator (uniquely owned, §5.1).
#[derive(Debug)]
pub struct Suspension {
    pub effect: String,
    pub op: String,
    pub arg: Value,
    pub witness: Option<String>,
    pub frames: Vec<Frame>,
    /// The innermost installed handler for this effect, if any (unification §5):
    /// the runtime executes its clause as steps 2–4 of the protocol. None = the
    /// transitional registry path (dual dispatch, migration M2, removed at M7).
    pub handler: Option<SuspendHandler>,
}

/// What the runtime needs to run a suspending clause: the clause, the installation
/// environment, and the capability the handler was installed with (checked live and
/// spent at Intend). The Handle frame itself stays inside the captured continuation.
#[derive(Debug)]
pub struct SuspendHandler {
    pub clause: entoli_syntax::OpClause,
    pub env: Env,
    pub cap: crate::value::CapId,
}

pub const WINDOW_KONT_ID: &str = "\u{ab}window\u{bb}";

#[derive(Debug)]
pub enum Outcome {
    Value(Value),
    /// The durable-window sentinel was resumed: the clause is over and `value`
    /// is what Commit writes. Distinguished from Value so the runtime can tell
    /// a tail resume from a clause that resumed mid-body and kept computing —
    /// the P-5 dynamic proxy for unchecked programs.
    WindowReturn(Value),
    Suspended(Suspension),
    /// A foreign application reached with its argument evaluated: the machine yields
    /// to the runtime, which invokes the host binding and resumes with the result.
    ForeignCall { name: String, arg: Value, frames: Vec<Frame> },
}

enum Control {
    Eval(Rc<Term>, Env),
    Ret(Value),
}

pub struct Machine<'a> {
    window_return: Option<Value>,
    program: &'a Program,
    authority: &'a dyn CapAuthority,
    control: Control,
    stack: Vec<Frame>,
    /// Fresh continuation ids: hierarchical paths under `id_base` (§12 commitment 1).
    id_base: String,
    next_kont: u64,
}

impl<'a> Machine<'a> {
    pub fn new(program: &'a Program, authority: &'a dyn CapAuthority, term: Rc<Term>) -> Self {
        Machine {
            window_return: None,
            program,
            authority,
            control: Control::Eval(term, Env::empty()),
            stack: Vec::new(),
            id_base: "0".to_string(),
            next_kont: 0,
        }
    }

    /// Rebuild a machine around a continuation stack awaiting a value — step 5 of the
    /// protocol in normal operation, and cold resume during recovery.
    pub fn resume_frames(
        program: &'a Program,
        authority: &'a dyn CapAuthority,
        frames: Vec<Frame>,
        value: Value,
        id_base: String,
    ) -> Self {
        Machine {
            window_return: None,
            program,
            authority,
            control: Control::Ret(value),
            stack: frames,
            id_base,
            next_kont: 0,
        }
    }

    pub fn with_env(mut self, env: Env) -> Self {
        if let Control::Eval(term, _) = self.control {
            self.control = Control::Eval(term, env);
        }
        self
    }

    fn fresh_kont_id(&mut self) -> String {
        let id = format!("{}.{}", self.id_base, self.next_kont);
        self.next_kont += 1;
        id
    }

    pub fn run(&mut self) -> Result<Outcome, EntoliError> {
        self.run_fuel(u64::MAX)
    }

    pub fn run_fuel(&mut self, mut fuel: u64) -> Result<Outcome, EntoliError> {
        loop {
            if fuel == 0 {
                return Err(EntoliError::Dynamic("out of fuel".into()));
            }
            fuel -= 1;
            if let Some(v) = self.window_return.take() {
                return Ok(Outcome::WindowReturn(v));
            }
            if let Some(out) = self.step()? {
                return Ok(out);
            }
        }
    }

    /// One small step. Returns Some(outcome) when the machine halts (final value or
    /// suspension to the durable machine).
    fn step(&mut self) -> Result<Option<Outcome>, EntoliError> {
        let control = std::mem::replace(&mut self.control, Control::Ret(Value::Unit));
        match control {
            Control::Eval(term, env) => self.eval(term, env).map(|()| None),
            Control::Ret(value) => self.ret(value),
        }
    }

    fn eval(&mut self, term: Rc<Term>, env: Env) -> Result<(), EntoliError> {
        match &*term {
            Term::Var(name) => {
                let v = env.lookup(name).cloned().ok_or_else(|| {
                    EntoliError::Dynamic(format!("unbound variable {name}"))
                })?;
                self.control = Control::Ret(v);
            }
            Term::Unit => self.control = Control::Ret(Value::Unit),
            Term::Int(i) => self.control = Control::Ret(Value::Int(*i)),
            Term::Bool(b) => self.control = Control::Ret(Value::Bool(*b)),
            Term::Str(s) => self.control = Control::Ret(Value::Str(s.clone())),
            Term::Lam { param, body } => {
                self.control = Control::Ret(Value::Closure {
                    param: param.clone(),
                    body: body.clone(),
                    env,
                });
            }
            Term::ForeignApp { name, arg } => {
                self.stack.push(Frame::ForeignArg { name: name.clone() });
                self.control = Control::Eval(arg.clone(), env);
            }
            Term::Global(name) => {
                let def = self.program.defs.get(name).cloned().ok_or_else(|| {
                    EntoliError::Dynamic(format!("unknown global {name}"))
                })?;
                // Top-level definitions are closed: evaluate under the empty environment.
                self.control = Control::Eval(def, Env::empty());
            }
            Term::App { func, arg } => {
                self.stack.push(Frame::AppFn { arg: arg.clone(), env: env.clone() });
                self.control = Control::Eval(func.clone(), env);
            }
            Term::Let { name, bound, body } => {
                self.stack.push(Frame::Let {
                    name: name.clone(),
                    body: body.clone(),
                    env: env.clone(),
                });
                self.control = Control::Eval(bound.clone(), env);
            }
            Term::If { cond, then, els } => {
                self.stack.push(Frame::If { then: then.clone(), els: els.clone(), env: env.clone() });
                self.control = Control::Eval(cond.clone(), env);
            }
            Term::Do { effect, op, arg, witness } => {
                self.stack.push(Frame::DoArg {
                    effect: effect.clone(),
                    op: op.clone(),
                    witness: witness.clone(),
                    env: env.clone(),
                });
                self.control = Control::Eval(arg.clone(), env);
            }
            Term::Handle { body, handler, cap } => {
                self.stack.push(Frame::HandleCap {
                    body: body.clone(),
                    handler: handler.clone(),
                    env: env.clone(),
                });
                self.control = Control::Eval(cap.clone(), env);
            }
            Term::Resume { kont, arg } => {
                self.stack.push(Frame::ResumeKArg { arg: arg.clone(), env: env.clone() });
                self.control = Control::Eval(kont.clone(), env);
            }
            Term::Ctor { name, args } => {
                if args.is_empty() {
                    self.control = Control::Ret(Value::Ctor { name: name.clone(), args: vec![] });
                } else {
                    let mut rest = args.clone();
                    let first = rest.remove(0);
                    self.stack.push(Frame::CtorArgs {
                        name: name.clone(),
                        done: vec![],
                        rest,
                        env: env.clone(),
                    });
                    self.control = Control::Eval(Rc::new(first), env);
                }
            }
            Term::Prim { op, args } => {
                if args.is_empty() {
                    self.control = Control::Ret(apply_prim(*op, &[])?);
                } else {
                    let mut rest = args.clone();
                    let first = rest.remove(0);
                    self.stack.push(Frame::PrimArgs {
                        op: *op,
                        done: vec![],
                        rest,
                        env: env.clone(),
                    });
                    self.control = Control::Eval(Rc::new(first), env);
                }
            }
            Term::Match { scrut, arms } => {
                self.stack.push(Frame::Match { arms: arms.clone(), env: env.clone() });
                self.control = Control::Eval(scrut.clone(), env);
            }
        }
        Ok(())
    }

    fn ret(&mut self, value: Value) -> Result<Option<Outcome>, EntoliError> {
        let frame = match self.stack.pop() {
            None => return Ok(Some(Outcome::Value(value))),
            Some(f) => f,
        };
        match frame {
            Frame::AppFn { arg, env } => {
                self.stack.push(Frame::AppArg { func: value });
                self.control = Control::Eval(arg, env);
            }
            Frame::AppArg { func } => match func {
                Value::Closure { param, body, env } => {
                    self.control = Control::Eval(body, env.bind(param, value));
                }
                other => {
                    return Err(EntoliError::Dynamic(format!(
                        "application of non-function value {other:?}"
                    )))
                }
            },
            Frame::Let { name, body, env } => {
                self.control = Control::Eval(body, env.bind(name, value));
            }
            Frame::If { then, els, env } => match value {
                Value::Bool(true) => self.control = Control::Eval(then, env),
                Value::Bool(false) => self.control = Control::Eval(els, env),
                other => {
                    return Err(EntoliError::Dynamic(format!("if on non-boolean {other:?}")))
                }
            },
            Frame::DoArg { effect, op, witness, env } => match witness {
                Some(wterm) => {
                    self.stack.push(Frame::DoWitness { effect, op, arg: value });
                    self.control = Control::Eval(wterm, env);
                }
                None => return self.perform(effect, op, value, None),
            },
            Frame::DoWitness { effect, op, arg } => {
                let w = match value {
                    Value::Str(s) => s,
                    Value::Witness(s) => s,
                    other => {
                        return Err(EntoliError::Dynamic(format!(
                            "witness must be a string or witness value, got {other:?}"
                        )))
                    }
                };
                return self.perform(effect, op, arg, Some(w));
            }
            Frame::HandleCap { body, handler, env } => {
                let cap = match value {
                    Value::Cap(id) => id,
                    other => {
                        return Err(EntoliError::MissingCapability {
                            effect: handler.effect.clone(),
                            detail: format!("`using` expression evaluated to {other:?}, not a capability"),
                        })
                    }
                };
                match self.authority.effect_of(cap) {
                    Some(e) if e == handler.effect => {}
                    Some(e) => {
                        return Err(EntoliError::MissingCapability {
                            effect: handler.effect.clone(),
                            detail: format!("capability grants {e}, handler is for {}", handler.effect),
                        })
                    }
                    None => {
                        return Err(EntoliError::MissingCapability {
                            effect: handler.effect.clone(),
                            detail: "capability is not a live grant".into(),
                        })
                    }
                }
                self.stack.push(Frame::Handle { handler, cap, env: env.clone() });
                self.control = Control::Eval(body, env);
            }
            Frame::Handle { handler, env, .. } => {
                // Normal return through the delimiter: apply the return clause.
                match &handler.return_clause {
                    Some((x, e)) => {
                        self.control = Control::Eval(e.clone(), env.bind(x.clone(), value));
                    }
                    None => self.control = Control::Ret(value),
                }
            }
            Frame::ClauseBoundary { .. } => {
                // Clause body finished without resuming: the captured continuation is
                // dropped (affine drop = abort). The clause's value flows outward.
                self.control = Control::Ret(value);
            }
            Frame::ResumeBoundary { .. } => {
                self.control = Control::Ret(value);
            }
            Frame::ResumeKArg { arg, env } => {
                self.stack.push(Frame::ResumeApply { kont: value });
                self.control = Control::Eval(arg, env);
            }
            Frame::ResumeApply { kont } => return self.do_resume(kont, value).map(|()| None),
            Frame::ForeignArg { name } => {
                let frames = std::mem::take(&mut self.stack);
                return Ok(Some(Outcome::ForeignCall { name, arg: value, frames }));
            }
            Frame::CtorArgs { name, mut done, mut rest, env } => {
                done.push(value);
                if rest.is_empty() {
                    self.control = Control::Ret(Value::Ctor { name, args: done });
                } else {
                    let next = rest.remove(0);
                    self.stack.push(Frame::CtorArgs { name, done, rest, env: env.clone() });
                    self.control = Control::Eval(Rc::new(next), env);
                }
            }
            Frame::PrimArgs { op, mut done, mut rest, env } => {
                done.push(value);
                if rest.is_empty() {
                    self.control = Control::Ret(apply_prim(op, &done)?);
                } else {
                    let next = rest.remove(0);
                    self.stack.push(Frame::PrimArgs { op, done, rest, env: env.clone() });
                    self.control = Control::Eval(Rc::new(next), env);
                }
            }
            Frame::Match { arms, env } => {
                for arm in arms.iter() {
                    match &arm.pat {
                        Pattern::Any(b) => {
                            self.control =
                                Control::Eval(Rc::new(arm.body.clone()), env.bind(b.clone(), value));
                            return Ok(None);
                        }
                        Pattern::Ctor { name, binders } => {
                            if let Value::Ctor { name: vn, args } = &value {
                                if vn == name && args.len() == binders.len() {
                                    let mut e = env.clone();
                                    for (b, v) in binders.iter().zip(args.iter()) {
                                        e = e.bind(b.clone(), v.clone());
                                    }
                                    self.control = Control::Eval(Rc::new(arm.body.clone()), e);
                                    return Ok(None);
                                }
                            }
                        }
                    }
                }
                return Err(EntoliError::Dynamic(format!("no match arm for {value:?}")));
            }
        }
        Ok(None)
    }

    /// `do E.op(v)` — dispatch on the effect's suspension class (§4.1).
    fn perform(
        &mut self,
        effect: String,
        op: String,
        arg: Value,
        witness: Option<String>,
    ) -> Result<Option<Outcome>, EntoliError> {
        let decl = self
            .program
            .effects
            .get(&effect)
            .ok_or_else(|| EntoliError::UnknownOperation { effect: effect.clone(), op: op.clone() })?;
        let opdecl = decl
            .op(&op)
            .ok_or_else(|| EntoliError::UnknownOperation { effect: effect.clone(), op: op.clone() })?
            .clone();

        match decl.class {
            SuspensionClass::Prompt => {
                if witness.is_some() {
                    return Err(EntoliError::Dynamic(format!(
                        "witness supplied to prompt operation {effect}.{op}"
                    )));
                }
                self.perform_prompt(effect, op, arg).map(|()| None)
            }
            SuspensionClass::Suspending => {
                if matches!(opdecl.delivery, DeliveryClass::AtLeastOnce) && witness.is_none() {
                    return Err(EntoliError::MissingWitness { effect, op });
                }
                // §4.3 dynamic proxy: a suspension while any handler clause is servicing
                // an operation would put a second entry on the pending stack — E005.
                if self
                    .stack
                    .iter()
                    .any(|f| matches!(f, Frame::ClauseBoundary { .. }))
                {
                    return Err(EntoliError::NestedSuspension { effect, op });
                }
                // Dual dispatch (unification §5 / migration M2): the innermost
                // installed handler for E, if any, is what the runtime will execute
                // as the crash window. The Handle frame is NOT removed — the whole
                // continuation, delimiter included, is captured for durability.
                let handler = self
                    .stack
                    .iter()
                    .rev()
                    .find_map(|f| match f {
                        Frame::Handle { handler, cap, env }
                            if handler.effect == effect && handler.clause(&op).is_some() =>
                        {
                            Some(SuspendHandler {
                                clause: handler.clause(&op).unwrap().clone(),
                                env: env.clone(),
                                cap: *cap,
                            })
                        }
                        _ => None,
                    });
                // The entire continuation moves out, uniquely owned (§5.1). Nothing is
                // converted: frames are already in representable layout (§7.3).
                let frames = std::mem::take(&mut self.stack);
                Ok(Some(Outcome::Suspended(Suspension { effect, op, arg, witness, frames, handler })))
            }
        }
    }

    /// Prompt operation: delimited capture up to (and including — deep handlers) the
    /// nearest matching Handle frame; the clause body runs outside it.
    fn perform_prompt(&mut self, effect: String, op: String, arg: Value) -> Result<(), EntoliError> {
        let idx = self
            .stack
            .iter()
            .rposition(|f| matches!(f, Frame::Handle { handler, .. } if handler.effect == effect))
            .ok_or_else(|| EntoliError::UnhandledEffect { effect: effect.clone(), op: op.clone() })?;

        let (handler, henv) = match &self.stack[idx] {
            Frame::Handle { handler, env, .. } => (handler.clone(), env.clone()),
            _ => unreachable!(),
        };
        let clause = handler
            .clause(&op)
            .ok_or_else(|| EntoliError::UnknownOperation { effect: effect.clone(), op: op.clone() })?
            .clone();

        // Captured continuation: the Handle frame and everything above it.
        let captured: Vec<Frame> = self.stack.split_off(idx);
        let kont = new_kont(self.fresh_kont_id(), captured);

        self.stack.push(Frame::ClauseBoundary {
            effect: effect.clone(),
            op: op.clone(),
            class: SuspensionClass::Prompt,
        });
        let clause_env = henv
            .bind(clause.param.clone(), arg)
            .bind(clause.kont.clone(), Value::Kont(kont));
        self.control = Control::Eval(clause.body.clone(), clause_env);
        Ok(())
    }

    /// `resume k v` (§5): at most once (E003), and inside a handler clause only in tail
    /// position (E004) — mechanically: when the resume happens, nothing may stand
    /// between it and its clause boundary.
    fn do_resume(&mut self, kont: Value, value: Value) -> Result<(), EntoliError> {
        let cell = match kont {
            Value::Kont(k) => k,
            other => {
                return Err(EntoliError::Dynamic(format!("resume of non-continuation {other:?}")))
            }
        };
        let (kont_id, frames) = {
            let mut c = cell.borrow_mut();
            if c.consumed {
                return Err(EntoliError::DoubleResume { kont_id: c.id.clone() });
            }
            c.consumed = true;
            (c.id.clone(), std::mem::take(&mut c.frames))
        };

        // P-5: resuming the durable-window sentinel ENDS the clause — the value
        // is the commit value, and nothing in the body may run after it. The
        // sub-machine halts with a distinguished outcome; anything that would
        // have followed a non-tail resume is simply never reached, and the
        // runtime refuses the clause when a consumed sentinel coexists with a
        // normally-computed value.
        if kont_id == WINDOW_KONT_ID {
            // Tail means nothing remains to run in the window sub-machine: any
            // live frame here is computation scheduled AFTER the resume — the
            // non-tail shape the clause rules forbid (E004), refused rather
            // than silently truncated.
            if !self.stack.is_empty() {
                return Err(EntoliError::NonTailResume { kont_id });
            }
            self.window_return = Some(value);
            self.control = Control::Ret(Value::Unit);
            return Ok(());
        }

        // Tail-position check: if a clause boundary is live, it must be the top frame.
        if let Some(b) = self
            .stack
            .iter()
            .rposition(|f| matches!(f, Frame::ClauseBoundary { .. }))
        {
            if b != self.stack.len() - 1 {
                return Err(EntoliError::NonTailResume { kont_id });
            }
            self.stack.pop(); // the clause completes by transferring control
        }

        self.stack.push(Frame::ResumeBoundary { kont_id });
        self.stack.extend(frames);
        self.control = Control::Ret(value);
        Ok(())
    }
}

fn apply_prim(op: PrimOp, args: &[Value]) -> Result<Value, EntoliError> {
    use PrimOp::*;
    let int = |v: &Value| -> Result<i64, EntoliError> {
        match v {
            Value::Int(i) => Ok(*i),
            other => Err(EntoliError::Dynamic(format!("prim expected int, got {other:?}"))),
        }
    };
    let str_ = |v: &Value| -> Result<String, EntoliError> {
        match v {
            Value::Str(s) => Ok(s.clone()),
            Value::Witness(s) => Ok(s.clone()),
            other => Err(EntoliError::Dynamic(format!("prim expected string, got {other:?}"))),
        }
    };
    let need = |n: usize| -> Result<(), EntoliError> {
        if args.len() == n {
            Ok(())
        } else {
            Err(EntoliError::Dynamic(format!("prim {op:?} arity {n}, got {}", args.len())))
        }
    };
    Ok(match op {
        Add => {
            need(2)?;
            Value::Int(int(&args[0])? + int(&args[1])?)
        }
        Sub => {
            need(2)?;
            Value::Int(int(&args[0])? - int(&args[1])?)
        }
        Mul => {
            need(2)?;
            Value::Int(int(&args[0])? * int(&args[1])?)
        }
        Lt => {
            need(2)?;
            Value::Bool(int(&args[0])? < int(&args[1])?)
        }
        IntEq => {
            need(2)?;
            Value::Bool(int(&args[0])? == int(&args[1])?)
        }
        StrEq => {
            need(2)?;
            Value::Bool(str_(&args[0])? == str_(&args[1])?)
        }
        Concat => {
            need(2)?;
            Value::Str(format!("{}{}", str_(&args[0])?, str_(&args[1])?))
        }
        StrOfInt => {
            need(1)?;
            Value::Str(int(&args[0])?.to_string())
        }
    })
}
