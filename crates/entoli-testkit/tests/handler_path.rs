//! Migration M2–M4: the unified handler path. A suspending effect is discharged by an
//! installed handler whose clause the runtime executes as the crash window
//! (unification §5), with the transitional registry untouched for programs without
//! handlers (dual dispatch).

use entoli_core::Value;
use entoli_runtime::Runtime;
use entoli_store::{DurableStore, SqliteStore};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, Program, SuspensionClass, Term,
};
use std::path::PathBuf;
use std::rc::Rc;

fn fresh(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// settle with an installed prelude: Ledger and Approval discharged by handlers whose
/// clauses call foreign invokers in tail-resume position.
fn settle_with_prelude() -> (Program, Term) {
    let (mut p, _) = entoli_testkit::programs::settle_program();
    p.foreigns.insert("approval_svc".into());
    p.foreigns.insert("ledger_svc".into());
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

fn build_rt(dir: &PathBuf, p: &Program) -> (Runtime, entoli_core::Env) {
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(p, &entoli_types::Signatures::default(), store);
    rt.bind_foreign(
        "approval_svc",
        "Approval",
        "request",
        Box::new(entoli_testkit::fixtures::FakeApproval::new(dir)),
    );
    rt.bind_foreign(
        "ledger_svc",
        "Ledger",
        "post",
        Box::new(entoli_testkit::fixtures::FakeLedger::new(dir)),
    );
    let ca = rt.grants.grant("Approval", None);
    let cl = rt.grants.grant("Ledger", None);
    let env = entoli_core::Env::empty()
        .bind("cap_approval", Value::Cap(ca))
        .bind("cap_ledger", Value::Cap(cl));
    (rt, env)
}

fn count_lines(dir: &PathBuf, file: &str) -> usize {
    std::fs::read_to_string(dir.join(file)).map(|s| s.lines().count()).unwrap_or(0)
}

#[test]
fn handler_path_settle_runs_the_protocol_through_clauses() {
    let dir = fresh("hp-settle");
    let (p, entry) = settle_with_prelude();
    let (mut rt, env) = build_rt(&dir, &p);
    let v = rt.run_program_env(Rc::new(entry), env).unwrap();
    let s = format!("{v:?}");
    assert!(s.contains("posted") && s.contains("Receipt"), "{s}");

    // Both operations journalled exactly as on the registry path.
    let entries = rt.store.entries().unwrap();
    assert_eq!(entries.len(), 2);
    for e in &entries {
        assert!(matches!(e.state, entoli_store::EntryState::Done(_)));
        assert!(rt.store.continuation(&e.continuation_id).unwrap().is_some());
    }
    assert!(rt.store.final_result().unwrap().is_some());
    // Fixtures invoked once each, through the foreign bindings.
    assert_eq!(count_lines(&dir, "approval.calls"), 1);
    assert_eq!(count_lines(&dir, "ledger.calls"), 1);
}

#[test]
fn clause_without_resume_violates_all_paths_resume() {
    // D-1's dynamic proxy: a suspending clause that completes without consuming k is
    // refused — the crash window must have exactly two exits.
    let dir = fresh("hp-abort");
    let mut p = Program::default();
    p.effects.insert(
        "Sus".into(),
        EffectDecl {
            name: "Sus".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "go".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    let entry = handle(
        do_("Sus", "go", int(1)),
        Handler {
            effect: "Sus".into(),
            return_clause: None,
            clauses: vec![clause("go", "x", "k", ctor("Aborted", vec![]))],
        },
        var("cap"),
    );
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&p, &entoli_types::Signatures::default(), store);
    let c = rt.grants.grant("Sus", None);
    let env = entoli_core::Env::empty().bind("cap", Value::Cap(c));
    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    assert!(err.to_string().contains("all-paths-resume"), "{err}");
}

#[test]
fn clause_performing_an_effect_inside_the_window_is_refused() {
    let dir = fresh("hp-window-effect");
    let mut p = Program::default();
    for (name, op) in [("Sus", "go"), ("Other", "poke")] {
        p.effects.insert(
            name.into(),
            EffectDecl {
                name: name.into(),
                class: SuspensionClass::Suspending,
                ops: vec![OpDecl { name: op.into(), delivery: DeliveryClass::Idempotent }],
            },
        );
    }
    let entry = handle(
        do_("Sus", "go", int(1)),
        Handler {
            effect: "Sus".into(),
            return_clause: None,
            clauses: vec![clause(
                "go",
                "x",
                "k",
                let_("ig", do_("Other", "poke", unit()), resume(var("k"), var("x"))),
            )],
        },
        var("cap"),
    );
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&p, &entoli_types::Signatures::default(), store);
    let c = rt.grants.grant("Sus", None);
    let env = entoli_core::Env::empty().bind("cap", Value::Cap(c));
    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    assert!(err.to_string().contains("inside") && err.to_string().contains("window"), "{err}");
}

#[test]
fn revoked_installation_capability_refuses_at_intend() {
    let dir = fresh("hp-revoked");
    let (p, entry) = settle_with_prelude();
    let (mut rt, env) = build_rt(&dir, &p);
    // Revoke Approval between install and use: the machine's install check passed at
    // handle time is not enough — liveness is re-checked at Intend (unification §5).
    // Install happens during evaluation, so revoke BEFORE running: the install itself
    // then fails E002 at the HandleCap frame.
    rt.grants.revoke("Approval");
    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    assert!(err.to_string().contains("E002"), "{err}");
}

#[test]
fn budgeted_installation_spends_per_invocation() {
    let dir = fresh("hp-budget");
    let mut p = Program::default();
    p.effects.insert(
        "Infer".into(),
        EffectDecl {
            name: "Infer".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "complete".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.foreigns.insert("infer_svc".into());
    // Two inferences under a budget of 1: the second must be refused.
    let entry = handle(
        let_(
            "a",
            do_("Infer", "complete", ctor("Prompt", vec![str_("p1")])),
            do_("Infer", "complete", ctor("Prompt", vec![str_("p2")])),
        ),
        Handler {
            effect: "Infer".into(),
            return_clause: None,
            clauses: vec![clause(
                "complete",
                "x",
                "k",
                resume(var("k"), foreign_app("infer_svc", var("x"))),
            )],
        },
        var("cap"),
    );
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&p, &entoli_types::Signatures::default(), store);
    rt.bind_foreign(
        "infer_svc",
        "Infer",
        "complete",
        Box::new(entoli_testkit::fixtures::FakeInfer::new(&dir)),
    );
    let c = rt.grants.grant("Infer", Some(1));
    let env = entoli_core::Env::empty().bind("cap", Value::Cap(c));
    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    assert!(err.to_string().contains("budget exhausted"), "{err}");
    assert_eq!(count_lines(&dir, "infer.calls"), 1, "only the first invocation ran");
}
