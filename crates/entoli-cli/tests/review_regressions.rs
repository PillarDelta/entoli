//! Regressions for the adversarial-review findings (see EVALUATION.md R-1..R-6):
//! each of these was a CONFIRMED defect reproduced against the pre-fix tree.

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

fn inventory_events(dir: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(dir.join("inventory.txt"))
        .map(|s| s.lines().map(|l| l.split('\t').next().unwrap_or("").to_string()).collect())
        .unwrap_or_default()
}

/// R-1 (blocker): crash AFTER the retried operation's Commit during a compensable
/// recovery. Pre-fix: the store wedged permanently ([0 Done, 0.0 Done], invariants
/// clean, `complete on non-pending 0` forever). Post-fix: Protocol.complete honors its
/// own idempotent declaration and returns the stored result.
#[test]
fn r1_crash_after_retry_commit_during_compensable_recovery_recovers() {
    let dir = fresh_dir("review-r1");
    let d = dir.to_str().unwrap();

    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "run", "--crash",
        "Inventory.reserve:after_remote_before_receive",
    ]);
    assert_eq!(code, Some(87));

    // Recovery compensates, retries reserve, commits Done(0) — and dies before resume.
    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "recover", "--crash",
        "Inventory.reserve:after_commit",
    ]);
    assert_eq!(code, Some(87));

    // The store now reads [0 Done, 0.0 Done], no final — the state that used to wedge.
    let (code, stdout, _) = run(&["check", "--dir", d]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("ENTRY 0 parent=- Inventory.reserve done"), "{stdout}");
    assert!(stdout.contains("ENTRY 0.0 parent=0 Inventory.release done"), "{stdout}");
    assert!(stdout.contains("FINAL no"), "{stdout}");

    // One more recovery must complete the program, not error forever.
    let (code, stdout, stderr) = run(&["demo", "--which", "a2", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "post-fix recovery must succeed: {stderr}");
    assert!(stdout.contains("done"), "{stdout}");
    assert_eq!(inventory_events(&dir), vec!["RESERVE", "RELEASE", "RESERVE"]);
    let ledger = std::fs::read_to_string(dir.join("ledger.txt")).unwrap_or_default();
    assert_eq!(ledger.lines().filter(|l| l.starts_with("POST")).count(), 1);
    let (code, stdout, _) = run(&["check", "--dir", d]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("FINAL yes"), "{stdout}");
}

/// R-2: the machine-local privilege must not survive the hand-back of control to the
/// program. Pre-fix, a program performing Local.load_kont was refused in a fresh run
/// but SERVED after being resumed by the recovery driver — and could double-resume its
/// own persisted continuation.
#[test]
fn r2_machine_local_effects_refused_after_control_returns_to_program() {
    let dir = fresh_dir("review-r2");
    let d = dir.to_str().unwrap();

    // Fresh run: refused at the sneaky Local.load_kont (driver not active).
    let (code, _, stderr) = run(&["demo", "--which", "sneaky", "--dir", d, "--mode", "run"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("outside the recovery driver"), "{stderr}");

    // Crash the same program inside the ledger window instead, then recover: the
    // driver resumes it, and its sneaky Local.load_kont must STILL be refused.
    let dir2 = fresh_dir("review-r2b");
    let d2 = dir2.to_str().unwrap();
    let (code, _, _) = run(&[
        "demo", "--which", "sneaky", "--dir", d2, "--mode", "run", "--crash",
        "Ledger.post:after_commit",
    ]);
    assert_eq!(code, Some(87));
    let (code, _, stderr) = run(&["demo", "--which", "sneaky", "--dir", d2, "--mode", "recover"]);
    assert_eq!(code, Some(1), "the leak is closed: {stderr}");
    assert!(
        stderr.contains("after control returned to the program"),
        "refusal names the boundary: {stderr}"
    );
}
