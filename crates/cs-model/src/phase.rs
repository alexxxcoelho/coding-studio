//! The two coding phases.
//!
//! Phase 1 is a provisional pre-classification of every unit by the candidate
//! against the three contexts of the TOE framework (Tornatzky and Fleischer
//! 1990, as summarised by Baker 2012), plus CONTEXT for descriptive material.
//! It stays revisable at any time and is never challenged by the AI.
//!
//! Phase 2 codes the units whose phase 1 category is not NA with concrete
//! codes. Each code may be attached to a TOE context; every active code is
//! offered on every unit, the codes attached to the unit's phase 1 contexts
//! first. The AI challenges phase 2 only.

use crate::{toe_of_context, Code, CodeSet, Codebook, NA};

/// First phase: provisional TOE pre-classification.
pub const PHASE_1: u8 = 1;
/// Second phase: concrete codes, challenged by the AI.
pub const PHASE_2: u8 = 2;

/// Identifier of the phase 1 category set, stored with exports.
pub const PHASE1_CODEBOOK_VERSION: &str = "TOE-contexts v2.0 (provisional pre-classification)";

/// Phase 1 code for descriptive material: respondent, organisation, scope.
pub const CONTEXT: &str = "CONTEXT";

/// Origins of the codes usable in phase 2.
pub const PHASE2_ORIGINS: [&str; 2] = ["literature", "field"];

/// The phase 1 categories (NA is implicit, as in every phase). Definitions are
/// a draft derived from Baker (2012) and the family scopes of codebook v0.2.
pub fn phase1_codebook() -> Codebook {
    let c = |code: &str, toe: Option<&str>, label: &str, definition: &str| Code {
        code: code.to_string(),
        family: code.to_string(),
        label: label.to_string(),
        definition: definition.to_string(),
        origin: "phase1".to_string(),
        toe: toe.map(str::to_string),
        active: true,
        in_prompt: false,
        research_questions: Vec::new(),
        merged_into: None,
        version: 1,
        example: None,
        reason: None,
        created_at: None,
    };
    Codebook {
        source: PHASE1_CODEBOOK_VERSION.to_string(),
        codes: vec![
            c(
                "TECH",
                Some("T"),
                "Technological context",
                "Characteristics of RFID or of other technologies, in use at the firm or available on the market, as they bear on adoption or use: benefits over the status quo, cost and investment, complexity, fit with existing systems and data, read performance, data quality, standards.",
            ),
            c(
                "ORG",
                Some("O"),
                "Organisational context",
                "Characteristics and resources of the adopting organisation: executive sponsorship, champions, structure and decision rights, skills and expertise, budget and staff time, redesign of work processes, size and slack.",
            ),
            c(
                "ENV",
                Some("E"),
                "Environmental and interorganisational context",
                "The setting in which the organisation operates: industry and market structure, competitors, customers, suppliers and value-chain partners (who tags, who pays, who mandates), technology suppliers and support, regulation and standards bodies.",
            ),
            c(
                CONTEXT,
                None,
                "Context of the account",
                "Descriptive information about the respondent, the organisation or the scope of the account: role, background, company, portfolio, which project, where and when. It explains nothing about adoption on its own: when the unit says why adoption advanced, stalled or failed, code TECH, ORG or ENV instead.",
            ),
        ],
    }
}

/// True when a phase 1 set puts the unit in phase 2 scope.
pub fn is_relevant(phase1: &CodeSet) -> bool {
    !phase1.is_empty() && !phase1.contains(NA)
}

/// A code the candidate may use in phase 2: active, not merged, literature or field.
pub fn is_phase2_code(code: &Code) -> bool {
    code.active && code.merged_into.is_none() && PHASE2_ORIGINS.contains(&code.origin.as_str())
}

/// Every phase 2 code, the ones attached to the unit's phase 1 contexts
/// first, codebook order within each group.
pub fn phase2_codes<'a>(codebook: &'a Codebook, phase1: &CodeSet) -> Vec<&'a Code> {
    let contexts: Vec<&str> = phase1.iter().filter_map(|c| toe_of_context(c)).collect();
    phase2_codes_for_toe(codebook, &contexts)
}

/// Every phase 2 code offered for a unit whose phase 1 categories are
/// `phase1`, the categories being those of the study (see
/// `StudyConfig::effective_categories`): the codes attached to the TOE
/// letters of those categories first.
pub fn phase2_codes_for_categories<'a>(
    codebook: &'a Codebook,
    categories: &[crate::Category],
    phase1: &CodeSet,
) -> Vec<&'a Code> {
    let toes: Vec<&str> = phase1
        .iter()
        .filter_map(|name| categories.iter().find(|c| &c.name == name))
        .filter_map(|c| c.attachment.as_deref())
        .collect();
    phase2_codes_for_toe(codebook, &toes)
}

/// `phase2_codes` for the TOE letters `contexts` of a unit.
fn phase2_codes_for_toe<'a>(codebook: &'a Codebook, contexts: &[&str]) -> Vec<&'a Code> {
    let attached = |c: &Code| c.toe.as_deref().is_some_and(|t| contexts.contains(&t));
    let usable: Vec<&Code> = codebook
        .codes
        .iter()
        .filter(|c| is_phase2_code(c))
        .collect();
    let mut out: Vec<&Code> = usable.iter().copied().filter(|c| attached(c)).collect();
    out.extend(usable.iter().copied().filter(|c| !attached(c)));
    out
}

/// Codes offered to the AI coder: phase 2 codes marked `in_prompt`.
pub fn phase2_prompt_codes(codebook: &Codebook) -> Vec<&Code> {
    codebook
        .codes
        .iter()
        .filter(|c| is_phase2_code(c) && c.in_prompt)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(code: &str, toe: Option<&str>, origin: &str) -> Code {
        Code {
            code: code.into(),
            family: crate::family_for_toe(toe).into(),
            label: code.into(),
            definition: String::new(),
            origin: origin.into(),
            toe: toe.map(str::to_string),
            active: true,
            in_prompt: true,
            research_questions: Vec::new(),
            merged_into: None,
            version: 1,
            example: None,
            reason: None,
            created_at: None,
        }
    }

    fn set(codes: &[&str]) -> CodeSet {
        codes.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn phase1_has_the_three_contexts_and_context() {
        let book = phase1_codebook();
        let codes: Vec<&str> = book.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["TECH", "ORG", "ENV", CONTEXT]);
        assert!(book
            .codes
            .iter()
            .all(|c| !c.definition.contains('\u{2014}') && !c.definition.contains('\u{2013}')));
    }

    #[test]
    fn relevance() {
        assert!(is_relevant(&set(&["CONTEXT"])));
        assert!(!is_relevant(&set(&["NA"])));
        assert!(!is_relevant(&set(&[])));
    }

    #[test]
    fn phase2_offers_every_code_attached_ones_first() {
        let mut merged = code("OLD", Some("T"), "literature");
        merged.merged_into = Some("TECH-ADV".into());
        merged.active = false;
        let mut quiet = code("ORG-QUIET", Some("O"), "field");
        quiet.in_prompt = false;
        let book = Codebook {
            source: String::new(),
            codes: vec![
                code("TECH", Some("T"), "legacy"),
                code("LEGAL-OBLIGATION", Some("E"), "literature"),
                code("TECH-ADV", Some("T"), "literature"),
                quiet,
                code("COST-NO-BENEFIT", None, "field"),
                merged,
            ],
        };
        let names = |v: Vec<&Code>| v.iter().map(|c| c.code.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(phase2_codes(&book, &set(&["TECH"]))),
            [
                "TECH-ADV",
                "LEGAL-OBLIGATION",
                "ORG-QUIET",
                "COST-NO-BENEFIT"
            ]
        );
        assert_eq!(
            names(phase2_codes(&book, &set(&["ORG", "ENV"]))),
            [
                "LEGAL-OBLIGATION",
                "ORG-QUIET",
                "TECH-ADV",
                "COST-NO-BENEFIT"
            ]
        );
        assert_eq!(
            names(phase2_prompt_codes(&book)),
            ["LEGAL-OBLIGATION", "TECH-ADV", "COST-NO-BENEFIT"]
        );
    }
}
