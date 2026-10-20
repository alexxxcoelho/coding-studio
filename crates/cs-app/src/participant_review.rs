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
        format!("Relecture et validation de votre entretien {id}"),
    );
    para(&mut doc,"Ce document vous permet de vérifier la fidélité de votre entretien et de proposer des corrections. La transcription rapporte vos propos ; les codes et thèmes sont des interprétations du chercheur que vous pouvez commenter ou contester. Aucune validation n’est présumée par la création de ce document.");
    para(
        &mut doc,
        format!(
            "Version préparée le {}. Référence de l’entretien : {id}.",
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
        para(&mut doc, format!("Intervenant : {name} ({role})."));
    }
    head(&mut doc, 2, "Votre retour");
    for text in [
        "Transcription fidèle : oui / corrections demandées / à discuter.",
        "Traduction, si présente : fidèle / corrections demandées / à discuter.",
        "Codes et thèmes : accord / remarques / désaccord à préciser.",
        "Les points non commentés ne seront pas considérés automatiquement comme approuvés.",
        "Votre nom et la date : ____________________________________",
        "Corrections et remarques générales : ____________________________________",
    ] {
        bullet(&mut doc, text);
    }
    head(&mut doc, 2, "Transcription dans la langue originale");
    para(&mut doc,"Le texte ci-dessous reprend la source conservée. Citez l’identifiant du tour pour signaler une correction ; vous pouvez aussi utiliser les commentaires ou le suivi des modifications de Word.");
    for u in &original.units {
        head(
            &mut doc,
            3,
            format!("{} {} {}", u.unit_id, u.speaker_raw, u.timestamp),
        );
        para(&mut doc, &u.text);
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
                "Traduction en {}",
                d["options"]["target_language"]
                    .as_str()
                    .unwrap_or("langue cible")
            ),
        );
        para(&mut doc,"Cette version dérivée a été préparée avec un modèle local puis acceptée dans l’outil. Elle est présentée séparément afin que vous puissiez en vérifier le sens. Les labels entre crochets remplacent des identités.");
        for u in &translation.units {
            head(
                &mut doc,
                3,
                format!("{} {} {}", u.unit_id, u.speaker_raw, u.timestamp),
            );
            para(&mut doc, &u.text);
        }
    }
    let passages = store.passages(&PassageFilter {
        interview_id: Some(id.into()),
        ..Default::default()
    })?;
    let mut code_names = BTreeMap::new();
    let mut query = c.prepare("SELECT code_id,name,definition FROM codes")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, name, definition) = row?;
        code_names.insert(id, (name, definition));
    }
    let (finals, _) = store.final_codings(id)?;
    let mut used: BTreeSet<String> = finals
        .iter()
        .flat_map(|f| f.codes.iter().filter(|s| s.as_str() != "NA").cloned())
        .collect();
    if options.codes {
        head(&mut doc, 2, "Passages codés et interprétation proposée");
        para(&mut doc,"Les extraits cités ici sont ceux utilisés pour coder. Ils peuvent différer de la source originale si une correction a été enregistrée. Ces codes décrivent la lecture du chercheur ; votre retour peut porter sur leur pertinence et sur le contexte.");
        for p in &passages {
            head(&mut doc, 3, format!("{} {}", p.unit_id, p.passage_id));
            para(&mut doc, &p.text);
            for coding in &p.codings {
                used.insert(coding.code_id.clone());
                if let Some((name, definition)) = code_names.get(&coding.code_id) {
                    bullet(&mut doc, format!("{name} : {definition}"));
                }
                if options.comments && !coding.note.trim().is_empty() {
                    bullet(&mut doc, format!("Commentaire : {}", coding.note));
                }
            }
            para(
                &mut doc,
                "Votre remarque ou correction : ____________________________________",
            );
        }
        for f in &finals {
            if !passages.iter().any(|p| p.unit_id == f.unit_id) && f.codes.iter().any(|s| s != "NA")
            {
                head(&mut doc, 3, &f.unit_id);
                for code in &f.codes {
                    if let Some((name, definition)) = code_names.get(code) {
                        bullet(&mut doc, format!("{name} : {definition}"));
                    }
                }
            }
        }
    } else {
        for p in &passages {
            for code in &p.codings {
                used.insert(code.code_id.clone());
            }
        }
    }
    if options.themes {
        head(&mut doc, 2, "Thèmes liés à cet entretien");
        para(&mut doc,"Ces thèmes sont des interprétations à discuter. Leur présence dans ce dossier ne signifie pas que vous les avez validés.");
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
                "Aucun thème relié aux codes de cet entretien pour le moment.",
            );
        }
        for (_, title, proposition) in rows {
            head(&mut doc, 3, title);
            if !proposition.trim().is_empty() {
                para(&mut doc, proposition);
            }
            para(
                &mut doc,
                "Votre retour sur ce thème : ____________________________________",
            );
        }
    }
    if options.comments {
        head(&mut doc, 2, "Commentaires de cet entretien");
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
    let bytes = crate::docx::render(&doc)?;
    let filename = format!(
        "validation-{}.docx",
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
