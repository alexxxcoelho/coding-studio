//! Prompt generation from a configurable `PromptTemplate` (v5).
//!
//! A template supplies only the static text: `header` (title, role, task),
//! `codebook_intro`, `units_intro`, `footer` (the numbered rules) and the
//! `version` stored as `PromptPart::prompt_version`. The meta lines, the
//! codebook lines, the unit lines, the output instructions and the JSON
//! schema are always produced by cs-ai and cannot be removed or altered by a
//! template, so that every answer stays machine-checkable.
//!
//! Layout of a prompt: the title block of `header` (the text before its first
//! `## ` heading), the meta lines, the rest of `header`, then the fixed
//! sections `## Codebook`, `## Rules` (`footer`), `## Units` (`units_intro`)
//! and `## Output`. `codebook_intro` may contain `{count}`, replaced by the
//! number of codes offered.

use cs_model::{PromptPart, PromptTemplate};

use crate::prompt;
use crate::PromptInput;

/// Builds the copy-paste prompts with `template`, `batch_size` units per part
/// (0 = one part). Deterministic.
pub fn build_prompts_with(
    template: &PromptTemplate,
    input: &PromptInput,
    batch_size: usize,
) -> Vec<PromptPart> {
    prompt::build(template, input, batch_size)
}
