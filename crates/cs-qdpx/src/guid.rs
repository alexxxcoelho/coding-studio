//! Deterministic GUIDs (UUID version 5).
//!
//! REFI-QDA identifies codes, sources, selections and users by GUID. Coding
//! Studio derives every GUID from its own stable ids with UUID v5 in one fixed
//! namespace, so that exporting the same data twice writes the same GUIDs, and
//! a file exported earlier can be matched with the codes it came from.
//!
//! Namespace: [`NAMESPACE`], `6f2b7c1e-5a9d-4d34-8f0e-2c6a1b7d9e41`, chosen once
//! for Coding Studio and never changed. Names hashed:
//!
//! | object | name |
//! |---|---|
//! | code | `code:<code_id>` (e.g. `code:C-0001`) |
//! | text source | `source:<interview_id>` |
//! | selection | `passage:<passage_id>` |
//! | coding | `coding:<passage_id>:<code_id>` |
//! | user | `user:<name>` |
//! | highlights code | `highlights` |

use uuid::Uuid;

/// Namespace of every GUID written by Coding Studio.
pub const NAMESPACE: Uuid = Uuid::from_u128(0x6f2b_7c1e_5a9d_4d34_8f0e_2c6a_1b7d_9e41);

fn v5(name: &str) -> String {
    Uuid::new_v5(&NAMESPACE, name.as_bytes())
        .hyphenated()
        .to_string()
}

pub fn code(code_id: &str) -> String {
    v5(&format!("code:{code_id}"))
}

pub fn source(interview_id: &str) -> String {
    v5(&format!("source:{interview_id}"))
}

pub fn selection(passage_id: &str) -> String {
    v5(&format!("passage:{passage_id}"))
}

pub fn coding(passage_id: &str, code_id: &str) -> String {
    v5(&format!("coding:{passage_id}:{code_id}"))
}

pub fn user(name: &str) -> String {
    v5(&format!("user:{name}"))
}

pub fn highlights_code() -> String {
    v5("highlights")
}

/// True when `s` has the shape of a UUID (any version, either case).
pub fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_are_stable_and_distinct() {
        assert_eq!(code("C-0001"), code("C-0001"));
        assert_ne!(code("C-0001"), code("C-0002"));
        assert_ne!(code("C-0001"), source("C-0001"));
        assert!(is_guid(&code("C-0001")));
    }

    #[test]
    fn guid_values_are_frozen() {
        // Changing the namespace or the names would break every file exported
        // so far: this value must never change.
        assert_eq!(code("C-0001"), FROZEN_CODE_C0001);
    }

    const FROZEN_CODE_C0001: &str = "557fdda4-700c-5fd0-b4b6-9396aa70f392";

    #[test]
    fn guid_shape() {
        assert!(is_guid("2C2CB3A3-3E5A-5B59-A38F-4A4A3C1BD1B5"));
        assert!(!is_guid("not-a-guid"));
        assert!(!is_guid("2c2cb3a3-3e5a-5b59-a38f-4a4a3c1bd1b"));
    }
}
