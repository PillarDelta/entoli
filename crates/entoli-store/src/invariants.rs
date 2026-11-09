//! INV(σ), paper §7.2 — the cheapest instrument in the project (plan §11.4). Run after
//! every durable transition, before shutdown, immediately after restart, and during
//! every crash-injection test.

use crate::{DurableStore, EntryState, StoreError};

#[derive(Debug)]
pub enum InvariantViolation {
    /// I1 — the pending entries do not form a parent-linked stack of depth ≤ 2.
    PendingDepth { depth: usize },
    I1NotAStack { detail: String },
    /// I2 — a journal entry references a continuation that is not durable.
    MissingContinuation { journal_id: String, continuation_id: String },
    Store(StoreError),
}

impl std::fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvariantViolation::PendingDepth { depth } => {
                write!(f, "I1 violated: pending stack depth {depth} > 2")
            }
            InvariantViolation::I1NotAStack { detail } => {
                write!(f, "I1 violated: pending entries are not a stack: {detail}")
            }
            InvariantViolation::MissingContinuation { journal_id, continuation_id } => write!(
                f,
                "I2 violated: entry {journal_id} references non-durable continuation {continuation_id}"
            ),
            InvariantViolation::Store(e) => write!(f, "invariant check could not read σ: {e}"),
        }
    }
}

/// The light invariant check: I1 only, from the (indexed) pending stack — no
/// journal scan, no continuation blobs. Release builds run this after every
/// durable transition; the full audit below is what the Phase 10 preservation
/// theorem licenses confining to debug builds (T3.1 / audit A-06, A-13).
pub fn assert_pending_bounds(store: &dyn DurableStore) -> Result<(), InvariantViolation> {
    let pending = store.pending_stack().map_err(InvariantViolation::Store)?;
    if pending.len() > 2 {
        return Err(InvariantViolation::PendingDepth { depth: pending.len() });
    }
    if pending.len() == 2 {
        let (bottom, top) = (&pending[0], &pending[1]);
        if top.parent_journal_id.as_deref() != Some(bottom.journal_id.as_str()) {
            return Err(InvariantViolation::I1NotAStack {
                detail: format!(
                    "{} pending but not a child of pending {}",
                    top.journal_id, bottom.journal_id
                ),
            });
        }
        let expected = format!("compensable:{}.{}", top.effect, top.op);
        if bottom.delivery != expected {
            return Err(InvariantViolation::I1NotAStack {
                detail: format!(
                    "pending child {}.{} is not the recorded compensation of {} (recorded: {})",
                    top.effect, top.op, bottom.journal_id, bottom.delivery
                ),
            });
        }
    }
    Ok(())
}

pub fn assert_store_invariants(store: &dyn DurableStore) -> Result<(), InvariantViolation> {
    let entries = store.entries().map_err(InvariantViolation::Store)?;

    // I2 — continuation precedence. Intend writes both in one transaction, so every
    // entry's continuation must be resolvable, pending or done.
    for e in &entries {
        let k = store
            .continuation(&e.continuation_id)
            .map_err(InvariantViolation::Store)?;
        if k.is_none() {
            return Err(InvariantViolation::MissingContinuation {
                journal_id: e.journal_id.clone(),
                continuation_id: e.continuation_id.clone(),
            });
        }
        // I3 — representability was established before σ was written (the encoder is
        // the only path in); the stored blob being non-empty is the cheap residue of
        // that check available without a program in hand.
        debug_assert!(!k.unwrap().encoded_frames.is_empty());
    }

    // I1 — bounded pending: at most an operation and one recovery action for it,
    // linked by parent_journal_id.
    let pending: Vec<_> =
        entries.iter().filter(|e| e.state == EntryState::Pending).collect();
    if pending.len() > 2 {
        return Err(InvariantViolation::PendingDepth { depth: pending.len() });
    }
    if pending.len() == 2 {
        let (bottom, top) = (pending[0], pending[1]);
        if top.parent_journal_id.as_deref() != Some(bottom.journal_id.as_str()) {
            return Err(InvariantViolation::I1NotAStack {
                detail: format!(
                    "{} pending but not a child of pending {}",
                    top.journal_id, bottom.journal_id
                ),
            });
        }
        // I1's compensation clause, checkable from σ alone since F-011 recorded
        // the delivery class at Intend: the child must be the compensation the
        // parent's record names.
        let expected = format!("compensable:{}.{}", top.effect, top.op);
        if bottom.delivery != expected {
            return Err(InvariantViolation::I1NotAStack {
                detail: format!(
                    "pending child {}.{} is not the recorded compensation of {} (recorded: {})",
                    top.effect, top.op, bottom.journal_id, bottom.delivery
                ),
            });
        }
    }
    Ok(())
}
