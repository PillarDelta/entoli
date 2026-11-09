use std::fmt;

/// Runtime diagnostics. The E-codes are plan §12's; in Phases 1–4 they surface from the
/// dynamic proxies, in Phase 5 the same codes move to compile time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntoliError {
    /// E001 — no handler for a prompt effect in the continuation.
    UnhandledEffect { effect: String, op: String },
    /// E002 — handler installation without a live capability for the effect.
    MissingCapability { effect: String, detail: String },
    /// E003 — continuation resumed more than once (affinity, §5).
    DoubleResume { kont_id: String },
    /// E004 — resume not in tail position inside a handler clause (§4.3).
    NonTailResume { kont_id: String },
    /// E005 — suspension raised while a handler clause is servicing an operation (§4.3).
    NestedSuspension { effect: String, op: String },
    /// E014 — a continuation slot names a capability with no grant in force at resume.
    MissingResumeGrant { effect: String },
    /// At-least-once operation performed without an idempotency witness (T-Op's
    /// deliv side condition).
    MissingWitness { effect: String, op: String },
    UnknownOperation { effect: String, op: String },
    /// Shape errors that the Phase 5 checker will make unrepresentable.
    Dynamic(String),
}

impl fmt::Display for EntoliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EntoliError::*;
        match self {
            UnhandledEffect { effect, op } => {
                write!(f, "E001: unhandled effect {effect}.{op}")
            }
            MissingCapability { effect, detail } => {
                write!(f, "E002: missing capability for {effect}: {detail}")
            }
            DoubleResume { kont_id } => {
                write!(f, "E003: continuation {kont_id} resumed more than once")
            }
            NonTailResume { kont_id } => {
                write!(f, "E004: non-tail resume of {kont_id} in suspending handler clause")
            }
            NestedSuspension { effect, op } => {
                write!(f, "E005: {effect}.{op} would suspend while a handler clause is servicing an operation")
            }
            MissingResumeGrant { effect } => {
                write!(f, "E014: missing resume-time capability grant for {effect}")
            }
            MissingWitness { effect, op } => {
                write!(f, "at-least-once operation {effect}.{op} performed without an idempotency witness")
            }
            UnknownOperation { effect, op } => write!(f, "unknown operation {effect}.{op}"),
            Dynamic(msg) => write!(f, "dynamic error: {msg}"),
        }
    }
}

impl std::error::Error for EntoliError {}
