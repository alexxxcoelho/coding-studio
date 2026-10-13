//! Audit package: everything needed to reconstruct the coding of one
//! interview, written to a new folder that is never overwritten, with a
//! manifest of SHA-256 hashes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::phase::{PHASE_1, PHASE_2};
use cs_model::{
    sha256_hex, Arbitration, ArbitrationChoice, Case, Claim, CodeSet, CsError, HumanCoding,
    HumanSnapshot, RqMemo, SaturationRow, ToClarify, Unit, UnitCorrection, UnitFlags,
    VerificationCoverage, VerificationStatus, VerificationTarget,
};
use cs_store::Store;
use serde::Serialize;
use serde_json::{json, Value};

/// Git description of the source tree, set at build time through the
/// `CS_GIT_DESCRIBE` environment variable; `unknown` when it was not set.
const GIT_DESCRIBE: &str = match option_env!("CS_GIT_DESCRIBE") {
    Some(s) => s,
    None => "unknown",
};

#[derive(Debug, Serialize)]
pub struct ExportedFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// Status of the detailed coding in the package.
#[derive(Debug, Serialize)]
pub struct CodingStatus {
    pub n_units_phase1: usize,
    pub n_coded_phase1: usize,
    pub n_units_phase2: usize,
    pub n_coded_phase2: usize,
    pub n_runs: usize,
    pub n_complete_runs: usize,
    pub n_disagreements: usize,
    pub n_sampled: usize,
    pub n_decided: usize,
    pub n_revised_after_ai: usize,
    pub n_to_clarify_open: usize,
    pub n_cases: usize,
    pub n_snapshots: usize,
    pub n_claims: usize,
    /// Verification coverage of the interview; reported apart from the counts above.
    pub verification: VerificationCoverage,
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub status: CodingStatus,
}

pub(crate) struct Writer {
    pub dir: PathBuf,
    pub files: Vec<ExportedFile>,
}

impl Writer {
    pub fn write(&mut self, name: &str, content: &[u8]) -> Result<()> {
        let path = self.dir.join(name);
        if path.exists() {
            anyhow::bail!("refusing to overwrite {}", path.display());
        }
        fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        self.files.push(ExportedFile {
            name: name.to_string(),
            sha256: sha256_hex(content),
            size: content.len() as u64,
        });
        Ok(())
    }

    /// Adds a file that was written into the folder by another means (the database snapshot).
    pub fn register(&mut self, name: &str) -> Result<()> {
        let path = self.dir.join(name);
        let content = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        self.files.push(ExportedFile {
            name: name.to_string(),
            sha256: sha256_hex(&content),
            size: content.len() as u64,
        });
        Ok(())
    }
}

/// Keeps a run label safe as a file name part.
fn safe(label: &str) -> String {
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "run".to_string()
    } else {
        s
    }
}

pub(crate) fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// `unit_id,codes,derived_contexts` for the units with a phase 1 coding, in
/// segmentation order. `derived_contexts` comes from the phase 2 codes (see
/// `Store::phase2_context_for_unit`).
fn phase1_contexts_csv(
    store: &Store,
    interview_id: &str,
    p1: &cs_model::CodingMap,
    all_units: &[String],
) -> Result<String> {
    let mut out = String::from("unit_id,codes,derived_contexts\n");
    for unit_id in all_units {
        if let Some(codes) = p1.get(unit_id) {
            let derived = store
                .phase2_context_for_unit(interview_id, unit_id)?
                .join(";");
            out.push_str(&format!(
                "{},{},{}\n",
                csv_field(unit_id),
                csv_field(&codes_list(codes)),
                csv_field(&derived)
            ));
        }
    }
    Ok(out)
}

/// Codes of a set, sorted, joined with `;`.
fn codes_list(codes: &CodeSet) -> String {
    codes.iter().cloned().collect::<Vec<_>>().join(";")
}

/// Codes of a set, or `(none)` when empty.
fn codes_or_none(codes: &CodeSet) -> String {
    if codes.is_empty() {
        "(none)".to_string()
    } else {
        codes_list(codes)
    }
}

fn codes_text(c: &HumanCoding) -> String {
    codes_list(&c.codes)
}

fn opt_text(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// One row per case, lists joined with `;`.
fn cases_csv(cases: &[Case]) -> String {
    let mut out = String::from(
        "case_id,label,interview_id,interviews,decision,granularity,carrier_chosen,carriers_rejected,account_type,swiss_link,stage,outcome_status,n_units\n",
    );
    for c in cases {
        let row = [
            csv_field(&c.case_id),
            csv_field(&c.label),
            csv_field(&c.interview_id),
            csv_field(&c.interviews.join(";")),
            csv_field(opt_text(&c.decision)),
            csv_field(opt_text(&c.granularity)),
            csv_field(opt_text(&c.carrier_chosen)),
            csv_field(&c.carriers_rejected.join(";")),
            csv_field(opt_text(&c.account_type)),
            csv_field(opt_text(&c.swiss_link)),
            csv_field(opt_text(&c.stage)),
            csv_field(opt_text(&c.outcome_status)),
            c.units.len().to_string(),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// One row per attributed unit. The interview is the prefix before `-U`, as in the store.
fn case_units_csv(cases: &[Case]) -> String {
    let mut out = String::from("case_id,unit_id,interview_id\n");
    for c in cases {
        for unit in &c.units {
            let interview = unit.rsplit_once("-U").map_or("", |(i, _)| i);
            out.push_str(&format!(
                "{},{},{}\n",
                csv_field(&c.case_id),
                csv_field(unit),
                csv_field(interview)
            ));
        }
    }
    out
}

fn snapshot_csv(snap: &HumanSnapshot) -> String {
    let mut out = String::from("unit_id,codes\n");
    for (unit_id, codes) in &snap.codings {
        out.push_str(&format!(
            "{},{}\n",
            csv_field(unit_id),
            csv_field(&codes_list(codes))
        ));
    }
    out
}

fn decision_text(choice: &ArbitrationChoice) -> String {
    match choice {
        ArbitrationChoice::Human => "candidate's coding kept".to_string(),
        ArbitrationChoice::Ai { run_label } => format!("AI run {run_label} adopted"),
        ArbitrationChoice::Custom => "new code set decided by the candidate".to_string(),
    }
}

/// Units whose current coding differs from their snapshot, appended after the review log.
fn revisions_md(items: &[cs_model::ReviewItem]) -> String {
    let mut s = String::from("\n## Revisions since the snapshot\n\n");
    s.push_str(
        "Units whose current coding differs from the human reference frozen when their AI batch was generated. The AI answers were compared with the snapshot, not with these current codes.\n\n",
    );
    let revised: Vec<&cs_model::ReviewItem> =
        items.iter().filter(|i| i.revised_since_snapshot).collect();
    if revised.is_empty() {
        s.push_str("None.\n");
        return s;
    }
    for item in revised {
        let decision = match &item.decision {
            Some(a) => format!(
                "{}, final codes {}",
                decision_text(&a.choice),
                codes_or_none(&a.final_codes)
            ),
            None => "pending".to_string(),
        };
        s.push_str(&format!(
            "- `{}`: snapshot codes {}; current codes {}; decision: {}.\n",
            item.unit_id,
            codes_or_none(&item.human),
            codes_or_none(&item.human_current),
            decision
        ));
    }
    s
}

/// The snapshot references of the complete runs, appended after the review log.
fn snapshots_md(refs: &[cs_model::ReviewSnapshotRef]) -> String {
    let mut s = String::from("\n## Snapshots\n\n");
    s.push_str(
        "The human reference of each complete run, frozen when its prompt batch was generated. Each SHA-256 is that of the snapshot codings in the matching human_snapshot_batch<id>.json.\n\n",
    );
    if refs.is_empty() {
        s.push_str("None.\n");
        return s;
    }
    s.push_str("| Run | Snapshot id | SHA-256 |\n|---|---|---|\n");
    for r in refs {
        s.push_str(&format!(
            "| {} | {} | `{}` |\n",
            r.run_label.replace('|', "\\|"),
            r.snapshot_id,
            r.sha256
        ));
    }
    s
}

fn human_csv(order: &[String], codings: &BTreeMap<String, HumanCoding>) -> String {
    let mut out = String::from("unit_id,codes,revised_after_ai\n");
    for unit_id in order {
        if let Some(c) = codings.get(unit_id) {
            out.push_str(&format!(
                "{},{},{}\n",
                csv_field(unit_id),
                csv_field(&codes_text(c)),
                c.revised_after_ai as u8
            ));
        }
    }
    out
}

fn human_full_csv(order: &[String], codings: &BTreeMap<String, HumanCoding>) -> String {
    let mut out = String::from(
        "unit_id,codes,subcode,modifier,ev_level,provenance,memo,excerpt,revised_after_ai\n",
    );
    for unit_id in order {
        let Some(c) = codings.get(unit_id) else {
            continue;
        };
        let opt = |v: &Option<String>| csv_field(v.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            csv_field(unit_id),
            csv_field(&codes_text(c)),
            opt(&c.subcode),
            opt(&c.modifier),
            opt(&c.ev_level),
            opt(&c.provenance),
            opt(&c.memo),
            opt(&c.excerpt),
            c.revised_after_ai as u8,
        ));
    }
    out
}

fn to_clarify_csv(items: &[ToClarify]) -> String {
    let mut out = String::from(
        "id,unit_id,label,note,created_at,created_by,resolution,code,rationale,resolved_at,resolved_by\n",
    );
    for t in items {
        let (kind, code, rationale, at, by) = match &t.resolved {
            Some(r) => (
                match r.kind {
                    cs_model::ResolutionKind::NewCode => "new_code",
                    cs_model::ResolutionKind::Mapped => "mapped",
                },
                r.code.as_str(),
                r.rationale.as_deref().unwrap_or(""),
                r.at.as_str(),
                r.by.as_str(),
            ),
            None => ("open", "", "", "", ""),
        };
        out.push_str(
            &[
                t.id.to_string(),
                csv_field(&t.unit_id),
                csv_field(&t.label),
                csv_field(&t.note),
                csv_field(&t.created_at),
                csv_field(&t.created_by),
                kind.to_string(),
                csv_field(code),
                csv_field(rationale),
                csv_field(at),
                csv_field(by),
            ]
            .join(","),
        );
        out.push('\n');
    }
    out
}

/// Markdown synthesis of the cases of one interview.
pub(crate) fn cases_md(interview_id: &str, cases: &[Case]) -> String {
    let mut s = format!("# Case syntheses, {interview_id}\n\n");
    if cases.is_empty() {
        s.push_str("No case recorded.\n");
        return s;
    }
    for c in cases {
        s.push_str(&format!("## {}, {}\n\n", c.case_id, c.label));
        for (name, value) in [
            ("Decision", &c.decision),
            ("Granularity", &c.granularity),
            ("Carrier chosen", &c.carrier_chosen),
            ("Account type", &c.account_type),
            ("Swiss link", &c.swiss_link),
        ] {
            if let Some(v) = value {
                s.push_str(&format!("{name}: {v}\n"));
            }
        }
        if !c.carriers_rejected.is_empty() {
            s.push_str(&format!(
                "Carriers rejected: {}\n",
                c.carriers_rejected.join("; ")
            ));
        }
        if c.units.is_empty() {
            s.push_str("Attributed units: 0.\n");
        } else {
            s.push_str(&format!(
                "Attributed units: {} ({}).\n",
                c.units.len(),
                c.units.join(", ")
            ));
        }
        s.push('\n');
        if let Some(stage) = &c.stage {
            s.push_str(&format!("Stage: {stage}. "));
        }
        if let Some(status) = &c.outcome_status {
            s.push_str(&format!("Outcome status: {status}."));
        }
        if c.stage.is_some() || c.outcome_status.is_some() {
            s.push_str("\n\n");
        }
        for (name, f) in c.fields() {
            let title = match name {
                "trigger" => "Trigger",
                "alternatives" => "Alternatives",
                "constraints" => "Constraints",
                "choice" => "Choice",
                _ => "Result",
            };
            s.push_str(&format!("### {title}\n\n"));
            s.push_str(if f.text.is_empty() {
                "(not filled)"
            } else {
                f.text.as_str()
            });
            s.push_str("\n\n");
            if !f.contradictions.is_empty() {
                s.push_str(&format!("Contradictions: {}\n\n", f.contradictions));
            }
            if !f.missing.is_empty() {
                s.push_str(&format!("Missing: {}\n\n", f.missing));
            }
            if !f.evidence.is_empty() {
                s.push_str(&format!("Evidence: {}.\n\n", f.evidence.join(", ")));
            }
        }
        if let Some(n) = &c.notes {
            s.push_str(&format!("Notes: {n}\n\n"));
        }
    }
    s
}

/// Overlays the corrections on the segmented units. Returns the current units
/// (text and role of the latest correction, `corrected` set) and every
/// correction of the interview, oldest first. The segmentation is not touched.
fn current_units(
    store: &Store,
    interview_id: &str,
    seg: &cs_model::Segmentation,
) -> Result<(Vec<Unit>, Vec<UnitCorrection>)> {
    let mut units = Vec::with_capacity(seg.units.len());
    let mut corrections = Vec::new();
    for unit in &seg.units {
        let mut unit = unit.clone();
        let mine = store.unit_corrections(interview_id, &unit.unit_id)?;
        if let Some(latest) = mine.last() {
            unit.text = latest.text.clone();
            unit.role = latest.role;
            unit.corrected = true;
        }
        corrections.extend(mine);
        units.push(unit);
    }
    corrections.sort_by_key(|c| c.correction_id);
    Ok((units, corrections))
}

/// Speaker field of a unit as in the segmentation: INT, RESP, or the source label when unknown.
fn speaker_of(unit: &Unit) -> &str {
    match unit.role {
        cs_model::SpeakerRole::Unknown => unit.speaker_raw.as_str(),
        role => role.short(),
    }
}

/// `unit_id,speaker,timestamp,text,corrected`: the current text of each unit.
fn units_current_csv(units: &[Unit]) -> String {
    let mut out = String::from("unit_id,speaker,timestamp,text,corrected\n");
    for u in units {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(&u.unit_id),
            csv_field(speaker_of(u)),
            csv_field(&u.timestamp),
            csv_field(&u.text),
            u8::from(u.corrected)
        ));
    }
    out
}

/// Serialized name of a speaker role, as stored.
fn role_text(role: &cs_model::SpeakerRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn unit_corrections_csv(corrections: &[UnitCorrection]) -> String {
    let mut out = String::from("unit_id,correction_id,text,role,reason,created_at\n");
    for c in corrections {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_field(&c.unit_id),
            c.correction_id,
            csv_field(&c.text),
            csv_field(&role_text(&c.role)),
            csv_field(&c.reason),
            csv_field(&c.created_at)
        ));
    }
    out
}

/// One row per unit: its verification state, the reasons it is a target, its flags and corrections.
fn units_verification_csv(
    statuses: &[VerificationStatus],
    targets: &[VerificationTarget],
    flags: &[UnitFlags],
    units: &[Unit],
) -> String {
    let reasons: HashMap<&str, String> = targets
        .iter()
        .map(|t| (t.unit_id.as_str(), t.reasons.join(";")))
        .collect();
    let flag_of: HashMap<&str, &UnitFlags> =
        flags.iter().map(|f| (f.unit_id.as_str(), f)).collect();
    let corrected: BTreeSet<&str> = units
        .iter()
        .filter(|u| u.corrected)
        .map(|u| u.unit_id.as_str())
        .collect();
    let mut out = String::from("unit_id,state,stale,reasons,flags,co_constructed_kind,corrected\n");
    for s in statuses {
        let uid = s.unit_id.as_str();
        let (flag_list, kind) = match flag_of.get(uid) {
            Some(f) => (
                f.flags.join(";"),
                f.co_constructed_kind.clone().unwrap_or_default(),
            ),
            None => (String::new(), String::new()),
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(uid),
            csv_field(&s.state),
            u8::from(s.stale),
            csv_field(reasons.get(uid).map_or("", String::as_str)),
            csv_field(&flag_list),
            csv_field(&kind),
            u8::from(corrected.contains(uid))
        ));
    }
    out
}

/// One row per claim, with its codes joined by `;`.
fn claims_csv(claims: &[Claim]) -> String {
    let mut out = String::from("claim_id,unit_id,excerpt,codes,ev_level,provenance,role,memo\n");
    for c in claims {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            c.claim_id,
            csv_field(&c.unit_id),
            csv_field(&c.excerpt),
            csv_field(&codes_list(&c.codes)),
            csv_field(opt_text(&c.ev_level)),
            csv_field(opt_text(&c.provenance)),
            csv_field(opt_text(&c.role)),
            csv_field(opt_text(&c.memo))
        ));
    }
    out
}

/// Distinct claims per value of one field; a claim without the field counts as `(none)`.
fn distribution_csv(claims: &[Claim], field: fn(&Claim) -> Option<&str>) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for c in claims {
        *counts.entry(field(c).unwrap_or("(none)")).or_default() += 1;
    }
    let mut out = String::from("value,n_claims\n");
    for (value, n) in counts {
        out.push_str(&format!("{},{}\n", csv_field(value), n));
    }
    out
}

/// Recursively sorted copy of a JSON value, so that its serialization is canonical.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> =
                map.iter().map(|(k, v)| (k, canonical(v))).collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k.clone(), v)).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// The human state of the given interviews: both phases of human coding, claims,
/// cases, case units, decisions, flags, verifications and codebook, canonical.
fn state_parts(store: &Store, interview_ids: &[String], cases: &[Case]) -> Result<Value> {
    let mut phase1 = BTreeMap::new();
    let mut phase2 = BTreeMap::new();
    let mut decisions: BTreeMap<&str, Vec<Arbitration>> = BTreeMap::new();
    let mut verifications: BTreeMap<&str, Vec<VerificationStatus>> = BTreeMap::new();
    let mut claims: Vec<Claim> = Vec::new();
    let mut flags: Vec<UnitFlags> = Vec::new();
    for id in interview_ids {
        phase1.insert(id.as_str(), store.human_codings(id, PHASE_1)?);
        phase2.insert(id.as_str(), store.human_codings(id, PHASE_2)?);
        decisions.insert(id.as_str(), store.decision_history(id)?);
        verifications.insert(id.as_str(), store.verification_status(id)?);
        claims.extend(store.claims(id)?);
        flags.extend(store.unit_flags(id)?);
    }
    let mut case_units: Vec<Value> = Vec::new();
    for c in cases {
        for unit in &c.units {
            let interview = unit.rsplit_once("-U").map_or("", |(i, _)| i);
            case_units.push(json!({
                "case_id": c.case_id,
                "unit_id": unit,
                "interview_id": interview,
            }));
        }
    }
    let mut parts = json!({
        "human_phase1": phase1,
        "human_phase2": phase2,
        "claims": claims,
        "cases": cases,
        "case_units": case_units,
        "decisions": decisions,
        "flags": flags,
        "verifications": verifications,
        "codebook": store.codebook()?,
    });
    if let Some(map) = parts.as_object_mut() {
        for (k, v) in crate::export_v1::state_parts(store, interview_ids)? {
            map.insert(k.to_string(), v);
        }
    }
    Ok(canonical(&parts))
}

/// `state.json` bytes and the SHA-256 of the canonical state they carry.
fn state_document(
    store: &Store,
    interview_ids: &[String],
    cases: &[Case],
) -> Result<(Vec<u8>, String)> {
    let parts = state_parts(store, interview_ids, cases)?;
    let sha = sha256_hex(serde_json::to_string(&parts)?.as_bytes());
    let doc = json!({
        "sha256": sha,
        "sha256_of": "canonical JSON of `parts` (sorted keys, no whitespace)",
        "generator": { "version": crate::VERSION, "git": GIT_DESCRIBE },
        "parts": parts,
    });
    Ok((serde_json::to_string_pretty(&doc)?.into_bytes(), sha))
}

/// Copies the whole database to `path` with `VACUUM INTO`. The path is quoted for SQL.
fn write_snapshot(store: &Store, path: &Path) -> Result<()> {
    let quoted = path.display().to_string().replace('\'', "''");
    store
        .connection()
        .execute_batch(&format!("VACUUM INTO '{quoted}';"))
        .with_context(|| format!("writing snapshot {}", path.display()))?;
    Ok(())
}

/// `unit_id,codes,...` of the saturation table, lists joined with `;`.
fn saturation_csv(rows: &[SaturationRow]) -> String {
    let mut out = String::from(
        "interview_id,order,first_coded_at,codes_first_used,field_codes_created,labels_opened,labels_resolved\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&r.interview_id),
            r.order,
            csv_field(&r.first_coded_at),
            csv_field(&r.codes_first_used.join(";")),
            csv_field(&r.field_codes_created.join(";")),
            r.labels_opened,
            r.labels_resolved
        ));
    }
    out
}

fn md_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn saturation_md(rows: &[SaturationRow]) -> String {
    let mut s = String::from("# Saturation table, provisional\n\n");
    s.push_str(
        "This table is provisional. It orders the interviews by their first phase 2 coding and counts, for each one, the codes first used and the field codes created, with the labels opened and resolved. The enrichment of meaning is not computed: a code first used in an interview is not evidence that the interview adds meaning, and this export makes no saturation judgement. A row with an empty interview holds field codes whose interview cannot be placed.\n\n",
    );
    if rows.is_empty() {
        s.push_str("No interview coded in phase 2 yet.\n");
        return s;
    }
    s.push_str("| Order | Interview | First coded | Codes first used | Field codes created | Labels opened | Labels resolved |\n|---|---|---|---|---|---|---|\n");
    for r in rows {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            r.order,
            md_cell(&r.interview_id),
            md_cell(&r.first_coded_at),
            md_cell(&r.codes_first_used.join(", ")),
            md_cell(&r.field_codes_created.join(", ")),
            r.labels_opened,
            r.labels_resolved
        ));
    }
    s
}

/// One section per research question: the text, the four memo parts, the units for and against.
fn answers_md(questions: &[cs_model::ResearchQuestion], memos: &[Option<RqMemo>]) -> String {
    let mut s = String::from("# Answers to the research questions\n\n");
    s.push_str(
        "Written across interviews. Each question gives its text, then the four memo parts (documented, interpretation, rivals, limits), then the units for and against the answer. \"no memo yet\" marks a question without a memo.\n\n",
    );
    if questions.is_empty() {
        s.push_str("No research question recorded.\n");
        return s;
    }
    for (q, memo) in questions.iter().zip(memos) {
        s.push_str(&format!("## {}\n\n{}\n\n", q.id, q.text));
        let Some(m) = memo else {
            s.push_str("no memo yet\n\n");
            continue;
        };
        for (title, text) in [
            ("Documented", &m.documented),
            ("Interpretation", &m.interpretation),
            ("Rivals", &m.rivals),
            ("Limits", &m.limits),
        ] {
            let body = if text.trim().is_empty() {
                "(not filled)"
            } else {
                text.as_str()
            };
            s.push_str(&format!("### {title}\n\n{body}\n\n"));
        }
        for (title, units) in [
            ("Units for", &m.units_for),
            ("Units against", &m.units_against),
        ] {
            s.push_str(&format!("**{title}**\n\n"));
            if units.is_empty() {
                s.push_str("None.\n\n");
            } else {
                for u in units {
                    s.push_str(&format!("- `{u}`\n"));
                }
                s.push('\n');
            }
        }
    }
    s
}

/// One row per research question and case; one row per code of the cell.
fn cross_case_csv(cells: &[cs_model::CrossCaseCell]) -> String {
    let mut out = String::from("rq,case_id,documented,code,units\n");
    for cell in cells {
        let documented = u8::from(cell.documented);
        if cell.codes.is_empty() {
            out.push_str(&format!(
                "{},{},{},,\n",
                csv_field(&cell.rq),
                csv_field(&cell.case_id),
                documented
            ));
            continue;
        }
        for code in &cell.codes {
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                csv_field(&cell.rq),
                csv_field(&cell.case_id),
                documented,
                csv_field(&code.code),
                csv_field(&code.units.join(";"))
            ));
        }
    }
    out
}

/// Picks `<base>/<YYYY-MM-DD>_<name>`, or a time-suffixed sibling when it exists.
pub(crate) fn target_dir(base: &Path, name: &str) -> PathBuf {
    let now = chrono::Local::now();
    let first = base.join(format!("{}_{}", now.format("%Y-%m-%d"), name));
    if !first.exists() {
        return first;
    }
    let mut n = 1;
    loop {
        let candidate = base.join(format!(
            "{}_{}_{}{}",
            now.format("%Y-%m-%d"),
            name,
            now.format("%H%M%S"),
            if n == 1 {
                String::new()
            } else {
                format!("_{n}")
            }
        ));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

pub fn write_package(
    store: &Store,
    interview_id: &str,
    base: &Path,
    db_path: &Path,
    actor: &str,
) -> Result<ExportResult> {
    let entry = store
        .interview(interview_id)?
        .ok_or_else(|| CsError::NotFound(format!("interview {interview_id}")))?;
    let seg = store.segmentation(interview_id)?.ok_or_else(|| {
        CsError::Validation(format!(
            "{interview_id} has no segmentation; nothing to export yet"
        ))
    })?;
    let all_units: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let p1 = store.human_codings(interview_id, PHASE_1)?;
    let p2 = store.human_codings(interview_id, PHASE_2)?;
    let scope2 = store.phase_units(interview_id, PHASE_2)?;
    let runs = store.ai_runs(interview_id)?;
    let prompts = store.prompts(interview_id)?;
    let review = store.review(interview_id)?;
    let history = store.decision_history(interview_id)?;
    let (finals, _uncoded) = store.final_codings(interview_id)?;
    let to_clarify = store.to_clarify_items(Some(interview_id), false)?;
    let cases = store.cases(Some(interview_id))?;
    let human_snaps = store.human_snapshots(interview_id)?;
    let (codebook_csv, _) = store.codebook_export()?;
    // Read before the audit events: the first draw of the verification sample is audited.
    let (units_now, corrections) = current_units(store, interview_id, &seg)?;
    let claims = store.claims(interview_id)?;
    let flags = store.unit_flags(interview_id)?;
    let statuses = store.verification_status(interview_id)?;
    let targets = store.verification_targets(interview_id)?;
    let coverage = store.verification_coverage(interview_id)?;
    let events = store.audit_events(Some(interview_id))?;
    let chain = store.verify_chain()?;

    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, interview_id);
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut w = Writer {
        dir: dir.clone(),
        files: Vec::new(),
    };

    w.write("units.csv", units_current_csv(&units_now).as_bytes())?;
    w.write(
        "units_verification.csv",
        units_verification_csv(&statuses, &targets, &flags, &units_now).as_bytes(),
    )?;
    w.write(
        "unit_corrections.csv",
        unit_corrections_csv(&corrections).as_bytes(),
    )?;
    w.write("claims.csv", claims_csv(&claims).as_bytes())?;
    w.write(
        "ev_distribution.csv",
        distribution_csv(&claims, |c| c.ev_level.as_deref()).as_bytes(),
    )?;
    w.write(
        "provenance_distribution.csv",
        distribution_csv(&claims, |c| c.provenance.as_deref()).as_bytes(),
    )?;
    w.write(
        "roles_distribution.csv",
        distribution_csv(&claims, |c| c.role.as_deref()).as_bytes(),
    )?;
    let p1_map = store.human_coding_map(interview_id, PHASE_1)?;
    w.write(
        "phase1_contexts.csv",
        phase1_contexts_csv(store, interview_id, &p1_map, &all_units)?.as_bytes(),
    )?;
    w.write(
        "phase2_coding_human.csv",
        human_csv(&all_units, &p2).as_bytes(),
    )?;
    w.write(
        "phase2_human_full.csv",
        human_full_csv(&all_units, &p2).as_bytes(),
    )?;

    for p in &prompts {
        let name = format!(
            "phase2_batch{}_prompt_part{}_of{}_{}.txt",
            p.batch_id,
            p.part,
            p.of,
            &p.sha256[..8]
        );
        w.write(&name, p.text.as_bytes())?;
    }

    for run in &runs {
        let tag = format!("run{}_{}", run.run_id, safe(&run.meta.run_label));
        let meta = json!({
            "run_id": run.run_id,
            "batch_id": run.batch_id,
            "meta": run.meta,
            "unit_ids": run.unit_ids,
            "parts_received": run.parts_received,
            "parts_expected": run.parts_expected,
            "complete": run.complete,
        });
        w.write(
            &format!("phase2_{tag}.json"),
            serde_json::to_string_pretty(&meta)?.as_bytes(),
        )?;
        w.write(
            &format!("phase2_coding_ai_{tag}.csv"),
            cs_ai::coding_csv(&run.codings, &run.unit_ids).as_bytes(),
        )?;
        for (part, raw, _sha, _at) in store.ai_raw_outputs(run.run_id)? {
            w.write(
                &format!("phase2_ai_{tag}_part{part}_raw.txt"),
                raw.as_bytes(),
            )?;
        }
    }

    let (snapshots, complete) = crate::candidate_runs(store, interview_id)?;
    if !complete.is_empty() {
        let order = crate::compared_units(store, interview_id)?;
        let reports = crate::agreement_reports(&snapshots, &complete, &order)?;
        w.write(
            "phase2_agreement.md",
            cs_ai::render_agreement_md(&seg, &order, &reports).as_bytes(),
        )?;
        let items: Vec<cs_model::ReviewItem> =
            review.items.iter().map(|e| e.item.clone()).collect();
        let mut review_md = cs_ai::render_review_md(&seg, &items, &history, &review.seeds);
        review_md.push_str(&revisions_md(&items));
        review_md.push_str(&snapshots_md(&review.snapshots));
        w.write("phase2_review.md", review_md.as_bytes())?;
    }
    for snap in &human_snaps {
        w.write(
            &format!("human_snapshot_batch{}.csv", snap.batch_id),
            snapshot_csv(snap).as_bytes(),
        )?;
        w.write(
            &format!("human_snapshot_batch{}.json", snap.batch_id),
            serde_json::to_string_pretty(snap)?.as_bytes(),
        )?;
    }
    w.write(
        "phase2_coding_final.csv",
        cs_ai::final_csv(&finals).as_bytes(),
    )?;
    w.write("to_clarify.csv", to_clarify_csv(&to_clarify).as_bytes())?;
    w.write(
        "cases.json",
        serde_json::to_string_pretty(&cases)?.as_bytes(),
    )?;
    w.write("cases.md", cases_md(interview_id, &cases).as_bytes())?;
    w.write("cases.csv", cases_csv(&cases).as_bytes())?;
    w.write("case_units.csv", case_units_csv(&cases).as_bytes())?;
    w.write("codebook.csv", codebook_csv.as_bytes())?;
    let ids = [interview_id.to_string()];
    let passages = crate::export_v1::all_passages(store, &ids)?;
    w.write(
        "passages.csv",
        crate::export_v1::passages_csv(store, &passages)?.as_bytes(),
    )?;
    w.write(
        "highlights.csv",
        crate::export_v1::highlights_csv(&passages).as_bytes(),
    )?;
    w.write(
        "attributes.csv",
        crate::export_v1::attributes_csv(store, &ids)?.as_bytes(),
    )?;
    w.write("memos.json", &crate::export_v1::memos_json(store, &ids)?)?;
    w.write("review.json", &crate::export_v1::review_json(store, &ids)?)?;
    w.write(
        "co-occurrence.csv",
        crate::export_v1::cooccurrence_csv(store, &ids)?.as_bytes(),
    )?;
    w.write(
        "transcript_versions.json",
        &crate::export_v1::transcript_versions_json(store, &ids)?,
    )?;
    w.write(
        "study_config.json",
        &crate::export_v1::study_config_json(store)?,
    )?;

    let (state_bytes, state_sha) = state_document(store, &[interview_id.to_string()], &cases)?;
    w.write("state.json", &state_bytes)?;
    write_snapshot(store, &dir.join("db_snapshot.sqlite"))?;
    w.register("db_snapshot.sqlite")?;

    let mut jsonl = String::new();
    for e in &events {
        jsonl.push_str(&serde_json::to_string(e)?);
        jsonl.push('\n');
    }
    w.write("audit_log.jsonl", jsonl.as_bytes())?;
    w.write(
        "audit_chain.json",
        serde_json::to_string_pretty(&chain)?.as_bytes(),
    )?;

    let status = CodingStatus {
        n_units_phase1: all_units.len(),
        n_coded_phase1: p1.len(),
        n_units_phase2: scope2.len(),
        n_coded_phase2: store.guard(interview_id, PHASE_2)?.n_units
            - store.guard(interview_id, PHASE_2)?.missing.len(),
        n_runs: runs.len(),
        n_complete_runs: complete.len(),
        n_disagreements: review.n_disagreements,
        n_sampled: review.n_sampled,
        n_decided: review.n_decided,
        n_revised_after_ai: p2.values().filter(|c| c.revised_after_ai).count(),
        n_to_clarify_open: to_clarify.iter().filter(|t| t.resolved.is_none()).count(),
        n_cases: cases.len(),
        n_snapshots: human_snaps.len(),
        n_claims: claims.len(),
        verification: coverage,
    };
    w.write(
        "README.md",
        readme(interview_id, &entry.folder_name, &seg, &status).as_bytes(),
    )?;

    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "prompt_version": store.study_config()?.prompt_template.version,
        "phase1_codebook": cs_model::phase::PHASE1_CODEBOOK_VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "interview_id": interview_id,
        "db_path": db_path,
        "segmentation_hash": seg.hash,
        "source": { "path": seg.source_rel_path, "sha256": seg.source_sha256 },
        "audit_chain_ok": chain.ok,
        "status": status,
        "files": w.files,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_sha = sha256_hex(&manifest_bytes);
    w.write("manifest.json", &manifest_bytes)?;

    store.log(
        actor,
        "export.package",
        Some(interview_id),
        Some(&dir.display().to_string()),
        json!({
            "dir": dir.display().to_string(),
            "manifest_sha256": manifest_sha,
            "state_sha256": state_sha,
            "n_files": w.files.len(),
            "status": status,
        }),
    )?;

    Ok(ExportResult {
        dir: dir.display().to_string(),
        files: w.files,
        manifest_sha256: manifest_sha,
        status,
    })
}

fn readme(id: &str, folder: &str, seg: &cs_model::Segmentation, st: &CodingStatus) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Coding audit package, {id}\n\n"));
    s.push_str(&format!(
        "Interview folder: {folder}. Segmented source: {} (SHA-256 {}). Segmentation hash: {}. Rule: {}. Units: {}.\n\n",
        seg.source_rel_path, seg.source_sha256, seg.hash, seg.rule, seg.units.len()
    ));
    s.push_str("Phase 1 is the candidate's provisional pre-classification of every unit against the three contexts of the TOE framework (TECH, ORG, ENV), plus CONTEXT for descriptive material and NA for the rest; it stays revisable and is not challenged by the AI. Phase 2 codes the units whose phase 1 category is not NA with concrete codes, each attached to a TOE context where relevant and marked as coming from the literature or from the field. A unit may carry several codes, or a provisional label to clarify that is later resolved into a new code or mapped to an existing one. An AI coder then challenges the detailed codes blind, from a generated prompt; the candidate reviews every disagreement and a random sample of agreements and keeps the final decision.\n\n");
    s.push_str(&format!(
        "Status at export: phase 1, {} of {} units coded; phase 2, {} of {} units coded, {} AI run(s) of which {} complete, {} disagreement(s) and {} sampled agreement(s), {} decision(s); {} coding(s) revised after the AI answer; {} open label(s) to clarify; {} case(s); {} human snapshot(s), one per prompt batch; {} claim(s). Verification coverage, reported apart from the counts above: {} of {} verification target(s) verified or marked unusable, {} unit(s) verified, {} unusable, {} stale, out of {} unit(s).\n\n",
        st.n_coded_phase1, st.n_units_phase1, st.n_coded_phase2, st.n_units_phase2, st.n_runs,
        st.n_complete_runs, st.n_disagreements, st.n_sampled, st.n_decided, st.n_revised_after_ai,
        st.n_to_clarify_open, st.n_cases, st.n_snapshots, st.n_claims,
        st.verification.n_targets_verified, st.verification.n_targets,
        st.verification.n_verified, st.verification.n_unusable, st.verification.n_stale,
        st.verification.n_units
    ));
    s.push_str(
        "Counting unit. The counting unit is the claim. Each distribution (`ev_distribution.csv`, `provenance_distribution.csv`, `roles_distribution.csv`) counts distinct claims once, by the value of one field, with `(none)` for a claim without that field. The counts describe the eligible coded corpus of this interview (its phase 2 coded units and the claims made on them). Verification coverage is reported separately, in `units_verification.csv` and in the status above, and is not mixed into these counts.\n\n",
    );
    s.push_str(
        "Immutable state. `state.json` holds the human state behind this package: both phases of the candidate's coding, claims, cases, case units, decisions, flags, verifications and codebook. It is canonical JSON (sorted keys, no whitespace); its `sha256` field is the SHA-256 of that canonical JSON, and `generator` names the tool version and the source description it was built from. `db_snapshot.sqlite` is a consistent copy of the whole database, taken with `VACUUM INTO` at export time: it is the immutable state behind this package, from which every figure here can be recomputed. Both files are listed in `manifest.json`.\n\n",
    );
    s.push_str("## Files\n\n");
    for (name, what) in [
        ("units.csv", "The units of analysis with their current text and role, `unit_id,speaker,timestamp,text,corrected`: the segmentation as produced by the deterministic rule, overlaid by the corrections; `corrected` is 1 when a correction exists."),
        ("units_verification.csv", "Verification of each unit against the audio: `unit_id,state,stale,reasons,flags,co_constructed_kind,corrected`. `state` is verified, unusable or unverified; `stale` is 1 when the verification was made on a text or role that has since changed; `reasons` says why the unit must be verified (quoted, flagged, disagreement, case, sample)."),
        ("unit_corrections.csv", "Every correction of a unit's text or role, oldest first, with its reason: `unit_id,correction_id,text,role,reason,created_at`. The segmentation itself is never rewritten."),
        ("claims.csv", "The claims of this interview: one qualified statement per row, with its excerpt, codes, evidence level, provenance, role and memo."),
        ("ev_distribution.csv, provenance_distribution.csv, roles_distribution.csv", "Distinct claims per value of evidence level, provenance and role, `value,n_claims`."),
        ("state.json", "The human state behind the package, canonical, with its SHA-256 and the generator (see Immutable state)."),
        ("db_snapshot.sqlite", "A consistent copy of the whole database at export time, taken with VACUUM INTO (see Immutable state)."),
        ("phase1_contexts.csv", "The provisional TOE pre-classification, `unit_id,codes,derived_contexts`: `derived_contexts` lists the TOE contexts (TECH, ORG, ENV) attached to the unit's phase 2 codes, `;`-separated in that order, or CONTEXT when no code is attached; empty for interviewer turns and NA units."),
        ("phase2_coding_human.csv", "The candidate's detailed coding, `unit_id,codes,revised_after_ai`; the last column is 1 when the coding was changed after an AI answer covering the unit."),
        ("phase2_human_full.csv", "The same coding with every field: sub-code, modifier, evidence level, provenance, memo and verbatim excerpt."),
        ("phase2_batch<b>_prompt_part<n>_of<m>_<hash>.txt", "Each prompt generated for the AI coder, byte for byte, by batch; the hash prefix is the start of its SHA-256."),
        ("phase2_run<id>_<label>.json", "Parameters of each AI run: batch answered, model, version, temperature, seed, context length, prompt version, execution time."),
        ("phase2_coding_ai_run<id>_<label>.csv", "Codes per unit returned by that run, after validation."),
        ("phase2_ai_run<id>_<label>_part<n>_raw.txt", "The AI answer exactly as pasted, before any parsing."),
        ("phase2_agreement.md", "Exact agreement and Cohen's kappa per code, candidate against each run and runs against each other; reported, not optimised."),
        ("phase2_review.md", "Every disagreement and every sampled agreement with the unit text, the competing codes, the decision and its rationale, and the sample seeds; then the units revised since their snapshot and the snapshot references."),
        ("phase2_coding_final.csv", "Final detailed coding: the review decision where one exists, otherwise the candidate's codes (`basis`)."),
        ("to_clarify.csv", "Every provisional label, its note, and how it was resolved (new code or mapped code)."),
        ("human_snapshot_batch<id>.csv", "The candidate's phase 2 coding frozen when prompt batch <id> was generated, `unit_id,codes`: the reference the AI answers of that batch were compared with."),
        ("human_snapshot_batch<id>.json", "The same snapshot with its SHA-256, the segmentation and codebook hashes, and its creation time."),
        ("cases.json, cases.md", "The case syntheses: decision, granularity, carriers chosen and rejected, account type, Swiss link, stage and outcome status; for trigger, alternatives, constraints, choice and result, the text, the contradictions, what is missing and the evidence units; and the units attributed to the case, from any interview."),
        ("cases.csv", "One row per case: identifiers, decision, granularity, carriers, account type, Swiss link, stage, outcome status and number of attributed units; lists joined with `;`."),
        ("case_units.csv", "The units attributed to each case, `case_id,unit_id,interview_id`."),
        ("codebook.csv", "The codebook at export time: TOE attachment, origin, research questions, version, then the code tree: `code_id` (stable), `name` (editable), `parent_id`, `aliases` joined with `;`, `includes`, `excludes`."),
        ("passages.csv", "Every passage (a selected part of a turn), highlights included: `passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale,anchor_digest,created_at,created_by,transcript_version`. `transcript_version` is the version of the transcript the offsets were checked against (the current one, or the one a passage waiting for review is still anchored on). Positions count Unicode characters from 0, end excluded. `later` is 1 for the reserved code Plus tard, which is not in `code_ids` and counts in no analysis. `stale` is 1 when the turn text no longer matches `anchor_digest`."),
        ("highlights.csv", "The passages that carry no code yet, `passage_id,interview_id,unit_id,start,end,text,stale,created_at,created_by`."),
        ("attributes.csv", "The interview attribute values, `interview_id,attribute_id,label,kind,value`, one row per attribute that has a value."),
        ("memos.json", "The familiarisation memo of each interview (template and version, the interview's own added, removed and reworded questions, answers with their time) and its end-of-interview checklist (items, checked flags with time, note)."),
        ("review.json", "Review (lot 4): the candidate concepts the model proposed with their decisions (create, map or reject, with reason and time), the digest, the categories and the number of entries of the pseudonymisation list (never the names), the harmonisation worklist (merge, redefinition, new sub-code) and the review state of each interview. Candidate texts are kept as the model saw them, pseudonymised."),
        ("co-occurrence.csv", "Code co-occurrence (F-56), `relation,group,kind,code_a_id,code_a_name,code_b_id,code_b_name,passages,units,interviews,passage_pairs,jaccard`, for the three relations `same_passage`, `overlap` and `same_unit`, over the interviews of the package. `kind` is `single` for a code alone (`passages`, `units` and `interviews` of the code) and `pair` for two codes (turns, interviews and passage pairs, counted apart; `jaccard` is the turns with both codes over the turns with either). Codes are listed in codebook order and a pair once. It describes how the coding falls together, not causation or prevalence; the reserved code Plus tard is left out."),
        ("transcript_versions.json", "Transcript versions (F-24): for each interview, its versions (cause, text digest) and every projection of a passage onto a new version, with the old, proposed and final span, how it was settled (re-anchored, accepted, adjusted, detached, superseded, or still to review) and by whom."),
        ("review.json", "Review (lot 4): the candidate concepts the model proposed with their decisions (create, map or reject, with reason and time), the digest, the categories and the number of entries of the pseudonymisation list (never the names), the harmonisation worklist (merge, redefinition, new sub-code) and the review state of each interview. Candidate texts are kept as the model saw them, pseudonymised."),
        ("transcript_versions.json", "Transcript versions (F-24): for each interview, its versions (cause, text digest) and every projection of a passage onto a new version, with the old, proposed and final span, how it was settled (re-anchored, accepted, adjusted, detached, superseded, or still to review) and by whom."),
        ("study_config.json", "The study configuration as a project template: study description and settings, research questions, attribute definitions, memo templates, checklist, theme checks, evidence natures, link types, matrix dimensions and interpretive framework. It holds no interview data."),
        ("audit_log.jsonl", "Every recorded action on this interview, one JSON event per line, each chained to the previous one by SHA-256."),
        ("audit_chain.json", "Result of the verification of the whole audit chain at export time."),
        ("manifest.json", "SHA-256 and size of every file above, with the tool version and the export time."),
    ] {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the package, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}

/// Result of the cross-case export: its folder and the files written there.
#[derive(Debug, Serialize)]
pub struct CrossCaseExport {
    pub folder: String,
    pub files: Vec<ExportedFile>,
}

fn cross_case_readme(
    n_interviews: usize,
    questions: &[cs_model::ResearchQuestion],
    n_memos: usize,
    n_cases: usize,
    saturation_rows: usize,
    state_sha: &str,
) -> String {
    let mut s = String::from("# Cross-case export\n\n");
    s.push_str(&format!(
        "Across {n_interviews} interview(s): {} research question(s), of which {n_memos} with a memo; {n_cases} case(s); a saturation table of {saturation_rows} row(s), provisional. State SHA-256 {state_sha}.\n\n",
        questions.len()
    ));
    s.push_str("The answers are written by research question: the question, the four memo parts (documented, interpretation, rivals, limits), and the units for and against. The cross-case matrix lists, for each question and case, the codes found in the units attributed to that case. The saturation table orders the interviews by their first phase 2 coding and is provisional: the enrichment of meaning is not computed.\n\n");
    s.push_str("Immutable state. `state.json` holds the human state of every interview (both phases of coding, claims, cases, case units, decisions, flags, verifications, codebook, passages, code tree, attribute values, memos, checklists and project configuration) as canonical JSON. Its `sha256` field is the SHA-256 of that canonical JSON. Each interview package carries its own `db_snapshot.sqlite`.\n\n");
    s.push_str("## Files\n\n");
    for (name, what) in [
        ("answers.md", "One section per research question: the text, the four memo parts, the units for and against the answer; `no memo yet` when absent."),
        ("cross_case.csv", "The cross-case matrix, `rq,case_id,documented,code,units`: one row per code found in a case for a question; `documented` is 1 when the case has attributed units."),
        ("saturation.csv, saturation.md", "The provisional saturation table: codes first used, field codes created and labels opened and resolved, per interview in order of first coding."),
        ("cases.csv", "One row per case, as in the interview packages."),
        ("codebook.csv", "The codebook with its code tree, as in the interview packages."),
        ("passages.csv", "Every passage of every interview (a selected part of a turn), highlights included: `passage_id,interview_id,unit_id,start,end,text,kind,code_ids,code_names,later,later_note,stale,anchor_digest,created_at,created_by,transcript_version`. `transcript_version` is the version of the transcript the offsets were checked against (the current one, or the one a passage waiting for review is still anchored on). Positions count Unicode characters from 0, end excluded. `later` is 1 for the reserved code Plus tard, which is not in `code_ids` and counts in no analysis. `stale` is 1 when the turn text no longer matches `anchor_digest`."),
        ("highlights.csv", "The passages that carry no code yet, `passage_id,interview_id,unit_id,start,end,text,stale,created_at,created_by`."),
        ("attributes.csv", "The interview attribute values of every interview, `interview_id,attribute_id,label,kind,value`, one row per attribute that has a value."),
        ("memos.json", "The familiarisation memo of every interview (template and version, the interview's own added, removed and reworded questions, answers with their time) and its end-of-interview checklist (items, checked flags with time, note)."),
        ("study_config.json", "The study configuration as a project template: study description and settings, research questions, attribute definitions, memo templates, checklist, theme checks, evidence natures, link types, matrix dimensions and interpretive framework. It holds no interview data."),
        ("co-occurrence.csv", "Code co-occurrence (F-56), `relation,group,kind,code_a_id,code_a_name,code_b_id,code_b_name,passages,units,interviews,passage_pairs,jaccard`, for the three relations `same_passage`, `overlap` and `same_unit`, over the interviews of the package. `kind` is `single` for a code alone (`passages`, `units` and `interviews` of the code) and `pair` for two codes (turns, interviews and passage pairs, counted apart; `jaccard` is the turns with both codes over the turns with either). Codes are listed in codebook order and a pair once. It describes how the coding falls together, not causation or prevalence; the reserved code Plus tard is left out."),
        ("state.json", "The human state of every interview, canonical, with its SHA-256 and the generator."),
        ("manifest.json", "SHA-256 and size of every file above, with the tool version and the export time."),
    ]
    .into_iter()
    .chain(crate::export_analysis::README_LINES)
    {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the export, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}

/// Writes the cross-case package to `<base>/<YYYY-MM-DD>_cross-case/` (time suffix when it exists).
pub fn write_cross_case(
    store: &Store,
    base: &Path,
    db_path: &Path,
    actor: &str,
) -> Result<CrossCaseExport> {
    let cross = store.cross_case()?;
    let saturation = store.saturation()?;
    let memos: Vec<Option<RqMemo>> = cross
        .questions
        .iter()
        .map(|q| store.rq_memo(&q.id))
        .collect::<Result<_>>()?;
    let interview_ids: Vec<String> = store
        .summaries()?
        .into_iter()
        .map(|s| s.interview_id)
        .collect();
    let (state_bytes, state_sha) = state_document(store, &interview_ids, &cross.cases)?;
    let n_memos = memos.iter().filter(|m| m.is_some()).count();

    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, "cross-case");
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut w = Writer {
        dir: dir.clone(),
        files: Vec::new(),
    };

    w.write(
        "answers.md",
        answers_md(&cross.questions, &memos).as_bytes(),
    )?;
    w.write("cross_case.csv", cross_case_csv(&cross.cells).as_bytes())?;
    w.write("saturation.csv", saturation_csv(&saturation).as_bytes())?;
    w.write("saturation.md", saturation_md(&saturation).as_bytes())?;
    w.write("cases.csv", cases_csv(&cross.cases).as_bytes())?;
    let (codebook_csv, _) = store.codebook_export()?;
    w.write("codebook.csv", codebook_csv.as_bytes())?;
    let passages = crate::export_v1::all_passages(store, &interview_ids)?;
    w.write(
        "passages.csv",
        crate::export_v1::passages_csv(store, &passages)?.as_bytes(),
    )?;
    w.write(
        "highlights.csv",
        crate::export_v1::highlights_csv(&passages).as_bytes(),
    )?;
    w.write(
        "attributes.csv",
        crate::export_v1::attributes_csv(store, &interview_ids)?.as_bytes(),
    )?;
    w.write(
        "memos.json",
        &crate::export_v1::memos_json(store, &interview_ids)?,
    )?;
    w.write(
        "review.json",
        &crate::export_v1::review_json(store, &interview_ids)?,
    )?;
    w.write(
        "co-occurrence.csv",
        crate::export_v1::cooccurrence_csv(store, &interview_ids)?.as_bytes(),
    )?;
    w.write(
        "transcript_versions.json",
        &crate::export_v1::transcript_versions_json(store, &interview_ids)?,
    )?;
    w.write(
        "study_config.json",
        &crate::export_v1::study_config_json(store)?,
    )?;
    crate::export_analysis::write_files(&mut w, store)?;
    w.write("state.json", &state_bytes)?;
    w.write(
        "README.md",
        cross_case_readme(
            interview_ids.len(),
            &cross.questions,
            n_memos,
            cross.cases.len(),
            saturation.len(),
            &state_sha,
        )
        .as_bytes(),
    )?;

    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "db_path": db_path,
        "state_sha256": state_sha,
        "counts": {
            "interviews": interview_ids.len(),
            "research_questions": cross.questions.len(),
            "memos": n_memos,
            "cases": cross.cases.len(),
            "saturation_rows": saturation.len(),
        },
        "files": w.files,
    });
    w.write("manifest.json", &serde_json::to_vec_pretty(&manifest)?)?;

    store.log(
        actor,
        "export.cross_case",
        None,
        Some(&dir.display().to_string()),
        json!({
            "dir": dir.display().to_string(),
            "state_sha256": state_sha,
            "n_files": w.files.len(),
        }),
    )?;

    Ok(CrossCaseExport {
        folder: dir.display().to_string(),
        files: w.files,
    })
}

/// Result of the write-up export.
#[derive(Debug, Serialize)]
pub struct WriteupExport {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub markdown: String,
}

fn writeup_readme(state_sha: &str) -> String {
    let mut s = String::from("# Write-up export\n\n");
    s.push_str(&format!(
        "The themes, the matrix, the typed links, the write-up, the final audit and the documented decisions of the study. State SHA-256 {state_sha}.\n\n"
    ));
    s.push_str("Every number is computed from the codings at export time. Nothing here was proposed by the tool: themes, cell values, texts and verdicts are the researcher's. The final audit records its seed, so the draw can be replayed.\n\n## Files\n\n");
    for (name, what) in crate::export_analysis::README_LINES.iter().chain([
        &(
            "state.json",
            "The human state of the study, canonical, with its SHA-256 and the generator.",
        ),
        &(
            "manifest.json",
            "SHA-256 and size of every file above, with the tool version and the export time.",
        ),
    ]) {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the export, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}

/// Writes the write-up package to `<base>/<YYYY-MM-DD>_writeup/` (time suffix when it exists).
pub fn write_writeup(
    store: &Store,
    base: &Path,
    db_path: &Path,
    actor: &str,
) -> Result<WriteupExport> {
    let interview_ids: Vec<String> = store
        .summaries()?
        .into_iter()
        .map(|s| s.interview_id)
        .collect();
    let cases = store.cases(None)?;
    let (state_bytes, state_sha) = state_document(store, &interview_ids, &cases)?;
    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, "writeup");
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut w = Writer {
        dir: dir.clone(),
        files: Vec::new(),
    };
    crate::export_analysis::write_files(&mut w, store)?;
    w.write("state.json", &state_bytes)?;
    w.write("README.md", writeup_readme(&state_sha).as_bytes())?;
    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "db_path": db_path,
        "state_sha256": state_sha,
        "counts": {
            "themes": store.themes(None)?.len(),
            "links": store.links()?.len(),
            "decisions": cases.len(),
        },
        "files": w.files,
    });
    w.write("manifest.json", &serde_json::to_vec_pretty(&manifest)?)?;
    store.log(
        actor,
        "export.writeup",
        None,
        Some(&dir.display().to_string()),
        json!({"dir": dir.display().to_string(), "state_sha256": state_sha, "n_files": w.files.len()}),
    )?;
    Ok(WriteupExport {
        dir: dir.display().to_string(),
        files: w.files,
        markdown: store.writeup_markdown()?,
    })
}
