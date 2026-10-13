//! Passages (spans), their anchors, and the code tree types (V1 lot 1).
//!
//! A passage is a selected part of a unit, possibly the whole unit. It stores
//! the unit, the start and end positions, the exact text, the hash of the
//! segmentation and a digest of the unit text it was anchored on, so that a
//! changed transcript can be detected (the passage is then flagged stale, it
//! is never moved silently).
//!
//! Position convention: Unicode scalar values (Rust `char`), counted from 0,
//! end exclusive. A browser counts UTF-16 code units; the conversion lives
//! here and refuses a position inside a surrogate pair.

use serde::{Deserialize, Serialize};

use crate::sha256_hex;

/// Stable id of the reserved system code "Plus tard" (marker to revisit).
pub const LATER_ID: &str = "LATER";

/// Display name of the reserved system code.
pub const LATER_NAME: &str = "Plus tard";

/// Number of Unicode scalar values of a text.
pub fn scalar_len(text: &str) -> usize {
    text.chars().count()
}

/// Digest of the unit text a passage is anchored on.
pub fn text_digest(text: &str) -> String {
    sha256_hex(text.as_bytes())
}

/// Byte index of the scalar position `pos` (`pos == len` gives `text.len()`).
fn byte_index(text: &str, pos: usize) -> Option<usize> {
    if pos == 0 {
        return Some(0);
    }
    let mut count = 0usize;
    for (i, _) in text.char_indices() {
        if count == pos {
            return Some(i);
        }
        count += 1;
    }
    (count == pos).then_some(text.len())
}

/// The text between two scalar positions, `None` when the range is not inside
/// the text or reversed. An empty range at a valid position gives `""`.
pub fn slice_scalars(text: &str, start: usize, end: usize) -> Option<&str> {
    if start > end {
        return None;
    }
    let a = byte_index(text, start)?;
    let b = byte_index(text, end)?;
    Some(&text[a..b])
}

/// Scalar position of a UTF-16 position of the same text. `None` past the end
/// or inside a surrogate pair.
pub fn utf16_to_scalar(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0usize;
    let mut scalars = 0usize;
    for c in text.chars() {
        if units == utf16 {
            return Some(scalars);
        }
        if units > utf16 {
            return None;
        }
        units += c.len_utf16();
        scalars += 1;
    }
    (units == utf16).then_some(scalars)
}

/// UTF-16 position of a scalar position. `None` past the end.
pub fn scalar_to_utf16(text: &str, scalar: usize) -> Option<usize> {
    let mut units = 0usize;
    for (i, c) in text.chars().enumerate() {
        if i == scalar {
            return Some(units);
        }
        units += c.len_utf16();
    }
    (scalar_len(text) == scalar).then_some(units)
}

/// Unit of the offsets sent by a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetUnit {
    /// Unicode scalar values (the server convention).
    #[default]
    Scalar,
    /// UTF-16 code units (what a browser's selection API reports).
    Utf16,
}

/// A code applied to a passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassageCoding {
    pub coding_id: i64,
    pub code_id: String,
    /// Display name of the code at read time.
    pub code_name: String,
    /// True for the reserved code "Plus tard".
    pub system: bool,
    /// Free note (used with "Plus tard").
    pub note: String,
    pub created_at: String,
    pub created_by: String,
}

/// A passage with its active codings. `kind` is `highlight` when no coding is
/// active and `coded` otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    pub passage_id: String,
    pub interview_id: String,
    pub unit_id: String,
    /// Scalar positions, end exclusive.
    pub start: usize,
    pub end: usize,
    /// The same positions in UTF-16 units of the unit text at read time
    /// (absent when the anchor is stale).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_utf16: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_utf16: Option<usize>,
    /// Exact text stored when the passage was made.
    pub text: String,
    pub whole_unit: bool,
    pub segmentation_hash: String,
    /// SHA-256 of the unit text the offsets refer to.
    pub anchor_digest: String,
    pub created_at: String,
    pub created_by: String,
    pub kind: String,
    /// True when the unit text no longer matches the anchor, or the unit left
    /// the current segmentation.
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    /// Text before and after the passage in the unit (up to 60 scalars each),
    /// empty when stale.
    pub before: String,
    pub after: String,
    pub codings: Vec<PassageCoding>,
}

/// A neighbouring turn, shown around a passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitBrief {
    pub unit_id: String,
    pub speaker: String,
    pub text: String,
}

/// A passage of a code with what the reader needs around it (F-44).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassageContext {
    pub passage: Passage,
    /// Speaker label of the unit (`RESP`, `INT`...).
    pub speaker: String,
    pub previous: Option<UnitBrief>,
    pub next: Option<UnitBrief>,
    /// Names of the other codes on the same passage, the reserved code
    /// included.
    pub other_codes: Vec<String>,
}

/// One alias of a code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeAlias {
    pub alias: String,
    /// `former_name`, `synonym`, `merge` or `legacy_token`.
    pub reason: String,
    pub created_at: String,
}

/// Counts of a code, never mixing passages and interviews. The roll-up
/// figures include the passages of all descendants (a passage counted once).
/// The reserved code "Plus tard" is not counted anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CodeCounts {
    pub passages: usize,
    pub interviews: usize,
    pub passages_rollup: usize,
    pub interviews_rollup: usize,
}

/// A code as the V1 interface sees it: stable id, free display name, aliases,
/// parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeNode {
    pub code_id: String,
    /// Legacy handle (upper-case token) used by the unit-level coding and the
    /// AI pass. Never changes, even when the code is renamed.
    pub token: String,
    pub name: String,
    pub label: String,
    pub definition: String,
    pub includes: String,
    pub excludes: String,
    pub parent_id: Option<String>,
    pub origin: String,
    pub toe: Option<String>,
    pub research_questions: Vec<String>,
    pub active: bool,
    pub system: bool,
    pub in_prompt: bool,
    /// Stable id of the code this one was merged into.
    pub merged_into: Option<String>,
    pub version: u32,
    pub example: Option<String>,
    pub reason: Option<String>,
    pub origin_passage_id: Option<String>,
    pub example_passage_id: Option<String>,
    pub created_at: Option<String>,
    pub aliases: Vec<CodeAlias>,
    pub counts: CodeCounts,
}

/// The code tree: nodes in codebook order, each with its children ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeTree {
    pub codebook_version: i64,
    pub codes: Vec<CodeNode>,
}

/// The reserved system code as a node (not stored in the codes table).
pub fn later_node() -> CodeNode {
    CodeNode {
        code_id: LATER_ID.into(),
        token: LATER_ID.into(),
        name: LATER_NAME.into(),
        label: LATER_NAME.into(),
        definition: "Marqueur pour revoir un passage plus tard. Ne compte dans aucune analyse."
            .into(),
        includes: String::new(),
        excludes: String::new(),
        parent_id: None,
        origin: "system".into(),
        toe: None,
        research_questions: Vec::new(),
        active: true,
        system: true,
        in_prompt: false,
        merged_into: None,
        version: 1,
        example: None,
        reason: None,
        origin_passage_id: None,
        example_passage_id: None,
        created_at: None,
        aliases: Vec::new(),
        counts: CodeCounts::default(),
    }
}
