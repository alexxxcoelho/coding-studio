use std::collections::{BTreeMap, BTreeSet};

use cs_model::{
    Arbitration, ArbitrationChoice, Code, CodeSet, CodingMap, DivergenceKind, FinalBasis,
    ReviewItem, ReviewKind, Segmentation, SpeakerRole, Unit,
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
        corrected: false,
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

fn code(c: &str, toe: Option<&str>, def: &str) -> Code {
    Code {
        code: c.into(),
        family: cs_model::family_for_toe(toe).into(),
        label: format!("{c} label"),
        definition: def.into(),
        origin: "literature".into(),
        toe: toe.map(str::to_string),
        active: true,
        in_prompt: true,
        research_questions: vec![],
        merged_into: None,
        version: 1,
        example: None,
        reason: None,
        created_at: None,
    }
}

/// Codes offered: one per TOE group, the last not attached.
fn codes() -> Vec<Code> {
    vec![
        code(
            "COST-NO-BENEFIT",
            None,
            "The payer bears the cost \u{2013} others get the benefit.",
        ),
        code(
            "TECH-COST",
            Some("T"),
            "Lifecycle cost and investment burden.",
        ),
        code(
            "LEGAL-OBLIGATION",
            Some("E"),
            "A law or regulation requires it.",
        ),
        code("ORG-CHAMPION", Some("O"), "Champion continuity."),
    ]
}

fn ids(seg: &Segmentation) -> Vec<String> {
    seg.units.iter().map(|u| u.unit_id.clone()).collect()
}

fn prompts_with(seg: &Segmentation, codes: &[Code], batch: usize) -> Vec<cs_model::PromptPart> {
    let ids = ids(seg);
    build_prompts(
        &PromptInput {
            seg,
            unit_ids: &ids,
            codes,
        },
        batch,
    )
}

fn offered() -> BTreeSet<String> {
    codes().into_iter().map(|c| c.code).collect()
}

fn parse_in(
    raw: &str,
    seg: &Segmentation,
    expected: Option<(u32, u32)>,
) -> Result<cs_model::AiOutput, Vec<cs_model::ValidationIssue>> {
    let ids = ids(seg);
    let codes = offered();
    parse_ai_output(
        raw,
        &AnswerScope {
            seg,
            unit_ids: &ids,
            codes: &codes,
        },
        expected,
    )
}

fn answer(seg: &Segmentation, part: u32, of: u32, codings: Value) -> String {
    answer_phase(seg, 2, part, of, codings)
}

fn answer_phase(seg: &Segmentation, phase: u8, part: u32, of: u32, codings: Value) -> String {
    json!({
        "schema": OUTPUT_SCHEMA_ID,
        "interview_id": seg.interview_id,
        "phase": phase,
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
    parse_in(raw, &sample_seg(), expected).expect_err("should fail")
}

fn has(issues: &[cs_model::ValidationIssue], needle: &str) -> bool {
    issues.iter().any(|i| i.message.contains(needle))
}

fn no_dash(text: &str) -> bool {
    !text.contains('\u{2014}') && !text.contains('\u{2013}')
}

// ------------------------------------------------------------------ prompts

#[test]
fn prompt_is_deterministic_and_has_no_dashes() {
    let a = prompts_with(&sample_seg(), &codes(), 0);
    assert_eq!(a, prompts_with(&sample_seg(), &codes(), 0));
    assert_eq!(a.len(), 1);
    let p = &a[0];
    assert!(no_dash(&p.text));
    assert!(
        p.text.contains("\\u2014"),
        "dash in unit text kept as a JSON escape"
    );
    assert_eq!(p.sha256, cs_model::sha256_hex(p.text.as_bytes()));
    assert_eq!(p.prompt_version, PROMPT_VERSION);
    assert_eq!(p.phase, 2);
    assert_eq!(p.batch_id, 0, "assigned by the store");
    assert_eq!(p.unit_ids.len(), 3);
}

#[test]
fn codes_are_grouped_by_toe_and_snapshotted() {
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
    assert_eq!(
        p.codes,
        [
            "TECH-COST",
            "ORG-CHAMPION",
            "LEGAL-OBLIGATION",
            "COST-NO-BENEFIT"
        ]
    );
    let t = p.text.find("### Technological context (T)").unwrap();
    let o = p.text.find("### Organisational context (O)").unwrap();
    let e = p
        .text
        .find("### Environmental and interorganisational context (E)")
        .unwrap();
    let n = p.text.find("### Not attached to a TOE context").unwrap();
    assert!(t < o && o < e && e < n);
    assert!(p
        .text
        .contains("These 4 codes and NA are the only valid values."));
    // An empty group is not printed.
    let only_t: Vec<Code> = codes()
        .into_iter()
        .filter(|c| c.toe.as_deref() == Some("T"))
        .collect();
    let q = &prompts_with(&sample_seg(), &only_t, 0)[0];
    assert!(!q.text.contains("### Organisational context"));
}

#[test]
fn prompt_is_blind() {
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
    assert!(p.text.contains("\"code\":\"TECH-COST\""));
    assert!(
        p.text.contains("\"speaker\":\"S1\""),
        "unknown role keeps the source label"
    );
    assert!(p.text.contains("\"speaker\":\"INT\""));
    assert!(p.text.contains(&sample_seg().hash));
    // No phase 1 context, no allowed list, no candidate material.
    for absent in [
        "allowed_codes",
        "phase1",
        "CONTEXT",
        "INDUCTIVE-PENDING",
        "to clarify",
        "revised",
    ] {
        assert!(!p.text.contains(absent), "{absent}");
    }
}

#[test]
fn embedded_dashes_decode_back_to_the_original_text() {
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
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
    let cb = codes();
    assert_eq!(prompts_with(&seg_n(3), &cb, 5).len(), 1);
    assert_eq!(prompts_with(&seg_n(5), &cb, 5).len(), 1);
    let parts = prompts_with(&seg_n(6), &cb, 5);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].unit_ids.len(), 3);
    assert_eq!(parts[1].unit_ids.len(), 3);
    assert!(parts.iter().all(|p| p.of == 2));
    for n in 1..=120u32 {
        let seg = seg_n(n);
        for batch in 0..=40usize {
            let parts = prompts_with(&seg, &cb, batch);
            let all: Vec<String> = parts.iter().flat_map(|p| p.unit_ids.clone()).collect();
            assert_eq!(all, ids(&seg), "n={n} batch={batch}");
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
                assert_eq!(part_unit_ids(&ids(&seg), p.part, p.of), p.unit_ids);
            }
            let (units, offered) = batch_scope(&parts);
            assert_eq!(units, ids(&seg));
            assert_eq!(offered.len(), 4);
        }
    }
}

#[test]
fn unit_lines_keep_field_order_and_schema_is_valid_json() {
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
    assert!(p.text.contains(
        "{\"unit_id\":\"ECO-16-U003\",\"speaker\":\"S1\",\"timestamp\":\"00:03\",\"text\":\"We had a champion in the IT team.\"}"
    ));
    let schema_text = p.text.split("JSON Schema:\n\n").nth(1).unwrap();
    let schema: Value = serde_json::from_str(schema_text).unwrap();
    assert_eq!(schema["properties"]["interview_id"]["const"], "ECO-16");
    assert_eq!(schema["properties"]["phase"]["const"], 2);
    let allowed = &schema["properties"]["codings"]["items"]["properties"]["codes"]["items"]["enum"];
    assert_eq!(
        allowed,
        &json!([
            "TECH-COST",
            "ORG-CHAMPION",
            "LEGAL-OBLIGATION",
            "COST-NO-BENEFIT",
            "NA"
        ])
    );
}

#[test]
fn generic_schema_shape() {
    let s = output_json_schema();
    assert_eq!(s["properties"]["schema"]["const"], OUTPUT_SCHEMA_ID);
    assert_eq!(s["properties"]["phase"]["const"], 2);
    assert!(s["required"].as_array().unwrap().contains(&json!("phase")));
    assert_eq!(s["additionalProperties"], false);
}

// ------------------------------------------------------------------ parsing

#[test]
fn round_trip_prompt_then_answer() {
    let seg = seg_n(7);
    let parts = prompts_with(&seg, &codes(), 3);
    assert_eq!(parts.len(), 3);
    for p in &parts {
        let codings: Vec<Value> = p
            .unit_ids
            .iter()
            .map(|id| json!({"unit_id": id, "codes": ["NA"]}))
            .collect();
        let raw = answer(&seg, p.part, p.of, Value::Array(codings));
        let out = parse_in(&raw, &seg, Some((p.part, p.of))).unwrap();
        assert_eq!(out.codings.len(), p.unit_ids.len());
        assert_eq!((out.part, out.of, out.phase), (p.part, p.of, 2));
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
        let out = parse_in(&raw, &seg, Some((1, 1))).unwrap();
        assert_eq!(
            out.codings["ECO-16-U003"],
            set(&["ORG-CHAMPION", "TECH-COST"])
        );
    }
    // Prose holding braces before the object is skipped.
    let raw = format!("Answer {{see below}}:\n{body}");
    assert!(parse_in(&raw, &seg, None).is_ok());
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
    let out = parse_in(&raw, &seg, None).unwrap();
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
        "phase": 1,
        "segmentation_hash": "f".repeat(64),
        "part": 2,
        "of": 3,
        "codings": valid_codings(),
        "notes": "extra",
    })
    .to_string();
    let issues = parse_in(&raw, &seg, Some((1, 1))).unwrap_err();
    assert!(has(&issues, "\"schema\" must be"));
    assert!(has(&issues, "expected \"ECO-16\""));
    assert!(has(&issues, "another segmentation"));
    assert!(has(&issues, "only phase 2 is coded by the AI"));
    assert!(has(&issues, "expected part 1 of 1"));
    assert!(has(&issues, "unexpected field \"notes\""));
}

#[test]
fn reports_missing_fields_and_bad_types() {
    let issues = issues_of("{\"codings\": 3, \"part\": 0}", None);
    assert!(has(&issues, "missing field \"schema\""));
    assert!(has(&issues, "missing field \"interview_id\""));
    assert!(has(&issues, "missing field \"phase\""));
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
            {"unit_id": "ECO-16-U003", "codes": ["CONTEXT", "", 7], "why": "x"},
            "not an object",
        ]),
    );
    let issues = parse_in(&raw, &seg, None).unwrap_err();
    assert!(has(&issues, "NA cannot be combined"));
    assert!(has(&issues, "more than once"));
    assert!(has(&issues, "empty codes array"));
    assert!(has(&issues, "unknown unit_id"));
    assert!(has(&issues, "\"CONTEXT\" was not offered in the prompt"));
    assert!(has(&issues, "empty code"));
    assert!(has(&issues, "is not a string"));
    assert!(has(&issues, "unexpected field \"why\""));
    assert!(has(&issues, "must be an object"));
}

#[test]
fn units_outside_the_batch_are_rejected() {
    let seg = sample_seg();
    let scope_ids = vec!["ECO-16-U002".to_string()];
    let codes = offered();
    let raw = answer(
        &seg,
        1,
        1,
        json!([
            {"unit_id": "ECO-16-U001", "codes": ["NA"]},
            {"unit_id": "ECO-16-U002", "codes": ["NA"]},
        ]),
    );
    let issues = parse_ai_output(
        &raw,
        &AnswerScope {
            seg: &seg,
            unit_ids: &scope_ids,
            codes: &codes,
        },
        None,
    )
    .unwrap_err();
    assert!(has(&issues, "not in the scope of this prompt"));
}

#[test]
fn reports_missing_units_and_units_of_another_part() {
    let seg = seg_n(6);
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
    let issues = parse_in(&raw, &seg, Some((1, 2))).unwrap_err();
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

fn arb(unit_id: &str, codes: &[&str], rationale: &str) -> Arbitration {
    Arbitration {
        unit_id: unit_id.into(),
        phase: 2,
        choice: ArbitrationChoice::Custom,
        final_codes: set(codes),
        rationale: rationale.into(),
        decided_by: "candidate".into(),
        decided_at: "2026-10-12T20:00:00Z".into(),
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
    let d = divergences(&ids(&seg), &human, &runs);
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].kind, DivergenceKind::AiRunsUnstable);
    assert_eq!(d[1].kind, DivergenceKind::HumanVsAi);
    assert!(divergences(&ids(&seg), &human, &[]).is_empty());
    let mut partial = runs.clone();
    partial[1].1.remove("ECO-16-U003");
    assert_eq!(divergences(&ids(&seg), &human, &partial).len(), 1);
}

#[test]
fn final_coding_takes_decisions_then_the_candidate() {
    let (seg, mut human, _) = three_unit_case();
    human.remove("ECO-16-U001");
    let mut decisions = BTreeMap::new();
    decisions.insert(
        "ECO-16-U003".to_string(),
        arb("ECO-16-U003", &["TECH-COST"], "r"),
    );
    let (finals, uncoded) = final_coding(&ids(&seg), &human, &decisions);
    assert_eq!(uncoded, ["ECO-16-U001"]);
    assert_eq!(finals[0].basis, FinalBasis::Candidate);
    assert_eq!(finals[1].basis, FinalBasis::Reviewed);
    assert_eq!(finals[1].codes, set(&["TECH-COST"]));
}

// ------------------------------------------------------------- CSV, reports

#[test]
fn csv_quoting_and_formats() {
    let seg = sample_seg();
    let csv = units_csv(&seg);
    assert!(csv.starts_with("unit_id,speaker,timestamp,text\n"));
    assert!(csv.contains("ECO-16-U002,RESP,00:02,\"The tags cost too much \u{2014} \"\"really\"\", so we stopped, then restarted.\""));
    assert!(csv.contains("ECO-16-U003,S1,00:03,We had a champion in the IT team.\n"));
    let order = ids(&seg);
    let c = coding_csv(
        &map(&[("ECO-16-U002", &["TECH-COST", "ORG-CHAMPION"])]),
        &order,
    );
    assert_eq!(c, "unit_id,codes\nECO-16-U002,ORG-CHAMPION;TECH-COST\n");
    let finals = vec![cs_model::FinalCoding {
        unit_id: "ECO-16-U001".into(),
        codes: set(&["NA"]),
        basis: FinalBasis::Reviewed,
    }];
    assert_eq!(
        final_csv(&finals),
        "unit_id,codes,basis\nECO-16-U001,NA,reviewed\n"
    );
}

#[test]
fn reports_are_deterministic_and_dash_free() {
    let (seg, human, runs) = three_unit_case();
    let order = ids(&seg);
    let reports: Vec<_> = runs
        .iter()
        .map(|(l, m)| agreement("candidate", &human, l, m, &order))
        .collect();
    let mut clean = seg.clone();
    clean.units[1].text = "The tags cost too much, so we stopped.".into();
    let md = render_agreement_md(&clean, &order, &reports);
    assert_eq!(md, render_agreement_md(&clean, &order, &reports));
    assert!(md.starts_with("# Inter-coder agreement report, ECO-16, Phase 2, concrete codes"));
    assert!(md.contains("| Units compared with the AI | 3 |"));
    assert!(md.contains("reported, not optimised"));
    assert!(no_dash(&md));
    assert!(md.contains("| Exact agreement on the full code set | 2 (66.7 %) |"));

    let items = vec![
        ReviewItem {
            unit_id: "ECO-16-U003".into(),
            kind: ReviewKind::Disagreement,
            human: set(&["ORG-CHAMPION"]),
            human_current: set(&["ORG-CHAMPION"]),
            revised_since_snapshot: false,
            ai: [("R1".to_string(), set(&["TECH-COST"]))]
                .into_iter()
                .collect(),
            decision: Some(arb(
                "ECO-16-U003",
                &["ORG-CHAMPION"],
                "Respondent names the champion | explicitly.",
            )),
        },
        ReviewItem {
            unit_id: "ECO-16-U002".into(),
            kind: ReviewKind::AgreementSample,
            human: set(&["TECH-COST"]),
            human_current: set(&["TECH-COST"]),
            revised_since_snapshot: false,
            ai: [("R1".to_string(), set(&["TECH-COST"]))]
                .into_iter()
                .collect(),
            decision: None,
        },
    ];
    let history = vec![
        arb("ECO-16-U003", &["TECH-COST"], "first"),
        arb("ECO-16-U003", &["ORG-CHAMPION"], "second"),
    ];
    let seeds = vec![("R1".to_string(), "abcdef0123456789".to_string())];
    let log = render_review_md(&clean, &items, &history, &seeds);
    assert!(log.starts_with("# Review log, ECO-16, Phase 2, concrete codes"));
    assert!(no_dash(&log));
    assert!(log.contains("| Disagreements | 1 |"));
    assert!(log.contains("| Sampled agreements | 1 |"));
    assert!(log.contains("| Decided | 1 |"));
    assert!(log.contains("`abcdef0123456789`"));
    assert!(log.contains("sampled agreement"));
    assert!(log.contains("Pending: no decision recorded."));
    assert!(log.contains("Earlier decisions on this unit: 1."));
    assert!(log.contains("| AI R1 | TECH-COST |"));
}

#[test]
#[ignore = "prints a sample prompt; run with --ignored --nocapture"]
fn print_sample_prompt() {
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
    println!("{}", p.text);
}

// ------------------------------------------------------------ redaction

/// Slice of `text` from the first `from` up to the next `to`.
fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let a = text.find(from).expect(from);
    let b = a + text[a..].find(to).expect(to);
    &text[a..b]
}

#[test]
fn prompt_redacts_unit_ids_in_code_text() {
    let mut cs = codes();
    // Printed fields (label, definition) and the unprinted ones (example,
    // reason) all carry ids of this interview and of another one.
    cs[1].label = "Cost, from ECO-16-U007".into();
    cs[1].definition = "Lifecycle cost, see ECO-16-U002 and ECO-16-U007 and RET-22-U001.".into();
    cs[2].example = Some("see ECO-16-U002 and ECO-16-U007 and RET-22-U001".into());
    cs[2].reason = Some("see ECO-16-U002 and ECO-16-U007 and RET-22-U001".into());
    let p = &prompts_with(&sample_seg(), &cs, 0)[0];

    let codebook = section(&p.text, "## Codebook", "## Rules");
    for id in ["ECO-16-U002", "ECO-16-U007", "RET-22-U001"] {
        assert!(!codebook.contains(id), "{id} leaked into the codebook");
    }
    assert!(codebook.contains("[unit]"));
    assert!(
        codebook.contains("\"definition\":\"Lifecycle cost, see [unit] and [unit] and [unit].\"")
    );
    assert!(codebook.contains("\"label\":\"Cost, from [unit]\""));

    // The prompted units keep their ids in the units section.
    let units = section(&p.text, "## Units", "## Output");
    for id in ["ECO-16-U001", "ECO-16-U002", "ECO-16-U003"] {
        assert!(units.contains(id), "{id} missing from the units");
    }
    assert!(!units.contains("[unit]"));
}

#[test]
fn unit_id_redaction_matches_the_pattern_only() {
    use crate::prompt::redact_unit_ids as r;
    assert_eq!(r("ECO-16-U002"), "[unit]");
    assert_eq!(r("ECO-16-U002 and RET-22-U001."), "[unit] and [unit].");
    // Four letters are the most a prefix may have: the match starts at X.
    assert_eq!(r("XECO-16-U0071x"), "[unit]x");
    // Five letters: the match starts at the second letter.
    assert_eq!(r("ABCDE-1-U1"), "A[unit]");
    // Digits: at most three before the dash, at most four after U.
    assert_eq!(r("ECO-1234-U001"), "ECO-1234-U001");
    assert_eq!(r("ECO-16-U00712"), "[unit]2");
    // Not ids: no digits, lowercase, a missing U or a missing digit.
    assert_eq!(r("ECO-16-U"), "ECO-16-U");
    assert_eq!(r("eco-16-u002"), "eco-16-u002");
    assert_eq!(r("ECO-16-002"), "ECO-16-002");
    assert_eq!(r("ECO-U002"), "ECO-U002");
    assert!(no_dash(&r("plain text")));
}

#[test]
fn prompt_contains_no_human_material() {
    // The builder takes only the segmentation, the unit ids in scope and the
    // codes offered (`PromptInput`): no memo, claim, flag, label to clarify,
    // excerpt or phase 1 context can reach the text. The check is on the
    // section structure of the output, which must be the template's own.
    let p = &prompts_with(&sample_seg(), &codes(), 0)[0];
    let headers: Vec<&str> = p.text.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headers,
        [
            "## Role",
            "## Task",
            "## Codebook",
            "## Rules",
            "## Units",
            "## Output"
        ]
    );
    // "memo" is not a sentinel: the rule "Write no ... memo" is in the template.
    for absent in [
        "ZZSENTINEL",
        "EV1",
        "excerpt",
        "claim",
        "to clarify",
        "flag",
    ] {
        assert!(!p.text.contains(absent), "{absent}");
    }
}
