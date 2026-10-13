//! V1 lot 5: the schema migration is idempotent, runs for every older
//! version and keeps the data.

use super::*;

fn tables(s: &Store) -> Vec<String> {
    let mut stmt = s
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn columns(s: &Store, table: &str) -> Vec<String> {
    let mut stmt = s
        .connection()
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .unwrap();
    stmt.query_map([table], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

const LOT5_TABLES: [&str; 14] = [
    "themes",
    "theme_codes",
    "theme_questions",
    "theme_check_answers",
    "theme_history",
    "matrix_views",
    "matrix_cells",
    "matrix_cell_passages",
    "passage_links",
    "writing_versions",
    "final_audits",
    "final_audit_verdicts",
    "unit_marks",
    "code_memo_history",
];

#[test]
fn the_version_is_seven_and_every_lot_5_table_exists() {
    let (s, _d) = ready();
    assert_eq!(SCHEMA_VERSION, 7);
    let t = tables(&s);
    for name in LOT5_TABLES {
        assert!(t.contains(&name.to_string()), "missing {name}");
    }
    assert!(columns(&s, "passages").contains(&"origin".to_string()));
    assert!(columns(&s, "passages").contains(&"role".to_string()));
    assert!(columns(&s, "claims").contains(&"nature".to_string()));
}

#[test]
fn a_version_4_database_is_migrated_and_keeps_its_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("v4.db");
    {
        let s = Store::open(&path).unwrap();
        s.upsert_interview(&default_entry(), ACTOR).unwrap();
        s.save_segmentation(
            &segmentation(&["Un tour assez long pour etre selectionne."]),
            ACTOR,
        )
        .unwrap();
        let (p, _) = s
            .create_passage(
                &crate::NewPassage {
                    interview_id: ID.into(),
                    unit_id: U1.into(),
                    start: None,
                    end: None,
                    offset_unit: Default::default(),
                    text: None,
                },
                ACTOR,
            )
            .unwrap();
        assert_eq!(p.passage_id, "P-00001");
        // Go back to what a version 4 database looked like.
        let c = s.connection();
        for table in LOT5_TABLES {
            c.execute_batch(&format!("DROP TABLE {table};")).unwrap();
        }
        c.execute_batch(
            "ALTER TABLE passages DROP COLUMN origin;
             ALTER TABLE passages DROP COLUMN role;
             ALTER TABLE claims DROP COLUMN nature;
             PRAGMA user_version = 4;",
        )
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let v: i64 = s
        .connection()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 7);
    for name in LOT5_TABLES {
        assert!(tables(&s).contains(&name.to_string()), "missing {name}");
    }
    assert_eq!(s.passage("P-00001").unwrap().unit_id, U1);
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn a_version_5_database_is_migrated_too_and_reopening_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("v5.db");
    drop(Store::open(&path).unwrap());
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA user_version = 5;").unwrap();
    }
    let first = Store::open(&path).unwrap();
    let before = tables(&first);
    drop(first);
    let second = Store::open(&path).unwrap();
    assert_eq!(tables(&second), before);
    let v: i64 = second
        .connection()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 7);
}

#[test]
fn the_append_only_tables_refuse_update_and_delete() {
    let (s, _d) = ready();
    let c = s.connection();
    c.execute(
        "INSERT INTO writing_versions (scope, target, sub, body, at, by) VALUES ('theme_part','T-1','a','{}','t','b')",
        [],
    )
    .unwrap();
    assert!(c
        .execute("UPDATE writing_versions SET body = 'x'", [])
        .is_err());
    assert!(c.execute("DELETE FROM writing_versions", []).is_err());
    c.execute(
        "INSERT INTO final_audits (seed, n, eligible, claims, created_at, created_by) VALUES ('s',3,'[]','[]','t','b')",
        [],
    )
    .unwrap();
    assert!(c.execute("DELETE FROM final_audits", []).is_err());
    c.execute(
        "INSERT INTO theme_history (theme_id, kind, at, by) VALUES ('T-1','created','t','b')",
        [],
    )
    .unwrap();
    assert!(c
        .execute("UPDATE theme_history SET reason = 'x'", [])
        .is_err());
    c.execute(
        "INSERT INTO code_memo_history (code_id, text, at, by) VALUES ('C-1','m','t','b')",
        [],
    )
    .unwrap();
    assert!(c.execute("DELETE FROM code_memo_history", []).is_err());
}
