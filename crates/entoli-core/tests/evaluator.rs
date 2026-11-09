//! Phase 1 exit conditions (plan §4): a handler intercepts an operation, inspects its
//! argument, and tail-resumes exactly once; double resume is E003. Plus the rest of the
//! dynamic proxies: E001, E002, E004, E005, witness enforcement, abort, deep handlers.

use entoli_core::{CapId, Machine, MapAuthority, Outcome, Value};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, PrimOp, Program, SuspensionClass, Term,
};
use std::rc::Rc;

const FUEL: u64 = 1_000_000;

fn program() -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Ask".into(),
        EffectDecl {
            name: "Ask".into(),
            class: SuspensionClass::Prompt,
            ops: vec![OpDecl { name: "ask".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.effects.insert(
        "S".into(),
        EffectDecl {
            name: "S".into(),
            class: SuspensionClass::Prompt,
            ops: vec![
                OpDecl { name: "grab".into(), delivery: DeliveryClass::Idempotent },
                OpDecl { name: "stop".into(), delivery: DeliveryClass::Idempotent },
            ],
        },
    );
    p.effects.insert(
        "Sus".into(),
        EffectDecl {
            name: "Sus".into(),
            class: SuspensionClass::Suspending,
            ops: vec![
                OpDecl { name: "ext".into(), delivery: DeliveryClass::Idempotent },
                OpDecl { name: "post".into(), delivery: DeliveryClass::AtLeastOnce },
            ],
        },
    );
    p
}

fn authority() -> MapAuthority {
    let mut a = MapAuthority::default();
    a.grant(CapId(1), "Ask");
    a.grant(CapId(2), "S");
    a.grant(CapId(9), "Other");
    a
}

fn ask_handler(body: Term) -> Handler {
    Handler {
        effect: "Ask".into(),
        return_clause: None,
        clauses: vec![clause("ask", "x", "k", body)],
    }
}

fn run(p: &Program, a: &MapAuthority, t: Term, caps: &[(&str, CapId)]) -> Result<Outcome, entoli_core::EntoliError> {
    let mut env = entoli_core::Env::empty();
    for (n, c) in caps {
        env = env.bind(*n, Value::Cap(*c));
    }
    let mut m = Machine::new(p, a, Rc::new(t)).with_env(env);
    m.run_fuel(FUEL)
}

fn expect_int(out: Outcome, want: i64) {
    match out {
        Outcome::Value(Value::Int(i)) => assert_eq!(i, want),
        other => panic!("expected Int({want}), got {other:?}"),
    }
}

#[test]
fn handler_intercepts_inspects_and_tail_resumes() {
    // 1 + ask(10) + ask(20), handler doubles the argument: 1 + 20 + 40 = 61.
    // Exercises interception, argument inspection, deep re-handling after resume.
    let p = program();
    let a = authority();
    let body = prim(
        PrimOp::Add,
        vec![
            int(1),
            prim(PrimOp::Add, vec![do_("Ask", "ask", int(10)), do_("Ask", "ask", int(20))]),
        ],
    );
    let h = ask_handler(resume(var("k"), prim(PrimOp::Mul, vec![var("x"), int(2)])));
    let t = handle(body, h, var("cap"));
    expect_int(run(&p, &a, t, &[("cap", CapId(1))]).unwrap(), 61);
}

#[test]
fn return_clause_runs_after_resumed_body() {
    let p = program();
    let a = authority();
    let h = Handler {
        effect: "Ask".into(),
        return_clause: Some(("r".into(), Rc::new(prim(PrimOp::Add, vec![var("r"), int(1)])))),
        clauses: vec![clause("ask", "x", "k", resume(var("k"), var("x")))],
    };
    let t = handle(do_("Ask", "ask", int(5)), h, var("cap"));
    expect_int(run(&p, &a, t, &[("cap", CapId(1))]).unwrap(), 6);
}

#[test]
fn clause_without_resume_aborts_the_body() {
    // handle (stop() + 100) with { stop(_;k) -> 7 }  ==>  7; the continuation is dropped.
    let p = program();
    let a = authority();
    let h = Handler {
        effect: "S".into(),
        return_clause: None,
        clauses: vec![clause("stop", "x", "k", int(7))],
    };
    let body = prim(PrimOp::Add, vec![do_("S", "stop", unit()), int(100)]);
    let t = handle(body, h, var("cap"));
    expect_int(run(&p, &a, t, &[("cap", CapId(2))]).unwrap(), 7);
}

#[test]
fn first_class_continuation_single_resume_ok_double_is_e003() {
    let p = program();
    let a = authority();
    // grab's clause returns k itself (abort with the continuation as the result).
    let h = Handler {
        effect: "S".into(),
        return_clause: None,
        clauses: vec![clause("grab", "x", "k", var("k"))],
    };
    let grab = handle(do_("S", "grab", unit()), h, var("cap"));

    // Single top-level resume: fine.
    let t1 = let_("kk", grab.clone(), let_("aa", resume(var("kk"), int(5)), prim(PrimOp::Add, vec![var("aa"), int(1)])));
    expect_int(run(&p, &a, t1, &[("cap", CapId(2))]).unwrap(), 6);

    // Second resume of the same continuation: E003.
    let t2 = let_(
        "kk",
        grab,
        let_(
            "aa",
            resume(var("kk"), int(5)),
            let_("bb", resume(var("kk"), int(6)), prim(PrimOp::Add, vec![var("aa"), var("bb")])),
        ),
    );
    match run(&p, &a, t2, &[("cap", CapId(2))]) {
        Err(entoli_core::EntoliError::DoubleResume { .. }) => {}
        other => panic!("expected E003 DoubleResume, got {other:?}"),
    }
}

#[test]
fn non_tail_resume_in_clause_is_e004() {
    let p = program();
    let a = authority();
    let h = ask_handler(let_(
        "aa",
        resume(var("k"), int(1)),
        prim(PrimOp::Add, vec![var("aa"), int(1)]),
    ));
    let t = handle(do_("Ask", "ask", unit()), h, var("cap"));
    match run(&p, &a, t, &[("cap", CapId(1))]) {
        Err(entoli_core::EntoliError::NonTailResume { .. }) => {}
        other => panic!("expected E004 NonTailResume, got {other:?}"),
    }
}

#[test]
fn suspension_while_servicing_is_e005() {
    let p = program();
    let a = authority();
    let h = ask_handler(let_("ig", do_("Sus", "ext", unit()), resume(var("k"), int(1))));
    let t = handle(do_("Ask", "ask", unit()), h, var("cap"));
    match run(&p, &a, t, &[("cap", CapId(1))]) {
        Err(entoli_core::EntoliError::NestedSuspension { .. }) => {}
        other => panic!("expected E005 NestedSuspension, got {other:?}"),
    }
}

#[test]
fn unhandled_prompt_effect_is_e001() {
    let p = program();
    let a = authority();
    match run(&p, &a, do_("Ask", "ask", int(1)), &[]) {
        Err(entoli_core::EntoliError::UnhandledEffect { .. }) => {}
        other => panic!("expected E001 UnhandledEffect, got {other:?}"),
    }
}

#[test]
fn handler_install_with_wrong_capability_is_e002() {
    let p = program();
    let a = authority();
    let t = handle(do_("Ask", "ask", int(1)), ask_handler(resume(var("k"), var("x"))), var("cap"));
    match run(&p, &a, t, &[("cap", CapId(9))]) {
        Err(entoli_core::EntoliError::MissingCapability { .. }) => {}
        other => panic!("expected E002 MissingCapability, got {other:?}"),
    }
}

#[test]
fn suspending_effect_suspends_whole_stack_and_resumes_in_process() {
    let p = program();
    let a = authority();
    let t = let_("x", do_("Sus", "ext", int(5)), prim(PrimOp::Add, vec![var("x"), int(1)]));
    let mut m = Machine::new(&p, &a, Rc::new(t));
    let out = m.run_fuel(FUEL).unwrap();
    let susp = match out {
        Outcome::Suspended(s) => s,
        other => panic!("expected suspension, got {other:?}"),
    };
    assert_eq!(susp.effect, "Sus");
    assert_eq!(susp.op, "ext");
    assert_eq!(susp.arg, Value::Int(5));
    assert!(susp.witness.is_none());

    // Step 5 in normal operation: resume the moved-out frames with the result.
    let mut m2 = Machine::resume_frames(&p, &a, susp.frames, Value::Int(41), "0".into());
    expect_int(m2.run_fuel(FUEL).unwrap(), 42);
}

#[test]
fn suspension_by_the_resumed_computation_is_permitted() {
    // §4.3's permitted half (review finding R-9): a suspension raised by the RESUMED
    // computation is fine — by then the handler has finished servicing and the clause
    // boundary is gone. Only suspension WHILE servicing is E005.
    let p = program();
    let a = authority();
    // handle (let x = ask(1) in Sus.ext(x)) with { ask(x;k) -> resume k x }
    let body = let_("x", do_("Ask", "ask", int(1)), do_("Sus", "ext", var("x")));
    let t = handle(body, ask_handler(resume(var("k"), var("x"))), var("cap"));
    let mut m = Machine::new(&p, &a, Rc::new(t))
        .with_env(entoli_core::Env::empty().bind("cap", Value::Cap(CapId(1))));
    match m.run_fuel(FUEL).unwrap() {
        Outcome::Suspended(s) => {
            assert_eq!(s.effect, "Sus");
            assert_eq!(s.arg, Value::Int(1));
        }
        other => panic!("expected the resumed computation's suspension, got {other:?}"),
    }
}

#[test]
fn at_least_once_without_witness_is_refused() {
    let p = program();
    let a = authority();
    match run(&p, &a, do_("Sus", "post", int(1)), &[]) {
        Err(entoli_core::EntoliError::MissingWitness { .. }) => {}
        other => panic!("expected MissingWitness, got {other:?}"),
    }
}

#[test]
fn at_least_once_with_witness_carries_it_into_the_suspension() {
    let p = program();
    let a = authority();
    let t = do_w("Sus", "post", int(1), str_("inv-42"));
    let mut m = Machine::new(&p, &a, Rc::new(t));
    match m.run_fuel(FUEL).unwrap() {
        Outcome::Suspended(s) => assert_eq!(s.witness.as_deref(), Some("inv-42")),
        other => panic!("expected suspension, got {other:?}"),
    }
}

#[test]
fn clause_sees_handler_installation_environment() {
    let p = program();
    let a = authority();
    let h = ask_handler(resume(var("k"), prim(PrimOp::Add, vec![var("x"), var("base")])));
    let t = let_("base", int(100), handle(do_("Ask", "ask", int(1)), h, var("cap")));
    expect_int(run(&p, &a, t, &[("cap", CapId(1))]).unwrap(), 101);
}

#[test]
fn compensation_may_not_be_compensable_is_e011_at_load() {
    let mut p = Program::default();
    p.effects.insert(
        "L".into(),
        EffectDecl {
            name: "L".into(),
            class: SuspensionClass::Suspending,
            ops: vec![
                OpDecl {
                    name: "a".into(),
                    delivery: DeliveryClass::Compensable { comp_effect: "L".into(), comp_op: "b".into() },
                },
                OpDecl {
                    name: "b".into(),
                    delivery: DeliveryClass::Compensable { comp_effect: "L".into(), comp_op: "c".into() },
                },
                OpDecl { name: "c".into(), delivery: DeliveryClass::Idempotent },
            ],
        },
    );
    let err = p.validate().unwrap_err();
    assert!(err.contains("E011"), "expected E011, got: {err}");
}

#[test]
fn recursive_global_fib() {
    let mut p = program();
    // fib(n) = if n < 2 then n else fib(n-1) + fib(n-2)
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
    let a = authority();
    expect_int(run(&p, &a, app(global("fib"), int(10)), &[]).unwrap(), 55);
}
