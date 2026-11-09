//! PHASE 2 KILL POINT (plan §5.1): build a computation, suspend it, encode, kill the
//! process, decode and resume in a fresh process — no durability machinery, no store,
//! no external effects. If this fails structurally, stop and revise the semantics.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_entoli-cli")
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn continuation_survives_process_death_and_resumes_cold() {
    let file = tmp("phase2.kont");
    let _ = std::fs::remove_file(&file);

    // Process 1: run to suspension, persist the continuation, die hard (exit 87 — no
    // destructors, nothing volatile survives).
    let out1 = Command::new(bin())
        .args(["phase2-encode", file.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert_eq!(out1.status.code(), Some(87), "encoder must die hard: {out1:?}");
    let blob = std::fs::read(&file).expect("continuation file exists");
    assert!(!blob.is_empty());

    // Process 2: a fresh process, durable bytes only. fib(10)=55; resume with 100:
    // (100 + 55) + 55 = 210.
    let out2 = Command::new(bin())
        .args(["phase2-resume", file.to_str().unwrap(), "100"])
        .output()
        .expect("spawn");
    assert!(out2.status.success(), "resume failed: {out2:?}");
    let stdout = String::from_utf8_lossy(&out2.stdout);
    assert_eq!(stdout.trim(), "RESULT 210");

    // Different resume value, same durable bytes — the continuation is a real function
    // of its argument, not a recorded outcome.
    let out3 = Command::new(bin())
        .args(["phase2-resume", file.to_str().unwrap(), "7"])
        .output()
        .expect("spawn");
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    assert_eq!(stdout3.trim(), "RESULT 117");
}
