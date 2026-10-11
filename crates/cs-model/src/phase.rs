//! The two coding phases.
//!
//! Phase 1 codes every unit against the three contexts of the TOE framework
//! (Tornatzky and Fleischer 1990, as summarised by Baker 2012), plus one closed
//! category for relevant material outside the framework. The framework names
//! the contexts but prescribes no fixed list of factors within them; the
//! factors are study-specific, so they belong to phase 2.
//!
//! Phase 2 runs only on units whose reconciled phase 1 coding is not `NA`. A
//! unit may receive the study-specific codes of the families it was assigned
//! in phase 1, plus the cross-cutting families (outcome, turning point,
//! transfer, stage, context). Inductive codes are created by the candidate in
//! phase 2 and are never offered to the AI coder.

use crate::{Code, CodeSet, Codebook, NA};

/// First phase: TOE contexts.
pub const PHASE_1: u8 = 1;
/// Second phase: study-specific codes within the reconciled contexts.
pub const PHASE_2: u8 = 2;

/// Identifier of the phase 1 category set, stored with prompts and exports.
pub const PHASE1_CODEBOOK_VERSION: &str =
    "TOE-contexts v1.0 (draft, to be validated by the candidate)";

/// Families whose study-specific codes are reachable from a phase 1 code.
pub const TOE_FAMILIES: [&str; 3] = ["TECH", "ORG", "ENV"];

/// Families open to every relevant unit in phase 2, whatever its context.
pub const CROSS_CUTTING_FAMILIES: [&str; 5] =
    ["OUTCOME", "TURNING-POINT", "TRANSFER", "STAGE", "CONTEXT"];

/// Phase 1 code for relevant material outside the three contexts.
pub const OUTSIDE_TOE: &str = "INDUCTIVE-PENDING";

/// The phase 1 categories (NA is implicit, as in every phase). Definitions are
/// a draft derived from Baker (2012) and the family scopes of codebook v0.2.
pub fn phase1_codebook() -> Codebook {
    let c = |code: &str, family: &str, label: &str, definition: &str| Code {
        code: code.to_string(),
        family: family.to_string(),
        label: label.to_string(),
        definition: definition.to_string(),
        origin: "phase1".to_string(),
        active: true,
        in_prompt: true,
        example: None,
        reason: None,
        created_at: None,
    };
    Codebook {
        source: PHASE1_CODEBOOK_VERSION.to_string(),
        codes: vec![
            c(
                "TECH",
                "TECH",
                "Technological context",
                "Characteristics of RFID or of other technologies, in use at the firm or available on the market, as they bear on adoption or use: benefits over the status quo, cost and investment, complexity, fit with existing systems and data, read performance, data quality, standards.",
            ),
            c(
                "ORG",
                "ORG",
                "Organisational context",
                "Characteristics and resources of the adopting organisation: executive sponsorship, champions, structure and decision rights, skills and expertise, budget and staff time, redesign of work processes, size and slack.",
            ),
            c(
                "ENV",
                "ENV",
                "Environmental and interorganisational context",
                "The setting in which the organisation operates: industry and market structure, competitors, customers, suppliers and value-chain partners (who tags, who pays, who mandates), technology suppliers and support, regulation and standards bodies.",
            ),
            c(
                OUTSIDE_TOE,
                "INDUCTIVE",
                "Relevant, outside the three contexts",
                "Relevant to the adoption, implementation, outcome or transfer of the technology, but not attributable to the technological, organisational or environmental context: for example a result, a turning point, a transfer claim, or a mechanism the framework does not cover.",
            ),
        ],
    }
}

/// True when a reconciled phase 1 set makes the unit eligible for phase 2.
pub fn is_relevant(phase1_final: &CodeSet) -> bool {
    !phase1_final.is_empty() && !phase1_final.contains(NA)
}

/// Codes a unit may receive in phase 2, given its reconciled phase 1 set.
/// `with_inductive` adds the candidate's inductive codes (never for the AI).
/// Legacy and bookkeeping codes are excluded; NA is always allowed besides.
pub fn phase2_allowed<'a>(
    codebook: &'a Codebook,
    phase1_final: &CodeSet,
    with_inductive: bool,
) -> Vec<&'a Code> {
    codebook
        .codes
        .iter()
        .filter(|c| c.active)
        .filter(|c| match c.origin.as_str() {
            "codebook" => {
                let fam = c.family.as_str();
                CROSS_CUTTING_FAMILIES.contains(&fam)
                    || (TOE_FAMILIES.contains(&fam) && phase1_final.contains(fam))
            }
            "inductive" => with_inductive,
            _ => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(code: &str, family: &str, origin: &str) -> Code {
        Code {
            code: code.into(),
            family: family.into(),
            label: code.into(),
            definition: String::new(),
            origin: origin.into(),
            active: true,
            in_prompt: true,
            example: None,
            reason: None,
            created_at: None,
        }
    }

    fn set(codes: &[&str]) -> CodeSet {
        codes.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn phase1_has_the_three_contexts_and_the_outside_category() {
        let book = phase1_codebook();
        let codes: Vec<&str> = book.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["TECH", "ORG", "ENV", OUTSIDE_TOE]);
        assert!(book
            .codes
            .iter()
            .all(|c| !c.definition.contains('\u{2014}') && !c.definition.contains('\u{2013}')));
    }

    #[test]
    fn relevance() {
        assert!(is_relevant(&set(&["TECH"])));
        assert!(!is_relevant(&set(&["NA"])));
        assert!(!is_relevant(&set(&[])));
    }

    #[test]
    fn phase2_follows_the_reconciled_families() {
        let book = Codebook {
            source: String::new(),
            codes: vec![
                code("TECH", "TECH", "legacy"),
                code("TECH-ADV", "TECH", "codebook"),
                code("ORG-SKILLS", "ORG", "codebook"),
                code("ENV-PARTNER", "ENV", "codebook"),
                code("OUT-REALISED", "OUTCOME", "codebook"),
                code("INDUCTIVE-PENDING", "INDUCTIVE", "bookkeeping"),
                code("BUSINESS-CASE", "INDUCTIVE", "inductive"),
            ],
        };
        let names = |v: Vec<&Code>| v.iter().map(|c| c.code.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(phase2_allowed(&book, &set(&["TECH"]), false)),
            ["TECH-ADV", "OUT-REALISED"]
        );
        assert_eq!(
            names(phase2_allowed(&book, &set(&["ORG", "ENV"]), true)),
            ["ORG-SKILLS", "ENV-PARTNER", "OUT-REALISED", "BUSINESS-CASE"]
        );
        assert_eq!(
            names(phase2_allowed(&book, &set(&[OUTSIDE_TOE]), false)),
            ["OUT-REALISED"]
        );
    }
}
