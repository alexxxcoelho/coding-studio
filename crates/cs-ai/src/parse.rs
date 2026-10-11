//! Validation of an AI answer pasted back by the candidate.
//!
//! Tolerant on the envelope (prose around the object, a markdown fence),
//! strict on the content: every problem is reported, none is repaired.

use std::collections::{BTreeMap, BTreeSet};

use cs_model::{AiOutput, CodeSet, ValidationIssue, NA, PHASE_2};
use serde_json::Value;

use crate::prompt::{allowed_for, listed_codes, part_unit_ids};
use crate::{PromptInput, OUTPUT_SCHEMA_ID};

const TOP_KEYS: &[&str] = &[
    "schema",
    "interview_id",
    "phase",
    "segmentation_hash",
    "part",
    "of",
    "codings",
];

fn issue(location: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        location: location.into(),
        message: message.into(),
    }
}

/// Balanced `{...}` slice starting at byte `start` (which must be `{`),
/// ignoring braces inside strings. `None` when the object never closes.
fn balanced_object(raw: &str, start: usize) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in raw[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The JSON object of an answer: the first balanced `{...}` that parses as
/// JSON, so prose or a markdown fence around it (even prose holding braces)
/// is ignored. When none parses, the first balanced candidate is returned so
/// the JSON error can be reported. `None` when no complete object exists.
pub(crate) fn extract_json_object(raw: &str) -> Option<&str> {
    let mut first = None;
    for (start, _) in raw.match_indices('{') {
        if let Some(obj) = balanced_object(raw, start) {
            if serde_json::from_str::<Value>(obj).is_ok() {
                return Some(obj);
            }
            first.get_or_insert(obj);
        }
    }
    first
}

fn read_u32(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    issues: &mut Vec<ValidationIssue>,
) -> Option<u32> {
    match obj.get(key) {
        None => {
            issues.push(issue("$", format!("missing field \"{key}\"")));
            None
        }
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 && n <= u32::MAX as u64 => Some(n as u32),
            _ => {
                issues.push(issue(
                    format!("$.{key}"),
                    format!("\"{key}\" must be an integer of 1 or more, found {v}"),
                ));
                None
            }
        },
    }
}

fn read_str<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
    issues: &mut Vec<ValidationIssue>,
) -> Option<&'a str> {
    match obj.get(key) {
        None => {
            issues.push(issue("$", format!("missing field \"{key}\"")));
            None
        }
        Some(Value::String(s)) => Some(s.as_str()),
        Some(v) => {
            issues.push(issue(
                format!("$.{key}"),
                format!("\"{key}\" must be a string, found {v}"),
            ));
            None
        }
    }
}

pub(crate) fn parse(
    raw: &str,
    input: &PromptInput,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    let seg = input.seg;
    let codebook = input.codebook;
    let mut issues = Vec::new();

    let Some(json_text) = extract_json_object(raw) else {
        return Err(vec![issue(
            "$",
            "no complete JSON object found in the answer",
        )]);
    };
    let value: Value = match serde_json::from_str(json_text) {
        Ok(v) => v,
        Err(e) => return Err(vec![issue("$", format!("invalid JSON: {e}"))]),
    };
    let Value::Object(obj) = value else {
        return Err(vec![issue("$", "the answer must be a JSON object")]);
    };

    for key in obj.keys() {
        if !TOP_KEYS.contains(&key.as_str()) {
            issues.push(issue("$", format!("unexpected field \"{key}\"")));
        }
    }

    if let Some(schema) = read_str(&obj, "schema", &mut issues) {
        if schema != OUTPUT_SCHEMA_ID {
            issues.push(issue(
                "$.schema",
                format!("\"schema\" must be \"{OUTPUT_SCHEMA_ID}\", found \"{schema}\""),
            ));
        }
    }
    let interview_id = read_str(&obj, "interview_id", &mut issues);
    if let Some(id) = interview_id {
        if id != seg.interview_id {
            issues.push(issue(
                "$.interview_id",
                format!(
                    "answer is for interview \"{id}\", expected \"{}\"",
                    seg.interview_id
                ),
            ));
        }
    }
    if let Some(h) = read_str(&obj, "segmentation_hash", &mut issues) {
        if h != seg.hash {
            issues.push(issue(
                "$.segmentation_hash",
                "segmentation_hash does not match the current segmentation: the answer comes from a prompt built on another segmentation; generate the prompt again",
            ));
        }
    }
    if let Some(phase) = read_u32(&obj, "phase", &mut issues) {
        if phase != u32::from(input.phase) {
            issues.push(issue(
                "$.phase",
                format!(
                    "answer is for phase {phase}, expected phase {}: paste it in the tab of the phase its prompt was built for",
                    input.phase
                ),
            ));
        }
    }
    let part = read_u32(&obj, "part", &mut issues);
    let of = read_u32(&obj, "of", &mut issues);
    if let (Some(p), Some(o)) = (part, of) {
        if p > o {
            issues.push(issue("$.part", format!("part {p} is greater than of {o}")));
        }
    }
    if let Some((ep, eo)) = expected_part {
        if part.is_some_and(|p| p != ep) || of.is_some_and(|o| o != eo) {
            issues.push(issue(
                "$.part",
                format!(
                    "answer is for part {} of {}, expected part {ep} of {eo}",
                    part.map_or("?".to_string(), |p| p.to_string()),
                    of.map_or("?".to_string(), |o| o.to_string())
                ),
            ));
        }
    }

    // Range of units this answer must cover: the part the candidate pasted
    // it for, otherwise the part the answer declares.
    let range: Option<(u32, u32)> = expected_part.or(match (part, of) {
        (Some(p), Some(o)) if p <= o => Some((p, o)),
        _ => None,
    });
    let expected_ids: Vec<String> = match range {
        Some((p, o)) => part_unit_ids(input.unit_ids, p, o),
        None => Vec::new(),
    };
    let expected_set: BTreeSet<&str> = expected_ids.iter().map(String::as_str).collect();
    let known: BTreeSet<&str> = seg.units.iter().map(|u| u.unit_id.as_str()).collect();
    let in_scope: BTreeSet<&str> = input.unit_ids.iter().map(String::as_str).collect();
    let phase2 = input.phase == PHASE_2;
    // Phase 1: the listed codes and NA. Phase 2: per unit, see below.
    let listed: BTreeSet<&str> = listed_codes(input)
        .into_iter()
        .map(|c| c.code.as_str())
        .chain(std::iter::once(NA))
        .collect();

    let mut codings: BTreeMap<String, CodeSet> = BTreeMap::new();
    match obj.get("codings") {
        None => issues.push(issue("$", "missing field \"codings\"")),
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let path = format!("$.codings[{i}]");
                let Value::Object(entry) = item else {
                    issues.push(issue(path, "each coding must be an object"));
                    continue;
                };
                for key in entry.keys() {
                    if key != "unit_id" && key != "codes" {
                        issues.push(issue(&path, format!("unexpected field \"{key}\"")));
                    }
                }
                let unit_id = match entry.get("unit_id") {
                    Some(Value::String(s)) => s.trim().to_string(),
                    _ => {
                        issues.push(issue(&path, "missing or non-string \"unit_id\""));
                        continue;
                    }
                };
                if !known.contains(unit_id.as_str()) {
                    issues.push(issue(&unit_id, "unknown unit_id"));
                } else if !in_scope.contains(unit_id.as_str()) {
                    issues.push(issue(
                        &unit_id,
                        format!("unit_id is not in the scope of phase {}", input.phase),
                    ));
                } else if range.is_some() && !expected_set.contains(unit_id.as_str()) {
                    issues.push(issue(&unit_id, "unit_id does not belong to this part"));
                }
                if codings.contains_key(&unit_id) {
                    issues.push(issue(&unit_id, "unit_id appears more than once"));
                    continue;
                }
                let codes = match entry.get("codes") {
                    Some(Value::Array(cs)) => cs,
                    _ => {
                        issues.push(issue(&unit_id, "missing or non-array \"codes\""));
                        continue;
                    }
                };
                let unit_allowed: BTreeSet<&str> = if phase2 {
                    allowed_for(input, &unit_id)
                        .iter()
                        .map(String::as_str)
                        .chain(std::iter::once(NA))
                        .collect()
                } else {
                    listed.clone()
                };
                let mut set = CodeSet::new();
                for c in codes {
                    let Some(s) = c.as_str() else {
                        issues.push(issue(&unit_id, format!("code {c} is not a string")));
                        continue;
                    };
                    let s = s.trim();
                    if s.is_empty() {
                        issues.push(issue(&unit_id, "empty code"));
                    } else if !unit_allowed.contains(s) {
                        let msg = if phase2 && (codebook.get(s).is_some() || listed.contains(s)) {
                            format!("code \"{s}\" is not among the allowed codes of this unit")
                        } else if codebook.get(s).is_some() {
                            format!("code \"{s}\" exists in the codebook but was not offered in the prompt")
                        } else {
                            format!("code \"{s}\" is not in the codebook")
                        };
                        issues.push(issue(&unit_id, msg));
                    } else {
                        set.insert(s.to_string());
                    }
                }
                if codes.is_empty() {
                    issues.push(issue(
                        &unit_id,
                        "empty codes array; use NA when no code applies",
                    ));
                }
                if set.contains(NA) && set.len() > 1 {
                    issues.push(issue(&unit_id, "NA cannot be combined with other codes"));
                }
                // Recorded even when invalid, so the unit is not also reported
                // as missing.
                codings.insert(unit_id, set);
            }
        }
        Some(_) => issues.push(issue("$.codings", "\"codings\" must be an array")),
    }

    let missing: Vec<&str> = expected_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !codings.contains_key(*id))
        .collect();
    if !missing.is_empty() && obj.get("codings").is_some_and(Value::is_array) {
        issues.push(issue(
            "$.codings",
            format!("{} unit(s) missing: {}", missing.len(), missing.join(", ")),
        ));
    }

    if !issues.is_empty() {
        return Err(issues);
    }
    let (part, of) = (part.unwrap_or(1), of.unwrap_or(1));
    Ok(AiOutput {
        interview_id: seg.interview_id.clone(),
        phase: input.phase,
        segmentation_hash: seg.hash.clone(),
        part,
        of,
        codings,
    })
}
