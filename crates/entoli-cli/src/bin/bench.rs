//! Performance measurement for the paper's evaluation (roadmap T5.3 / audit
//! A-06, A-13, A-18): per-effect cost as the journal grows, and encoded
//! continuation size as nesting depth grows. Run: `entoli-bench <out_dir>`.
//! Prints CSV to stdout; no assertions — this is measurement, not a test.

use entoli_core::Value;
use entoli_runtime::{CrashInjector, ExternalOp, Runtime};
use entoli_store::{DurableStore, SqliteStore};
use entoli_syntax::build::*;
use entoli_syntax::{
    DeliveryClass, EffectDecl, Handler, OpDecl, Program, SuspensionClass, Term,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

struct Echo;
impl ExternalOp for Echo {
    fn invoke(
        &mut self,
        _op: &str,
        arg: &Value,
        _witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<Value, String> {
        Ok(arg.clone())
    }
}

fn program() -> Program {
    let mut p = Program::default();
    p.effects.insert(
        "Tick".into(),
        EffectDecl {
            name: "Tick".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "tick".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.foreigns.insert("tick_svc".into());
    p
}

fn prelude(inner: Term) -> Term {
    handle(
        inner,
        Handler {
            effect: "Tick".into(),
            return_clause: None,
            clauses: vec![clause(
                "tick",
                "x",
                "k",
                resume(var("k"), foreign_app("tick_svc", var("x"))),
            )],
        },
        var("cap_tick"),
    )
}

/// N sequential effects: let _ = do Tick.tick(i) in ... in last.
fn seq_effects(n: usize) -> Term {
    let mut body = do_("Tick", "tick", int(n as i64));
    for i in (0..n.saturating_sub(1)).rev() {
        body = let_("_x", do_("Tick", "tick", int(i as i64)), body);
    }
    body
}

/// One effect at depth D with D LIVE Let frames at capture: the effect sits in
/// each let's BOUND expression, and value-bindings are interleaved so each live
/// frame's environment grows with depth (the audit A-18 shape: Θ(D·B) encoding
/// without env sharing).
fn deep_effect(d: usize) -> Term {
    let mut body = do_("Tick", "tick", int(0));
    for i in (0..d).rev() {
        body = let_(
            &format!("v{i}"),
            int(i as i64),
            let_(&format!("a{i}"), body, var(&format!("a{i}"))),
        );
    }
    body
}

fn fresh(root: &PathBuf, name: &str) -> PathBuf {
    let dir = root.join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_once(dir: &PathBuf, entry: Term) -> (f64, u64, usize, usize) {
    let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
    let mut rt = Runtime::new(&program(), &entoli_types::Signatures::default(), store);
    rt.bind_foreign("tick_svc", "Tick", "tick", Box::new(Echo));
    let cap = rt.grants.grant("Tick", None);
    let env = entoli_core::Env::empty().bind("cap_tick", Value::Cap(cap));
    let t0 = Instant::now();
    rt.run_program_env(Rc::new(entry), env).unwrap();
    let secs = t0.elapsed().as_secs_f64();
    let entries = rt.store.entries().unwrap();
    let n_entries = entries.len();
    let max_blob = entries
        .iter()
        .filter_map(|e| rt.store.continuation(&e.continuation_id).unwrap())
        .map(|k| k.encoded_frames.len())
        .max()
        .unwrap_or(0);
    drop(rt);
    let db = std::fs::metadata(dir.join("store.sqlite")).map(|m| m.len()).unwrap_or(0);
    (secs, db, n_entries, max_blob)
}

fn main() {
    let out = PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| "bench-out".into()),
    );
    std::fs::create_dir_all(&out).unwrap();

    println!("experiment,param,total_secs,per_effect_ms,db_bytes,entries,max_blob_bytes");
    // Experiment 1: per-effect cost vs journal length (sequential effects).
    for n in [25usize, 50, 100, 200, 400] {
        let dir = fresh(&out, &format!("seq-{n}"));
        let (secs, db, entries, blob) = run_once(&dir, prelude(seq_effects(n)));
        println!(
            "journal_growth,{n},{secs:.4},{:.4},{db},{entries},{blob}",
            secs * 1000.0 / n as f64
        );
    }
    // Experiment 2: encoded continuation size vs captured depth (one effect).
    for d in [10usize, 25, 50, 100, 200] {
        let dir = fresh(&out, &format!("deep-{d}"));
        let (secs, db, entries, blob) = run_once(&dir, prelude(deep_effect(d)));
        println!("depth_blob,{d},{secs:.4},{:.4},{db},{entries},{blob}", secs * 1000.0);
    }
}
