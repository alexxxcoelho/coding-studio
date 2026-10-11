//! Audit package: everything needed to reconstruct the coding of one
//! interview, written to a new folder that is never overwritten, with a
//! manifest of SHA-256 hashes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cs_model::{sha256_hex, CsError, HumanCoding};
use cs_store::Store;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct ExportedFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub dir: String,
    pub files: Vec<ExportedFile>,
    pub manifest_sha256: String,
    pub final_complete: bool,
    pub pending_units: Vec<String>,
}

struct Writer {
    dir: PathBuf,
    files: Vec<ExportedFile>,
}

impl Writer {
    fn write(&mut self, name: &str, content: &[u8]) -> Result<()> {
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

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn human_full_csv(
    order: &[String],
    codings: &std::collections::BTreeMap<String, HumanCoding>,
) -> String {
    let mut out = String::from("unit_id,codes,subcode,modifier,ev_level,provenance,memo,excerpt\n");
    for unit_id in order {
        let Some(c) = codings.get(unit_id) else {
            continue;
        };
        let codes = c.codes.iter().cloned().collect::<Vec<_>>().join(";");
        let opt = |v: &Option<String>| csv_field(v.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_field(unit_id),
            csv_field(&codes),
            opt(&c.subcode),
            opt(&c.modifier),
            opt(&c.ev_level),
            opt(&c.provenance),
            opt(&c.memo),
            opt(&c.excerpt),
        ));
    }
    out
}

/// Picks `<base>/<YYYY-MM-DD>_<ID>`, or a time-suffixed sibling when it exists.
fn target_dir(base: &Path, interview_id: &str) -> PathBuf {
    let now = chrono::Local::now();
    let first = base.join(format!("{}_{}", now.format("%Y-%m-%d"), interview_id));
    if !first.exists() {
        return first;
    }
    let stamped = base.join(format!(
        "{}_{}_{}",
        now.format("%Y-%m-%d"),
        interview_id,
        now.format("%H%M%S")
    ));
    if !stamped.exists() {
        return stamped;
    }
    let mut n = 2;
    loop {
        let candidate = base.join(format!(
            "{}_{}_{}_{n}",
            now.format("%Y-%m-%d"),
            interview_id,
            now.format("%H%M%S")
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
    let order: Vec<String> = seg.units.iter().map(|u| u.unit_id.clone()).collect();
    let human_full = store.human_codings(interview_id)?;
    let human = store.human_coding_map(interview_id)?;
    let runs = store.ai_runs(interview_id)?;
    let complete: Vec<(String, cs_model::CodingMap)> = runs
        .iter()
        .filter(|r| r.complete)
        .map(|r| (r.meta.run_label.clone(), r.codings.clone()))
        .collect();
    let arbitrations = store.arbitrations(interview_id)?;
    let events = store.audit_events(Some(interview_id))?;
    let chain = store.verify_chain()?;

    fs::create_dir_all(base).with_context(|| format!("creating {}", base.display()))?;
    let dir = target_dir(base, interview_id);
    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut w = Writer {
        dir: dir.clone(),
        files: Vec::new(),
    };

    w.write("units.csv", cs_ai::units_csv(&seg).as_bytes())?;
    w.write(
        "coding_human.csv",
        cs_ai::coding_csv(&human, &order).as_bytes(),
    )?;
    w.write(
        "human_full.csv",
        human_full_csv(&order, &human_full).as_bytes(),
    )?;

    let mut seen_prompts = BTreeSet::new();
    for p in store.prompts(interview_id)? {
        if !seen_prompts.insert(p.sha256.clone()) {
            continue;
        }
        let name = format!("prompt_part{}_of{}_{}.txt", p.part, p.of, &p.sha256[..8]);
        w.write(&name, p.text.as_bytes())?;
    }

    for run in &runs {
        let tag = format!("run{}_{}", run.run_id, safe(&run.meta.run_label));
        let meta = json!({
            "run_id": run.run_id,
            "meta": run.meta,
            "parts_received": run.parts_received,
            "parts_expected": run.parts_expected,
            "complete": run.complete,
        });
        w.write(
            &format!("{tag}.json"),
            serde_json::to_string_pretty(&meta)?.as_bytes(),
        )?;
        w.write(
            &format!("coding_ai_{tag}.csv"),
            cs_ai::coding_csv(&run.codings, &order).as_bytes(),
        )?;
        for (part, raw, _sha, _at) in store.ai_raw_outputs(run.run_id)? {
            w.write(&format!("ai_{tag}_part{part}_raw.txt"), raw.as_bytes())?;
        }
    }

    let divergences = if complete.is_empty() {
        Vec::new()
    } else {
        cs_ai::divergences(&seg, &human, &complete)
    };
    if !complete.is_empty() {
        let mut reports = Vec::new();
        for (label, codings) in &complete {
            reports.push(cs_ai::agreement(
                "Human",
                &human,
                &format!("AI {label}"),
                codings,
                &order,
            ));
        }
        for (i, (la, a)) in complete.iter().enumerate() {
            for (lb, b) in complete.iter().skip(i + 1) {
                reports.push(cs_ai::agreement(
                    &format!("AI {la}"),
                    a,
                    &format!("AI {lb}"),
                    b,
                    &order,
                ));
            }
        }
        w.write(
            "agreement.md",
            cs_ai::render_agreement_md(&seg, &reports).as_bytes(),
        )?;
        w.write(
            "adjudication_log.md",
            cs_ai::render_adjudication_md(&seg, &divergences, &arbitrations).as_bytes(),
        )?;
    }

    let (final_complete, pending) = if complete.is_empty() {
        (false, Vec::new())
    } else {
        match cs_ai::final_coding(&seg, &human, &complete, &arbitrations) {
            Ok(finals) => {
                w.write("coding_final.csv", cs_ai::final_csv(&finals).as_bytes())?;
                (true, Vec::new())
            }
            Err(pending) => (false, pending),
        }
    };

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

    let readme = readme(
        interview_id,
        &entry.folder_name,
        &seg,
        runs.len(),
        complete.len(),
        divergences.len(),
        final_complete,
        &pending,
    );
    w.write("README.md", readme.as_bytes())?;

    let manifest = json!({
        "tool": "Coding Studio",
        "version": crate::VERSION,
        "generated_at": crate::now(),
        "generated_by": actor,
        "interview_id": interview_id,
        "db_path": db_path,
        "segmentation_hash": seg.hash,
        "source": { "path": seg.source_rel_path, "sha256": seg.source_sha256 },
        "audit_chain_ok": chain.ok,
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
            "n_files": w.files.len(),
            "final_complete": final_complete,
            "pending_units": pending,
        }),
    )?;

    Ok(ExportResult {
        dir: dir.display().to_string(),
        files: w.files,
        manifest_sha256: manifest_sha,
        final_complete,
        pending_units: pending,
    })
}

#[allow(clippy::too_many_arguments)]
fn readme(
    id: &str,
    folder: &str,
    seg: &cs_model::Segmentation,
    n_runs: usize,
    n_complete: usize,
    n_div: usize,
    final_complete: bool,
    pending: &[String],
) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Coding audit package, {id}\n\n"));
    s.push_str(&format!(
        "Interview folder: {folder}. Segmented source: {} (SHA-256 {}). Segmentation hash: {}. Rule: {}. Units: {}.\n\n",
        seg.source_rel_path, seg.source_sha256, seg.hash, seg.rule, seg.units.len()
    ));
    s.push_str(&format!(
        "AI runs recorded: {n_runs}, complete: {n_complete}. Divergences between the human coding and the complete runs: {n_div}.\n\n"
    ));
    if final_complete {
        s.push_str("Every divergence has been arbitrated; coding_final.csv is present.\n\n");
    } else if n_complete == 0 {
        s.push_str("No complete AI run yet, so no agreement, adjudication or final coding is included.\n\n");
    } else {
        s.push_str(&format!(
            "The final coding is not included because {} unit(s) still need a decision: {}.\n\n",
            pending.len(),
            pending.join(", ")
        ));
    }
    s.push_str("## Files\n\n");
    for (name, what) in [
        ("units.csv", "The units of analysis, `unit_id,speaker,timestamp,text`, as produced by the deterministic segmentation."),
        ("coding_human.csv", "The candidate's primary coding, `unit_id,codes`, codes separated by `;`, NA when no code applies."),
        ("human_full.csv", "The same coding with every field: sub-code, modifier, evidence level, provenance, memo and verbatim excerpt."),
        ("prompt_part<p>_of<n>_<hash>.txt", "Each prompt generated for the AI coder, byte for byte; the hash prefix is the start of its SHA-256."),
        ("run<id>_<label>.json", "Parameters of each AI run: model, version, temperature, seed, context length, batch size, prompt version, execution time."),
        ("coding_ai_run<id>_<label>.csv", "Codes per unit returned by that run, after validation, in the format of coding_human.csv."),
        ("ai_run<id>_<label>_part<p>_raw.txt", "The AI answer exactly as pasted, before any parsing."),
        ("agreement.md", "Exact agreement and Cohen's kappa per code, human against each run and runs against each other."),
        ("adjudication_log.md", "Every divergence with the unit text, the competing codes, the decision and its rationale."),
        ("coding_final.csv", "Final coding: agreed units keep the common codes, divergent units take the arbitrated codes (`basis`)."),
        ("audit_log.jsonl", "Every recorded action on this interview, one JSON event per line, each chained to the previous one by SHA-256."),
        ("audit_chain.json", "Result of the verification of the whole audit chain at export time."),
        ("manifest.json", "SHA-256 and size of every file above, with the tool version and the export time."),
    ] {
        s.push_str(&format!("- `{name}`: {what}\n"));
    }
    s.push_str("\nTo check the package, recompute the SHA-256 of each file and compare it with manifest.json.\n");
    s
}
