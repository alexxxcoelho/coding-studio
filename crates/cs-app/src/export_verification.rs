//! The verification package: what an examiner needs to check that the
//! transcripts behind a thesis are real, without learning who asked not to be
//! named.
//!
//! For the chosen interviews it holds the original transcripts and the coded
//! transcript, with the names of the records that are not anonymised kept and
//! the names (and aliases) of the records that are anonymised replaced by
//! their pseudonym; a register of the persons (pseudonym for the first kind,
//! real name for the second); and a manifest that gives, for each source
//! file, the SHA-256 of the original as it is on the researcher's disk, so
//! that the original can be shown later and checked against it. A file in
//! which nothing was replaced is delivered byte for byte, and its delivered
//! digest equals the original digest.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use axum::extract::State;
use axum::Json;
use cs_model::pseudonym::Pseudonymiser;
use cs_model::{sha256_hex, CsError, SourceKind};
use cs_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::export::{target_dir, ExportedFile};
use crate::{ApiResult, AppState};

/// Source kinds that are transcripts of the interview (the others are notes
/// and working files of the researcher, listed but not delivered).
fn is_transcript(kind: SourceKind) -> bool {
    matches!(
        kind,
        SourceKind::Vtt
            | SourceKind::TimestampedText
            | SourceKind::CleanedMarkdown
            | SourceKind::BudgieMarkdown
            | SourceKind::WrittenResponse
            | SourceKind::Docx
            | SourceKind::Pdf
    )
}

/// Result of the export.
#[derive(Debug, Serialize)]
pub struct VerificationExport {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub interview_ids: Vec<String>,
    /// Number of replacements by kind of record (never the names).
    pub replacements: BTreeMap<String, usize>,
    pub pseudonymisation_digest: String,
    /// Source files whose digest on disk differs from the one recorded at the
    /// scan, or that could not be read.
    pub warnings: Vec<String>,
}

struct Out {
    dir: PathBuf,
    files: Vec<ExportedFile>,
}

impl Out {
    fn put(&mut self, name: &str, bytes: &[u8]) -> Result<ExportedFile> {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        if path.exists() {
            anyhow::bail!("refusing to overwrite {}", path.display());
        }
        fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        let f = ExportedFile {
            name: name.to_string(),
            sha256: sha256_hex(bytes),
            size: bytes.len() as u64,
        };
        self.files.push(ExportedFile {
            name: f.name.clone(),
            sha256: f.sha256.clone(),
            size: f.size,
        });
        Ok(f)
    }
}

/// The coded transcript of an interview in Markdown: each unit with its
/// speaker and time, and under it the passages with their codes.
fn coded_transcript(store: &Store, id: &str) -> Result<Option<String>> {
    let Some(seg) = store.segmentation(id)? else {
        return Ok(None);
    };
    let (units, _) = crate::export::current_units(store, id, &seg)?;
    let passages = crate::export_v1::all_passages(store, &[id.to_string()])?;
    let mut by_unit: BTreeMap<&str, Vec<&cs_model::passage::Passage>> = BTreeMap::new();
    for p in &passages {
        by_unit.entry(p.unit_id.as_str()).or_default().push(p);
    }
    let mut s = format!("# {id}, transcription codée\n\n");
    s.push_str("Texte courant de chaque unité (corrections comprises), suivi des passages sélectionnés et de leurs codes.\n\n");
    for u in &units {
        s.push_str(&format!(
            "## {} ({}, {})\n\n{}\n\n",
            u.unit_id,
            crate::export::speaker_of(u),
            u.timestamp,
            u.text
        ));
        for p in by_unit.get(u.unit_id.as_str()).into_iter().flatten() {
            let codes: Vec<&str> = p
                .codings
                .iter()
                .filter(|c| !c.system)
                .map(|c| c.code_name.as_str())
                .collect();
            let later = p.codings.iter().any(|c| c.system);
            let label = if codes.is_empty() {
                if later {
                    "à reprendre plus tard".to_string()
                } else {
                    "surlignage sans code".to_string()
                }
            } else {
                codes.join("; ")
            };
            s.push_str(&format!("- « {} » : {}\n", p.text.trim(), label));
        }
        if by_unit.contains_key(u.unit_id.as_str()) {
            s.push('\n');
        }
    }
    Ok(Some(s))
}

fn replacements_by_pseudonym(
    rules: &Pseudonymiser,
    counts: &[usize],
    entries: &[cs_model::pseudonym::PseudonymEntry],
) -> Vec<Value> {
    let mut by: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (o, n) in rules.originals().iter().zip(counts) {
        if *n == 0 {
            continue;
        }
        if let Some(e) = entries.iter().find(|e| e.original == *o) {
            *by.entry((e.pseudonym.clone(), e.kind.clone())).or_default() += n;
        }
    }
    by.into_iter()
        .map(|((pseudonym, kind), count)| json!({"pseudonym": pseudonym, "kind": kind, "count": count}))
        .collect()
}

fn readme(
    ids: &[String],
    r: &BTreeMap<String, usize>,
    n_entries: usize,
    n_register: usize,
) -> String {
    let mut s = String::from("# Dossier de vérification des transcriptions\n\n");
    s.push_str(&format!(
        "Dossier produit par Coding Studio {} le {}, pour les entretiens suivants : {}.\n\n",
        crate::VERSION,
        crate::now(),
        ids.join(", ")
    ));
    s.push_str("## Objet\n\n");
    s.push_str("Ce dossier permet à un examinateur de s'assurer que les transcriptions sur lesquelles repose la recherche existent et n'ont pas été modifiées, sans révéler l'identité des personnes et des organisations qui ont demandé à rester anonymes.\n\n");
    s.push_str("## Ce qui a été remplacé\n\n");
    s.push_str(&format!(
        "Le registre des personnes du projet compte {n_register} fiche(s) dans ce dossier. Les noms, les variantes de noms (prénoms seuls, initiales, autres graphies) et les noms d'organisations des fiches marquées « à anonymiser » ont été remplacés par le pseudonyme de la fiche : {} remplacement(s) pour les personnes, {} pour les organisations. {n_entries} entrée(s) de la liste de pseudonymisation du projet ont été appliquées, y compris celles saisies à la main.\n\n",
        r.get("person").copied().unwrap_or(0),
        r.get("organisation").copied().unwrap_or(0)
    ));
    s.push_str("Les noms des personnes et des organisations dont la fiche n'est pas marquée « à anonymiser » sont conservés tels qu'ils figurent dans les originaux. Le fichier `persons.csv` donne, pour chaque fiche, le pseudonyme si la fiche est anonymisée et le nom réel sinon.\n\n");
    s.push_str("## Ce qui n'a pas été remplacé\n\n");
    s.push_str("La liste ne remplace que les noms qu'elle contient, sous la forme où ils y figurent. Une autre graphie d'un nom, un surnom absent des variantes, une information qui identifie indirectement une personne (une fonction, un lieu, une date, un chiffre) et le texte libre ne sont pas couverts. Il faut relire les fichiers avant de les transmettre. Les fichiers Word et PDF ne sont pas copiés, car leur texte ne peut pas être remplacé sans les modifier ; ils figurent dans le manifeste avec l'empreinte de l'original.\n\n");
    s.push_str("## Contenu\n\n");
    s.push_str("- `persons.csv` : le registre des personnes liées aux entretiens, avec les groupes et les rôles dans chaque entretien.\n");
    s.push_str("- `groups.csv` : l'arbre des groupes.\n");
    s.push_str("- `interviews/<identifiant>/original/` : les transcriptions d'origine, avec les remplacements décrits ci-dessus.\n");
    s.push_str("- `interviews/<identifiant>/transcript_coded.md` : la transcription codée, unité par unité, avec les passages sélectionnés et leurs codes.\n");
    s.push_str("- `manifest.json` : la liste des fichiers livrés avec leur empreinte SHA-256, et, pour chaque fichier source, l'empreinte de l'original.\n\n");
    s.push_str("## Vérifier les empreintes\n\n");
    s.push_str("1. Fichiers livrés. Calculer l'empreinte SHA-256 d'un fichier de ce dossier (`shasum -a 256 fichier` ou `sha256sum fichier`) et la comparer à l'entrée correspondante de `files` dans `manifest.json`.\n\n");
    s.push_str("2. Authenticité des transcriptions. Pour chaque fichier source, `interviews[].sources[]` du manifeste donne `original_sha256`, l'empreinte du fichier d'origine tel que le chercheur le conserve. Si `replaced` vaut `false`, la copie livrée est identique à l'original octet pour octet et `delivered_sha256` est égal à `original_sha256`. Si `replaced` vaut `true`, l'examinateur demande au chercheur de présenter l'original, calcule son empreinte avec la même commande et la compare à `original_sha256` ; il peut aussi constater que le texte livré s'obtient en remplaçant dans l'original chaque nom du registre par son pseudonyme. La rubrique `replacements` du fichier indique, par pseudonyme, combien de remplacements ont été faits, sans donner les noms.\n\n");
    s.push_str("3. Intégrité depuis l'enregistrement. `recorded_sha256` est l'empreinte enregistrée par Coding Studio lorsque l'entretien a été ajouté à la bibliothèque. Si `intact` vaut `false`, le fichier a changé depuis.\n");
    s
}

/// Builds the verification package for `interview_ids` under `base`.
pub fn write_verification(
    store: &Store,
    interview_ids: &[String],
    base: &Path,
    actor: &str,
) -> Result<VerificationExport> {
    if interview_ids.is_empty() {
        return Err(CsError::Validation("no interview chosen".into()).into());
    }
    let mut ids: Vec<String> = Vec::new();
    for id in interview_ids {
        store
            .interview(id)?
            .ok_or_else(|| CsError::NotFound(format!("interview {id}")))?;
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids.sort();

    let eff = store.effective_pseudonyms()?;
    let entries = eff.config.entries.clone();
    let rules = Pseudonymiser::new(&entries);
    let digest = eff.config.digest();
    let mut counts: Vec<usize> = Vec::new();

    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, "verification");
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut out = Out {
        dir: dir.clone(),
        files: Vec::new(),
    };
    let mut warnings: Vec<String> = Vec::new();
    let mut text_files: Vec<(String, String)> = Vec::new();

    let mut interviews_json: Vec<Value> = Vec::new();
    for id in &ids {
        let entry = store
            .interview(id)?
            .ok_or_else(|| CsError::NotFound(format!("interview {id}")))?;
        let mut sources_json: Vec<Value> = Vec::new();
        for s in &entry.sources {
            let path = Path::new(&entry.folder_path).join(&s.rel_path);
            let mut item = json!({
                "rel_path": s.rel_path,
                "kind": s.kind,
                "recorded_sha256": s.sha256,
                "size": s.size,
            });
            if !is_transcript(s.kind) {
                item["delivered"] = json!(false);
                item["reason"] = json!("not a transcript: a working file of the researcher");
                sources_json.push(item);
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    warnings.push(format!("{id}/{}: cannot be read ({e})", s.rel_path));
                    item["delivered"] = json!(false);
                    item["reason"] = json!("the file could not be read");
                    sources_json.push(item);
                    continue;
                }
            };
            let original = sha256_hex(&bytes);
            item["original_sha256"] = json!(original);
            item["original_size"] = json!(bytes.len());
            item["intact"] = json!(original == s.sha256);
            if original != s.sha256 {
                warnings.push(format!(
                    "{id}/{}: the file changed since it was added to the library",
                    s.rel_path
                ));
            }
            let name = format!("interviews/{id}/original/{}", s.rel_path);
            match String::from_utf8(bytes.clone()) {
                Err(_) => {
                    item["delivered"] = json!(false);
                    item["reason"] = json!(
                        "not a text file: its text cannot be replaced without changing it, so it is listed with the digest of the original only"
                    );
                }
                Ok(text) => {
                    let mut file_counts: Vec<usize> = Vec::new();
                    let masked = rules.mask_counted(&text, &mut file_counts);
                    let replaced = masked != text;
                    if rules.contains_original(&masked) {
                        anyhow::bail!(
                            "a listed name is still present in {id}/{}; the export is refused",
                            s.rel_path
                        );
                    }
                    let delivered = if replaced {
                        masked.as_bytes().to_vec()
                    } else {
                        bytes
                    };
                    let f = out.put(&name, &delivered)?;
                    if counts.len() < file_counts.len() {
                        counts.resize(file_counts.len(), 0);
                    }
                    for (i, n) in file_counts.iter().enumerate() {
                        counts[i] += n;
                    }
                    item["delivered"] = json!(true);
                    item["delivered_as"] = json!(name);
                    item["delivered_sha256"] = json!(f.sha256);
                    item["replaced"] = json!(replaced);
                    item["replacements"] =
                        json!(replacements_by_pseudonym(&rules, &file_counts, &entries));
                }
            }
            sources_json.push(item);
        }
        let mut coded =
            json!({"delivered": false, "reason": "the interview has no segmentation yet"});
        if let Some(md) = coded_transcript(store, id)? {
            let mut file_counts: Vec<usize> = Vec::new();
            let masked = rules.mask_counted(&md, &mut file_counts);
            if counts.len() < file_counts.len() {
                counts.resize(file_counts.len(), 0);
            }
            for (i, n) in file_counts.iter().enumerate() {
                counts[i] += n;
            }
            let name = format!("interviews/{id}/transcript_coded.md");
            text_files.push((name.clone(), masked.clone()));
            let f = out.put(&name, masked.as_bytes())?;
            coded = json!({
                "delivered": true,
                "delivered_as": name,
                "delivered_sha256": f.sha256,
                "replaced": masked != md,
            });
        }
        interviews_json.push(json!({
            "interview_id": id,
            "folder_name": entry.folder_name,
            "primary_source": entry.primary_source,
            "sources": sources_json,
            "coded_transcript": coded,
        }));
    }

    // The register, in its public form, through the same list.
    let register = crate::export_persons::persons_csv_public(store, Some(&ids))?;
    let register = rules.mask(&register);
    let n_register = register.lines().count().saturating_sub(1);
    out.put("persons.csv", register.as_bytes())?;
    let groups = rules.mask(&crate::export_persons::groups_csv(store)?);
    out.put("groups.csv", groups.as_bytes())?;
    for (name, text) in &text_files {
        if rules.contains_original(text) {
            anyhow::bail!("a listed name is still present in {name}; the export is refused");
        }
    }

    let mut replacements: BTreeMap<String, usize> = BTreeMap::new();
    for (o, n) in rules.originals().iter().zip(&counts) {
        if let Some(e) = entries.iter().find(|e| e.original == *o) {
            *replacements.entry(e.kind.clone()).or_default() += n;
        }
    }
    out.put(
        "README.md",
        readme(&ids, &replacements, entries.len(), n_register).as_bytes(),
    )?;
    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "kind": "verification",
        "generated_at": crate::now(),
        "generated_by": actor,
        "interview_ids": ids,
        "anonymisation": {
            "pseudonymisation_digest": digest,
            "n_entries": entries.len(),
            "replacements": replacements,
            "free_text_covered": false,
            "note": cs_model::pseudonym::FREE_TEXT_NOTE,
        },
        "interviews": interviews_json,
        "warnings": warnings,
        "files": out.files,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_sha = sha256_hex(&manifest_bytes);
    let path = dir.join("manifest.json");
    fs::write(&path, &manifest_bytes).with_context(|| format!("writing {}", path.display()))?;
    store.log(
        actor,
        "export.verification",
        None,
        Some(&dir.display().to_string()),
        json!({
            "dir": dir.display().to_string(),
            "interview_ids": ids,
            "manifest_sha256": manifest_sha,
            "pseudonymisation_digest": digest,
            "replacements": replacements,
            "n_files": out.files.len() + 1,
        }),
    )?;
    Ok(VerificationExport {
        dir: dir.display().to_string(),
        files: out.files,
        manifest_sha256: manifest_sha,
        interview_ids: ids,
        replacements,
        pseudonymisation_digest: digest,
        warnings,
    })
}

#[derive(Deserialize)]
pub(crate) struct VerificationBody {
    /// Interview ids; absent or empty with `all` true: every interview.
    #[serde(default)]
    interview_ids: Vec<String>,
    #[serde(default)]
    all: bool,
    /// Folder the package is written under; absent, the exports folder.
    #[serde(default)]
    out_dir: Option<String>,
}

/// `POST /api/export/verification`.
pub(crate) async fn export_verification_route(
    State(st): State<AppState>,
    Json(body): Json<VerificationBody>,
) -> ApiResult {
    let store = st.store();
    let ids: Vec<String> = if body.all {
        store
            .summaries()?
            .into_iter()
            .map(|s| s.interview_id)
            .collect()
    } else {
        body.interview_ids
    };
    let base = body
        .out_dir
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::v2::exports_base(&st.db_path));
    let r = write_verification(&store, &ids, &base, &st.actor)?;
    Ok(Json(json!(r)))
}
