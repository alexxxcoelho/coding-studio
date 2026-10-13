//! V1 lot 5: the write-up.
//!
//! Per theme, five parts (analytical statement, mechanism and evidence,
//! variations and divergent cases, competing explanations and limits,
//! mini-synthesis). Each part is a text, a nature of evidence from the project
//! list, and passages linked to it. Per research question, a synthesis: the
//! themes that answer it, an answer, the limits. When the project has an
//! interpretive framework, a reading of each theme by that framework.
//!
//! Every save is a new row of `writing_versions` (append-only): earlier
//! versions stay readable. The Markdown export gathers the retained themes in
//! the document order, with citations as footnotes: interview, unit, exact
//! quotation.

use std::collections::BTreeMap;

use anyhow::Result;
use cs_model::analysis::{is_theme_part, LinkTarget, THEME_PARTS};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::links::LinkView;
use crate::{append_event, guard_err, now, validation, Store};

const MAX_TEXT: usize = 100_000;

const SCOPE_PART: &str = "theme_part";
const SCOPE_RQ: &str = "rq_synthesis";
const SCOPE_FRAMEWORK: &str = "framework_theme";

/// A save of a part: what is sent changes, what is left out stays.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PartEdit {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, deserialize_with = "crate::codebook::present")]
    pub nature: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartState {
    pub text: String,
    pub nature: Option<String>,
    pub nature_label: Option<String>,
    pub updated_at: Option<String>,
    pub n_versions: usize,
    pub links: Vec<LinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeWriting {
    pub theme_id: String,
    pub title: String,
    pub status: String,
    pub research_questions: Vec<String>,
    /// Keyed by the part keys of `THEME_PARTS`; all five are present.
    pub parts: BTreeMap<String, PartState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SynthesisTheme {
    pub theme_id: String,
    pub title: String,
    pub mini_synthesis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionSynthesis {
    pub rq_id: String,
    pub text: String,
    /// Themes that answer the question, in document order.
    pub themes: Vec<SynthesisTheme>,
    pub answer: String,
    pub limits: String,
    pub updated_at: Option<String>,
    pub n_versions: usize,
    pub links: Vec<LinkView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkCellReading {
    #[serde(default)]
    pub explains: bool,
    #[serde(default)]
    pub note: String,
}

/// Reading of a theme by the framework of the study.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkReading {
    #[serde(default)]
    pub theme_id: String,
    /// Dimension id to what the framework explains of the theme there.
    #[serde(default)]
    pub cells: BTreeMap<String, FrameworkCellReading>,
    #[serde(default)]
    pub out_of_frame: bool,
    #[serde(default)]
    pub out_of_frame_note: String,
    #[serde(default)]
    pub complementary_dimension: String,
    #[serde(default)]
    pub evidence_limit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameworkRow {
    #[serde(flatten)]
    pub reading: FrameworkReading,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameworkTable {
    pub preset: String,
    pub dimensions: Vec<cs_model::project::Dimension>,
    pub rows: Vec<FrameworkRow>,
}

fn clean_text(what: &str, text: &str) -> Result<String> {
    if text.chars().count() > MAX_TEXT {
        return Err(validation(format!(
            "{what} is longer than {MAX_TEXT} characters"
        )));
    }
    Ok(text.trim().to_string())
}

impl Store {
    // ------------------------------------------------ version rows

    pub(crate) fn latest_version(
        &self,
        scope: &str,
        target: &str,
        sub: &str,
    ) -> Result<Option<(Value, String)>> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT body, at FROM writing_versions
                 WHERE scope = ?1 AND target = ?2 AND sub = ?3
                 ORDER BY version_id DESC LIMIT 1",
                params![scope, target, sub],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((body, at)) => Ok(Some((serde_json::from_str(&body)?, at))),
        }
    }

    pub(crate) fn count_versions(&self, scope: &str, target: &str, sub: &str) -> Result<usize> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM writing_versions WHERE scope = ?1 AND target = ?2 AND sub = ?3",
            params![scope, target, sub],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }

    /// Every saved version of a written part, oldest first, as (at, by, body).
    pub fn writing_versions(
        &self,
        scope: &str,
        target: &str,
        sub: &str,
    ) -> Result<Vec<(String, String, Value)>> {
        let mut stmt = self.conn.prepare(
            "SELECT at, by, body FROM writing_versions
             WHERE scope = ?1 AND target = ?2 AND sub = ?3 ORDER BY version_id",
        )?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map(params![scope, target, sub], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .map(|(a, b, body)| Ok((a, b, serde_json::from_str(&body)?)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn put_version(
        &self,
        scope: &str,
        target: &str,
        sub: &str,
        body: &Value,
        action: &str,
        before: &Option<Value>,
        actor: &str,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO writing_versions (scope, target, sub, body, at, by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![scope, target, sub, body.to_string(), now(), actor],
        )?;
        append_event(
            &tx,
            actor,
            action,
            None,
            Some(&format!("{target}:{sub}")),
            json!({"scope": scope, "target": target, "sub": sub, "before": before, "after": body}),
        )?;
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------ theme parts

    fn nature_label(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .project_config()?
            .evidence_natures
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.label.clone()))
    }

    pub(crate) fn part_text(&self, theme_id: &str, part: &str) -> Result<(String, Option<String>)> {
        Ok(match self.latest_version(SCOPE_PART, theme_id, part)? {
            Some((b, _)) => (
                b["text"].as_str().unwrap_or_default().to_string(),
                b["nature"].as_str().map(str::to_string),
            ),
            None => (String::new(), None),
        })
    }

    fn part_state(&self, theme_id: &str, part: &str, links: Vec<LinkView>) -> Result<PartState> {
        let latest = self.latest_version(SCOPE_PART, theme_id, part)?;
        let (text, nature, at) = match &latest {
            Some((b, at)) => (
                b["text"].as_str().unwrap_or_default().to_string(),
                b["nature"].as_str().map(str::to_string),
                Some(at.clone()),
            ),
            None => (String::new(), None, None),
        };
        Ok(PartState {
            nature_label: match &nature {
                Some(n) => self.nature_label(n)?.or_else(|| Some(n.clone())),
                None => None,
            },
            text,
            nature,
            updated_at: at,
            n_versions: self.count_versions(SCOPE_PART, theme_id, part)?,
            links,
        })
    }

    /// The five parts of a theme, with their links.
    pub fn theme_writing(&self, theme_id: &str) -> Result<ThemeWriting> {
        let theme = self.theme(theme_id)?;
        let mut links = self.theme_links(&theme.theme_id)?;
        let mut parts = BTreeMap::new();
        for (key, _) in THEME_PARTS {
            let l = links.remove(key).unwrap_or_default();
            parts.insert(key.to_string(), self.part_state(&theme.theme_id, key, l)?);
        }
        Ok(ThemeWriting {
            theme_id: theme.theme_id,
            title: theme.title,
            status: theme.status,
            research_questions: theme.research_questions,
            parts,
        })
    }

    /// Saves a part. No new version when nothing changed.
    pub fn set_theme_part(
        &self,
        theme_id: &str,
        part: &str,
        edit: &PartEdit,
        actor: &str,
    ) -> Result<PartState> {
        let theme = self.theme(theme_id)?;
        if !is_theme_part(part) {
            return Err(validation(format!(
                "unknown part {part}; use {}",
                THEME_PARTS.map(|(k, _)| k).join(", ")
            )));
        }
        let (old_text, old_nature) = self.part_text(&theme.theme_id, part)?;
        let text = match &edit.text {
            Some(t) => clean_text("a part", t)?,
            None => old_text.clone(),
        };
        let nature = match &edit.nature {
            None => old_nature.clone(),
            Some(None) => None,
            Some(Some(n)) => {
                let n = n.trim();
                if n.is_empty() {
                    None
                } else {
                    // A value already saved stays accepted if the list changed since.
                    if old_nature.as_deref() != Some(n) {
                        let list = self.project_config()?.evidence_natures;
                        if !list.iter().any(|i| i.id == n) {
                            return Err(validation(format!(
                                "nature must be one of {}, not {n}",
                                list.iter()
                                    .map(|i| i.id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )));
                        }
                    }
                    Some(n.to_string())
                }
            }
        };
        let had = self.latest_version(SCOPE_PART, &theme.theme_id, part)?;
        if had.is_some() && text == old_text && nature == old_nature {
            let links = self
                .theme_links(&theme.theme_id)?
                .remove(part)
                .unwrap_or_default();
            return self.part_state(&theme.theme_id, part, links);
        }
        if had.is_none() && text.is_empty() && nature.is_none() {
            return self.part_state(&theme.theme_id, part, Vec::new());
        }
        self.put_version(
            SCOPE_PART,
            &theme.theme_id,
            part,
            &json!({"text": text, "nature": nature}),
            "writing.part",
            &had.map(|(b, _)| b),
            actor,
        )?;
        let links = self
            .theme_links(&theme.theme_id)?
            .remove(part)
            .unwrap_or_default();
        self.part_state(&theme.theme_id, part, links)
    }

    // ------------------------------------------------ research questions

    /// The synthesis of a research question.
    pub fn question_synthesis(&self, rq_id: &str) -> Result<QuestionSynthesis> {
        let q = self
            .research_questions()?
            .into_iter()
            .find(|q| q.id == rq_id.trim())
            .ok_or_else(|| crate::not_found(format!("research question {rq_id}")))?;
        let mut themes = Vec::new();
        for t in self.themes(None)? {
            if t.research_questions.contains(&q.id) {
                let (mini, _) = self.part_text(&t.theme_id, "mini_synthesis")?;
                themes.push(SynthesisTheme {
                    theme_id: t.theme_id,
                    title: t.title,
                    mini_synthesis: mini,
                });
            }
        }
        let latest = self.latest_version(SCOPE_RQ, &q.id, "")?;
        let (answer, limits, at) = match &latest {
            Some((b, at)) => (
                b["answer"].as_str().unwrap_or_default().to_string(),
                b["limits"].as_str().unwrap_or_default().to_string(),
                Some(at.clone()),
            ),
            None => (String::new(), String::new(), None),
        };
        Ok(QuestionSynthesis {
            links: self.links_of(&LinkTarget::RqPart {
                rq_id: q.id.clone(),
            })?,
            n_versions: self.count_versions(SCOPE_RQ, &q.id, "")?,
            rq_id: q.id,
            text: q.text,
            themes,
            answer,
            limits,
            updated_at: at,
        })
    }

    /// Saves the answer and the limits of a research question.
    pub fn set_question_synthesis(
        &self,
        rq_id: &str,
        answer: &str,
        limits: &str,
        actor: &str,
    ) -> Result<QuestionSynthesis> {
        let current = self.question_synthesis(rq_id)?;
        let answer = clean_text("the answer", answer)?;
        let limits = clean_text("the limits", limits)?;
        if (current.n_versions > 0 || !answer.is_empty() || !limits.is_empty())
            && !(answer == current.answer && limits == current.limits && current.n_versions > 0)
        {
            let before = self
                .latest_version(SCOPE_RQ, &current.rq_id, "")?
                .map(|(b, _)| b);
            self.put_version(
                SCOPE_RQ,
                &current.rq_id,
                "",
                &json!({"answer": answer, "limits": limits}),
                "writing.question",
                &before,
                actor,
            )?;
        }
        self.question_synthesis(&current.rq_id)
    }

    // ------------------------------------------------ framework reading

    fn require_framework(&self) -> Result<cs_model::project::Framework> {
        self.project_config()?
            .interpretive_framework
            .ok_or_else(|| {
                guard_err(
                    "the project has no interpretive framework: set one in the project settings",
                )
            })
    }

    /// The reading of a theme by the framework (empty until written).
    pub fn framework_reading(&self, theme_id: &str) -> Result<FrameworkReading> {
        let fw = self.require_framework()?;
        let theme = self.theme(theme_id)?;
        let mut reading = match self.latest_version(SCOPE_FRAMEWORK, &theme.theme_id, "")? {
            Some((b, _)) => serde_json::from_value(b).unwrap_or_default(),
            None => FrameworkReading::default(),
        };
        reading.theme_id = theme.theme_id;
        // Only the dimensions the framework has now; one entry for each.
        reading.cells = fw
            .dimensions
            .iter()
            .map(|d| {
                (
                    d.id.clone(),
                    reading.cells.remove(&d.id).unwrap_or_default(),
                )
            })
            .collect();
        Ok(reading)
    }

    pub fn set_framework_reading(
        &self,
        theme_id: &str,
        reading: &FrameworkReading,
        actor: &str,
    ) -> Result<FrameworkReading> {
        let fw = self.require_framework()?;
        let before = self.framework_reading(theme_id)?;
        for id in reading.cells.keys() {
            if !fw.dimensions.iter().any(|d| d.id == *id) {
                return Err(validation(format!("unknown framework dimension {id}")));
            }
        }
        let mut next = FrameworkReading {
            theme_id: before.theme_id.clone(),
            cells: BTreeMap::new(),
            out_of_frame: reading.out_of_frame,
            out_of_frame_note: clean_text("the out-of-frame note", &reading.out_of_frame_note)?,
            complementary_dimension: clean_text(
                "the complementary dimension",
                &reading.complementary_dimension,
            )?,
            evidence_limit: clean_text("the evidence limit", &reading.evidence_limit)?,
        };
        for d in &fw.dimensions {
            let c = reading.cells.get(&d.id).cloned().unwrap_or_default();
            next.cells.insert(
                d.id.clone(),
                FrameworkCellReading {
                    explains: c.explains,
                    note: clean_text("a framework note", &c.note)?,
                },
            );
        }
        if next == before {
            return Ok(before);
        }
        let prev = self
            .latest_version(SCOPE_FRAMEWORK, &before.theme_id, "")?
            .map(|(b, _)| b);
        let mut body = serde_json::to_value(&next)?;
        body.as_object_mut().map(|o| o.remove("theme_id"));
        self.put_version(
            SCOPE_FRAMEWORK,
            &before.theme_id,
            "",
            &body,
            "writing.framework",
            &prev,
            actor,
        )?;
        self.framework_reading(&before.theme_id)
    }

    /// Themes by framework dimensions.
    pub fn framework_table(&self) -> Result<FrameworkTable> {
        let fw = self.require_framework()?;
        let mut rows = Vec::new();
        for t in self.themes(None)? {
            rows.push(FrameworkRow {
                reading: self.framework_reading(&t.theme_id)?,
                title: t.title,
            });
        }
        Ok(FrameworkTable {
            preset: fw.preset,
            dimensions: fw.dimensions,
            rows,
        })
    }
}

/// One citation of the write-up: a linked passage, numbered in order of first use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteupCite {
    pub n: usize,
    pub interview_id: String,
    pub unit_id: String,
    /// The exact quotation, whitespace collapsed to single spaces.
    pub quote: String,
    pub note: String,
}

impl WriteupCite {
    /// `interview, unité unit` (the reference as printed in a footnote).
    pub fn reference(&self) -> String {
        format!("{}, unité {}", self.interview_id, self.unit_id)
    }
}

/// A block of the write-up, independent of the output format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WriteupBlock {
    Heading {
        level: u8,
        text: String,
    },
    /// `cites` are the numbers of the citations that support the paragraph.
    Para {
        text: String,
        italic: bool,
        cites: Vec<usize>,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Bullet {
        text: String,
    },
}

/// The write-up as blocks and citations; rendered to Markdown or to docx.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteupDoc {
    pub blocks: Vec<WriteupBlock>,
    pub cites: Vec<WriteupCite>,
}

impl WriteupDoc {
    /// Markdown with the citations as footnotes: interview, unit, exact quotation.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            match b {
                WriteupBlock::Heading { level, text } => {
                    md.push_str(&format!("{} {text}\n\n", "#".repeat(*level as usize)));
                }
                WriteupBlock::Para {
                    text,
                    italic,
                    cites,
                } => {
                    let marks: String = cites.iter().map(|n| format!("[^{n}]")).collect();
                    if *italic {
                        md.push_str(&format!("_{text}_\n\n"));
                    } else {
                        md.push_str(&format!("{text}{marks}\n\n"));
                    }
                }
                WriteupBlock::Table { header, rows } => {
                    md.push('|');
                    for h in header {
                        md.push_str(&format!(" {h} |"));
                    }
                    md.push_str("\n|");
                    for _ in header {
                        md.push_str("---|");
                    }
                    md.push('\n');
                    for r in rows {
                        md.push('|');
                        for c in r {
                            md.push_str(&format!(" {c} |"));
                        }
                        md.push('\n');
                    }
                    md.push('\n');
                }
                WriteupBlock::Bullet { text } => {
                    md.push_str(&format!("- {text}\n"));
                    if !matches!(self.blocks.get(i + 1), Some(WriteupBlock::Bullet { .. })) {
                        md.push('\n');
                    }
                }
            }
        }
        if !self.cites.is_empty() {
            md.push_str("---\n\n");
            for c in &self.cites {
                let mut line = format!("{} : « {} »", c.reference(), c.quote);
                if !c.note.is_empty() {
                    line.push_str(&format!(" ({})", c.note));
                }
                md.push_str(&format!("[^{}]: {line}\n", c.n));
            }
        }
        md
    }
}

impl Store {
    // ------------------------------------------------ export

    /// The write-up as blocks: retained themes in the document order, their
    /// five parts, the syntheses, the framework table, the final audit. Each
    /// linked passage is a citation: interview, unit, exact quotation.
    pub fn writeup_doc(&self) -> Result<WriteupDoc> {
        let cfg = self.study_config()?;
        let mut cites: Vec<WriteupCite> = Vec::new();
        let mut note_of: BTreeMap<String, usize> = BTreeMap::new();
        let mut cite = |l: &LinkView| -> usize {
            let key = l.passage.passage_id.clone();
            *note_of.entry(key).or_insert_with(|| {
                let quote = l
                    .passage
                    .text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let n = cites.len() + 1;
                cites.push(WriteupCite {
                    n,
                    interview_id: l.passage.interview_id.clone(),
                    unit_id: l.passage.unit_id.clone(),
                    quote,
                    note: l.note.clone(),
                });
                n
            })
        };
        let mut blocks: Vec<WriteupBlock> = Vec::new();
        let heading = |level: u8, text: String| WriteupBlock::Heading { level, text };
        let para = |text: String| WriteupBlock::Para {
            text,
            italic: false,
            cites: Vec::new(),
        };
        let title = if cfg.study_name.trim().is_empty() {
            "Rédaction".to_string()
        } else {
            format!("{} : rédaction", cfg.study_name.trim())
        };
        blocks.push(heading(1, title));
        let retained = self.themes(Some("retained"))?;
        if retained.is_empty() {
            blocks.push(para("Aucun thème retenu pour l'instant.".into()));
        }
        for t in &retained {
            let w = self.theme_writing(&t.theme_id)?;
            blocks.push(heading(2, t.title.clone()));
            for (key, part_title) in THEME_PARTS {
                let p = &w.parts[key];
                blocks.push(heading(3, part_title.to_string()));
                if p.text.is_empty() {
                    blocks.push(WriteupBlock::Para {
                        text: "Partie non rédigée.".into(),
                        italic: true,
                        cites: Vec::new(),
                    });
                } else {
                    let marks: Vec<usize> = p.links.iter().map(&mut cite).collect();
                    blocks.push(WriteupBlock::Para {
                        text: p.text.clone(),
                        italic: false,
                        cites: marks,
                    });
                }
                if let Some(n) = &p.nature_label {
                    blocks.push(para(format!("Nature de la preuve : {n}.")));
                }
            }
        }
        let questions = self.research_questions()?;
        if !questions.is_empty() {
            blocks.push(heading(2, "Synthèse par question de recherche".into()));
            for q in &questions {
                let s = self.question_synthesis(&q.id)?;
                blocks.push(heading(3, format!("{} : {}", s.rq_id, s.text)));
                let answering: Vec<&str> = s
                    .themes
                    .iter()
                    .filter(|t| retained.iter().any(|r| r.theme_id == t.theme_id))
                    .map(|t| t.title.as_str())
                    .collect();
                if !answering.is_empty() {
                    blocks.push(para(format!(
                        "Thèmes qui y répondent : {}.",
                        answering.join(" ; ")
                    )));
                }
                if s.answer.is_empty() {
                    blocks.push(WriteupBlock::Para {
                        text: "Synthèse non rédigée.".into(),
                        italic: true,
                        cites: Vec::new(),
                    });
                } else {
                    let marks: Vec<usize> = s.links.iter().map(&mut cite).collect();
                    blocks.push(WriteupBlock::Para {
                        text: s.answer.clone(),
                        italic: false,
                        cites: marks,
                    });
                }
                if !s.limits.is_empty() {
                    blocks.push(para(format!("Limites : {}", s.limits)));
                }
            }
        }
        if let Ok(table) = self.framework_table() {
            blocks.push(heading(2, "Lecture par le cadre interprétatif".into()));
            let mut header = vec!["Thème".to_string()];
            header.extend(table.dimensions.iter().map(|d| d.label.clone()));
            header.push("Hors cadre".into());
            let mut rows = Vec::new();
            for r in &table.rows {
                let mut row = vec![r.title.replace('|', "/")];
                for d in &table.dimensions {
                    let c = r.reading.cells.get(&d.id).cloned().unwrap_or_default();
                    let cell = match (c.explains, c.note.is_empty()) {
                        (true, true) => "explique".to_string(),
                        (true, false) => format!("explique : {}", c.note),
                        (false, true) => "n'explique pas".to_string(),
                        (false, false) => format!("n'explique pas : {}", c.note),
                    };
                    row.push(cell.replace(['|', '\n'], " "));
                }
                let out = if r.reading.out_of_frame {
                    if r.reading.out_of_frame_note.is_empty() {
                        "oui".to_string()
                    } else {
                        format!("oui : {}", r.reading.out_of_frame_note)
                    }
                } else {
                    "non".to_string()
                };
                row.push(out.replace(['|', '\n'], " "));
                rows.push(row);
            }
            blocks.push(WriteupBlock::Table { header, rows });
        }
        if let Some(a) = self.latest_final_audit()? {
            blocks.push(heading(2, "Audit final".into()));
            blocks.push(para(format!(
                "Tirage de {} affirmations, graine `{}`.",
                a.n, a.seed
            )));
            for c in &a.claims {
                let verdict = c
                    .verdict
                    .as_ref()
                    .map(|v| {
                        let label = cs_model::analysis::audit_verdict_label(&v.verdict);
                        if v.note.is_empty() {
                            label.to_string()
                        } else {
                            format!("{label} ({})", v.note)
                        }
                    })
                    .unwrap_or_else(|| "pas encore jugée".to_string());
                blocks.push(WriteupBlock::Bullet {
                    text: format!("{} : {} Verdict : {}.", c.part_title, c.sentence, verdict),
                });
            }
        }
        Ok(WriteupDoc { blocks, cites })
    }

    /// The write-up in Markdown (see [`Store::writeup_doc`]).
    pub fn writeup_markdown(&self) -> Result<String> {
        Ok(self.writeup_doc()?.to_markdown())
    }
}
