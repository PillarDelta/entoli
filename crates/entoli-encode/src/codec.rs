//! The closed encoding of values, environments, and frames. Every arm of the runtime's
//! data is covered except `Value::Cap`, which has **no** byte form: capabilities encode
//! as `CapSlot { effect }` — a name that must be re-granted at decode (Rule 4, §9.1).
//!
//! An encoded continuation opens with a header naming every top-level definition and
//! every effect it references, with their content hashes (§10.1). Decode verifies the
//! header against the loaded program before reconstructing a single frame: code cannot
//! silently change beneath a sleeping continuation.

use crate::bytes::{ReadError, Reader, Writer};
use crate::hash::{def_hashes, effect_sig, globals_of, Hash256};
use entoli_core::{CapAuthority, CapId, Env, Frame, KontCell, Value};
use entoli_syntax::{
    Arm, Handler, OpClause, Pattern, PrimOp, Program, SuspensionClass, Term,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const KONT_MAGIC: &[u8; 5] = b"ENTK2";
const VALUE_MAGIC: &[u8; 5] = b"ENTV2";

/// The two conditions the Phase 5 checker will make unrepresentable (I3); neither is an
/// I/O failure — encode stays pure and total over checked programs (Rule 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A capability value with no live grant: it cannot even be named.
    UnknownCapability(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Malformed(String),
    /// A referenced definition's content hash differs from the loaded program's (§10.1).
    DefDrift { name: String },
    /// A referenced effect's signature differs (§10.2, computed half).
    EffectDrift { effect: String },
    /// E014 — no grant in force at resume for a capability slot (§9.1).
    MissingResumeGrant { effect: String },
}

impl From<ReadError> for DecodeError {
    fn from(e: ReadError) -> Self {
        DecodeError::Malformed(e.0)
    }
}

/// Effect → capability, in force at resume time. Deliberately the reverse direction of
/// `CapAuthority`: persisted slots name effects, never capabilities.
pub trait ResumeGranter {
    fn grant_for(&self, effect: &str) -> Option<CapId>;
}

#[derive(Debug, Default, Clone)]
pub struct MapGranter {
    pub grants: BTreeMap<String, CapId>,
}

impl MapGranter {
    pub fn grant(&mut self, effect: &str, cap: CapId) {
        self.grants.insert(effect.to_string(), cap);
    }
}

impl ResumeGranter for MapGranter {
    fn grant_for(&self, effect: &str) -> Option<CapId> {
        self.grants.get(effect).copied()
    }
}

// ---------------------------------------------------------------------------
// Canonical term bytes (shared with content hashing).
// ---------------------------------------------------------------------------

pub fn write_term_canonical(
    w: &mut Writer,
    term: &Term,
    global: &mut dyn FnMut(&mut Writer, &str),
) {
    match term {
        Term::Var(n) => {
            w.u8(0);
            w.str(n);
        }
        Term::Unit => w.u8(1),
        Term::Int(i) => {
            w.u8(2);
            w.i64(*i);
        }
        Term::Bool(b) => {
            w.u8(3);
            w.u8(*b as u8);
        }
        Term::Str(s) => {
            w.u8(4);
            w.str(s);
        }
        Term::Lam { param, body } => {
            w.u8(5);
            w.str(param);
            write_term_canonical(w, body, global);
        }
        Term::App { func, arg } => {
            w.u8(6);
            write_term_canonical(w, func, global);
            write_term_canonical(w, arg, global);
        }
        Term::Let { name, bound, body } => {
            w.u8(7);
            w.str(name);
            write_term_canonical(w, bound, global);
            write_term_canonical(w, body, global);
        }
        Term::If { cond, then, els } => {
            w.u8(8);
            write_term_canonical(w, cond, global);
            write_term_canonical(w, then, global);
            write_term_canonical(w, els, global);
        }
        Term::Do { effect, op, arg, witness } => {
            w.u8(9);
            w.str(effect);
            w.str(op);
            write_term_canonical(w, arg, global);
            match witness {
                None => w.u8(0),
                Some(t) => {
                    w.u8(1);
                    write_term_canonical(w, t, global);
                }
            }
        }
        Term::Handle { body, handler, cap } => {
            w.u8(10);
            write_term_canonical(w, body, global);
            write_handler(w, handler, global);
            write_term_canonical(w, cap, global);
        }
        Term::Resume { kont, arg } => {
            w.u8(11);
            write_term_canonical(w, kont, global);
            write_term_canonical(w, arg, global);
        }
        Term::Ctor { name, args } => {
            w.u8(12);
            w.str(name);
            w.u32(args.len() as u32);
            for a in args {
                write_term_canonical(w, a, global);
            }
        }
        Term::Match { scrut, arms } => {
            w.u8(13);
            write_term_canonical(w, scrut, global);
            w.u32(arms.len() as u32);
            for a in arms.iter() {
                write_pattern(w, &a.pat);
                write_term_canonical(w, &a.body, global);
            }
        }
        Term::Prim { op, args } => {
            w.u8(14);
            w.u8(prim_tag(*op));
            w.u32(args.len() as u32);
            for a in args {
                write_term_canonical(w, a, global);
            }
        }
        Term::Global(n) => {
            w.u8(15);
            global(w, n);
        }
        Term::ForeignApp { name, arg } => {
            w.u8(16);
            w.str(name);
            write_term_canonical(w, arg, global);
        }
    }
}

fn write_handler(w: &mut Writer, h: &Handler, global: &mut dyn FnMut(&mut Writer, &str)) {
    w.str(&h.effect);
    match &h.return_clause {
        None => w.u8(0),
        Some((x, e)) => {
            w.u8(1);
            w.str(x);
            write_term_canonical(w, e, global);
        }
    }
    w.u32(h.clauses.len() as u32);
    for c in &h.clauses {
        w.str(&c.op);
        w.str(&c.param);
        w.str(&c.kont);
        write_term_canonical(w, &c.body, global);
    }
}

fn write_pattern(w: &mut Writer, p: &Pattern) {
    match p {
        Pattern::Ctor { name, binders } => {
            w.u8(0);
            w.str(name);
            w.u32(binders.len() as u32);
            for b in binders {
                w.str(b);
            }
        }
        Pattern::Any(b) => {
            w.u8(1);
            w.str(b);
        }
    }
}

fn prim_tag(op: PrimOp) -> u8 {
    match op {
        PrimOp::Add => 0,
        PrimOp::Sub => 1,
        PrimOp::Mul => 2,
        PrimOp::Lt => 3,
        PrimOp::IntEq => 4,
        PrimOp::StrEq => 5,
        PrimOp::Concat => 6,
        PrimOp::StrOfInt => 7,
    }
}

fn prim_of(tag: u8) -> Result<PrimOp, DecodeError> {
    Ok(match tag {
        0 => PrimOp::Add,
        1 => PrimOp::Sub,
        2 => PrimOp::Mul,
        3 => PrimOp::Lt,
        4 => PrimOp::IntEq,
        5 => PrimOp::StrEq,
        6 => PrimOp::Concat,
        7 => PrimOp::StrOfInt,
        t => return Err(DecodeError::Malformed(format!("bad prim tag {t}"))),
    })
}

// ---------------------------------------------------------------------------
// Codec
// ---------------------------------------------------------------------------

/// Encode-side sharing context: Rc identity of every not-yet-consumed continuation
/// already written, in traversal order. Preserves aliasing across the durable boundary
/// (review finding R-4): a shared continuation must decode as ONE cell, or affinity
/// becomes per-copy instead of per-continuation.
#[derive(Default)]
pub struct EncCtx {
    seen: Vec<usize>,
    /// Env-node sharing table (T3.2 / audit A-18): Rc identity -> post-order id.
    /// Without it, every frame re-encodes its whole environment chain and blobs
    /// grow quadratically with captured depth (measured 333 KB at depth 200).
    envs: std::collections::HashMap<usize, u32>,
}

#[derive(Default)]
pub struct DecCtx {
    konts: Vec<entoli_core::Kont>,
    /// Decoded env nodes by encode-side post-order id; sharing is restored, so
    /// decode yields one node per persisted identity.
    envs: Vec<Env>,
}

pub struct Codec<'a> {
    pub program: &'a Program,
    pub sigs: &'a entoli_types::Signatures,
    pub def_hashes: BTreeMap<String, Hash256>,
    pub authority: &'a dyn CapAuthority,
}

impl<'a> Codec<'a> {
    pub fn new(
        program: &'a Program,
        sigs: &'a entoli_types::Signatures,
        authority: &'a dyn CapAuthority,
    ) -> Self {
        Codec { program, sigs, def_hashes: def_hashes(program), authority }
    }

    // -- encode ------------------------------------------------------------

    pub fn encode_kont(&self, frames: &[Frame]) -> Result<Vec<u8>, EncodeError> {
        let mut refs = Refs::default();
        for f in frames {
            self.collect_frame(f, &mut refs);
        }
        let mut w = Writer::new();
        w.raw(KONT_MAGIC);
        self.write_header(&mut w, &refs);
        let mut enc = EncCtx::default();
        self.write_frames(&mut w, frames, &mut enc)?;
        Ok(w.buf)
    }

    pub fn encode_value(&self, v: &Value) -> Result<Vec<u8>, EncodeError> {
        let mut refs = Refs::default();
        self.collect_value(v, &mut refs);
        let mut w = Writer::new();
        w.raw(VALUE_MAGIC);
        self.write_header(&mut w, &refs);
        let mut enc = EncCtx::default();
        self.write_value(&mut w, v, &mut enc)?;
        Ok(w.buf)
    }

    fn write_header(&self, w: &mut Writer, refs: &Refs) {
        // Names unknown at encode time are recorded with a zero sentinel: "this name
        // had no definition when I slept" is itself an identity to verify (R-6).
        const ABSENT: Hash256 = [0u8; 32];
        w.u32(refs.globals.len() as u32);
        for g in &refs.globals {
            w.str(g);
            w.raw(self.def_hashes.get(g).unwrap_or(&ABSENT));
        }
        w.u32(refs.effects.len() as u32);
        for e in &refs.effects {
            w.str(e);
            match self.program.effects.get(e) {
                Some(d) => w.raw(&effect_sig(d, self.sigs)),
                None => w.raw(&ABSENT),
            }
        }
    }

    fn write_frames(&self, w: &mut Writer, frames: &[Frame], enc: &mut EncCtx) -> Result<(), EncodeError> {
        w.u32(frames.len() as u32);
        for f in frames {
            self.write_frame(w, f, enc)?;
        }
        Ok(())
    }

    fn write_frame(&self, w: &mut Writer, f: &Frame, enc: &mut EncCtx) -> Result<(), EncodeError> {
        let mut name_global = |w: &mut Writer, g: &str| {
            w.str(g);
        };
        match f {
            Frame::AppFn { arg, env } => {
                w.u8(0);
                write_term_canonical(w, arg, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::AppArg { func } => {
                w.u8(1);
                self.write_value(w, func, enc)?;
            }
            Frame::Let { name, body, env } => {
                w.u8(2);
                w.str(name);
                write_term_canonical(w, body, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::If { then, els, env } => {
                w.u8(3);
                write_term_canonical(w, then, &mut name_global);
                write_term_canonical(w, els, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::DoArg { effect, op, witness, env } => {
                w.u8(4);
                w.str(effect);
                w.str(op);
                match witness {
                    None => w.u8(0),
                    Some(t) => {
                        w.u8(1);
                        write_term_canonical(w, t, &mut name_global);
                    }
                }
                self.write_env(w, env, enc)?;
            }
            Frame::DoWitness { effect, op, arg } => {
                w.u8(5);
                w.str(effect);
                w.str(op);
                self.write_value(w, arg, enc)?;
            }
            Frame::HandleCap { body, handler, env } => {
                w.u8(6);
                write_term_canonical(w, body, &mut name_global);
                write_handler(w, handler, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::Handle { handler, env, .. } => {
                // The live CapId is Kcaps and is NOT written (§9.1): the handler's
                // effect name identifies what must be re-granted at resume.
                w.u8(7);
                write_handler(w, handler, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::ClauseBoundary { effect, op, class } => {
                w.u8(8);
                w.str(effect);
                w.str(op);
                w.u8(match class {
                    SuspensionClass::Suspending => 0,
                    SuspensionClass::Prompt => 1,
                });
            }
            Frame::ResumeBoundary { kont_id } => {
                w.u8(9);
                w.str(kont_id);
            }
            Frame::ResumeKArg { arg, env } => {
                w.u8(10);
                write_term_canonical(w, arg, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Frame::ResumeApply { kont } => {
                w.u8(11);
                self.write_value(w, kont, enc)?;
            }
            Frame::CtorArgs { name, done, rest, env } => {
                w.u8(12);
                w.str(name);
                w.u32(done.len() as u32);
                for v in done {
                    self.write_value(w, v, enc)?;
                }
                w.u32(rest.len() as u32);
                for t in rest {
                    write_term_canonical(w, t, &mut name_global);
                }
                self.write_env(w, env, enc)?;
            }
            Frame::PrimArgs { op, done, rest, env } => {
                w.u8(13);
                w.u8(prim_tag(*op));
                w.u32(done.len() as u32);
                for v in done {
                    self.write_value(w, v, enc)?;
                }
                w.u32(rest.len() as u32);
                for t in rest {
                    write_term_canonical(w, t, &mut name_global);
                }
                self.write_env(w, env, enc)?;
            }
            Frame::ForeignArg { name } => {
                w.u8(15);
                w.str(name);
            }
            Frame::Match { arms, env } => {
                w.u8(14);
                w.u32(arms.len() as u32);
                for a in arms.iter() {
                    write_pattern(w, &a.pat);
                    write_term_canonical(w, &a.body, &mut name_global);
                }
                self.write_env(w, env, enc)?;
            }
        }
        Ok(())
    }

    fn write_env(&self, w: &mut Writer, env: &Env, enc: &mut EncCtx) -> Result<(), EncodeError> {
        // Structure-preserving encoding: 0 = empty, 1 = node (name, value, next),
        // 2 = back-reference to an already-written node. Ids are assigned in
        // post-order (a node's id is written down only after its tail), so every
        // back-reference points at a completed node; chains are acyclic.
        match env.node() {
            None => w.u8(0),
            Some(rc) => {
                let ptr = std::rc::Rc::as_ptr(rc) as usize;
                if let Some(&id) = enc.envs.get(&ptr) {
                    w.u8(2);
                    w.u32(id);
                } else {
                    w.u8(1);
                    w.str(&rc.name);
                    self.write_value(w, &rc.value, enc)?;
                    self.write_env(w, &rc.next, enc)?;
                    let id = enc.envs.len() as u32;
                    enc.envs.insert(ptr, id);
                }
            }
        }
        Ok(())
    }

    fn write_value(&self, w: &mut Writer, v: &Value, enc: &mut EncCtx) -> Result<(), EncodeError> {
        let mut name_global = |w: &mut Writer, g: &str| {
            w.str(g);
        };
        match v {
            Value::Unit => w.u8(0),
            Value::Int(i) => {
                w.u8(1);
                w.i64(*i);
            }
            Value::Bool(b) => {
                w.u8(2);
                w.u8(*b as u8);
            }
            Value::Str(s) => {
                w.u8(3);
                w.str(s);
            }
            Value::Ctor { name, args } => {
                w.u8(4);
                w.str(name);
                w.u32(args.len() as u32);
                for a in args {
                    self.write_value(w, a, enc)?;
                }
            }
            Value::Closure { param, body, env } => {
                w.u8(5);
                w.str(param);
                write_term_canonical(w, body, &mut name_global);
                self.write_env(w, env, enc)?;
            }
            Value::Kont(k) => {
                let ptr = Rc::as_ptr(k) as usize;
                if let Some(idx) = enc.seen.iter().position(|p| *p == ptr) {
                    // Already written: a back-reference, so aliasing survives the
                    // boundary and affinity stays per-continuation (R-4).
                    w.u8(9);
                    w.u32(idx as u32);
                } else {
                    let cell = k.borrow();
                    if cell.consumed {
                        // A consumed continuation has no future, but it may still sit
                        // in a captured environment; refusing here would make encode
                        // partial over reachable states (R-5, Rule 2). A tombstone
                        // preserves the semantics: resuming it after decode is E003.
                        w.u8(10);
                        w.str(&cell.id);
                    } else {
                        enc.seen.push(ptr);
                        w.u8(6);
                        w.str(&cell.id);
                        self.write_frames(w, &cell.frames, enc)?;
                    }
                }
            }
            Value::Cap(id) => {
                // Kframes ⊗ Kcaps: only the *name* of what was granted crosses (§9.1).
                let effect = self
                    .authority
                    .effect_of(*id)
                    .ok_or(EncodeError::UnknownCapability(id.0))?;
                w.u8(7);
                w.str(&effect);
            }
            Value::Witness(s) => {
                w.u8(8);
                w.str(s);
            }
        }
        Ok(())
    }

    // -- decode ------------------------------------------------------------

    pub fn decode_kont(
        &self,
        bytes: &[u8],
        granter: &dyn ResumeGranter,
    ) -> Result<Vec<Frame>, DecodeError> {
        let mut r = Reader::new(bytes);
        let magic = r.raw(5)?;
        if magic != KONT_MAGIC {
            return Err(DecodeError::Malformed("bad kont magic".into()));
        }
        self.check_header(&mut r)?;
        let mut dec = DecCtx::default();
        let frames = self.read_frames(&mut r, granter, &mut dec)?;
        if !r.done() {
            return Err(DecodeError::Malformed("trailing bytes after continuation".into()));
        }
        Ok(frames)
    }

    pub fn decode_value(
        &self,
        bytes: &[u8],
        granter: &dyn ResumeGranter,
    ) -> Result<Value, DecodeError> {
        let mut r = Reader::new(bytes);
        let magic = r.raw(5)?;
        if magic != VALUE_MAGIC {
            return Err(DecodeError::Malformed("bad value magic".into()));
        }
        self.check_header(&mut r)?;
        let mut dec = DecCtx::default();
        let v = self.read_value(&mut r, granter, &mut dec)?;
        if !r.done() {
            return Err(DecodeError::Malformed("trailing bytes after value".into()));
        }
        Ok(v)
    }

    fn check_header(&self, r: &mut Reader) -> Result<(), DecodeError> {
        let ndefs = r.u32()?;
        for _ in 0..ndefs {
            let name = r.str()?;
            let recorded: Hash256 = r
                .raw(32)?
                .try_into()
                .map_err(|_| DecodeError::Malformed("bad hash".into()))?;
            let absent = recorded == [0u8; 32];
            match (self.def_hashes.get(&name), absent) {
                (Some(current), false) if *current == recorded => {}
                (None, true) => {}
                _ => return Err(DecodeError::DefDrift { name }),
            }
        }
        let neffs = r.u32()?;
        for _ in 0..neffs {
            let name = r.str()?;
            let recorded: Hash256 = r
                .raw(32)?
                .try_into()
                .map_err(|_| DecodeError::Malformed("bad hash".into()))?;
            let absent = recorded == [0u8; 32];
            match (self.program.effects.get(&name), absent) {
                (Some(decl), false) if effect_sig(decl, self.sigs) == recorded => {}
                (None, true) => {}
                _ => return Err(DecodeError::EffectDrift { effect: name }),
            }
        }
        Ok(())
    }

    fn read_frames(
        &self,
        r: &mut Reader,
        granter: &dyn ResumeGranter,
        dec: &mut DecCtx,
    ) -> Result<Vec<Frame>, DecodeError> {
        let n = r.u32()?;
        let mut frames = Vec::with_capacity(n as usize);
        for _ in 0..n {
            frames.push(self.read_frame(r, granter, dec)?);
        }
        Ok(frames)
    }

    fn read_frame(&self, r: &mut Reader, granter: &dyn ResumeGranter, dec: &mut DecCtx) -> Result<Frame, DecodeError> {
        Ok(match r.u8()? {
            0 => Frame::AppFn { arg: Rc::new(read_term(r)?), env: self.read_env(r, granter, dec)? },
            1 => Frame::AppArg { func: self.read_value(r, granter, dec)? },
            2 => Frame::Let {
                name: r.str()?,
                body: Rc::new(read_term(r)?),
                env: self.read_env(r, granter, dec)?,
            },
            3 => Frame::If {
                then: Rc::new(read_term(r)?),
                els: Rc::new(read_term(r)?),
                env: self.read_env(r, granter, dec)?,
            },
            4 => {
                let effect = r.str()?;
                let op = r.str()?;
                let witness = match r.u8()? {
                    0 => None,
                    1 => Some(Rc::new(read_term(r)?)),
                    t => return Err(DecodeError::Malformed(format!("bad witness tag {t}"))),
                };
                Frame::DoArg { effect, op, witness, env: self.read_env(r, granter, dec)? }
            }
            5 => Frame::DoWitness {
                effect: r.str()?,
                op: r.str()?,
                arg: self.read_value(r, granter, dec)?,
            },
            6 => Frame::HandleCap {
                body: Rc::new(read_term(r)?),
                handler: Rc::new(read_handler(r)?),
                env: self.read_env(r, granter, dec)?,
            },
            7 => {
                let handler = Rc::new(read_handler(r)?);
                let env = self.read_env(r, granter, dec)?;
                // Re-grant at resume, from the authority in force *now* (§9.1). This is
                // where a revoked grant halts a sleeping computation: E014.
                let cap = granter
                    .grant_for(&handler.effect)
                    .ok_or_else(|| DecodeError::MissingResumeGrant { effect: handler.effect.clone() })?;
                Frame::Handle { handler, cap, env }
            }
            8 => Frame::ClauseBoundary {
                effect: r.str()?,
                op: r.str()?,
                class: match r.u8()? {
                    0 => SuspensionClass::Suspending,
                    1 => SuspensionClass::Prompt,
                    t => return Err(DecodeError::Malformed(format!("bad class tag {t}"))),
                },
            },
            9 => Frame::ResumeBoundary { kont_id: r.str()? },
            10 => Frame::ResumeKArg {
                arg: Rc::new(read_term(r)?),
                env: self.read_env(r, granter, dec)?,
            },
            11 => Frame::ResumeApply { kont: self.read_value(r, granter, dec)? },
            12 => {
                let name = r.str()?;
                let ndone = r.u32()?;
                let mut done = Vec::with_capacity(ndone as usize);
                for _ in 0..ndone {
                    done.push(self.read_value(r, granter, dec)?);
                }
                let nrest = r.u32()?;
                let mut rest = Vec::with_capacity(nrest as usize);
                for _ in 0..nrest {
                    rest.push(read_term(r)?);
                }
                Frame::CtorArgs { name, done, rest, env: self.read_env(r, granter, dec)? }
            }
            13 => {
                let op = prim_of(r.u8()?)?;
                let ndone = r.u32()?;
                let mut done = Vec::with_capacity(ndone as usize);
                for _ in 0..ndone {
                    done.push(self.read_value(r, granter, dec)?);
                }
                let nrest = r.u32()?;
                let mut rest = Vec::with_capacity(nrest as usize);
                for _ in 0..nrest {
                    rest.push(read_term(r)?);
                }
                Frame::PrimArgs { op, done, rest, env: self.read_env(r, granter, dec)? }
            }
            14 => {
                let n = r.u32()?;
                let mut arms = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    let pat = read_pattern(r)?;
                    let body = read_term(r)?;
                    arms.push(Arm { pat, body });
                }
                Frame::Match { arms: Rc::new(arms), env: self.read_env(r, granter, dec)? }
            }
            15 => Frame::ForeignArg { name: r.str()? },
            t => return Err(DecodeError::Malformed(format!("bad frame tag {t}"))),
        })
    }

    fn read_env(&self, r: &mut Reader, granter: &dyn ResumeGranter, dec: &mut DecCtx) -> Result<Env, DecodeError> {
        match r.u8()? {
            0 => Ok(Env::empty()),
            2 => {
                let id = r.u32()? as usize;
                dec.envs
                    .get(id)
                    .cloned()
                    .ok_or_else(|| DecodeError::Malformed(format!("env backref {id} out of range")))
            }
            1 => {
                let name = r.str()?;
                let value = self.read_value(r, granter, dec)?;
                let next = self.read_env(r, granter, dec)?;
                let env = Env::from_node(Some(std::rc::Rc::new(entoli_core::EnvNode {
                    name,
                    value,
                    next,
                })));
                dec.envs.push(env.clone());
                Ok(env)
            }
            t => Err(DecodeError::Malformed(format!("bad env tag {t}"))),
        }
    }

    fn read_value(&self, r: &mut Reader, granter: &dyn ResumeGranter, dec: &mut DecCtx) -> Result<Value, DecodeError> {
        Ok(match r.u8()? {
            0 => Value::Unit,
            1 => Value::Int(r.i64()?),
            2 => Value::Bool(r.u8()? != 0),
            3 => Value::Str(r.str()?),
            4 => {
                let name = r.str()?;
                let n = r.u32()?;
                let mut args = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    args.push(self.read_value(r, granter, dec)?);
                }
                Value::Ctor { name, args }
            }
            5 => {
                let param = r.str()?;
                let body = Rc::new(read_term(r)?);
                let env = self.read_env(r, granter, dec)?;
                Value::Closure { param, body, env }
            }
            6 => {
                let id = r.str()?;
                let kont = Rc::new(RefCell::new(KontCell { id, consumed: false, frames: vec![] }));
                dec.konts.push(kont.clone());
                let frames = self.read_frames(r, granter, dec)?;
                kont.borrow_mut().frames = frames;
                Value::Kont(kont)
            }
            7 => {
                let effect = r.str()?;
                let cap = granter
                    .grant_for(&effect)
                    .ok_or(DecodeError::MissingResumeGrant { effect })?;
                Value::Cap(cap)
            }
            8 => Value::Witness(r.str()?),
            9 => {
                let idx = r.u32()? as usize;
                let kont = dec
                    .konts
                    .get(idx)
                    .ok_or_else(|| DecodeError::Malformed(format!("bad kont backref {idx}")))?;
                Value::Kont(kont.clone())
            }
            10 => {
                let id = r.str()?;
                Value::Kont(Rc::new(RefCell::new(KontCell { id, consumed: true, frames: vec![] })))
            }
            t => return Err(DecodeError::Malformed(format!("bad value tag {t}"))),
        })
    }

    /// The definitions and effects a continuation references, with their content
    /// hashes — what the runtime records as `handled_effects` (§10.2, computed half).
    pub fn kont_refs(
        &self,
        frames: &[Frame],
    ) -> (Vec<(String, Hash256)>, Vec<(String, Hash256)>) {
        let mut refs = Refs::default();
        for f in frames {
            self.collect_frame(f, &mut refs);
        }
        let defs = refs
            .globals
            .iter()
            .filter_map(|g| self.def_hashes.get(g).map(|h| (g.clone(), *h)))
            .collect();
        let effs = refs
            .effects
            .iter()
            .filter_map(|e| self.program.effects.get(e).map(|d| (e.clone(), effect_sig(d, self.sigs))))
            .collect();
        (defs, effs)
    }

    // -- reference collection ----------------------------------------------

    fn collect_frame(&self, f: &Frame, refs: &mut Refs) {
        match f {
            Frame::AppFn { arg, env } => {
                refs.term(arg);
                self.collect_env(env, refs);
            }
            Frame::AppArg { func } => self.collect_value(func, refs),
            Frame::Let { body, env, .. } => {
                refs.term(body);
                self.collect_env(env, refs);
            }
            Frame::If { then, els, env } => {
                refs.term(then);
                refs.term(els);
                self.collect_env(env, refs);
            }
            Frame::DoArg { effect, witness, env, .. } => {
                refs.effects.insert(effect.clone());
                if let Some(t) = witness {
                    refs.term(t);
                }
                self.collect_env(env, refs);
            }
            Frame::DoWitness { effect, arg, .. } => {
                refs.effects.insert(effect.clone());
                self.collect_value(arg, refs);
            }
            Frame::HandleCap { body, handler, env } => {
                refs.term(body);
                refs.handler(handler);
                self.collect_env(env, refs);
            }
            Frame::Handle { handler, env, .. } => {
                refs.handler(handler);
                self.collect_env(env, refs);
            }
            Frame::ClauseBoundary { effect, .. } => {
                refs.effects.insert(effect.clone());
            }
            Frame::ResumeBoundary { .. } | Frame::ForeignArg { .. } => {}
            Frame::ResumeKArg { arg, env } => {
                refs.term(arg);
                self.collect_env(env, refs);
            }
            Frame::ResumeApply { kont } => self.collect_value(kont, refs),
            Frame::CtorArgs { done, rest, env, .. } | Frame::PrimArgs { done, rest, env, .. } => {
                for v in done {
                    self.collect_value(v, refs);
                }
                for t in rest {
                    refs.term(t);
                }
                self.collect_env(env, refs);
            }
            Frame::Match { arms, env } => {
                for a in arms.iter() {
                    refs.term(&a.body);
                }
                self.collect_env(env, refs);
            }
        }
    }

    fn collect_env(&self, env: &Env, refs: &mut Refs) {
        for (_, v) in env.iter() {
            self.collect_value(v, refs);
        }
    }

    fn collect_value(&self, v: &Value, refs: &mut Refs) {
        match v {
            Value::Unit | Value::Int(_) | Value::Bool(_) | Value::Str(_) | Value::Witness(_) => {}
            Value::Ctor { args, .. } => {
                for a in args {
                    self.collect_value(a, refs);
                }
            }
            Value::Closure { body, env, .. } => {
                refs.term(body);
                self.collect_env(env, refs);
            }
            Value::Kont(k) => {
                for f in &k.borrow().frames {
                    self.collect_frame(f, refs);
                }
            }
            Value::Cap(id) => {
                if let Some(e) = self.authority.effect_of(*id) {
                    refs.effects.insert(e);
                }
            }
        }
    }
}

#[derive(Default)]
struct Refs {
    globals: BTreeSet<String>,
    effects: BTreeSet<String>,
}

impl Refs {
    fn term(&mut self, t: &Term) {
        globals_of(t, &mut self.globals);
        effects_of(t, &mut self.effects);
    }
    fn handler(&mut self, h: &Handler) {
        self.effects.insert(h.effect.clone());
        if let Some((_, e)) = &h.return_clause {
            self.term(e);
        }
        for c in &h.clauses {
            self.term(&c.body);
        }
    }
}

/// Effects mentioned by a term's `do` and `handle` nodes.
fn effects_of(term: &Term, acc: &mut BTreeSet<String>) {
    match term {
        Term::Do { effect, arg, witness, .. } => {
            acc.insert(effect.clone());
            effects_of(arg, acc);
            if let Some(w) = witness {
                effects_of(w, acc);
            }
        }
        Term::Handle { body, handler, cap } => {
            acc.insert(handler.effect.clone());
            effects_of(body, acc);
            effects_of(cap, acc);
            if let Some((_, e)) = &handler.return_clause {
                effects_of(e, acc);
            }
            for c in &handler.clauses {
                effects_of(&c.body, acc);
            }
        }
        Term::Var(_) | Term::Unit | Term::Int(_) | Term::Bool(_) | Term::Str(_) | Term::Global(_) => {}
        Term::ForeignApp { arg, .. } => effects_of(arg, acc),
        Term::Lam { body, .. } => effects_of(body, acc),
        Term::App { func, arg } => {
            effects_of(func, acc);
            effects_of(arg, acc);
        }
        Term::Let { bound, body, .. } => {
            effects_of(bound, acc);
            effects_of(body, acc);
        }
        Term::If { cond, then, els } => {
            effects_of(cond, acc);
            effects_of(then, acc);
            effects_of(els, acc);
        }
        Term::Resume { kont, arg } => {
            effects_of(kont, acc);
            effects_of(arg, acc);
        }
        Term::Ctor { args, .. } | Term::Prim { args, .. } => {
            for a in args {
                effects_of(a, acc);
            }
        }
        Term::Match { scrut, arms } => {
            effects_of(scrut, acc);
            for a in arms.iter() {
                effects_of(&a.body, acc);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Term reading (codec form: Global carries its name).
// ---------------------------------------------------------------------------

fn read_term(r: &mut Reader) -> Result<Term, DecodeError> {
    Ok(match r.u8()? {
        0 => Term::Var(r.str()?),
        1 => Term::Unit,
        2 => Term::Int(r.i64()?),
        3 => Term::Bool(r.u8()? != 0),
        4 => Term::Str(r.str()?),
        5 => Term::Lam { param: r.str()?, body: Rc::new(read_term(r)?) },
        6 => Term::App { func: Rc::new(read_term(r)?), arg: Rc::new(read_term(r)?) },
        7 => Term::Let {
            name: r.str()?,
            bound: Rc::new(read_term(r)?),
            body: Rc::new(read_term(r)?),
        },
        8 => Term::If {
            cond: Rc::new(read_term(r)?),
            then: Rc::new(read_term(r)?),
            els: Rc::new(read_term(r)?),
        },
        9 => {
            let effect = r.str()?;
            let op = r.str()?;
            let arg = Rc::new(read_term(r)?);
            let witness = match r.u8()? {
                0 => None,
                1 => Some(Rc::new(read_term(r)?)),
                t => return Err(DecodeError::Malformed(format!("bad witness tag {t}"))),
            };
            Term::Do { effect, op, arg, witness }
        }
        10 => Term::Handle {
            body: Rc::new(read_term(r)?),
            handler: Rc::new(read_handler(r)?),
            cap: Rc::new(read_term(r)?),
        },
        11 => Term::Resume { kont: Rc::new(read_term(r)?), arg: Rc::new(read_term(r)?) },
        12 => {
            let name = r.str()?;
            let n = r.u32()?;
            let mut args = Vec::with_capacity(n as usize);
            for _ in 0..n {
                args.push(read_term(r)?);
            }
            Term::Ctor { name, args }
        }
        13 => {
            let scrut = Rc::new(read_term(r)?);
            let n = r.u32()?;
            let mut arms = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let pat = read_pattern(r)?;
                let body = read_term(r)?;
                arms.push(Arm { pat, body });
            }
            Term::Match { scrut, arms: Rc::new(arms) }
        }
        14 => {
            let op = prim_of(r.u8()?)?;
            let n = r.u32()?;
            let mut args = Vec::with_capacity(n as usize);
            for _ in 0..n {
                args.push(read_term(r)?);
            }
            Term::Prim { op, args }
        }
        15 => Term::Global(r.str()?),
        16 => Term::ForeignApp { name: r.str()?, arg: Rc::new(read_term(r)?) },
        t => return Err(DecodeError::Malformed(format!("bad term tag {t}"))),
    })
}

fn read_handler(r: &mut Reader) -> Result<Handler, DecodeError> {
    let effect = r.str()?;
    let return_clause = match r.u8()? {
        0 => None,
        1 => {
            let x = r.str()?;
            let e = read_term(r)?;
            Some((x, Rc::new(e)))
        }
        t => return Err(DecodeError::Malformed(format!("bad return tag {t}"))),
    };
    let n = r.u32()?;
    let mut clauses = Vec::with_capacity(n as usize);
    for _ in 0..n {
        clauses.push(OpClause {
            op: r.str()?,
            param: r.str()?,
            kont: r.str()?,
            body: Rc::new(read_term(r)?),
        });
    }
    Ok(Handler { effect, return_clause, clauses })
}

fn read_pattern(r: &mut Reader) -> Result<Pattern, DecodeError> {
    Ok(match r.u8()? {
        0 => {
            let name = r.str()?;
            let n = r.u32()?;
            let mut binders = Vec::with_capacity(n as usize);
            for _ in 0..n {
                binders.push(r.str()?);
            }
            Pattern::Ctor { name, binders }
        }
        1 => Pattern::Any(r.str()?),
        t => return Err(DecodeError::Malformed(format!("bad pattern tag {t}"))),
    })
}
