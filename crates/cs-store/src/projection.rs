//! F-24: projection of a span of an old unit text onto a new unit text, by a
//! character-level diff. Positions are Unicode scalar values, end exclusive
//! (the convention of passages).
//!
//! The result is one of two things. `Unchanged`: every scalar of the span
//! survives the edit and the survivors are contiguous in the new text, so the
//! span selects exactly the same words there; it can be re-anchored without
//! asking. `Touched`: the edit falls inside the span (or deletes it); a
//! proposed span is given when one can be drawn (the span as it now reads,
//! with its inserted and replaced characters), and the researcher decides.

use similar::{capture_diff_slices, Algorithm, DiffOp};

/// What the diff says about one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanProjection {
    /// The same characters, in the new text at `start..end`.
    Unchanged { start: usize, end: usize },
    /// The edit touches the span. `proposal` is the span as it now reads, or
    /// `None` when nothing of it is left.
    Touched { proposal: Option<(usize, usize)> },
}

/// Where one old scalar went.
#[derive(Clone, Copy)]
enum Fate {
    /// Kept, at this position of the new text.
    Kept(usize),
    /// Deleted or replaced; the new text has `len` scalars at `at` in its place.
    Replaced { at: usize, len: usize },
}

fn fates(old: &[char], new: &[char]) -> Vec<Fate> {
    let mut out = vec![Fate::Replaced { at: 0, len: 0 }; old.len()];
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    out[old_index + k] = Fate::Kept(new_index + k);
                }
            }
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => {
                for k in 0..old_len {
                    out[old_index + k] = Fate::Replaced {
                        at: new_index,
                        len: 0,
                    };
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for k in 0..old_len {
                    out[old_index + k] = Fate::Replaced {
                        at: new_index,
                        len: new_len,
                    };
                }
            }
            DiffOp::Insert { .. } => {}
        }
    }
    out
}

/// Projects `start..end` of `old` onto `new`. The span must lie inside `old`
/// and be non-empty (the caller guarantees both).
pub fn project_span(old: &str, new: &str, start: usize, end: usize) -> SpanProjection {
    let old: Vec<char> = old.chars().collect();
    let new: Vec<char> = new.chars().collect();
    if start >= end || end > old.len() {
        return SpanProjection::Touched { proposal: None };
    }
    let fate = fates(&old, &new);
    let span = &fate[start..end];
    let mut kept_all = true;
    let mut contiguous = true;
    let mut prev: Option<usize> = None;
    for f in span {
        match f {
            Fate::Kept(n) => {
                if let Some(p) = prev {
                    contiguous &= *n == p + 1;
                }
                prev = Some(*n);
            }
            Fate::Replaced { .. } => kept_all = false,
        }
    }
    if kept_all && contiguous {
        if let Fate::Kept(first) = span[0] {
            let len = end - start;
            // Belt and braces: the survivors read the same.
            if new.get(first..first + len) == Some(&old[start..end]) {
                return SpanProjection::Unchanged {
                    start: first,
                    end: first + len,
                };
            }
        }
    }
    let new_start = match span[0] {
        Fate::Kept(n) => n,
        Fate::Replaced { at, .. } => at,
    };
    let new_end = match span[span.len() - 1] {
        Fate::Kept(n) => n + 1,
        Fate::Replaced { at, len } => at + len,
    };
    SpanProjection::Touched {
        proposal: (new_end > new_start).then_some((new_start, new_end)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_before_the_span_shifts_it() {
        let old = "Our staff had no RFID skills at all.";
        let new = "Honestly, our staff had no RFID skills at all.";
        let s = old.find("RFID skills").unwrap();
        let p = project_span(old, new, s, s + 11);
        let t = new.find("RFID skills").unwrap();
        assert_eq!(
            p,
            SpanProjection::Unchanged {
                start: t,
                end: t + 11
            }
        );
    }

    #[test]
    fn an_edit_after_the_span_leaves_it_in_place() {
        let old = "The tags paid back within a year.";
        let new = "The tags paid back within a year, roughly.";
        assert_eq!(
            project_span(old, new, 4, 8),
            SpanProjection::Unchanged { start: 4, end: 8 }
        );
    }

    #[test]
    fn an_edit_inside_the_span_touches_it_and_proposes_the_new_reading() {
        let old = "The tags paid back within a year.";
        let new = "The tags paid back within two years.";
        // "paid back within a year" -> "paid back within two years"
        let s = 9;
        let e = old.chars().count() - 1;
        match project_span(old, new, s, e) {
            SpanProjection::Touched {
                proposal: Some((a, b)),
            } => {
                let got: String = new.chars().skip(a).take(b - a).collect();
                assert!(got.starts_with("paid back within"), "{got:?}");
                assert!(got.contains("year"), "{got:?}");
            }
            other => panic!("expected a touched span with a proposal, got {other:?}"),
        }
    }

    #[test]
    fn a_deleted_span_has_no_proposal() {
        let old = "First part. Second part.";
        let new = "First part.";
        assert_eq!(
            project_span(old, new, 12, 24),
            SpanProjection::Touched { proposal: None }
        );
    }

    #[test]
    fn positions_count_scalars_not_bytes() {
        let old = "Café « prêt » 😀 fini";
        let new = "Un café « prêt » 😀 fini";
        let s = old.chars().position(|c| c == '«').unwrap();
        let e = old.chars().position(|c| c == '»').unwrap() + 1;
        let p = project_span(old, new, s, e);
        let ns = new.chars().position(|c| c == '«').unwrap();
        assert_eq!(
            p,
            SpanProjection::Unchanged {
                start: ns,
                end: ns + (e - s)
            }
        );
    }

    #[test]
    fn identical_texts_project_to_themselves() {
        assert_eq!(
            project_span("abc def", "abc def", 4, 7),
            SpanProjection::Unchanged { start: 4, end: 7 }
        );
    }

    #[test]
    fn an_insertion_in_the_middle_of_the_span_touches_it() {
        let old = "alpha beta";
        let new = "alpha big beta";
        // whole span 0..10 survives character by character but is no longer contiguous
        match project_span(old, new, 0, 10) {
            SpanProjection::Touched {
                proposal: Some((0, 14)),
            } => {}
            other => panic!("{other:?}"),
        }
    }
}
