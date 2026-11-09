//! PHASE 4 KILL POINT · Demonstration A (paper §13.1, plan §10.1): kill and resume
//! inside the window. Every crash run must end with **exactly one ledger entry**, each
//! restart from cold durable state only. The invocation counter distinguishes
//! "re-invoked and deduplicated" (A2) from "never re-invoked" (A4).

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

fn lines_starting(dir: &PathBuf, file: &str, prefix: &str) -> usize {
    std::fs::read_to_string(dir.join(file))
        .map(|s| s.lines().filter(|l| l.starts_with(prefix)).count())
        .unwrap_or(0)
}

fn count_lines(dir: &PathBuf, file: &str) -> usize {
    std::fs::read_to_string(dir.join(file)).map(|s| s.lines().count()).unwrap_or(0)
}

fn check_ok(dir: &PathBuf) -> String {
    let (code, stdout, stderr) = run(&["check", "--dir", dir.to_str().unwrap()]);
    assert_eq!(code, Some(0), "invariants must hold: {stdout} {stderr}");
    stdout
}

/// One crash scenario: run (dies at the injected point), then recover cold.
fn crash_scenario(name: &str, crash: &str, expected_post_calls: usize) {
    let dir = fresh_dir(name);
    let d = dir.to_str().unwrap();

    let (code, _, stderr) =
        run(&["demo", "--which", "a", "--dir", d, "--mode", "run", "--crash", crash]);
    assert_eq!(code, Some(87), "{name}: run must die hard at {crash}: {stderr}");
    check_ok(&dir); // invariants hold in the crashed state

    let (code, stdout, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{name}: recovery failed: {stderr}");
    assert!(stdout.contains("RESULT") && stdout.contains("posted"), "{name}: {stdout}");

    // Pass condition (§13.1): exactly one ledger entry, in every run.
    assert_eq!(lines_starting(&dir, "ledger.txt", "POST"), 1, "{name}: ledger entries");
    assert_eq!(count_lines(&dir, "ledger.calls"), expected_post_calls, "{name}: post invocations");
    // The human was asked exactly once across crash and recovery.
    assert_eq!(count_lines(&dir, "approval.calls"), 1, "{name}: approval invocations");

    let summary = check_ok(&dir);
    assert!(summary.contains("FINAL yes"), "{name}: final result recorded: {summary}");
    assert!(summary.contains("PENDING \n") || summary.contains("PENDING "), "{name}");
}

#[test]
fn control_run_without_crash() {
    let dir = fresh_dir("demo-a-control");
    let d = dir.to_str().unwrap();
    let (code, stdout, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "run"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("posted"), "{stdout}");
    assert_eq!(lines_starting(&dir, "ledger.txt", "POST"), 1);
    assert_eq!(count_lines(&dir, "ledger.calls"), 1);
    check_ok(&dir);
}

#[test]
fn a0_crash_before_any_intend() {
    // Empty journal: recovery restarts the program from the entry (§8.1 first row,
    // degenerate case — F-002).
    let dir = fresh_dir("demo-a0");
    let d = dir.to_str().unwrap();
    let (code, _, _) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "run", "--crash",
        "Approval.request:before_intend",
    ]);
    assert_eq!(code, Some(87));
    let (code, stdout, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("posted"));
    assert_eq!(lines_starting(&dir, "ledger.txt", "POST"), 1);
    assert_eq!(count_lines(&dir, "approval.calls"), 1);
    check_ok(&dir);
}

#[test]
fn a1_crash_before_intend_of_post() {
    // Nothing posted; the effect never reached the world. Restart posts once (the one
    // recovery invocation).
    crash_scenario("demo-a1", "Ledger.post:before_intend", 1);
}

#[test]
fn a3_crash_before_remote_commit() {
    // Intent durable, remote never contacted. Restart retries: one invocation, one entry.
    crash_scenario("demo-a3", "Ledger.post:after_intend", 1);
}

#[test]
fn a2_crash_after_remote_commit_before_receive() {
    // THE decisive case (README: "first decisive system test"): the remote committed,
    // the local Commit did not. Restart re-invokes under the same witness ι and the
    // remote deduplicates: two invocations, ONE entry.
    crash_scenario("demo-a2-window", "Ledger.post:after_remote_before_receive", 2);
}

#[test]
fn a2b_crash_after_receive_before_commit() {
    // Same store state as a2 (Pending), result died in volatile memory.
    crash_scenario("demo-a2b", "Ledger.post:after_receive_before_commit", 2);
}

#[test]
fn a4_crash_after_commit() {
    // Done(r) is durable: recovery resumes with r and the effect is NEVER re-invoked —
    // the invocation counter stays at 1.
    crash_scenario("demo-a4", "Ledger.post:after_commit", 1);
}

#[test]
fn a5_crash_before_resume() {
    // Between step 4 and step 5: same recovery action as a4.
    crash_scenario("demo-a5", "Ledger.post:before_resume", 1);
}

#[test]
fn repeated_crashes_converge() {
    // Crash the same site twice in a row (recovery crashing too), then recover clean.
    let dir = fresh_dir("demo-a-repeat");
    let d = dir.to_str().unwrap();
    let (code, _, _) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "run", "--crash",
        "Ledger.post:after_remote_before_receive",
    ]);
    assert_eq!(code, Some(87));
    // Recovery itself dies inside the window: an ordinary crash at an ordinary site.
    // (Not at after_remote_before_receive — on the re-invocation the remote
    // deduplicates and returns without a fresh commit, so that fixture-side point
    // cannot fire again; the receive→commit window is the one still open.)
    let (code, _, _) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "recover", "--crash",
        "Ledger.post:after_receive_before_commit",
    ]);
    assert_eq!(code, Some(87));
    check_ok(&dir);
    let (code, stdout, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("posted"));
    // Three invocations (original + two recoveries), still exactly one ledger entry.
    assert_eq!(count_lines(&dir, "ledger.calls"), 3);
    assert_eq!(lines_starting(&dir, "ledger.txt", "POST"), 1);
    check_ok(&dir);
}
