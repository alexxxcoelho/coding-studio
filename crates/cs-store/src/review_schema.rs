//! Schema changes of V1 lot 4 (Review), kept apart from `schema.sql` and from
//! the lot 5 schema so that the lots merge cleanly.
//!
//! [`migrate`] runs on every open, whatever the version stored in the
//! database (the version is moved to 7 afterwards): new tables are
//! `CREATE ... IF NOT EXISTS` and a column is added only when
//! `pragma_table_info` says it is missing. A new database, a version 3 to 6
//! database and a database already at 7 end in the same schema.

use anyhow::Result;
use rusqlite::Connection;

const REVIEW_SCHEMA: &str = include_str!("review_schema.sql");

/// Columns that lot 4 adds to existing tables: table, column, definition.
const ADDED_COLUMNS: [(&str, &str, &str); 1] = [
    // `passages` when the unit coding was written from the codings of the
    // passages of the unit; NULL when the candidate wrote it by hand.
    ("human_codings", "source", "TEXT"),
];

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let found: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info(?1)")?
        .query_map([table], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(found.iter().any(|n| n == column))
}

/// Applies the lot 4 schema. Safe to run any number of times.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(REVIEW_SCHEMA)?;
    for (table, column, ddl) in ADDED_COLUMNS {
        if !has_column(conn, table, column)? {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
        }
    }
    Ok(())
}
