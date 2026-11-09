//! Migration M6: the checker under the unification design — suspending effects are
//! handled effects with the tightened T-Clause (empty own row, all-paths-resume,
//! foreign calls only there), and demonstration B's containment now covers
//! suspending tools.

use entoli_check::{check, Checker};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, Program, SuspensionClass,
};
use entoli_types::{Row, Signatures, Type};
use std::rc::Rc;

fn program() -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Ledger".into(),
        EffectDecl {
            name: "Ledger".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "post".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.effects.insert(
        "Log".into(),
        EffectDecl {
            name: "Log".into(),
            class: SuspensionClass::Prompt,
            ops: vec![OpDecl { name: "info".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.effects.insert(
        "Http".into(),
        EffectDecl {
            name: "Http".into(),
            class: SuspensionClass::Prompt,
            ops: vec![OpDecl { name: "get".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.foreigns.insert("ledger_svc".into());
    p
}

fn sigs() -> Signatures {
    let mut s = Signatures::default();
    s.declare_op("Ledger", "post", Type::Data, Type::Data);
    s.declare_op("Log", "info", Type::Str, Type::Unit);
    s.declare_op("Http", "get", Type::Data, Type::Data);
    s.foreigns.insert("ledger_svc".into(), (Type::Data, Type::Data));
    s
}

fn ledger_handler(body: entoli_syntax::Term) -> Handler {
    Handler {
        effect: "Ledger".into(),
        return_clause: None,
        clauses: vec![clause("post", "x", "k", body)],
    }
}

fn codes(diags: &[entoli_check::Diagnostic]) -> Vec<&'static str> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn suspending_handler_with_tail_foreign_clause_checks_clean() {
    let p = program();
    let s = sigs();
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(resume(var("k"), foreign_app("ledger_svc", var("x")))),
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn suspending_clause_aborting_on_one_path_violates_all_paths_resume() {
    // D-1: `if ok then resume k r else Aborted()` — the window would have a third exit.
    let p = program();
    let s = sigs();
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(if_(
            boolean(true),
            resume(var("k"), foreign_app("ledger_svc", var("x"))),
            resume(var("k"), var("x")),
        )),
        var("cap"),
    );
    // Both branches resume: clean.
    assert!(check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]).is_empty());

    let t2 = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(if_(
            boolean(true),
            resume(var("k"), foreign_app("ledger_svc", var("x"))),
            ctor("Aborted", vec![]),
        )),
        var("cap"),
    );
    let diags = check(&p, &s, &t2, &[("cap".into(), Type::Cap("Ledger".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("all-paths-resume")), "{msgs:?}");
}

#[test]
fn suspending_clause_performing_an_effect_in_the_window_is_rejected() {
    let p = program();
    let s = sigs();
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(let_(
            "ig",
            do_("Log", "info", str_("posting")),
            resume(var("k"), foreign_app("ledger_svc", var("x"))),
        )),
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E005") && m.contains("crash window")),
        "{msgs:?}"
    );
}

#[test]
fn foreign_application_outside_a_suspending_clause_is_rejected() {
    let p = program();
    let s = sigs();
    // At top level.
    let diags = check(&p, &s, &foreign_app("ledger_svc", ctor("Entry", vec![])), &[]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("outside a suspending handler clause")), "{msgs:?}");

    // Inside a PROMPT clause: also rejected.
    let t = handle(
        do_("Http", "get", ctor("Q", vec![])),
        Handler {
            effect: "Http".into(),
            return_clause: None,
            clauses: vec![clause(
                "get",
                "x",
                "k",
                resume(var("k"), foreign_app("ledger_svc", var("x"))),
            )],
        },
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Http".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("outside a suspending handler clause")), "{msgs:?}");
}

#[test]
fn demo_b_containment_now_covers_suspending_tools() {
    // research performs prompt Http AND suspending Ledger; delegate installs only the
    // Http handler and claims everything is discharged. Under the unification the
    // suspending tool no longer escapes through a registry: E001 names Ledger and
    // E002 names the missing Cap[Ledger] — escalation beyond the delegated envelope
    // is impossible for the WHOLE language (unification §7.1).
    let mut p = program();
    p.defs.insert(
        "research".into(),
        Rc::new(lam(
            "task",
            ctor(
                "Pair",
                vec![
                    do_("Http", "get", var("task")),
                    do_("Ledger", "post", var("task")),
                ],
            ),
        )),
    );
    let mut s = sigs();
    s.declare_def(
        "research",
        Type::Fn(
            Box::new(Type::Data),
            Row(["Http".to_string(), "Ledger".to_string()].into()),
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
    let mut c = Checker::new(&p, &s);
    c.check_defs();
    c.check_entry(
        &delegate_body,
        &[("task".into(), Type::Data), ("net".into(), Type::Cap("Http".into()))],
        &Row::empty(),
    );
    let msgs: Vec<String> = c.diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E001") && m.contains("Ledger")),
        "suspending tool named: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.starts_with("E002") && m.contains("Cap[Ledger]")),
        "missing capability named: {msgs:?}"
    );
    assert_eq!(codes(&c.diags).iter().filter(|c| **c == "E001").count(), 1, "{msgs:?}");
}

#[test]
fn the_unified_settle_program_checks_clean_with_empty_required_row() {
    // The whole-program claim (unification §4): with the prelude installed, settle
    // reaches row {} — no registry escape hatch in the required row.
    let (p, entry) = entoli_testkit_settle_unified();
    let mut s = Signatures::default();
    s.declare_op("Approval", "request", Type::Data, Type::Data);
    s.declare_op("Ledger", "post", Type::Data, Type::Data);
    s.foreigns.insert("approval_svc".into(), (Type::Data, Type::Data));
    s.foreigns.insert("ledger_svc".into(), (Type::Data, Type::Data));
    s.declare_def("audit", Type::Fn(Box::new(Type::Str), Row::empty(), Box::new(Type::Data)));
    s.declare_def(
        "settle",
        Type::Fn(
            Box::new(Type::Str),
            Row(["Approval".to_string(), "Ledger".to_string()].into()),
            Box::new(Type::Data),
        ),
    );
    let mut c = Checker::new(&p, &s);
    c.check_defs();
    c.check_entry(
        &entry,
        &[
            ("cap_approval".into(), Type::Cap("Approval".into())),
            ("cap_ledger".into(), Type::Cap("Ledger".into())),
        ],
        &Row::empty(),
    );
    assert!(c.diags.is_empty(), "{:?}", c.diags);
}

// Local copy (entoli-check cannot depend on entoli-testkit).
fn entoli_testkit_settle_unified() -> (Program, entoli_syntax::Term) {
    use entoli_syntax::PrimOp;
    let mut p = Program::default();
    for (name, op) in [("Approval", "request"), ("Ledger", "post")] {
        p.effects.insert(
            name.into(),
            EffectDecl {
                name: name.into(),
                class: SuspensionClass::Suspending,
                ops: vec![OpDecl { name: op.into(), delivery: DeliveryClass::AtLeastOnce }],
            },
        );
    }
    p.foreigns.insert("approval_svc".into());
    p.foreigns.insert("ledger_svc".into());
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
    let inner = handle(
        app(global("settle"), str_("INV-7")),
        Handler {
            effect: "Approval".into(),
            return_clause: None,
            clauses: vec![clause(
                "request",
                "x",
                "k",
                resume(var("k"), foreign_app("approval_svc", var("x"))),
            )],
        },
        var("cap_approval"),
    );
    let entry = handle(
        inner,
        Handler {
            effect: "Ledger".into(),
            return_clause: None,
            clauses: vec![clause(
                "post",
                "x",
                "k",
                resume(var("k"), foreign_app("ledger_svc", var("x"))),
            )],
        },
        var("cap_ledger"),
    );
    (p, entry)
}

// -- P-series regressions (paper v0.6 review, 2026-08-28) ---------------------

#[test]
fn suspending_clause_reperforming_its_own_effect_is_rejected() {
    // P-1: the pre-fix condition subtracted the continuation's row from the
    // clause body's row, so any effect also occurring in εk — E itself first
    // among them — escaped the empty-own-row guard and could suspend inside
    // the crash window. The tail resume is now typed as a jump and the body's
    // own row must be empty.
    let p = program();
    let s = sigs();
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(let_(
            "again",
            do_("Ledger", "post", var("x")),
            resume(var("k"), foreign_app("ledger_svc", var("again"))),
        )),
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E005") && m.contains("crash window")),
        "{msgs:?}"
    );
}

#[test]
fn prompt_clause_suspending_via_an_ambient_effect_is_rejected() {
    // P-1's prompt half: the handled body also performs Ledger, so Ledger sat
    // in εk and the old subtraction erased it from the clause's own row — a
    // prompt clause could suspend while servicing, breaking I1, whenever the
    // ambient row happened to mention the same suspending effect.
    let p = program();
    let s = sigs();
    let body = let_(
        "ig",
        do_("Log", "info", str_("hi")),
        do_("Ledger", "post", ctor("Entry", vec![])),
    );
    let log_handler = Handler {
        effect: "Log".into(),
        return_clause: None,
        clauses: vec![clause(
            "info",
            "x",
            "k",
            let_(
                "posted",
                do_("Ledger", "post", ctor("Entry", vec![])),
                resume(var("k"), unit()),
            ),
        )],
    };
    let t = handle(body, log_handler, var("logcap"));
    let diags = check(&p, &s, &t, &[("logcap".into(), Type::Cap("Log".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E005") && m.contains("suspends while servicing")),
        "{msgs:?}"
    );
}

#[test]
fn foreign_application_deferred_under_a_lambda_is_rejected() {
    // P-2: resume k (λy. f y) placed the foreign application syntactically
    // inside the clause while deferring its execution past the crash window.
    // A lambda body is not clause execution.
    let p = program();
    let s = sigs();
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(resume(var("k"), lam("y", foreign_app("ledger_svc", var("y"))))),
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.contains("outside a suspending handler clause")),
        "{msgs:?}"
    );
}

#[test]
fn foreign_without_signature_is_a_diagnostic_not_a_silent_default() {
    // A-31: builder-constructed programs can declare a foreign name without
    // registering its signature; that must be an error, not Data -> Data.
    let mut p = program();
    p.foreigns.insert("mystery_svc".into());
    let s = sigs(); // no entry for mystery_svc
    let t = handle(
        do_("Ledger", "post", ctor("Entry", vec![])),
        ledger_handler(resume(var("k"), foreign_app("mystery_svc", var("x")))),
        var("cap"),
    );
    let diags = check(&p, &s, &t, &[("cap".into(), Type::Cap("Ledger".into()))]);
    let msgs: Vec<String> = diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("no declared signature")), "{msgs:?}");
}
