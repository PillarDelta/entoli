//! Demonstration programs as AST builders (no surface syntax until Phase 5, plan §3.6).

use entoli_syntax::build::*;
use entoli_syntax::{DeliveryClass, EffectDecl, Handler, OpDecl, PrimOp, Program, SuspensionClass};
use std::rc::Rc;

/// Demonstration A (paper §13.1, plan §10.1): audit, suspend for approval, then post to
/// the ledger under an idempotency witness. Approval is at-least-once with a request-id
/// witness (plan §2: re-asking a human is not idempotent).
pub fn settle_program() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Approval".into(),
        EffectDecl {
            name: "Approval".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "request".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    p.effects.insert(
        "Ledger".into(),
        EffectDecl {
            name: "Ledger".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "post".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    // audit(inv) = Audited(inv, 250)
    p.defs.insert(
        "audit".into(),
        Rc::new(lam("inv", ctor("Audited", vec![var("inv"), int(250)]))),
    );
    // settle(inv) = let f = audit(inv) in
    //   match Approval.request(f, ι=req:inv) {
    //     .approve => posted(Ledger.post(f, ι=idem:inv))
    //     .reject  => halted(f) }
    p.defs.insert(
        "settle".into(),
        Rc::new(lam(
            "inv",
            let_(
                "f",
                app(global("audit"), var("inv")),
                match_(
                    do_w(
                        "Approval",
                        "request",
                        var("f"),
                        prim(PrimOp::Concat, vec![str_("req:"), var("inv")]),
                    ),
                    vec![
                        arm_ctor(
                            "approve",
                            &[],
                            ctor(
                                "posted",
                                vec![do_w(
                                    "Ledger",
                                    "post",
                                    var("f"),
                                    prim(PrimOp::Concat, vec![str_("idem:"), var("inv")]),
                                )],
                            ),
                        ),
                        arm_ctor("reject", &[], ctor("halted", vec![var("f")])),
                    ],
                ),
            ),
        )),
    );
    let entry = app(global("settle"), str_("INV-7"));
    (p, entry)
}

/// Demonstration A2 (plan §10.2): a compensable reservation followed by an
/// at-least-once post. Crashing after `reserve`'s remote commit selects the
/// compensation; crashing again after the compensation commits exercises recovery of
/// the recovery (Rule 3).
pub fn settle2_program() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Inventory".into(),
        EffectDecl {
            name: "Inventory".into(),
            class: SuspensionClass::Suspending,
            ops: vec![
                OpDecl {
                    name: "reserve".into(),
                    delivery: DeliveryClass::Compensable {
                        comp_effect: "Inventory".into(),
                        comp_op: "release".into(),
                    },
                },
                OpDecl { name: "release".into(), delivery: DeliveryClass::Idempotent },
            ],
        },
    );
    p.effects.insert(
        "Ledger".into(),
        EffectDecl {
            name: "Ledger".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "post".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    let entry = let_(
        "r",
        do_("Inventory", "reserve", str_("item-1")),
        let_(
            "receipt",
            do_w(
                "Ledger",
                "post",
                ctor("Entry", vec![str_("item-1"), int(99)]),
                str_("idem:item-1"),
            ),
            ctor("done", vec![var("r"), var("receipt")]),
        ),
    );
    (p, entry)
}

fn tail_foreign_handler(effect: &str, op: &str, foreign_name: &str) -> Handler {
    Handler {
        effect: effect.into(),
        return_clause: None,
        clauses: vec![clause(op, "x", "k", resume(var("k"), foreign_app(foreign_name, var("x"))))],
    }
}

/// Migration M5: the settle prelude — Approval and Ledger discharged by installed
/// handlers whose clauses call foreign invokers in tail-resume position
/// (unification §5). The cap variables are bound by the host.
pub fn settle_wrap(inner: entoli_syntax::Term) -> entoli_syntax::Term {
    handle(
        handle(
            inner,
            tail_foreign_handler("Approval", "request", "approval_svc"),
            var("cap_approval"),
        ),
        tail_foreign_handler("Ledger", "post", "ledger_svc"),
        var("cap_ledger"),
    )
}

/// Demonstration A under the unified machine: same program, discharged by handlers.
pub fn settle_unified() -> (Program, entoli_syntax::Term) {
    let (mut p, entry) = settle_program();
    p.foreigns.insert("approval_svc".into());
    p.foreigns.insert("ledger_svc".into());
    (p, settle_wrap(entry))
}

/// Migration M5: the settle2 prelude — Inventory (both ops: the driver's compensation
/// performs `release` through the same installation) and Ledger.
pub fn settle2_wrap(inner: entoli_syntax::Term) -> entoli_syntax::Term {
    let inventory = Handler {
        effect: "Inventory".into(),
        return_clause: None,
        clauses: vec![
            clause("reserve", "x", "k", resume(var("k"), foreign_app("inventory_reserve", var("x")))),
            clause("release", "x", "k", resume(var("k"), foreign_app("inventory_release", var("x")))),
        ],
    };
    handle(
        handle(inner, inventory, var("cap_inventory")),
        tail_foreign_handler("Ledger", "post", "ledger_svc"),
        var("cap_ledger"),
    )
}

pub fn settle2_unified() -> (Program, entoli_syntax::Term) {
    let (mut p, entry) = settle2_program();
    p.foreigns.insert("inventory_reserve".into());
    p.foreigns.insert("inventory_release".into());
    p.foreigns.insert("ledger_svc".into());
    (p, settle2_wrap(entry))
}

/// Migration M5: the agent prelude — budgeted Infer plus Approval. The restart entry
/// point installs only the handlers it holds capabilities for (unification §6):
/// a revoked effect is simply absent from the prelude, and the refusal then comes
/// from the persisted continuation's slot at decode — E014 naming the effect.
pub fn agent_wrap_with(infer: bool, approval: bool, inner: entoli_syntax::Term) -> entoli_syntax::Term {
    let mut t = inner;
    if infer {
        t = handle(t, tail_foreign_handler("Infer", "complete", "infer_svc"), var("cap_infer"));
    }
    if approval {
        t = handle(
            t,
            tail_foreign_handler("Approval", "request", "approval_svc"),
            var("cap_approval"),
        );
    }
    t
}

pub fn agent_wrap(inner: entoli_syntax::Term) -> entoli_syntax::Term {
    agent_wrap_with(true, true, inner)
}

pub fn agent_unified() -> (Program, entoli_syntax::Term) {
    let (mut p, entry) = agent_program();
    p.foreigns.insert("infer_svc".into());
    p.foreigns.insert("approval_svc".into());
    (p, agent_wrap(entry))
}

/// Migration M7: sneaky under handlers.
pub fn sneaky_wrap(inner: entoli_syntax::Term) -> entoli_syntax::Term {
    handle(inner, tail_foreign_handler("Ledger", "post", "ledger_svc"), var("cap_ledger"))
}

pub fn sneaky_unified() -> (Program, entoli_syntax::Term) {
    let (mut p, entry) = sneaky_program();
    p.foreigns.insert("ledger_svc".into());
    (p, sneaky_wrap(entry))
}

/// Adversarial program for review finding R-2: after one ledger post, tries to use the
/// machine-local class to materialise and double-resume its own persisted continuation.
/// Must be refused in a fresh run AND after being resumed by the recovery driver.
pub fn sneaky_program() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Ledger".into(),
        EffectDecl {
            name: "Ledger".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "post".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    let entry = let_(
        "r",
        do_w("Ledger", "post", ctor("Entry", vec![str_("s"), int(1)]), str_("idem:s")),
        let_("k2", do_("Local", "load_kont", str_("0")), ctor("Stolen", vec![var("k2")])),
    );
    (p, entry)
}

/// Demonstration C (paper §13.3, plan §10.4): an agent holding a budgeted inference
/// capability and a confidential customer record suspends awaiting approval. The
/// persisted continuation must contain no capability/budget/credential material, its
/// classification must be *derived* (confidential — the record is in a frame), a
/// revoked grant must refuse resume naming Infer, and a smaller re-grant must bound
/// the resumed computation.
pub fn agent_program() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Infer".into(),
        EffectDecl {
            name: "Infer".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "complete".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    p.effects.insert(
        "Approval".into(),
        EffectDecl {
            name: "Approval".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "request".into(), delivery: DeliveryClass::AtLeastOnce }],
        },
    );
    // agent(cap) = let secret = Confidential(CustomerRecord("acme-042")) in
    //   let a = Infer.complete(Prompt("p1")) in
    //   match Approval.request(a, ι="req:agent") {
    //     approve() => Done(Infer.complete(Prompt("p2")), Infer.complete(Prompt("p3")))
    //     reject()  => Halted(secret)  # keeps secret + cap live across the suspension
    //   }
    p.defs.insert(
        "agent".into(),
        Rc::new(lam(
            "cap",
            let_(
                "secret",
                ctor("Confidential", vec![ctor("CustomerRecord", vec![str_("acme-042")])]),
                let_(
                    "a",
                    do_("Infer", "complete", ctor("Prompt", vec![str_("p1")])),
                    match_(
                        do_w("Approval", "request", var("a"), str_("req:agent")),
                        vec![
                            arm_ctor(
                                "approve",
                                &[],
                                let_(
                                    "b",
                                    do_("Infer", "complete", ctor("Prompt", vec![str_("p2")])),
                                    let_(
                                        "c",
                                        do_("Infer", "complete", ctor("Prompt", vec![str_("p3")])),
                                        ctor("Done", vec![var("b"), var("c")]),
                                    ),
                                ),
                            ),
                            arm_ctor("reject", &[], ctor("Halted", vec![var("secret")])),
                        ],
                    ),
                ),
            ),
        )),
    );
    let entry = app(global("agent"), var("infer_cap"));
    (p, entry)
}

/// Phase 2 kill-test program: pure computation, one suspension, **no external effect
/// involved** (plan §5.1). Suspends at `Mark.here` carrying an environment with an
/// intermediate integer, a closure over it, and a constructor value.
///
/// With `fib(10) = 55` and resume value `b`: result = `(b + 55) + 55`.
pub fn phase2_program() -> (Program, entoli_syntax::Term) {
    let mut p = Program::default();
    p.effects.insert(
        "Mark".into(),
        EffectDecl {
            name: "Mark".into(),
            class: SuspensionClass::Suspending,
            ops: vec![OpDecl { name: "here".into(), delivery: DeliveryClass::Idempotent }],
        },
    );
    let fib = lam(
        "n",
        if_(
            prim(PrimOp::Lt, vec![var("n"), int(2)]),
            var("n"),
            prim(
                PrimOp::Add,
                vec![
                    app(global("fib"), prim(PrimOp::Sub, vec![var("n"), int(1)])),
                    app(global("fib"), prim(PrimOp::Sub, vec![var("n"), int(2)])),
                ],
            ),
        ),
    );
    p.defs.insert("fib".into(), Rc::new(fib));

    let entry = let_(
        "a",
        app(global("fib"), int(10)),
        let_(
            "f",
            lam("y", prim(PrimOp::Add, vec![var("y"), var("a")])),
            let_(
                "pair",
                ctor("Pair", vec![var("a"), str_("tag")]),
                let_(
                    "b",
                    do_("Mark", "here", var("a")),
                    match_(
                        var("pair"),
                        vec![arm_ctor(
                            "Pair",
                            &["x", "ignored"],
                            prim(PrimOp::Add, vec![app(var("f"), var("b")), var("x")]),
                        )],
                    ),
                ),
            ),
        ),
    );
    (p, entry)
}
