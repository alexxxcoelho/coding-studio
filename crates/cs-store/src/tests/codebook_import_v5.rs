//! Codebook import from CSV and JSON, and the CSV export round trip (v5).
//! Red until implemented. `import_codebook` (coding.db) is unchanged and
//! covered by the existing tests.

use super::*;
use cs_model::Codebook;

const HEADER: &str =
    "code,family,label,definition,origin,toe,research_questions,version,example,reason";

fn write(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn csv(rows: &[&str]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for r in rows {
        out.push_str(r);
        out.push('\n');
    }
    out
}

fn import_actions(s: &Store) -> Vec<AuditEvent> {
    s.audit_events(None)
        .unwrap()
        .into_iter()
        .filter(|e| e.action == "codebook.import")
        .collect()
}

fn assert_refused_and_untouched(s: &Store, r: anyhow::Result<Codebook>, needle: &str) {
    let e = r.unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
    assert!(
        e.to_string()
            .to_lowercase()
            .contains(&needle.to_lowercase()),
        "message {e:?} should mention {needle:?}"
    );
    assert!(s.codebook().unwrap().codes.is_empty(), "nothing imported");
    assert!(import_actions(s).is_empty(), "no audit event");
}

fn json_book(codes: Vec<cs_model::Code>) -> String {
    serde_json::to_string(&Codebook {
        source: "test".into(),
        codes,
    })
    .unwrap()
}

// ------------------------------------------------------------------- CSV

#[test]
fn csv_import_maps_every_column() {
    let dir = TempDir::new().unwrap();
    let path = write(
        &dir,
        "codebook.csv",
        &csv(&[
            "TECH-ADV,TECH,Relative advantage,\"Perceived advantage, over the status quo.\",literature,T,SQ1;SQ2,2,\"e.g. \"\"paid back\"\"\",",
            "ORG-SKILLS,ORG,Skills,Skills and expertise.,literature,O,SQ2,1,,",
            "NEW-FIELD,OTHER,A field code,\"Created from the field,\nsecond line.\",field,,,1,,seen in the third interview",
        ]),
    );
    let s = Store::open_in_memory().unwrap();
    let book = s.import_codebook_csv(&path, "alice").unwrap();
    assert_eq!(book.codes.len(), 3);
    assert_eq!(s.codebook().unwrap().codes, book.codes);
    let order: Vec<&str> = book.codes.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(
        order,
        ["TECH-ADV", "ORG-SKILLS", "NEW-FIELD"],
        "file order kept"
    );

    let adv = book.get("TECH-ADV").unwrap();
    assert_eq!(adv.label, "Relative advantage");
    assert_eq!(adv.definition, "Perceived advantage, over the status quo.");
    assert_eq!(adv.origin, "literature");
    assert_eq!(adv.toe.as_deref(), Some("T"));
    assert_eq!(adv.family, "TECH");
    assert_eq!(adv.research_questions, ["SQ1", "SQ2"]);
    assert_eq!(adv.version, 2);
    assert_eq!(adv.example.as_deref(), Some("e.g. \"paid back\""));
    assert_eq!(adv.reason, None);
    assert!(adv.active && adv.in_prompt);

    let field = book.get("NEW-FIELD").unwrap();
    assert_eq!(field.origin, "field");
    assert_eq!(field.toe, None);
    assert_eq!(field.family, "OTHER");
    assert!(field.research_questions.is_empty());
    assert_eq!(field.definition, "Created from the field,\nsecond line.");
    assert_eq!(field.reason.as_deref(), Some("seen in the third interview"));

    // Research questions named in the file exist afterwards.
    let rq: Vec<String> = s
        .research_questions()
        .unwrap()
        .into_iter()
        .map(|q| q.id)
        .collect();
    assert_eq!(rq, ["SQ1", "SQ2"]);

    let events = import_actions(&s);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor, "alice");
    assert_eq!(events[0].detail["format"], "csv");
    assert_eq!(events[0].detail["n_codes"], 3);
}

#[test]
fn csv_columns_are_read_by_name_and_only_code_and_definition_are_required() {
    let dir = TempDir::new().unwrap();
    let path = write(
        &dir,
        "min.csv",
        "definition,code\nA law requires it.,LEGAL-OBLIGATION\r\nCustomers ask for it.,CUSTOMER-REQUEST\r\n",
    );
    let s = Store::open_in_memory().unwrap();
    let book = s.import_codebook_csv(&path, ACTOR).unwrap();
    assert_eq!(book.codes.len(), 2);
    let c = book.get("LEGAL-OBLIGATION").unwrap();
    assert_eq!(c.definition, "A law requires it.");
    assert_eq!(c.origin, "field", "default origin");
    assert_eq!(c.toe, None);
    assert_eq!(c.version, 1);
}

#[test]
fn csv_with_a_missing_required_column_is_refused() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let path = write(&dir, "a.csv", "code,label\nTECH-ADV,Advantage\n");
    assert_refused_and_untouched(&s, s.import_codebook_csv(&path, ACTOR), "definition");
    let path = write(&dir, "b.csv", "label,definition\nAdvantage,x\n");
    assert_refused_and_untouched(&s, s.import_codebook_csv(&path, ACTOR), "code");
    let path = write(&dir, "c.csv", "");
    assert!(s.import_codebook_csv(&path, ACTOR).is_err());
    assert!(s.codebook().unwrap().codes.is_empty());
}

#[test]
fn csv_codes_must_match_the_prompt_schema_pattern() {
    // ^[A-Z][A-Z0-9-]*$ as in the output schema of the prompt.
    for bad in [
        "tech-adv", "Tech-Adv", "1TECH", "-TECH", "TECH ADV", "TECH_ADV", "TECH,X", "NA",
    ] {
        let dir = TempDir::new().unwrap();
        let s = Store::open_in_memory().unwrap();
        let row = format!("\"{bad}\",TECH,Label,Some definition.,literature,T,,1,,");
        let path = write(&dir, "bad.csv", &csv(&[&row]));
        let e = s.import_codebook_csv(&path, ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{bad}: {e}");
        assert!(s.codebook().unwrap().codes.is_empty(), "{bad}");
    }
    for good in ["A", "TECH-ADV", "ORG-2", "X9"] {
        let dir = TempDir::new().unwrap();
        let s = Store::open_in_memory().unwrap();
        let row = format!("{good},TECH,Label,Some definition.,literature,T,,1,,");
        let path = write(&dir, "good.csv", &csv(&[&row]));
        s.import_codebook_csv(&path, ACTOR).unwrap();
    }
}

#[test]
fn csv_duplicates_refuse_the_whole_file() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let path = write(
        &dir,
        "dup.csv",
        &csv(&[
            "TECH-ADV,TECH,Advantage,First.,literature,T,,1,,",
            "ORG-SKILLS,ORG,Skills,Skills.,literature,O,,1,,",
            "TECH-ADV,TECH,Advantage again,Second.,literature,T,,1,,",
        ]),
    );
    assert_refused_and_untouched(&s, s.import_codebook_csv(&path, ACTOR), "duplicate");
    let e = s.import_codebook_csv(&path, ACTOR).unwrap_err();
    assert!(e.to_string().contains("TECH-ADV"), "{e}");
}

#[test]
fn csv_bad_toe_origin_or_version_are_refused() {
    for (row, needle) in [
        ("A-ONE,TECH,L,Def.,literature,X,,1,,", "toe"),
        ("A-ONE,TECH,L,Def.,phase1,T,,1,,", "origin"),
        ("A-ONE,TECH,L,Def.,invented,T,,1,,", "origin"),
        ("A-ONE,TECH,L,Def.,literature,T,,zero,,", "version"),
        ("A-ONE,TECH,L,,literature,T,,1,,", "definition"),
    ] {
        let dir = TempDir::new().unwrap();
        let s = Store::open_in_memory().unwrap();
        let path = write(&dir, "x.csv", &csv(&[row]));
        assert_refused_and_untouched(&s, s.import_codebook_csv(&path, ACTOR), needle);
    }
}

#[test]
fn csv_import_of_a_missing_file_fails_cleanly() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    assert!(s
        .import_codebook_csv(&dir.path().join("absent.csv"), ACTOR)
        .is_err());
}

// ------------------------------------------------------------------ JSON

fn json_code(code: &str, toe: Option<&str>, origin: &str) -> cs_model::Code {
    cs_model::Code {
        code: code.into(),
        family: cs_model::family_for_toe(toe).into(),
        label: format!("{code} label"),
        definition: format!("{code} definition"),
        origin: origin.into(),
        toe: toe.map(str::to_string),
        active: true,
        in_prompt: origin != "legacy",
        research_questions: vec!["SQ1".into()],
        merged_into: None,
        version: 3,
        example: Some("an example".into()),
        reason: Some("a reason".into()),
        created_at: None,
    }
}

#[test]
fn json_import_takes_a_codebook() {
    let dir = TempDir::new().unwrap();
    let path = write(
        &dir,
        "codebook.json",
        &json_book(vec![
            json_code("TECH-ADV", Some("T"), "literature"),
            json_code("NEW-FIELD", None, "field"),
        ]),
    );
    let s = Store::open_in_memory().unwrap();
    let book = s.import_codebook_json(&path, "alice").unwrap();
    assert_eq!(book.codes.len(), 2);
    let adv = book.get("TECH-ADV").unwrap();
    assert_eq!(adv.toe.as_deref(), Some("T"));
    assert_eq!(adv.family, "TECH");
    assert_eq!(adv.version, 3);
    assert_eq!(adv.research_questions, ["SQ1"]);
    assert_eq!(adv.example.as_deref(), Some("an example"));
    assert_eq!(adv.reason.as_deref(), Some("a reason"));
    assert_eq!(s.codebook().unwrap().codes, book.codes);
    assert!(s
        .research_questions()
        .unwrap()
        .iter()
        .any(|q| q.id == "SQ1"));

    let events = import_actions(&s);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].detail["format"], "json");
    assert_eq!(events[0].detail["n_codes"], 2);
}

#[test]
fn json_codes_are_validated_like_csv_codes() {
    for bad in ["tech-adv", "1TECH", "TECH ADV", "NA", ""] {
        let dir = TempDir::new().unwrap();
        let s = Store::open_in_memory().unwrap();
        let path = write(
            &dir,
            "bad.json",
            &json_book(vec![json_code(bad, Some("T"), "literature")]),
        );
        let e = s.import_codebook_json(&path, ACTOR).unwrap_err();
        assert_eq!(kind(&e), "validation", "{bad:?}: {e}");
        assert!(s.codebook().unwrap().codes.is_empty());
    }
}

#[test]
fn json_duplicates_refuse_the_whole_file() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let path = write(
        &dir,
        "dup.json",
        &json_book(vec![
            json_code("TECH-ADV", Some("T"), "literature"),
            json_code("ORG-SKILLS", Some("O"), "literature"),
            json_code("TECH-ADV", Some("T"), "literature"),
        ]),
    );
    assert_refused_and_untouched(&s, s.import_codebook_json(&path, ACTOR), "duplicate");
}

#[test]
fn malformed_json_is_a_validation_error() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let path = write(&dir, "broken.json", "{\"source\": \"x\", \"codes\": [");
    assert_refused_and_untouched(&s, s.import_codebook_json(&path, ACTOR), "json");
    let path = write(&dir, "empty.json", "{\"source\": \"x\", \"codes\": []}");
    let e = s.import_codebook_json(&path, ACTOR).unwrap_err();
    assert_eq!(kind(&e), "validation", "{e}");
}

// ----------------------------------------------------- coding.db and export

#[test]
fn coding_db_import_still_works() {
    let dir = TempDir::new().unwrap();
    let s = Store::open_in_memory().unwrap();
    let book = s
        .import_codebook(&coding_db(&dir, &default_codes()), ACTOR)
        .unwrap();
    assert_eq!(book.codes.len(), 6);
    let ev = import_actions(&s);
    assert_eq!(ev.len(), 1);
    assert!(ev[0].detail.get("sha256").is_some());
}

/// A codebook with every kind of cell: commas, quotes and newlines in text,
/// two research questions, an inactive code, a code out of the prompt, a
/// local code with a creation time.
fn rich_store(dir: &TempDir) -> Store {
    let s = Store::open_in_memory().unwrap();
    s.import_codebook(&coding_db(dir, &default_codes()), ACTOR)
        .unwrap();
    s.set_code_in_prompt("ENV-PARTNER", false, ACTOR).unwrap();
    s.set_code_active("ORG-SKILLS", false, ACTOR).unwrap();
    let mut c = new_code("LEGAL-OBLIGATION", Some("E"));
    c.label = "Legal, obligation".into();
    c.definition = "A law, or a \"regulation\",\nrequires the identification.".into();
    c.research_questions = vec!["SQ1".into(), "SQ2".into()];
    c.example = Some("\"We had to, by law.\"".into());
    c.reason = Some("seen in ECO-16-U002".into());
    s.create_code(&c, ACTOR).unwrap();
    s
}

#[test]
fn csv_export_reimports_identically() {
    let dir = TempDir::new().unwrap();
    let source = rich_store(&dir);
    let original = source.codebook().unwrap();
    assert!(original.codes.len() >= 7);

    let (csv_text, _sql) = source.codebook_export().unwrap();
    let path = write(&dir, "export.csv", &csv_text);

    let fresh = Store::open_in_memory().unwrap();
    let again = fresh.import_codebook_csv(&path, ACTOR).unwrap();
    assert_eq!(again.codes, original.codes);
    assert_eq!(fresh.codebook().unwrap().codes, original.codes);

    // The research questions of the codes exist in the fresh store.
    let rq: Vec<String> = fresh
        .research_questions()
        .unwrap()
        .into_iter()
        .map(|q| q.id)
        .collect();
    for id in ["SQ1", "SQ2", "SQ3", "SQ4"] {
        assert!(rq.contains(&id.to_string()), "{id} in {rq:?}");
    }

    // A second export of the re-imported store is byte-identical.
    let (csv_again, _) = fresh.codebook_export().unwrap();
    assert_eq!(csv_again, csv_text);
}

#[test]
fn json_form_of_the_codebook_reimports_identically() {
    let dir = TempDir::new().unwrap();
    let source = rich_store(&dir);
    let original = source.codebook().unwrap();
    let path = write(
        &dir,
        "codebook.json",
        &serde_json::to_string(&original).unwrap(),
    );

    let fresh = Store::open_in_memory().unwrap();
    let again = fresh.import_codebook_json(&path, ACTOR).unwrap();
    assert_eq!(again.codes, original.codes);
    let (a, _) = source.codebook_export().unwrap();
    let (b, _) = fresh.codebook_export().unwrap();
    assert_eq!(a, b);
}
