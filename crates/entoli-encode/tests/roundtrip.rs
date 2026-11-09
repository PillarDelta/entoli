//! Phase 2 in-process properties: encode/decode round trips over generated values,
//! capability stripping and re-granting, code-drift detection, consumed-kont refusal,
//! and byte-level determinism. The cross-process kill test lives in entoli-cli.

use entoli_core::{CapId, Env, Machine, MapAuthority, Outcome, Value};
use entoli_encode::{Codec, DecodeError, MapGranter};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, PrimOp, Program, SuspensionClass,
};
use std::rc::Rc;

fn sigs0() -> &'static entoli_types::Signatures {
    use std::sync::OnceLock;
    static S: OnceLock<entoli_types::Signatures> = OnceLock::new();
    S.get_or_init(entoli_types::Signatures::default)
}

fn program() -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Mark".into(),
        EffectDecl {
            name: "Mark".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "here".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.effects.insert(
        "Ask".into(),
        EffectDecl {
            name: "Ask".into(),
            class: SuspensionClass::Prompt,
            ops: vec![OpDecl { name: "ask".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    let fib = lam(
        "n",
        if_(
            prim(PrimOp::Lt, vec![var("n"), int(2)]),
            var("n"),
            prim(
                PrimOp::Add,
                vec![
                    app(global("fib"), prim(PrimOp::Sub, vec![var("n"), int(1)])),
                    app(global("fib"), prim(PrimOp::Sub, vec![var("n"), int(2)])),
                ],
            ),
        ),
    );
    p.defs.insert("fib".into(), Rc::new(fib));
    p
}

/// A small seeded LCG so the generator is deterministic without a rand dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn gen_value(rng: &mut Lcg, depth: u32) -> Value {
    let choice = if depth == 0 { rng.below(5) } else { rng.below(8) };
    match choice {
        0 => Value::Unit,
        1 => Value::Int(rng.next() as i64 - (u32::MAX as i64)),
        2 => Value::Bool(rng.below(2) == 0),
        3 => Value::Str(format!("s{}", rng.below(1000))),
        4 => Value::Witness(format!("w{}", rng.below(1000))),
        5 => {
            let n = rng.below(4);
            Value::Ctor {
                name: format!("C{}", rng.below(5)),
                args: (0..n).map(|_| gen_value(rng, depth - 1)).collect(),
            }
        }
        6 => {
            let mut env = Env::empty();
            for i in 0..rng.below(4) {
                env = env.bind(format!("v{i}"), gen_value(rng, depth - 1));
            }
            Value::Closure {
                param: "x".into(),
                body: Rc::new(prim(PrimOp::Add, vec![var("x"), int(rng.below(100) as i64)])),
                env,
            }
        }
        _ => {
            // Shadowed names must survive the round trip in order.
            let env = Env::empty()
                .bind("a", gen_value(rng, depth - 1))
                .bind("a", gen_value(rng, depth - 1));
            Value::Closure { param: "y".into(), body: Rc::new(var("a")), env }
        }
    }
}

#[test]
fn generated_values_round_trip() {
    let p = program();
    let auth = MapAuthority::default();
    let codec = Codec::new(&p, sigs0(), &auth);
    let granter = MapGranter::default();
    let mut rng = Lcg(0xEBEE_2026);
    for _ in 0..500 {
        let v = gen_value(&mut rng, 3);
        let bytes = codec.encode_value(&v).expect("encode is total over the language domain");
        let back = codec.decode_value(&bytes, &granter).expect("decode");
        assert_eq!(v, back);
        // Determinism: same value, same bytes.
        assert_eq!(bytes, codec.encode_value(&v).unwrap());
    }
}

fn suspend_demo(p: &Program, auth: &MapAuthority) -> entoli_core::Suspension {
    // let a = fib(10) in let f = λy. y+a in let pair = Pair(a,"tag") in
    // let b = Mark.here(a) in match pair { Pair(x,_) -> f(b) + x }
    let t = let_(
        "a",
        app(global("fib"), int(10)),
        let_(
            "f",
            lam("y", prim(PrimOp::Add, vec![var("y"), var("a")])),
            let_(
                "pair",
                ctor("Pair", vec![var("a"), str_("tag")]),
                let_(
                    "b",
                    do_("Mark", "here", var("a")),
                    match_(
                        var("pair"),
                        vec![arm_ctor(
                            "Pair",
                            &["x", "ignored"],
                            prim(PrimOp::Add, vec![app(var("f"), var("b")), var("x")]),
                        )],
                    ),
                ),
            ),
        ),
    );
    let mut m = Machine::new(p, auth, Rc::new(t));
    match m.run_fuel(1_000_000).unwrap() {
        Outcome::Suspended(s) => s,
        other => panic!("expected suspension, got {other:?}"),
    }
}

#[test]
fn continuation_round_trips_and_resumes_to_the_same_result() {
    let p = program();
    let auth = MapAuthority::default();
    let codec = Codec::new(&p, sigs0(), &auth);
    let granter = MapGranter::default();

    let susp = suspend_demo(&p, &auth);
    assert_eq!(susp.arg, Value::Int(55));

    let bytes = codec.encode_kont(&susp.frames).unwrap();
    assert_eq!(bytes, codec.encode_kont(&susp.frames).unwrap(), "deterministic");

    let frames = codec.decode_kont(&bytes, &granter).unwrap();
    let mut m = Machine::resume_frames(&p, &auth, frames, Value::Int(100), "0".into());
    match m.run_fuel(1_000_000).unwrap() {
        // f(100) + a = (100 + 55) + 55 = 210
        Outcome::Value(Value::Int(i)) => assert_eq!(i, 210),
        other => panic!("expected value, got {other:?}"),
    }
}

/// Verification is dependency-precise (§10.2 rejects pin-everything): a continuation
/// that no longer references a definition is unaffected by changes to it. These tests
/// therefore suspend at a point whose *remaining* computation still references the
/// definition / performs the effect.
fn suspend_with_live_refs(p: &Program, auth: &MapAuthority) -> entoli_core::Suspension {
    // let a = fib(3) in let b = Mark.here(a) in let c = Mark.here(b) in fib(c)
    let t = let_(
        "a",
        app(global("fib"), int(3)),
        let_(
            "b",
            do_("Mark", "here", var("a")),
            let_("c", do_("Mark", "here", var("b")), app(global("fib"), var("c"))),
        ),
    );
    let mut m = Machine::new(p, auth, Rc::new(t));
    match m.run_fuel(1_000_000).unwrap() {
        Outcome::Suspended(s) => s,
        other => panic!("expected suspension, got {other:?}"),
    }
}

#[test]
fn def_drift_is_detected_at_decode() {
    let p = program();
    let auth = MapAuthority::default();
    let susp = suspend_with_live_refs(&p, &auth);
    let bytes = Codec::new(&p, sigs0(), &auth).encode_kont(&susp.frames).unwrap();

    // Same name, different body: the sleeping continuation must not resume against it.
    let mut p2 = p.clone();
    p2.defs.insert("fib".into(), Rc::new(lam("n", int(999))));
    let granter = MapGranter::default();
    match Codec::new(&p2, sigs0(), &auth).decode_kont(&bytes, &granter) {
        Err(DecodeError::DefDrift { name }) => assert_eq!(name, "fib"),
        other => panic!("expected DefDrift, got {other:?}"),
    }
}

#[test]
fn unreferenced_def_change_does_not_strand_the_continuation() {
    // The dependency-precision counterpart: `suspend_demo`'s continuation no longer
    // mentions fib, so changing fib must NOT strand it (§10.2, "pin the implementation
    // hash" is rejected precisely because it strands sleeping continuations).
    let p = program();
    let auth = MapAuthority::default();
    let susp = suspend_demo(&p, &auth);
    let bytes = Codec::new(&p, sigs0(), &auth).encode_kont(&susp.frames).unwrap();

    let mut p2 = p.clone();
    p2.defs.insert("fib".into(), Rc::new(lam("n", int(999))));
    let granter = MapGranter::default();
    let frames = Codec::new(&p2, sigs0(), &auth).decode_kont(&bytes, &granter).unwrap();
    let mut m = Machine::resume_frames(&p2, &auth, frames, Value::Int(100), "0".into());
    match m.run_fuel(1_000_000).unwrap() {
        Outcome::Value(Value::Int(i)) => assert_eq!(i, 210),
        other => panic!("expected value, got {other:?}"),
    }
}

#[test]
fn effect_signature_drift_is_detected_at_decode() {
    let p = program();
    let auth = MapAuthority::default();
    let susp = suspend_with_live_refs(&p, &auth);
    let bytes = Codec::new(&p, sigs0(), &auth).encode_kont(&susp.frames).unwrap();

    // Promote Mark.here from idempotent to at-least-once: a breaking change (§10.1).
    let mut p2 = p.clone();
    p2.effects.get_mut("Mark").unwrap().ops[0].delivery = DeliveryClass::AtLeastOnce;
    let granter = MapGranter::default();
    match Codec::new(&p2, sigs0(), &auth).decode_kont(&bytes, &granter) {
        Err(DecodeError::EffectDrift { effect }) => assert_eq!(effect, "Mark"),
        other => panic!("expected EffectDrift, got {other:?}"),
    }
}

#[test]
fn effect_type_only_drift_is_detected_at_decode() {
    // F-008: sig(h) covers (τ1, τ2, δ) per §10.1. Pre-fix the hash omitted the
    // types, so a deployment changing only an operation's argument or result
    // type resumed a stale continuation with no E012 — the merchant-class drift
    // §10.2 exists to catch. Same declaration, same delivery class; only the
    // declared result type changes between persist and resume.
    let p = program();
    let auth = MapAuthority::default();
    let susp = suspend_with_live_refs(&p, &auth);
    let mut sigs_old = entoli_types::Signatures::default();
    sigs_old.declare_op("Mark", "here", entoli_types::Type::Int, entoli_types::Type::Int);
    let bytes = Codec::new(&p, &sigs_old, &auth).encode_kont(&susp.frames).unwrap();

    let mut sigs_new = entoli_types::Signatures::default();
    sigs_new.declare_op("Mark", "here", entoli_types::Type::Int, entoli_types::Type::Str);
    let granter = MapGranter::default();
    match Codec::new(&p, &sigs_new, &auth).decode_kont(&bytes, &granter) {
        Err(DecodeError::EffectDrift { effect }) => assert_eq!(effect, "Mark"),
        other => panic!("expected EffectDrift on type-only change, got {other:?}"),
    }
    // And the negative direction: identical types resume normally.
    let ok = Codec::new(&p, &sigs_old, &auth).decode_kont(&bytes, &granter);
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn capability_encodes_as_effect_slot_and_regrants_or_refuses() {
    let p = program();
    let mut auth = MapAuthority::default();
    auth.grant(CapId(7), "Ask");
    let codec = Codec::new(&p, sigs0(), &auth);

    // A continuation whose environment holds a live capability.
    let t = let_(
        "held",
        var("c"),
        let_("b", do_("Mark", "here", int(1)), var("held")),
    );
    let mut m = Machine::new(&p, &auth, Rc::new(t))
        .with_env(Env::empty().bind("c", Value::Cap(CapId(7))));
    let susp = match m.run_fuel(1_000_000).unwrap() {
        Outcome::Suspended(s) => s,
        other => panic!("expected suspension, got {other:?}"),
    };
    let bytes = codec.encode_kont(&susp.frames).unwrap();

    // The raw CapId (7) must not appear as authority: with a fresh grant table the slot
    // rebinds to the *current* capability for Ask.
    let mut granter = MapGranter::default();
    granter.grant("Ask", CapId(42));
    let frames = codec.decode_kont(&bytes, &granter).unwrap();
    let mut m2 = Machine::resume_frames(&p, &auth, frames, Value::Int(0), "0".into());
    match m2.run_fuel(1_000_000).unwrap() {
        Outcome::Value(Value::Cap(id)) => assert_eq!(id, CapId(42), "authority is resume-time authority"),
        other => panic!("expected re-granted capability, got {other:?}"),
    }

    // Revoked (no grant in force): refusal naming the effect — E014.
    let empty = MapGranter::default();
    match codec.decode_kont(&bytes, &empty) {
        Err(DecodeError::MissingResumeGrant { effect }) => assert_eq!(effect, "Ask"),
        other => panic!("expected MissingResumeGrant, got {other:?}"),
    }
}

#[test]
fn consumed_continuation_encodes_as_tombstone_and_stays_dead() {
    // Review finding R-5: encode must stay total over reachable states (Rule 2). A
    // consumed continuation in a captured environment persists as a tombstone whose
    // only future is E003.
    let p = program();
    let auth = MapAuthority::default();
    let codec = Codec::new(&p, sigs0(), &auth);
    let kont = Value::Kont(Rc::new(std::cell::RefCell::new(entoli_core::KontCell {
        id: "k0".into(),
        consumed: true,
        frames: vec![],
    })));
    let bytes = codec.encode_value(&kont).expect("encode is total");
    let back = codec.decode_value(&bytes, &MapGranter::default()).unwrap();
    match &back {
        Value::Kont(k) => {
            assert!(k.borrow().consumed, "tombstone stays consumed");
            assert_eq!(k.borrow().id, "k0");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn aliased_continuation_survives_the_boundary_as_one_cell() {
    // Review finding R-4: without a sharing table, one aliased continuation becomes
    // two independently-resumable copies after decode — affinity per copy instead of
    // per continuation. Encode a value holding the SAME kont twice; decode must yield
    // one cell reachable twice, so consuming one alias consumes the other.
    let p = program();
    let auth = MapAuthority::default();
    let codec = Codec::new(&p, sigs0(), &auth);
    let shared = Rc::new(std::cell::RefCell::new(entoli_core::KontCell {
        id: "k7".into(),
        consumed: false,
        frames: vec![],
    }));
    let v = Value::Ctor {
        name: "Pair".into(),
        args: vec![Value::Kont(shared.clone()), Value::Kont(shared)],
    };
    let bytes = codec.encode_value(&v).unwrap();
    let back = codec.decode_value(&bytes, &MapGranter::default()).unwrap();
    let (a, b) = match &back {
        Value::Ctor { args, .. } => match (&args[0], &args[1]) {
            (Value::Kont(a), Value::Kont(b)) => (a.clone(), b.clone()),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    assert!(Rc::ptr_eq(&a, &b), "aliases decode to one cell");
    a.borrow_mut().consumed = true;
    assert!(b.borrow().consumed, "consuming one alias consumes the other");
}
