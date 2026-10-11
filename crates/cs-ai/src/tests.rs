use std::collections::BTreeMap;

use cs_model::{
    Arbitration, ArbitrationChoice, Code, CodeSet, Codebook, CodingMap, DivergenceKind, FinalBasis,
    Segmentation, SpeakerRole, Unit,
};
use serde_json::{json, Value};

use super::*;

fn set(codes: &[&str]) -> CodeSet {
    codes.iter().map(|s| s.to_string()).collect()
}

fn unit(id: &str, seq: u32, role: SpeakerRole, raw: &str, text: &str) -> Unit {
    Unit {
        unit_id: format!("ECO-16-{id}"),
        seq,
        speaker_raw: raw.to_string(),
        role,
        timestamp: format!("00:{seq:02}"),
        text: text.to_string(),
    }
}

fn seg_n(n: u32) -> Segmentation {
    let units: Vec<Unit> = (1..=n)
        .map(|i| {
            let role = if i % 2 == 1 {
                SpeakerRole::Interviewer
            } else {
                SpeakerRole::Respondent
            };
            unit(
                &format!("U{i:03}"),
                i,
                role,
                if i % 2 == 1 { "INT" } else { "RESP" },
                &format!("text {i}"),
            )
        })
        .collect();
    Segmentation {
        interview_id: "ECO-16".into(),
        source_rel_path: "02_transcript_cleaned_diarised.md".into(),
        source_sha256: "0".repeat(64),
        rule: cs_model::SEGMENTATION_RULE.into(),
        hash: Segmentation::units_hash(&units),
        units,
        warnings: vec![],
    }
}

fn sample_seg() -> Segmentation {
    let units = vec![
        unit(
            "U001",
            1,
            SpeakerRole::Interviewer,
            "INT",
            "How did the pilot start?",
        ),
        unit(
            "U002",
            2,
            SpeakerRole::Respondent,
            "RESP",
            "The tags cost too much \u{2014} \"really\", so we stopped, then restarted.",
        ),
        unit(
            "U003",
            3,
            SpeakerRole::Unknown,
            "S1",
            "We had a champion in the IT team.",
        ),
    ];
    Segmentation {
        interview_id: "ECO-16".into(),
        source_rel_path: "02_transcript_cleaned_diarised.md".into(),
        source_sha256: "0".repeat(64),
        rule: cs_model::SEGMENTATION_RULE.into(),
        hash: Segmentation::units_hash(&units),
        units,
        warnings: vec![],
    }
}

fn code(c: &str, family: &str, origin: &str, def: &str) -> Code {
    Code {
        code: c.into(),
        family: family.into(),
        label: format!("{c} label"),
        definition: def.into(),
        origin: origin.into(),
        active: true,
        in_prompt: origin != "legacy",
    }
}

fn codebook() -> Codebook {
    Codebook {
        source: "05_Analysis/coding/coding.db".into(),
        codes: vec![
            code(
                "TECH-COST",
                "TECH",
                "codebook",
                "Lifecycle cost \u{2013} investment burden.",
            ),
            code("ORG-CHAMPION", "ORG", "codebook", "Champion continuity."),
            code("TECH", "TECH", "legacy", "Technology, unrefined."),
        ],
    }
}

fn answer(seg: &Segmentation, part: u32, of: u32, codings: Value) -> String {
    json!({
        "schema": OUTPUT_SCHEMA_ID,
        "interview_id": seg.interview_id,
        "segmentation_hash": seg.hash,
        "part": part,
        "of": of,
        "codings": codings,
    })
    .to_string()
}

fn valid_codings() -> Value {
    json!([
        {"unit_id": "ECO-16-U001", "codes": ["NA"]},
        {"unit_id": "ECO-16-U002", "codes": ["TECH-COST"]},
        {"unit_id": "ECO-16-U003", "codes": ["ORG-CHAMPION", "TECH-COST"]},
    ])
}

fn issues_of(raw: &str, expected: Option<(u32, u32)>) -> Vec<cs_model::ValidationIssue> {
    parse_ai_output(raw, &sample_seg(), &codebook(), expected).expect_err("should fail")
}

fn has(issues: &[cs_model::ValidationIssue], needle: &str) -> bool {
    issues.iter().any(|i| i.message.contains(needle))
}

// ------------------------------------------------------------------ prompts

#[test]
fn prompt_is_deterministic_and_has_no_dashes() {
    let a = build_prompts(&sample_seg(), &codebook(), 0);
    let b = build_prompts(&sample_seg(), &codebook(), 0);
    assert_eq!(a, b);
    assert_eq!(a.len(), 1);
    let p = &a[0];
    assert!(!p.text.contains('\u{2014}') && !p.text.contains('\u{2013}'));
    assert!(
        p.text.contains("\\u2014"),
        "dash in unit text kept as a JSON escape"
    );
    assert_eq!(p.sha256, cs_model::sha256_hex(p.text.as_bytes()));
    assert_eq!(p.prompt_version, PROMPT_VERSION);
    assert_eq!(p.codebook_hash, codebook().prompt_hash());
    assert_eq!(p.unit_ids.len(), 3);
}

#[test]
fn prompt_is_blind_and_closed() {
    let p = &build_prompts(&sample_seg(), &codebook(), 0)[0];
    assert!(p.text.contains("\"code\":\"TECH-COST\""));
    assert!(
        !p.text.contains("\"code\":\"TECH\""),
        "legacy code not offered"
    );
    assert!(
        p.text.contains("\"speaker\":\"S1\""),
        "unknown role keeps the source label"
    );
    assert!(p.text.contains("\"speaker\":\"INT\""));
    assert!(p.text.contains(&sample_seg().hash));
    // The embedded schema restricts the codes to the offered ones.
    assert!(p.text.contains("\"enum\""));
}

#[test]
fn embedded_dashes_decode_back_to_the_original_text() {
    let p = &build_prompts(&sample_seg(), &codebook(), 0)[0];
    let line = p
        .text
        .lines()
        .find(|l| l.contains("ECO-16-U002") && l.starts_with('{'))
        .unwrap();
    let v: Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["text"], sample_seg().units[1].text);
}

#[test]
fn part_splitting_edge_cases() {
    let cb = codebook();
    // n < batch, n == batch: one part.
    assert_eq!(build_prompts(&seg_n(3), &cb, 5).len(), 1);
    assert_eq!(build_prompts(&seg_n(5), &cb, 5).len(), 1);
    // n == batch + 1: two parts of ceil(6/2) = 3.
    let parts = build_prompts(&seg_n(6), &cb, 5);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].unit_ids.len(), 3);
    assert_eq!(parts[1].unit_ids.len(), 3);
    assert!(parts.iter().all(|p| p.of == 2));
    // Every split covers all units once, no empty part, no part over the batch size.
    for n in 1..=120u32 {
        let seg = seg_n(n);
        for batch in 0..=40usize {
            let parts = build_prompts(&seg, &cb, batch);
            let all: Vec<String> = parts.iter().flat_map(|p| p.unit_ids.clone()).collect();
            let expected: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
            assert_eq!(all, expected, "n={n} batch={batch}");
            assert!(
                parts.iter().all(|p| !p.unit_ids.is_empty()),
                "n={n} batch={batch}"
            );
            if batch > 0 {
                assert!(
                    parts.iter().all(|p| p.unit_ids.len() <= batch),
                    "n={n} batch={batch}"
                );
            }
            for p in &parts {
                assert_eq!(part_unit_ids(&seg, p.part, p.of), p.unit_ids);
            }
        }
    }
}

#[test]
fn unit_lines_keep_field_order_and_schema_is_valid_json() {
    let p = &build_prompts(&sample_seg(), &codebook(), 0)[0];
    assert!(p.text.contains(
        "{\"unit_id\":\"ECO-16-U003\",\"speaker\":\"S1\",\"timestamp\":\"00:03\",\"text\":\"We had a champion in the IT team.\"}"
    ));
    let schema_text = p.text.split("JSON Schema:\n\n").nth(1).unwrap();
    let schema: Value = serde_json::from_str(schema_text).unwrap();
    assert_eq!(schema["properties"]["interview_id"]["const"], "ECO-16");
    let allowed = &schema["properties"]["codings"]["items"]["properties"]["codes"]["items"]["enum"];
    assert_eq!(allowed, &json!(["TECH-COST", "ORG-CHAMPION", "NA"]));
}

#[test]
fn generic_schema_shape() {
    let s = output_json_schema();
    assert_eq!(s["properties"]["schema"]["const"], OUTPUT_SCHEMA_ID);
    assert_eq!(s["additionalProperties"], false);
}

// ------------------------------------------------------------------ parsing

#[test]
fn round_trip_prompt_then_answer() {
    let seg = seg_n(7);
    let cb = codebook();
    let parts = build_prompts(&seg, &cb, 3);
    assert_eq!(parts.len(), 3);
    for p in &parts {
        let codings: Vec<Value> = p
            .unit_ids
            .iter()
            .map(|id| json!({"unit_id": id, "codes": ["NA"]}))
            .collect();
        let raw = answer(&seg, p.part, p.of, Value::Array(codings));
        let out = parse_ai_output(&raw, &seg, &cb, Some((p.part, p.of))).unwrap();
        assert_eq!(out.codings.len(), p.unit_ids.len());
        assert_eq!((out.part, out.of), (p.part, p.of));
    }
}

#[test]
fn accepts_fenced_and_prose_wrapped_answers() {
    let seg = sample_seg();
    let body = answer(&seg, 1, 1, valid_codings());
    for raw in [
        format!("```json\n{body}\n```"),
        format!("Here is the coding:\n\n{body}\n\nHope it helps {{ok}}."),
        body.clone(),
    ] {
        let out = parse_ai_output(&raw, &seg, &codebook(), Some((1, 1))).unwrap();
        assert_eq!(
            out.codings["ECO-16-U003"],
            set(&["ORG-CHAMPION", "TECH-COST"])
        );
    }
    // Prose holding braces before the object is skipped.
    let raw = format!("Answer {{see below}}:\n{body}");
    assert!(parse_ai_output(&raw, &seg, &codebook(), None).is_ok());
}

#[test]
fn extractor_ignores_braces_inside_strings() {
    let raw = r#"x {"a": "close } open {", "b": {"c": 1}} tail }"#;
    assert_eq!(
        parse::extract_json_object(raw),
        Some(r#"{"a": "close } open {", "b": {"c": 1}}"#)
    );
}

#[test]
fn trims_whitespace_in_codes() {
    let seg = sample_seg();
    let raw = answer(
        &seg,
        1,
        1,
        json!([
            {"unit_id": "ECO-16-U001", "codes": [" NA "]},
            {"unit_id": "ECO-16-U002", "codes": ["TECH-COST"]},
            {"unit_id": "ECO-16-U003", "codes": ["ORG-CHAMPION"]},
        ]),
    );
    let out = parse_ai_output(&raw, &seg, &codebook(), None).unwrap();
    assert_eq!(out.codings["ECO-16-U001"], set(&["NA"]));
}

#[test]
fn reports_invalid_json_and_missing_object() {
    assert!(has(
        &issues_of("no json here", None),
        "no complete JSON object"
    ));
    assert!(has(&issues_of("{\"schema\": }", None), "invalid JSON"));
    assert!(has(
        &issues_of("{\"a\": [1, 2", None),
        "no complete JSON object"
    ));
}

#[test]
fn reports_envelope_mismatches_all_at_once() {
    let seg = sample_seg();
    let raw = json!({
        "schema": "other/v9",
        "interview_id": "RET-22",
        "segmentation_hash": "f".repeat(64),
        "part": 2,
        "of": 3,
        "codings": valid_codings(),
        "notes": "extra",
    })
    .to_string();
    let issues = parse_ai_output(&raw, &seg, &codebook(), Some((1, 1))).unwrap_err();
    assert!(has(&issues, "\"schema\" must be"));
    assert!(has(&issues, "expected \"ECO-16\""));
    assert!(has(&issues, "another segmentation"));
    assert!(has(&issues, "expected part 1 of 1"));
    assert!(has(&issues, "unexpected field \"notes\""));
}

#[test]
fn reports_missing_fields_and_bad_types() {
    let issues = issues_of("{\"codings\": 3, \"part\": 0}", None);
    assert!(has(&issues, "missing field \"schema\""));
    assert!(has(&issues, "missing field \"interview_id\""));
    assert!(has(&issues, "missing field \"of\""));
    assert!(has(&issues, "integer of 1 or more"));
    assert!(has(&issues, "must be an array"));
}

#[test]
fn reports_unit_level_problems() {
    let seg = sample_seg();
    let raw = answer(
        &seg,
        1,
        1,
        json!([
            {"unit_id": "ECO-16-U001", "codes": ["NA", "TECH-COST"]},
            {"unit_id": "ECO-16-U001", "codes": ["NA"]},
            {"unit_id": "ECO-16-U002", "codes": []},
            {"unit_id": "ECO-16-U999", "codes": ["NA"]},
            {"unit_id": "ECO-16-U003", "codes": ["TECH", "MADE-UP", "", 7], "why": "x"},
            "not an object",
        ]),
    );
    let issues = parse_ai_output(&raw, &seg, &codebook(), None).unwrap_err();
    assert!(has(&issues, "NA cannot be combined"));
    assert!(has(&issues, "more than once"));
    assert!(has(&issues, "empty codes array"));
    assert!(has(&issues, "unknown unit_id"));
    assert!(has(&issues, "not offered in the prompt"));
    assert!(has(&issues, "\"MADE-UP\" is not in the codebook"));
    assert!(has(&issues, "empty code"));
    assert!(has(&issues, "is not a string"));
    assert!(has(&issues, "unexpected field \"why\""));
    assert!(has(&issues, "must be an object"));
}

#[test]
fn reports_missing_units_and_units_of_another_part() {
    let seg = seg_n(6);
    let cb = codebook();
    // Part 1 of 2 covers U001..U003; U004 belongs to part 2; U003 is missing.
    let raw = answer(
        &seg,
        1,
        2,
        json!([
            {"unit_id": "ECO-16-U001", "codes": ["NA"]},
            {"unit_id": "ECO-16-U002", "codes": ["NA"]},
            {"unit_id": "ECO-16-U004", "codes": ["NA"]},
        ]),
    );
    let issues = parse_ai_output(&raw, &seg, &cb, Some((1, 2))).unwrap_err();
    assert!(has(&issues, "does not belong to this part"));
    assert!(has(&issues, "1 unit(s) missing: ECO-16-U003"));
}

// ---------------------------------------------------------------- agreement

fn map(entries: &[(&str, &[&str])]) -> CodingMap {
    entries
        .iter()
        .map(|(u, c)| (u.to_string(), set(c)))
        .collect()
}

#[test]
fn kappa_matches_hand_computation() {
    // a = [1,1,0,0,1], b = [1,0,0,0,1]: po = 0.8, pa = 0.6, pb = 0.4,
    // pe = 0.24 + 0.24 = 0.48, kappa = 0.32 / 0.52 = 0.615384...
    let a = [true, true, false, false, true];
    let b = [true, false, false, false, true];
    let k = cohen_kappa(&a, &b).unwrap();
    assert!((k - 0.32 / 0.52).abs() < 1e-12);
    // A single level observed: undefined.
    assert_eq!(cohen_kappa(&[true, true], &[true, true]), None);
    assert_eq!(cohen_kappa(&[false, false], &[false, false]), None);
    assert_eq!(cohen_kappa(&[], &[]), None);
}

#[test]
fn agreement_ports_the_python_script() {
    let order: Vec<String> = ["U1", "U2", "U3", "U4", "U5"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let human = map(&[
        ("U1", &["A"]),
        ("U2", &["A", "B"]),
        ("U3", &["NA"]),
        ("U4", &["B"]),
        ("U5", &["A"]),
    ]);
    // U5 missing on the AI side: skipped.
    let ai = map(&[
        ("U1", &["A"]),
        ("U2", &["A"]),
        ("U3", &["NA"]),
        ("U4", &["A"]),
    ]);
    let r = agreement("human", &human, "R1", &ai, &order);
    assert_eq!(r.n_units, 4);
    assert_eq!(r.n_exact, 2);
    assert_eq!(
        r.divergent_unit_ids,
        vec!["U2".to_string(), "U4".to_string()]
    );
    let codes: Vec<&str> = r.per_code.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(codes, vec!["A", "B", "NA"]);
    let a = &r.per_code[0];
    // A: human [1,1,0,0], ai [1,1,0,1] -> raw 0.75, po .75, pa .5, pb .75, pe .5, kappa .5
    assert_eq!((a.n_a, a.n_b), (2, 3));
    assert!((a.raw - 0.75).abs() < 1e-12);
    assert!((a.kappa.unwrap() - 0.5).abs() < 1e-12);
    let b = &r.per_code[1];
    // B: human [0,1,0,1], ai [0,0,0,0] -> pb = 0, pe = .5, po = .5, kappa 0
    assert!((b.kappa.unwrap() - 0.0).abs() < 1e-12);
}

// ------------------------------------------------- divergences, final coding

fn arb(unit_id: &str, codes: &[&str]) -> Arbitration {
    Arbitration {
        unit_id: unit_id.into(),
        choice: ArbitrationChoice::Custom,
        final_codes: set(codes),
        rationale: "Respondent states the cost | explicitly.".into(),
        decided_by: "candidate".into(),
        decided_at: "2026-10-10T20:00:00Z".into(),
    }
}

fn three_unit_case() -> (Segmentation, CodingMap, Vec<(String, CodingMap)>) {
    let seg = sample_seg();
    let human = map(&[
        ("ECO-16-U001", &["NA"]),
        ("ECO-16-U002", &["TECH-COST"]),
        ("ECO-16-U003", &["ORG-CHAMPION"]),
    ]);
    let r1 = map(&[
        ("ECO-16-U001", &["NA"]),
        ("ECO-16-U002", &["TECH-COST"]),
        ("ECO-16-U003", &["TECH-COST"]),
    ]);
    let r2 = map(&[
        ("ECO-16-U001", &["NA"]),
        ("ECO-16-U002", &["NA"]),
        ("ECO-16-U003", &["TECH-COST"]),
    ]);
    (seg, human, vec![("R1".into(), r1), ("R2".into(), r2)])
}

#[test]
fn divergence_kinds() {
    let (seg, human, runs) = three_unit_case();
    let d = divergences(&seg, &human, &runs);
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].unit_id, "ECO-16-U002");
    assert_eq!(d[0].kind, DivergenceKind::AiRunsUnstable);
    assert_eq!(d[1].unit_id, "ECO-16-U003");
    assert_eq!(d[1].kind, DivergenceKind::HumanVsAi);
    assert_eq!(d[1].ai["R2"], set(&["TECH-COST"]));
    // No runs: nothing to compare.
    assert!(divergences(&seg, &human, &[]).is_empty());
    // A unit missing in a run is incomplete, not divergent.
    let mut partial = runs.clone();
    partial[1].1.remove("ECO-16-U003");
    assert_eq!(divergences(&seg, &human, &partial).len(), 1);
}

#[test]
fn final_coding_requires_every_arbitration() {
    let (seg, human, runs) = three_unit_case();
    let mut arbs = BTreeMap::new();
    arbs.insert(
        "ECO-16-U003".to_string(),
        arb("ECO-16-U003", &["ORG-CHAMPION"]),
    );
    assert_eq!(
        final_coding(&seg, &human, &runs, &arbs).unwrap_err(),
        vec!["ECO-16-U002".to_string()]
    );
    arbs.insert(
        "ECO-16-U002".to_string(),
        arb("ECO-16-U002", &["TECH-COST"]),
    );
    let finals = final_coding(&seg, &human, &runs, &arbs).unwrap();
    assert_eq!(finals.len(), 3);
    assert_eq!(finals[0].basis, FinalBasis::Agreed);
    assert_eq!(finals[1].basis, FinalBasis::Arbitrated);
    assert_eq!(finals[2].codes, set(&["ORG-CHAMPION"]));
}

#[test]
fn final_coding_lists_uncoded_units() {
    let (seg, mut human, mut runs) = three_unit_case();
    human.remove("ECO-16-U001");
    runs[0].1.remove("ECO-16-U002");
    let err = final_coding(&seg, &human, &runs, &BTreeMap::new()).unwrap_err();
    assert_eq!(
        err,
        vec![
            "ECO-16-U001".to_string(),
            "ECO-16-U002".to_string(),
            "ECO-16-U003".to_string()
        ]
    );
}

// ------------------------------------------------------------- CSV, reports

#[test]
fn csv_quoting_and_formats() {
    let seg = sample_seg();
    let csv = units_csv(&seg);
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("unit_id,speaker,timestamp,text"));
    assert!(csv.contains("ECO-16-U002,RESP,00:02,\"The tags cost too much \u{2014} \"\"really\"\", so we stopped, then restarted.\""));
    assert!(csv.contains("ECO-16-U003,S1,00:03,We had a champion in the IT team.\n"));
    assert!(csv.ends_with('\n'));

    let (_, human, _) = three_unit_case();
    let order: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let c = coding_csv(
        &map(&[("ECO-16-U002", &["TECH-COST", "ORG-CHAMPION"])]),
        &order,
    );
    assert_eq!(c, "unit_id,codes\nECO-16-U002,ORG-CHAMPION;TECH-COST\n");
    assert!(coding_csv(&human, &order).starts_with("unit_id,codes\nECO-16-U001,NA\n"));

    let finals = vec![cs_model::FinalCoding {
        unit_id: "ECO-16-U001".into(),
        codes: set(&["NA"]),
        basis: FinalBasis::Arbitrated,
    }];
    assert_eq!(
        final_csv(&finals),
        "unit_id,codes,basis\nECO-16-U001,NA,arbitrated\n"
    );
}

#[test]
fn reports_are_deterministic_and_dash_free() {
    let (seg, human, runs) = three_unit_case();
    let order: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let reports: Vec<_> = runs
        .iter()
        .map(|(l, m)| agreement("human", &human, l, m, &order))
        .collect();
    // Template text is checked on units without dashes.
    let mut clean = seg.clone();
    clean.units[1].text = "The tags cost too much, so we stopped.".into();
    let md = render_agreement_md(&clean, &reports);
    assert_eq!(md, render_agreement_md(&clean, &reports));
    assert!(!md.contains('\u{2014}') && !md.contains('\u{2013}'));
    assert!(md.contains("## human against R1"));
    assert!(md.contains("| Exact agreement on the full code set | 2 (66.7 %) |"));
    assert!(md.contains("undefined") || md.contains("0."));

    let d = divergences(&clean, &human, &runs);
    let mut arbs = BTreeMap::new();
    arbs.insert(
        "ECO-16-U003".to_string(),
        arb("ECO-16-U003", &["ORG-CHAMPION"]),
    );
    let adj = render_adjudication_md(&clean, &d, &arbs);
    assert!(!adj.contains('\u{2014}') && !adj.contains('\u{2013}'));
    assert!(adj.contains("| Divergences | 2 |"));
    assert!(adj.contains("| Arbitrated | 1 |"));
    assert!(adj.contains("| Pending | 1 |"));
    assert!(adj.contains("Pending: no decision recorded."));
    assert!(adj.contains("Rationale: Respondent states the cost | explicitly."));
    assert!(adj.contains("| AI R2 | TECH-COST |"));
}

#[test]
#[ignore = "prints a sample prompt; run with --ignored --nocapture"]
fn print_sample_prompt() {
    let p = &build_prompts(&sample_seg(), &codebook(), 0)[0];
    println!("{}", p.text);
}
