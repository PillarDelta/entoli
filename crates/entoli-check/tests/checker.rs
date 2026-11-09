//! Phase 5 exit condition (plan §4): demonstration B fails at compile time for the
//! exact effect and capability reasons, with both named — plus the static forms of
//! E003/E004/E005 and the at-least-once witness obligation.

use entoli_check::{check, Checker};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, Program, PrimOp, SuspensionClass,
};
use entoli_types::{Row, Signatures, Type};
use std::rc::Rc;

fn base_program() -> Program {
    let mut p = Program::default();
    for (name, class) in [
        ("Http", SuspensionClass::Prompt),
        ("Fs", SuspensionClass::Prompt),
        ("S", SuspensionClass::Prompt),
        ("Ask", SuspensionClass::Prompt),
    ] {
        let ops = match name {
            "Http" => vec![OpDecl { name: "get".into(), delivery: DeliveryClass::Idempotent }],
            "Fs" => vec![OpDecl { name: "read".into(), delivery: DeliveryClass::Idempotent }],
            "S" => vec![OpDecl { name: "grab".into(), delivery: DeliveryClass::Idempotent }],
            _ => vec![OpDecl { name: "ask".into(), delivery: DeliveryClass::Idempotent }],
        };
        p.effects.insert(name.into(), EffectDecl { name: name.into(), class, ops });
    }
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

fn codes(diags: &[entoli_check::Diagnostic]) -> Vec<&'static str> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn demo_b_containment_is_a_compile_error_naming_both_causes() {
    // research performs Http AND Fs; delegate handles only Http and claims the rest is
    // empty. §13.2: Fs remains in the row, and no Cap[Fs, _] is in scope to handle it.
    let mut p = base_program();
    p.defs.insert(
        "research".into(),
        Rc::new(lam(
            "task",
            ctor("Pair", vec![do_("Http", "get", var("task")), do_("Fs", "read", var("task"))]),
        )),
    );
    let mut sigs = Signatures::default();
    sigs.declare_op("Http", "get", Type::Data, Type::Data);
    sigs.declare_op("Fs", "read", Type::Data, Type::Data);
    sigs.declare_def(
        "research",
        Type::Fn(
            Box::new(Type::Data),
            Row(["Http".to_string(), "Fs".to_string()].into()),
            Box::new(Type::Data),
        ),
    );

    let delegate_body = handle(
        app(global("research"), var("task")),
        Handler {
            effect: "Http".into(),
            return_clause: None,
            clauses: vec![clause("get", "x", "k", resume(var("k"), var("x")))],
        },
        var("net"),
    );
    let diags = check(
        &p,
        &sigs,
        &delegate_body,
        &[("task".into(), Type::Data), ("net".into(), Type::Cap("Http".into()))],
    );
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E001") && m.contains("Fs")),
        "must name the escaping effect: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.starts_with("E002") && m.contains("Cap[Fs]")),
        "must name the missing capability: {msgs:?}"
    );
}

#[test]
fn demo_b_negative_handling_both_effects_is_clean() {
    let mut p = base_program();
    p.defs.insert(
        "research".into(),
        Rc::new(lam(
            "task",
            ctor("Pair", vec![do_("Http", "get", var("task")), do_("Fs", "read", var("task"))]),
        )),
    );
    let mut sigs = Signatures::default();
    sigs.declare_op("Http", "get", Type::Data, Type::Data);
    sigs.declare_op("Fs", "read", Type::Data, Type::Data);
    sigs.declare_def(
        "research",
        Type::Fn(
            Box::new(Type::Data),
            Row(["Http".to_string(), "Fs".to_string()].into()),
            Box::new(Type::Data),
        ),
    );
    let both = handle(
        handle(
            app(global("research"), var("task")),
            Handler {
                effect: "Http".into(),
                return_clause: None,
                clauses: vec![clause("get", "x", "k", resume(var("k"), var("x")))],
            },
            var("net"),
        ),
        Handler {
            effect: "Fs".into(),
            return_clause: None,
            clauses: vec![clause("read", "x", "k", resume(var("k"), var("x")))],
        },
        var("fs"),
    );
    let diags = check(
        &p,
        &sigs,
        &both,
        &[
            ("task".into(), Type::Data),
            ("net".into(), Type::Cap("Http".into())),
            ("fs".into(), Type::Cap("Fs".into())),
        ],
    );
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn static_affinity_double_resume_is_e003() {
    let mut p = base_program();
    let kty = Type::Kont(Box::new(Type::Int), Row(["S".to_string()].into()), Box::new(Type::Data));
    // twice(kk) = let a = resume kk 1 in resume kk 2 — two uses of an affine binding.
    p.defs.insert(
        "twice".into(),
        Rc::new(lam("kk", let_("a", resume(var("kk"), int(1)), resume(var("kk"), int(2))))),
    );
    let mut sigs = Signatures::default();
    sigs.declare_op("S", "grab", Type::Unit, Type::Int);
    sigs.declare_def(
        "twice",
        Type::Fn(Box::new(kty), Row(["S".to_string()].into()), Box::new(Type::Data)),
    );
    let mut c = Checker::new(&p, &sigs);
    c.check_defs();
    assert!(codes(&c.diags).contains(&"E003"), "{:?}", c.diags);
}

#[test]
fn non_tail_resume_in_clause_is_e004_statically() {
    let p = base_program();
    let mut sigs = Signatures::default();
    sigs.declare_op("Ask", "ask", Type::Int, Type::Int);
    let t = handle(
        do_("Ask", "ask", int(1)),
        Handler {
            effect: "Ask".into(),
            return_clause: None,
            clauses: vec![clause(
                "ask",
                "x",
                "k",
                let_("a", resume(var("k"), int(1)), prim(PrimOp::Add, vec![var("a"), int(1)])),
            )],
        },
        var("cap"),
    );
    let diags = check(&p, &sigs, &t, &[("cap".into(), Type::Cap("Ask".into()))]);
    assert!(codes(&diags).contains(&"E004"), "{diags:?}");
}

#[test]
fn tail_resume_in_match_arms_is_accepted() {
    let p = base_program();
    let mut sigs = Signatures::default();
    sigs.declare_op("Ask", "ask", Type::Data, Type::Int);
    let t = handle(
        do_("Ask", "ask", ctor("q", vec![])),
        Handler {
            effect: "Ask".into(),
            return_clause: None,
            clauses: vec![clause(
                "ask",
                "x",
                "k",
                match_(
                    var("x"),
                    vec![
                        arm_ctor("a", &[], resume(var("k"), int(1))),
                        arm_any("other", resume(var("k"), int(2))),
                    ],
                ),
            )],
        },
        var("cap"),
    );
    let diags = check(&p, &sigs, &t, &[("cap".into(), Type::Cap("Ask".into()))]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn clause_suspending_while_servicing_is_e005_statically() {
    let p = base_program();
    let mut sigs = Signatures::default();
    sigs.declare_op("Ask", "ask", Type::Int, Type::Int);
    sigs.declare_op("Sus", "ext", Type::Unit, Type::Int);
    let t = handle(
        do_("Ask", "ask", int(1)),
        Handler {
            effect: "Ask".into(),
            return_clause: None,
            clauses: vec![clause(
                "ask",
                "x",
                "k",
                let_("ig", do_("Sus", "ext", unit()), resume(var("k"), int(1))),
            )],
        },
        var("cap"),
    );
    let diags = check(&p, &sigs, &t, &[("cap".into(), Type::Cap("Ask".into()))]);
    assert!(codes(&diags).contains(&"E005"), "{diags:?}");
}

#[test]
fn unwitnessed_at_least_once_cannot_reach_top_level() {
    let p = base_program();
    let mut sigs = Signatures::default();
    sigs.declare_op("Sus", "post", Type::Int, Type::Data);
    // Without a witness: the AtLeastOnce marker propagates to the top and is refused.
    let mut c = Checker::new(&p, &sigs);
    c.check_entry(&do_("Sus", "post", int(1)), &[], &Row(["Sus".to_string()].into()));
    let msgs: Vec<String> = c.diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("at-least-once")), "{msgs:?}");

    // With a witness: clean.
    let mut c2 = Checker::new(&p, &sigs);
    c2.check_entry(
        &do_w("Sus", "post", int(1), str_("idem-1")),
        &[],
        &Row(["Sus".to_string()].into()),
    );
    assert!(c2.diags.is_empty(), "{:?}", c2.diags);
}

#[test]
fn wrong_capability_at_install_is_e002_statically() {
    let p = base_program();
    let mut sigs = Signatures::default();
    sigs.declare_op("Ask", "ask", Type::Int, Type::Int);
    let t = handle(
        do_("Ask", "ask", int(1)),
        Handler {
            effect: "Ask".into(),
            return_clause: None,
            clauses: vec![clause("ask", "x", "k", resume(var("k"), var("x")))],
        },
        var("cap"),
    );
    let diags = check(&p, &sigs, &t, &[("cap".into(), Type::Cap("Http".into()))]);
    assert!(codes(&diags).contains(&"E002"), "{diags:?}");
}

#[test]
fn the_real_settle_program_checks_clean() {
    // Type the demonstration-A program itself: settle : Str -[Approval, Ledger]-> Data,
    // with suspending effects discharged by the runtime registry (the required row).
    let (mut p, entry) = entoli_testkit_programs_settle();
    let mut sigs = Signatures::default();
    sigs.declare_op("Approval", "request", Type::Data, Type::Data);
    sigs.declare_op("Ledger", "post", Type::Data, Type::Data);
    sigs.declare_def("audit", Type::Fn(Box::new(Type::Str), Row::empty(), Box::new(Type::Data)));
    sigs.declare_def(
        "settle",
        Type::Fn(
            Box::new(Type::Str),
            Row(["Approval".to_string(), "Ledger".to_string()].into()),
            Box::new(Type::Data),
        ),
    );
    let mut c = Checker::new(&p, &sigs);
    c.check_defs();
    c.check_entry(&entry, &[], &Row(["Approval".to_string(), "Ledger".to_string()].into()));
    assert!(c.diags.is_empty(), "{:?}", c.diags);
    p.defs.clear();
}

// Local copy of the settle builder (entoli-check must not depend on entoli-testkit —
// testkit sits above the runtime in the crate graph).
fn entoli_testkit_programs_settle() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Approval".into(),
        EffectDecl {
            name: "Approval".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "request".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    p.effects.insert(
        "Ledger".into(),
        EffectDecl {
            name: "Ledger".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "post".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    p.defs.insert("audit".into(), Rc::new(lam("inv", ctor("Audited", vec![var("inv"), int(250)]))));
    p.defs.insert(
        "settle".into(),
        Rc::new(lam(
            "inv",
            let_(
                "f",
                app(global("audit"), var("inv")),
                match_(
                    do_w(
                        "Approval",
                        "request",
                        var("f"),
                        prim(PrimOp::Concat, vec![str_("req:"), var("inv")]),
                    ),
                    vec![
                        arm_ctor(
                            "approve",
                            &[],
                            ctor(
                                "posted",
                                vec![do_w(
                                    "Ledger",
                                    "post",
                                    var("f"),
                                    prim(PrimOp::Concat, vec![str_("idem:"), var("inv")]),
                                )],
                            ),
                        ),
                        arm_ctor("reject", &[], ctor("halted", vec![var("f")])),
                    ],
                ),
            ),
        )),
    );
    let entry = app(global("settle"), str_("INV-7"));
    (p, entry)
}
