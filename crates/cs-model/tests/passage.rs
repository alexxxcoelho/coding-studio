//! Offsets and anchors of passages (V1 lot 1, F-27 and F-28).
//!
//! Convention: the server stores Unicode scalar positions (Rust `char`
//! indices), end exclusive. The browser counts UTF-16 code units; the
//! conversion is done here and refuses a position inside a surrogate pair.

use cs_model::passage::{
    scalar_len, scalar_to_utf16, slice_scalars, text_digest, utf16_to_scalar, LATER_ID, LATER_NAME,
};

const TEXT: &str = "Éric dit: « c'est l'été » 😀 fin\nligne deux";

#[test]
fn scalar_length_counts_characters_not_bytes_or_utf16_units() {
    assert_eq!(scalar_len("abc"), 3);
    assert_eq!(scalar_len("été"), 3);
    assert_eq!(scalar_len("😀"), 1);
    assert_eq!("😀".encode_utf16().count(), 2);
}

#[test]
fn slice_by_scalars_returns_the_exact_text() {
    assert_eq!(slice_scalars("Éric dit", 0, 4), Some("Éric"));
    assert_eq!(slice_scalars(TEXT, 10, 11), Some("«"));
    let emoji = TEXT.chars().position(|c| c == '😀').unwrap();
    assert_eq!(slice_scalars(TEXT, emoji, emoji + 1), Some("😀"));
    assert_eq!(slice_scalars(TEXT, 5, 5), Some(""));
}

#[test]
fn slice_refuses_a_range_outside_the_text() {
    assert_eq!(slice_scalars("abc", 2, 4), None);
    assert_eq!(slice_scalars("abc", 3, 2), None);
    assert_eq!(slice_scalars("abc", 4, 4), None);
    assert_eq!(slice_scalars("abc", 3, 3), Some(""));
}

#[test]
fn utf16_positions_convert_without_shift() {
    // "😀" is two UTF-16 units: the position after it differs between the two counts.
    let text = "a😀b";
    assert_eq!(utf16_to_scalar(text, 0), Some(0));
    assert_eq!(utf16_to_scalar(text, 1), Some(1));
    assert_eq!(utf16_to_scalar(text, 3), Some(2));
    assert_eq!(utf16_to_scalar(text, 4), Some(3));
    assert_eq!(scalar_to_utf16(text, 2), Some(3));
    assert_eq!(scalar_to_utf16(text, 3), Some(4));
}

#[test]
fn utf16_position_inside_a_surrogate_pair_is_refused() {
    assert_eq!(utf16_to_scalar("a😀b", 2), None);
}

#[test]
fn utf16_position_past_the_end_is_refused() {
    assert_eq!(utf16_to_scalar("abc", 4), None);
    assert_eq!(scalar_to_utf16("abc", 4), None);
    assert_eq!(utf16_to_scalar("abc", 3), Some(3));
}

#[test]
fn round_trip_over_every_boundary_of_a_mixed_text() {
    let n = scalar_len(TEXT);
    for s in 0..=n {
        let u = scalar_to_utf16(TEXT, s).unwrap();
        assert_eq!(utf16_to_scalar(TEXT, u), Some(s), "scalar {s}");
    }
}

#[test]
fn line_breaks_are_single_scalars() {
    let text = "ab\r\ncd\nef";
    assert_eq!(scalar_len(text), 9);
    assert_eq!(slice_scalars(text, 2, 4), Some("\r\n"));
}

#[test]
fn digest_is_the_sha256_of_the_unit_text() {
    assert_eq!(
        text_digest("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_ne!(text_digest("abc"), text_digest("abc "));
}

#[test]
fn the_later_code_has_a_reserved_id_and_name() {
    assert_eq!(LATER_ID, "LATER");
    assert_eq!(LATER_NAME, "Plus tard");
}
