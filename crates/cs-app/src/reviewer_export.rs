//! Explicit research-data projection, exact field selection and encrypted reviewer package.
use crate::{invalid, ApiError, AppState};
use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    Json,
};
use coding_studio_final_export::{catalogue, select, Catalogue, CryptoSession, Record};
use cs_store::Store;
use rusqlite::{types::ValueRef, Connection};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2_010::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
struct Source {
    root: PathBuf,
    relative: String,
    hash: String,
    size: u64,
}
struct Projection {
    catalogue: Catalogue,
    sources: HashMap<String, Source>,
    rules: usize,
}

// Every identifier and SELECT below is fixed in source, never supplied by a client.
fn rows(
    c: &Connection,
    records: &mut Vec<Record>,
    category: &str,
    sql: &str,
    ids: &[&str],
    default: bool,
) -> Result<()> {
    let mut statement = c.prepare(sql).with_context(|| category.to_string())?;
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    let mut result = statement.query([])?;
    while let Some(row) = result.next()? {
        let mut object = Map::new();
        for (i, name) in names.iter().enumerate() {
            let value = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => json!(v),
                ValueRef::Real(v) => json!(v),
                ValueRef::Text(v) => {
                    let text = std::str::from_utf8(v)?;
                    if matches!(
                        name.as_str(),
                        "body"
                            | "fields"
                            | "attributes"
                            | "flags"
                            | "codes"
                            | "final_codes"
                            | "research_questions"
                            | "supporting_cases"
                            | "divergent_cases"
                            | "main_passages"
                            | "units_for"
                            | "units_against"
                            | "config"
                            | "unit_ids"
                            | "codings"
                    ) {
                        serde_json::from_str(text).unwrap_or_else(|_| json!(text))
                    } else {
                        json!(text)
                    }
                }
                ValueRef::Blob(_) => anyhow::bail!("Unexpected binary field in {category}"),
            };
            object.insert(name.clone(), value);
        }
        let mut path = vec![category.to_string()];
        for id in ids {
            let value = object.get(*id).context("Missing record identity")?;
            path.push(
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
        records.push(Record {
            path,
            value: Value::Object(object),
            selected_by_default: default,
        });
    }
    Ok(())
}

fn mask_value(value: &mut Value, masker: &crate::export_v2::Masker) -> Result<()> {
    match value {
        Value::String(s) => *s = masker.mask(s),
        Value::Array(items) => {
            for item in items {
                mask_value(item, masker)?;
            }
        }
        Value::Object(items) => {
            let old = std::mem::take(items);
            for (key, mut item) in old {
                mask_value(&mut item, masker)?;
                let masked_key = masker.mask(&key);
                anyhow::ensure!(items.insert(masked_key,item).is_none(), "Deux champs deviennent identiques après anonymisation : choisissez des labels distincts");
            }
        }
        _ => (),
    }
    Ok(())
}

fn projection(store: &Store, pseudonymise: bool, actor: &str) -> Result<Projection> {
    let c = store.connection();
    let mut records = Vec::new();
    rows(
        c,
        &mut records,
        "Interviews",
        "SELECT interview_id FROM interviews",
        &["interview_id"],
        false,
    )?;
    rows(
        c,
        &mut records,
        "Interview notes",
        "SELECT interview_id,kind,body FROM interview_data",
        &["interview_id", "kind"],
        true,
    )?;
    rows(c,&mut records,"Transcript","SELECT s.interview_id,u.unit_id,u.seq,u.speaker_raw,COALESCE((SELECT text FROM unit_corrections x WHERE x.interview_id=s.interview_id AND x.unit_id=u.unit_id ORDER BY correction_id DESC LIMIT 1),u.text) AS text,COALESCE((SELECT role FROM unit_corrections x WHERE x.interview_id=s.interview_id AND x.unit_id=u.unit_id ORDER BY correction_id DESC LIMIT 1),u.role) AS role,u.timestamp FROM units u JOIN segmentations s ON s.seg_id=u.seg_id WHERE s.seg_id=(SELECT MAX(z.seg_id) FROM segmentations z WHERE z.interview_id=s.interview_id)",&["interview_id","unit_id"],true)?;
    rows(c,&mut records,"Codebook","SELECT code_id,name,definition,active,parent_id,merged_into,origin,toe,research_questions,includes,excludes,example_passage_id FROM codes WHERE active=1 AND merged_into IS NULL",&["code_id"],true)?;
    rows(c,&mut records,"Inactive codes","SELECT code_id,name,definition,active,parent_id,merged_into FROM codes WHERE active=0 OR merged_into IS NOT NULL",&["code_id"],false)?;
    rows(c,&mut records,"Passages","SELECT passage_id,interview_id,unit_id,start_offset,end_offset,text,origin,role FROM passages WHERE deleted_at IS NULL",&["interview_id","passage_id"],true)?;
    rows(c,&mut records,"Passage coding","SELECT p.interview_id,c.coding_id,c.passage_id,c.code_id,c.note FROM passage_codings c JOIN passages p USING(passage_id) WHERE c.removed_at IS NULL AND p.deleted_at IS NULL",&["interview_id","coding_id"],true)?;
    rows(c,&mut records,"Coding decisions","SELECT interview_id,phase,unit_id,codes,subcode,modifier,ev_level,provenance,memo,excerpt,revised_after_ai,source FROM human_codings",&["interview_id","phase","unit_id"],true)?;
    rows(c,&mut records,"Arbitration","SELECT interview_id,phase,unit_id,choice,final_codes,rationale,decided_by,decided_at FROM arbitrations a WHERE arb_id=(SELECT MAX(b.arb_id) FROM arbitrations b WHERE b.interview_id=a.interview_id AND b.phase=a.phase AND b.unit_id=a.unit_id)",&["interview_id","phase","unit_id"],true)?;
    rows(
        c,
        &mut records,
        "Unit status",
        "SELECT interview_id,unit_id,flags,co_constructed_kind FROM unit_flags",
        &["interview_id", "unit_id"],
        true,
    )?;
    rows(c,&mut records,"Audio verification","SELECT interview_id,unit_id,state,text_sha256,role,note FROM unit_verifications a WHERE verification_id=(SELECT MAX(b.verification_id) FROM unit_verifications b WHERE a.interview_id=b.interview_id AND a.unit_id=b.unit_id)",&["interview_id","unit_id"],true)?;
    rows(c,&mut records,"Themes","SELECT theme_id,title,status,proposition,conditions,alternative_interpretation,limits,decision_memo,supporting_cases,divergent_cases,main_passages,ord FROM themes WHERE deleted_at IS NULL",&["theme_id"],true)?;
    rows(
        c,
        &mut records,
        "Theme codes",
        "SELECT theme_id,code_id FROM theme_codes",
        &["theme_id", "code_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Theme questions",
        "SELECT theme_id,rq_id FROM theme_questions",
        &["theme_id", "rq_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Research questions",
        "SELECT id,text,ord FROM research_questions",
        &["id"],
        true,
    )?;
    rows(c,&mut records,"Question notes","SELECT rq_id,documented,interpretation,rivals,limits,units_for,units_against FROM rq_memos",&["rq_id"],true)?;
    rows(
        c,
        &mut records,
        "Cases",
        "SELECT case_id,interview_id,label,fields,stage,outcome_status,notes FROM cases",
        &["case_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Case units",
        "SELECT case_id,unit_id,interview_id FROM case_units",
        &["case_id", "interview_id", "unit_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Matrix",
        "SELECT row_key,col_key,value,summary FROM matrix_cells",
        &["row_key", "col_key"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Matrix passages",
        "SELECT row_key,col_key,passage_id FROM matrix_cell_passages WHERE removed_at IS NULL",
        &["row_key", "col_key", "passage_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "Matrix views",
        "SELECT view_id,name,config FROM matrix_views WHERE deleted_at IS NULL",
        &["view_id"],
        true,
    )?;
    rows(c,&mut records,"Analytical notes","SELECT scope,target,sub,body FROM writing_versions a WHERE version_id=(SELECT MAX(b.version_id) FROM writing_versions b WHERE b.scope=a.scope AND b.target=a.target AND b.sub=a.sub)",&["scope","target","sub"],true)?;
    rows(c,&mut records,"Quotations","SELECT claim_id,interview_id,unit_id,excerpt,codes,ev_level,provenance,role,memo,nature FROM claims",&["interview_id","claim_id"],true)?;
    rows(c,&mut records,"Evidence links","SELECT link_id,passage_id,target_kind,target_id,part,link_type,note FROM passage_links WHERE removed_at IS NULL",&["link_id"],true)?;
    rows(c,&mut records,"People","SELECT person_id,kind,name,organisation,person_function,email,phone,notes,consent_status,consent_date,consent_document,anonymise,pseudonym,attributes FROM persons",&["person_id"],false)?;
    rows(
        c,
        &mut records,
        "People groups",
        "SELECT group_id,name,parent_id,description FROM person_groups",
        &["group_id"],
        false,
    )?;
    rows(
        c,
        &mut records,
        "People interviews",
        "SELECT person_id,interview_id,role FROM person_interviews",
        &["person_id", "interview_id"],
        false,
    )?;
    rows(c,&mut records,"AI phase 2 prompts","SELECT prompt_id,interview_id,batch_id,part,of,unit_ids,codes,segmentation_hash,codebook_hash,prompt_version,text,sha256 FROM prompts WHERE phase=2",&["interview_id","prompt_id"],false)?;
    rows(c,&mut records,"AI phase 2 responses","SELECT o.output_id,r.interview_id,o.run_id,o.part,o.of,o.raw_text,o.raw_sha256 FROM ai_outputs o JOIN ai_runs r USING(run_id) WHERE EXISTS(SELECT 1 FROM prompts p WHERE p.interview_id=r.interview_id AND p.batch_id=r.batch_id AND p.phase=2)",&["interview_id","output_id"],false)?;
    rows(c,&mut records,"AI phase 2 runs","SELECT r.run_id,r.interview_id,r.batch_id,r.run_label,r.model_name,r.model_version,r.temperature,r.seed,r.prompt_id_version,r.executed_at,r.notes FROM ai_runs r WHERE EXISTS(SELECT 1 FROM prompts p WHERE p.interview_id=r.interview_id AND p.batch_id=r.batch_id AND p.phase=2)",&["interview_id","run_id"],false)?;
    rows(c,&mut records,"AI phase 2 references","SELECT snapshot_id,interview_id,batch_id,phase,segmentation_hash,codebook_hash,codings,sha256 FROM human_snapshots WHERE phase=2",&["interview_id","snapshot_id"],false)?;
    rows(
        c,
        &mut records,
        "AI review samples",
        "SELECT run_id,seed,unit_ids,n_agreeing FROM review_samples",
        &["run_id"],
        false,
    )?;
    rows(
        c,
        &mut records,
        "Unit marks",
        "SELECT interview_id,unit_id,mark,note FROM unit_marks",
        &["interview_id", "unit_id"],
        true,
    )?;
    rows(
        c,
        &mut records,
        "People memberships",
        "SELECT person_id,group_id FROM person_group_members",
        &["person_id", "group_id"],
        false,
    )?;
    rows(c, &mut records, "Project configuration", "SELECT 'attributes' AS name, json_extract(value,'$.attributes') AS body FROM settings WHERE key='project_config'", &["name"], true)?;
    rows(c, &mut records, "Project configuration", "SELECT 'memo_templates' AS name, json_extract(value,'$.memo_templates') AS body FROM settings WHERE key='project_config'", &["name"], true)?;
    rows(c,&mut records,"Transcript preparation","SELECT substr(key,length('local_preparation:')+1) AS interview_id,json_extract(value,'$.version') AS version,json_extract(value,'$.options.model') AS local_model,json_extract(value,'$.options.anonymise') AS anonymisation_requested,json_extract(value,'$.options.cleanup') AS cleanup_requested,json_extract(value,'$.options.target_language') AS target_language,json_extract(value,'$.accepted_variant') AS accepted_variant,json_extract(value,'$.created_at') AS prepared_at FROM settings WHERE key LIKE 'local_preparation:%' AND json_valid(value) AND json_extract(value,'$.reviewed')=1",&["interview_id"],true)?;
    let mut sources = HashMap::new();
    let mut q = c.prepare("SELECT s.interview_id,i.folder_path,s.rel_path,s.sha256,s.size,s.kind FROM sources s JOIN interviews i USING(interview_id) ORDER BY s.interview_id,s.rel_path")?;
    let mut r = q.query([])?;
    while let Some(row) = r.next()? {
        let interview: String = row.get(0)?;
        let root: String = row.get(1)?;
        let relative: String = row.get(2)?;
        let hash: String = row.get(3)?;
        let size: u64 = row.get(4)?;
        let kind: String = row.get(5)?;
        let token = cs_model::sha256_hex(relative.as_bytes());
        let path = vec!["Source files".into(), interview, token];
        records.push(Record {
            path: path.clone(),
            value: json!({"name":relative,"sha256":hash,"size":size,"kind":kind,"download":size}),
            selected_by_default: false,
        });
        let mut leaf = path;
        leaf.push("download".into());
        let id = cs_model::sha256_hex(&serde_json::to_vec(&leaf)?);
        sources.insert(
            id,
            Source {
                root: PathBuf::from(root),
                relative,
                hash,
                size,
            },
        );
    }
    let original_units: HashMap<(String, String), String> = records
        .iter()
        .filter(|r| r.path[0] == "Transcript")
        .filter_map(|r| {
            Some((
                (
                    r.value["interview_id"].as_str()?.into(),
                    r.value["unit_id"].as_str()?.into(),
                ),
                r.value["text"].as_str()?.into(),
            ))
        })
        .collect();
    if pseudonymise {
        let (privacy_masker, _) = crate::export_v2::Masker::new(store, &Default::default(), actor)?;
        for record in records.iter_mut().filter(|r| r.path[0] == "Passages") {
            if let (Some(id), Some(unit), Some(start), Some(end)) = (
                record.value["interview_id"].as_str(),
                record.value["unit_id"].as_str(),
                record.value["start_offset"].as_u64(),
                record.value["end_offset"].as_u64(),
            ) {
                if let Some(text) = original_units.get(&(id.into(), unit.into())) {
                    let original_quote = record.value["text"].as_str().unwrap_or("");
                    let count = text.chars().count();
                    let mut range = None;
                    if start <= end
                        && end as usize <= count
                        && text
                            .chars()
                            .skip(start as usize)
                            .take((end - start) as usize)
                            .collect::<String>()
                            == original_quote
                    {
                        range = Some((start as usize, end as usize));
                    } else if !original_quote.is_empty() {
                        let matches: Vec<_> = text.match_indices(original_quote).collect();
                        if matches.len() == 1 {
                            let a = text[..matches[0].0].chars().count();
                            range = Some((a, a + original_quote.chars().count()));
                        }
                    }
                    if let Some((start, end)) = range {
                        let (a, b, quote) = privacy_masker.mask_passage(text, start, end)?;
                        record.value["start_offset"] = json!(a);
                        record.value["end_offset"] = json!(b);
                        record.value["text"] = json!(quote);
                    } else {
                        // A correction can invalidate a historical anchor. Never
                        // replace the researcher's quote with another source slice.
                        record.value["start_offset"] = Value::Null;
                        record.value["end_offset"] = Value::Null;
                        record.value["anchor_status"] = json!("unresolved_after_source_edit");
                    }
                }
            }
        }
    }
    let rules = crate::anonymisation::entries(store)?.len();
    if pseudonymise {
        records.retain(|r| r.path[0] != "Source files");
        sources.clear();
        for record in &mut records {
            if record.path[0] == "People" {
                if let Some(o) = record.value.as_object_mut() {
                    for k in [
                        "email",
                        "phone",
                        "notes",
                        "consent_document",
                        "consent_date",
                        "consent_status",
                    ] {
                        o.remove(k);
                    }
                }
            }
        }
    }
    if pseudonymise {
        let (masker, _) = crate::export_v2::Masker::new(store, &Default::default(), actor)?;
        for record in &mut records {
            for path in &mut record.path {
                *path = masker.mask(path);
            }
            mask_value(&mut record.value, &masker)?;
        }
    }
    Ok(Projection {
        catalogue: catalogue(&records).map_err(anyhow::Error::msg)?,
        sources,
        rules,
    })
}

#[derive(Deserialize)]
pub(crate) struct CatalogueQuery {
    #[serde(default = "yes")]
    pseudonymise: bool,
}
fn ensure_anonymised(value: bool) -> Result<()> {
    anyhow::ensure!(
        value,
        "Seule la copie anonymisée est disponible pour les correcteurs"
    );
    Ok(())
}
fn yes() -> bool {
    true
}
pub(crate) async fn get_catalogue(
    State(st): State<AppState>,
    Query(q): Query<CatalogueQuery>,
) -> Result<Json<Value>, ApiError> {
    let store = st.store();
    ensure_anonymised(q.pseudonymise)?;
    let p = projection(&store, true, &st.actor)?;
    let interviews = store
        .connection()
        .query_row("SELECT COUNT(*) FROM interviews", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(anyhow::Error::from)?;
    let transcripts = store
        .connection()
        .query_row(
            "SELECT COUNT(DISTINCT interview_id) FROM segmentations",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(anyhow::Error::from)?;
    Ok(Json(
        json!({"snapshot":p.catalogue.snapshot,"fields":p.catalogue.fields,"file_ids":p.sources.iter().map(|(id,s)|(id,s.size)).collect::<HashMap<_,_>>(),"pseudonym_rules":p.rules,"coverage":{"interviews":interviews,"transcripts":transcripts}}),
    ))
}

#[derive(Deserialize)]
pub(crate) struct ExportBody {
    #[serde(default)]
    privacy_reviewed: bool,
    snapshot: String,
    selected: Vec<String>,
    password: String,
    #[serde(default = "yes")]
    pseudonymise: bool,
    #[serde(default)]
    url: String,
}

fn checked_url(text: &str) -> Result<Option<String>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let url = url::Url::parse(text.trim()).context("Adresse HTTPS invalide")?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Utilisez une adresse HTTPS sans identifiants, paramètres ni fragment"
    );
    Ok(Some(url.to_string()))
}

fn source_path(source: &Source) -> Result<PathBuf> {
    let relative = Path::new(&source.relative);
    anyhow::ensure!(
        !relative.is_absolute()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Chemin source non autorisé"
    );
    let root = source.root.canonicalize()?;
    let mut path = root.clone();
    for component in relative.components() {
        path.push(component);
        anyhow::ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Lien symbolique source non autorisé"
        );
    }
    let real = path.canonicalize()?;
    anyhow::ensure!(
        real.starts_with(&root) && real.is_file(),
        "Source hors du dossier de l’entretien"
    );
    Ok(real)
}

fn package(store: &Store, actor: &str, db: &Path, body: ExportBody) -> Result<Value> {
    anyhow::ensure!(
        body.pseudonymise,
        "L’export correcteurs accepte uniquement une copie anonymisée"
    );
    crate::anonymisation::require_all(store)?;
    anyhow::ensure!(
        body.privacy_reviewed,
        "Confirmez la relecture des champs anonymisés sélectionnés"
    );
    let public_url = checked_url(&body.url)?;
    let p = projection(store, body.pseudonymise, actor)?;
    let mut payload =
        select(&p.catalogue, &body.snapshot, &body.selected).map_err(anyhow::Error::msg)?;
    let session = CryptoSession::new(&body.password).map_err(anyhow::Error::msg)?;
    let base = crate::v2::exports_base(db);
    fs::create_dir_all(&base)?;
    let dir = crate::export::target_dir(&base, "reviewer");
    // No partial export is returned. The private work directory is removed on failure.
    let stage = dir.with_extension("building");
    fs::create_dir(&stage)?;
    let result = (|| -> Result<Value> {
        fs::create_dir(stage.join("data"))?;
        let mut file_count = 0usize;
        let mut total_bytes = 0u64;
        let mut replacements = 0usize;
        for field in &mut payload.fields {
            let id = cs_model::sha256_hex(&serde_json::to_vec(&field.path)?);
            if let Some(source) = p.sources.get(&id) {
                anyhow::ensure!(source.size<=1024*1024*1024,"Un fichier dépasse 1 Gio, la limite de lecture : sélectionner une version plus légère");
                let path = source_path(source)?;
                let mut input = fs::File::open(path)?;
                anyhow::ensure!(
                    input.metadata()?.len() == source.size,
                    "Source modifiée : relancez le scan et la sélection"
                );
                let mut hash = Sha256::new();
                let mut chunks = Vec::new();
                let mut size = 0u64;
                let token = cs_model::sha256_hex(
                    format!(
                        "{id}:{}",
                        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                    )
                    .as_bytes(),
                );
                loop {
                    let mut bytes = vec![0; 8 * 1024 * 1024];
                    let mut used = 0;
                    while used < bytes.len() {
                        let n = input.read(&mut bytes[used..])?;
                        if n == 0 {
                            break;
                        }
                        used += n;
                    }
                    if used == 0 {
                        break;
                    }
                    bytes.truncate(used);
                    hash.update(&bytes);
                    size += used as u64;
                    let identity = format!("CodingStudio file v1:{token}:{}", chunks.len());
                    let (ciphertext, iv) = session
                        .chunk(&bytes, &identity)
                        .map_err(anyhow::Error::msg)?;
                    let name = format!("{token}-{}.bin", chunks.len());
                    fs::write(stage.join("data").join(&name), &ciphertext)?;
                    chunks.push(json!({"name":name,"iv":iv,"aad":identity}));
                }
                anyhow::ensure!(
                    size == source.size && format!("{:x}", hash.finalize()) == source.hash,
                    "Source modifiée ou empreinte incorrecte : aucun dossier final créé"
                );
                // The download has a neutral name. Original names are separate selectable fields.
                field.value = json!({"reviewer_file_v1":true,"download_name":format!("source-{}",&token[..12]),"size":size,"sha256":source.hash,"chunks":chunks});
                file_count += 1;
                total_bytes += size;
            }
        }
        if body.pseudonymise {
            // Projection was already masked; count replacements on the raw selected copy only.
            let raw = projection(store, false, actor)?;
            let raw_selected = select(&raw.catalogue, &raw.catalogue.snapshot, &body.selected)
                .map_err(anyhow::Error::msg)?;
            let (masker, _) = crate::export_v2::Masker::new(store, &Default::default(), actor)?;
            for mut field in raw_selected.fields {
                mask_value(&mut field.value, &masker)?;
            }
            let counts = masker.replacements();
            replacements = counts.person + counts.organisation + counts.researcher;
        }
        payload.export_info = Some(
            json!({"pseudonymised_text":body.pseudonymise,"replacements":replacements,"stored_digests_refer_to_originals":true,"original_files_pseudonymised":false}),
        );
        let envelope = session.payload(&payload).map_err(anyhow::Error::msg)?;
        fs::write(
            stage.join("index.html"),
            coding_studio_final_export::reader_html(&envelope).map_err(anyhow::Error::msg)?,
        )?;
        if let Some(address) = &public_url {
            fs::write(stage.join("URL.txt"), format!("{address}\n"))?;
            let code = qrcode::QrCode::new(address.as_bytes())?;
            fs::write(
                stage.join("QR.svg"),
                code.render::<qrcode::render::svg::Color>()
                    .min_dimensions(320, 320)
                    .build(),
            )?;
        }
        fs::write(stage.join("README.txt"),"CODING STUDIO · DOSSIER DE CONSULTATION\n\nContenu chiffré, accès en lecture seule. Ouvrir index.html avec un navigateur récent. Le mot de passe doit être transmis séparément. Pour les pièces jointes en local, choisir le dossier data dans le lecteur. Pour héberger, déposer l’intégralité de ce dossier sur un serveur HTTPS de confiance. Ne pas déposer le mot de passe. Aucun hébergement ni contrôle de comptes n’est fourni. Les copies téléchargées ne peuvent pas être révoquées.\n")?;
        fs::write(
            stage.join("COPYING.LESSER"),
            include_str!("../../../LICENSE"),
        )?;
        fs::write(stage.join("NOTICE.txt"), include_str!("../../../NOTICE"))?;
        // Only the encrypted/public package enters the upload ZIP, never the database or password.
        let mut names = vec![
            "index.html".to_string(),
            "README.txt".to_string(),
            "COPYING.LESSER".to_string(),
            "NOTICE.txt".to_string(),
        ];
        if public_url.is_some() {
            names.extend(["URL.txt".into(), "QR.svg".into()]);
        }
        for entry in fs::read_dir(stage.join("data"))? {
            names.push(format!("data/{}", entry?.file_name().to_string_lossy()));
        }
        names.sort();
        let mut zip = zip::ZipWriter::new(fs::File::create(stage.join("upload.zip"))?);
        for name in names {
            zip.start_file(
                &name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )?;
            std::io::copy(&mut fs::File::open(stage.join(name))?, &mut zip)?;
        }
        zip.finish()?.sync_all()?;
        fs::rename(&stage, &dir)?;
        Ok(
            json!({"dir":dir.to_string_lossy(),"reader":dir.join("index.html").to_string_lossy(),"zip":dir.join("upload.zip").to_string_lossy(),"fields":payload.fields.len(),"files":file_count,"source_bytes":total_bytes,"pseudonym_replacements":replacements,"url":public_url,"published":false}),
        )
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(stage);
    }
    result
}

pub(crate) async fn generate(
    State(st): State<AppState>,
    Json(body): Json<ExportBody>,
) -> Result<Json<Value>, ApiError> {
    // Disk IO and key derivation must not block the API runtime.
    tokio::task::spawn_blocking(move || {
        package(&st.store(), &st.actor, &st.db_path, body)
            .map(Json)
            .map_err(|e| invalid(e.to_string()))
    })
    .await
    .map_err(|_| invalid("L’export n’a pas pu être terminé"))?
}

#[derive(Deserialize)]
pub(crate) struct OpenBody {
    dir: String,
    kind: String,
}
pub(crate) async fn open_package(
    State(st): State<AppState>,
    Json(body): Json<OpenBody>,
) -> Result<Json<Value>, ApiError> {
    let base = crate::v2::exports_base(&st.db_path)
        .canonicalize()
        .map_err(anyhow::Error::from)?;
    let dir = PathBuf::from(&body.dir)
        .canonicalize()
        .map_err(anyhow::Error::from)?;
    if dir.parent() != Some(base.as_path())
        || !dir
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("_reviewer"))
        || !dir.join("upload.zip").is_file()
    {
        return Err(invalid("Dossier d’export non autorisé"));
    }
    let path = match body.kind.as_str() {
        "folder" => dir,
        "reader" => dir.join("index.html"),
        _ => return Err(invalid("Choisissez le dossier ou le lecteur")),
    };
    open::that(path).map_err(anyhow::Error::from)?;
    Ok(Json(json!({"opened":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_never_contains_credentials_or_secret_query() {
        assert_eq!(checked_url("").unwrap(), None);
        assert_eq!(
            checked_url("https://example.org/research/")
                .unwrap()
                .as_deref(),
            Some("https://example.org/research/")
        );
        for bad in [
            "http://example.org/",
            "https://user:pass@example.org/",
            "https://example.org/?password=x",
            "https://example.org/#secret",
            "javascript:alert(1)",
        ] {
            assert!(checked_url(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn source_cannot_escape_or_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let source = |relative: &str| Source {
            root: dir.path().into(),
            relative: relative.into(),
            hash: String::new(),
            size: 0,
        };
        fs::write(dir.path().join("ok.txt"), "ok").unwrap();
        assert!(source_path(&source("ok.txt")).is_ok());
        assert!(source_path(&source("../secret")).is_err());
        assert!(source_path(&source("/etc/passwd")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("ok.txt"), dir.path().join("link")).unwrap();
            assert!(source_path(&source("link")).is_err());
        }
    }
}
