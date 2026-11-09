//! Phase 4: the recovery driver, expressed in core ENTOLI (paper Rule 3, §7.4).
//!
//! `driver_for` *generates* an ordinary ENTOLI term — one match over the recovery
//! frontier, with one arm per declared compensable operation. The generator is host
//! code, but its output is ordinary ENTOLI syntax: printable, inspectable, and executed
//! by the same machine as any other program. No recovery machinery exists that is not
//! represented in the calculus; the host supplies only the machine-local effects
//! (pure reads of σ + `Protocol.complete`) that plan §8 predicted, and nothing else.
//!
//! **Standing rule (plan §6):** this crate must not acquire privileged behaviour that
//! cannot be expressed in the language. Any host-side special case added here is a
//! stop-condition trigger.
//!
//! The driver implements the §8.1 dispatch exactly:
//!
//! ```text
//! match Local.frontier() {
//!   .none            -> <program entry>          // nothing external ever happened
//!   .done(j)         -> resume (Local.load_kont j) (Local.load_result j)
//!   .pending_re(j)   -> resume (Local.load_kont j) (Protocol.complete j)
//!   .pending_comp_E_op(j) ->                     // one arm per compensable op
//!       let _ = do E.op'(Local.comp_arg j, ι = "comp:" ++ j) in
//!       resume (Local.load_kont j) (Protocol.complete j)
//! }
//! ```
//!
//! A crash anywhere in here is an ordinary crash: the compensation `do` runs through
//! the full five-step protocol, its Intend persists *this driver's own continuation*
//! (the `let` frame above), and the next recovery run resumes it — which is exactly how
//! the paper's derived state sequence Pending → Compensating → Retrying → Done emerges
//! without any of those states existing in the store (plan §7.2).

use entoli_syntax::build::*;
use entoli_syntax::{Arm, DeliveryClass, Pattern, PrimOp, Program, Term};

/// The fourth §8.1 row is unreachable by construction: `Program::validate` (load time)
/// and the frontier view admit no operation without a delivery class, so this
/// generator has no arm — and needs no arm — for "no class".
pub fn driver_for(program: &Program, entry: &Term) -> Term {
    let resume_completed = |j: Term| {
        resume(
            do_("Local", "load_kont", j.clone()),
            do_("Protocol", "complete", j),
        )
    };

    let mut arms: Vec<Arm> = vec![
        // Before step 1, degenerate case: empty journal — the last committed
        // continuation is the program entry itself (EVALUATION.md F-002).
        Arm { pat: Pattern::Ctor { name: "none".into(), binders: vec![] }, body: entry.clone() },
        // After step 4: resume the named continuation with r; never re-invoke.
        arm_ctor(
            "done",
            &["j"],
            resume(
                do_("Local", "load_kont", var("j")),
                do_("Local", "load_result", var("j")),
            ),
        ),
        // Between 1 and 4, idempotent / at-least-once: re-invoke (same witness ι,
        // written at step 1 — the store replays it into the completion).
        arm_ctor("pending_re", &["j"], resume_completed(var("j"))),
    ];

    // Between 1 and 4, compensable(op'): perform op' as an ordinary effect, then
    // re-invoke. One arm per declared compensable operation.
    for decl in program.effects.values() {
        for op in &decl.ops {
            if let DeliveryClass::Compensable { comp_effect, comp_op } = &op.delivery {
                let comp_delivery = program
                    .effects
                    .get(comp_effect)
                    .and_then(|e| e.op(comp_op))
                    .map(|o| o.delivery.clone())
                    // validate() has already run; unreachable in practice.
                    .unwrap_or(DeliveryClass::Idempotent);
                let comp_arg = do_("Local", "comp_arg", var("j"));
                let comp_do = match comp_delivery {
                    DeliveryClass::AtLeastOnce => do_w(
                        comp_effect,
                        comp_op,
                        comp_arg,
                        // Deterministic witness: retrying a crashed compensation reuses
                        // the same ι (ADR-0001 §10).
                        prim(PrimOp::Concat, vec![str_("comp:"), var("j")]),
                    ),
                    _ => do_(comp_effect, comp_op, comp_arg),
                };
                arms.push(arm_ctor(
                    &format!("pending_comp_{comp_effect}_{comp_op}"),
                    &["j"],
                    let_("compensated", comp_do, resume_completed(var("j"))),
                ));
            }
        }
    }

    match_(do_("Local", "frontier", unit()), arms)
}
