//! T5.1 pilot: the approval-gated brief writer. The human gate leaves the
//! journal entry durably Pending across process exits; `approve` + `recover`
//! completes with exactly-once publication. The LLM binding is substituted by
//! echo here (its witness-cache dedup logic needs no network to be honest);
//! the live LLM and webhook runs are in docs/pilot-runbook.md.

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

fn pilot_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../pilot/brief.ent")
        .to_str()
        .unwrap()
        .to_string()
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(bin()).args(args).output().expect("spawn");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn std_args<'a>(file: &'a str, dir: &'a str) -> Vec<&'a str> {
    vec![
        file, "--dir", dir,
        "--grant", "Infer", "--grant", "Approval", "--grant", "Publish",
        "--bind", "llm_svc=echo:Infer.complete",
        "--bind", "approval_svc=approve:Approval.request",
        "--bind", "publish_svc=ledger:Publish.post",
    ]
}

const GATE: &str = "gate:durable execution for AI agents";

fn ledger_lines(dir: &PathBuf) -> usize {
    std::fs::read_to_string(dir.join("ledger.txt")).map(|s| s.lines().count()).unwrap_or(0)
}

#[test]
fn pilot_suspends_on_the_human_gate_and_publishes_once_after_approval() {
    let dir = fresh_dir("pilot-approve");
    let file = pilot_path();
    let d = dir.to_str().unwrap();

    // Run: the draft happens, the gate is pending, the process exits.
    let mut a = vec!["run"];
    a.extend(std_args(&file, d));
    let (code, _, stderr) = run(&a);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("pending human approval"), "{stderr}");
    assert_eq!(ledger_lines(&dir), 0, "nothing published before approval");

    // Hours later: a human decides.
    let (code, _, _) =
        run(&["approve", "--dir", d, "--witness", GATE, "--decision", "approve"]);
    assert_eq!(code, Some(0));

    // Recover cold: completes, publishes exactly once.
    let mut a = vec!["recover"];
    a.extend(std_args(&file, d));
    let (code, stdout, stderr) = run(&a);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("Published"), "{stdout}");
    assert_eq!(ledger_lines(&dir), 1);

    // A second recover after completion returns the final result and
    // publishes nothing further.
    let mut a = vec!["recover"];
    a.extend(std_args(&file, d));
    let (code, stdout, _) = run(&a);
    assert_eq!(code, Some(0), "{stdout}");
    assert_eq!(ledger_lines(&dir), 1, "exactly-once across repeated recovery");
}

#[test]
fn pilot_reject_path_halts_without_publishing() {
    let dir = fresh_dir("pilot-reject");
    let file = pilot_path();
    let d = dir.to_str().unwrap();
    let mut a = vec!["run"];
    a.extend(std_args(&file, d));
    let (code, _, _) = run(&a);
    assert_eq!(code, Some(1));
    run(&["approve", "--dir", d, "--witness", GATE, "--decision", "reject"]);
    let mut a = vec!["recover"];
    a.extend(std_args(&file, d));
    let (code, stdout, stderr) = run(&a);
    assert_eq!(code, Some(0), "{stdout} {stderr}");
    assert!(stdout.contains("Halted"), "{stdout}");
    assert_eq!(ledger_lines(&dir), 0, "a rejected brief is never published");
}
