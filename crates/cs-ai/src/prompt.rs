//! Copy-paste prompts for the blind secondary coder (English translation of
//! P-04). Only the detailed codes of phase 2 are challenged by the AI.
//!
//! The prompt carries the codes offered and the units in scope only. The
//! candidate's codes, phase 1 contexts, labels to clarify, memos, summaries
//! and record sheets never enter it: the AI coder works blind.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cs_model::{canonical_hash, sha256_hex, Code, PromptPart, SpeakerRole, Unit, NA, PHASE_2};
use serde_json::{json, Value};

use crate::{PromptInput, OUTPUT_SCHEMA_ID, PROMPT_VERSION};

/// Number of parts for `n` units and a requested batch size (0 = one part).
pub fn parts_for_batch(n: usize, batch_size: usize) -> u32 {
    if batch_size == 0 || n <= batch_size {
        1
    } else {
        n.div_ceil(batch_size) as u32
    }
}

/// Unit IDs of part `part` (1-based) when the units in scope are split into
/// `of` parts.
///
/// The split depends on `of` only: every part holds `ceil(n / of)` units, the
/// last one the remainder. `build_prompts` derives `of` from the batch size
/// and then uses this same function, so the parser can recompute the range of
/// a part from the `part`/`of` fields of an answer alone.
pub fn part_unit_ids(unit_ids: &[String], part: u32, of: u32) -> Vec<String> {
    part_slice(unit_ids, part, of).to_vec()
}

pub(crate) fn part_slice<T>(items: &[T], part: u32, of: u32) -> &[T] {
    if of == 0 || part == 0 || part > of {
        return &[];
    }
    let n = items.len();
    let size = n.div_ceil(of as usize);
    let start = (size * (part as usize - 1)).min(n);
    let end = (size * part as usize).min(n);
    &items[start..end]
}

/// Title of a phase, used in reports.
pub fn phase_title(phase: u8) -> String {
    match phase {
        1 => "Phase 1, provisional TOE pre-classification".to_string(),
        2 => "Phase 2, concrete codes".to_string(),
        p => format!("Phase {p}"),
    }
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
pub(crate) fn json_line(fields: &[(&str, Value)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}:{}",
                json_str(k),
                serde_json::to_string(v).expect("json values serialise")
            )
        })
        .collect();
    escape_dashes(&format!("{{{}}}", body.join(",")))
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).expect("strings always serialise")
}

/// Pretty JSON with object keys sorted, two-space indent: stable whatever the
/// key order of the `Value` maps.
pub(crate) fn canonical_pretty(value: &Value, indent: usize, out: &mut String) {
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

// --------------------------------------------------------------- scope

/// Units in scope, in the order of `input.unit_ids`. IDs unknown to the
/// segmentation are skipped (the caller builds the scope from it).
pub(crate) fn scope_units<'a>(input: &PromptInput<'a>) -> Vec<&'a Unit> {
    let by_id: BTreeMap<&str, &Unit> = input
        .seg
        .units
        .iter()
        .map(|u| (u.unit_id.as_str(), u))
        .collect();
    input
        .unit_ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .collect()
}

/// The TOE groups of the codebook section, in display order.
const GROUPS: [(Option<&str>, &str); 4] = [
    (Some("T"), "Technological context (T)"),
    (Some("O"), "Organisational context (O)"),
    (
        Some("E"),
        "Environmental and interorganisational context (E)",
    ),
    (None, "Not attached to a TOE context"),
];

/// Codes in display order: grouped by TOE attachment, codebook order within.
pub(crate) fn ordered_codes(codes: &[Code]) -> Vec<&Code> {
    GROUPS
        .iter()
        .flat_map(|(toe, _)| codes.iter().filter(move |c| c.toe.as_deref() == *toe))
        .collect()
}

// --------------------------------------------------------------- build

pub(crate) fn build(input: &PromptInput, batch_size: usize) -> Vec<PromptPart> {
    let units = scope_units(input);
    let of = parts_for_batch(units.len(), batch_size);
    let listed = ordered_codes(input.codes);
    let hash = canonical_hash(&listed);
    let codes: Vec<String> = listed.iter().map(|c| c.code.clone()).collect();
    (1..=of)
        .map(|part| {
            let part_units = part_slice(&units, part, of);
            let text = render(input, &listed, &hash, part_units, part, of);
            PromptPart {
                interview_id: input.seg.interview_id.clone(),
                phase: PHASE_2,
                batch_id: 0,
                codes: codes.clone(),
                part,
                of,
                unit_ids: part_units.iter().map(|u| u.unit_id.clone()).collect(),
                segmentation_hash: input.seg.hash.clone(),
                codebook_hash: hash.clone(),
                prompt_version: PROMPT_VERSION.to_string(),
                sha256: sha256_hex(text.as_bytes()),
                text,
            }
        })
        .collect()
}

/// Schema embedded in a prompt: the generic schema with the values of this
/// part fixed and the codes restricted to those offered.
fn part_schema(input: &PromptInput, valid: &[&str], part: u32, of: u32) -> Value {
    let mut schema = crate::output_json_schema();
    let props = &mut schema["properties"];
    props["interview_id"] = json!({ "const": input.seg.interview_id });
    props["segmentation_hash"] = json!({ "const": input.seg.hash });
    props["part"] = json!({ "const": part });
    props["of"] = json!({ "const": of });
    props["codings"]["items"]["properties"]["codes"]["items"] = json!({ "enum": valid });
    schema
}

/// Replaces every unit id, `<LETTERS>-<digits>-U<digits>`, in a code text with
/// `[unit]`. The pattern is `[A-Z]{2,4}-[0-9]{1,3}-U[0-9]{1,4}`, matched
/// leftmost and greedily without word boundaries, as a regex would; it is
/// hand-written because cs-ai has no regex dependency. Any interview's ids are
/// redacted, not only the prompted one's: the codebook never names a unit.
pub(crate) fn redact_unit_ids(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        match unit_id_len_at(b, i) {
            Some(len) => {
                out.push_str(&text[last..i]);
                out.push_str("[unit]");
                i += len;
                last = i;
            }
            None => i += 1,
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Length of the unit id starting at `start`, if one starts there.
fn unit_id_len_at(b: &[u8], start: usize) -> Option<usize> {
    fn run(b: &[u8], from: usize, max: usize, pred: impl Fn(u8) -> bool) -> usize {
        b.iter()
            .skip(from)
            .take(max)
            .take_while(|&&c| pred(c))
            .count()
    }
    // [A-Z]{2,4}: a longer run leaves an uppercase letter where '-' is needed.
    let letters = run(b, start, 4, |c| c.is_ascii_uppercase());
    if letters < 2 {
        return None;
    }
    let mut p = start + letters;
    if b.get(p) != Some(&b'-') {
        return None;
    }
    p += 1;
    // [0-9]{1,3}, then '-'.
    let digits = run(b, p, 3, |c| c.is_ascii_digit());
    if digits == 0 {
        return None;
    }
    p += digits;
    if b.get(p) != Some(&b'-') {
        return None;
    }
    p += 1;
    // U[0-9]{1,4}
    if b.get(p) != Some(&b'U') {
        return None;
    }
    p += 1;
    let n = run(b, p, 4, |c| c.is_ascii_digit());
    if n == 0 {
        return None;
    }
    Some(p + n - start)
}

fn code_line(c: &Code) -> String {
    json_line(&[
        ("code", json!(c.code)),
        ("label", json!(redact_unit_ids(&c.label))),
        ("definition", json!(redact_unit_ids(&c.definition))),
    ])
}

fn render(
    input: &PromptInput,
    listed: &[&Code],
    codebook_hash: &str,
    units: &[&Unit],
    part: u32,
    of: u32,
) -> String {
    let seg = input.seg;
    let phase = PHASE_2;
    let mut t = String::new();
    let first = units.first().map(|u| u.unit_id.as_str()).unwrap_or("");
    let last = units.last().map(|u| u.unit_id.as_str()).unwrap_or("");

    // Writing to a String cannot fail; the results are ignored on purpose.
    let _ = writeln!(t, "# Coding Studio: blind secondary coding");
    let _ = writeln!(t);
    let _ = writeln!(t, "Detailed coding with concrete codes");
    let _ = writeln!(t);
    let _ = writeln!(t, "interview_id: {}", seg.interview_id);
    let _ = writeln!(t, "phase: {phase}");
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
        "For each unit listed under UNITS, assign the code or codes of the codebook that \
apply. The codes name concrete mechanisms, such as a legal obligation, a customer requirement \
or a lack of skills. They are grouped by the context of the technology-organisation-environment \
(TOE) framework they belong to; the grouping is there for orientation only, and any code may \
apply to any unit."
    );
    let _ = writeln!(t);

    let _ = writeln!(t, "## Codebook");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "One code per line, as JSON with the fields code, label and definition. These {} \
codes and NA are the only valid values.",
        listed.len()
    );
    for (toe, title) in GROUPS {
        let group: Vec<&&Code> = listed.iter().filter(|c| c.toe.as_deref() == toe).collect();
        if group.is_empty() {
            continue;
        }
        let _ = writeln!(t);
        let _ = writeln!(t, "### {title}");
        let _ = writeln!(t);
        for c in group {
            let _ = writeln!(t, "{}", code_line(c));
        }
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
        let fields = vec![
            ("unit_id", json!(u.unit_id)),
            ("speaker", json!(speaker_label(u))),
            ("timestamp", json!(u.timestamp)),
            ("text", json!(u.text)),
        ];
        let _ = writeln!(t, "{}", json_line(&fields));
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
    let _ = writeln!(t, "- \"phase\": {phase}");
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
        "{{\"schema\":\"{OUTPUT_SCHEMA_ID}\",\"interview_id\":\"{}\",\"phase\":{phase},\"segmentation_hash\":\"{}\",\"part\":{part},\"of\":{of},\"codings\":[{{\"unit_id\":\"<unit_id>\",\"codes\":[\"<CODE>\"]}}]}}",
        seg.interview_id, seg.hash
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "The answer must validate against this JSON Schema:");
    let _ = writeln!(t);
    let mut valid: Vec<&str> = listed.iter().map(|c| c.code.as_str()).collect();
    valid.push(NA);
    let schema = part_schema(input, &valid, part, of);
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
