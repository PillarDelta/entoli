//! Tier 1 milestone (roadmap T1.1–T1.3): a SURFACE-SYNTAX program — foreign
//! declarations included — compiles under the entry gate, runs from source,
//! survives hard process death inside the crash window, and recovers from the
//! same file with exactly-once effects. This is the loop the authorship study
//! writes for: model → checker → runtime, no Rust in the path.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_entoli-cli")
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(bin()).args(args).output().expect("spawn");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PILOT: &str = r#"
effect Approval suspending { request : Data -> Data @at_least_once }
effect Ledger   suspending { post    : Data -> Data @at_least_once }
foreign approval_svc : Data ~> Data
foreign ledger_svc   : Data ~> Data

def audit : Str -[]-> Data = fn inv -> Audited(inv, 250)

def settle : Str -[Approval, Ledger]-> Data =
  fn inv ->
    let f = audit(inv) in
    match do Approval.request(f) witness concat("req:", inv) {
      approve() -> Posted(do Ledger.post(f) witness concat("idem:", inv))
      reject()  -> Halted(f)
    }

def wrap : Cap[Approval] -[]-> Cap[Ledger] -[]-> (Unit -[Approval, Ledger]-> Data) -[]-> Data =
  fn ca -> fn cl -> fn work ->
    handle
      handle work(()) with Approval { request(x; k) -> resume k (approval_svc(x)) } using ca
    with Ledger { post(x; k) -> resume k (ledger_svc(x)) } using cl

def main : Cap[Approval] -[]-> Cap[Ledger] -[]-> Unit -[]-> Data =
  fn ca -> fn cl -> fn u -> wrap(ca, cl, fn u2 -> settle("INV-7"))
"#;

fn write_pilot(dir: &PathBuf) -> String {
    let p = dir.join("pilot.ent");
    std::fs::write(&p, PILOT).unwrap();
    p.to_str().unwrap().to_string()
}

fn ledger_lines(dir: &PathBuf) -> usize {
    std::fs::read_to_string(dir.join("ledger.txt")).map(|s| s.lines().count()).unwrap_or(0)
}

fn std_args<'a>(file: &'a str, dir: &'a str) -> Vec<&'a str> {
    vec![
        file,
        "--dir",
        dir,
        "--grant",
        "Approval",
        "--grant",
        "Ledger",
        "--bind",
        "approval_svc=approval:Approval.request",
        "--bind",
        "ledger_svc=ledger:Ledger.post",
    ]
}

#[test]
fn surface_program_compiles_under_the_entry_gate() {
    let dir = fresh_dir("tier1-compile");
    let file = write_pilot(&dir);
    let (code, stdout, _) = run(&["compile", &file]);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("entry: main"), "{stdout}");

    // The gate refuses an undischarged row on main.
    let bad = PILOT.replace(
        "def main : Cap[Approval] -[]-> Cap[Ledger] -[]-> Unit -[]-> Data",
        "def main : Cap[Approval] -[]-> Cap[Ledger] -[]-> Unit -[Ledger]-> Data",
    );
    let badfile = dir.join("bad.ent");
    std::fs::write(&badfile, bad).unwrap();
    let (code, stdout, _) = run(&["compile", badfile.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("E001"), "{stdout}");
}

#[test]
fn surface_program_runs_from_source() {
    let dir = fresh_dir("tier1-run");
    let file = write_pilot(&dir);
    let mut args = vec!["run"];
    args.extend(std_args(&file, dir.to_str().unwrap()));
    let (code, stdout, stderr) = run(&args);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("Posted"), "{stdout}");
    assert_eq!(ledger_lines(&dir), 1);
}

#[test]
fn surface_program_survives_hard_death_and_recovers_exactly_once() {
    let dir = fresh_dir("tier1-crash");
    let file = write_pilot(&dir);
    let d = dir.to_str().unwrap();

    // Kill inside the window, remote already committed — the ambiguous case.
    let mut args = vec!["run"];
    args.extend(std_args(&file, d));
    args.extend(["--crash", "Ledger.post:after_remote_before_receive"]);
    let (code, _, _) = run(&args);
    assert_eq!(code, Some(87), "hard death expected");
    assert_eq!(ledger_lines(&dir), 1, "remote committed before the crash");

    // Recover cold, from the same source file: re-invokes under the recorded
    // witness, the remote deduplicates — exactly one entry survives.
    let mut args = vec!["recover"];
    args.extend(std_args(&file, d));
    let (code, stdout, stderr) = run(&args);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("Posted"), "{stdout}");
    assert_eq!(ledger_lines(&dir), 1, "exactly-once across the crash");
}

#[test]
fn study_failure_classes_now_parse() {
    // The 2026-11-08 authorship study's residual failure classes, as minimal
    // programs: parenthesised resume arguments, parenthesised continuation
    // types, keyword-swallowing after resume, string equality, and bare
    // nullary constructor patterns matching by NAME (A-28), not as wildcards.
    let dir = fresh_dir("tier1-grammar");
    let src = r#"
effect Tick prompt { tick : Int -> Int @idempotent }

def forward : (Int -[]-o Int) -[]-> Int -[]-> Int =
  fn k -> fn v -> resume k (v + 100)

def count3 : Unit -[Tick]-> Int =
  fn u -> do Tick.tick(1) + do Tick.tick(2) + do Tick.tick(3)

def pick : Str -[]-> Int = fn x -> if str_eq(x, "north") then 10 else 20

def shape : Unit -[]-> Int =
  fn u -> match Just(5) { None -> 0  Just(v) -> 1 }

def run_ticks : Cap[Tick] -[]-> Int =
  fn cap ->
    handle count3(()) with Tick {
      tick(x; k) -> forward(k, x)
      return r -> r + pick("north") + shape(())
    } using cap

def main : Cap[Tick] -[]-> Unit -[]-> Int = fn cap -> fn u -> run_ticks(cap)
"#;
    let p = dir.join("grammar.ent");
    std::fs::write(&p, src).unwrap();
    let (code, stdout, _) = run(&["compile", p.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout}");

    // And it computes the right answer: 306 + 10 + 1 (A-28: Just(5) must NOT
    // fall into the bare `None` arm).
    let (code, stdout, stderr) =
        run(&["run", p.to_str().unwrap(), "--dir", dir.to_str().unwrap(), "--grant", "Tick"]);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("317"), "expected 317: {stdout}");
}

#[test]
fn greater_than_parses_and_ablation_is_out_of_the_production_gate() {
    let dir = fresh_dir("tier1-gt-ablate");
    let src = r#"
def clamp : Int -[]-> Int = fn x -> if x > 100 then 100 else x
def main : Unit -[]-> Int = fn u -> clamp(250)
"#;
    let p = dir.join("gt.ent");
    std::fs::write(&p, src).unwrap();
    let (code, stdout, _) = run(&["compile", p.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout}");
    let (code, stdout, stderr) =
        run(&["run", p.to_str().unwrap(), "--dir", dir.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("100"), "{stdout}");

    // A-30: the production gate refuses ablation switches.
    let (code, stdout, _) = run(&["compile", p.to_str().unwrap(), "--ablate", "affine"]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("measurement-only"), "{stdout}");
}
