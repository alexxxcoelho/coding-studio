//! The AI challenge of phase 2: prompt batches, runs, answers, the agreement
//! samples, the review decisions and the final coding.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use cs_model::phase::is_phase2_code;
use cs_model::reconcile::{self, agreements, sample_agreements, sample_seed};
use cs_model::{
    check_code_set, sha256_hex, AiOutput, AiRun, AiRunMeta, Arbitration, ArbitrationChoice,
    CodeSet, CodingMap, FinalCoding, HumanSnapshot, PhaseSummary, PromptPart, ReviewItem,
    ReviewKind, ReviewSnapshotRef, NA, PHASE_1, PHASE_2,
};
use rusqlite::{params, OptionalExtension};
use serde_json::json;

use crate::{
    append_event, check_phase, clean_codes, clean_opt, from_json, guard_err, not_found, now,
    to_json, validation, Store,
};

/// One entry of the review list: the item and every decision ever taken on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReviewEntry {
    #[serde(flatten)]
    pub item: ReviewItem,
    pub history: Vec<Arbitration>,
}

/// What the candidate reviews: disagreements first, then the sampled agreements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Review {
    /// Labels of the complete runs.
    pub runs: Vec<String>,
    pub items: Vec<ReviewEntry>,
    pub n_disagreements: usize,
    pub n_sampled: usize,
    pub n_decided: usize,
    /// (run_label, seed) of every agreement sample.
    pub seeds: Vec<(String, String)>,
    /// The frozen human reference of each complete run, in run order.
    pub snapshots: Vec<ReviewSnapshotRef>,
}

fn only_phase2(phase: u8) -> Result<()> {
    check_phase(phase)?;
    if phase == PHASE_1 {
        return Err(validation(
            "phase 1 is a provisional pre-classification: it has no AI pass and no review",
        ));
    }
    Ok(())
}

impl Store {
    // ------------------------------------------------------------- prompts

    /// Records the parts of a new prompt batch (append-only) and returns them
    /// with their batch id.
    pub fn record_prompts(&self, parts: &[PromptPart], actor: &str) -> Result<Vec<PromptPart>> {
        let first = parts.first().ok_or_else(|| validation("no prompt part"))?;
        let interview_id = first.interview_id.clone();
        self.require_interview(&interview_id)?;
        for p in parts {
            only_phase2(p.phase)?;
            if p.interview_id != interview_id {
                return Err(validation("prompt parts of different interviews"));
            }
            if sha256_hex(p.text.as_bytes()) != p.sha256 {
                return Err(validation("prompt sha256 does not match its text"));
            }
        }
        // Every unit of the batch must be coded by the candidate: the human
        // reference is frozen from these codings.
        let batch_units: BTreeSet<String> = parts
            .iter()
            .flat_map(|p| p.unit_ids.iter().cloned())
            .collect();
        let coded = self.coded_units(&interview_id, PHASE_2)?;
        let missing: Vec<&String> = batch_units.iter().filter(|u| !coded.contains(*u)).collect();
        if !missing.is_empty() {
            return Err(validation(format!(
                "{} units of the batch are not yet coded by the candidate in phase 2: {}",
                missing.len(),
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let human = self.human_coding_map(&interview_id, PHASE_2)?;
        let codings: CodingMap = batch_units
            .iter()
            .map(|u| (u.clone(), human.get(u).cloned().unwrap_or_default()))
            .collect();
        let codings_json = to_json(&codings);
        let snapshot_sha = sha256_hex(codings_json.as_bytes());

        let tx = self.conn.unchecked_transaction()?;
        let batch_id: i64 = tx.query_row(
            "SELECT COALESCE(MAX(batch_id), 0) + 1 FROM prompts WHERE interview_id = ?1",
            [&interview_id],
            |r| r.get(0),
        )?;
        let mut out = Vec::new();
        for p in parts {
            let mut p = p.clone();
            p.batch_id = batch_id;
            tx.execute(
                "INSERT INTO prompts (interview_id, batch_id, phase, part, of, unit_ids, codes, segmentation_hash,
                   codebook_hash, prompt_version, text, sha256, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    p.interview_id,
                    batch_id,
                    p.phase,
                    p.part,
                    p.of,
                    to_json(&p.unit_ids),
                    to_json(&p.codes),
                    p.segmentation_hash,
                    p.codebook_hash,
                    p.prompt_version,
                    p.text,
                    p.sha256,
                    now()
                ],
            )?;
            append_event(
                &tx,
                actor,
                "prompt.generate",
                Some(&p.interview_id),
                Some(&format!("batch {batch_id} part {}/{}", p.part, p.of)),
                json!({
                    "phase": p.phase,
                    "batch_id": batch_id,
                    "part": p.part,
                    "of": p.of,
                    "sha256": p.sha256,
                    "codebook_hash": p.codebook_hash,
                    "segmentation_hash": p.segmentation_hash,
                    "prompt_version": p.prompt_version,
                    "n_units": p.unit_ids.len(),
                    "codes": p.codes,
                }),
            )?;
            out.push(p);
        }
        let first = &out[0];
        tx.execute(
            "INSERT INTO human_snapshots (interview_id, batch_id, phase, segmentation_hash, codebook_hash,
               codings, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                interview_id,
                batch_id,
                PHASE_2,
                first.segmentation_hash,
                first.codebook_hash,
                codings_json,
                snapshot_sha,
                now()
            ],
        )?;
        append_event(
            &tx,
            actor,
            "human_snapshot.take",
            Some(&interview_id),
            Some(&format!("batch {batch_id}")),
            json!({
                "batch_id": batch_id,
                "sha256": snapshot_sha,
                "n_units": batch_units.len(),
                "segmentation_hash": first.segmentation_hash,
                "codebook_hash": first.codebook_hash,
            }),
        )?;
        tx.commit()?;
        Ok(out)
    }

    /// The frozen human references of an interview, ordered by batch.
    pub fn human_snapshots(&self, interview_id: &str) -> Result<Vec<HumanSnapshot>> {
        self.snapshot_rows(interview_id, None)
    }

    /// The frozen human reference of one prompt batch.
    pub fn human_snapshot(&self, interview_id: &str, batch_id: i64) -> Result<HumanSnapshot> {
        self.snapshot_rows(interview_id, Some(batch_id))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                not_found(format!(
                    "human snapshot of batch {batch_id} of {interview_id}"
                ))
            })
    }

    fn snapshot_rows(
        &self,
        interview_id: &str,
        batch_id: Option<i64>,
    ) -> Result<Vec<HumanSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT snapshot_id, interview_id, batch_id, phase, segmentation_hash, codebook_hash, codings,
                    sha256, created_at
             FROM human_snapshots WHERE interview_id = ?1 AND (?2 IS NULL OR batch_id = ?2)
             ORDER BY batch_id",
        )?;
        let rows = stmt.query_map(params![interview_id, batch_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, u8>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (snapshot_id, interview_id, batch_id, phase, sh, ch, codings, sha, created_at) =
                row?;
            out.push(HumanSnapshot {
                snapshot_id,
                interview_id,
                batch_id,
                phase,
                segmentation_hash: sh,
                codebook_hash: ch,
                codings: from_json(&codings)?,
                sha256: sha,
                created_at,
            });
        }
        Ok(out)
    }

    /// Every prompt part ever generated for the interview, oldest first.
    pub fn prompts(&self, interview_id: &str) -> Result<Vec<PromptPart>> {
        let mut stmt = self.conn.prepare(
            "SELECT interview_id, phase, batch_id, codes, part, of, unit_ids, segmentation_hash, codebook_hash,
                    prompt_version, text, sha256
             FROM prompts WHERE interview_id = ?1 ORDER BY prompt_id",
        )?;
        let rows = stmt.query_map([interview_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u8>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, u32>(4)?,
                r.get::<_, u32>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, String>(11)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (interview_id, phase, batch_id, codes, part, of, unit_ids, sh, ch, pv, text, sha) =
                row?;
            out.push(PromptPart {
                interview_id,
                phase,
                batch_id,
                codes: from_json(&codes)?,
                part,
                of,
                unit_ids: from_json(&unit_ids)?,
                segmentation_hash: sh,
                codebook_hash: ch,
                prompt_version: pv,
                text,
                sha256: sha,
            });
        }
        Ok(out)
    }

    fn batch_parts(&self, interview_id: &str, batch_id: i64) -> Result<Vec<PromptPart>> {
        let parts: Vec<PromptPart> = self
            .prompts(interview_id)?
            .into_iter()
            .filter(|p| p.batch_id == batch_id)
            .collect();
        if parts.is_empty() {
            return Err(not_found(format!(
                "prompt batch {batch_id} of {interview_id}"
            )));
        }
        Ok(parts)
    }

    // ---------------------------------------------------------------- runs

    /// Declares an AI run answering a prompt batch (the latest when
    /// `meta.batch_id` is empty). Run labels are unique per interview.
    pub fn create_ai_run(&self, interview_id: &str, meta: &AiRunMeta, actor: &str) -> Result<i64> {
        self.require_interview(interview_id)?;
        let latest: Option<i64> = self.conn.query_row(
            "SELECT MAX(batch_id) FROM prompts WHERE interview_id = ?1",
            [interview_id],
            |r| r.get(0),
        )?;
        let batch_id = match meta.batch_id {
            Some(b) => {
                self.batch_parts(interview_id, b)?;
                b
            }
            None => latest
                .ok_or_else(|| validation("generate the prompts before declaring an AI run"))?,
        };
        let mut meta = meta.clone();
        meta.batch_id = Some(batch_id);
        meta.run_label = meta.run_label.trim().to_string();
        meta.model_name = meta.model_name.trim().to_string();
        meta.model_version = meta.model_version.trim().to_string();
        meta.prompt_id_version = meta.prompt_id_version.trim().to_string();
        meta.executed_at = meta.executed_at.trim().to_string();
        meta.seed = clean_opt(&meta.seed);
        meta.notes = clean_opt(&meta.notes);
        if meta.run_label.is_empty() {
            return Err(validation("run label is required (R1, R2, R3...)"));
        }
        if meta.model_name.is_empty() {
            return Err(validation("model name is required"));
        }
        if meta.prompt_id_version.is_empty() {
            return Err(validation("prompt id and version are required"));
        }
        let taken: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ai_runs WHERE interview_id = ?1 AND run_label = ?2",
            params![interview_id, meta.run_label],
            |r| r.get(0),
        )?;
        if taken > 0 {
            return Err(validation(format!(
                "run label {} is already used for {interview_id}",
                meta.run_label
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO ai_runs (interview_id, batch_id, run_label, model_name, model_version, temperature, seed,
               context_length, batch_size, prompt_id_version, executed_at, notes, created_at, provider)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                interview_id,
                batch_id,
                meta.run_label,
                meta.model_name,
                meta.model_version,
                meta.temperature,
                meta.seed,
                meta.context_length,
                meta.batch_size,
                meta.prompt_id_version,
                meta.executed_at,
                meta.notes,
                now(),
                meta.provider.as_ref().map(to_json)
            ],
        )?;
        let run_id = tx.last_insert_rowid();
        append_event(
            &tx,
            actor,
            "ai.run.create",
            Some(interview_id),
            Some(&meta.run_label),
            json!({"run_id": run_id, "phase": PHASE_2, "batch_id": batch_id, "meta": meta}),
        )?;
        tx.commit()?;
        Ok(run_id)
    }

    /// (interview_id, batch_id, meta) of a run.
    fn run_row(&self, run_id: i64) -> Result<(String, i64, AiRunMeta)> {
        let (interview_id, batch_id, meta, provider) = self
            .conn
            .query_row(
                "SELECT interview_id, batch_id, run_label, model_name, model_version, temperature, seed,
                   context_length, batch_size, prompt_id_version, executed_at, notes, provider
                 FROM ai_runs WHERE run_id = ?1",
                [run_id],
                |r| {
                    let batch_id: i64 = r.get(1)?;
                    let provider: Option<String> = r.get(12)?;
                    Ok((
                        r.get::<_, String>(0)?,
                        batch_id,
                        AiRunMeta {
                            run_label: r.get(2)?,
                            model_name: r.get(3)?,
                            model_version: r.get(4)?,
                            temperature: r.get(5)?,
                            seed: r.get(6)?,
                            context_length: r.get(7)?,
                            batch_size: r.get(8)?,
                            prompt_id_version: r.get(9)?,
                            executed_at: r.get(10)?,
                            notes: r.get(11)?,
                            batch_id: Some(batch_id),
                            provider: None,
                        },
                        provider,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| not_found(format!("AI run {run_id}")))?;
        let mut meta = meta;
        meta.provider = provider.map(|p| from_json(&p)).transpose()?;
        Ok((interview_id, batch_id, meta))
    }

    /// The prompt part an answer refers to: (part, units, codes) of the run's batch.
    pub fn run_part(&self, run_id: i64, part: u32) -> Result<PromptPart> {
        let (interview_id, batch_id, meta) = self.run_row(run_id)?;
        self.batch_parts(&interview_id, batch_id)?
            .into_iter()
            .find(|p| p.part == part)
            .ok_or_else(|| {
                validation(format!(
                    "batch {batch_id} of run {} has no part {part}",
                    meta.run_label
                ))
            })
    }

    /// Stores the raw answer verbatim (and its hash) with the parsed codings.
    /// Checked against the prompt part it answers (its units and code
    /// snapshot). Blind guard: every unit of the part must already carry the
    /// candidate's phase 2 coding. When the run becomes complete, its agreement
    /// sample is drawn and stored in the same transaction.
    pub fn record_ai_output(
        &self,
        run_id: i64,
        raw: &str,
        parsed: &AiOutput,
        actor: &str,
    ) -> Result<()> {
        let (interview_id, batch_id, meta) = self.run_row(run_id)?;
        only_phase2(parsed.phase)?;
        if parsed.interview_id != interview_id {
            return Err(validation(format!(
                "answer is for {} but run {} belongs to {interview_id}",
                parsed.interview_id, meta.run_label
            )));
        }
        let part = self.run_part(run_id, parsed.part)?;
        if parsed.of != part.of {
            return Err(validation(format!(
                "batch {batch_id} has {} parts, this answer says {}",
                part.of, parsed.of
            )));
        }
        let seg = self
            .segmentation(&interview_id)?
            .ok_or_else(|| validation("no segmentation"))?;
        if parsed.segmentation_hash != part.segmentation_hash || part.segmentation_hash != seg.hash
        {
            return Err(guard_err(format!(
                "answer was produced on another segmentation (hash {}), current is {}",
                parsed.segmentation_hash, seg.hash
            )));
        }
        let coded = self.coded_units(&interview_id, PHASE_2)?;
        let missing: Vec<&String> = part
            .unit_ids
            .iter()
            .filter(|u| !coded.contains(*u))
            .collect();
        if !missing.is_empty() {
            return Err(guard_err(format!(
                "blind coding guard: {} units of this part are not yet coded by the candidate ({})",
                missing.len(),
                missing
                    .iter()
                    .take(5)
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let received: Vec<u32> = {
            let mut stmt = self
                .conn
                .prepare("SELECT part FROM ai_outputs WHERE run_id = ?1")?;
            let rows = stmt.query_map([run_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if received.contains(&parsed.part) {
            return Err(validation(format!(
                "part {} of run {} was already imported",
                parsed.part, meta.run_label
            )));
        }
        let expected: BTreeSet<&String> = part.unit_ids.iter().collect();
        let got: BTreeSet<&String> = parsed.codings.keys().collect();
        if expected != got {
            return Err(validation(format!(
                "answer does not cover exactly the {} units of part {}",
                expected.len(),
                parsed.part
            )));
        }
        let offered: BTreeSet<&String> = part.codes.iter().collect();
        for (unit, codes) in &parsed.codings {
            let mut issues = check_code_set(codes);
            for c in codes {
                if c != NA && !offered.contains(c) {
                    issues.push(format!("code {c} was not offered in this prompt"));
                }
            }
            if !issues.is_empty() {
                return Err(validation(format!("{unit}: {}", issues.join("; "))));
            }
        }
        let raw_sha = sha256_hex(raw.as_bytes());
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO ai_outputs (run_id, part, of, raw_text, raw_sha256, segmentation_hash, parsed, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![run_id, parsed.part, parsed.of, raw, raw_sha, parsed.segmentation_hash, to_json(parsed), now()],
        )?;
        append_event(
            &tx,
            actor,
            "ai.output.import",
            Some(&interview_id),
            Some(&meta.run_label),
            json!({
                "phase": PHASE_2,
                "run_id": run_id,
                "run_label": meta.run_label,
                "batch_id": batch_id,
                "part": parsed.part,
                "of": parsed.of,
                "raw_sha256": raw_sha,
                "n_units": parsed.codings.len(),
                "segmentation_hash": parsed.segmentation_hash,
            }),
        )?;
        // Complete? Then draw the agreement sample.
        let run = self.run_with(&tx, run_id)?;
        if run.complete {
            let snapshot = self.human_snapshot(&interview_id, batch_id)?;
            let agreeing = agreements(&run.unit_ids, &snapshot.codings, &run.codings);
            let seed = sample_seed(run_id, &seg.hash);
            let sample = sample_agreements(&seed, &agreeing);
            tx.execute(
                "INSERT INTO review_samples (run_id, seed, unit_ids, n_agreeing, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![run_id, seed, to_json(&sample), agreeing.len() as i64, now()],
            )?;
            append_event(
                &tx,
                actor,
                "review.sample",
                Some(&interview_id),
                Some(&meta.run_label),
                json!({
                    "run_id": run_id,
                    "seed": seed,
                    "n_agreeing": agreeing.len(),
                    "sample": sample,
                    "rule": "max(5, ceil(10%)) of the agreeing units, ordered by sha256(seed + unit_id)",
                }),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn run_with(&self, conn: &rusqlite::Connection, run_id: i64) -> Result<AiRun> {
        let (interview_id, batch_id, meta) = self.run_row(run_id)?;
        let parts = self.batch_parts(&interview_id, batch_id)?;
        let unit_ids: Vec<String> = parts.iter().flat_map(|p| p.unit_ids.clone()).collect();
        let mut stmt = conn.prepare(
            "SELECT part, of, parsed FROM ai_outputs WHERE run_id = ?1 ORDER BY output_id",
        )?;
        let rows: Vec<(u32, u32, String)> = stmt
            .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut codings = CodingMap::new();
        let mut received = Vec::new();
        for (part, _, parsed) in rows {
            let o: AiOutput = from_json(&parsed)?;
            codings.extend(o.codings);
            received.push(part);
        }
        received.sort_unstable();
        let complete = !unit_ids.is_empty() && unit_ids.iter().all(|u| codings.contains_key(u));
        Ok(AiRun {
            run_id,
            interview_id,
            phase: PHASE_2,
            meta,
            batch_id,
            parts_expected: parts.first().map(|p| p.of),
            unit_ids,
            codings,
            parts_received: received,
            complete,
        })
    }

    /// AI runs of an interview, oldest first.
    pub fn ai_runs(&self, interview_id: &str) -> Result<Vec<AiRun>> {
        let ids: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT run_id FROM ai_runs WHERE interview_id = ?1 ORDER BY run_id")?;
            let rows = stmt.query_map([interview_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        ids.into_iter()
            .map(|id| self.run_with(&self.conn, id))
            .collect()
    }

    pub fn ai_run(&self, run_id: i64) -> Result<AiRun> {
        self.run_with(&self.conn, run_id)
    }

    /// Raw answers of a run, in the order received: (part, raw_text, sha256, imported_at).
    pub fn ai_raw_outputs(&self, run_id: i64) -> Result<Vec<(u32, String, String, String)>> {
        self.run_row(run_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT part, raw_text, raw_sha256, imported_at FROM ai_outputs WHERE run_id = ?1 ORDER BY output_id",
        )?;
        let rows = stmt.query_map([run_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// (run_label, codings) of the complete runs.
    pub fn complete_runs(&self, interview_id: &str) -> Result<Vec<(String, CodingMap)>> {
        Ok(self
            .ai_runs(interview_id)?
            .into_iter()
            .filter(|r| r.complete)
            .map(|r| (r.meta.run_label, r.codings))
            .collect())
    }

    /// Agreement samples of the complete runs: (run_label, seed, unit_ids).
    pub fn review_samples(&self, interview_id: &str) -> Result<Vec<(String, String, Vec<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.run_label, s.seed, s.unit_ids FROM review_samples s JOIN ai_runs r USING (run_id)
             WHERE r.interview_id = ?1 ORDER BY s.run_id",
        )?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map([interview_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .map(|(l, s, u)| Ok((l, s, from_json(&u)?)))
            .collect()
    }

    // -------------------------------------------------------------- review

    /// Units compared with the AI, in segmentation order: those of the complete runs' batches.
    fn compared_units(&self, interview_id: &str) -> Result<Vec<String>> {
        let in_runs: BTreeSet<String> = self
            .ai_runs(interview_id)?
            .into_iter()
            .filter(|r| r.complete)
            .flat_map(|r| r.unit_ids)
            .collect();
        Ok(self
            .segmentation(interview_id)?
            .map(|s| {
                s.units
                    .into_iter()
                    .map(|u| u.unit_id)
                    .filter(|u| in_runs.contains(u))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The review list: every disagreement with a complete run, then the
    /// sampled agreements (still agreeing), each with its current decision.
    pub fn review(&self, interview_id: &str) -> Result<Review> {
        self.require_interview(interview_id)?;
        let runs: Vec<AiRun> = self
            .ai_runs(interview_id)?
            .into_iter()
            .filter(|r| r.complete)
            .collect();
        let complete: Vec<(String, CodingMap)> = runs
            .iter()
            .map(|r| (r.meta.run_label.clone(), r.codings.clone()))
            .collect();
        let snapshots: Vec<HumanSnapshot> = runs
            .iter()
            .map(|r| self.human_snapshot(interview_id, r.batch_id))
            .collect::<Result<_>>()?;
        // The human reference of a unit is the snapshot of the latest batch
        // among the complete runs that cover it.
        let mut reference: BTreeMap<String, (i64, CodeSet)> = BTreeMap::new();
        for (run, snap) in runs.iter().zip(&snapshots) {
            for u in &run.unit_ids {
                let codes = snap.codings.get(u).cloned().unwrap_or_default();
                if reference.get(u).is_none_or(|(b, _)| *b <= snap.batch_id) {
                    reference.insert(u.clone(), (snap.batch_id, codes));
                }
            }
        }
        let human: CodingMap = reference
            .iter()
            .map(|(u, (_, codes))| (u.clone(), codes.clone()))
            .collect();
        let current = self.human_coding_map(interview_id, PHASE_2)?;
        let order = self.compared_units(interview_id)?;
        let decisions = self.decisions(interview_id)?;
        let history = self.decision_history(interview_id)?;
        let entry = |unit_id: &str, kind: ReviewKind, ai: BTreeMap<String, CodeSet>| {
            let frozen = human.get(unit_id).cloned().unwrap_or_default();
            let live = current.get(unit_id).cloned().unwrap_or_default();
            ReviewEntry {
                item: ReviewItem {
                    unit_id: unit_id.to_string(),
                    kind,
                    revised_since_snapshot: frozen != live,
                    human: frozen,
                    human_current: live,
                    ai,
                    decision: decisions.get(unit_id).cloned(),
                },
                history: history
                    .iter()
                    .filter(|d| d.unit_id == unit_id)
                    .cloned()
                    .collect(),
            }
        };
        let mut items: Vec<ReviewEntry> = reconcile::divergences(&order, &human, &complete)
            .into_iter()
            .map(|d| entry(&d.unit_id, ReviewKind::Disagreement, d.ai))
            .collect();
        let n_disagreements = items.len();
        let disagreeing: BTreeSet<String> = items.iter().map(|e| e.item.unit_id.clone()).collect();
        let samples = self.review_samples(interview_id)?;
        let complete_labels: BTreeSet<&String> = complete.iter().map(|(l, _)| l).collect();
        let sampled: BTreeSet<String> = samples
            .iter()
            .filter(|(l, _, _)| complete_labels.contains(l))
            .flat_map(|(_, _, u)| u.clone())
            .filter(|u| !disagreeing.contains(u))
            .collect();
        for u in order.iter().filter(|u| sampled.contains(*u)) {
            let ai = complete
                .iter()
                .filter_map(|(l, m)| m.get(u).map(|s| (l.clone(), s.clone())))
                .collect();
            items.push(entry(u, ReviewKind::AgreementSample, ai));
        }
        let n_sampled = items.len() - n_disagreements;
        let n_decided = items.iter().filter(|e| e.item.decision.is_some()).count();
        Ok(Review {
            runs: complete.iter().map(|(l, _)| l.clone()).collect(),
            items,
            n_disagreements,
            n_sampled,
            n_decided,
            seeds: samples.into_iter().map(|(l, s, _)| (l, s)).collect(),
            snapshots: runs
                .iter()
                .zip(&snapshots)
                .map(|(r, s)| ReviewSnapshotRef {
                    run_label: r.meta.run_label.clone(),
                    snapshot_id: s.snapshot_id,
                    sha256: s.sha256.clone(),
                })
                .collect(),
        })
    }

    /// Records a review decision on a disagreement (rationale required) or on
    /// a sampled agreement (note optional). Appended; the latest is current.
    pub fn record_decision(
        &self,
        interview_id: &str,
        arb: &Arbitration,
        actor: &str,
    ) -> Result<Arbitration> {
        self.require_interview(interview_id)?;
        self.current_unit(interview_id, &arb.unit_id)?;
        let review = self.review(interview_id)?;
        let item = review
            .items
            .iter()
            .find(|e| e.item.unit_id == arb.unit_id)
            .ok_or_else(|| {
                validation(format!(
                    "unit {} is neither a disagreement nor a sampled agreement",
                    arb.unit_id
                ))
            })?;
        let rationale = arb.rationale.trim();
        if item.item.kind == ReviewKind::Disagreement && rationale.is_empty() {
            return Err(validation(
                "a rationale is required to decide a disagreement",
            ));
        }
        let final_codes = clean_codes(&arb.final_codes);
        if let Some(issue) = check_code_set(&final_codes).into_iter().next() {
            return Err(validation(issue));
        }
        for c in final_codes.iter().filter(|c| c.as_str() != NA) {
            if !self.code_row(c)?.as_ref().is_some_and(is_phase2_code) {
                return Err(validation(format!(
                    "code {c} is unknown, inactive or merged"
                )));
            }
        }
        if let ArbitrationChoice::Ai { run_label } = &arb.choice {
            if !review.runs.contains(run_label) {
                return Err(validation(format!("unknown complete AI run {run_label}")));
            }
        }
        let decided_by = if arb.decided_by.trim().is_empty() {
            actor.to_string()
        } else {
            arb.decided_by.trim().to_string()
        };
        let stored = Arbitration {
            unit_id: arb.unit_id.clone(),
            phase: PHASE_2,
            choice: arb.choice.clone(),
            final_codes,
            rationale: rationale.to_string(),
            decided_by,
            decided_at: now(),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO arbitrations (interview_id, phase, unit_id, choice, final_codes, rationale, decided_by, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                interview_id,
                PHASE_2,
                stored.unit_id,
                to_json(&stored.choice),
                to_json(&stored.final_codes),
                stored.rationale,
                stored.decided_by,
                stored.decided_at
            ],
        )?;
        append_event(
            &tx,
            actor,
            "review.decide",
            Some(interview_id),
            Some(&stored.unit_id),
            json!({
                "phase": PHASE_2,
                "kind": item.item.kind,
                "human": item.item.human,
                "ai": item.item.ai,
                "decision": stored,
                "previous": item.item.decision,
            }),
        )?;
        tx.commit()?;
        Ok(stored)
    }

    fn query_decisions(&self, sql: &str, interview_id: &str) -> Result<Vec<Arbitration>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![interview_id, PHASE_2], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (unit_id, choice, final_codes, rationale, decided_by, decided_at) = row?;
            out.push(Arbitration {
                unit_id,
                phase: PHASE_2,
                choice: from_json(&choice)?,
                final_codes: from_json(&final_codes)?,
                rationale,
                decided_by,
                decided_at,
            });
        }
        Ok(out)
    }

    /// Current decision per unit (the latest recorded).
    pub fn decisions(&self, interview_id: &str) -> Result<BTreeMap<String, Arbitration>> {
        Ok(self
            .query_decisions(
                "SELECT unit_id, choice, final_codes, rationale, decided_by, decided_at FROM arbitrations
                 WHERE arb_id IN (SELECT MAX(arb_id) FROM arbitrations
                                  WHERE interview_id = ?1 AND phase = ?2 GROUP BY unit_id)
                 ORDER BY unit_id",
                interview_id,
            )?
            .into_iter()
            .map(|a| (a.unit_id.clone(), a))
            .collect())
    }

    /// Every decision ever recorded, oldest first.
    pub fn decision_history(&self, interview_id: &str) -> Result<Vec<Arbitration>> {
        self.query_decisions(
            "SELECT unit_id, choice, final_codes, rationale, decided_by, decided_at FROM arbitrations
             WHERE interview_id = ?1 AND phase = ?2 ORDER BY arb_id",
            interview_id,
        )
    }

    /// Final phase 2 coding over the scope: the decision where one exists,
    /// otherwise the candidate's current codes; plus the uncoded units.
    pub fn final_codings(&self, interview_id: &str) -> Result<(Vec<FinalCoding>, Vec<String>)> {
        let units = self.phase_units(interview_id, PHASE_2)?;
        let human = self.human_coding_map(interview_id, PHASE_2)?;
        let decisions = self.decisions(interview_id)?;
        Ok(reconcile::final_coding(&units, &human, &decisions))
    }

    /// Progress of one phase.
    pub fn phase_summary(&self, interview_id: &str, phase: u8) -> Result<PhaseSummary> {
        check_phase(phase)?;
        let guard = self.guard(interview_id, phase)?;
        let mut s = PhaseSummary {
            phase,
            n_units: guard.n_units,
            n_human_coded: guard.n_units - guard.missing.len(),
            human_complete: guard.complete,
            ..PhaseSummary::default()
        };
        if phase == PHASE_2 {
            let runs = self.ai_runs(interview_id)?;
            let review = self.review(interview_id)?;
            s.n_ai_runs = runs.len();
            s.n_ai_runs_complete = runs.iter().filter(|r| r.complete).count();
            s.n_divergences = review.n_disagreements;
            s.n_sampled = review.n_sampled;
            s.n_arbitrated = review.n_decided;
            s.n_to_clarify = self.to_clarify_items(Some(interview_id), true)?.len();
            let open = review
                .items
                .iter()
                .filter(|e| e.item.kind == ReviewKind::Disagreement && e.item.decision.is_none())
                .count();
            s.reconciled = s.n_ai_runs_complete > 0 && open == 0;
        }
        Ok(s)
    }
}
