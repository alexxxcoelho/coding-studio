//! Prompt generation, AI output validation, agreement, divergences, final coding
//! and report rendering. Pure functions: no I/O, no database.
//!
//! Splitting into parts: a segmentation of `n` units with a batch size `b`
//! gives `of = ceil(n / b)` parts (one part when `b` is 0 or `b >= n`); every
//! part then holds `ceil(n / of)` units, the last one the remainder. The split
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
pub use prompt::{part_unit_ids, parts_for_batch};

/// Identifier of the prompt template, journalised with every run.
pub const PROMPT_VERSION: &str = "CS-P04-EN v1.0";
/// Value the AI must put in the `schema` field of its answer.
pub const OUTPUT_SCHEMA_ID: &str = "coding-studio/ai-coding/v1";

/// Builds the copy-paste prompts, `batch_size` units per part (0 = one part).
/// Deterministic: the same segmentation, codebook and batch size always give
/// byte-identical text. The prompt holds the codebook and the units only.
pub fn build_prompts(
    seg: &Segmentation,
    codebook: &Codebook,
    batch_size: usize,
) -> Vec<PromptPart> {
    prompt::build(seg, codebook, batch_size)
}

/// JSON Schema (draft 2020-12) of the expected answer. Generic: the prompt
/// embeds a stricter copy with this part's values and the allowed codes fixed.
pub fn output_json_schema() -> serde_json::Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": OUTPUT_SCHEMA_ID,
        "title": "Coding Studio blind AI coding answer",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema", "interview_id", "segmentation_hash", "part", "of", "codings"],
        "properties": {
            "schema": { "const": OUTPUT_SCHEMA_ID },
            "interview_id": { "type": "string" },
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
                            "description": "Codes of the codebook offered in the prompt, or NA alone.",
                            "items": { "type": "string", "pattern": "^[A-Z][A-Z0-9-]*$" }
                        }
                    }
                }
            }
        }
    })
}

/// Parses and validates an AI answer against the segmentation and the codebook.
/// `expected_part` = Some((part, of)) when the candidate pasted it for a given part.
/// All problems are returned together; nothing is repaired.
pub fn parse_ai_output(
    raw: &str,
    seg: &Segmentation,
    codebook: &Codebook,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    parse::parse(raw, seg, codebook, expected_part)
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
/// themselves, in segmentation order.
///
/// Only units coded by the human and by every run are compared; a unit a run
/// has not coded yet is incomplete, not divergent. `HumanVsAi`: the human set
/// matches no run. `AiRunsUnstable`: the human set matches at least one run
/// but the runs differ among themselves (P-04: run instability goes to
/// arbitration like any human/machine divergence).
pub fn divergences(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    analysis::divergences(seg, human, runs)
}

/// Final coding: agreed units keep the common set, divergent units take the arbitration.
/// Err lists the unit_ids still pending: not coded by the human, not coded by
/// a run, or divergent without an arbitration. With no run at all, every
/// human-coded unit counts as agreed. An arbitration on a unit that is not
/// divergent is ignored.
pub fn final_coding(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> Result<Vec<FinalCoding>, Vec<String>> {
    analysis::final_coding(seg, human, runs, arbitrations)
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

/// Markdown agreement report (English), deterministic.
pub fn render_agreement_md(seg: &Segmentation, reports: &[AgreementReport]) -> String {
    report::render_agreement_md(seg, reports)
}

/// Markdown adjudication log: every divergence, every decision and its rationale.
pub fn render_adjudication_md(
    seg: &Segmentation,
    divergences: &[Divergence],
    arbitrations: &BTreeMap<String, Arbitration>,
) -> String {
    report::render_adjudication_md(seg, divergences, arbitrations)
}

#[cfg(test)]
mod tests;
