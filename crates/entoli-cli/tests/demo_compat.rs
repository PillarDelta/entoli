//! Phase 7 exit condition (plan §4): a sleeping continuation cannot silently resume
//! under a structurally or semantically incompatible handler. Semantic drift is a
//! deployer-declared compatibility token (§10.2); on mismatch the continuation is
//! quarantined — cleanly refused, not lost.

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

#[test]
fn compat_token_mismatch_quarantines_instead_of_resuming() {
    let dir = fresh_dir("demo-compat");
    let d = dir.to_str().unwrap();

    // Sleep inside the ledger window (captured under the default token "v1").
    let (code, _, _) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "run", "--crash",
        "Ledger.post:after_intend",
    ]);
    assert_eq!(code, Some(87));

    // The deployer bumps Ledger's compatibility token: the observable contract the
    // continuation depends on has changed. Resume must refuse and quarantine.
    let (code, _, stderr) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "recover", "--compat",
        "Ledger=2026-09",
    ]);
    assert_eq!(code, Some(1), "must not silently resume");
    assert!(stderr.contains("E013") && stderr.contains("Ledger"), "{stderr}");
    assert!(stderr.contains("quarantined"), "{stderr}");
    // Nothing was invoked under the changed contract.
    assert!(!dir.join("ledger.calls").exists() ||
        std::fs::read_to_string(dir.join("ledger.calls")).unwrap().lines().count() == 0);

    // The quarantine is durable and sticky: even the original token cannot silently
    // resume it now — stranding is a migration event with a hook, not a retry loop.
    let (code, _, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("quarantined"), "{stderr}");

    // The store still holds everything (nothing lost), and says so.
    let (code, stdout, _) = run(&["check", "--dir", d]);
    assert_eq!(code, Some(0), "invariants hold under quarantine: {stdout}");
    assert!(stdout.contains("quarantined=[E013"), "{stdout}");
    assert!(stdout.contains("PENDING 1"), "the entry remains, awaiting migration: {stdout}");
}

#[test]
fn matching_token_resumes_normally() {
    let dir = fresh_dir("demo-compat-ok");
    let d = dir.to_str().unwrap();
    let (code, _, _) = run(&[
        "demo", "--which", "a", "--dir", d, "--mode", "run", "--crash",
        "Ledger.post:after_intend",
    ]);
    assert_eq!(code, Some(87));
    // Same declared token on both sides of the sleep: ordinary recovery.
    let (code, stdout, stderr) = run(&["demo", "--which", "a", "--dir", d, "--mode", "recover"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("posted"), "{stdout}");
}
