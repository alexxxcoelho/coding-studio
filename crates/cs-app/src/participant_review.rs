//! Deterministic, single-interview participant review. Never sends or changes data.
use crate::{ApiError, AppState};
use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use cs_model::Segmentation;
use cs_store::{PassageFilter, Store, WriteupBlock, WriteupDoc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
pub(crate) struct Options {
    #[serde(default = "yes")]
    codes: bool,
    #[serde(default = "yes")]
    themes: bool,
    #[serde(default = "yes")]
    comments: bool,
}
fn yes() -> bool {
    true
}
fn head(d: &mut WriteupDoc, level: u8, text: impl Into<String>) {
    d.blocks.push(WriteupBlock::Heading {
        level,
        text: text.into(),
    });
}
fn para(d: &mut WriteupDoc, text: impl Into<String>) {
    d.blocks.push(WriteupBlock::Para {
        text: text.into(),
        italic: false,
        cites: vec![],
    });
}
fn bullet(d: &mut WriteupDoc, text: impl Into<String>) {
    d.blocks.push(WriteupBlock::Bullet { text: text.into() });
}
fn annotate_turn(
    doc: &mut WriteupDoc,
    u: &cs_model::Unit,
    codes: &BTreeMap<String, Vec<String>>,
    notes: &BTreeMap<String, Vec<String>>,
    options: &Options,
) {
    if options.comments {
        if let Some(notes) = notes.get(&u.unit_id) {
            for note in notes {
                bullet(doc, note);
            }
        }
    }
    if options.codes {
        let text = if u.role == cs_model::SpeakerRole::Interviewer {
            "Not applicable (interviewer)".to_string()
        } else {
            match codes.get(&u.unit_id) {
                Some(v) if v.is_empty() => "None (no code assigned)".to_string(),
                Some(v) => v.join("; "),
                None => "Not coded yet".to_string(),
            }
        };
        doc.blocks.push(WriteupBlock::Para {
            text: format!("Codes: {text}"),
            italic: true,
            cites: vec![],
        });
    }
}
pub(crate) fn document(store: &Store, id: &str, options: &Options) -> Result<WriteupDoc> {
    let entry = store.interview(id)?.context("Entretien introuvable")?;
    let draft = store
        .setting(&format!("local_preparation:{id}"))?
        .map(|s| serde_json::from_str::<Value>(&s))
        .transpose()?;
    let original = if let Some(v) = draft
        .as_ref()
        .filter(|d| d["reviewed"] == true)
        .and_then(|d| {
            d.get("source_original")
                .filter(|v| !v.is_null())
                .or_else(|| d.get("original"))
        }) {
        serde_json::from_value::<Segmentation>(v.clone())?
    } else {
        let rel = entry
            .primary_source
            .as_deref()
            .context("Choisissez une transcription originale")?;
        cs_ingest::build_segmentation_with(&store.study_config()?, &entry, rel)?
    };
    let mut doc = WriteupDoc {
        blocks: vec![],
        cites: vec![],
    };
    head(
        &mut doc,
        1,
        format!("Interview review and validation — {id}"),
    );
    para(&mut doc,"Please check that this transcript faithfully represents your interview and suggest corrections. The transcript records your words; codes and themes are the researcher’s interpretations, which you may comment on or challenge. Creating this document does not imply your approval.");
    para(
        &mut doc,
        format!(
            "Prepared on {}. Interview reference: {id}.",
            chrono::Utc::now().format("%Y-%m-%d")
        ),
    );
    let c = store.connection();
    let mut q=c.prepare("SELECT p.name,l.role FROM person_interviews l JOIN persons p USING(person_id) WHERE l.interview_id=?1 ORDER BY p.name")?;
    let people = q
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (name, role) in people {
        para(&mut doc, format!("Participant: {name} ({role})."));
    }
    head(&mut doc, 2, "Your feedback");
    for text in [
        "Transcript: accurate / corrections requested / to discuss.",
        "Translation, if included: accurate / corrections requested / to discuss.",
        "Codes and themes: agree / comments / please explain any disagreement.",
        "Items without comments will not automatically be treated as approved.",
        "Your name and date: ____________________________________",
        "General corrections and comments: ____________________________________",
    ] {
        bullet(&mut doc, text);
    }
    let passages = store.passages(&PassageFilter {
        interview_id: Some(id.into()),
        ..Default::default()
    })?;
    let mut code_names = BTreeMap::new();
    let mut query = c.prepare("SELECT code_id,code,name,definition FROM codes")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, token, name, definition) = row?;
        code_names.insert(id.clone(), (id.clone(), name.clone(), definition.clone()));
        code_names.insert(token, (id, name, definition));
    }
    let (finals, _) = store.final_codings(id)?;
    let final_map: BTreeMap<_, _> = finals
        .iter()
        .map(|f| (f.unit_id.clone(), f.codes.clone()))
        .collect();
    let mut inline_codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut used = BTreeSet::new();
    for u in &original.units {
        let values = if let Some(codes) = final_map.get(&u.unit_id) {
            codes.clone()
        } else {
            passages
                .iter()
                .filter(|p| p.unit_id == u.unit_id)
                .flat_map(|p| p.codings.iter().map(|c| c.code_id.clone()))
                .collect()
        };
        let mut labels = BTreeSet::new();
        for code in values.iter().filter(|s| s.as_str() != "NA") {
            if let Some((stable, name, _)) = code_names.get(code) {
                used.insert(stable.clone());
                labels.insert(format!("{name} [{stable}]"));
            }
        }
        if final_map.contains_key(&u.unit_id) || !labels.is_empty() {
            inline_codes.insert(u.unit_id.clone(), labels.into_iter().collect());
        }
    }
    let mut notes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for passage in &passages {
        for coding in &passage.codings {
            if !coding.note.trim().is_empty() {
                notes
                    .entry(passage.unit_id.clone())
                    .or_default()
                    .push(format!(
                        "Coding note [{}]: {}",
                        passage.passage_id, coding.note
                    ));
            }
        }
    }
    head(&mut doc, 2, "Original-language transcript");
    para(&mut doc,"The text below reproduces the retained original source. Refer to the turn identifier when requesting a correction. You may also use Word comments or tracked changes.");
    if options.codes {
        para(
            &mut doc,
            "Codes appear after each turn; full definitions are provided at the end.",
        );
    }
    for u in &original.units {
        head(
            &mut doc,
            3,
            format!("{} {} {}", u.unit_id, u.speaker_raw, u.timestamp),
        );
        para(&mut doc, &u.text);
        annotate_turn(&mut doc, u, &inline_codes, &notes, options);
    }
    if let Some(d) = draft.as_ref().filter(|d| {
        d["reviewed"] == true
            && d["accepted_variant"] == "prepared"
            && !d["options"]["target_language"]
                .as_str()
                .unwrap_or("")
                .trim()
                .is_empty()
    }) {
        let translation: Segmentation = serde_json::from_value(d["prepared"].clone())?;
        head(
            &mut doc,
            2,
            format!(
                "Translation — {}",
                d["options"]["target_language"]
                    .as_str()
                    .unwrap_or("target language")
            ),
        );
        para(&mut doc,"This derived version was prepared with a local model and then reviewed and accepted in the tool. It is shown separately so you can check its meaning. Labels in square brackets replace identities.");
        for u in &translation.units {
            head(
                &mut doc,
                3,
                format!("{} {} {}", u.unit_id, u.speaker_raw, u.timestamp),
            );
            para(&mut doc, &u.text);
            annotate_turn(&mut doc, u, &inline_codes, &notes, options);
        }
    }
    if options.codes {
        head(&mut doc, 2, "Code definitions");
        for id in &used {
            if let Some((_, name, definition)) = code_names.get(id) {
                head(&mut doc, 3, format!("{name} [{id}]"));
                para(&mut doc, definition);
            }
        }
    }
    if options.themes {
        head(&mut doc, 2, "Themes related to this interview");
        para(&mut doc,"These themes are interpretations for discussion. Their inclusion in this document does not imply that you have approved them.");
        let mut query=c.prepare("SELECT t.theme_id,t.title,t.proposition FROM themes t WHERE t.deleted_at IS NULL AND t.status!='discarded' AND EXISTS(SELECT 1 FROM theme_codes tc WHERE tc.theme_id=t.theme_id AND tc.code_id IN (SELECT value FROM json_each(?1))) ORDER BY t.ord,t.theme_id")?;
        let codes = serde_json::to_string(&used)?;
        let rows = query
            .query_map([codes], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            para(
                &mut doc,
                "No themes are currently linked to this interview’s codes.",
            );
        }
        for (_, title, proposition) in rows {
            head(&mut doc, 3, title);
            if !proposition.trim().is_empty() {
                para(&mut doc, proposition);
            }
            para(
                &mut doc,
                "Your feedback sur ce thème : ____________________________________",
            );
        }
    }
    if options.comments {
        head(&mut doc, 2, "Interview comments");
        let mut q=c.prepare("SELECT kind,body FROM interview_data WHERE interview_id=?1 AND kind IN ('memo','checklist')")?;
        for row in q.query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (_, body) = row?;
            if let Ok(v) = serde_json::from_str::<Value>(&body) {
                if let Some(answers) = v["answers"].as_object() {
                    for a in answers.values() {
                        if let Some(t) = a["text"].as_str().filter(|s| !s.trim().is_empty()) {
                            bullet(&mut doc, t);
                        }
                    }
                }
                if let Some(t) = v["note"].as_str().filter(|s| !s.trim().is_empty()) {
                    bullet(&mut doc, t);
                }
            }
        }
    }
    Ok(doc)
}
pub(crate) async fn export(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(options): Query<Options>,
) -> Result<Response, ApiError> {
    let doc = document(&st.store(), &id, &options)?;
    let bytes = crate::docx::render_english(&doc)?;
    let filename = format!(
        "participant-review-{}.docx",
        id.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
    );
    Ok(crate::v2::attachment(
        bytes,
        crate::v2::DOCX_TYPE,
        &filename,
    ))
}
