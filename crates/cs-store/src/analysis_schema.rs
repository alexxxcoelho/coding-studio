//! Schema changes of V1 lot 5, kept apart from `schema.sql` so that they merge
//! cleanly with the changes of the other lots.
//!
//! [`migrate`] runs on every open, whatever the version stored in the
//! database: new tables are `CREATE ... IF NOT EXISTS`, and columns added to
//! existing tables are added only when `pragma_table_info` says they are
//! missing. The result is the same for a new database, a version 3, 4 or 5
//! database, and a database that was already migrated.

use anyhow::Result;
use rusqlite::Connection;

/// Tables, indexes and guards of lot 5 (idempotent).
const ANALYSIS_SCHEMA: &str = include_str!("analysis.sql");

/// Columns that lot 5 adds to existing tables: table, column, definition.
const ADDED_COLUMNS: [(&str, &str, &str); 3] = [
    // Optional qualification of a passage, fixed lists of the guide.
    ("passages", "origin", "TEXT"),
    ("passages", "role", "TEXT"),
    // Nature of evidence of a claim, an id of the project list.
    ("claims", "nature", "TEXT"),
];

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let found: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info(?1)")?
        .query_map([table], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(found.iter().any(|n| n == column))
}

/// Applies the lot 5 schema. Safe to run any number of times.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(ANALYSIS_SCHEMA)?;
    for (table, column, ddl) in ADDED_COLUMNS {
        if !has_column(conn, table, column)? {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
        }
    }
    Ok(())
}
