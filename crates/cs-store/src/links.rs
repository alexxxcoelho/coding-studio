//! V1 lot 5: typed links between passages and the parts of the text.
//!
//! A link says: this passage supports, nuances or contradicts that part of the
//! write-up (a part of a theme, or the synthesis of a research question). The
//! type comes from the project list (`link_types`), the note is one line. A
//! link is removed logically, with its reason.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::analysis::{is_theme_part, LinkTarget};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use crate::{append_event, guard_err, not_found, now, validation, Store};

const MAX_NOTE: usize = 300;

/// A passage as the write-up screens show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PassageBrief {
    pub passage_id: String,
    pub interview_id: String,
    pub unit_id: String,
    /// The exact quotation.
    pub text: String,
    /// Verification state of the unit (`verified`, `unverified`, `unusable`).
    pub verification: Option<String>,
    pub stale: bool,
}

/// A link with its passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkView {
    pub link_id: String,
    pub passage: PassageBrief,
    pub target: LinkTarget,
    pub link_type: String,
    pub note: String,
    pub created_at: String,
    pub created_by: String,
}

struct Raw {
    link_id: String,
    passage_id: String,
    kind: String,
    target_id: String,
    part: String,
    link_type: String,
    note: String,
    created_at: String,
    created_by: String,
}

const COLS: &str =
    "link_id, passage_id, target_kind, target_id, part, link_type, note, created_at, created_by";

fn raw_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Raw> {
    Ok(Raw {
        link_id: r.get(0)?,
        passage_id: r.get(1)?,
        kind: r.get(2)?,
        target_id: r.get(3)?,
        part: r.get(4)?,
        link_type: r.get(5)?,
        note: r.get(6)?,
        created_at: r.get(7)?,
        created_by: r.get(8)?,
    })
}

fn target_of(raw: &Raw) -> LinkTarget {
    if raw.kind == "rq_part" {
        LinkTarget::RqPart {
            rq_id: raw.target_id.clone(),
        }
    } else {
        LinkTarget::ThemePart {
            theme_id: raw.target_id.clone(),
            part: raw.part.clone(),
        }
    }
}

fn columns_of(target: &LinkTarget) -> (&'static str, String, String) {
    match target {
        LinkTarget::ThemePart { theme_id, part } => ("theme_part", theme_id.clone(), part.clone()),
        LinkTarget::RqPart { rq_id } => ("rq_part", rq_id.clone(), String::new()),
    }
}

impl Store {
    /// Verification state of a unit as one word. A state that no longer matches
    /// the text counts as unverified.
    pub(crate) fn verification_words(&self) -> Result<BTreeMap<(String, String), String>> {
        let mut out = BTreeMap::new();
        for s in self.summaries()? {
            for v in self.verification_status(&s.interview_id)? {
                let word = if v.stale {
                    "unverified".to_string()
                } else {
                    v.state
                };
                out.insert((s.interview_id.clone(), v.unit_id), word);
            }
        }
        Ok(out)
    }

    pub(crate) fn brief_of(
        &self,
        p: &cs_model::passage::Passage,
        words: &BTreeMap<(String, String), String>,
    ) -> PassageBrief {
        PassageBrief {
            passage_id: p.passage_id.clone(),
            interview_id: p.interview_id.clone(),
            unit_id: p.unit_id.clone(),
            text: p.text.clone(),
            verification: words
                .get(&(p.interview_id.clone(), p.unit_id.clone()))
                .cloned(),
            stale: p.stale,
        }
    }

    fn view_of_raw(
        &self,
        raw: Raw,
        words: &BTreeMap<(String, String), String>,
    ) -> Result<LinkView> {
        let passage = self.passage(&raw.passage_id)?;
        Ok(LinkView {
            target: target_of(&raw),
            passage: self.brief_of(&passage, words),
            link_id: raw.link_id,
            link_type: raw.link_type,
            note: raw.note,
            created_at: raw.created_at,
            created_by: raw.created_by,
        })
    }

    fn check_target(&self, target: &LinkTarget) -> Result<()> {
        match target {
            LinkTarget::ThemePart { theme_id, part } => {
                if !is_theme_part(part) {
                    return Err(validation(format!("unknown part {part} of a theme")));
                }
                self.theme(theme_id).map(|_| ())
            }
            LinkTarget::RqPart { rq_id } => {
                if self.research_questions()?.iter().any(|q| q.id == *rq_id) {
                    Ok(())
                } else {
                    Err(validation(format!("unknown research question {rq_id}")))
                }
            }
        }
    }

    fn check_link_type(&self, link_type: &str) -> Result<()> {
        let list = self.project_config()?.link_types;
        if list.iter().any(|t| t.id == link_type) {
            Ok(())
        } else {
            Err(validation(format!(
                "link type must be one of {}, not {link_type}",
                list.iter()
                    .map(|t| t.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    fn clean_note(note: &str) -> Result<String> {
        let n = note.trim();
        if n.contains('\n') || n.chars().count() > MAX_NOTE {
            return Err(validation(format!(
                "a link note is one line of at most {MAX_NOTE} characters"
            )));
        }
        Ok(n.to_string())
    }

    fn next_link_id(&self) -> Result<String> {
        let max: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(CAST(substr(link_id, 3) AS INTEGER)), 0) FROM passage_links",
            [],
            |r| r.get(0),
        )?;
        Ok(format!("L-{:04}", max + 1))
    }

    /// Links a passage to a part of the text.
    pub fn create_link(
        &self,
        passage_id: &str,
        target: &LinkTarget,
        link_type: &str,
        note: &str,
        actor: &str,
    ) -> Result<LinkView> {
        let passage = self.passage(passage_id)?;
        self.check_target(target)?;
        self.check_link_type(link_type)?;
        let note = Self::clean_note(note)?;
        let (kind, tid, part) = columns_of(target);
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM passage_links
             WHERE passage_id = ?1 AND target_kind = ?2 AND target_id = ?3 AND part = ?4
               AND removed_at IS NULL",
            params![passage_id, kind, tid, part],
            |r| r.get(0),
        )?;
        if exists > 0 {
            return Err(guard_err(format!(
                "passage {passage_id} is already linked to this part; edit that link"
            )));
        }
        let id = self.next_link_id()?;
        let ts = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO passage_links (link_id, passage_id, target_kind, target_id, part,
               link_type, note, created_at, created_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?8)",
            params![id, passage_id, kind, tid, part, link_type, note, ts, actor],
        )?;
        append_event(
            &tx,
            actor,
            "link.create",
            Some(&passage.interview_id),
            Some(&id),
            json!({
                "link_id": id, "passage_id": passage_id, "target": target,
                "link_type": link_type, "note": note,
            }),
        )?;
        tx.commit()?;
        self.link(&id)
    }

    /// One link.
    pub fn link(&self, link_id: &str) -> Result<LinkView> {
        let raw = self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLS} FROM passage_links WHERE link_id = ?1 AND removed_at IS NULL"
                ),
                [link_id.trim()],
                raw_of,
            )
            .optional()?
            .ok_or_else(|| not_found(format!("link {link_id}")))?;
        self.view_of_raw(raw, &self.verification_words()?)
    }

    /// Changes the type and/or the note of a link.
    pub fn update_link(
        &self,
        link_id: &str,
        link_type: Option<&str>,
        note: Option<&str>,
        actor: &str,
    ) -> Result<LinkView> {
        let before = self.link(link_id)?;
        let new_type = link_type.unwrap_or(&before.link_type).to_string();
        self.check_link_type(&new_type)?;
        let new_note = match note {
            Some(n) => Self::clean_note(n)?,
            None => before.note.clone(),
        };
        if new_type == before.link_type && new_note == before.note {
            return Ok(before);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE passage_links SET link_type = ?2, note = ?3, updated_at = ?4 WHERE link_id = ?1",
            params![before.link_id, new_type, new_note, now()],
        )?;
        append_event(
            &tx,
            actor,
            "link.update",
            Some(&before.passage.interview_id),
            Some(&before.link_id),
            json!({
                "link_id": before.link_id,
                "before": {"link_type": before.link_type, "note": before.note},
                "after": {"link_type": new_type, "note": new_note},
            }),
        )?;
        tx.commit()?;
        self.link(&before.link_id)
    }

    /// Removes a link (logically; the reason is kept).
    pub fn remove_link(&self, link_id: &str, reason: &str, actor: &str) -> Result<()> {
        let before = self.link(link_id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE passage_links SET removed_at = ?2, removed_by = ?3, remove_reason = ?4
             WHERE link_id = ?1",
            params![before.link_id, now(), actor, reason.trim()],
        )?;
        append_event(
            &tx,
            actor,
            "link.remove",
            Some(&before.passage.interview_id),
            Some(&before.link_id),
            json!({
                "link_id": before.link_id, "passage_id": before.passage.passage_id,
                "target": before.target, "reason": reason.trim(),
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn links_where(
        &self,
        kind: Option<&str>,
        target_id: Option<&str>,
        part: Option<&str>,
    ) -> Result<Vec<LinkView>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM passage_links
             WHERE removed_at IS NULL
               AND (?1 IS NULL OR target_kind = ?1)
               AND (?2 IS NULL OR target_id = ?2)
               AND (?3 IS NULL OR part = ?3)
             ORDER BY link_id"
        ))?;
        let raws: Vec<Raw> = stmt
            .query_map(params![kind, target_id, part], raw_of)?
            .collect::<rusqlite::Result<_>>()?;
        let words = self.verification_words()?;
        raws.into_iter()
            .filter_map(|r| match self.view_of_raw(r, &words) {
                Ok(v) => Some(Ok(v)),
                // A link to a passage deleted since is not listed.
                Err(e) if crate::is_not_found(&e) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Every active link, in creation order.
    pub fn links(&self) -> Result<Vec<LinkView>> {
        self.links_where(None, None, None)
    }

    /// The links of one part of the text.
    pub fn links_of(&self, target: &LinkTarget) -> Result<Vec<LinkView>> {
        let (kind, tid, part) = columns_of(target);
        self.links_where(Some(kind), Some(&tid), Some(&part))
    }

    /// The links of every part of a theme, by part key.
    pub fn theme_links(&self, theme_id: &str) -> Result<BTreeMap<String, Vec<LinkView>>> {
        let mut out: BTreeMap<String, Vec<LinkView>> = BTreeMap::new();
        for l in self.links_where(Some("theme_part"), Some(theme_id), None)? {
            if let LinkTarget::ThemePart { part, .. } = &l.target {
                out.entry(part.clone()).or_default().push(l);
            }
        }
        Ok(out)
    }
}
