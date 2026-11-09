//! Demonstration A2 (plan §10.2): crash during compensation — required by Rule 3 and
//! "mandatory before recovery semantics may be called implemented."
//!
//! Sequence: reserve succeeds remotely, crash before local Commit; recovery selects the
//! compensation (release); the compensation succeeds (remotely AND locally), crash
//! before the original retry; restart. Expected: the recovery action is itself
//! recoverable, no third pending entry, the compensation is not unsafely repeated,
//! pending depth never exceeds two, and the final state satisfies op ; op' ; op ≈ op.

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

fn calls(dir: &PathBuf, file: &str, op: &str) -> usize {
    std::fs::read_to_string(dir.join(file))
        .map(|s| s.lines().filter(|l| *l == op).count())
        .unwrap_or(0)
}

fn check(dir: &PathBuf) -> String {
    let (code, stdout, stderr) = run(&["check", "--dir", dir.to_str().unwrap()]);
    assert_eq!(code, Some(0), "invariants: {stdout} {stderr}");
    stdout
}

#[test]
fn a2_crash_during_compensation_recovers_the_recovery() {
    let dir = fresh_dir("demo-a2-full");
    let d = dir.to_str().unwrap();

    // 1 · reserve commits remotely, process dies before the local Commit.
    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "run", "--crash",
        "Inventory.reserve:after_remote_before_receive",
    ]);
    assert_eq!(code, Some(87));
    let s = check(&dir);
    assert!(s.contains("PENDING 0"), "reserve pending: {s}");
    assert_eq!(inventory_events(&dir), vec!["RESERVE"]);

    // 2 · recovery selects the compensation; the compensation commits (remotely and in
    // the journal as the nested entry 0.0), then the process dies before the retry.
    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "recover", "--crash",
        "Inventory.release:after_commit",
    ]);
    assert_eq!(code, Some(87));
    let s = check(&dir);
    // The derived state sequence, read off the store: original still pending, its
    // recovery action a *child* entry, already done. No third entry, depth ≤ 2.
    assert!(s.contains("ENTRY 0 parent=- Inventory.reserve pending"), "{s}");
    assert!(s.contains("ENTRY 0.0 parent=0 Inventory.release done"), "{s}");
    assert!(s.contains("PENDING 0\n") || s.ends_with("PENDING 0"), "only the original pending: {s}");
    assert_eq!(inventory_events(&dir), vec!["RESERVE", "RELEASE"]);

    // 3 · restart: the driver resumes its own persisted continuation (the frame after
    // the compensation), retries reserve, and the program completes.
    let (code, stdout, stderr) = run(&["demo", "--which", "a2", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("done"), "{stdout}");

    // op ; op' ; op ≈ op — net state: reserved exactly once.
    assert_eq!(inventory_events(&dir), vec!["RESERVE", "RELEASE", "RESERVE"]);
    // The compensation was invoked exactly once — not unsafely repeated.
    assert_eq!(calls(&dir, "inventory.calls", "release"), 1);
    assert_eq!(calls(&dir, "inventory.calls", "reserve"), 2);
    // Downstream post still exactly once.
    let ledger = std::fs::read_to_string(dir.join("ledger.txt")).unwrap_or_default();
    assert_eq!(ledger.lines().filter(|l| l.starts_with("POST")).count(), 1, "{ledger}");

    let s = check(&dir);
    assert!(s.contains("FINAL yes"), "{s}");
    assert!(s.contains("ENTRY 0 parent=- Inventory.reserve done"), "{s}");
}

#[test]
fn a2_crash_during_compensation_invoke_window() {
    // Variant: the compensation itself dies inside ITS remote window (depth-2 pending),
    // then everything recovers. Exercises §8.1's middle row one level down.
    let dir = fresh_dir("demo-a2-compwindow");
    let d = dir.to_str().unwrap();

    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "run", "--crash",
        "Inventory.reserve:after_remote_before_receive",
    ]);
    assert_eq!(code, Some(87));

    let (code, _, _) = run(&[
        "demo", "--which", "a2", "--dir", d, "--mode", "recover", "--crash",
        "Inventory.release:after_remote_before_receive",
    ]);
    assert_eq!(code, Some(87));
    let s = check(&dir);
    // Both pending, parent-linked: the I1 stack at its maximum legal depth.
    assert!(s.contains("PENDING 0,0.0"), "depth-2 pending stack: {s}");

    let (code, stdout, stderr) = run(&["demo", "--which", "a2", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("done"), "{stdout}");
    // release deduplicated by its own idempotency (invoked twice, applied once).
    assert_eq!(calls(&dir, "inventory.calls", "release"), 2);
    assert_eq!(inventory_events(&dir), vec!["RESERVE", "RELEASE", "RESERVE"]);
    let s = check(&dir);
    assert!(s.contains("FINAL yes"), "{s}");
}
