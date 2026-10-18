//! Local, reversible copies only. No detector or anonymisation call leaves this process.
//! Human review is content-bound, not a claim of automatic complete anonymity.
use crate::{ApiError, ApiResult, AppState};
use anyhow::{ensure, Result};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use cs_model::{
    pseudonym::{PseudonymEntry, Pseudonymiser},
    sha256_hex, PromptPart,
};
use cs_store::{PersonFilter, Store};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const KEY: &str = "anonymisation_v1";
const POLICY: &str = "local-anonymisation-1";
#[derive(Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    entries: Vec<PseudonymEntry>,
    #[serde(default)]
    reviewed: BTreeMap<String, String>,
    #[serde(default)]
    batches: BTreeMap<String, String>,
}
fn config(store: &Store) -> Result<Config> {
    Ok(serde_json::from_str(
        &store.setting(KEY)?.unwrap_or_else(|| "{}".into()),
    )?)
}
fn save(store: &Store, cfg: &Config, actor: &str) -> Result<()> {
    // The store audits only digests for this dedicated private setting.
    store.set_setting(KEY, &serde_json::to_string(cfg)?, actor)
}
pub(crate) fn entries(store: &Store) -> Result<Vec<PseudonymEntry>> {
    let mut entries = store.pseudonym_config()?.entries;
    // Privacy boundaries override the legacy opt-out for personal names.
    for row in store.persons(&PersonFilter::default())? {
        let p = row.person;
        if p.kind == "person" || p.anonymise {
            let label = if p.pseudonym.trim().is_empty() {
                format!("[Person {}]", p.person_id)
            } else {
                p.pseudonym
            };
            for name in std::iter::once(p.name).chain(p.aliases) {
                entries.push(PseudonymEntry {
                    kind: p.kind.clone(),
                    original: name,
                    pseudonym: label.clone(),
                });
            }
        }
        for (value, label) in [(p.email, "[Email]"), (p.phone, "[Phone]")] {
            if !value.trim().is_empty() {
                entries.push(PseudonymEntry {
                    kind: "contact".into(),
                    original: value,
                    pseudonym: label.into(),
                });
            }
        }
    }
    for name in store.study_config()?.interviewers {
        // Generic role labels carry no identity and must retain their structure.
        if !generic_speaker(&name) {
            entries.push(PseudonymEntry {
                kind: "person".into(),
                original: name,
                pseudonym: "[Researcher]".into(),
            });
        }
    }
    // Explicit local rules win, including a correction to a proposed identity.
    entries.extend(config(store)?.entries);
    let mut map = BTreeMap::new();
    for mut e in entries {
        e.original = e.original.trim().into();
        if !e.original.is_empty() {
            map.insert(e.original.to_lowercase(), e);
        }
    }
    let out: Vec<_> = map.into_values().collect();
    let rules = Pseudonymiser::new(&out);
    ensure!(
        out.iter().all(|e| !rules.contains_original(&e.pseudonym)),
        "Un pseudonyme contient une identité de la table locale"
    );
    Ok(out)
}
pub(crate) fn generic_speaker(name: &str) -> bool {
    let s = name.trim().to_lowercase();
    s.is_empty()
        || [
            "int",
            "resp",
            "interviewer",
            "respondent",
            "intervieweur",
            "intervieweuse",
            "répondant",
            "speaker",
            "locuteur",
            "unknown",
            "candidate",
            "researcher",
            "chercheur",
            "intervenant",
        ]
        .iter()
        .any(|r| {
            s == *r
                || s.strip_prefix(r).is_some_and(|tail| {
                    tail.chars()
                        .all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '_' || c == '-')
                })
        })
}
pub(crate) fn rules(store: &Store) -> Result<Pseudonymiser> {
    Ok(Pseudonymiser::new(&entries(store)?))
}
// Direct contacts get an independent technical privacy pass. Timestamp and business
// figures are deliberately not a broad number regex. Other identifying details need review.
fn contact_regexes() -> &'static [(Regex, &'static str)] {
    static R: std::sync::OnceLock<Vec<(Regex, &'static str)>> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        vec![
            (
                Regex::new(
                    r"(?i)[\p{L}\p{N}.!#$%&'*+/=?^_`{|}~-]+@[\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)+",
                )
                .unwrap(),
                "[Email]",
            ),
            (
                Regex::new(r"\+[1-9][0-9]{0,2}(?:[ .()-]*[0-9]){7,14}\b").unwrap(),
                "[Phone]",
            ),
            (
                Regex::new(r"(?i)(?:https?://|www\.)[^\s<>\x22]+").unwrap(),
                "[URL]",
            ),
        ]
    })
}
pub(crate) fn replacement_spans(rules: &Pseudonymiser, text: &str) -> Vec<(usize, usize, String)> {
    // Contacts take precedence over name fragments inside an address or URL.
    let mut spans = Vec::new();
    let mut contacts = Vec::new();
    for (regex, label) in contact_regexes() {
        for hit in regex.find_iter(text) {
            let matched = if *label == "[URL]" {
                hit.as_str()
                    .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}'])
            } else {
                hit.as_str()
            };
            let start = text[..hit.start()].chars().count();
            let end = start + matched.chars().count();
            contacts.push((start, end, (*label).to_string()));
        }
    }
    contacts.sort_by(|a, b| (b.1 - b.0).cmp(&(a.1 - a.0)).then(a.0.cmp(&b.0)));
    for (start, end, label) in contacts {
        if !spans.iter().any(|(a, b, _)| start < *b && end > *a) {
            spans.push((start, end, label));
        }
    }
    for (start, end, label) in rules.replacement_spans(text) {
        if !spans.iter().any(|(a, b, _)| start < *b && end > *a) {
            spans.push((start, end, label));
        }
    }
    spans.sort_by_key(|s| s.0);
    spans
}
pub(crate) fn mask_with(rules: &Pseudonymiser, text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut pos = 0;
    for (start, end, label) in replacement_spans(rules, text) {
        result.extend(&chars[pos..start]);
        result.push_str(&label);
        pos = end;
    }
    result.extend(&chars[pos..]);
    result
}
pub(crate) fn mask_passage(
    rules: &Pseudonymiser,
    text: &str,
    start: usize,
    end: usize,
) -> Result<(usize, usize, String)> {
    ensure!(
        start <= end && end <= text.chars().count(),
        "Passage source invalide"
    );
    let spans = replacement_spans(rules, text);
    let map = |p: usize, is_end: bool| -> usize {
        let mut delta = 0isize;
        for (a, b, label) in &spans {
            if p <= *a {
                break;
            }
            if p < *b {
                return (*a as isize + delta) as usize
                    + if is_end { label.chars().count() } else { 0 };
            }
            delta += label.chars().count() as isize - (*b - *a) as isize;
        }
        (p as isize + delta) as usize
    };
    let a = map(start, false);
    let b = map(end, true);
    let masked = mask_with(rules, text);
    Ok((a, b, masked.chars().skip(a).take(b - a).collect()))
}
fn digest(store: &Store, id: &str) -> Result<String> {
    let seg = store.corrected_segmentation(id)?;
    ensure!(
        seg.is_some(),
        "Segmentez l’entretien avant de vérifier son anonymisation"
    );
    Ok(sha256_hex(&serde_json::to_vec(&(
        POLICY,
        seg,
        entries(store)?,
    ))?))
}
fn guard(ok: bool, message: String) -> Result<()> {
    if !ok {
        return Err(cs_model::CsError::Guard(message).into());
    }
    Ok(())
}
pub(crate) fn require_review(store: &Store, id: &str) -> Result<()> {
    guard(config(store)?.reviewed.get(id)==Some(&digest(store,id)?),format!("Anonymisation à vérifier pour {id} : ouvrez Anonymiser dans les paramètres de l’étude, relisez la copie puis validez-la. Aucun envoi IA autorisé."))
}
pub(crate) fn require_all(store: &Store) -> Result<()> {
    for s in store.summaries()? {
        require_review(store, &s.interview_id)?;
    }
    Ok(())
}
pub(crate) fn safe_parts(store: &Store, parts: &mut [PromptPart]) -> Result<()> {
    let r = rules(store)?;
    for p in parts {
        p.text = mask_with(&r, &p.text);
        p.sha256 = sha256_hex(p.text.as_bytes());
    }
    Ok(())
}
fn batch(store: &Store, id: &str, bid: i64) -> Result<(Vec<PromptPart>, String)> {
    let mut parts: Vec<_> = store
        .prompts(id)?
        .into_iter()
        .filter(|p| p.batch_id == bid)
        .collect();
    ensure!(!parts.is_empty(), "Lot de prompts introuvable");
    parts.sort_by_key(|p| p.part);
    safe_parts(store, &mut parts)?;
    let context = (
        store.study_config()?,
        store.codebook()?,
        store.research_questions()?,
        store.interview_attributes(id)?,
        store.shared_person_attributes(id)?,
    );
    let d = sha256_hex(&serde_json::to_vec(&(
        POLICY,
        digest(store, id)?,
        &parts,
        context,
    ))?);
    Ok((parts, d))
}
pub(crate) fn require_batch(store: &Store, id: &str, bid: i64) -> Result<Vec<PromptPart>> {
    require_review(store, id)?;
    let (parts, d) = batch(store, id, bid)?;
    guard(config(store)?.batches.get(&format!("{id}:{bid}")) == Some(&d), "Relisez et validez le prompt anonymisé complet avant copie ou envoi IA. Le texte ou son contexte peut avoir changé.".into())?;
    Ok(parts)
}
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/anonymisation", get(project).put(update))
        .route(
            "/api/anonymisation/interviews/{id}",
            get(preview).put(approve),
        )
        .route(
            "/api/anonymisation/interviews/{id}/export",
            post(export_transcript),
        )
        .route(
            "/api/anonymisation/interviews/{id}/batches/{bid}",
            get(batch_preview).put(batch_approve),
        )
        .route(
            "/api/anonymisation/interviews/{id}/batches/{bid}/copy",
            post(copy_batch),
        )
}
async fn project(State(st): State<AppState>) -> ApiResult {
    let store = st.store();
    let cfg = config(&store)?;
    let interviews: Vec<_> = store
        .summaries()?
        .into_iter()
        .map(|s| {
            let reviewed = digest(&store, &s.interview_id)
                .is_ok_and(|d| cfg.reviewed.get(&s.interview_id) == Some(&d));
            json!({"id":s.interview_id,"reviewed":reviewed})
        })
        .collect();
    Ok(Json(
        json!({"entries":entries(&store)?,"manual_entries":cfg.entries,"interviews":interviews,"prompt":include_str!("../../../docs/prompts/anonymise-research-transcript-verbatim.md"),"local_only":true}),
    ))
}
#[derive(Deserialize)]
struct Update {
    entries: Vec<PseudonymEntry>,
}
async fn update(State(st): State<AppState>, Json(body): Json<Update>) -> ApiResult {
    let store = st.store();
    let mut cfg = config(&store)?;
    let mut names = BTreeSet::new();
    for e in &body.entries {
        ensure_api(
            !e.original.trim().is_empty() && !e.pseudonym.trim().is_empty(),
            "Nom et remplacement requis",
        )?;
        ensure_api(
            ["person", "organisation", "contact", "other"].contains(&e.kind.as_str()),
            "Type de remplacement inconnu",
        )?;
        ensure_api(
            names.insert(e.original.trim().to_lowercase()),
            "Une forme est listée plusieurs fois",
        )?;
    }
    let r = Pseudonymiser::new(&body.entries);
    ensure_api(
        body.entries
            .iter()
            .all(|e| !r.contains_original(&e.pseudonym)),
        "Un remplacement contient une identité",
    )?;
    let overridden: BTreeSet<_> = body
        .entries
        .iter()
        .map(|e| e.original.trim().to_lowercase())
        .collect();
    let mut effective: Vec<_> = entries(&store)?
        .into_iter()
        .filter(|e| !overridden.contains(&e.original.to_lowercase()))
        .collect();
    effective.extend(body.entries.clone());
    let effective_rules = Pseudonymiser::new(&effective);
    ensure_api(
        effective
            .iter()
            .all(|e| !effective_rules.contains_original(&e.pseudonym)),
        "Un remplacement contient une identité de la table existante",
    )?;
    cfg.entries = body.entries;
    cfg.reviewed.clear();
    cfg.batches.clear();
    save(&store, &cfg, &st.actor)?;
    Ok(Json(json!({"saved":true})))
}
fn ensure_api(ok: bool, msg: &str) -> Result<(), ApiError> {
    if ok {
        Ok(())
    } else {
        Err(crate::invalid(msg))
    }
}
async fn preview(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    let store = st.store();
    let seg = store
        .corrected_segmentation(&id)?
        .ok_or_else(|| crate::invalid("Entretien non segmenté"))?;
    let r = rules(&store)?;
    let mut speakers = BTreeSet::new();
    let mut candidates = BTreeMap::<String, Value>::new();
    let capital =
        Regex::new(r"\b\p{Lu}[\p{L}'’\-]{2,}(?:[ \t]+\p{Lu}[\p{L}'’\-]{2,}){0,2}\b").unwrap();
    let units: Vec<_>=seg.units.iter().map(|u| {
        if !generic_speaker(&u.speaker_raw) && r.mask(&u.speaker_raw)==u.speaker_raw {speakers.insert(u.speaker_raw.clone());}
        let masked=mask_with(&r,&u.text);
        for hit in capital.find_iter(&masked) {
            candidates.entry(hit.as_str().into()).or_insert_with(|| json!({"text":hit.as_str(),"unit":u.unit_id}));
        }
        json!({"id":u.unit_id,"speaker":u.speaker_raw,"speaker_anonymised":mask_with(&r,&u.speaker_raw),"original":u.text,"anonymised":masked,"replacement_spans":replacement_spans(&r,&u.text)})
    }).collect();
    let d = digest(&store, &id)?;
    Ok(Json(
        json!({"digest":d,"reviewed":config(&store)?.reviewed.get(&id)==Some(&d),"units":units,"speaker_suggestions":speakers,"candidates":candidates.into_values().collect::<Vec<_>>(),"detection_complete":false}),
    ))
}
#[derive(Deserialize)]
struct Approval {
    digest: String,
    reviewed: bool,
}
async fn approve(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Approval>,
) -> ApiResult {
    let store = st.store();
    let d = digest(&store, &id)?;
    ensure_api(
        body.reviewed && body.digest == d,
        "Copie modifiée ou relecture non confirmée : rechargez et relisez",
    )?;
    let mut cfg = config(&store)?;
    cfg.reviewed.insert(id, d);
    save(&store, &cfg, &st.actor)?;
    Ok(Json(json!({"reviewed":true})))
}
async fn batch_preview(
    State(st): State<AppState>,
    Path((id, bid)): Path<(String, i64)>,
) -> ApiResult {
    let store = st.store();
    let (parts, d) = batch(&store, &id, bid)?;
    Ok(Json(
        json!({"parts":parts,"digest":d,"reviewed":require_batch(&store,&id,bid).is_ok(),"transcript_reviewed":require_review(&store,&id).is_ok()}),
    ))
}
async fn batch_approve(
    State(st): State<AppState>,
    Path((id, bid)): Path<(String, i64)>,
    Json(body): Json<Approval>,
) -> ApiResult {
    let store = st.store();
    require_review(&store, &id)?;
    let (_, d) = batch(&store, &id, bid)?;
    ensure_api(
        body.reviewed && body.digest == d,
        "Prompt modifié ou relecture non confirmée",
    )?;
    let mut cfg = config(&store)?;
    cfg.batches.insert(format!("{id}:{bid}"), d);
    save(&store, &cfg, &st.actor)?;
    Ok(Json(json!({"reviewed":true})))
}
async fn copy_batch(State(st): State<AppState>, Path((id, bid)): Path<(String, i64)>) -> ApiResult {
    Ok(Json(json!(require_batch(&st.store(), &id, bid)?)))
}

async fn export_transcript(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    let store = st.store();
    require_review(&store, &id)?;
    let seg = store
        .corrected_segmentation(&id)?
        .ok_or_else(|| crate::invalid("Entretien non segmenté"))?;
    let rules = rules(&store)?;
    let units:Vec<_>=seg.units.iter().map(|u|json!({"unit_id":u.unit_id,"speaker":mask_with(&rules,&u.speaker_raw),"timestamp":u.timestamp,"text":mask_with(&rules,&u.text)})).collect();
    let base = crate::v2::exports_base(&st.db_path);
    std::fs::create_dir_all(&base).map_err(anyhow::Error::from)?;
    let dir = crate::export::target_dir(&base, "transcript_anonymised");
    let stage = dir.with_extension("building");
    std::fs::create_dir(&stage).map_err(anyhow::Error::from)?;
    let result = (|| -> Result<()> {
        let json = serde_json::to_vec_pretty(
            &json!({"interview_id":mask_with(&rules,&id),"units":units}),
        )?;
        let text = units
            .iter()
            .map(|u| {
                format!(
                    "{} · {}\n{}",
                    u["unit_id"].as_str().unwrap_or(""),
                    u["speaker"].as_str().unwrap_or(""),
                    u["text"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        std::fs::write(stage.join("transcript.json"), &json)?;
        std::fs::write(stage.join("transcript.txt"), &text)?;
        std::fs::write(
            stage.join("manifest.json"),
            serde_json::to_vec_pretty(
                &json!({"policy":POLICY,"review_digest":digest(&store,&id)?,"original_modified":false,"identity_table_included":false,"format":"segmented transcript; unit text and speaker fields preserved except approved replacements and contact masking","json_sha256":sha256_hex(&json),"text_sha256":sha256_hex(text.as_bytes())}),
            )?,
        )?;
        std::fs::rename(&stage, &dir)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    result?;
    Ok(Json(
        json!({"dir":dir.to_string_lossy(),"anonymised":true,"identity_table_included":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule(name: &str, label: &str) -> PseudonymEntry {
        PseudonymEntry {
            kind: "person".into(),
            original: name.into(),
            pseudonym: label.into(),
        }
    }
    #[test]
    fn contact_masking_precedes_name_fragments_and_unicode_offsets_remain_exact() {
        let r = Pseudonymiser::new(&[rule("Jean", "[Person 1]"), rule("Élodie", "[Person 2]")]);
        assert_eq!(
            mask_with(
                &r,
                "Visit https://private.example/Jean@private.example.org, yes."
            ),
            "Visit [URL], yes."
        );
        assert_eq!(
            mask_with(&r, "Jean@private.example.org; Élodie, um…"),
            "[Email]; [Person 2], um…"
        );
        let raw = "😀 Élodie said, yes, yes. Élodie said, yes, yes.";
        let a = raw.chars().position(|c| c == 'É').unwrap();
        let (start, end, quote) = mask_passage(&r, raw, a, a + 6).unwrap();
        assert_eq!(quote, "[Person 2]");
        let masked = mask_with(&r, raw);
        assert_eq!(
            masked
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
            quote
        );
        let (start, end, quote) = mask_passage(&r, raw, a + 1, a + 3).unwrap();
        assert_eq!(quote, "[Person 2]");
        assert_eq!(
            masked
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
            quote
        );
    }
    #[test]
    fn span_plan_preserves_all_unchanged_characters_and_speaker_label_structure() {
        let r = Pseudonymiser::new(&[
            rule("Sophie", "[Person 1]"),
            rule("Jean Dupont", "[Person 2]"),
        ]);
        let raw = "  Sophie [00:01]: Um, Jean Dupont—yes?\r\nSOPHIE: yeah, yeah.\t";
        let spans = replacement_spans(&r, raw);
        assert_eq!(spans.len(), 3);
        let chars: Vec<_> = raw.chars().collect();
        let mut copy = String::new();
        let mut last = 0;
        for (a, b, label) in spans {
            copy.extend(&chars[last..a]);
            copy.push_str(&label);
            last = b;
        }
        copy.extend(&chars[last..]);
        assert_eq!(copy, mask_with(&r, raw));
        assert_eq!(
            copy,
            "  [Person 1] [00:01]: Um, [Person 2]—yes?\r\n[Person 1]: yeah, yeah.\t"
        );
    }
}
