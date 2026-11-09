//! F-011: recovery dispatch is a function of σ alone. The delivery class in force
//! at Intend is recorded in the journal entry; recovery dispatches on the record,
//! and a program in force whose declaration disagrees REFUSES — it never silently
//! dispatches differently. (Pre-fix, `frontier` resolved δ against the program in
//! force: a redeployment changing a pending op's delivery class changed what
//! recovery *did*, and a compensable-chain redeployment could push pending depth
//! past I1's bound before any drift check ran.)

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

fn inv_program(reserve_delivery: DeliveryClass) -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Inv".into(),
        EffectDecl {
            name: "Inv".into(),
            class: SuspensionClass::Suspending,
            ops: vec![
                OpDecl { name: "reserve".into(), delivery: reserve_delivery },
                OpDecl { name: "release".into(), delivery: DeliveryClass::Idempotent },
            ],
        },
    );
    p.foreigns.insert("inv_reserve".into());
    p.foreigns.insert("inv_release".into());
    p
}

fn prelude(inner: Term) -> Term {
    handle(
        inner,
        Handler {
            effect: "Inv".into(),
            return_clause: None,
            clauses: vec![
                clause("reserve", "x", "k", resume(var("k"), foreign_app("inv_reserve", var("x")))),
                clause("release", "x", "k", resume(var("k"), foreign_app("inv_release", var("x")))),
            ],
        },
        var("cap_inv"),
    )
}

struct FailingRemote;
impl entoli_runtime::ExternalOp for FailingRemote {
    fn invoke(
        &mut self,
        _op: &str,
        _arg: &Value,
        _witness: Option<&str>,
        _crash: &entoli_runtime::CrashInjector,
    ) -> Result<Value, String> {
        Err("remote unavailable".into())
    }
}

fn build_rt(dir: &PathBuf, p: &Program, working: bool) -> (Runtime, entoli_core::Env) {
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(p, &entoli_types::Signatures::default(), store);
    if working {
        rt.bind_foreign(
            "inv_reserve", "Inv", "reserve",
            Box::new(entoli_testkit::fixtures::FakeInventory::new(dir)),
        );
    } else {
        rt.bind_foreign("inv_reserve", "Inv", "reserve", Box::new(FailingRemote));
    }
    rt.bind_foreign(
        "inv_release", "Inv", "release",
        Box::new(entoli_testkit::fixtures::FakeInventory::new(dir)),
    );
    let ci = rt.grants.grant("Inv", None);
    let env = entoli_core::Env::empty().bind("cap_inv", Value::Cap(ci));
    (rt, env)
}

#[test]
fn delivery_drift_between_suspension_and_recovery_refuses() {
    let dir = fresh("f011-drift");
    let p1 = inv_program(DeliveryClass::Compensable {
        comp_effect: "Inv".into(),
        comp_op: "release".into(),
    });
    let entry = prelude(do_("Inv", "reserve", str_("x")));

    // Run 1 under P1: the remote fails inside the window; reserve stays Pending,
    // and the entry carries the delivery class in force at Intend.
    let (mut rt1, env1) = build_rt(&dir, &p1, false);
    assert!(rt1.run_program_env(Rc::new(entry.clone()), env1).is_err());
    let frontier = rt1.store.frontier().unwrap().unwrap();
    assert_eq!(frontier.delivery, "compensable:Inv.release");
    drop(rt1);

    // Run 2 under P2, which redeclares reserve as idempotent: recovery must
    // REFUSE — pre-fix it silently dispatched pending_re and re-invoked with no
    // compensation.
    let p2 = inv_program(DeliveryClass::Idempotent);
    let (mut rt2, env2) = build_rt(&dir, &p2, true);
    let driver = prelude(entoli_recovery::driver_for(&p2, &entry));
    let err = rt2.recover_env(Rc::new(driver), env2).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("E012") && msg.contains("recorded delivery"), "{msg}");
    assert_eq!(rt2.store.pending_stack().unwrap().len(), 1, "entry must stay pending");
    drop(rt2);

    // Run 3 under the original P1: the recorded dispatch agrees with the program
    // in force, and recovery completes through the compensation path.
    let (mut rt3, env3) = build_rt(&dir, &p1, true);
    let driver = prelude(entoli_recovery::driver_for(&p1, &entry));
    let v = rt3.recover_env(Rc::new(driver), env3).unwrap();
    assert!(format!("{v:?}").contains("Reserved"), "{v:?}");
    assert!(rt3.store.pending_stack().unwrap().is_empty());
}
