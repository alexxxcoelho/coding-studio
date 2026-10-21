//! Editable JSON exchange for the complete current write-up. Preview runs the
//! ordinary domain operations in an isolated SQLite snapshot. Only their scoped
//! row changes are committed, together, after a concurrency check.
use crate::{
    guard_err, validation,
    writing::{FrameworkReading, PartEdit},
    Store,
};
use anyhow::Result;
use cs_model::{analysis::LinkTarget, sha256_hex, Case};
use rusqlite::{backup::Backup, params_from_iter, types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingJson {
    pub schema: String,
    pub source_fingerprint: String,
    pub theme_order: Vec<String>,
    pub themes: Vec<WritingThemeJson>,
    pub questions: Vec<WritingQuestionJson>,
    pub decisions: Vec<Case>,
    pub audit_verdicts: Vec<WritingVerdictJson>,
    /// Exact source quotations, theme identities and current audit draws.
    /// Context for editing; never imported as new source or historical actions.
    pub references: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingThemeJson {
    pub theme_id: String,
    pub parts: BTreeMap<String, WritingPartJson>,
    pub framework: Option<FrameworkReading>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingPartJson {
    pub text: String,
    pub nature: Option<String>,
    pub links: Vec<WritingLinkJson>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingLinkJson {
    pub passage_id: String,
    pub link_type: String,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingQuestionJson {
    pub rq_id: String,
    pub answer: String,
    pub limits: String,
    pub links: Vec<WritingLinkJson>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingVerdictJson {
    pub audit_id: i64,
    pub claim_key: String,
    pub chain_sha256: String,
    pub verdict: Option<String>,
    pub note: String,
}

const TABLES: &[&str] = &[
    "themes",
    "writing_versions",
    "passage_links",
    "cases",
    "case_units",
    "final_audit_verdicts",
    "audit_log",
];
fn fingerprint(value: &Value) -> String {
    sha256_hex(serde_json::to_string(value).unwrap().as_bytes())
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn rows(
    conn: &Connection,
    table: &str,
) -> Result<(Vec<String>, Vec<usize>, BTreeMap<String, Vec<SqlValue>>)> {
    let mut info = conn.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    let columns = info
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let pk: Vec<_> = columns
        .iter()
        .enumerate()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(i, _)| i)
        .collect();
    anyhow::ensure!(!pk.is_empty(), "exchange table needs a primary key");
    let names = columns.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM {}",
        names.iter().map(|n| quote(n)).collect::<Vec<_>>().join(","),
        quote(table)
    ))?;
    let mut query = stmt.query([])?;
    let mut out = BTreeMap::new();
    while let Some(row) = query.next()? {
        let values = (0..names.len())
            .map(|i| row.get(i))
            .collect::<rusqlite::Result<Vec<SqlValue>>>()?;
        let key = format!("{:?}", pk.iter().map(|i| &values[*i]).collect::<Vec<_>>());
        out.insert(key, values);
    }
    Ok((names, pk, out))
}

impl Store {
    pub fn export_writing_json(&self) -> Result<WritingJson> {
        let themes = self.themes(None)?;
        let config = self.project_config()?;
        let mut editable = Vec::new();
        let mut pools = Vec::new();
        let mut identities = Vec::new();
        for t in &themes {
            let writing = self.theme_writing(&t.theme_id)?;
            editable.push(WritingThemeJson {
                theme_id: t.theme_id.clone(),
                parts: writing
                    .parts
                    .into_iter()
                    .map(|(key, state)| {
                        (
                            key,
                            WritingPartJson {
                                text: state.text,
                                nature: state.nature,
                                links: state
                                    .links
                                    .into_iter()
                                    .map(|l| WritingLinkJson {
                                        passage_id: l.passage.passage_id,
                                        link_type: l.link_type,
                                        note: l.note,
                                    })
                                    .collect(),
                            },
                        )
                    })
                    .collect(),
                framework: if config.interpretive_framework.is_some() {
                    Some(self.framework_reading(&t.theme_id)?)
                } else {
                    None
                },
            });
            identities.push(json!({"theme_id":t.theme_id,"title":t.title,"status":t.status}));
            pools.push(json!({"theme_id":t.theme_id,"passages":self.theme_passages(&t.theme_id)?}));
        }
        let questions = self.research_questions()?;
        let mut syntheses = Vec::new();
        for q in &questions {
            let s = self.question_synthesis(&q.id)?;
            syntheses.push(WritingQuestionJson {
                rq_id: q.id.clone(),
                answer: s.answer,
                limits: s.limits,
                links: s
                    .links
                    .into_iter()
                    .map(|l| WritingLinkJson {
                        passage_id: l.passage.passage_id,
                        link_type: l.link_type,
                        note: l.note,
                    })
                    .collect(),
            });
        }
        let mut audits = Vec::new();
        let mut verdicts = Vec::new();
        for id in self.final_audit_ids()? {
            let audit = self.final_audit(id)?;
            for c in &audit.claims {
                verdicts.push(WritingVerdictJson {
                    audit_id: id,
                    claim_key: c.claim_key.clone(),
                    chain_sha256: c.chain_sha256.clone(),
                    verdict: c.verdict.as_ref().map(|v| v.verdict.clone()),
                    note: c
                        .verdict
                        .as_ref()
                        .map(|v| v.note.clone())
                        .unwrap_or_default(),
                });
            }
            audits.push(audit);
        }
        let references = json!({"themes":identities,"research_questions":questions,"passages":pools,"evidence_natures":config.evidence_natures,"link_types":config.link_types,"framework":config.interpretive_framework,"final_audits":audits});
        let corpus=self.summaries()?.into_iter().map(|s|Ok(json!({"id":s.interview_id,"segmentation":self.segmentation(&s.interview_id)?.map(|s|s.hash)}))).collect::<Result<Vec<_>>>()?;
        let source_fingerprint = fingerprint(
            &json!({"corpus":corpus,"theme_ids":themes.iter().map(|t|&t.theme_id).collect::<Vec<_>>(),"questions":questions.iter().map(|q|&q.id).collect::<Vec<_>>()}),
        );
        Ok(WritingJson {
            schema: "coding-studio.writing.v1".into(),
            source_fingerprint,
            theme_order: themes.iter().map(|t| t.theme_id.clone()).collect(),
            themes: editable,
            questions: syntheses,
            decisions: self.cases(None)?,
            audit_verdicts: verdicts,
            references,
        })
    }

    fn sync_writing_links(
        &self,
        target: &LinkTarget,
        desired: &[WritingLinkJson],
        actor: &str,
    ) -> Result<()> {
        let old = self.links_of(target)?;
        let mut seen = BTreeSet::new();
        for link in desired {
            if !seen.insert(&link.passage_id) {
                return Err(validation("duplicate passage in a written part"));
            }
            if old.iter().any(|l| {
                l.passage.passage_id == link.passage_id
                    && l.link_type == link.link_type
                    && l.note == link.note
            }) {
                continue;
            }
            let passage = self.passage(&link.passage_id)?;
            if passage.stale {
                return Err(validation(format!(
                    "passage {} needs re-anchoring",
                    link.passage_id
                )));
            }
            if let Some(existing) = old.iter().find(|l| l.passage.passage_id == link.passage_id) {
                self.update_link(
                    &existing.link_id,
                    Some(&link.link_type),
                    Some(&link.note),
                    actor,
                )?;
            } else {
                self.create_link(&link.passage_id, target, &link.link_type, &link.note, actor)?;
            }
        }
        for old in old.iter().filter(|l| !seen.contains(&l.passage.passage_id)) {
            self.remove_link(&old.link_id, "Removed through writing JSON import", actor)?;
        }
        Ok(())
    }

    fn edit_writing_json(
        &self,
        doc: &WritingJson,
        before: &WritingJson,
        actor: &str,
    ) -> Result<()> {
        if doc.schema != "coding-studio.writing.v1" {
            return Err(validation("unsupported writing JSON schema"));
        }
        if doc.source_fingerprint != before.source_fingerprint {
            return Err(validation("this writing JSON belongs to another corpus or outdated theme identities; export it again"));
        }
        if doc.references != before.references {
            return Err(validation("source references changed; quotations and source identities are read-only. Export the current writing JSON again"));
        }
        let mut seen = BTreeSet::new();
        for theme in &doc.themes {
            if !seen.insert(&theme.theme_id) {
                return Err(validation("duplicate theme"));
            }
            if !before.themes.iter().any(|t| t.theme_id == theme.theme_id) {
                return Err(validation("unknown theme"));
            }
            for (key, part) in &theme.parts {
                self.set_theme_part(
                    &theme.theme_id,
                    key,
                    &PartEdit {
                        text: Some(part.text.clone()),
                        nature: Some(part.nature.clone()),
                    },
                    actor,
                )?;
                self.sync_writing_links(
                    &LinkTarget::ThemePart {
                        theme_id: theme.theme_id.clone(),
                        part: key.clone(),
                    },
                    &part.links,
                    actor,
                )?;
            }
            if let Some(f) = &theme.framework {
                if !f.theme_id.is_empty() && f.theme_id != theme.theme_id {
                    return Err(validation("framework theme id mismatch"));
                }
                self.set_framework_reading(&theme.theme_id, f, actor)?;
            }
        }
        if doc.theme_order != before.theme_order {
            self.reorder_themes(&doc.theme_order, actor)?;
        }
        let mut seen = BTreeSet::new();
        for q in &doc.questions {
            if !seen.insert(&q.rq_id) {
                return Err(validation("duplicate research question"));
            }
            self.set_question_synthesis(&q.rq_id, &q.answer, &q.limits, actor)?;
            self.sync_writing_links(
                &LinkTarget::RqPart {
                    rq_id: q.rq_id.clone(),
                },
                &q.links,
                actor,
            )?;
        }
        let mut seen = BTreeSet::new();
        for c in &doc.decisions {
            if !seen.insert(&c.case_id) {
                return Err(validation("duplicate documented decision"));
            }
            let old = self.case(&c.case_id)?;
            if c.interview_id != old.interview_id {
                return Err(validation("a documented decision cannot change interview"));
            }
            self.update_case(&c.case_id, c, actor)?;
        }
        let mut seen = BTreeSet::new();
        for v in &doc.audit_verdicts {
            if !seen.insert((v.audit_id, &v.claim_key)) {
                return Err(validation("duplicate audit verdict"));
            }
            let prior = before
                .audit_verdicts
                .iter()
                .find(|p| p.audit_id == v.audit_id && p.claim_key == v.claim_key)
                .ok_or_else(|| validation("unknown audit claim"))?;
            if v.verdict == prior.verdict && v.note == prior.note {
                continue;
            }
            let verdict = v
                .verdict
                .as_deref()
                .ok_or_else(|| validation("use a supported, to_nuance or to_withdraw verdict"))?;
            let audit = self.final_audit(v.audit_id)?;
            let claim = audit
                .claims
                .iter()
                .find(|c| c.claim_key == v.claim_key)
                .ok_or_else(|| validation("unknown audit claim"))?;
            if claim.chain_sha256 != v.chain_sha256 {
                return Err(validation("audit claim evidence changed; review the new evidence before importing its verdict"));
            }
            if verdict == "supported" && !claim.gaps.is_empty() {
                return Err(validation(
                    "a claim with evidence gaps cannot be marked supported",
                ));
            }
            if verdict != "supported" && v.note.trim().is_empty() {
                return Err(validation("a qualified or withdrawn claim needs a note"));
            }
            self.set_audit_verdict(v.audit_id, &v.claim_key, verdict, &v.note, actor)?;
        }
        Ok(())
    }

    fn writing_exchange_plan(
        &self,
        doc: &WritingJson,
        actor: &str,
    ) -> Result<(Store, Value, i64, String)> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?;
        let head: String = self.conn.query_row(
            "SELECT COALESCE((SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1),'')",
            [],
            |r| r.get(0),
        )?;
        let mut isolated = Store::open_in_memory()?;
        Backup::new(&self.conn, &mut isolated.conn)?.run_to_completion(
            256,
            Duration::from_millis(5),
            None,
        )?;
        let before = isolated.export_writing_json()?;
        isolated.edit_writing_json(doc, &before, actor)?;
        let after = isolated.export_writing_json()?;
        let mut changes = Vec::new();
        let mut add = |id: String, old: Value, new: Value| {
            if old != new {
                changes.push(json!({"code_id":id,"before":old,"after":new}));
            }
        };
        add(
            "theme_order".into(),
            json!(before.theme_order),
            json!(after.theme_order),
        );
        for t in &after.themes {
            let old = before
                .themes
                .iter()
                .find(|o| o.theme_id == t.theme_id)
                .unwrap();
            for (key, part) in &t.parts {
                add(
                    format!("{}:{key}", t.theme_id),
                    json!(old.parts[key]),
                    json!(part),
                );
            }
            add(
                format!("{}:framework", t.theme_id),
                json!(old.framework),
                json!(t.framework),
            );
        }
        for q in &after.questions {
            add(
                format!("question:{}", q.rq_id),
                json!(before.questions.iter().find(|o| o.rq_id == q.rq_id)),
                json!(q),
            );
        }
        for d in &after.decisions {
            add(
                format!("decision:{}", d.case_id),
                json!(before.decisions.iter().find(|o| o.case_id == d.case_id)),
                json!(d),
            );
        }
        for v in &after.audit_verdicts {
            let old = before
                .audit_verdicts
                .iter()
                .find(|o| o.audit_id == v.audit_id && o.claim_key == v.claim_key);
            // Evidence hashes may change when text is edited, but that does not
            // itself import an audit verdict.
            let edited = old.is_none_or(|o| o.verdict != v.verdict || o.note != v.note);
            if edited {
                add(
                    format!("audit:{}:{}", v.audit_id, v.claim_key),
                    json!(old),
                    json!(v),
                );
            }
        }
        let digest = fingerprint(&json!({"before":before,"input":doc,"audit_head":head}));
        let replacements = changes.iter().filter(|c| !c["before"].is_null()).count();
        let preview = json!({"digest":digest,"count":changes.len(),"replacements":replacements,"changes":changes});
        Ok((isolated, preview, version, head))
    }

    pub fn preview_writing_json(&self, doc: &WritingJson, actor: &str) -> Result<Value> {
        Ok(self.writing_exchange_plan(doc, actor)?.1)
    }

    pub fn import_writing_json(
        &mut self,
        doc: &WritingJson,
        expected_digest: &str,
        replace_existing: bool,
        actor: &str,
    ) -> Result<usize> {
        let (isolated, preview, version, head) = self.writing_exchange_plan(doc, actor)?;
        if preview["digest"].as_str() != Some(expected_digest) {
            return Err(guard_err(
                "Writing changed since preview; preview the JSON again",
            ));
        }
        if preview["replacements"].as_u64().unwrap_or(0) > 0 && !replace_existing {
            return Err(guard_err(
                "Confirm replacement of existing writing before importing",
            ));
        }
        let count = preview["count"].as_u64().unwrap_or(0) as usize;
        if count == 0 {
            return Ok(0);
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current_version: i64 = tx.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        let current_head: String = tx.query_row(
            "SELECT COALESCE((SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1),'')",
            [],
            |r| r.get(0),
        )?;
        if version != current_version || head != current_head {
            return Err(guard_err(
                "Database changed while preparing the import; preview it again",
            ));
        }
        for table in TABLES {
            let (columns, pk, old) = rows(&tx, table)?;
            let (_, _, new) = rows(&isolated.conn, table)?;
            let where_sql = pk
                .iter()
                .enumerate()
                .map(|(n, i)| format!("{}=?{}", quote(&columns[*i]), n + 1))
                .collect::<Vec<_>>()
                .join(" AND ");
            for (key, row) in old.iter().filter(|(key, _)| !new.contains_key(*key)) {
                anyhow::ensure!(
                    *table == "case_units",
                    "unexpected deletion from {table} ({key})"
                );
                tx.execute(
                    &format!("DELETE FROM {} WHERE {where_sql}", quote(table)),
                    params_from_iter(pk.iter().map(|i| &row[*i])),
                )?;
            }
            for (key, row) in &new {
                if old.get(key) == Some(row) {
                    continue;
                }
                if old.contains_key(key) {
                    let set_sql = columns
                        .iter()
                        .enumerate()
                        .map(|(i, col)| format!("{}=?{}", quote(col), i + 1))
                        .collect::<Vec<_>>()
                        .join(",");
                    let where_sql = pk
                        .iter()
                        .enumerate()
                        .map(|(n, i)| format!("{}=?{}", quote(&columns[*i]), columns.len() + n + 1))
                        .collect::<Vec<_>>()
                        .join(" AND ");
                    let values = row.iter().chain(pk.iter().map(|i| &row[*i]));
                    tx.execute(
                        &format!("UPDATE {} SET {set_sql} WHERE {where_sql}", quote(table)),
                        params_from_iter(values),
                    )?;
                } else {
                    let params = (1..=columns.len())
                        .map(|i| format!("?{i}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    tx.execute(
                        &format!(
                            "INSERT INTO {} ({}) VALUES ({params})",
                            quote(table),
                            columns
                                .iter()
                                .map(|c| quote(c))
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                        params_from_iter(row.iter()),
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(count)
    }
}
