//! Combining two records of the same paper, e.g. when a journal name was read
//! differently from two messages.

use rusqlite::params;

use crate::{Store, StoreError};

impl Store {
    /// Moves every event of manuscript `from` to `into` and removes `from`.
    /// Events are kept as they are, so the merged timeline holds all the
    /// evidence and the current state is derived from it as usual. `into`
    /// keeps its journal name and ID.
    pub fn merge_manuscripts(&mut self, from: i64, into: i64) -> Result<(), StoreError> {
        if from == into {
            return Err(StoreError::Format(
                "A manuscript can't be merged into itself.".into(),
            ));
        }
        self.write(|tx| {
            for id in [from, into] {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM manuscripts WHERE id = ?1)",
                    params![id],
                    |row| row.get(0),
                )?;
                if !exists {
                    return Err(StoreError::NotFound {
                        entity: "manuscript",
                        id,
                    });
                }
            }
            tx.execute(
                "UPDATE events SET manuscript_id = ?2 WHERE manuscript_id = ?1",
                params![from, into],
            )?;
            // Resubmission links that pointed at `from` now point at `into`.
            tx.execute(
                "UPDATE manuscripts SET previous_id = ?2 WHERE previous_id = ?1 AND id <> ?2",
                params![from, into],
            )?;
            tx.execute(
                "UPDATE manuscripts SET previous_id = NULL WHERE id = ?1 AND previous_id = ?2",
                params![into, from],
            )?;
            tx.execute("DELETE FROM manuscripts WHERE id = ?1", params![from])?;
            Ok(())
        })
    }
}
