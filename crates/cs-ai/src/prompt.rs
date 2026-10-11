//! Copy-paste prompts for the blind secondary coder (English translation of P-04).
//!
//! The prompt carries the codebook and the units only. Human codes, memos,
//! summaries and record sheets never enter it: the AI coder works blind.

use std::fmt::Write as _;

use cs_model::{sha256_hex, Code, Codebook, PromptPart, Segmentation, SpeakerRole, Unit, NA};
use serde_json::json;

use crate::{OUTPUT_SCHEMA_ID, PROMPT_VERSION};

/// Number of parts for `n` units and a requested batch size (0 = one part).
pub fn parts_for_batch(n: usize, batch_size: usize) -> u32 {
    if batch_size == 0 || n <= batch_size {
        1
    } else {
        n.div_ceil(batch_size) as u32
    }
}

/// Unit IDs of part `part` (1-based) when the segmentation is split into `of`
/// parts.
///
/// The split depends on `of` only: every part holds `ceil(n / of)` units, the
/// last one the remainder. `build_prompts` derives `of` from the batch size
/// and then uses this same function, so the parser can recompute the range of
/// a part from the `part`/`of` fields of an answer alone.
pub fn part_unit_ids(seg: &Segmentation, part: u32, of: u32) -> Vec<String> {
    part_units(&seg.units, part, of)
        .iter()
        .map(|u| u.unit_id.clone())
        .collect()
}

pub(crate) fn part_units(units: &[Unit], part: u32, of: u32) -> &[Unit] {
    if of == 0 || part == 0 || part > of {
        return &[];
    }
    let n = units.len();
    let size = n.div_ceil(of as usize);
    let start = (size * (part as usize - 1)).min(n);
    let end = (size * part as usize).min(n);
    &units[start..end]
}

/// Speaker label used in prompts and exports: the role when known, otherwise
/// the label as written in the source (S1, Speaker 2).
pub(crate) fn speaker_label(unit: &Unit) -> &str {
    match unit.role {
        SpeakerRole::Unknown => unit.speaker_raw.as_str(),
        role => role.short(),
    }
}

/// One-line JSON object with its fields in the given order, em and en dashes
/// written as JSON escapes so the prompt never contains those characters
/// (project rule) while the text stays byte-for-byte recoverable. The order is
/// set here, not by serde_json, so the text does not depend on its features.
pub(crate) fn json_line(fields: &[(&str, &str)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}:{}", json_str(k), json_str(v)))
        .collect();
    escape_dashes(&format!("{{{}}}", body.join(",")))
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).expect("strings always serialise")
}

/// Pretty JSON with object keys sorted, two-space indent: stable whatever the
/// key order of the `Value` maps.
pub(crate) fn canonical_pretty(value: &serde_json::Value, indent: usize, out: &mut String) {
    use serde_json::Value;
    let pad = "  ".repeat(indent + 1);
    let close = "  ".repeat(indent);
    match value {
        Value::Object(map) if !map.is_empty() => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push_str("{\n");
            for (i, k) in keys.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&json_str(k));
                out.push_str(": ");
                canonical_pretty(&map[*k], indent + 1, out);
                out.push_str(if i + 1 < keys.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                canonical_pretty(item, indent + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).expect("json values serialise")),
    }
}

fn escape_dashes(s: &str) -> String {
    s.replace('\u{2014}', "\\u2014")
        .replace('\u{2013}', "\\u2013")
}

/// Schema embedded in a prompt: the generic schema with the values of this
/// part fixed and the codes restricted to the codebook offered.
fn part_schema(seg: &Segmentation, codes: &[&Code], part: u32, of: u32) -> serde_json::Value {
    let mut allowed: Vec<&str> = codes.iter().map(|c| c.code.as_str()).collect();
    allowed.push(NA);
    let mut schema = crate::output_json_schema();
    let props = &mut schema["properties"];
    props["interview_id"] = json!({ "const": seg.interview_id });
    props["segmentation_hash"] = json!({ "const": seg.hash });
    props["part"] = json!({ "const": part });
    props["of"] = json!({ "const": of });
    props["codings"]["items"]["properties"]["codes"]["items"] = json!({ "enum": allowed });
    schema
}

pub(crate) fn build(seg: &Segmentation, codebook: &Codebook, batch_size: usize) -> Vec<PromptPart> {
    let of = parts_for_batch(seg.units.len(), batch_size);
    let codes = codebook.prompt_codes();
    let codebook_hash = codebook.prompt_hash();
    (1..=of)
        .map(|part| {
            let units = part_units(&seg.units, part, of);
            let text = render(seg, &codes, &codebook_hash, units, part, of);
            PromptPart {
                interview_id: seg.interview_id.clone(),
                part,
                of,
                unit_ids: units.iter().map(|u| u.unit_id.clone()).collect(),
                segmentation_hash: seg.hash.clone(),
                codebook_hash: codebook_hash.clone(),
                prompt_version: PROMPT_VERSION.to_string(),
                sha256: sha256_hex(text.as_bytes()),
                text,
            }
        })
        .collect()
}

fn render(
    seg: &Segmentation,
    codes: &[&Code],
    codebook_hash: &str,
    units: &[Unit],
    part: u32,
    of: u32,
) -> String {
    let mut t = String::new();
    let first = units.first().map(|u| u.unit_id.as_str()).unwrap_or("");
    let last = units.last().map(|u| u.unit_id.as_str()).unwrap_or("");

    // Writing to a String cannot fail; the results are ignored on purpose.
    let _ = writeln!(t, "# Coding Studio: blind secondary coding");
    let _ = writeln!(t);
    let _ = writeln!(t, "interview_id: {}", seg.interview_id);
    let _ = writeln!(t, "part: {part} of {of}");
    let _ = writeln!(t, "prompt_version: {PROMPT_VERSION}");
    let _ = writeln!(t, "segmentation_hash: {}", seg.hash);
    let _ = writeln!(t, "codebook_hash: {codebook_hash}");
    let _ = writeln!(t, "units_in_this_part: {} ({first} to {last})", units.len());
    let _ = writeln!(t);

    let _ = writeln!(t, "## Role");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "You are a secondary coder on a qualitative study. You apply a closed codebook. \
You do not produce analysis, you do not build themes, and you do not interpret beyond \
the assignment of codes."
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "## Task");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "For each unit listed under UNITS, assign the code or codes of the codebook that apply."
    );
    let _ = writeln!(t);

    let _ = writeln!(t, "## Codebook");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "One code per line, as JSON with the fields code, family, label and definition. \
These {} codes and NA are the only valid values.",
        codes.len()
    );
    let _ = writeln!(t);
    for c in codes {
        let _ = writeln!(
            t,
            "{}",
            json_line(&[
                ("code", &c.code),
                ("family", &c.family),
                ("label", &c.label),
                ("definition", &c.definition),
            ])
        );
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Rules");
    let _ = writeln!(t);
    for (i, rule) in RULES.iter().enumerate() {
        let _ = writeln!(t, "{}. {rule}", i + 1);
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Units");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "One unit per line, as JSON with the fields unit_id, speaker (INT = interviewer, \
RESP = respondent, any other label is the speaker label of the source), timestamp and text. \
Characters may appear as JSON escapes; read them as the characters they encode."
    );
    let _ = writeln!(t);
    for u in units {
        let _ = writeln!(
            t,
            "{}",
            json_line(&[
                ("unit_id", &u.unit_id),
                ("speaker", speaker_label(u)),
                ("timestamp", &u.timestamp),
                ("text", &u.text),
            ])
        );
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Output");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "Return exactly one JSON object and nothing else: no prose before or after it, \
no markdown code fence. The object has these fixed values for this part:"
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "- \"schema\": \"{OUTPUT_SCHEMA_ID}\"");
    let _ = writeln!(t, "- \"interview_id\": \"{}\"", seg.interview_id);
    let _ = writeln!(t, "- \"segmentation_hash\": \"{}\"", seg.hash);
    let _ = writeln!(t, "- \"part\": {part}");
    let _ = writeln!(t, "- \"of\": {of}");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "\"codings\" is an array with exactly one object per unit, in the order of the units \
above. Each object has \"unit_id\" and \"codes\", an array of one or more codes. Shape of the \
answer, where the angle brackets are placeholders:"
    );
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "{{\"schema\":\"{OUTPUT_SCHEMA_ID}\",\"interview_id\":\"{}\",\"segmentation_hash\":\"{}\",\"part\":{part},\"of\":{of},\"codings\":[{{\"unit_id\":\"<unit_id>\",\"codes\":[\"<CODE>\"]}}]}}",
        seg.interview_id, seg.hash
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "The answer must validate against this JSON Schema:");
    let _ = writeln!(t);
    let schema = part_schema(seg, codes, part, of);
    let mut pretty = String::new();
    canonical_pretty(&schema, 0, &mut pretty);
    let _ = writeln!(t, "{}", escape_dashes(&pretty));
    t
}

const RULES: &[&str] = &[
    "Use ONLY the codes of the codebook, written exactly as listed. You may not create, merge or rename codes.",
    "If no code applies, answer NA. NA is a valid and frequent answer; never force a code for the sake of completeness. NA is never combined with another code.",
    "A unit may carry several codes.",
    "Code what is SAID in the unit, not what you infer from the context of the interview. A unit that could not be coded on its own is coded NA.",
    "Interviewer turns (speaker INT) are coded NA by default, unless the interviewer reports a fact from the field in that turn.",
    "Write no comment, no justification and no memo.",
    "Process EVERY unit listed under UNITS, in order, without omitting any: one entry per unit, no unit twice, no unit that is not listed.",
];
