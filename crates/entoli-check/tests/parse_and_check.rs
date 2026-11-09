//! The demonstration-D surface syntax, end to end: parse → resolve → validate → check.
//! Includes the full settle program in concrete syntax, a surface-syntax demo B
//! (compile-fail naming both causes), and parse→eval round trips.

use entoli_check::parse::parse_program;
use entoli_check::Checker;
use entoli_types::{Row, Type};

const SETTLE: &str = r#"
# Demonstration A, in the plain surface syntax.
effect Approval suspending { request : Data -> Data @at_least_once }
effect Ledger   suspending { post    : Data -> Data @at_least_once }

def audit : Str -[]-> Data = fn inv -> Audited(inv, 250)

def settle : Str -[Approval, Ledger]-> Data =
  fn inv ->
    let f = audit(inv) in
    match do Approval.request(f) witness concat("req:", inv) {
      approve() -> Posted(do Ledger.post(f) witness concat("idem:", inv))
      reject()  -> Halted(f)
    }

def main : Str -[Approval, Ledger]-> Data = fn inv -> settle(inv)
"#;

#[test]
fn settle_in_surface_syntax_parses_and_checks_clean() {
    let (program, sigs) = parse_program(SETTLE).expect("parse");
    program.validate().expect("validate");
    let mut c = Checker::new(&program, &sigs);
    c.check_defs();
    assert!(c.diags.is_empty(), "{:?}", c.diags);
    // Globals were resolved: settle's body references audit as a Global.
    let settle = program.defs.get("settle").unwrap();
    let mut found = std::collections::BTreeSet::new();
    entoli_encode_globals(settle, &mut found);
    assert!(found.contains("audit"), "{found:?}");
}

fn entoli_encode_globals(t: &entoli_syntax::Term, acc: &mut std::collections::BTreeSet<String>) {
    use entoli_syntax::Term::*;
    match t {
        Global(n) => {
            acc.insert(n.clone());
        }
        Lam { body, .. } => entoli_encode_globals(body, acc),
        App { func, arg } => {
            entoli_encode_globals(func, acc);
            entoli_encode_globals(arg, acc);
        }
        Let { bound, body, .. } => {
            entoli_encode_globals(bound, acc);
            entoli_encode_globals(body, acc);
        }
        Match { scrut, arms } => {
            entoli_encode_globals(scrut, acc);
            for a in arms.iter() {
                entoli_encode_globals(&a.body, acc);
            }
        }
        Do { arg, witness, .. } => {
            entoli_encode_globals(arg, acc);
            if let Some(w) = witness {
                entoli_encode_globals(w, acc);
            }
        }
        Ctor { args, .. } | Prim { args, .. } => {
            for a in args {
                entoli_encode_globals(a, acc);
            }
        }
        _ => {}
    }
}

#[test]
fn surface_syntax_demo_b_fails_with_both_causes_named() {
    let src = r#"
effect Http prompt { get  : Data -> Data @idempotent }
effect Fs   prompt { read : Data -> Data @idempotent }

def research : Data -[Http, Fs]-> Data =
  fn task -> Pair(do Http.get(task), do Fs.read(task))

# delegate claims an empty row while research still performs Fs.
def delegate : Data -[]-> Data =
  fn task ->
    handle research(task) with Http {
      get(x; k) -> resume k x
    } using net
"#;
    let (program, sigs) = parse_program(src).expect("parse");
    let mut c = Checker::new(&program, &sigs);
    c.check_defs();
    let msgs: Vec<String> = c.diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E001") && m.contains("Fs")),
        "escaping effect named: {msgs:?}"
    );
    // `net` is unbound inside delegate — the def-level diagnostic set names it too.
    assert!(msgs.iter().any(|m| m.contains("net")), "{msgs:?}");
}

#[test]
fn parsed_program_evaluates_correctly() {
    let src = r#"
effect Ask prompt { ask : Int -> Int @idempotent }

def double_via_ask : Int -[Ask]-> Int =
  fn n -> do Ask.ask(n) + do Ask.ask(n)

def main : Unit -[Ask]-> Int =
  fn u -> 1 + double_via_ask(20)
"#;
    let (program, sigs) = parse_program(src).expect("parse");
    let mut c = Checker::new(&program, &sigs);
    c.check_defs();
    assert!(c.diags.is_empty(), "{:?}", c.diags);

    // Evaluate: handler doubles the argument; 1 + (40 + 40) = 81.
    use entoli_core::{CapId, Machine, MapAuthority, Outcome, Value};
    use entoli_syntax::build::*;
    let mut auth = MapAuthority::default();
    auth.grant(CapId(1), "Ask");
    let entry = handle(
        app(global("main"), unit()),
        entoli_syntax::Handler {
            effect: "Ask".into(),
            return_clause: None,
            clauses: vec![clause(
                "ask",
                "x",
                "k",
                resume(var("k"), prim(entoli_syntax::PrimOp::Mul, vec![var("x"), int(2)])),
            )],
        },
        var("cap"),
    );
    let mut m = Machine::new(&program, &auth, std::rc::Rc::new(entry))
        .with_env(entoli_core::Env::empty().bind("cap", Value::Cap(CapId(1))));
    match m.run_fuel(1_000_000).unwrap() {
        Outcome::Value(Value::Int(i)) => assert_eq!(i, 81),
        other => panic!("{other:?}"),
    }
}

#[test]
fn witnessless_at_least_once_is_rejected_in_surface_syntax() {
    let src = r#"
effect Ledger suspending { post : Data -> Data @at_least_once }
def main : Data -[Ledger]-> Data = fn e -> do Ledger.post(e)
"#;
    let (program, sigs) = parse_program(src).expect("parse");
    let mut c = Checker::new(&program, &sigs);
    c.check_defs();
    let msgs: Vec<String> = c.diags.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m.contains("at-least-once")), "{msgs:?}");
}

#[test]
fn kont_type_annotations_parse() {
    let (_, sigs) = parse_program(
        "def f : (Int -[Ask]-o Data) -[Ask]-> Data = fn k -> resume k 1",
    )
    .expect("parse");
    match sigs.defs.get("f") {
        Some(Type::Fn(a, r, _)) => {
            assert!(matches!(**a, Type::Kont(..)));
            assert!(r.contains("Ask"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn row_and_delivery_appear_in_diagnostics_not_just_booleans() {
    // The diagnostic text is part of the Phase 5 result (plan §12): it must name the
    // violated rule's subject.
    let src = r#"
effect A prompt { p : Int -> Int @idempotent }
effect Sus suspending { s : Unit -> Int @idempotent }
def main : Unit -[A, Sus]-> Int =
  fn u ->
    handle do A.p(1) with A {
      p(x; k) -> let ig = do Sus.s(()) in resume k x
    } using cap
"#;
    let (program, sigs) = parse_program(src).expect("parse");
    let mut c = Checker::new(&program, &sigs);
    c.check_defs();
    let msgs: Vec<String> = c.diags.iter().map(|d| d.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("E005") && m.contains("Sus")),
        "E005 names the suspending effect: {msgs:?}"
    );
}
