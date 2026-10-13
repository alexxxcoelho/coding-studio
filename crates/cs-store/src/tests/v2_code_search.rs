//! V2, F-41: code search by name, alias, definition and legacy label; prefix
//! and accent-insensitive, ranked.

use super::v2_common::*;
use super::*;
use crate::codesearch::CodeSearchOptions;

fn fixture() -> (Store, TempDir, String, String, String) {
    let (s, d) = ready();
    let eco = code_def(
        &s,
        "Économie circulaire",
        "Réutilisation des étiquettes en fin de vie.",
    );
    s.add_alias(&eco, "Circular economy", ACTOR).unwrap();
    let cost = code_def(
        &s,
        "Coût des puces",
        "Investissement initial et prix unitaire par étiquette.",
    );
    let skills = code_def(
        &s,
        "Compétences",
        "Savoir-faire internes, formation du personnel.",
    );
    (s, d, eco, cost, skills)
}

fn ids(r: &crate::codesearch::CodeSearch) -> Vec<String> {
    r.items.iter().map(|i| i.code_id.clone()).collect()
}

fn search(s: &Store, q: &str) -> crate::codesearch::CodeSearch {
    s.search_codes(q, &CodeSearchOptions::default()).unwrap()
}

#[test]
fn a_name_prefix_matches_without_accents_or_case() {
    let (s, _d, eco, ..) = fixture();
    for q in ["econ", "ÉCON", "Économie", "economie circ"] {
        let r = search(&s, q);
        assert_eq!(ids(&r)[0], eco, "{q}");
        assert_eq!(r.items[0].matched.field, "name");
    }
    assert!(
        search(&s, "conomie").items.is_empty(),
        "a prefix, not a substring"
    );
}

#[test]
fn an_alias_is_found_and_says_so() {
    let (s, _d, eco, ..) = fixture();
    let r = search(&s, "circular");
    assert_eq!(ids(&r), [eco]);
    assert_eq!(r.items[0].matched.field, "alias");
    assert_eq!(r.items[0].matched.text, "Circular economy");
}

#[test]
fn the_legacy_token_and_the_renamed_former_name_are_found() {
    let (s, _d, eco, ..) = fixture();
    s.update_code(
        &eco,
        &crate::CodeUpdate {
            name: Some("Circularité".into()),
            ..Default::default()
        },
        ACTOR,
    )
    .unwrap();
    // The former name is an alias now.
    let r = search(&s, "econom");
    assert_eq!(ids(&r)[0], eco);
    assert_eq!(r.items[0].matched.field, "alias");
    // The tokens of the imported codebook (TECH-ADV and others).
    let r = search(&s, "tech-a");
    assert!(!r.items.is_empty());
}

#[test]
fn a_definition_word_matches_after_names_and_aliases() {
    let (s, _d, eco, cost, _skills) = fixture();
    // "etiquette" is in two definitions, in no name.
    let r = search(&s, "etiquet");
    assert_eq!(ids(&r).len(), 2);
    assert!(r.items.iter().all(|i| i.matched.field == "definition"));
    assert!(ids(&r).contains(&eco) && ids(&r).contains(&cost));
    // A name hit outranks a definition hit.
    let skills = code_def(&s, "Étiquettes grises", "Autre chose.");
    let r = search(&s, "etiquet");
    assert_eq!(ids(&r)[0], skills);
}

#[test]
fn every_word_of_the_query_must_match() {
    let (s, _d, eco, ..) = fixture();
    assert_eq!(ids(&search(&s, "circ econ")), std::slice::from_ref(&eco));
    assert_eq!(ids(&search(&s, "econ reutil")), [eco]);
    assert!(search(&s, "econ zzz").items.is_empty());
}

#[test]
fn inactive_and_merged_codes_are_left_out_unless_asked() {
    let (s, _d, eco, cost, _) = fixture();
    s.set_code_active(&cost, false, ACTOR).unwrap();
    assert!(search(&s, "cout").items.is_empty());
    let r = s
        .search_codes(
            "cout",
            &CodeSearchOptions {
                include_inactive: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ids(&r), std::slice::from_ref(&cost));
    assert!(!r.items[0].active);
    // The merged name finds the target.
    s.set_code_active(&cost, true, ACTOR).unwrap();
    s.merge_code(&cost, &eco, "same idea", ACTOR).unwrap();
    assert_eq!(ids(&search(&s, "cout"))[0], eco);
}

#[test]
fn an_empty_query_lists_the_active_codes_up_to_the_limit() {
    let (s, _d, ..) = fixture();
    let all = search(&s, "");
    assert!(all.items.len() >= 7);
    assert!(all.items.iter().all(|i| i.active && !i.system));
    let few = s
        .search_codes(
            "",
            &CodeSearchOptions {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(few.items.len(), 2);
    assert_eq!(few.total, all.total);
}

#[test]
fn items_carry_the_path_and_counts_for_the_picker() {
    let (s, _d, eco, ..) = fixture();
    let child = code(&s, "Réemploi", Some(&eco));
    pcode(&s, ID, U2, None, &[&child]);
    let r = search(&s, "reemp");
    let it = &r.items[0];
    assert_eq!(it.code_id, child);
    assert_eq!(it.path, ["Économie circulaire"]);
    assert_eq!(it.passages, 1);
}
