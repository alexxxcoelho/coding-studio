//! Prompt generation, AI output validation, agreement, divergences, final coding
//! and report rendering. Pure functions: no I/O, no database.

use cs_model::{
    AgreementReport, AiOutput, Arbitration, Codebook, CodingMap, Divergence, FinalCoding,
    PromptPart, Segmentation, ValidationIssue,
};

/// Identifier of the prompt template, journalised with every run.
pub const PROMPT_VERSION: &str = "CS-P04-EN v1.0";
/// Value the AI must put in the `schema` field of its answer.
pub const OUTPUT_SCHEMA_ID: &str = "coding-studio/ai-coding/v1";

/// Builds the copy-paste prompts, `batch_size` units per part (0 = one part).
pub fn build_prompts(
    seg: &Segmentation,
    codebook: &Codebook,
    batch_size: usize,
) -> Vec<PromptPart> {
    let _ = (seg, codebook, batch_size);
    todo!()
}

/// JSON Schema of the expected answer, embedded in the prompt and the docs.
pub fn output_json_schema() -> serde_json::Value {
    todo!()
}

/// Parses and validates an AI answer against the segmentation and the codebook.
/// `expected_part` = Some((part, of)) when the candidate pasted it for a given part.
pub fn parse_ai_output(
    raw: &str,
    seg: &Segmentation,
    codebook: &Codebook,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    let _ = (raw, seg, codebook, expected_part);
    todo!()
}

/// Agreement between two codings over `unit_order` (units missing in either side are skipped).
pub fn agreement(
    label_a: &str,
    a: &CodingMap,
    label_b: &str,
    b: &CodingMap,
    unit_order: &[String],
) -> AgreementReport {
    let _ = (label_a, a, label_b, b, unit_order);
    todo!()
}

/// Units where the human set differs from an AI run, or AI runs disagree among themselves.
pub fn divergences(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
) -> Vec<Divergence> {
    let _ = (seg, human, runs);
    todo!()
}

/// Final coding: agreed units keep the common set, divergent units take the arbitration.
/// Err lists the unit_ids still waiting for an arbitration.
pub fn final_coding(
    seg: &Segmentation,
    human: &CodingMap,
    runs: &[(String, CodingMap)],
    arbitrations: &std::collections::BTreeMap<String, Arbitration>,
) -> Result<Vec<FinalCoding>, Vec<String>> {
    let _ = (seg, human, runs, arbitrations);
    todo!()
}

/// `unit_id,speaker,timestamp,text` (same format as Scripts/segment_transcript.py).
pub fn units_csv(seg: &Segmentation) -> String {
    let _ = seg;
    todo!()
}

/// `unit_id,codes` with `;` separators (format of Scripts/compute_agreement.py).
pub fn coding_csv(coding: &CodingMap, unit_order: &[String]) -> String {
    let _ = (coding, unit_order);
    todo!()
}

/// `unit_id,codes,basis` for the final coding.
pub fn final_csv(finals: &[FinalCoding]) -> String {
    let _ = finals;
    todo!()
}

/// Markdown agreement report (English), deterministic.
pub fn render_agreement_md(seg: &Segmentation, reports: &[AgreementReport]) -> String {
    let _ = (seg, reports);
    todo!()
}

/// Markdown adjudication log: every divergence, every decision and its rationale.
pub fn render_adjudication_md(
    seg: &Segmentation,
    divergences: &[Divergence],
    arbitrations: &std::collections::BTreeMap<String, Arbitration>,
) -> String {
    let _ = (seg, divergences, arbitrations);
    todo!()
}
