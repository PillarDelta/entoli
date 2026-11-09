//! Phase 2: stable frame/value encoding and content addressing (paper §7.3, §10.1).
//!
//! `encode` is **pure and total over the language domain** (Rule 2): it performs no I/O
//! and cannot fail on any value the (future) checker admits. The two encode errors below
//! are exactly the conditions the Phase 5 checker will reject statically (I3 as a
//! precondition); durable-commit failure is a *store* concern and never appears here.
//!
//! Rule 4 structurally: `Value::Cap` has no byte representation. A capability crossing
//! the boundary is written as a `CapSlot` naming its effect — a name, not authority.
//! Decoding a slot consults the grant table in force *at resume*; a missing grant is an
//! E014 refusal naming the effect (paper §9.1, demo C).

mod bytes;
mod codec;
mod hash;

pub use codec::{Codec, DecodeError, EncodeError, MapGranter, ResumeGranter};
pub use hash::{def_hashes, effect_sig, program_hash, Hash256};
