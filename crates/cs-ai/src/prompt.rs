//! Copy-paste prompts for the blind secondary coder (English translation of
//! P-04), one template per coding phase.
//!
//! The prompt carries the codes and the units in scope only. Human codes,
//! memos, summaries and record sheets never enter it: the AI coder works
//! blind. In phase 2 each unit carries the list of codes it may receive,
//! which is the reconciled outcome of phase 1 (by design), never the
//! candidate's phase 2 coding.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cs_model::{canonical_hash, sha256_hex, Code, PromptPart, SpeakerRole, Unit, NA};
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

/// Title of a phase, used in prompts and reports.
pub fn phase_title(phase: u8) -> String {
    match phase {
        1 => "Phase 1, TOE contexts".to_string(),
        2 => "Phase 2, study-specific codes".to_string(),
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

/// Codes a unit may receive in phase 2 (empty when the caller gave none).
pub(crate) fn allowed_for<'a>(input: &PromptInput<'a>, unit_id: &str) -> &'a [String] {
    input
        .allowed
        .and_then(|m| m.get(unit_id))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Codes listed in the CODEBOOK section. Phase 1: the codes offered to the AI
/// (`Codebook::prompt_codes`). Phase 2: every codebook code that appears in
/// the allowed list of at least one unit in scope, in codebook order.
pub(crate) fn listed_codes<'a>(input: &PromptInput<'a>) -> Vec<&'a Code> {
    if input.phase == cs_model::PHASE_2 {
        let used: std::collections::BTreeSet<&str> = input
            .unit_ids
            .iter()
            .flat_map(|id| allowed_for(input, id).iter().map(String::as_str))
            .collect();
        input
            .codebook
            .codes
            .iter()
            .filter(|c| used.contains(c.code.as_str()))
            .collect()
    } else {
        input.codebook.prompt_codes()
    }
}

/// Hash binding a prompt (and its answers) to the codes it offered. Phase 1:
/// the listed codes (equal to `Codebook::prompt_hash`). Phase 2: the listed
/// codes and the allowed list of every unit in scope.
pub(crate) fn codebook_hash(input: &PromptInput) -> String {
    let listed = listed_codes(input);
    if input.phase == cs_model::PHASE_2 {
        let allowed: BTreeMap<&str, &[String]> = input
            .unit_ids
            .iter()
            .map(|id| (id.as_str(), allowed_for(input, id)))
            .collect();
        canonical_hash(&(listed, allowed))
    } else {
        canonical_hash(&listed)
    }
}

// --------------------------------------------------------------- build

pub(crate) fn build(input: &PromptInput, batch_size: usize) -> Vec<PromptPart> {
    let units = scope_units(input);
    let of = parts_for_batch(units.len(), batch_size);
    let listed = listed_codes(input);
    let hash = codebook_hash(input);
    (1..=of)
        .map(|part| {
            let part_units = part_slice(&units, part, of);
            let text = render(input, &listed, &hash, part_units, part, of);
            PromptPart {
                interview_id: input.seg.interview_id.clone(),
                phase: input.phase,
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
/// part fixed and the codes restricted to those valid in this part.
fn part_schema(input: &PromptInput, valid: &[&str], part: u32, of: u32) -> Value {
    let mut schema = crate::output_json_schema(input.phase);
    let props = &mut schema["properties"];
    props["interview_id"] = json!({ "const": input.seg.interview_id });
    props["segmentation_hash"] = json!({ "const": input.seg.hash });
    props["part"] = json!({ "const": part });
    props["of"] = json!({ "const": of });
    props["codings"]["items"]["properties"]["codes"]["items"] = json!({ "enum": valid });
    schema
}

/// Codes valid somewhere in this part, plus NA, without duplicates.
fn valid_codes<'a>(input: &PromptInput<'a>, listed: &[&'a Code], units: &[&Unit]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    if input.phase == cs_model::PHASE_2 {
        // Codebook order first, then any allowed code missing from the codebook.
        let in_part: std::collections::BTreeSet<&str> = units
            .iter()
            .flat_map(|u| allowed_for(input, &u.unit_id).iter().map(String::as_str))
            .collect();
        for c in listed {
            if in_part.contains(c.code.as_str()) {
                out.push(c.code.as_str());
            }
        }
        for u in units {
            for a in allowed_for(input, &u.unit_id) {
                if !out.contains(&a.as_str()) {
                    out.push(a.as_str());
                }
            }
        }
    } else {
        out.extend(listed.iter().map(|c| c.code.as_str()));
    }
    out.push(NA);
    out
}

fn code_line(c: &Code) -> String {
    json_line(&[
        ("code", json!(c.code)),
        ("family", json!(c.family)),
        ("label", json!(c.label)),
        ("definition", json!(c.definition)),
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
    let phase = input.phase;
    let phase2 = phase == cs_model::PHASE_2;
    let mut t = String::new();
    let first = units.first().map(|u| u.unit_id.as_str()).unwrap_or("");
    let last = units.last().map(|u| u.unit_id.as_str()).unwrap_or("");
    let phase_line = if phase2 {
        "Phase 2 of 2: study-specific codes"
    } else {
        "Phase 1 of 2: TOE contexts"
    };

    // Writing to a String cannot fail; the results are ignored on purpose.
    let _ = writeln!(t, "# Coding Studio: blind secondary coding");
    let _ = writeln!(t);
    let _ = writeln!(t, "{phase_line}");
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
    if phase2 {
        let _ = writeln!(
            t,
            "Each unit listed under UNITS was found relevant in an earlier coding phase and \
carries the list of codes that may apply to it (allowed_codes). For each unit, assign the \
code or codes of its allowed_codes that apply. The codebook gives the definition of every \
code that appears in an allowed_codes list."
        );
    } else {
        let _ = writeln!(
            t,
            "For each unit listed under UNITS, decide which context of the \
technology-organisation-environment (TOE) framework the unit speaks to: the technological \
context (TECH), the organisational context (ORG) or the environmental and \
interorganisational context (ENV). A unit that touches two or three contexts carries each \
of them. INDUCTIVE-PENDING marks material that is relevant to the study but that none of \
the three contexts accommodates. NA marks a unit that is not relevant."
        );
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Codebook");
    let _ = writeln!(t);
    if phase2 {
        let _ = writeln!(
            t,
            "One code per line, as JSON with the fields code, family, label and definition, \
grouped by family. A code is valid for a unit only if it appears in that unit's \
allowed_codes; NA is always valid."
        );
        let mut families: Vec<&str> = Vec::new();
        for c in listed {
            if !families.contains(&c.family.as_str()) {
                families.push(c.family.as_str());
            }
        }
        for fam in families {
            let _ = writeln!(t);
            let _ = writeln!(t, "### {fam}");
            let _ = writeln!(t);
            for c in listed.iter().filter(|c| c.family == fam) {
                let _ = writeln!(t, "{}", code_line(c));
            }
        }
    } else {
        let _ = writeln!(
            t,
            "One code per line, as JSON with the fields code, family, label and definition. \
These {} codes and NA are the only valid values.",
            listed.len()
        );
        let _ = writeln!(t);
        for c in listed {
            let _ = writeln!(t, "{}", code_line(c));
        }
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Rules");
    let _ = writeln!(t);
    let rules: &[&str] = if phase2 { RULES_PHASE2 } else { RULES_PHASE1 };
    for (i, rule) in rules.iter().enumerate() {
        let _ = writeln!(t, "{}. {rule}", i + 1);
    }
    let _ = writeln!(t);

    let _ = writeln!(t, "## Units");
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "One unit per line, as JSON with the fields unit_id, speaker (INT = interviewer, \
RESP = respondent, any other label is the speaker label of the source), timestamp and text{}. \
Characters may appear as JSON escapes; read them as the characters they encode.",
        if phase2 {
            ", then allowed_codes, the codes valid for that unit"
        } else {
            ""
        }
    );
    let _ = writeln!(t);
    for u in units {
        let mut fields = vec![
            ("unit_id", json!(u.unit_id)),
            ("speaker", json!(speaker_label(u))),
            ("timestamp", json!(u.timestamp)),
            ("text", json!(u.text)),
        ];
        if phase2 {
            fields.push(("allowed_codes", json!(allowed_for(input, &u.unit_id))));
        }
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
    let valid = valid_codes(input, listed, units);
    let schema = part_schema(input, &valid, part, of);
    let mut pretty = String::new();
    canonical_pretty(&schema, 0, &mut pretty);
    let _ = writeln!(t, "{}", escape_dashes(&pretty));
    t
}

const RULES_PHASE1: &[&str] = &[
    "Use ONLY the codes of the codebook, written exactly as listed. You may not create, merge or rename codes.",
    "If no code applies, answer NA. NA is a valid and frequent answer; never force a code for the sake of completeness. NA is never combined with another code.",
    "A unit may carry several codes.",
    "Code what is SAID in the unit, not what you infer from the context of the interview. A unit that could not be coded on its own is coded NA.",
    "Interviewer turns (speaker INT) are coded NA by default, unless the interviewer reports a fact from the field in that turn.",
    "Write no comment, no justification and no memo.",
    "Process EVERY unit listed under UNITS, in order, without omitting any: one entry per unit, no unit twice, no unit that is not listed.",
];

const RULES_PHASE2: &[&str] = &[
    "For each unit, use ONLY codes from that unit's allowed_codes, written exactly as listed. A code of the codebook that is absent from a unit's allowed_codes is not valid for that unit. You may not create, merge or rename codes.",
    "If none of the unit's allowed_codes applies, answer NA. NA is a valid answer; never force a code for the sake of completeness. NA is never combined with another code.",
    "A unit may carry several codes.",
    "Code what is SAID in the unit, not what you infer from the context of the interview. A unit that could not be coded on its own is coded NA.",
    "Interviewer turns (speaker INT) are coded NA by default, unless the interviewer reports a fact from the field in that turn.",
    "Write no comment, no justification and no memo.",
    "Process EVERY unit listed under UNITS, in order, without omitting any: one entry per unit, no unit twice, no unit that is not listed.",
];
