//! Prompt generation, AI output validation, agreement, divergences, final coding
//! and report rendering, for both coding phases. Pure functions: no I/O, no
//! database.
//!
//! Phases (see `cs_model::phase`): phase 1 codes every unit against the TOE
//! contexts with `phase1_codebook()`; phase 2 codes the relevant units, each
//! with its own list of allowed study-specific codes computed by the caller
//! from the reconciled phase 1 coding (`phase2_allowed(.., false)`).
//!
//! Splitting into parts: `n` units in scope and a batch size `b` give
//! `of = ceil(n / b)` parts (one part when `b` is 0 or `b >= n`); every part
//! then holds `ceil(n / of)` units, the last one the remainder. The split
//! depends on `of` alone, so the range of a part can be recomputed from the
//! `part` and `of` fields of an answer (see [`part_unit_ids`]).

use std::collections::BTreeMap;

use cs_model::{
    AgreementReport, AiOutput, Arbitration, Codebook, CodingMap, Divergence, FinalCoding,
    PromptPart, Segmentation, ValidationIssue,
};
use serde_json::json;

mod analysis;
mod parse;
mod prompt;
mod report;

pub use analysis::cohen_kappa;
pub use prompt::{part_unit_ids, parts_for_batch, phase_title};

/// Identifier of the prompt template, journalised with every run. Version 2
/// introduced the two coding phases.
pub const PROMPT_VERSION: &str = "CS-P04-EN v2.0";
/// Value the AI must put in the `schema` field of its answer.
pub const OUTPUT_SCHEMA_ID: &str = "coding-studio/ai-coding/v1";

/// What a prompt is built from, and what an answer is checked against.
#[derive(Debug, Clone, Copy)]
pub struct PromptInput<'a> {
    pub seg: &'a Segmentation,
    /// `cs_model::PHASE_1` or `cs_model::PHASE_2`.
    pub phase: u8,
    /// Units in scope, in order: every unit in phase 1, the relevant ones in phase 2.
    pub unit_ids: &'a [String],
    /// Phase 1: the codes listed are `codebook.prompt_codes()` (pass
    /// `phase1_codebook()`). Phase 2: the thesis codebook, of which only the
    /// codes present in some allowed list are listed.
    pub codebook: &'a Codebook,
    /// Phase 2 only: unit_id to the codes that unit may receive (NA is always
    /// valid besides). A unit absent from the map may only be coded NA.
    pub allowed: Option<&'a BTreeMap<String, Vec<String>>>,
}

/// Builds the copy-paste prompts, `batch_size` units per part (0 = one part).
/// Deterministic: the same input and batch size always give byte-identical
/// text. The prompt holds the codes and the units in scope only.
pub fn build_prompts(input: &PromptInput, batch_size: usize) -> Vec<PromptPart> {
    prompt::build(input, batch_size)
}

/// JSON Schema (draft 2020-12) of the expected answer for a phase. Generic:
/// the prompt embeds a stricter copy with this part's values and the valid
/// codes fixed.
pub fn output_json_schema(phase: u8) -> serde_json::Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": OUTPUT_SCHEMA_ID,
        "title": "Coding Studio blind AI coding answer",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema", "interview_id", "phase", "segmentation_hash", "part", "of", "codings"],
        "properties": {
            "schema": { "const": OUTPUT_SCHEMA_ID },
            "interview_id": { "type": "string" },
            "phase": { "const": phase },
            "segmentation_hash": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "part": { "type": "integer", "minimum": 1 },
            "of": { "type": "integer", "minimum": 1 },
            "codings": {
                "type": "array",
                "description": "One entry per unit of the part, in input order.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["unit_id", "codes"],
                    "properties": {
                        "unit_id": { "type": "string" },
                        "codes": {
                            "type": "array",
                            "minItems": 1,
                            "uniqueItems": true,
                            "description": if phase == cs_model::PHASE_2 {
                                "Codes from the unit's allowed_codes, or NA alone."
                            } else {
                                "Codes of the codebook offered in the prompt, or NA alone."
                            },
                            "items": { "type": "string", "pattern": "^[A-Z][A-Z0-9-]*$" }
                        }
                    }
                }
            }
        }
    })
}

/// Parses and validates an AI answer against the same input its prompt was
/// built from. `expected_part` = Some((part, of)) when the candidate pasted it
/// for a given part. The answer must declare the input's phase; in phase 2
/// every code must be in the unit's allowed list. All problems are returned
/// together; nothing is repaired.
pub fn parse_ai_output(
    raw: &str,
    input: &PromptInput,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    parse::parse(raw, input, expected_part)
}

/// Agreement between two codings over `unit_order` (units missing in either
/// side are skipped). Port of `00_Tools/Scripts/compute_agreement.py`.
pub fn agreement(
    label_a: &str,
    a: &CodingMap,
    label_b: &str,
    b: &CodingMap,
    unit_order: &[String],
) -> AgreementReport {
    analysis::agreement(label_a, a, label_b, b, unit_order)
}

/// Units where the human set differs from an AI run, or AI runs disagree among
/// themselves, in `unit_order`. Delegates to `cs_model::reconcile`, the rule
/// the store also enforces.
///
/// Only units coded by the human and by every run are compared; a unit a run
/// has not coded yet is incomplete, not divergent. `HumanVsAi`: the human set
/// matches no run. `AiRunsUnstable`: the human set matches at least one run
/// but the runs differ among themselves (P-04: run instability goes to
/// arbitration like any human/machine divergence).
pub fn divergences(
    unit_order: &[String],
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    cs_model::reconcile::divergences(unit_order, human, runs)
}

/// Final coding of every unit in `unit_order`: agreed units keep the common
/// set, divergent units take the arbitration. Err lists the unit_ids still
/// pending: not coded by the human, not coded by a run, or divergent without
/// an arbitration. With no run at all, every human-coded unit counts as
/// agreed. An arbitration on a unit that is not divergent is ignored.
pub fn final_coding(
    unit_order: &[String],
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> Result<Vec<FinalCoding>, Vec<String>> {
    cs_model::reconcile::final_coding(unit_order, human, runs, arbitrations)
}

/// `unit_id,speaker,timestamp,text` (same format as Scripts/segment_transcript.py).
/// Speaker is INT or RESP when the role is known, else the source label.
pub fn units_csv(seg: &Segmentation) -> String {
    report::units_csv(seg)
}

/// `unit_id,codes` with `;` separators (format of Scripts/compute_agreement.py).
pub fn coding_csv(coding: &CodingMap, unit_order: &[String]) -> String {
    report::coding_csv(coding, unit_order)
}

/// `unit_id,codes,basis` for the final coding (basis: agreed | arbitrated).
pub fn final_csv(finals: &[FinalCoding]) -> String {
    report::final_csv(finals)
}

/// Markdown agreement report (English), deterministic. `unit_order` is the
/// scope of the phase.
pub fn render_agreement_md(
    seg: &Segmentation,
    phase: u8,
    unit_order: &[String],
    reports: &[AgreementReport],
) -> String {
    report::render_agreement_md(seg, phase, unit_order, reports)
}

/// Markdown adjudication log of a phase: every divergence, every decision and
/// its rationale.
pub fn render_adjudication_md(
    seg: &Segmentation,
    phase: u8,
    divergences: &[Divergence],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> String {
    report::render_adjudication_md(seg, phase, divergences, arbitrations)
}

#[cfg(test)]
mod tests;
