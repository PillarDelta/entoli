//! P-4 / P-5 dynamic-proxy closures: the empty-journal recovery door drops
//! machine-local privilege, and a non-tail resume in a durable window is
//! refused before Commit.

use entoli_core::Value;
use entoli_runtime::Runtime;
use entoli_store::{DurableStore, SqliteStore};
use entoli_syntax::build::*;
use entoli_syntax::{DeliveryClass, EffectDecl, Handler, OpDecl, Program, SuspensionClass};
use std::path::PathBuf;
use std::rc::Rc;

fn fresh(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn inv_program() -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Inv".into(),
        EffectDecl {
            name: "Inv".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "reserve".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.foreigns.insert("inv_svc".into());
    p
}

#[test]
fn p5_non_tail_resume_in_durable_window_is_refused_before_commit() {
    // Builder-level (unchecked) program: the clause resumes mid-body and keeps
    // computing. Pre-fix the machine committed the POST-resume value; now the
    // runtime refuses before Commit and the entry stays pending.
    let dir = fresh("p5-nontail");
    let p = inv_program();
    let entry = handle(
        do_("Inv", "reserve", str_("x")),
        Handler {
            effect: "Inv".into(),
            return_clause: None,
            clauses: vec![clause(
                "reserve",
                "x",
                "k",
                let_(
                    "mid",
                    resume(var("k"), foreign_app("inv_svc", var("x"))),
                    int(999), // runs after the resume: the third exit P-5 closes
                ),
            )],
        },
        var("cap"),
    );
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&p, &entoli_types::Signatures::default(), store);
    rt.bind_foreign(
        "inv_svc", "Inv", "reserve",
        Box::new(entoli_testkit::fixtures::FakeInventory::new(&dir)),
    );
    let cap = rt.grants.grant("Inv", None);
    let env = entoli_core::Env::empty().bind("cap", Value::Cap(cap));
    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("E004"), "{msg}");
    assert_eq!(rt.store.pending_stack().unwrap().len(), 1, "no Commit happened");
}

#[test]
fn p4_machine_local_ops_are_masked_in_the_empty_journal_door() {
    // An (unchecked) program whose entry point performs a machine-local read.
    // The none-arm runs the entry directly; pre-fix it inherited the driver's
    // privilege, post-fix the privilege ends at the `none` dispatch.
    let dir = fresh("p4-door");
    let p = Program::default();
    let entry = do_("Local", "load_kont", str_("0"));
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&p, &entoli_types::Signatures::default(), store);
    let driver = entoli_recovery::driver_for(&p, &entry);
    let err = rt.recover_env(Rc::new(driver), entoli_core::Env::empty()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("outside the recovery driver"), "{msg}");
}
