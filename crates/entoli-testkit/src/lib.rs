//! Test fixtures and demonstration programs (plan §9–§10). Fixtures record their own
//! state independently of ENTOLI's store, so "remote committed / local pending" and
//! "remote not committed / local pending" are distinguishable — without that
//! independence demonstration A proves nothing (plan §9).

pub mod conformance;
pub mod fixtures;
pub mod programs;
