//! Phase 7: the delivery-contract conformance harness (paper §8.3, plan §11.3).
//!
//! The type system does not prove remote semantic properties; declaring a class is a
//! claim about a foreign system. The harness makes each claim executable against a
//! sandbox:
//!
//! ```text
//! idempotent          op ; op                 ≈ op
//! at_least_once(ι)    op(ι) ; op(ι)           ≈ op(ι)
//! compensable(op')    op^a ; op' ; op^b       ≈ op   for a in {0,1}, b >= 1
//! ```
//!
//! The compensable schedules are the machine's actual recovery schedules (P-3,
//! then corrected per F-012: as free equations the family entails idempotence,
//! so the DECLARED contract is now state-indexed — compensating a non-delivery
//! is a no-op; the compensation undoes one delivery). The harness exercises the
//! schedules those conditions must survive: a=1,b=1 (undo then retry), a=0,b=1
//! (compensate a non-delivery), and a=1,b=2 — the crash-mid-retry bare
//! re-delivery, which probes the OPEN F-012 machine gap: today's dispatch
//! additionally requires the op to tolerate re-delivery on the retry path.
//!
//! "≈" compares the remote's *observable state* under a per-service projection (a
//! ledger's entries, an inventory's net reservations) plus result equality. This is
//! testing, not proof — a handler can pass on a sandbox and fail in production — and
//! the distinction belongs here as much as in the docs.

use crate::fixtures;
use entoli_core::Value;
use entoli_runtime::{CrashInjector, ExternalOp};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ContractReport {
    pub effect: String,
    pub op: String,
    pub class: String,
    pub passed: bool,
    pub detail: String,
}

impl std::fmt::Display for ContractReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CONTRACT {}.{} {} {}{}",
            self.effect,
            self.op,
            self.class,
            if self.passed { "PASSED" } else { "FAILED" },
            if self.detail.is_empty() { String::new() } else { format!(" ({})", self.detail) }
        )
    }
}

fn fresh(root: &PathBuf, name: &str) -> PathBuf {
    let d = root.join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sorted_state(dir: &PathBuf, file: &str) -> Vec<String> {
    let mut v = fixtures::read_lines(&dir.join(file));
    v.sort();
    v
}

type MkFixture = Box<dyn Fn(&PathBuf) -> Box<dyn ExternalOp>>;
type Projection = Box<dyn Fn(&PathBuf) -> Vec<String>>;

/// idempotent: op ; op ≈ op (same argument, no witness).
pub fn check_idempotent(
    root: &PathBuf,
    label: &str,
    mk: MkFixture,
    effect: &str,
    op: &str,
    arg: Value,
    observe: Projection,
) -> ContractReport {
    let crash = CrashInjector::default();
    let d1 = fresh(root, &format!("{label}-once"));
    let d2 = fresh(root, &format!("{label}-twice"));
    let mut once = mk(&d1);
    let r1 = once.invoke(op, &arg, None, &crash);
    let mut twice = mk(&d2);
    let r2a = twice.invoke(op, &arg, None, &crash);
    let r2b = twice.invoke(op, &arg, None, &crash);
    let passed = r1.is_ok()
        && r2a.is_ok()
        && r2b.is_ok()
        && r2a == r2b
        && observe(&d1) == observe(&d2);
    ContractReport {
        effect: effect.into(),
        op: op.into(),
        class: "idempotent".into(),
        passed,
        detail: if passed {
            String::new()
        } else {
            format!("state once={:?} twice={:?}", observe(&d1), observe(&d2))
        },
    }
}

/// at_least_once(ι): op(ι) ; op(ι) ≈ op(ι).
pub fn check_at_least_once(
    root: &PathBuf,
    label: &str,
    mk: MkFixture,
    effect: &str,
    op: &str,
    arg: Value,
    observe: Projection,
) -> ContractReport {
    let crash = CrashInjector::default();
    let d1 = fresh(root, &format!("{label}-once"));
    let d2 = fresh(root, &format!("{label}-twice"));
    let mut once = mk(&d1);
    let r1 = once.invoke(op, &arg, Some("wit-1"), &crash);
    let mut twice = mk(&d2);
    let r2a = twice.invoke(op, &arg, Some("wit-1"), &crash);
    let r2b = twice.invoke(op, &arg, Some("wit-1"), &crash);
    let passed = r1.is_ok()
        && r2a.is_ok()
        && r2b.is_ok()
        && r2a == r2b
        && observe(&d1) == observe(&d2);
    ContractReport {
        effect: effect.into(),
        op: op.into(),
        class: "at_least_once".into(),
        passed,
        detail: if passed {
            String::new()
        } else {
            format!("state once={:?} twice={:?}", observe(&d1), observe(&d2))
        },
    }
}

/// compensable(op'): op^a ; op' ; op^b ≈ op for a in {0,1}, b >= 1 (paper v0.6
/// §8.3; P-3). Exercised at (a,b) = (1,1), (0,1), (1,2).
#[allow(clippy::too_many_arguments)]
pub fn check_compensable(
    root: &PathBuf,
    label: &str,
    mk: MkFixture,
    effect: &str,
    op: &str,
    comp_op: &str,
    arg: Value,
    observe: Projection,
) -> ContractReport {
    let crash = CrashInjector::default();
    let d1 = fresh(root, &format!("{label}-op"));
    let mut plain = mk(&d1);
    let r1 = plain.invoke(op, &arg, None, &crash);
    let reference = observe(&d1);
    let mut failed: Vec<String> = Vec::new();
    if r1.is_err() {
        failed.push("op itself failed".into());
    }
    for (a, b) in [(1usize, 1usize), (0, 1), (1, 2)] {
        let d = fresh(root, &format!("{label}-a{a}b{b}"));
        let mut seq = mk(&d);
        let mut ok = true;
        for _ in 0..a {
            ok &= seq.invoke(op, &arg, None, &crash).is_ok();
        }
        ok &= seq.invoke(comp_op, &arg, None, &crash).is_ok();
        for _ in 0..b {
            ok &= seq.invoke(op, &arg, None, &crash).is_ok();
        }
        if !ok || observe(&d) != reference {
            failed.push(format!(
                "op^{a};{comp_op};op^{b}: net {:?} (expected {reference:?})",
                observe(&d)
            ));
        }
    }
    let passed = failed.is_empty();
    ContractReport {
        effect: effect.into(),
        op: op.into(),
        class: format!("compensable({comp_op})"),
        passed,
        detail: if passed { String::new() } else { failed.join("; ") },
    }
}

/// Inventory's observable state: the NET reservation set (order-insensitive), not the
/// event log — op ; op' ; op leaves a longer log but the same world.
pub fn inventory_net(dir: &PathBuf) -> Vec<String> {
    let mut reserved = std::collections::BTreeSet::new();
    for line in fixtures::read_lines(&dir.join("inventory.txt")) {
        let mut parts = line.split('\t');
        match (parts.next(), parts.next()) {
            (Some("RESERVE"), Some(i)) => {
                reserved.insert(i.to_string());
            }
            (Some("RELEASE"), Some(i)) => {
                reserved.remove(i);
            }
            _ => {}
        }
    }
    reserved.into_iter().collect()
}

/// Run every declared contract of the demonstration fixtures.
pub fn run_all(root: &PathBuf) -> Vec<ContractReport> {
    let entry = Value::Ctor {
        name: "Entry".into(),
        args: vec![Value::Str("x".into()), Value::Int(9)],
    };
    vec![
        check_at_least_once(
            root,
            "ledger-post",
            Box::new(|d| Box::new(fixtures::FakeLedger::new(d))),
            "Ledger",
            "post",
            entry.clone(),
            Box::new(|d| sorted_state(d, "ledger.txt")),
        ),
        check_at_least_once(
            root,
            "approval-request",
            Box::new(|d| Box::new(fixtures::FakeApproval::new(d))),
            "Approval",
            "request",
            entry.clone(),
            Box::new(|d| sorted_state(d, "approval.txt")),
        ),
        check_idempotent(
            root,
            "infer-complete",
            Box::new(|d| Box::new(fixtures::FakeInfer::new(d))),
            "Infer",
            "complete",
            Value::Ctor { name: "Prompt".into(), args: vec![Value::Str("p".into())] },
            Box::new(|d| sorted_state(d, "infer.txt")),
        ),
        check_idempotent(
            root,
            "inventory-release",
            Box::new(|d| {
                // Idempotency of release is checked from a reserved state.
                let mut f = fixtures::FakeInventory::new(d);
                let _ = f.invoke(
                    "reserve",
                    &Value::Str("x".into()),
                    None,
                    &CrashInjector::default(),
                );
                Box::new(f)
            }),
            "Inventory",
            "release",
            Value::Str("x".into()),
            Box::new(inventory_net),
        ),
        check_compensable(
            root,
            "inventory-reserve",
            Box::new(|d| Box::new(fixtures::FakeInventory::new(d))),
            "Inventory",
            "reserve",
            "release",
            Value::Str("x".into()),
            Box::new(inventory_net),
        ),
    ]
}

/// A compensation that misbehaves against a clean state: releasing an unreserved
/// item leaves a visible artifact behind. It passes the v0.5 equation
/// (op ; op' ; op ≈ op) exactly — the old harness approved this shape — and fails
/// the a = 0 instance of the v0.6 family, which is the sequence the ambiguous
/// Pending crash actually realizes (P-3: op may never have been delivered when
/// the compensation fires).
pub struct OverzealousCompensation {
    dir: PathBuf,
}

impl OverzealousCompensation {
    pub fn new(dir: &PathBuf) -> Self {
        OverzealousCompensation { dir: dir.clone() }
    }

    fn append(&self, line: &str) {
        use std::io::Write;
        std::fs::create_dir_all(&self.dir).ok();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("inventory.txt"))
            .unwrap();
        f.write_all(format!("{line}\n").as_bytes()).unwrap();
    }
}

impl ExternalOp for OverzealousCompensation {
    fn invoke(
        &mut self,
        op: &str,
        arg: &Value,
        _witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<Value, String> {
        let item = match arg {
            Value::Str(s) => s.clone(),
            _ => return Err("expected item".into()),
        };
        match op {
            "reserve" => {
                self.append(&format!("RESERVE\t{item}"));
                Ok(Value::Ctor { name: "Reserved".into(), args: vec![Value::Str(item)] })
            }
            "release" => {
                if inventory_net(&self.dir).contains(&item) {
                    self.append(&format!("RELEASE\t{item}"));
                } else {
                    // The misbehaviour: compensating nothing leaves a mark.
                    self.append("RESERVE\tcomp-artifact");
                }
                Ok(Value::Ctor { name: "Released".into(), args: vec![Value::Str(item)] })
            }
            other => Err(format!("unknown op {other}")),
        }
    }
}

/// A service that CLAIMS at-least-once but does not deduplicate — the §8.3 situation
/// the harness exists to catch before deployment.
pub struct LyingLedger {
    dir: PathBuf,
}

impl LyingLedger {
    pub fn new(dir: &PathBuf) -> Self {
        LyingLedger { dir: dir.clone() }
    }
}

impl ExternalOp for LyingLedger {
    fn invoke(
        &mut self,
        _op: &str,
        _arg: &Value,
        witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<Value, String> {
        // Appends unconditionally: same witness, second entry. A false claim.
        let n = fixtures::read_lines(&self.dir.join("ledger.txt")).len();
        std::fs::create_dir_all(&self.dir).ok();
        let line = format!("POST\t{}\t0\tR{}\n", witness.unwrap_or("-"), n + 1);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("ledger.txt"))
            .map_err(|e| e.to_string())?;
        f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Value::Ctor { name: "Receipt".into(), args: vec![Value::Str(format!("R{}", n + 1))] })
    }
}
