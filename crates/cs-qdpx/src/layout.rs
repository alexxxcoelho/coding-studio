//! Plain text of an interview and the offset map.
//!
//! The plain text of a source is the units of the interview, interviewer and
//! respondent turns alike, as the store holds them (corrections applied),
//! joined by [`SEPARATOR`]. Nothing is added to a unit's text. The offset map
//! gives, for each unit, its first and last position in that text. Positions
//! count Unicode scalar values from 0, end exclusive, the same convention as
//! passages, so that a passage `(unit, start, end)` is the selection
//! `(unit.start + start, unit.start + end)`.

use cs_model::passage::{scalar_len, slice_scalars};
use cs_model::{sha256_hex, SpeakerRole, Unit};
use serde::Serialize;

/// Between two units. Never part of a unit, so never codable.
pub const SEPARATOR: &str = "\n\n";

/// Where a unit sits in the plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitSpan {
    pub unit_id: String,
    #[serde(serialize_with = "role_str")]
    pub role: SpeakerRole,
    /// First position of the unit text.
    pub start: usize,
    /// Position after the last character of the unit text.
    pub end: usize,
}

fn role_str<S: serde::Serializer>(r: &SpeakerRole, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(match r {
        SpeakerRole::Interviewer => "interviewer",
        SpeakerRole::Respondent => "respondent",
        SpeakerRole::Unknown => "unknown",
    })
}

/// Plain text of one interview with its offset map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    pub text: String,
    pub spans: Vec<UnitSpan>,
}

impl SourceText {
    /// Builds the text and the map from the units in segmentation order.
    pub fn build(units: &[Unit]) -> SourceText {
        let mut text = String::new();
        let mut spans = Vec::with_capacity(units.len());
        let mut pos = 0usize;
        for (i, u) in units.iter().enumerate() {
            if i > 0 {
                text.push_str(SEPARATOR);
                pos += scalar_len(SEPARATOR);
            }
            let len = scalar_len(&u.text);
            spans.push(UnitSpan {
                unit_id: u.unit_id.clone(),
                role: u.role,
                start: pos,
                end: pos + len,
            });
            text.push_str(&u.text);
            pos += len;
        }
        SourceText { text, spans }
    }

    /// SHA-256 of the text (UTF-8).
    pub fn digest(&self) -> String {
        sha256_hex(self.text.as_bytes())
    }

    pub fn len(&self) -> usize {
        scalar_len(&self.text)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn span(&self, unit_id: &str) -> Option<&UnitSpan> {
        self.spans.iter().find(|s| s.unit_id == unit_id)
    }

    /// The text between two positions.
    pub fn slice(&self, start: usize, end: usize) -> Option<&str> {
        slice_scalars(&self.text, start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: &str, role: SpeakerRole, text: &str) -> Unit {
        Unit {
            unit_id: id.into(),
            seq: 1,
            speaker_raw: String::new(),
            role,
            timestamp: String::new(),
            text: text.into(),
            corrected: false,
        }
    }

    #[test]
    fn offsets_count_scalars_not_bytes() {
        let units = [
            unit("U1", SpeakerRole::Interviewer, "Salut 😀"),
            unit("U2", SpeakerRole::Respondent, "Été chaud"),
        ];
        let s = SourceText::build(&units);
        assert_eq!(s.text, "Salut 😀\n\nÉté chaud");
        assert_eq!(
            (
                s.spans[0].start,
                s.spans[0].end,
                s.spans[1].start,
                s.spans[1].end
            ),
            (0, 7, 9, 18)
        );
        assert_eq!(s.slice(9, 12), Some("Été"));
        assert_eq!(s.len(), 18);
    }

    #[test]
    fn digest_is_of_the_text() {
        let s = SourceText::build(&[unit("U1", SpeakerRole::Respondent, "abc")]);
        assert_eq!(s.digest(), sha256_hex(b"abc"));
        assert!(SourceText::build(&[]).is_empty());
    }
}
