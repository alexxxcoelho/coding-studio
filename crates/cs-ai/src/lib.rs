//! Prompt generation, AI answer validation, agreement, final coding and report
//! rendering. Pure functions: no I/O, no database.
//!
//! Only the detailed codes of phase 2 are challenged by the AI (phase 1 is the
//! candidate's provisional pre-classification). The prompt lists the codes
//! offered, grouped by TOE attachment, and the units; never the candidate's
//! codes, phase 1 contexts, labels to clarify, memos or summaries.
//!
//! Splitting into parts: `n` units in scope and a batch size `b` give
//! `of = ceil(n / b)` parts (one part when `b` is 0 or `b >= n`); every part
//! then holds `ceil(n / of)` units, the last one the remainder. The split
//! depends on `of` alone, so the range of a part can be recomputed from the
//! `part` and `of` fields of an answer (see [`part_unit_ids`]).

use std::collections::{BTreeMap, BTreeSet};

use cs_model::{
    AgreementReport, AiOutput, Arbitration, Code, CodingMap, Divergence, FinalCoding, PromptPart,
    ReviewItem, Segmentation, ValidationIssue,
};
use serde_json::json;

mod analysis;
mod parse;
mod prompt;
pub mod providers;
mod report;

pub use analysis::cohen_kappa;
pub use prompt::{part_unit_ids, parts_for_batch, phase_title};
pub use providers::{
    providers_from_config, Execution, IsolationReport, ModelProvider, ProviderDescription,
    ProviderKind, ProviderRequest, ProviderResponse, ProvidersConfig,
};

/// Identifier of the prompt template, journalised with every run. Version 3:
/// phase 2 only, codes grouped by TOE attachment. Version 3.1: unit ids found
/// in code texts are redacted, so the codebook cannot reveal code origins.
pub const PROMPT_VERSION: &str = "CS-P04-EN v3.1";
/// Value the AI must put in the `schema` field of its answer.
pub const OUTPUT_SCHEMA_ID: &str = "coding-studio/ai-coding/v1";

/// What a prompt is built from.
#[derive(Debug, Clone, Copy)]
pub struct PromptInput<'a> {
    pub seg: &'a Segmentation,
    /// Units in scope, in order.
    pub unit_ids: &'a [String],
    /// Codes offered (see `cs_model::phase::phase2_prompt_codes`).
    pub codes: &'a [Code],
}

/// What an answer is checked against: the batch its prompt belongs to.
#[derive(Debug, Clone, Copy)]
pub struct AnswerScope<'a> {
    pub seg: &'a Segmentation,
    /// Units of the whole batch, in order (parts concatenated).
    pub unit_ids: &'a [String],
    /// Codes offered in the batch.
    pub codes: &'a BTreeSet<String>,
}

/// Units and offered codes of a recorded batch, the input of [`AnswerScope`].
pub fn batch_scope(parts: &[PromptPart]) -> (Vec<String>, BTreeSet<String>) {
    let mut parts: Vec<&PromptPart> = parts.iter().collect();
    parts.sort_by_key(|p| p.part);
    let units = parts.iter().flat_map(|p| p.unit_ids.clone()).collect();
    let codes = parts.iter().flat_map(|p| p.codes.clone()).collect();
    (units, codes)
}

/// Builds the copy-paste prompts, `batch_size` units per part (0 = one part).
/// Deterministic: the same input and batch size always give byte-identical
/// text.
pub fn build_prompts(input: &PromptInput, batch_size: usize) -> Vec<PromptPart> {
    prompt::build(input, batch_size)
}

/// JSON Schema (draft 2020-12) of the expected answer. Generic: the prompt
/// embeds a stricter copy with this part's values and the codes fixed.
pub fn output_json_schema() -> serde_json::Value {
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
            "phase": { "const": cs_model::PHASE_2 },
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
                            "description": "Codes offered in the prompt, or NA alone.",
                            "items": { "type": "string", "pattern": "^[A-Z][A-Z0-9-]*$" }
                        }
                    }
                }
            }
        }
    })
}

/// Parses and validates an AI answer against the batch its prompt belongs to.
/// `expected_part` = Some((part, of)) when the candidate pasted it for a given
/// part. All problems are returned together; nothing is repaired.
pub fn parse_ai_output(
    raw: &str,
    scope: &AnswerScope,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    parse::parse(raw, scope, expected_part)
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

/// Disagreements between the candidate and the runs, in `unit_order`.
/// Delegates to `cs_model::reconcile`, the rule the store also applies.
pub fn divergences(
    unit_order: &[String],
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    cs_model::reconcile::divergences(unit_order, human, runs)
}

/// Final coding of `unit_order`: the review decision where one exists,
/// otherwise the candidate's codes; second value: the uncoded units.
pub fn final_coding(
    unit_order: &[String],
    human: &CodingMap,
    decisions: &BTreeMap<String, Arbitration>,
) -> (Vec<FinalCoding>, Vec<String>) {
    cs_model::reconcile::final_coding(unit_order, human, decisions)
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

/// `unit_id,codes,basis` for the final coding (basis: candidate | reviewed).
pub fn final_csv(finals: &[FinalCoding]) -> String {
    report::final_csv(finals)
}

/// Markdown agreement report (English), deterministic. `unit_order` is the
/// list of units compared with the AI.
pub fn render_agreement_md(
    seg: &Segmentation,
    unit_order: &[String],
    reports: &[AgreementReport],
) -> String {
    report::render_agreement_md(seg, unit_order, reports)
}

/// Markdown review log: every disagreement and sampled agreement, the
/// decision and its rationale, the sample seeds.
pub fn render_review_md(
    seg: &Segmentation,
    items: &[ReviewItem],
    history: &[Arbitration],
    seeds: &[(String, String)],
) -> String {
    report::render_review_md(seg, items, history, seeds)
}

#[cfg(test)]
mod tests;
