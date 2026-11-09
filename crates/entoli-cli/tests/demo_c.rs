//! Demonstration C (paper §13.3, plan §10.4): authority at rest.
//!
//! Suspend an agent holding a budgeted inference capability and a confidential
//! customer record. Verify: no capability/grant/budget material in the persisted
//! continuation; the derived data classification records what IS there (per §9.2 the
//! test must NOT claim the store holds no secrets); a revoked grant refuses resume
//! naming the missing capability; a re-grant binds the resumed computation to the new,
//! smaller figure.

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

fn infer_calls(dir: &PathBuf) -> usize {
    std::fs::read_to_string(dir.join("infer.calls")).map(|s| s.lines().count()).unwrap_or(0)
}

#[test]
fn demo_c_authority_at_rest() {
    let dir = fresh_dir("demo-c");
    let d = dir.to_str().unwrap();

    // 1 · Run under B1 = 100 until the approval suspension, then die (the injected
    // crash stands in for days of sleep).
    let (code, _, stderr) = run(&[
        "demo", "--which", "c", "--dir", d, "--mode", "run",
        "--grant", "Infer=100", "--grant", "Approval",
        "--crash", "Approval.request:after_intend",
    ]);
    assert_eq!(code, Some(87), "{stderr}");
    assert_eq!(infer_calls(&dir), 1, "one inference before sleeping");

    // 2 · Inspect the store: the sleeping continuation (k1, for the approval entry).
    let (code, stdout, _) = run(&["check", "--dir", d, "--dump", "k1"]);
    assert_eq!(code, Some(0));
    // §9.2: record the derived classification — the customer record IS in the frames,
    // so the store is confidential. (The withdrawn "no secrets" claim must not sneak
    // back in as an assertion of publicness.)
    assert!(stdout.contains("KONT k1 class=confidential"), "{stdout}");

    let hex = stdout
        .lines()
        .find(|l| l.starts_with("KONTBYTES "))
        .expect("dump")
        .trim_start_matches("KONTBYTES ")
        .to_string();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let as_string = String::from_utf8_lossy(&bytes).into_owned();
    // No capability material: the capability appears as its effect name only.
    assert!(as_string.contains("Infer"), "the slot names the effect");
    // No budget figure (B1 = 100 as a little-endian i64), no grant-table ids beyond
    // the effect name, no witness-independent credentials.
    let b1 = 100i64.to_le_bytes();
    assert!(
        !bytes.windows(8).any(|w| w == b1),
        "the budget figure must not cross the durable boundary"
    );
    assert!(!as_string.contains("budget"), "no policy material");
    // What non-authority data it DOES contain is recorded, not denied: the record.
    assert!(as_string.contains("acme-042"), "the confidential record is (permittedly) there");

    // 3 · Revoke: recover with Approval granted but no Infer grant. Resume must be
    // refused, naming the missing capability (E014), before any external effect runs.
    let (code, _, stderr) =
        run(&["demo", "--which", "c", "--dir", d, "--mode", "recover", "--grant", "Approval"]);
    assert_eq!(code, Some(1), "revoked grant must refuse resume");
    assert!(
        stderr.contains("MissingResumeGrant") && stderr.contains("Infer"),
        "refusal names the capability: {stderr}"
    );
    assert_eq!(infer_calls(&dir), 1, "no inference ran under revocation");

    // 4 · Re-grant with B2 = 1 < B1: the resumed path needs two inferences; the agent
    // is bound by the NEW figure and stops at the second.
    let (code, _, stderr) = run(&[
        "demo", "--which", "c", "--dir", d, "--mode", "recover",
        "--grant", "Infer=1", "--grant", "Approval",
    ]);
    assert_eq!(code, Some(1), "budget 1 cannot finish two inferences: {stderr}");
    assert!(stderr.contains("budget exhausted for Infer"), "{stderr}");
    assert_eq!(infer_calls(&dir), 2, "exactly one inference ran under B2=1");

    // 5 · Re-grant with enough budget: completes. The p2 inference is NOT re-run (its
    // Done is durable); only p3 executes. Three fixture calls total across all runs.
    let (code, stdout, stderr) = run(&[
        "demo", "--which", "c", "--dir", d, "--mode", "recover",
        "--grant", "Infer=5", "--grant", "Approval",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Done"), "{stdout}");
    assert_eq!(infer_calls(&dir), 3, "durability also protects the budget");

    let (code, stdout, _) = run(&["check", "--dir", d]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("FINAL yes"), "{stdout}");
}
