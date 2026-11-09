//! Phase 7: the conformance harness proves declared delivery classes against sandboxes
//! (plan §11.3), catches a false claim, and strict mode refuses unproven handlers.

use entoli_runtime::Runtime;
use entoli_store::{DurableStore, SqliteStore};
use entoli_testkit::conformance;
use std::path::PathBuf;
use std::rc::Rc;

fn root(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn all_demonstration_fixtures_pass_their_declared_contracts() {
    let r = root("contracts-all");
    let reports = conformance::run_all(&r);
    assert_eq!(reports.len(), 5);
    for rep in &reports {
        assert!(rep.passed, "{rep}");
    }
}

#[test]
fn a_false_at_least_once_claim_is_caught() {
    let r = root("contracts-lying");
    let report = conformance::check_at_least_once(
        &r,
        "lying-ledger",
        Box::new(|d| Box::new(conformance::LyingLedger::new(d))),
        "Ledger",
        "post",
        entoli_core::Value::Ctor { name: "Entry".into(), args: vec![] },
        Box::new(|d| {
            let mut v = entoli_testkit::fixtures::read_lines(&d.join("ledger.txt"));
            v.sort();
            v
        }),
    );
    assert!(!report.passed, "the harness must catch a service that does not deduplicate");
}

#[test]
fn strict_mode_refuses_unproven_contracts_and_admits_proven_ones() {
    let dir = root("contracts-strict");
    let (program, entry) = entoli_testkit::programs::settle_unified();
    let mk = |dir: &std::path::PathBuf| {
        let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
        let mut rt = Runtime::new(&program, &entoli_types::Signatures::default(), store);
        rt.bind_foreign(
            "approval_svc", "Approval", "request",
            Box::new(entoli_testkit::fixtures::FakeApproval::new(dir)),
        );
        rt.bind_foreign(
            "ledger_svc", "Ledger", "post",
            Box::new(entoli_testkit::fixtures::FakeLedger::new(dir)),
        );
        let ca = rt.grants.grant("Approval", None);
        let cl = rt.grants.grant("Ledger", None);
        let env = entoli_core::Env::empty()
            .bind("cap_approval", entoli_core::Value::Cap(ca))
            .bind("cap_ledger", entoli_core::Value::Cap(cl));
        rt.strict_contracts = true;
        (rt, env)
    };

    // Nothing proven: the first suspending invocation is refused.
    let (mut rt, env) = mk(&dir);
    let err = rt.run_program_env(Rc::new(entry.clone()), env).unwrap_err();
    assert!(err.to_string().contains("delivery contract"), "{err}");

    // Run the harness, feed the passes in, and the same program runs.
    let mut passed = std::collections::BTreeSet::new();
    for rep in conformance::run_all(&dir.join("sandbox")) {
        if rep.passed {
            passed.insert((rep.effect.clone(), rep.op.clone()));
        }
    }
    let dir2 = root("contracts-strict-2");
    let (mut rt2, env2) = mk(&dir2);
    rt2.contracts_passed = passed;
    let v = rt2.run_program_env(Rc::new(entry), env2).unwrap();
    assert!(format!("{v:?}").contains("posted"));
}

#[test]
fn a_compensation_unsafe_against_a_clean_state_is_caught() {
    // P-3 (paper v0.6 §8.3): the v0.5 equation op ; op' ; op ≈ op passes for
    // this fixture — the old harness approved it — but the ambiguous Pending
    // case realizes op' ; op against a possibly-clean state, and there the
    // compensation leaves an artifact. The extended family catches it.
    let r = root("contracts-overzealous");
    let rep = conformance::check_compensable(
        &r,
        "overzealous",
        Box::new(|d| Box::new(conformance::OverzealousCompensation::new(d))),
        "Inventory",
        "reserve",
        "release",
        entoli_core::Value::Str("x".into()),
        Box::new(conformance::inventory_net),
    );
    assert!(!rep.passed, "the a=0 instance must fail: {rep}");
    assert!(rep.detail.contains("op^0"), "{rep}");
}

#[test]
fn strict_mode_gates_recovery_reinvocation_too() {
    // F-009: strict mode's guarantee is that an unproven contract never reaches
    // the world — at EITHER invocation site. Pre-fix only protocol() (fresh
    // Intend) checked; recovery re-invocation executed the clause unchecked.
    //
    // A failing foreign call leaves the same journal state as a crash in the
    // window: entry Pending, continuation durable.
    struct FailingRemote;
    impl entoli_runtime::ExternalOp for FailingRemote {
        fn invoke(
            &mut self,
            _op: &str,
            _arg: &entoli_core::Value,
            _witness: Option<&str>,
            _crash: &entoli_runtime::CrashInjector,
        ) -> Result<entoli_core::Value, String> {
            Err("remote unavailable".into())
        }
    }

    let dir = root("contracts-strict-recovery");
    let (program, entry) = entoli_testkit::programs::settle_unified();
    let mk = |strict: bool| {
        let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
        let mut rt = Runtime::new(&program, &entoli_types::Signatures::default(), store);
        rt.bind_foreign(
            "approval_svc", "Approval", "request",
            Box::new(entoli_testkit::fixtures::FakeApproval::new(&dir)),
        );
        rt.bind_foreign(
            "ledger_svc", "Ledger", "post",
            Box::new(entoli_testkit::fixtures::FakeLedger::new(&dir)),
        );
        let ca = rt.grants.grant("Approval", None);
        let cl = rt.grants.grant("Ledger", None);
        let env = entoli_core::Env::empty()
            .bind("cap_approval", entoli_core::Value::Cap(ca))
            .bind("cap_ledger", entoli_core::Value::Cap(cl));
        rt.strict_contracts = strict;
        (rt, env)
    };

    // Run 1 (strict off — the operator enables strict later): the remote fails
    // inside the window; the entry stays Pending.
    let (mut rt1, env1) = mk(false);
    rt1.bind_foreign("approval_svc", "Approval", "request", Box::new(FailingRemote));
    assert!(rt1.run_program_env(Rc::new(entry.clone()), env1).is_err());
    assert_eq!(rt1.store.pending_stack().unwrap().len(), 1);
    drop(rt1);

    // Run 2 (fresh process, strict on, nothing proven): recovery re-invocation
    // must be refused, and the entry must STAY pending — a refusal, not a wedge.
    let (mut rt2, env2) = mk(true);
    let driver =
        entoli_testkit::programs::settle_wrap(entoli_recovery::driver_for(&program, &entry));
    let err = rt2.recover_env(Rc::new(driver), env2).unwrap_err();
    assert!(err.to_string().contains("strict mode"), "{err}");
    assert_eq!(rt2.store.pending_stack().unwrap().len(), 1);
    drop(rt2);

    // Run 3 (fresh process, strict on, contracts proven by the harness): the
    // same recovery completes.
    let mut passed = std::collections::BTreeSet::new();
    for rep in conformance::run_all(&dir.join("sandbox")) {
        if rep.passed {
            passed.insert((rep.effect.clone(), rep.op.clone()));
        }
    }
    let (mut rt3, env3) = mk(true);
    rt3.contracts_passed = passed;
    let driver =
        entoli_testkit::programs::settle_wrap(entoli_recovery::driver_for(&program, &entry));
    let v = rt3.recover_env(Rc::new(driver), env3).unwrap();
    assert!(format!("{v:?}").contains("posted"), "{v:?}");
    assert!(rt3.store.pending_stack().unwrap().is_empty());
}
