//! Phase 3: the durable store σ (paper §7).
//!
//! Schema per plan §7.2–7.3: a journal with NO Compensating/Retrying states — recovery
//! operations are ordinary nested entries and `parent_journal_id` is what makes the
//! pending stack readable from the store (Rule 3 expressed as a schema) — plus a
//! continuation table. Step 1 (Intend) writes the pending entry AND its continuation in
//! one transaction (I2 by construction); step 4 (Commit) is a single row update.
//!
//! Rule 2: a failed commit is a **no-transition** — rusqlite transactions roll back on
//! error/drop, leaving σ exactly as it was. Encoding failures cannot occur here: the
//! store receives bytes, never values.

mod invariants;
mod sqlite;

pub use invariants::{assert_pending_bounds, assert_store_invariants, InvariantViolation};
pub use sqlite::SqliteStore;

/// Journal entry state: `Pending | Done(encoded_result)` and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryState {
    Pending,
    Done(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub journal_id: String,
    pub parent_journal_id: Option<String>,
    pub effect: String,
    pub op: String,
    /// The operation's delivery class as declared by the program in force at
    /// Intend, in canonical form: "idempotent" | "at_least_once" |
    /// "compensable:Effect.op". Recorded so that recovery dispatch — and I1's
    /// compensation clause — are functions of σ alone; a program whose current
    /// declaration disagrees with the record refuses recovery rather than
    /// silently dispatching differently (F-011).
    pub delivery: String,
    pub encoded_arg: Vec<u8>,
    pub witness: Option<String>,
    pub continuation_id: String,
    pub state: EntryState,
    /// Insertion order; the recovery frontier is the entry with the greatest seq.
    pub seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableContinuation {
    pub continuation_id: String,
    pub encoded_frames: Vec<u8>,
    /// (effect name, signature hash, compatibility token) for every effect the frames
    /// reference — §10.2: the computed half catches structural drift, the declared half
    /// catches semantic drift. Both are required to resume.
    pub handled_effects: Vec<(String, [u8; 32], String)>,
    /// Derived join of the frame contents' classifications (§9.2). All-public until a
    /// policy table exists (Phase 6).
    pub data_classification: String,
}

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// The store refused a transition (e.g. duplicate Intend, Done on a missing entry).
    Refused(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "store: {e}"),
            StoreError::Refused(m) => write!(f, "store refused: {m}"),
        }
    }
}
impl std::error::Error for StoreError {}

/// The durable store interface (plan §6: trait + SQLite backend).
pub trait DurableStore {
    /// Step 1 · Intend: one atomic transaction writing the Pending entry and its
    /// continuation. Failure is a no-transition.
    fn intend(
        &mut self,
        entry: &JournalEntry,
        kont: &DurableContinuation,
    ) -> Result<(), StoreError>;

    /// Step 4 · Commit: mark the entry Done with its encoded result. Single write.
    fn commit_done(&mut self, journal_id: &str, encoded_result: &[u8]) -> Result<(), StoreError>;

    /// Record the whole computation's final result durably (root completion).
    fn commit_final(&mut self, encoded_result: &[u8]) -> Result<(), StoreError>;

    fn final_result(&self) -> Result<Option<Vec<u8>>, StoreError>;

    /// All entries in insertion order.
    fn entries(&self) -> Result<Vec<JournalEntry>, StoreError>;

    /// The recovery frontier: the most recently inserted entry (ADR-0001 §7).
    fn frontier(&self) -> Result<Option<JournalEntry>, StoreError>;

    fn entry(&self, journal_id: &str) -> Result<Option<JournalEntry>, StoreError>;

    fn continuation(&self, continuation_id: &str)
        -> Result<Option<DurableContinuation>, StoreError>;

    /// §10.2: where no adapter is supplied, resume fails cleanly and the continuation
    /// is quarantined rather than lost. The mark is durable and sticky.
    fn quarantine(&mut self, continuation_id: &str, reason: &str) -> Result<(), StoreError>;

    fn quarantine_of(&self, continuation_id: &str) -> Result<Option<String>, StoreError>;

    /// Depth of the pending stack (I1): pending entries linked by parent_journal_id.
    fn pending_stack(&self) -> Result<Vec<JournalEntry>, StoreError>;
    /// The most recently inserted pending entry (indexed; no full scan).
    fn pending_top(&self) -> Result<Option<JournalEntry>, StoreError>;
    /// Number of children of `parent` (None = roots) — journal-id allocation
    /// needs a COUNT, not the journal (T3.1 / audit A-06, A-17).
    fn count_children(&self, parent: Option<&str>) -> Result<usize, StoreError>;
}
