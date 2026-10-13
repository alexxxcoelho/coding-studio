//! Validation of an AI answer pasted back by the candidate.
//!
//! Tolerant on the envelope (prose around the object, a markdown fence),
//! strict on the content: every problem is reported, none is repaired.

use std::collections::{BTreeMap, BTreeSet};

use cs_model::pseudonym::Pseudonymiser;
use cs_model::{
    AiOutput, CandidateConcept, CandidateEvidence, CodeSet, ValidationIssue, NA, PHASE_2,
};
use serde_json::Value;

use crate::prompt::part_unit_ids;
use crate::{AnswerScope, OUTPUT_SCHEMA_ID};

const TOP_KEYS: &[&str] = &[
    "schema",
    "interview_id",
    "phase",
    "segmentation_hash",
    "part",
    "of",
    "codings",
    "candidate_concepts",
];

/// Longest accepted label of a candidate concept, in characters.
const MAX_LABEL: usize = 200;
/// Largest number of candidate concepts per part.
const MAX_CANDIDATES: usize = 50;

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
    scope: &AnswerScope,
    expected_part: Option<(u32, u32)>,
) -> Result<AiOutput, Vec<ValidationIssue>> {
    let seg = scope.seg;
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
        if phase != u32::from(PHASE_2) {
            issues.push(issue(
                "$.phase",
                format!("answer is for phase {phase}; only phase {PHASE_2} is coded by the AI"),
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
        Some((p, o)) => part_unit_ids(scope.unit_ids, p, o),
        None => Vec::new(),
    };
    let expected_set: BTreeSet<&str> = expected_ids.iter().map(String::as_str).collect();
    let known: BTreeSet<&str> = seg.units.iter().map(|u| u.unit_id.as_str()).collect();
    let in_scope: BTreeSet<&str> = scope.unit_ids.iter().map(String::as_str).collect();
    let offered: BTreeSet<&str> = scope
        .codes
        .iter()
        .map(String::as_str)
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
                        "unit_id is not in the scope of this prompt",
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
                let mut set = CodeSet::new();
                for c in codes {
                    let Some(s) = c.as_str() else {
                        issues.push(issue(&unit_id, format!("code {c} is not a string")));
                        continue;
                    };
                    let s = s.trim();
                    if s.is_empty() {
                        issues.push(issue(&unit_id, "empty code"));
                    } else if !offered.contains(s) {
                        issues.push(issue(
                            &unit_id,
                            format!("code \"{s}\" was not offered in the prompt"),
                        ));
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

    let candidates = match obj.get("candidate_concepts") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => parse_candidates(v, &expected_set, &mut issues),
    };

    if !issues.is_empty() {
        return Err(issues);
    }
    let (part, of) = (part.unwrap_or(1), of.unwrap_or(1));
    Ok(AiOutput {
        interview_id: seg.interview_id.clone(),
        phase: PHASE_2,
        segmentation_hash: seg.hash.clone(),
        part,
        of,
        codings,
        candidate_concepts: candidates,
    })
}

fn parse_candidates(
    value: &Value,
    part_units: &BTreeSet<&str>,
    issues: &mut Vec<ValidationIssue>,
) -> Vec<CandidateConcept> {
    let Value::Array(items) = value else {
        issues.push(issue(
            "$.candidate_concepts",
            "\"candidate_concepts\" must be an array",
        ));
        return Vec::new();
    };
    if items.len() > MAX_CANDIDATES {
        issues.push(issue(
            "$.candidate_concepts",
            format!("at most {MAX_CANDIDATES} candidate concepts per part"),
        ));
    }
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = format!("$.candidate_concepts[{i}]");
        let Value::Object(o) = item else {
            issues.push(issue(path, "each candidate concept must be an object"));
            continue;
        };
        for key in o.keys() {
            if !["label", "description", "unit_ids", "evidence"].contains(&key.as_str()) {
                issues.push(issue(&path, format!("unexpected field \"{key}\"")));
            }
        }
        let label = match o.get("label") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                issues.push(issue(&path, "missing or empty \"label\""));
                continue;
            }
        };
        if label.chars().count() > MAX_LABEL {
            issues.push(issue(
                &path,
                format!("label longer than {MAX_LABEL} characters"),
            ));
        }
        let description = match o.get("description") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.trim().is_empty() => None,
            Some(Value::String(s)) => Some(s.trim().to_string()),
            Some(_) => {
                issues.push(issue(&path, "\"description\" must be a string"));
                None
            }
        };
        let mut unit_ids: Vec<String> = Vec::new();
        match o.get("unit_ids") {
            Some(Value::Array(us)) if !us.is_empty() => {
                for u in us {
                    match u.as_str().map(str::trim) {
                        Some(id) if part_units.contains(id) => {
                            if !unit_ids.iter().any(|x| x == id) {
                                unit_ids.push(id.to_string());
                            }
                        }
                        Some(id) => issues.push(issue(
                            &path,
                            format!("unit \"{id}\" does not belong to this part"),
                        )),
                        None => issues.push(issue(&path, "unit_ids must hold strings")),
                    }
                }
            }
            _ => issues.push(issue(&path, "\"unit_ids\" must be a non-empty array")),
        }
        let mut evidence = Vec::new();
        match o.get("evidence") {
            None | Some(Value::Null) => {}
            Some(Value::Array(es)) => {
                for e in es {
                    let unit = e.get("unit_id").and_then(Value::as_str).map(str::trim);
                    let quote = e.get("quote").and_then(Value::as_str).map(str::trim);
                    match (unit, quote) {
                        (Some(u), Some(q)) if !q.is_empty() && part_units.contains(u) => evidence
                            .push(CandidateEvidence {
                                unit_id: u.to_string(),
                                quote: q.to_string(),
                            }),
                        _ => issues.push(issue(
                            &path,
                            "each evidence needs a unit_id of this part and a quote",
                        )),
                    }
                }
            }
            Some(_) => issues.push(issue(&path, "\"evidence\" must be an array")),
        }
        out.push(CandidateConcept {
            label,
            description,
            unit_ids,
            evidence,
        });
    }
    out
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A9: a quotation that is not in the unit is refused.
pub(crate) fn check_quotes(
    output: &AiOutput,
    unit_texts: &BTreeMap<String, String>,
    pseudonyms: &Pseudonymiser,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (i, c) in output.candidate_concepts.iter().enumerate() {
        for e in &c.evidence {
            let found = unit_texts
                .get(&e.unit_id)
                .map(|t| squash(&pseudonyms.mask(t)))
                .is_some_and(|t| t.contains(&squash(&e.quote)));
            if !found {
                issues.push(issue(
                    format!("$.candidate_concepts[{i}]"),
                    format!(
                        "the quotation \"{}\" is not found word for word in {}",
                        e.quote, e.unit_id
                    ),
                ));
            }
        }
    }
    issues
}
