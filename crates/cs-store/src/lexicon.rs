//! V1 lot 3: the lexical workbench over the stored corpus.
//!
//! The store gives `cs-lex` the current text of the units (corrections
//! applied), keeps the index in memory and rebuilds it when the fingerprint of
//! the transcripts changes: a new segmentation hash, a new correction or a new
//! language setting. Nothing here writes to the database except the language
//! setting.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::Result;
use cs_lex::{
    Corpus, CorpusUnit, KeywordOptions, KeywordTable, KwicQuery, KwicResult, Lang, LexIndex,
    LexicalPanel, PassageText, SimilarHit, Speakers, TermFreqRequest, TermFreqRow, TopTermsRow,
};
use cs_model::passage::Passage;
use cs_model::{sha256_hex, SpeakerRole};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::passages::PassageFilter;
use crate::{append_event, from_json, now, to_json, validation, Store};

/// Settings key of the lexical configuration.
pub const LEXICON_CONFIG_KEY: &str = "lexicon_config";

/// Language of the lexical workbench: the project's, and the interviews that
/// differ from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexiconConfig {
    /// `fr` (default), `en` or `de`.
    pub language: String,
    /// Interview id to language, for the interviews that are not in the
    /// project language.
    #[serde(default)]
    pub interview_languages: BTreeMap<String, String>,
}

impl Default for LexiconConfig {
    fn default() -> Self {
        LexiconConfig {
            language: "fr".into(),
            interview_languages: BTreeMap::new(),
        }
    }
}

impl LexiconConfig {
    pub fn default_lang(&self) -> Lang {
        Lang::parse(&self.language).unwrap_or(Lang::Fr)
    }
}

/// One similar unit, with the codes the passages of that unit already carry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimilarItem {
    #[serde(flatten)]
    pub hit: SimilarHit,
    /// Codes (id and name) of the passages of the unit.
    pub coded_with: Vec<CodeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeRef {
    pub code_id: String,
    pub name: String,
}

/// What a passage query reads.
#[derive(Debug, Clone, Default)]
pub struct SimilarOptions {
    pub k: usize,
    pub min_score: f64,
    pub interviews: Option<BTreeSet<String>>,
    /// Keep the unit of the passage among the results.
    pub include_own_unit: bool,
    /// Leave out the units that already carry this code (id, name, alias or
    /// token): what is left is what coding may have missed.
    pub without_code: Option<String>,
}

impl Store {
    pub fn lexicon_config(&self) -> Result<LexiconConfig> {
        match self.setting(LEXICON_CONFIG_KEY)? {
            Some(raw) => from_json(&raw),
            None => Ok(LexiconConfig::default()),
        }
    }

    /// Validates, stores and audits the language configuration.
    pub fn set_lexicon_config(&self, cfg: &LexiconConfig, actor: &str) -> Result<LexiconConfig> {
        if Lang::parse(&cfg.language).is_none() {
            return Err(validation(format!(
                "unknown language {:?}: use fr, en or de",
                cfg.language
            )));
        }
        let mut stored = LexiconConfig {
            language: Lang::parse(&cfg.language)
                .map(|l| l.code().to_string())
                .unwrap_or_default(),
            interview_languages: BTreeMap::new(),
        };
        for (iv, lang) in &cfg.interview_languages {
            if self.interview(iv)?.is_none() {
                return Err(validation(format!("unknown interview {iv}")));
            }
            let l = Lang::parse(lang).ok_or_else(|| {
                validation(format!(
                    "unknown language {lang:?} for {iv}: use fr, en or de"
                ))
            })?;
            if l.code() != stored.language {
                stored
                    .interview_languages
                    .insert(iv.clone(), l.code().into());
            }
        }
        let before = self.lexicon_config()?;
        if before == stored {
            return Ok(stored);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![LEXICON_CONFIG_KEY, to_json(&stored), now()],
        )?;
        append_event(
            &tx,
            actor,
            "lexicon.config.update",
            None,
            Some(LEXICON_CONFIG_KEY),
            json!({"before": before, "after": stored}),
        )?;
        tx.commit()?;
        Ok(stored)
    }

    /// Digest of what the index depends on: the current segmentation of every
    /// interview, the corrections and the language configuration.
    pub fn lexical_fingerprint(&self) -> Result<String> {
        let mut parts: Vec<String> = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT s.interview_id, s.hash FROM segmentations s
             WHERE s.seg_id = (SELECT MAX(seg_id) FROM segmentations WHERE interview_id = s.interview_id)
             ORDER BY s.interview_id",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (iv, h) = row?;
            parts.push(format!("s:{iv}:{h}"));
        }
        let mut stmt = self.conn.prepare(
            "SELECT interview_id, MAX(correction_id) FROM unit_corrections GROUP BY interview_id ORDER BY interview_id",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (iv, c) = row?;
            parts.push(format!("c:{iv}:{c}"));
        }
        parts.push(format!("l:{}", to_json(&self.lexicon_config()?)));
        Ok(sha256_hex(parts.join("\n").as_bytes()))
    }

    fn lexical_corpus(&self, cfg: &LexiconConfig) -> Result<Corpus> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT interview_id FROM interviews ORDER BY interview_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut units = Vec::new();
        for iv in ids {
            let mut current: Vec<_> = self.current_units(&iv)?.into_values().collect();
            current.sort_by_key(|u| u.seq);
            for u in current {
                units.push(CorpusUnit {
                    interview_id: iv.clone(),
                    unit_id: u.unit_id,
                    seq: u.seq as usize,
                    respondent: u.role != SpeakerRole::Interviewer,
                    text: u.text,
                });
            }
        }
        Ok(Corpus {
            default_lang: cfg.default_lang(),
            interview_langs: cfg
                .interview_languages
                .iter()
                .filter_map(|(k, v)| Lang::parse(v).map(|l| (k.clone(), l)))
                .collect(),
            units,
        })
    }

    /// The index of the current corpus: cached, rebuilt when the transcripts,
    /// their corrections or the language configuration changed.
    pub fn lexical_index(&self) -> Result<Arc<LexIndex>> {
        let fingerprint = self.lexical_fingerprint()?;
        {
            let cache = self.lex_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((fp, idx)) = cache.as_ref() {
                if *fp == fingerprint {
                    return Ok(Arc::clone(idx));
                }
            }
        }
        let cfg = self.lexicon_config()?;
        let idx = Arc::new(LexIndex::build(&self.lexical_corpus(&cfg)?));
        let mut cache = self.lex_cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some((fingerprint, Arc::clone(&idx)));
        Ok(idx)
    }

    /// Interview to value of an attribute (`None` when not filled), for every
    /// interview of the library.
    pub fn attribute_groups(&self, attribute: &str) -> Result<BTreeMap<String, Option<String>>> {
        if self.project_config()?.attribute(attribute).is_none() {
            return Err(validation(format!("unknown attribute {attribute}")));
        }
        let values = self.all_attribute_values()?;
        let mut out: BTreeMap<String, Option<String>> = BTreeMap::new();
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT interview_id FROM interviews ORDER BY interview_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for iv in ids {
            out.insert(iv, None);
        }
        for (iv, m) in values {
            if let Some(v) = m.get(attribute) {
                out.insert(iv, Some(v.clone()));
            }
        }
        Ok(out)
    }

    /// Interviews selected by `attribute` and `value` (both or none).
    pub fn interview_filter(
        &self,
        attribute: Option<&str>,
        value: Option<&str>,
    ) -> Result<Option<BTreeSet<String>>> {
        match (attribute, value) {
            (None, None) => Ok(None),
            (Some(a), Some(v)) => Ok(Some(self.interviews_with_attribute(a, v)?)),
            _ => Err(validation(
                "give both attribute and value to filter by an attribute, or neither",
            )),
        }
    }

    // ------------------------------------------------------------- queries

    pub fn lexical_kwic(&self, q: &KwicQuery) -> Result<KwicResult> {
        Ok(self.lexical_index()?.kwic(q))
    }

    /// Keywords of the passages that carry no active code (`scope`
    /// `highlights`) or of every passage (`passages`).
    pub fn lexical_keywords(
        &self,
        scope: &str,
        interviews: Option<&BTreeSet<String>>,
        opts: &KeywordOptions,
    ) -> Result<KeywordTable> {
        let passages = match scope {
            "highlights" => self.highlights(None)?,
            "passages" => self.passages(&PassageFilter::default())?,
            other => {
                return Err(validation(format!(
                    "unknown scope {other:?}: use highlights or passages"
                )))
            }
        };
        let texts: Vec<PassageText> = passages
            .into_iter()
            .filter(|p| interviews.is_none_or(|set| set.contains(&p.interview_id)))
            .map(|p| PassageText {
                passage_id: p.passage_id,
                interview_id: p.interview_id,
                text: p.text,
            })
            .collect();
        Ok(self.lexical_index()?.keyword_table(&texts, opts))
    }

    pub fn lexical_term_frequency(
        &self,
        attribute: &str,
        terms: &[String],
        stem: bool,
        speakers: Speakers,
    ) -> Result<Vec<TermFreqRow>> {
        let groups = self.attribute_groups(attribute)?;
        Ok(self.lexical_index()?.term_frequency(&TermFreqRequest {
            terms: terms.to_vec(),
            stem,
            groups,
            speakers,
        }))
    }

    pub fn lexical_top_terms(
        &self,
        attribute: &str,
        speakers: Speakers,
        n: usize,
    ) -> Result<Vec<TopTermsRow>> {
        let groups = self.attribute_groups(attribute)?;
        Ok(self.lexical_index()?.top_terms(&groups, speakers, n))
    }

    /// Keyphrases, stems and a provisional label for a passage.
    pub fn passage_lexical(&self, passage_id: &str) -> Result<LexicalPanel> {
        let p = self.passage(passage_id)?;
        let lang = self.lexicon_config().map(|c| {
            c.interview_languages
                .get(&p.interview_id)
                .and_then(|l| Lang::parse(l))
                .unwrap_or_else(|| c.default_lang())
        })?;
        Ok(cs_lex::analyse_text(&p.text, lang))
    }

    /// Respondent units whose vocabulary is close to a passage.
    pub fn similar_to_passage(
        &self,
        passage_id: &str,
        opts: &SimilarOptions,
    ) -> Result<Vec<SimilarItem>> {
        let p = self.passage(passage_id)?;
        let idx = self.lexical_index()?;
        let lang = idx.language_of(&p.interview_id);
        let wanted = if opts.without_code.is_some() {
            (opts.k * 5).clamp(opts.k, 500)
        } else {
            opts.k
        };
        let hits = idx.similar(&cs_lex::SimilarQuery {
            text: p.text.clone(),
            lang,
            k: wanted,
            exclude: (!opts.include_own_unit).then(|| (p.interview_id.clone(), p.unit_id.clone())),
            interviews: opts.interviews.clone(),
            min_score: opts.min_score,
        });
        self.annotate_similar(hits, opts)
    }

    /// Respondent units whose vocabulary is close to a unit.
    pub fn similar_to_unit(
        &self,
        interview_id: &str,
        unit_id: &str,
        opts: &SimilarOptions,
    ) -> Result<Vec<SimilarItem>> {
        self.require_interview(interview_id)?;
        let idx = self.lexical_index()?;
        let wanted = if opts.without_code.is_some() {
            (opts.k * 5).clamp(opts.k, 500)
        } else {
            opts.k
        };
        let hits = idx
            .similar_to_unit(
                interview_id,
                unit_id,
                wanted,
                opts.interviews.clone(),
                opts.min_score,
            )
            .ok_or_else(|| crate::not_found(format!("unit {unit_id} of {interview_id}")))?;
        self.annotate_similar(hits, opts)
    }

    fn annotate_similar(
        &self,
        hits: Vec<SimilarHit>,
        opts: &SimilarOptions,
    ) -> Result<Vec<SimilarItem>> {
        let without = match &opts.without_code {
            Some(c) => Some(self.resolve_code(c)?.code_id),
            None => None,
        };
        let mut out = Vec::new();
        for hit in hits {
            let passages: Vec<Passage> = self.passages(&PassageFilter {
                interview_id: Some(hit.interview_id.clone()),
                unit_id: Some(hit.unit_id.clone()),
                ..Default::default()
            })?;
            let mut coded_with: Vec<CodeRef> = Vec::new();
            for p in &passages {
                for c in &p.codings {
                    if !coded_with.iter().any(|x| x.code_id == c.code_id) {
                        coded_with.push(CodeRef {
                            code_id: c.code_id.clone(),
                            name: c.code_name.clone(),
                        });
                    }
                }
            }
            if let Some(w) = &without {
                if coded_with.iter().any(|c| &c.code_id == w) {
                    continue;
                }
            }
            out.push(SimilarItem { hit, coded_with });
            if out.len() >= opts.k {
                break;
            }
        }
        Ok(out)
    }
}
