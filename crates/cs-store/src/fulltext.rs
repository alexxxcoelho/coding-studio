//! V2, F-55: full-text search across the transcripts.
//!
//! The index is in memory (words of every current unit, accents and case
//! folded, with the position of each word in the original text). It is
//! rebuilt when a segmentation or a correction changes (fingerprint of the
//! current segmentations and of the latest correction of each interview).
//! SQLite FTS5 is bundled with rusqlite but is not used: the highlights need
//! exact positions in the original text and the corpus is small (tens of
//! interviews), so a scan of the folded words is both simpler and exact.
//!
//! Two modes: `all_words` (every word of the query is a word of the turn, in
//! any order) and `phrase` (the words follow each other, punctuation and case
//! ignored). A word matches as a whole word unless `prefix` is set (then the
//! words of `all_words`, or the last word of a phrase, may be a beginning).
//! Interviewer turns are searched only with `include_interviewer`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::Result;
use cs_model::passage::{scalar_to_utf16, slice_scalars};
use cs_model::{sha256_hex, SpeakerRole};
use serde::{Deserialize, Serialize};

use crate::textfold::{fold_text, folded_words, word_ranges};
use crate::{validation, Store};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
const DEFAULT_CONTEXT: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    AllWords,
    Phrase,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchRequest {
    pub q: String,
    #[serde(default)]
    pub mode: SearchMode,
    /// Words may be the beginning of a word (the last word only, in a phrase).
    #[serde(default)]
    pub prefix: bool,
    /// Also search the interviewer's turns.
    #[serde(default)]
    pub include_interviewer: bool,
    /// Restrict to these interviews.
    #[serde(default)]
    pub interviews: Vec<String>,
    /// Restrict to the interviews whose attribute has this value (both or none).
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    /// `Some(true)`: keep the turns where at least one match lies in a coded
    /// passage; `Some(false)`: where at least one match lies in no coded passage.
    #[serde(default)]
    pub coded: Option<bool>,
    /// Keep the turns where a match lies in a passage with this code.
    #[serde(default)]
    pub code: Option<String>,
    /// With `code`: also the descendants of the code (default true).
    #[serde(default = "yes")]
    pub descendants: bool,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
    /// Characters of context on each side of the first match of the excerpt.
    #[serde(default)]
    pub context: Option<usize>,
}

fn yes() -> bool {
    true
}

impl Default for SearchRequest {
    fn default() -> Self {
        SearchRequest {
            q: String::new(),
            mode: SearchMode::AllWords,
            prefix: false,
            include_interviewer: false,
            interviews: vec![],
            attribute: None,
            value: None,
            coded: None,
            code: None,
            descendants: true,
            limit: None,
            offset: 0,
            context: None,
        }
    }
}

/// A match in the original text of a turn: Unicode scalar positions (end
/// excluded) and the same in UTF-16 units, as a browser counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub start_utf16: usize,
    pub end_utf16: usize,
}

/// A window of the turn around the first match, with the highlights relative
/// to the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Excerpt {
    /// Scalar position of the window in the turn.
    pub offset: usize,
    pub text: String,
    pub highlights: Vec<Highlight>,
    pub truncated_before: bool,
    pub truncated_after: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Highlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Neighbour {
    pub unit_id: String,
    pub role: String,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub interview_id: String,
    pub unit_id: String,
    pub seq: u32,
    /// `respondent`, `interviewer` or `unknown`.
    pub role: String,
    pub speaker: String,
    pub text: String,
    pub matches: Vec<Span>,
    pub excerpt: Excerpt,
    pub previous: Option<Neighbour>,
    pub next: Option<Neighbour>,
    /// At least one match lies in a coded passage.
    pub coded: bool,
    /// Matches that lie in no coded passage.
    pub uncovered_matches: usize,
    /// Coded passages that overlap a match, and their codes.
    pub passage_ids: Vec<String>,
    pub code_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterviewHits {
    pub interview_id: String,
    /// Turns that match.
    pub units: usize,
    /// Matches in those turns.
    pub hits: usize,
    /// False for an interview that does not mention the query. "Not
    /// mentioned" describes the transcript, never the reality.
    pub mentioned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub query: String,
    pub mode: SearchMode,
    /// The folded words searched.
    pub words: Vec<String>,
    pub total_units: usize,
    pub total_hits: usize,
    pub truncated: bool,
    /// Every interview kept by the filters, in id order, mentioned or not.
    pub interviews: Vec<InterviewHits>,
    pub items: Vec<Hit>,
}

/// One current unit as the search sees it.
pub(crate) struct IndexedUnit {
    interview_id: String,
    unit_id: String,
    seq: u32,
    role: SpeakerRole,
    speaker: String,
    text: String,
    /// Folded words and the scalar range of each in the original text.
    words: Vec<(String, usize, usize)>,
}

pub(crate) struct FtIndex {
    /// interview id to its units in segmentation order.
    by_interview: BTreeMap<String, Vec<IndexedUnit>>,
}

fn role_str(r: SpeakerRole) -> &'static str {
    match r {
        SpeakerRole::Interviewer => "interviewer",
        SpeakerRole::Respondent => "respondent",
        SpeakerRole::Unknown => "unknown",
    }
}

fn index_unit(interview_id: &str, u: &cs_model::Unit) -> IndexedUnit {
    let f = fold_text(&u.text);
    let words = word_ranges(&f.chars)
        .into_iter()
        .map(|(a, b)| {
            (
                f.chars[a..b].iter().collect::<String>(),
                f.src[a],
                f.src[b - 1] + 1,
            )
        })
        .collect();
    IndexedUnit {
        interview_id: interview_id.to_string(),
        unit_id: u.unit_id.clone(),
        seq: u.seq,
        role: u.role,
        speaker: u.speaker_raw.clone(),
        text: u.text.clone(),
        words,
    }
}

impl Store {
    /// What the index depends on: the current segmentation of every interview
    /// and the latest correction of each.
    fn search_fingerprint(&self) -> Result<String> {
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
        Ok(sha256_hex(parts.join("\n").as_bytes()))
    }

    fn search_index(&self) -> Result<Arc<FtIndex>> {
        let fingerprint = self.search_fingerprint()?;
        {
            let cache = self.ft_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((fp, idx)) = cache.as_ref() {
                if *fp == fingerprint {
                    return Ok(Arc::clone(idx));
                }
            }
        }
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT interview_id FROM interviews ORDER BY interview_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut by_interview = BTreeMap::new();
        for iv in ids {
            if self.current_seg_id(&iv)?.is_none() {
                continue;
            }
            let mut units: Vec<_> = self.current_units(&iv)?.into_values().collect();
            units.sort_by_key(|u| u.seq);
            by_interview.insert(
                iv.clone(),
                units.iter().map(|u| index_unit(&iv, u)).collect(),
            );
        }
        let idx = Arc::new(FtIndex { by_interview });
        let mut cache = self.ft_cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some((fingerprint, Arc::clone(&idx)));
        Ok(idx)
    }

    /// Searches the transcripts. See the module documentation.
    pub fn search_text(&self, req: &SearchRequest) -> Result<SearchResult> {
        let words = folded_words(&req.q);
        if words.is_empty() {
            return Err(validation("give at least one word to search"));
        }
        // Filters on interviews.
        for id in &req.interviews {
            self.require_interview(id)?;
        }
        let by_attr = self.interview_filter(req.attribute.as_deref(), req.value.as_deref())?;
        let wanted: Option<BTreeSet<&str>> = if req.interviews.is_empty() {
            None
        } else {
            Some(req.interviews.iter().map(String::as_str).collect())
        };
        // Passage coverage, only when a coding filter is used or the hits
        // are described (always: cheap).
        let coverage = self.coded_spans()?;
        let code_filter: Option<BTreeSet<String>> = match req.code.as_deref() {
            Some(c) => {
                let root = self.resolve_code(c)?;
                let mut set = BTreeSet::new();
                set.insert(root.code_id.clone());
                if req.descendants {
                    let nodes = self.raw_nodes()?;
                    let mut grew = true;
                    while grew {
                        grew = false;
                        for n in &nodes {
                            if let Some(p) = &n.parent_id {
                                if set.contains(p) && set.insert(n.code_id.clone()) {
                                    grew = true;
                                }
                            }
                        }
                    }
                }
                Some(set)
            }
            None => None,
        };

        let index = self.search_index()?;
        let context = req.context.unwrap_or(DEFAULT_CONTEXT);
        let mut interviews = Vec::new();
        let mut all_hits: Vec<Hit> = Vec::new();
        let mut total_hits = 0usize;
        for (iv, units) in &index.by_interview {
            if wanted.as_ref().is_some_and(|w| !w.contains(iv.as_str()))
                || by_attr.as_ref().is_some_and(|s| !s.contains(iv))
            {
                continue;
            }
            let (mut n_units, mut n_hits) = (0usize, 0usize);
            for (pos, u) in units.iter().enumerate() {
                if u.role == SpeakerRole::Interviewer && !req.include_interviewer {
                    continue;
                }
                let spans = find_matches(u, &words, req.mode, req.prefix);
                if spans.is_empty() {
                    continue;
                }
                // Coverage of each match by the coded passages of the turn.
                let passages = coverage.get(&(u.interview_id.clone(), u.unit_id.clone()));
                let mut covered = 0usize;
                let mut passage_ids = BTreeSet::new();
                let mut code_ids = BTreeSet::new();
                let mut code_hit = code_filter.is_none();
                for (s, e) in &spans {
                    let mut any = false;
                    for p in passages.into_iter().flatten() {
                        if p.start < *e && *s < p.end {
                            any = true;
                            passage_ids.insert(p.passage_id.clone());
                            code_ids.extend(p.codes.iter().cloned());
                            if let Some(f) = &code_filter {
                                if p.codes.iter().any(|c| f.contains(c)) {
                                    code_hit = true;
                                }
                            }
                        }
                    }
                    if any {
                        covered += 1;
                    }
                }
                let uncovered = spans.len() - covered;
                let keep = code_hit
                    && match req.coded {
                        None => true,
                        Some(true) => covered > 0,
                        Some(false) => uncovered > 0,
                    };
                if !keep {
                    continue;
                }
                n_units += 1;
                n_hits += spans.len();
                let neighbour = |i: Option<usize>| -> Option<Neighbour> {
                    i.and_then(|i| units.get(i)).map(|n| Neighbour {
                        unit_id: n.unit_id.clone(),
                        role: role_str(n.role).into(),
                        speaker: n.speaker.clone(),
                        text: n.text.clone(),
                    })
                };
                all_hits.push(Hit {
                    interview_id: u.interview_id.clone(),
                    unit_id: u.unit_id.clone(),
                    seq: u.seq,
                    role: role_str(u.role).into(),
                    speaker: u.speaker.clone(),
                    text: u.text.clone(),
                    matches: spans
                        .iter()
                        .map(|(s, e)| Span {
                            start: *s,
                            end: *e,
                            start_utf16: scalar_to_utf16(&u.text, *s).unwrap_or(*s),
                            end_utf16: scalar_to_utf16(&u.text, *e).unwrap_or(*e),
                        })
                        .collect(),
                    excerpt: excerpt(&u.text, &spans, context),
                    previous: neighbour(pos.checked_sub(1)),
                    next: neighbour(Some(pos + 1)),
                    coded: covered > 0,
                    uncovered_matches: uncovered,
                    passage_ids: passage_ids.into_iter().collect(),
                    code_ids: code_ids.into_iter().collect(),
                });
            }
            total_hits += n_hits;
            interviews.push(InterviewHits {
                interview_id: iv.clone(),
                units: n_units,
                hits: n_hits,
                mentioned: n_units > 0,
            });
        }
        let total_units = all_hits.len();
        let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let items: Vec<Hit> = all_hits.into_iter().skip(req.offset).take(limit).collect();
        Ok(SearchResult {
            query: req.q.trim().to_string(),
            mode: req.mode,
            words,
            total_units,
            total_hits,
            truncated: total_units > req.offset + items.len(),
            interviews,
            items,
        })
    }

    /// Coded passages (an active coding other than the reserved code, not
    /// stale) by (interview, unit).
    fn coded_spans(&self) -> Result<BTreeMap<(String, String), Vec<CodedSpan>>> {
        let mut out: BTreeMap<(String, String), Vec<CodedSpan>> = BTreeMap::new();
        for p in self.passages(&Default::default())? {
            if p.stale {
                continue;
            }
            let codes: Vec<String> = p
                .codings
                .iter()
                .filter(|c| !c.system)
                .map(|c| c.code_id.clone())
                .collect();
            if codes.is_empty() {
                continue;
            }
            out.entry((p.interview_id.clone(), p.unit_id.clone()))
                .or_default()
                .push(CodedSpan {
                    passage_id: p.passage_id.clone(),
                    start: p.start,
                    end: p.end,
                    codes,
                });
        }
        Ok(out)
    }
}

struct CodedSpan {
    passage_id: String,
    start: usize,
    end: usize,
    codes: Vec<String>,
}

fn word_matches(word: &str, q: &str, prefix: bool) -> bool {
    if prefix {
        word.starts_with(q)
    } else {
        word == q
    }
}

/// Scalar ranges of the matches in a turn; empty when the turn does not match.
fn find_matches(
    u: &IndexedUnit,
    query: &[String],
    mode: SearchMode,
    prefix: bool,
) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    match mode {
        SearchMode::AllWords => {
            for q in query {
                let found: Vec<(usize, usize)> = u
                    .words
                    .iter()
                    .filter(|(w, ..)| word_matches(w, q, prefix))
                    .map(|(_, s, e)| (*s, *e))
                    .collect();
                if found.is_empty() {
                    return Vec::new();
                }
                spans.extend(found);
            }
        }
        SearchMode::Phrase => {
            let n = query.len();
            if u.words.len() < n {
                return Vec::new();
            }
            for i in 0..=(u.words.len() - n) {
                let ok = query.iter().enumerate().all(|(k, q)| {
                    let w = &u.words[i + k].0;
                    if k + 1 == n {
                        word_matches(w, q, prefix)
                    } else {
                        w == q
                    }
                });
                if ok {
                    spans.push((u.words[i].1, u.words[i + n - 1].2));
                }
            }
        }
    }
    spans.sort_unstable();
    spans.dedup();
    // Merge overlapping or touching ranges (the same word asked twice).
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s < last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// A window of about `2 * context` characters around the first match, cut on
/// word boundaries: a word that the window would cut is left out whole.
fn excerpt(text: &str, spans: &[(usize, usize)], context: usize) -> Excerpt {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let (first_s, first_e) = spans[0];
    let mut start = first_s.saturating_sub(context);
    if start > 0 && !chars[start - 1].is_whitespace() && !chars[start].is_whitespace() {
        while start < first_s && !chars[start].is_whitespace() {
            start += 1;
        }
        while start < first_s && chars[start].is_whitespace() {
            start += 1;
        }
    }
    let mut end = (first_e + context).min(len);
    if end < len && end > 0 && !chars[end - 1].is_whitespace() && !chars[end].is_whitespace() {
        while end > first_e && !chars[end - 1].is_whitespace() {
            end -= 1;
        }
        while end > first_e && chars[end - 1].is_whitespace() {
            end -= 1;
        }
    }
    let window = slice_scalars(text, start, end)
        .unwrap_or_default()
        .to_string();
    let highlights = spans
        .iter()
        .filter(|(s, e)| *s >= start && *e <= end)
        .map(|(s, e)| Highlight {
            start: s - start,
            end: e - start,
        })
        .collect();
    Excerpt {
        offset: start,
        text: window,
        highlights,
        truncated_before: start > 0,
        truncated_after: end < len,
    }
}

#[cfg(test)]
mod excerpt_tests {
    use super::excerpt;

    #[test]
    fn the_window_never_cuts_a_word() {
        let text = "Premièrement les fournisseurs étrangers imposent le marquage, ensuite le coût monte et les administrateurs hésitent encore longtemps.";
        let at = text.find("coût").unwrap();
        let s = text[..at].chars().count();
        let e = s + 4;
        let x = excerpt(text, &[(s, e)], 15);
        assert!(x.truncated_before && x.truncated_after);
        for w in x.text.split_whitespace() {
            assert!(
                text.split_whitespace().any(|t| t == w),
                "mot coupé : {w:?} dans {:?}",
                x.text
            );
        }
        let h = &x.highlights[0];
        let hit: String = x.text.chars().skip(h.start).take(h.end - h.start).collect();
        assert_eq!(hit, "coût");
        assert_eq!(x.text, "ensuite le coût monte et les");
    }

    #[test]
    fn a_short_text_is_given_whole() {
        let x = excerpt("Un coût élevé.", &[(3, 7)], 60);
        assert_eq!(x.text, "Un coût élevé.");
        assert!(!x.truncated_before && !x.truncated_after);
    }
}
