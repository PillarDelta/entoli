//! ENTOLI Core evaluator (plan Phase 1).
//!
//! A CEK-style machine over an explicit heap continuation: the host call stack is never
//! the semantic continuation (plan §7.1). Frames hold only typed symbolic data — terms,
//! values, environments — never host closures, pointers, or handles, so the continuation
//! is representable *by construction* as frames are pushed (paper §7.3).
//!
//! Static guarantees carried by dynamic proxies until the Phase 5 checker exists
//! (plan §3.4): one-shot resume (consumed flag, E003), tail-position resume
//! (ClauseBoundary, E004), nested-suspension prohibition (E005), handler installation
//! requires a live capability for the effect (E002), mandatory witnesses on
//! at-least-once operations.

mod env;
mod error;
mod machine;
mod value;

pub use env::{Env, EnvNode};

/// The durable-window sentinel continuation id (P-5): resuming it halts the
/// window sub-machine with `Outcome::WindowReturn`.
pub fn machine_window_id() -> String {
    machine::WINDOW_KONT_ID.to_string()
}
pub use error::EntoliError;
pub use machine::{Machine, Outcome, SuspendHandler, Suspension};
pub use value::{new_kont, CapAuthority, CapId, Frame, Kont, KontCell, MapAuthority, Value};
