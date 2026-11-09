//! Phase 3 exit conditions in-process: Intend atomicity (I2 by construction), Rule 2's
//! failed-commit-as-no-transition, and invariant checking.

use entoli_core::Value;
use entoli_runtime::Runtime;
use entoli_store::{assert_store_invariants, DurableStore, SqliteStore};
use std::path::PathBuf;
use std::rc::Rc;

/// The unified settle setup: prelude program, foreign bindings, grants, cap env.
fn unified_settle(dir: &PathBuf, store: SqliteStore) -> (Runtime, entoli_core::Env, entoli_syntax::Term) {
    let (program, entry) = entoli_testkit::programs::settle_unified();
    let mut rt = Runtime::new(&program, &entoli_types::Signatures::default(), store);
    rt.bind_foreign("approval_svc", "Approval", "request", Box::new(entoli_testkit::fixtures::FakeApproval::new(dir)));
    rt.bind_foreign("ledger_svc", "Ledger", "post", Box::new(entoli_testkit::fixtures::FakeLedger::new(dir)));
    let ca = rt.grants.grant("Approval", None);
    let cl = rt.grants.grant("Ledger", None);
    let env = entoli_core::Env::empty()
        .bind("cap_approval", Value::Cap(ca))
        .bind("cap_ledger", Value::Cap(cl));
    (rt, env, entry)
}

fn fresh_store(name: &str) -> (PathBuf, SqliteStore) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (dir.clone(), SqliteStore::open(&dir.join("store.sqlite")).unwrap())
}

#[test]
fn induced_commit_failure_is_a_no_transition() {
    // Rule 2 (§7.3): the durable commit may fail, in which case σ is left exactly as it
    // was — and a declined step 1 means the effect was never attempted.
    let (dir, mut store) = fresh_store("rule2");
    store.fail_next_commit = true;
    let (mut rt, env, entry) = unified_settle(&dir, store);

    let err = rt.run_program_env(Rc::new(entry), env).unwrap_err();
    assert!(err.to_string().contains("refused"), "{err}");

    // σ unchanged: no entries, no continuations reachable, no final result.
    assert!(rt.store.entries().unwrap().is_empty());
    assert!(rt.store.final_result().unwrap().is_none());
    // And the effect was never attempted: the approval fixture was never invoked.
    assert!(!dir.join("approval.calls").exists());
    assert_store_invariants(&rt.store).unwrap();
}

#[test]
fn happy_path_in_process_settle() {
    let (dir, store) = fresh_store("happy");
    let (mut rt, env, entry) = unified_settle(&dir, store);

    let v = rt.run_program_env(Rc::new(entry), env).unwrap();
    let s = format!("{v:?}");
    assert!(s.contains("posted") && s.contains("Receipt"), "{s}");

    // Two journalled effects, both done; every entry's continuation durable (I2);
    // final result recorded.
    let entries = rt.store.entries().unwrap();
    assert_eq!(entries.len(), 2);
    for e in &entries {
        assert!(rt.store.continuation(&e.continuation_id).unwrap().is_some());
        assert!(matches!(e.state, entoli_store::EntryState::Done(_)));
    }
    assert!(rt.store.final_result().unwrap().is_some());
    assert_store_invariants(&rt.store).unwrap();
}

#[test]
fn machine_local_effects_are_refused_outside_the_driver() {
    // Plan §8: the machine-local class is granted only to the recovery driver.
    let (_dir, store) = fresh_store("localgate");
    let (program, _) = entoli_testkit::programs::settle_program();
    let mut rt = Runtime::new(&program, &entoli_types::Signatures::default(), store);
    let sneaky = entoli_syntax::build::do_("Local", "frontier", entoli_syntax::build::unit());
    let err = rt.run_program(Rc::new(sneaky)).unwrap_err();
    assert!(err.to_string().contains("outside the recovery driver"), "{err}");
}
