use crate::{DurableContinuation, DurableStore, EntryState, JournalEntry, StoreError};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct SqliteStore {
    conn: Connection,
    /// Test hook (plan §11.4 / Rule 2): when set, the next durable transaction is
    /// refused *before* touching σ — inducing a commit failure as a no-transition.
    pub fail_next_commit: bool,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS journal (
    seq                INTEGER PRIMARY KEY AUTOINCREMENT,
    journal_id         TEXT NOT NULL UNIQUE,
    parent_journal_id  TEXT,
    effect             TEXT NOT NULL,
    op                 TEXT NOT NULL,
    delivery           TEXT NOT NULL,
    encoded_arg        BLOB NOT NULL,
    witness            TEXT,
    continuation_id    TEXT NOT NULL REFERENCES continuations(continuation_id),
    state              TEXT NOT NULL CHECK (state IN ('pending', 'done')),
    encoded_result     BLOB
);
CREATE TABLE IF NOT EXISTS continuations (
    continuation_id     TEXT PRIMARY KEY,
    encoded_frames      BLOB NOT NULL,
    handled_effects     BLOB NOT NULL,
    data_classification TEXT NOT NULL,
    quarantined         TEXT
);
CREATE INDEX IF NOT EXISTS journal_state  ON journal(state);
CREATE INDEX IF NOT EXISTS journal_parent ON journal(parent_journal_id);
CREATE TABLE IF NOT EXISTS final_result (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    encoded_result BLOB NOT NULL
);
";

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        // WAL + FULL sync: a committed transaction survives process death.
        conn.pragma_update(None, "journal_mode", "wal")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn, fail_next_commit: false })
    }

    fn induced_failure(&mut self) -> Result<(), StoreError> {
        if self.fail_next_commit {
            self.fail_next_commit = false;
            return Err(StoreError::Refused("induced commit failure (test)".into()));
        }
        Ok(())
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<JournalEntry> {
        let state: String = row.get("state")?;
        let result: Option<Vec<u8>> = row.get("encoded_result")?;
        Ok(JournalEntry {
            journal_id: row.get("journal_id")?,
            parent_journal_id: row.get("parent_journal_id")?,
            effect: row.get("effect")?,
            op: row.get("op")?,
            delivery: row.get("delivery")?,
            encoded_arg: row.get("encoded_arg")?,
            witness: row.get("witness")?,
            continuation_id: row.get("continuation_id")?,
            state: match state.as_str() {
                "pending" => EntryState::Pending,
                _ => EntryState::Done(result.unwrap_or_default()),
            },
            seq: row.get("seq")?,
        })
    }
}

fn encode_handled(effects: &[(String, [u8; 32], String)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(effects.len() as u32).to_le_bytes());
    for (name, hash, compat) in effects {
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(hash);
        out.extend_from_slice(&(compat.len() as u32).to_le_bytes());
        out.extend_from_slice(compat.as_bytes());
    }
    out
}

fn decode_handled(bytes: &[u8]) -> Vec<(String, [u8; 32], String)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let take = |pos: &mut usize, n: usize| -> Option<&[u8]> {
        if *pos + n > bytes.len() {
            return None;
        }
        let s = &bytes[*pos..*pos + n];
        *pos += n;
        Some(s)
    };
    let n = match take(&mut pos, 4) {
        Some(b) => u32::from_le_bytes(b.try_into().unwrap()),
        None => return out,
    };
    for _ in 0..n {
        let Some(lb) = take(&mut pos, 4) else { return out };
        let len = u32::from_le_bytes(lb.try_into().unwrap()) as usize;
        let Some(nb) = take(&mut pos, len) else { return out };
        let name = String::from_utf8_lossy(nb).into_owned();
        let Some(hb) = take(&mut pos, 32) else { return out };
        let hash: [u8; 32] = hb.try_into().unwrap();
        let Some(clb) = take(&mut pos, 4) else { return out };
        let clen = u32::from_le_bytes(clb.try_into().unwrap()) as usize;
        let Some(cb) = take(&mut pos, clen) else { return out };
        out.push((name, hash, String::from_utf8_lossy(cb).into_owned()));
    }
    out
}

impl DurableStore for SqliteStore {
    fn intend(
        &mut self,
        entry: &JournalEntry,
        kont: &DurableContinuation,
    ) -> Result<(), StoreError> {
        self.induced_failure()?;
        let tx = self.conn.transaction()?;
        // Continuation first (I2: no Pending without its continuation durable), then the
        // entry — one atomic transaction either way.
        tx.execute(
            "INSERT INTO continuations (continuation_id, encoded_frames, handled_effects, data_classification)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                kont.continuation_id,
                kont.encoded_frames,
                encode_handled(&kont.handled_effects),
                kont.data_classification
            ],
        )?;
        tx.execute(
            "INSERT INTO journal (journal_id, parent_journal_id, effect, op, delivery, encoded_arg, witness, continuation_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
            params![
                entry.journal_id,
                entry.parent_journal_id,
                entry.effect,
                entry.op,
                entry.delivery,
                entry.encoded_arg,
                entry.witness,
                entry.continuation_id
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn commit_done(&mut self, journal_id: &str, encoded_result: &[u8]) -> Result<(), StoreError> {
        self.induced_failure()?;
        let n = self.conn.execute(
            "UPDATE journal SET state = 'done', encoded_result = ?2
             WHERE journal_id = ?1 AND state = 'pending'",
            params![journal_id, encoded_result],
        )?;
        if n != 1 {
            return Err(StoreError::Refused(format!(
                "commit_done on {journal_id}: no pending entry"
            )));
        }
        Ok(())
    }

    fn commit_final(&mut self, encoded_result: &[u8]) -> Result<(), StoreError> {
        self.induced_failure()?;
        self.conn.execute(
            "INSERT INTO final_result (id, encoded_result) VALUES (1, ?1)
             ON CONFLICT(id) DO NOTHING",
            params![encoded_result],
        )?;
        Ok(())
    }

    fn final_result(&self) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT encoded_result FROM final_result WHERE id = 1", [], |r| r.get(0))
            .optional()?)
    }

    fn entries(&self) -> Result<Vec<JournalEntry>, StoreError> {
        let mut stmt = self.conn.prepare_cached("SELECT * FROM journal ORDER BY seq ASC")?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn frontier(&self) -> Result<Option<JournalEntry>, StoreError> {
        Ok(self
            .conn
            .prepare("SELECT * FROM journal ORDER BY seq DESC LIMIT 1")?
            .query_row([], Self::row_to_entry)
            .optional()?)
    }

    fn entry(&self, journal_id: &str) -> Result<Option<JournalEntry>, StoreError> {
        Ok(self
            .conn
            .prepare("SELECT * FROM journal WHERE journal_id = ?1")?
            .query_row(params![journal_id], Self::row_to_entry)
            .optional()?)
    }

    fn continuation(
        &self,
        continuation_id: &str,
    ) -> Result<Option<DurableContinuation>, StoreError> {
        Ok(self
            .conn
            .prepare("SELECT * FROM continuations WHERE continuation_id = ?1")?
            .query_row(params![continuation_id], |row| {
                let handled: Vec<u8> = row.get("handled_effects")?;
                Ok(DurableContinuation {
                    continuation_id: row.get("continuation_id")?,
                    encoded_frames: row.get("encoded_frames")?,
                    handled_effects: decode_handled(&handled),
                    data_classification: row.get("data_classification")?,
                })
            })
            .optional()?)
    }

    fn quarantine(&mut self, continuation_id: &str, reason: &str) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE continuations SET quarantined = ?2 WHERE continuation_id = ?1",
            params![continuation_id, reason],
        )?;
        if n != 1 {
            return Err(StoreError::Refused(format!("quarantine of unknown {continuation_id}")));
        }
        Ok(())
    }

    fn quarantine_of(&self, continuation_id: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT quarantined FROM continuations WHERE continuation_id = ?1",
                params![continuation_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    fn pending_top(&self) -> Result<Option<JournalEntry>, StoreError> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT * FROM journal WHERE state = 'pending' ORDER BY seq DESC LIMIT 1",
            )?
            .query_row([], Self::row_to_entry)
            .optional()?)
    }

    fn count_children(&self, parent: Option<&str>) -> Result<usize, StoreError> {
        let n: i64 = match parent {
            Some(p) => self
                .conn
                .prepare_cached("SELECT COUNT(*) FROM journal WHERE parent_journal_id = ?1")?
                .query_row(params![p], |r| r.get(0))?,
            None => self
                .conn
                .prepare_cached("SELECT COUNT(*) FROM journal WHERE parent_journal_id IS NULL")?
                .query_row([], |r| r.get(0))?,
        };
        Ok(n as usize)
    }

    fn pending_stack(&self) -> Result<Vec<JournalEntry>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM journal WHERE state = 'pending' ORDER BY seq ASC")?;
        let rows = stmt.query_map([], Self::row_to_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
