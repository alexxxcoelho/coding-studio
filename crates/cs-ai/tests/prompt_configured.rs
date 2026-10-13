//! V1 lot 4: the prompt built from the configuration of the study, its
//! pseudonymisation, the candidate concepts channel of the answer.

use std::collections::BTreeMap;

use cs_ai::{
    build_prompts_configured, build_prompts_with, check_candidate_quotes, parse_ai_output,
    AnswerScope, PromptContext, PromptInput, TreeCode,
};
use cs_model::pseudonym::{PseudonymConfig, PseudonymEntry};
use cs_model::{Code, PromptTemplate, Segmentation, SpeakerRole, Unit};

fn unit(seq: u32, role: SpeakerRole, text: &str) -> Unit {
    Unit {
        unit_id: format!("ECO-16-U{seq:03}"),
        seq,
        speaker_raw: if role == SpeakerRole::Interviewer {
            "INT".into()
        } else {
            "RESP".into()
        },
        role,
        timestamp: format!("00:{seq:02}"),
        text: text.into(),
        corrected: false,
    }
}

fn seg() -> Segmentation {
    let units = vec![
        unit(
            1,
            SpeakerRole::Interviewer,
            "Pourquoi Anna Muller a-t-elle arrêté ?",
        ),
        unit(
            2,
            SpeakerRole::Respondent,
            "Anna Muller a trouvé que la Banque Alpha coûtait trop cher.",
        ),
        unit(3, SpeakerRole::Respondent, "Nous manquons de compétences."),
        unit(4, SpeakerRole::Interviewer, "Et ensuite ?"),
        unit(5, SpeakerRole::Respondent, "Le partenaire a insisté."),
    ];
    Segmentation {
        interview_id: "ECO-16".into(),
        source_rel_path: "t.md".into(),
        source_sha256: "0".repeat(64),
        rule: cs_model::SEGMENTATION_RULE.into(),
        hash: Segmentation::units_hash(&units),
        units,
        warnings: vec![],
    }
}

fn code(c: &str, def: &str) -> Code {
    Code {
        code: c.into(),
        family: "OTHER".into(),
        label: c.into(),
        definition: def.into(),
        origin: "field".into(),
        toe: None,
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

fn tree(token: &str, name: &str, parent: Option<&str>, def: &str) -> TreeCode {
    TreeCode {
        code: token.into(),
        name: name.into(),
        parent: parent.map(str::to_string),
        definition: def.into(),
        includes: format!("inclut {name}"),
        excludes: "n'inclut pas le coût".into(),
        example: None,
    }
}

fn ctx() -> PromptContext {
    let list = PseudonymConfig {
        enabled: true,
        entries: vec![
            PseudonymEntry {
                kind: "person".into(),
                original: "Anna Muller".into(),
                pseudonym: "P1".into(),
            },
            PseudonymEntry {
                kind: "organisation".into(),
                original: "Banque Alpha".into(),
                pseudonym: "ORG1".into(),
            },
        ],
    };
    let mut preceding = BTreeMap::new();
    preceding.insert(
        "ECO-16-U002".to_string(),
        "Pourquoi Anna Muller a-t-elle arrêté ?".to_string(),
    );
    preceding.insert("ECO-16-U005".to_string(), "Et ensuite ?".to_string());
    PromptContext {
        description: "Adoption de la RFID dans la distribution chez Banque Alpha.".into(),
        coding_rules: "Une unité peut porter plusieurs codes.".into(),
        research_questions: vec![("RQ1".into(), "Pourquoi adopter ?".into())],
        tree: vec![
            tree("ORG", "Organisation", None, "Le niveau de l'organisation."),
            tree("ORG-SKILLS", "Compétences", Some("ORG"), "Savoir-faire."),
        ],
        attributes: vec![("Secteur".into(), "Public".into())],
        preceding,
        pseudonyms: list.pseudonymiser(),
        pseudonym_digest: list.digest(),
    }
}

fn build(ctx: &PromptContext, batch: usize) -> Vec<cs_model::PromptPart> {
    let seg = seg();
    // Interviewer units are never offered for coding.
    let ids: Vec<String> = seg
        .units
        .iter()
        .filter(|u| u.role != SpeakerRole::Interviewer)
        .map(|u| u.unit_id.clone())
        .collect();
    let codes = vec![
        code("ORG", "Le niveau de l'organisation."),
        code("ORG-SKILLS", "Savoir-faire."),
    ];
    build_prompts_configured(
        &PromptTemplate::default(),
        &PromptInput {
            seg: &seg,
            unit_ids: &ids,
            codes: &codes,
        },
        ctx,
        batch,
    )
}

#[test]
fn the_prompt_carries_the_configuration_and_the_tree() {
    let parts = build(&ctx(), 0);
    let t = &parts[0].text;
    for needle in [
        "## Study",
        "Adoption de la RFID",
        "## Research questions",
        "- RQ1: Pourquoi adopter ?",
        "## Respondent",
        "- Secteur: Public",
        "Coding rules of the study:",
        "Une unité peut porter plusieurs codes.",
        "\"parent\":\"ORG\"",
        "\"includes\":\"inclut Compétences\"",
        "\"excludes\":\"n'inclut pas le coût\"",
        "configuration_digest: ",
        "pseudonymisation_digest: ",
        "candidate_concepts",
    ] {
        assert!(t.contains(needle), "missing {needle:?}");
    }
    assert!(parts[0].codebook_hash.len() == 64);
}

#[test]
fn interviewer_turns_are_context_only_never_units() {
    let parts = build(&ctx(), 0);
    let t = &parts[0].text;
    assert_eq!(parts[0].unit_ids.len(), 3);
    assert!(!parts[0]
        .unit_ids
        .iter()
        .any(|u| u == "ECO-16-U001" || u == "ECO-16-U004"));
    // The preceding question rides on the unit it precedes.
    let line = t
        .lines()
        .find(|l| l.contains("\"unit_id\":\"ECO-16-U005\""))
        .unwrap();
    assert!(line.contains("\"preceding_question\":\"Et ensuite ?\""));
    // A unit with no interviewer turn before it has no such field.
    let line = t
        .lines()
        .find(|l| l.contains("\"unit_id\":\"ECO-16-U003\""))
        .unwrap();
    assert!(!line.contains("preceding_question"));
    // No line of the prompt is an interviewer unit.
    assert!(!t.lines().any(|l| l.contains("\"unit_id\":\"ECO-16-U001\"")));
    assert!(!t.lines().any(|l| l.contains("\"unit_id\":\"ECO-16-U004\"")));
}

#[test]
fn no_listed_name_survives_in_the_prompt() {
    let c = ctx();
    let parts = build(&c, 2);
    assert_eq!(parts.len(), 2);
    for p in &parts {
        assert!(!c.pseudonyms.contains_original(&p.text));
        assert!(!p.text.contains("Anna Muller") && !p.text.contains("Banque Alpha"));
    }
    let all: String = parts.iter().map(|p| p.text.as_str()).collect();
    assert!(all.contains("P1 a trouvé que la ORG1 coûtait"));
    assert!(all.contains("Pourquoi P1 a-t-elle arrêté"));
    assert!(all.contains("Adoption de la RFID dans la distribution chez ORG1"));
}

#[test]
fn the_prompt_is_deterministic_and_bound_to_the_configuration() {
    let a = build(&ctx(), 0);
    let b = build(&ctx(), 0);
    assert_eq!(a, b);
    let mut changed = ctx();
    changed.tree[1].definition = "Savoir-faire techniques.".into();
    let c = build(&changed, 0);
    assert_ne!(a[0].sha256, c[0].sha256);
    assert_ne!(a[0].codebook_hash, c[0].codebook_hash);
    let mut off = ctx();
    off.pseudonyms = Default::default();
    off.pseudonym_digest = PseudonymConfig::default().digest();
    assert_ne!(a[0].text, build(&off, 0)[0].text);
}

#[test]
fn a_prompt_with_an_empty_configuration_keeps_the_fixed_parts() {
    let seg = seg();
    let ids: Vec<String> = vec!["ECO-16-U002".into()];
    let codes = vec![code("ORG", "d")];
    let input = PromptInput {
        seg: &seg,
        unit_ids: &ids,
        codes: &codes,
    };
    let legacy = build_prompts_with(&PromptTemplate::default(), &input, 0);
    let configured = build_prompts_configured(
        &PromptTemplate::default(),
        &input,
        &PromptContext::default(),
        0,
    );
    for needle in ["## Codebook", "## Rules", "## Units", "## Output"] {
        assert!(legacy[0].text.contains(needle) && configured[0].text.contains(needle));
    }
    assert!(!legacy[0].text.contains("candidate_concepts"));
    assert!(!configured[0].text.contains("## Study"));
}

fn answer_with(candidates: &str) -> String {
    let seg = seg();
    format!(
        "{{\"schema\":\"coding-studio/ai-coding/v1\",\"interview_id\":\"ECO-16\",\"phase\":2,\"segmentation_hash\":\"{}\",\"part\":1,\"of\":1,\"codings\":[{{\"unit_id\":\"ECO-16-U002\",\"codes\":[\"ORG\"]}},{{\"unit_id\":\"ECO-16-U003\",\"codes\":[\"ORG-SKILLS\"]}},{{\"unit_id\":\"ECO-16-U005\",\"codes\":[\"NA\"]}}]{candidates}}}",
        seg.hash
    )
}

#[test]
fn candidate_concepts_are_parsed_and_quotes_are_checked() {
    let seg = seg();
    let ids: Vec<String> = ["ECO-16-U002", "ECO-16-U003", "ECO-16-U005"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let codes = ["ORG", "ORG-SKILLS"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let scope = AnswerScope {
        seg: &seg,
        unit_ids: &ids,
        codes: &codes,
    };
    let texts: BTreeMap<String, String> = seg
        .units
        .iter()
        .map(|u| (u.unit_id.clone(), u.text.clone()))
        .collect();
    let c = ctx();

    // Without the channel, nothing changes.
    let plain = parse_ai_output(&answer_with(""), &scope, Some((1, 1))).unwrap();
    assert!(plain.candidate_concepts.is_empty());

    let good = answer_with(
        ",\"candidate_concepts\":[{\"label\":\"Pression du partenaire\",\"description\":\"Un tiers impose.\",\"unit_ids\":[\"ECO-16-U005\"],\"evidence\":[{\"unit_id\":\"ECO-16-U005\",\"quote\":\"Le partenaire a  insisté\"}]}]",
    );
    let out = parse_ai_output(&good, &scope, Some((1, 1))).unwrap();
    assert_eq!(out.candidate_concepts.len(), 1);
    assert_eq!(out.candidate_concepts[0].label, "Pression du partenaire");
    assert!(check_candidate_quotes(&out, &texts, &c.pseudonyms).is_empty());

    // A name in a quotation is read as the model saw it: pseudonymised.
    let masked = answer_with(
        ",\"candidate_concepts\":[{\"label\":\"Coût\",\"unit_ids\":[\"ECO-16-U002\"],\"evidence\":[{\"unit_id\":\"ECO-16-U002\",\"quote\":\"P1 a trouvé que la ORG1 coûtait\"}]}]",
    );
    let out = parse_ai_output(&masked, &scope, Some((1, 1))).unwrap();
    assert!(check_candidate_quotes(&out, &texts, &c.pseudonyms).is_empty());

    // A9: a quotation that is not in the unit is refused.
    let fake = answer_with(
        ",\"candidate_concepts\":[{\"label\":\"X\",\"unit_ids\":[\"ECO-16-U003\"],\"evidence\":[{\"unit_id\":\"ECO-16-U003\",\"quote\":\"Nous avons licencié tout le monde\"}]}]",
    );
    let out = parse_ai_output(&fake, &scope, Some((1, 1))).unwrap();
    let issues = check_candidate_quotes(&out, &texts, &c.pseudonyms);
    assert_eq!(issues.len(), 1);
    assert!(issues[0].message.contains("word for word"));

    // Shape errors are all reported, with the coding errors, and nothing is kept.
    let bad = answer_with(
        ",\"candidate_concepts\":[{\"label\":\"\",\"unit_ids\":[\"ECO-16-U001\"]},{\"label\":\"ok\",\"unit_ids\":[],\"extra\":1}]",
    );
    let issues = parse_ai_output(&bad, &scope, Some((1, 1))).unwrap_err();
    assert!(issues.len() >= 3, "{issues:?}");
}
