//! V1 lot 5: the final audit.
//!
//! Draws a few claims at random (parts of the write-up of retained themes, and
//! syntheses of research questions, preferring those that carry linked
//! passages) and shows, for each, the whole chain: part, theme, codes,
//! passages, exact quotation, interview, verification state. The researcher
//! gives a verdict (supported, to nuance, to withdraw) with a note. The draw is
//! seeded and the seed is recorded with it: the same seed over the same
//! write-up gives the same draw.

use cs_model::analysis::{
    audit_verdict_label, fresh_seed, seeded_draw, theme_part_title, LinkTarget, AUDIT_VERDICTS,
};
use cs_model::sha256_hex;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use anyhow::Result;

use crate::links::{LinkView, PassageBrief};
use crate::{append_event, guard_err, not_found, now, validation, Store};

/// Most claims one draw may hold.
pub const MAX_DRAW: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditGap {
    /// `no_text`, `no_link`, `unverified_passage` or `stale_passage`.
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditChainLink {
    pub link_id: String,
    pub link_type: String,
    pub note: String,
    pub passage: PassageBrief,
    /// Hash of the segmentation the passage was anchored on.
    pub interview_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditThemeRef {
    pub theme_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditCodeRef {
    pub code_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditVerdictView {
    pub verdict: String,
    pub verdict_label: String,
    pub note: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditClaim {
    pub claim_key: String,
    pub part_title: String,
    /// The sentence drawn from the written text (seeded).
    pub sentence: String,
    /// The whole text of the part.
    pub text: String,
    pub theme: Option<AuditThemeRef>,
    pub rq_id: Option<String>,
    pub codes: Vec<AuditCodeRef>,
    pub links: Vec<AuditChainLink>,
    pub gaps: Vec<AuditGap>,
    pub verdict: Option<AuditVerdictView>,
    /// Digest of the chain as shown now.
    pub chain_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalAudit {
    pub audit_id: i64,
    pub seed: String,
    pub n: usize,
    /// Claims the draw was made among.
    pub eligible: usize,
    pub created_at: String,
    pub created_by: String,
    pub claims: Vec<AuditClaim>,
}

/// A claim of the write-up that can be drawn.
struct Candidate {
    key: String,
    linked: bool,
}

fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        cur.push(*c);
        let end = matches!(c, '.' | '!' | '?' | '…')
            && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if end {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
            cur.clear();
        }
    }
    let rest = cur.trim().to_string();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

impl Store {
    fn audit_candidates(&self) -> Result<Vec<Candidate>> {
        let mut out = Vec::new();
        for t in self.themes(Some("retained"))? {
            let mut links = self.theme_links(&t.theme_id)?;
            for (key, _) in cs_model::analysis::THEME_PARTS {
                let (text, _) = self.part_text(&t.theme_id, key)?;
                let linked = links.remove(key).is_some_and(|l| !l.is_empty());
                if linked || !text.is_empty() {
                    out.push(Candidate {
                        key: LinkTarget::ThemePart {
                            theme_id: t.theme_id.clone(),
                            part: key.to_string(),
                        }
                        .claim_key(),
                        linked,
                    });
                }
            }
        }
        for q in self.research_questions()? {
            let s = self.question_synthesis(&q.id)?;
            let linked = !s.links.is_empty();
            if linked || !s.answer.is_empty() {
                out.push(Candidate {
                    key: LinkTarget::RqPart { rq_id: q.id }.claim_key(),
                    linked,
                });
            }
        }
        Ok(out)
    }

    fn audit_claim(
        &self,
        seed: &str,
        key: &str,
        verdict: Option<AuditVerdictView>,
        words: &std::collections::BTreeMap<(String, String), String>,
    ) -> Result<AuditClaim> {
        let target = LinkTarget::from_claim_key(key)
            .ok_or_else(|| validation(format!("unknown claim {key}")))?;
        let (part_title, text, theme, rq_id, codes, links): (
            String,
            String,
            Option<AuditThemeRef>,
            Option<String>,
            Vec<AuditCodeRef>,
            Vec<LinkView>,
        ) = match &target {
            LinkTarget::ThemePart { theme_id, part } => {
                let t = self.theme(theme_id)?;
                let (text, _) = self.part_text(theme_id, part)?;
                (
                    theme_part_title(part).to_string(),
                    text,
                    Some(AuditThemeRef {
                        theme_id: t.theme_id.clone(),
                        title: t.title.clone(),
                    }),
                    None,
                    t.codes
                        .iter()
                        .map(|c| AuditCodeRef {
                            code_id: c.code_id.clone(),
                            name: c.name.clone(),
                        })
                        .collect(),
                    self.links_of(&target)?,
                )
            }
            LinkTarget::RqPart { rq_id } => {
                let s = self.question_synthesis(rq_id)?;
                (
                    format!("Synthèse de {rq_id}"),
                    s.answer.clone(),
                    None,
                    Some(rq_id.clone()),
                    Vec::new(),
                    s.links,
                )
            }
        };
        let mut gaps = Vec::new();
        if text.trim().is_empty() {
            gaps.push(AuditGap {
                kind: "no_text".into(),
                message: "La partie n'est pas rédigée.".into(),
            });
        }
        if links.is_empty() {
            gaps.push(AuditGap {
                kind: "no_link".into(),
                message: "Aucun passage n'est lié à cette partie.".into(),
            });
        }
        let mut chain = Vec::new();
        for l in &links {
            let passage = self.passage(&l.passage.passage_id)?;
            let mut brief = l.passage.clone();
            if brief.verification.is_none() {
                brief.verification = words
                    .get(&(brief.interview_id.clone(), brief.unit_id.clone()))
                    .cloned();
            }
            if brief.verification.as_deref() != Some("verified") {
                gaps.push(AuditGap {
                    kind: "unverified_passage".into(),
                    message: format!(
                        "Le passage {} ({}) n'est pas vérifié sur l'enregistrement.",
                        brief.passage_id, brief.unit_id
                    ),
                });
            }
            if passage.stale {
                gaps.push(AuditGap {
                    kind: "stale_passage".into(),
                    message: format!(
                        "Le texte de l'unité {} a changé depuis que le passage {} a été saisi.",
                        brief.unit_id, brief.passage_id
                    ),
                });
            }
            chain.push(AuditChainLink {
                link_id: l.link_id.clone(),
                link_type: l.link_type.clone(),
                note: l.note.clone(),
                passage: brief,
                interview_version: Some(passage.segmentation_hash.clone()),
            });
        }
        let all = sentences(&text);
        let sentence = if all.is_empty() {
            String::new()
        } else {
            let h = sha256_hex(format!("{seed}|{key}").as_bytes());
            let n = u64::from_str_radix(&h[..8], 16).unwrap_or(0) as usize;
            all[n % all.len()].clone()
        };
        let chain_sha256 = sha256_hex(
            json!({
                "key": key, "text": text, "codes": codes.iter().map(|c| &c.code_id).collect::<Vec<_>>(),
                "links": chain.iter().map(|c| json!({
                    "link": c.link_id, "type": c.link_type, "passage": c.passage.passage_id,
                    "quote": c.passage.text, "verification": c.passage.verification,
                })).collect::<Vec<_>>(),
            })
            .to_string()
            .as_bytes(),
        );
        Ok(AuditClaim {
            claim_key: key.to_string(),
            part_title,
            sentence,
            text,
            theme,
            rq_id,
            codes,
            links: chain,
            gaps,
            verdict,
            chain_sha256,
        })
    }

    fn final_audit_row(&self, audit_id: i64) -> Result<FinalAudit> {
        let row: (i64, String, i64, String, String, String, String) = self
            .conn
            .query_row(
                "SELECT audit_id, seed, n, eligible, claims, created_at, created_by
                 FROM final_audits WHERE audit_id = ?1",
                [audit_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| not_found(format!("final audit {audit_id}")))?;
        let eligible: Vec<String> = serde_json::from_str(&row.3)?;
        let keys: Vec<String> = serde_json::from_str(&row.4)?;
        let words = self.verification_words()?;
        let mut claims = Vec::new();
        for key in &keys {
            let v: Option<(String, String, String)> = self
                .conn
                .query_row(
                    "SELECT verdict, note, at FROM final_audit_verdicts
                     WHERE audit_id = ?1 AND claim_key = ?2 ORDER BY verdict_id DESC LIMIT 1",
                    params![audit_id, key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let verdict = v.map(|(v, note, at)| AuditVerdictView {
                verdict_label: audit_verdict_label(&v).to_string(),
                verdict: v,
                note,
                at,
            });
            claims.push(self.audit_claim(&row.1, key, verdict, &words)?);
        }
        Ok(FinalAudit {
            audit_id: row.0,
            seed: row.1,
            n: row.2 as usize,
            eligible: eligible.len(),
            created_at: row.5,
            created_by: row.6,
            claims,
        })
    }

    /// The latest draw, or none.
    pub fn latest_final_audit(&self) -> Result<Option<FinalAudit>> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT audit_id FROM final_audits ORDER BY audit_id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|i| self.final_audit_row(i)).transpose()
    }

    /// One draw by id.
    pub fn final_audit(&self, audit_id: i64) -> Result<FinalAudit> {
        self.final_audit_row(audit_id)
    }

    /// Every draw, newest first, without the chains.
    pub fn final_audit_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT audit_id FROM final_audits ORDER BY audit_id DESC")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Draws `n` claims (default 3) with the given seed, or a fresh one that is
    /// recorded. Linked parts are drawn first; unlinked written parts fill the
    /// draw only when there are not enough linked ones.
    pub fn draw_final_audit(
        &self,
        n: usize,
        seed: Option<&str>,
        actor: &str,
    ) -> Result<FinalAudit> {
        if n == 0 || n > MAX_DRAW {
            return Err(validation(format!("draw between 1 and {MAX_DRAW} claims")));
        }
        let candidates = self.audit_candidates()?;
        if candidates.is_empty() {
            return Err(guard_err(
                "nothing to audit yet: write and link at least one part of a retained theme",
            ));
        }
        let ts = now();
        let seed = match seed.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => {
                let count: i64 =
                    self.conn
                        .query_row("SELECT COUNT(*) FROM final_audits", [], |r| r.get(0))?;
                fresh_seed(&ts, &count.to_string())
            }
        };
        let linked: Vec<String> = candidates
            .iter()
            .filter(|c| c.linked)
            .map(|c| c.key.clone())
            .collect();
        let unlinked: Vec<String> = candidates
            .iter()
            .filter(|c| !c.linked)
            .map(|c| c.key.clone())
            .collect();
        let mut drawn = seeded_draw(&seed, &linked, n);
        if drawn.len() < n {
            drawn.extend(seeded_draw(&seed, &unlinked, n - drawn.len()));
        }
        let eligible: Vec<String> = candidates.iter().map(|c| c.key.clone()).collect();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO final_audits (seed, n, eligible, claims, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                seed,
                n as i64,
                serde_json::to_string(&eligible)?,
                serde_json::to_string(&drawn)?,
                ts,
                actor
            ],
        )?;
        let id = tx.last_insert_rowid();
        append_event(
            &tx,
            actor,
            "audit.draw",
            None,
            Some(&id.to_string()),
            json!({"audit_id": id, "seed": seed, "n": n, "eligible": eligible, "claims": drawn}),
        )?;
        tx.commit()?;
        self.final_audit_row(id)
    }

    /// Records a verdict on a drawn claim. A later verdict replaces the
    /// earlier one for display; both stay.
    pub fn set_audit_verdict(
        &self,
        audit_id: i64,
        claim_key: &str,
        verdict: &str,
        note: &str,
        actor: &str,
    ) -> Result<FinalAudit> {
        if !AUDIT_VERDICTS.contains(&verdict) {
            return Err(validation(format!(
                "verdict must be one of {}, not {verdict}",
                AUDIT_VERDICTS.join(", ")
            )));
        }
        let audit = self.final_audit_row(audit_id)?;
        let claim = audit
            .claims
            .iter()
            .find(|c| c.claim_key == claim_key)
            .ok_or_else(|| validation(format!("{claim_key} is not part of audit {audit_id}")))?;
        let note = note.trim();
        if note.chars().count() > 2000 {
            return Err(validation("a verdict note is at most 2000 characters"));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO final_audit_verdicts (audit_id, claim_key, verdict, note, chain_sha256, at, by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![audit_id, claim_key, verdict, note, claim.chain_sha256, now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            "audit.verdict",
            None,
            Some(&format!("{audit_id}:{claim_key}")),
            json!({
                "audit_id": audit_id, "claim_key": claim_key, "verdict": verdict,
                "note": note, "chain_sha256": claim.chain_sha256,
            }),
        )?;
        tx.commit()?;
        self.final_audit_row(audit_id)
    }

    /// Every verdict ever given, for export.
    pub fn audit_verdict_log(&self) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT verdict_id, audit_id, claim_key, verdict, note, chain_sha256, at, by
             FROM final_audit_verdicts ORDER BY verdict_id",
        )?;
        let out = stmt
            .query_map([], |r| {
                Ok(json!({
                    "verdict_id": r.get::<_, i64>(0)?, "audit_id": r.get::<_, i64>(1)?,
                    "claim_key": r.get::<_, String>(2)?, "verdict": r.get::<_, String>(3)?,
                    "note": r.get::<_, String>(4)?, "chain_sha256": r.get::<_, String>(5)?,
                    "at": r.get::<_, String>(6)?, "by": r.get::<_, String>(7)?,
                }))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }
}
