//! Subprocess entry points for the kill tests (plan §9): crash tests use hard process
//! death, never in-process exceptions, so volatile state cannot silently carry across.
//! Every run restarts from durable state only.

use entoli_core::{Machine, MapAuthority, Outcome, Value};
use entoli_encode::{Codec, MapGranter};
use entoli_runtime::{CrashInjector, Runtime};
use entoli_store::{assert_store_invariants, DurableStore, SqliteStore};
use entoli_syntax::{Program, Term};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// Hard death: no Rust destructors, no flushes beyond what is already durable.
const KILLED: i32 = 87;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    match cmd {
        "phase2-encode" => phase2_encode(&args),
        "phase2-resume" => phase2_resume(&args),
        "demo" => demo(&args),
        "run" => run_source(&args, false),
        "approve" => approve(&args),
        "recover" => run_source(&args, true),
        "check" => check(&args),
        "compile" => compile(&args, false),
        "compile-ablated" => compile(&args, true),
        "contracts" => contracts(&args),
        other => {
            eprintln!("unknown command {other:?}");
            std::process::exit(2);
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn flags_all(args: &[String], name: &str) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| *a == name)
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect()
}

fn build_runtime(dir: &PathBuf, program: &Program, crash: Option<String>) -> Runtime {
    std::fs::create_dir_all(dir).expect("dir");
    let store = SqliteStore::open(&dir.join("store.sqlite")).expect("store");
    let mut rt = Runtime::new(program, &entoli_types::Signatures::default(), store);
    if let Some(spec) = crash {
        rt.crash = CrashInjector::parse(&spec).expect("crash spec: E.op:point");
    }
    rt
}

fn apply_compat(rt: &mut Runtime, args: &[String]) {
    for spec in flags_all(args, "--compat") {
        let (effect, token) = spec.split_once('=').expect("--compat Effect=token");
        rt.compat.insert(effect.to_string(), token.to_string());
    }
}

fn pick_program(name: &str) -> (Program, Term) {
    match name {
        "a" => entoli_testkit::programs::settle_unified(),
        "a2" => entoli_testkit::programs::settle2_unified(),
        "c" => entoli_testkit::programs::agent_unified(),
        "sneaky" => entoli_testkit::programs::sneaky_unified(),
        other => panic!("unknown demo {other:?}"),
    }
}

/// The recovery driver runs under the same prelude the program used (unification §6:
/// the installation in force at recovery), so `Protocol.complete` finds the clause and
/// a fresh start (empty journal) re-enters the wrapped program.
fn wrap_driver(which: &str, driver: Term, rt: &Runtime) -> Term {
    match which {
        "a" => entoli_testkit::programs::settle_wrap(driver),
        "a2" => entoli_testkit::programs::settle2_wrap(driver),
        "c" => entoli_testkit::programs::agent_wrap_with(
            rt.grants.live_for("Infer").is_some(),
            rt.grants.live_for("Approval").is_some(),
            driver,
        ),
        "sneaky" => entoli_testkit::programs::sneaky_wrap(driver),
        _ => driver,
    }
}

/// Grants + cap-variable bindings + foreign bindings per unified demo.
fn demo_env(which: &str, rt: &mut Runtime, dir: &PathBuf, cli: &[String]) -> entoli_core::Env {
    use entoli_testkit::fixtures as fx;
    let mut env = entoli_core::Env::empty();
    match which {
        "a" => {
            rt.bind_foreign("approval_svc", "Approval", "request", Box::new(fx::FakeApproval::new(dir)));
            rt.bind_foreign("ledger_svc", "Ledger", "post", Box::new(fx::FakeLedger::new(dir)));
            let ca = rt.grants.grant("Approval", None);
            let cl = rt.grants.grant("Ledger", None);
            env = env
                .bind("cap_approval", entoli_core::Value::Cap(ca))
                .bind("cap_ledger", entoli_core::Value::Cap(cl));
        }
        "a2" => {
            rt.bind_foreign("inventory_reserve", "Inventory", "reserve", Box::new(fx::FakeInventory::new(dir)));
            rt.bind_foreign("inventory_release", "Inventory", "release", Box::new(fx::FakeInventory::new(dir)));
            rt.bind_foreign("ledger_svc", "Ledger", "post", Box::new(fx::FakeLedger::new(dir)));
            let ci = rt.grants.grant("Inventory", None);
            let cl = rt.grants.grant("Ledger", None);
            env = env
                .bind("cap_inventory", entoli_core::Value::Cap(ci))
                .bind("cap_ledger", entoli_core::Value::Cap(cl));
        }
        "c" => {
            rt.bind_foreign("infer_svc", "Infer", "complete", Box::new(fx::FakeInfer::new(dir)));
            rt.bind_foreign("approval_svc", "Approval", "request", Box::new(fx::FakeApproval::new(dir)));
            // Demo C's grants come from the command line — the authority in force for
            // THIS process run (Rule 4). No --grant Infer = revoked.
            for g in flags_all(cli, "--grant") {
                let (effect, budget) = match g.split_once('=') {
                    Some((e, n)) => (e.to_string(), Some(n.parse::<i64>().expect("budget"))),
                    None => (g.clone(), None),
                };
                rt.grants.grant(&effect, budget);
            }
            if let Some(cap) = rt.grants.live_for("Infer") {
                env = env
                    .bind("infer_cap", entoli_core::Value::Cap(cap))
                    .bind("cap_infer", entoli_core::Value::Cap(cap));
            }
            if let Some(cap) = rt.grants.live_for("Approval") {
                env = env.bind("cap_approval", entoli_core::Value::Cap(cap));
            }
        }
        "sneaky" => {
            rt.bind_foreign("ledger_svc", "Ledger", "post", Box::new(fx::FakeLedger::new(dir)));
            let cl = rt.grants.grant("Ledger", None);
            env = env.bind("cap_ledger", entoli_core::Value::Cap(cl));
        }
        _ => {}
    }
    env
}

/// `demo --which a|a2|c|sneaky --dir D [--crash E.op:point] [--grant E[=N]]* --mode run|recover`
fn demo(args: &[String]) {
    let which = flag(args, "--which").expect("--which");
    let dir = PathBuf::from(flag(args, "--dir").expect("--dir"));
    let mode = flag(args, "--mode").expect("--mode");
    let (program, entry) = pick_program(&which);
    program.validate().expect("program validates");
    let mut rt = build_runtime(&dir, &program, flag(args, "--crash"));
    apply_compat(&mut rt, args);
    let env = demo_env(&which, &mut rt, &dir, args);

    let result = match mode.as_str() {
        "run" => rt.run_program_env(Rc::new(entry), env),
        "recover" => {
            let driver = wrap_driver(&which, entoli_recovery::driver_for(&program, &entry), &rt);
            rt.recover_env(Rc::new(driver), env)
        }
        other => panic!("unknown mode {other:?}"),
    };
    match result {
        Ok(v) => println!("RESULT {v:?}"),
        Err(e) => {
            eprintln!("ERROR {e}");
            std::process::exit(1);
        }
    }
}

/// `check --dir D` — invariant checker + store summary, for test assertions.
fn check(args: &[String]) {
    let dir = PathBuf::from(flag(args, "--dir").expect("--dir"));
    let store = SqliteStore::open(&dir.join("store.sqlite")).expect("store");
    match assert_store_invariants(&store) {
        Ok(()) => println!("INVARIANTS ok"),
        Err(v) => {
            println!("INVARIANTS violated: {v}");
            std::process::exit(1);
        }
    }
    let entries = store.entries().expect("entries");
    println!("ENTRIES {}", entries.len());
    for e in &entries {
        let state = match &e.state {
            entoli_store::EntryState::Pending => "pending".to_string(),
            entoli_store::EntryState::Done(_) => "done".to_string(),
        };
        println!(
            "ENTRY {} parent={} {}.{} {} witness={}",
            e.journal_id,
            e.parent_journal_id.as_deref().unwrap_or("-"),
            e.effect,
            e.op,
            state,
            e.witness.as_deref().unwrap_or("-")
        );
    }
    let pending = store.pending_stack().expect("pending");
    println!(
        "PENDING {}",
        pending.iter().map(|e| e.journal_id.as_str()).collect::<Vec<_>>().join(",")
    );
    for e in &entries {
        if let Some(k) = store.continuation(&e.continuation_id).expect("kont") {
            let q = store
                .quarantine_of(&k.continuation_id)
                .expect("quarantine")
                .map(|r| format!(" quarantined=[{r}]"))
                .unwrap_or_default();
            println!(
                "KONT {} class={} bytes={}{}",
                k.continuation_id,
                k.data_classification,
                k.encoded_frames.len(),
                q
            );
            // Demo C's inspection: dump the raw persisted continuation for auditing.
            if flag(args, "--dump").as_deref() == Some(k.continuation_id.as_str()) {
                let hex: String =
                    k.encoded_frames.iter().map(|b| format!("{b:02x}")).collect();
                println!("KONTBYTES {hex}");
            }
        }
    }
    println!("FINAL {}", if store.final_result().expect("final").is_some() { "yes" } else { "no" });
}

/// `compile <file.ent> [--ablate affine,delivery,tail,susp]` — parse, validate, and
/// statically check a surface-syntax program. Exit 0 with "OK" or 1 with diagnostics.
fn compile(args: &[String], allow_ablation: bool) {
    let file = args.get(2).expect("usage: compile <file.ent>");
    let src = std::fs::read_to_string(file).expect("read source");
    let mut flags = entoli_check::CheckFlags::default();
    // A-30: ablation switches exist for the plan §10.5 measurement only and
    // have no place in the production gate — `compile` refuses them.
    if !allow_ablation && flag(args, "--ablate").is_some() {
        println!("DIAG ablation flags are measurement-only: use `compile-ablated`");
        std::process::exit(1);
    }
    if let Some(list) = flag(args, "--ablate") {
        for a in list.split(',') {
            match a {
                "affine" => flags.affine = false,
                "delivery" => flags.delivery = false,
                "tail" => flags.tail = false,
                "susp" => flags.nested_suspension = false,
                other => panic!("unknown ablation {other:?}"),
            }
        }
    }
    let (program, sigs) = match entoli_check::parse::parse_program(&src) {
        Ok(x) => x,
        Err(e) => {
            println!("PARSE-ERROR {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = program.validate() {
        println!("DIAG {e}");
        std::process::exit(1);
    }
    let mut checker = entoli_check::Checker::with_flags(&program, &sigs, flags);
    checker.check_defs();
    if !checker.diags.is_empty() {
        for d in &checker.diags {
            println!("DIAG {d}");
        }
        std::process::exit(1);
    }
    // T1.3: the whole-program gate. A runnable program defines `main`, and every
    // arrow row in main's declared type must be empty — the falsification
    // criterion's first clause: nothing escapes undischarged. Files without a
    // `main` are libraries and pass on the per-def checks above.
    match sigs.defs.get("main") {
        Some(ty) => {
            fn rows_empty(t: &entoli_types::Type) -> bool {
                match t {
                    entoli_types::Type::Fn(a, row, b) => {
                        row.0.is_empty() && rows_empty(a) && rows_empty(b)
                    }
                    entoli_types::Type::Kont(a, _, b) => rows_empty(a) && rows_empty(b),
                    _ => true,
                }
            }
            if rows_empty(ty) {
                println!("OK (entry: main, all rows discharged)");
            } else {
                println!(
                    "DIAG E001: main's type {ty} carries an undischarged effect row — \
                     every effect must be handled before the entry point"
                );
                std::process::exit(1);
            }
        }
        None => println!("OK (library: per-definition checks only, no `main`)"),
    }
}

/// `run <file.ent> --dir D [--grant E[=N]]... [--bind f=KIND:E.op]... [--crash SPEC]`
/// and `recover` with the same flags. Capability convention: each `--grant E`
/// binds the variable `cap_<e>` (lowercased) of type Cap[E]; `main : Unit -[]-> _`
/// is the entry; a program that wants recoverable compensations defines
/// `def wrap : Cap[..] -[]-> .. ((Unit -[E..]-> Data) -[]-> Data)` and `recover`
/// re-installs its prelude around the generated driver.
fn run_source(args: &[String], recover: bool) {
    let file = args.get(2).unwrap_or_else(|| {
        eprintln!("usage: run|recover <file.ent> --dir D [--grant E[=N]]... [--bind f=KIND:E.op]...");
        std::process::exit(2);
    });
    let src = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("ERROR cannot read {file}: {e}");
        std::process::exit(2);
    });
    let (program, sigs) = match entoli_check::parse::parse_program(&src) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("PARSE-ERROR {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = program.validate() {
        eprintln!("DIAG {e}");
        std::process::exit(1);
    }
    let grants_spec: Vec<(String, Option<i64>)> = flags_all(args, "--grant")
        .iter()
        .map(|g| match g.split_once('=') {
            Some((e, n)) => (e.to_string(), Some(n.parse().expect("budget"))),
            None => (g.clone(), None),
        })
        .collect();
    // Static gate with the granted capabilities bound.
    let bindings: Vec<(String, entoli_types::Type)> = grants_spec
        .iter()
        .map(|(e, _)| (format!("cap_{}", e.to_lowercase()), entoli_types::Type::Cap(e.clone())))
        .collect();
    if !program.defs.contains_key("main") {
        eprintln!(
            "DIAG no `main` — a runnable program defines \
             `def main : Cap[E1] -[]-> ... -[]-> Unit -[]-> ...`"
        );
        std::process::exit(1);
    }
    // Entry convention: main takes one capability parameter per --grant, in
    // flag order, then Unit. Caps exist only in the entry term and the runtime
    // environment — never free inside definitions, so `compile` works on the
    // same file with no grants in sight.
    let mut entry = entoli_syntax::Term::Global("main".into());
    for (e, _) in &grants_spec {
        entry = entoli_syntax::build::app(
            entry,
            entoli_syntax::build::var(&format!("cap_{}", e.to_lowercase())),
        );
    }
    let entry = entoli_syntax::build::app(entry, entoli_syntax::build::unit());
    let diags = entoli_check::check(&program, &sigs, &entry, &bindings);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!("DIAG {d}");
        }
        std::process::exit(1);
    }
    let dir = PathBuf::from(flag(args, "--dir").expect("--dir"));
    std::fs::create_dir_all(&dir).expect("dir");
    let store = SqliteStore::open(&dir.join("store.sqlite")).expect("store");
    let mut rt = Runtime::new(&program, &sigs, store);
    if let Some(spec) = flag(args, "--crash") {
        rt.crash = CrashInjector::parse(&spec).expect("crash spec: E.op:point");
    }
    apply_compat(&mut rt, args);
    for spec in flags_all(args, "--bind") {
        let (name, rest) = spec.split_once('=').expect("--bind f=KIND:Effect.op[:param]");
        let mut parts = rest.splitn(3, ':');
        let kind = parts.next().expect("kind");
        let site = parts.next().expect("--bind f=KIND:Effect.op[:param]");
        let param = parts.next().map(str::to_string);
        let (effect, op) = site.split_once('.').expect("--bind f=KIND:Effect.op[:param]");
        let ext: Box<dyn entoli_runtime::ExternalOp> = match kind {
            "echo" => Box::new(EchoRemote),
            // T5.1 — real bindings for the pilot:
            "llm" => Box::new(LlmRemote {
                cache: dir.join("llm-cache"),
                model: param.expect("--bind f=llm:Effect.op:<model-id>"),
            }),
            "approve" => Box::new(ApprovalGate { dir: dir.join("approvals") }),
            "http" => Box::new(HttpRemote { url: param.expect("--bind f=http:E.op:URL") }),
            "ledger" => Box::new(entoli_testkit::fixtures::FakeLedger::new(&dir)),
            "approval" => Box::new(entoli_testkit::fixtures::FakeApproval::new(&dir)),
            "inventory" => Box::new(entoli_testkit::fixtures::FakeInventory::new(&dir)),
            "infer" => Box::new(entoli_testkit::fixtures::FakeInfer::new(&dir)),
            other => panic!("unknown binding kind {other:?} (echo|ledger|approval|inventory|infer)"),
        };
        rt.bind_foreign(name, effect, op, ext);
    }
    let mut env = entoli_core::Env::empty();
    for (e, budget) in &grants_spec {
        let cap = rt.grants.grant(e, *budget);
        env = env.bind(&format!("cap_{}", e.to_lowercase()), entoli_core::Value::Cap(cap));
    }
    let result = if recover {
        let driver = entoli_recovery::driver_for(&program, &entry);
        // The program's own recovery prelude: `wrap` applied to the caps in
        // --grant order, then to the driver as a thunk.
        let wrapped = if program.defs.contains_key("wrap") {
            let mut t = entoli_syntax::Term::Global("wrap".into());
            for (e, _) in &grants_spec {
                t = entoli_syntax::build::app(
                    t,
                    entoli_syntax::build::var(&format!("cap_{}", e.to_lowercase())),
                );
            }
            entoli_syntax::build::app(t, entoli_syntax::build::lam("u", driver))
        } else {
            driver
        };
        rt.recover_env(Rc::new(wrapped), env)
    } else {
        rt.run_program_env(Rc::new(entry), env)
    };
    match result {
        Ok(v) => println!("RESULT {v:?}"),
        Err(e) => {
            eprintln!("ERROR {e}");
            std::process::exit(1);
        }
    }
}

/// `approve --dir D --witness W --decision approve|reject` — the human half of
/// the ApprovalGate binding: records the decision the suspended run waits for.
fn approve(args: &[String]) {
    let dir = PathBuf::from(flag(args, "--dir").expect("--dir"));
    let w = flag(args, "--witness").expect("--witness");
    let d = flag(args, "--decision").expect("--decision approve|reject");
    assert!(d == "approve" || d == "reject", "--decision approve|reject");
    let adir = dir.join("approvals");
    std::fs::create_dir_all(&adir).expect("approvals dir");
    std::fs::write(adir.join(sanitize(&w)), &d).expect("write decision");
    println!("DECISION {w} = {d}; now run `recover` with the same flags");
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn value_text(v: &entoli_core::Value) -> String {
    match v {
        entoli_core::Value::Str(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// Real LLM binding: one model API call via curl, deduplicated by a local
/// witness-keyed completion cache — which is exactly how the binding honours
/// its declared at-least-once contract (recovery replays the SAME draft).
/// Reads ANTHROPIC_API_KEY from the environment at invocation; never logs it.
struct LlmRemote {
    cache: PathBuf,
    model: String,
}

impl entoli_runtime::ExternalOp for LlmRemote {
    fn invoke(
        &mut self,
        _op: &str,
        arg: &entoli_core::Value,
        witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<entoli_core::Value, String> {
        let prompt = value_text(arg);
        let w = witness.ok_or("llm binding requires an at_least_once witness")?;
        std::fs::create_dir_all(&self.cache).map_err(|e| e.to_string())?;
        let cached = self.cache.join(sanitize(w));
        if let Ok(text) = std::fs::read_to_string(&cached) {
            return Ok(entoli_core::Value::Str(text));
        }
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY not set in the environment".to_string())?;
        let body = format!(
            r#"{{"model":{},"max_tokens":1024,"messages":[{{"role":"user","content":{}}}]}}"#,
            json_str(&self.model),
            json_str(&prompt)
        );
        let out = std::process::Command::new("curl")
            .args(["-s", "https://api.anthropic.com/v1/messages"])
            .args(["-H", "content-type: application/json"])
            .args(["-H", "anthropic-version: 2023-06-01"])
            .args(["-H", &format!("x-api-key: {key}")])
            .args(["--data-binary", "@-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.as_mut().unwrap().write_all(body.as_bytes())?;
                c.wait_with_output()
            })
            .map_err(|e| format!("curl: {e}"))?;
        let resp = String::from_utf8_lossy(&out.stdout);
        let text = extract_json_text(&resp)
            .ok_or_else(|| format!("unexpected API response: {}", &resp[..resp.len().min(300)]))?;
        std::fs::write(&cached, &text).map_err(|e| e.to_string())?;
        Ok(entoli_core::Value::Str(text))
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Extract the first "text":"..." string from the Messages API response.
fn extract_json_text(resp: &str) -> Option<String> {
    let i = resp.find("\"text\":\"")? + 8;
    let bytes = resp.as_bytes();
    let mut out = String::new();
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'"' => return Some(out),
            b'\\' => {
                j += 1;
                match bytes.get(j)? {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'u' => {
                        let code = u32::from_str_radix(resp.get(j + 1..j + 5)?, 16).ok()?;
                        out.push(char::from_u32(code).unwrap_or('?'));
                        j += 4;
                    }
                    other => out.push(*other as char),
                }
            }
            b => out.push(b as char),
        }
        j += 1;
    }
    None
}

/// Real human gate: the operation FAILS (pending) until a decision file exists,
/// which leaves the journal entry durably Pending — the run exits, hours or
/// days pass, `entoli approve` records the decision, `recover` completes.
struct ApprovalGate {
    dir: PathBuf,
}

impl entoli_runtime::ExternalOp for ApprovalGate {
    fn invoke(
        &mut self,
        _op: &str,
        _arg: &entoli_core::Value,
        witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<entoli_core::Value, String> {
        let w = witness.ok_or("approval binding requires an at_least_once witness")?;
        let path = self.dir.join(sanitize(w));
        match std::fs::read_to_string(&path).map(|s| s.trim().to_string()) {
            Ok(d) if d == "approve" => {
                Ok(entoli_core::Value::Ctor { name: "approve".into(), args: vec![] })
            }
            Ok(d) if d == "reject" => {
                Ok(entoli_core::Value::Ctor { name: "reject".into(), args: vec![] })
            }
            Ok(other) => Err(format!("unrecognised decision {other:?} for {w}")),
            Err(_) => Err(format!(
                "pending human approval ({w}); decide with: entoli-cli approve --dir <dir> \
                 --witness {w:?} --decision approve|reject, then `recover`"
            )),
        }
    }
}

/// Real webhook binding: witnessed POST; deduplication is the receiver's
/// contract (the witness travels in the payload for it).
struct HttpRemote {
    url: String,
}

impl entoli_runtime::ExternalOp for HttpRemote {
    fn invoke(
        &mut self,
        _op: &str,
        arg: &entoli_core::Value,
        witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<entoli_core::Value, String> {
        let body = format!(
            r#"{{"witness":{},"content":{}}}"#,
            json_str(witness.unwrap_or("-")),
            json_str(&value_text(arg))
        );
        let out = std::process::Command::new("curl")
            .args(["-s", "-X", "POST", "-H", "content-type: application/json"])
            .args(["--data-binary", &body, &self.url])
            .output()
            .map_err(|e| format!("curl: {e}"))?;
        if !out.status.success() {
            return Err(format!("http post failed: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let resp = String::from_utf8_lossy(&out.stdout);
        Ok(entoli_core::Value::Str(resp.chars().take(400).collect()))
    }
}

/// A trivial host service for `--bind f=echo:E.op`: returns its argument.
struct EchoRemote;
impl entoli_runtime::ExternalOp for EchoRemote {
    fn invoke(
        &mut self,
        _op: &str,
        arg: &entoli_core::Value,
        _witness: Option<&str>,
        _crash: &CrashInjector,
    ) -> Result<entoli_core::Value, String> {
        Ok(arg.clone())
    }
}

/// `contracts --dir D` — run the delivery-contract conformance harness (plan §11.3).
fn contracts(args: &[String]) {
    let dir = PathBuf::from(flag(args, "--dir").expect("--dir"));
    std::fs::create_dir_all(&dir).expect("dir");
    let reports = entoli_testkit::conformance::run_all(&dir);
    let mut ok = true;
    for r in &reports {
        println!("{r}");
        ok &= r.passed;
    }
    if !ok {
        std::process::exit(1);
    }
}

fn phase2_encode(args: &[String]) {
    let file = args.get(2).expect("usage: phase2-encode <file>");
    let (program, entry) = entoli_testkit::programs::phase2_program();
    let auth = MapAuthority::default();
    let mut m = Machine::new(&program, &auth, entry.into());
    let susp = match m.run_fuel(10_000_000).expect("evaluation") {
        Outcome::Suspended(s) => s,
        other => panic!("expected suspension, got {other:?}"),
    };
    assert_eq!(susp.effect, "Mark");
    assert_eq!(susp.arg, Value::Int(55));
    let sigs = entoli_types::Signatures::default();
    let codec = Codec::new(&program, &sigs, &auth);
    let bytes = codec.encode_kont(&susp.frames).expect("encode is total");
    let mut f = std::fs::File::create(file).expect("create");
    f.write_all(&bytes).expect("write");
    f.sync_all().expect("fsync");
    eprintln!("encoded {} bytes; dying", bytes.len());
    std::process::exit(KILLED);
}

fn phase2_resume(args: &[String]) {
    let file = args.get(2).expect("usage: phase2-resume <file> <int>");
    let resume_with: i64 = args.get(3).expect("resume value").parse().expect("int");
    let (program, _) = entoli_testkit::programs::phase2_program();
    let auth = MapAuthority::default();
    let sigs = entoli_types::Signatures::default();
    let codec = Codec::new(&program, &sigs, &auth);
    let bytes = std::fs::read(file).expect("read");
    let granter = MapGranter::default();
    let frames = codec.decode_kont(&bytes, &granter).expect("decode");
    let mut m = Machine::resume_frames(&program, &auth, frames, Value::Int(resume_with), "0".into());
    match m.run_fuel(10_000_000).expect("evaluation") {
        Outcome::Value(Value::Int(i)) => println!("RESULT {i}"),
        other => panic!("expected value, got {other:?}"),
    }
}
