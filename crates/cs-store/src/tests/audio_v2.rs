//! F-62: the manual audio offset of an interview.

use super::*;

#[test]
fn no_offset_is_zero_and_not_set() {
    let (s, _d) = ready();
    let o = s.audio_offset(ID).unwrap();
    assert_eq!(o.offset_seconds, 0.0);
    assert!(!o.set);
}

#[test]
fn setting_an_offset_is_stored_and_audited() {
    let (s, _d) = ready();
    let o = s.set_audio_offset(ID, -3.5, " starts late ", ACTOR).unwrap();
    assert_eq!(o.offset_seconds, -3.5);
    assert_eq!(o.note, "starts late");
    assert!(o.set);
    assert_eq!(s.audio_offset(ID).unwrap(), o);
    let ev = s.audio_events();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].detail["before"]["offset_seconds"], 0.0);
    assert_eq!(ev[0].detail["after"]["offset_seconds"], -3.5);
    assert!(s.verify_chain().unwrap().ok);
}

#[test]
fn the_same_value_writes_nothing() {
    let (s, _d) = ready();
    s.set_audio_offset(ID, 2.0, "", ACTOR).unwrap();
    s.set_audio_offset(ID, 2.0, "", ACTOR).unwrap();
    assert_eq!(s.audio_events().len(), 1);
    s.set_audio_offset(ID, 2.0, "now with a note", ACTOR).unwrap();
    assert_eq!(s.audio_events().len(), 2);
}

#[test]
fn values_that_are_not_sane_are_refused() {
    let (s, _d) = ready();
    for bad in [f64::NAN, f64::INFINITY, 100_000.0, -86_401.0] {
        assert!(s.set_audio_offset(ID, bad, "", ACTOR).is_err(), "{bad}");
    }
    assert!(s.set_audio_offset("NOPE-99", 1.0, "", ACTOR).is_err());
    assert!(!s.audio_offset(ID).unwrap().set);
}

#[test]
fn milliseconds_are_the_finest_grain() {
    let (s, _d) = ready();
    let o = s.set_audio_offset(ID, 1.23456, "", ACTOR).unwrap();
    assert_eq!(o.offset_seconds, 1.235);
}

impl Store {
    fn audio_events(&self) -> Vec<cs_model::AuditEvent> {
        self.audit_events(Some(ID))
            .unwrap()
            .into_iter()
            .filter(|e| e.action == "audio.offset")
            .collect()
    }
}
