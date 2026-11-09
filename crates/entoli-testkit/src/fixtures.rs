//! Fake remotes (plan §9). Each fixture records its own state in its own file,
//! independently of ENTOLI's store — the process may die between the fixture's commit
//! and ENTOLI's commit, which is precisely the window demonstrations A2/A3 distinguish.
//! Every fixture also counts invocations separately from state, so "was not re-invoked"
//! (A4) and "was re-invoked but deduplicated" (A2) are distinguishable claims.

use entoli_core::Value;
use entoli_runtime::{CrashInjector, CrashPoint, ExternalOp};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

fn append_line(path: &PathBuf, line: &str) {
    let mut f = OpenOptions::new().create(true).append(true).open(path).expect("fixture file");
    writeln!(f, "{line}").expect("fixture write");
    f.sync_all().expect("fixture fsync"); // the remote's own durable commit
}

pub fn read_lines(path: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
}

struct Files {
    state: PathBuf,
    calls: PathBuf,
}

impl Files {
    fn new(dir: &PathBuf, name: &str) -> Self {
        Files { state: dir.join(format!("{name}.txt")), calls: dir.join(format!("{name}.calls")) }
    }
    fn count_call(&self, op: &str) {
        append_line(&self.calls, op);
    }
}

/// Ledger: `post` is at-least-once — deduplicates by witness, like a real payment API.
pub struct FakeLedger {
    files: Files,
}

impl FakeLedger {
    pub fn new(dir: &PathBuf) -> Self {
        FakeLedger { files: Files::new(dir, "ledger") }
    }
    pub fn state_file(dir: &PathBuf) -> PathBuf {
        dir.join("ledger.txt")
    }
    pub fn calls_file(dir: &PathBuf) -> PathBuf {
        dir.join("ledger.calls")
    }
}

impl ExternalOp for FakeLedger {
    fn invoke(
        &mut self,
        op: &str,
        arg: &Value,
        witness: Option<&str>,
        crash: &CrashInjector,
    ) -> Result<Value, String> {
        if op != "post" {
            return Err(format!("ledger: unknown op {op}"));
        }
        self.files.count_call(op);
        let w = witness.ok_or("ledger.post requires a witness")?;
        let amount = match arg {
            Value::Ctor { args, .. } => args
                .iter()
                .find_map(|v| if let Value::Int(i) = v { Some(*i) } else { None })
                .unwrap_or(0),
            _ => 0,
        };
        let lines = read_lines(&self.files.state);
        // Dedup log: same witness ⇒ same receipt, no second entry [Helland].
        for line in &lines {
            let mut parts = line.split('\t');
            if let (Some("POST"), Some(lw), _, Some(r)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            {
                if lw == w {
                    return Ok(Value::Ctor {
                        name: "Receipt".into(),
                        args: vec![Value::Str(r.to_string())],
                    });
                }
            }
        }
        let receipt = format!("R{}", lines.iter().filter(|l| l.starts_with("POST")).count() + 1);
        append_line(&self.files.state, &format!("POST\t{w}\t{amount}\t{receipt}"));
        // The remote has committed; the local Commit has not happened yet.
        crash.hit("Ledger", "post", CrashPoint::AfterRemoteBeforeReceive);
        Ok(Value::Ctor { name: "Receipt".into(), args: vec![Value::Str(receipt)] })
    }
}

/// Approval service: `request` is at-least-once with a request-id witness — re-asking a
/// human on resume would yield a different answer or approval fatigue (plan §2), so the
/// service deduplicates and replays its recorded decision.
pub struct FakeApproval {
    files: Files,
}

impl FakeApproval {
    pub fn new(dir: &PathBuf) -> Self {
        FakeApproval { files: Files::new(dir, "approval") }
    }
    pub fn calls_file(dir: &PathBuf) -> PathBuf {
        dir.join("approval.calls")
    }
}

impl ExternalOp for FakeApproval {
    fn invoke(
        &mut self,
        op: &str,
        _arg: &Value,
        witness: Option<&str>,
        crash: &CrashInjector,
    ) -> Result<Value, String> {
        if op != "request" {
            return Err(format!("approval: unknown op {op}"));
        }
        self.files.count_call(op);
        let w = witness.ok_or("approval.request requires a request-id witness")?;
        for line in read_lines(&self.files.state) {
            let mut parts = line.split('\t');
            if let (Some("REQ"), Some(lw), Some(d)) = (parts.next(), parts.next(), parts.next()) {
                if lw == w {
                    return Ok(Value::Ctor { name: d.to_string(), args: vec![] });
                }
            }
        }
        let decision = "approve";
        append_line(&self.files.state, &format!("REQ\t{w}\t{decision}"));
        crash.hit("Approval", "request", CrashPoint::AfterRemoteBeforeReceive);
        Ok(Value::Ctor { name: decision.into(), args: vec![] })
    }
}

/// Inference service: `complete` is idempotent — the same prompt yields the same
/// completion (dedup by prompt), so re-invocation after a crash is safe.
pub struct FakeInfer {
    files: Files,
}

impl FakeInfer {
    pub fn new(dir: &PathBuf) -> Self {
        FakeInfer { files: Files::new(dir, "infer") }
    }
    pub fn calls_file(dir: &PathBuf) -> PathBuf {
        dir.join("infer.calls")
    }
}

impl ExternalOp for FakeInfer {
    fn invoke(
        &mut self,
        op: &str,
        arg: &Value,
        _witness: Option<&str>,
        crash: &CrashInjector,
    ) -> Result<Value, String> {
        if op != "complete" {
            return Err(format!("infer: unknown op {op}"));
        }
        self.files.count_call(op);
        let prompt = match arg {
            Value::Ctor { args, .. } => args
                .iter()
                .find_map(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                .unwrap_or_default(),
            Value::Str(s) => s.clone(),
            _ => return Err("infer: expected prompt".into()),
        };
        let completion = format!("done:{prompt}");
        let lines = read_lines(&self.files.state);
        if !lines.iter().any(|l| l == &format!("INFER\t{prompt}")) {
            append_line(&self.files.state, &format!("INFER\t{prompt}"));
        }
        crash.hit("Infer", "complete", CrashPoint::AfterRemoteBeforeReceive);
        Ok(Value::Ctor { name: "Completion".into(), args: vec![Value::Str(completion)] })
    }
}

/// Inventory: `reserve` is compensable by `release`; `release` is idempotent. The
/// fixture's contract is the §8.3 equation `op ; op' ; op ≈ op`, which the A2
/// demonstration exercises across two crashes.
pub struct FakeInventory {
    files: Files,
}

impl FakeInventory {
    pub fn new(dir: &PathBuf) -> Self {
        FakeInventory { files: Files::new(dir, "inventory") }
    }
    pub fn state_file(dir: &PathBuf) -> PathBuf {
        dir.join("inventory.txt")
    }
    pub fn calls_file(dir: &PathBuf) -> PathBuf {
        dir.join("inventory.calls")
    }

    fn net_reserved(lines: &[String], item: &str) -> bool {
        let mut reserved = false;
        for line in lines {
            let mut parts = line.split('\t');
            match (parts.next(), parts.next()) {
                (Some("RESERVE"), Some(i)) if i == item => reserved = true,
                (Some("RELEASE"), Some(i)) if i == item => reserved = false,
                _ => {}
            }
        }
        reserved
    }
}

impl ExternalOp for FakeInventory {
    fn invoke(
        &mut self,
        op: &str,
        arg: &Value,
        _witness: Option<&str>,
        crash: &CrashInjector,
    ) -> Result<Value, String> {
        let item = match arg {
            Value::Str(s) => s.clone(),
            Value::Ctor { args, .. } => args
                .iter()
                .find_map(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                .unwrap_or_default(),
            _ => return Err("inventory: expected item".into()),
        };
        self.files.count_call(op);
        let lines = read_lines(&self.files.state);
        match op {
            "reserve" => {
                append_line(&self.files.state, &format!("RESERVE\t{item}"));
                crash.hit("Inventory", "reserve", CrashPoint::AfterRemoteBeforeReceive);
                Ok(Value::Ctor { name: "Reserved".into(), args: vec![Value::Str(item)] })
            }
            "release" => {
                // Idempotent for real: releasing an unreserved item is a no-op.
                if Self::net_reserved(&lines, &item) {
                    append_line(&self.files.state, &format!("RELEASE\t{item}"));
                }
                crash.hit("Inventory", "release", CrashPoint::AfterRemoteBeforeReceive);
                Ok(Value::Ctor { name: "Released".into(), args: vec![Value::Str(item)] })
            }
            other => Err(format!("inventory: unknown op {other}")),
        }
    }
}
