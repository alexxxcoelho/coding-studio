//! Smoke test on a real interview folder, when `CS_REAL_LIBRARY` names one.
//! Read-only: the scan and the parsers never write. Skipped otherwise.
//! Run with `cargo test -p cs-ingest --test real_library -- --nocapture` to see the table.

use std::path::Path;

use cs_ingest::{build_segmentation, default_interviewer_names, scan_library};
use cs_model::SourceKind;

#[test]
fn real_library_segments_every_transcript() {
    let Some(library) = std::env::var_os("CS_REAL_LIBRARY") else {
        eprintln!("CS_REAL_LIBRARY not set, test skipped");
        return;
    };
    let root = Path::new(&library);
    if !root.is_dir() {
        eprintln!("real library not found, test skipped");
        return;
    }
    let lib = scan_library(root).expect("scan");
    assert!(!lib.is_empty());
    let names = default_interviewer_names();
    println!(
        "{:<8} {:<36} {:>7} {:>9} {:>10}",
        "id", "primary", "n_units", "seg_warn", "scan_warn"
    );
    let mut failures = Vec::new();
    for entry in &lib {
        let Some(primary) = &entry.primary_source else {
            println!(
                "{:<8} {:<36} {:>7} {:>9} {:>10}",
                entry.interview_id,
                "(none)",
                "-",
                "-",
                entry.warnings.len()
            );
            continue;
        };
        let kind = entry
            .sources
            .iter()
            .find(|s| &s.rel_path == primary)
            .map(|s| s.kind)
            .unwrap();
        match build_segmentation(entry, primary, &names) {
            Ok(seg) => {
                println!(
                    "{:<8} {:<36} {:>7} {:>9} {:>10}",
                    entry.interview_id,
                    primary,
                    seg.units.len(),
                    seg.warnings.len(),
                    entry.warnings.len()
                );
                // A written answer has a handful of paragraphs, not 20 turns.
                let min = if kind == SourceKind::WrittenResponse {
                    3
                } else {
                    20
                };
                if seg.units.len() < min {
                    failures.push(format!(
                        "{}: only {} units",
                        entry.interview_id,
                        seg.units.len()
                    ));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", entry.interview_id)),
        }
        // Every transcript of the folder parses without panicking.
        for s in entry.sources.iter().filter(|s| s.kind.is_transcript()) {
            let _ = build_segmentation(entry, &s.rel_path, &names);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
